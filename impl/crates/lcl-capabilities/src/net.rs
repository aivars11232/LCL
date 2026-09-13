//! The network capability: a primitive transport, and one std-only client.
//!
//! ## What this transport can and cannot do
//!
//! The workspace's dependency policy is std only — "The spec authority loader
//! is the trust root for every later layer, so it carries no third-party
//! supply-chain surface" — and TLS cannot be implemented on std alone. So
//! [`TcpTransport`] speaks HTTP/1.1 over a plain TCP socket and speaks nothing
//! else.
//!
//! An `https` target is therefore reported as a **limitation**, not a refusal
//! and never a silent downgrade: [`Grants::permit_tls`] is left unset, the
//! grant decision returns `Refusal::Unavailable`, and the standard library maps
//! that to `error.host.constraint`, which "never changes LCL meaning". A
//! product layer that links a TLS stack installs its own transport and declares
//! the grant; nothing in the language changes when it does.
//!
//! ## Bounds are not optional here
//!
//! A socket that never answers would hang the engine, and a response with no
//! declared length could be unbounded. Both socket timeouts and the response
//! cap come from [`Bounds`], and exceeding either is [`Cancelled`].

use crate::bounds::{Bounds, Cancelled};
use crate::grant::{Grant, Grants, Refusal};
use std::fmt;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

/// What one transport request could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The host refuses or cannot supply the capability.
    Refused(Refusal),
    /// A declared bound stopped the work.
    Bounded(Cancelled),
    /// The endpoint could not be reached or did not answer usefully.
    Unreachable(String),
    /// The address is not one this transport understands.
    Malformed(String),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::Refused(refusal) => write!(f, "{refusal}"),
            NetError::Bounded(cancelled) => write!(f, "{cancelled}"),
            NetError::Unreachable(detail) => f.write_str(detail),
            NetError::Malformed(detail) => f.write_str(detail),
        }
    }
}

/// One parsed absolute URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    /// The path and query, beginning with `/`.
    pub target: String,
}

impl Address {
    /// Parse one absolute URI into the parts a transport needs.
    pub fn parse(uri: &str) -> Result<Address, NetError> {
        let (scheme, rest) = uri
            .split_once("://")
            .ok_or_else(|| NetError::Malformed(format!("{uri} declares no scheme")))?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, "/"),
        };
        let authority = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host,
                port.parse::<u16>()
                    .map_err(|_| NetError::Malformed(format!("{uri} declares no valid port")))?,
            ),
            None => (
                authority,
                match scheme {
                    "http" => 80,
                    "https" => 443,
                    other => {
                        return Err(NetError::Malformed(format!("{other} has no default port")))
                    }
                },
            ),
        };
        if host.is_empty() {
            return Err(NetError::Malformed(format!("{uri} declares no host")));
        }
        Ok(Address {
            scheme: scheme.to_string(),
            host: host.to_string(),
            port,
            target: path.to_string(),
        })
    }

    /// Whether this scheme requires a secure transport.
    pub fn is_secure(&self) -> bool {
        matches!(self.scheme.as_str(), "https" | "wss" | "ftps")
    }
}

/// One response, as a transport observed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The primitive network capability.
pub trait Transport {
    /// Retrieve the content one address names.
    fn get(&mut self, address: &Address, bounds: &Bounds) -> Result<Response, NetError>;
    /// Send content to one address.
    fn put(
        &mut self,
        address: &Address,
        body: &[u8],
        bounds: &Bounds,
    ) -> Result<Response, NetError>;
}

/// An HTTP/1.1 client over a plain TCP socket.
#[derive(Debug, Clone)]
pub struct TcpTransport {
    grants: Grants,
}

impl TcpTransport {
    pub fn new(grants: Grants) -> TcpTransport {
        TcpTransport { grants }
    }

    fn admit(&self, address: &Address) -> Result<(), NetError> {
        self.grants
            .decide(&Grant::Network {
                host: address.host.clone(),
                secure: address.is_secure(),
            })
            .map_err(NetError::Refused)
    }

    fn exchange(
        &mut self,
        address: &Address,
        request: Vec<u8>,
        bounds: &Bounds,
    ) -> Result<Response, NetError> {
        self.admit(address)?;
        let budget = Budget::new(
            bounds
                .deadline
                .map(|deadline| deadline.as_duration())
                .unwrap_or(DEFAULT_EXCHANGE_BUDGET),
        );

        let mut stream = connect_within(address, &budget)?;
        // Each socket wait is given whatever is left of the whole exchange, so
        // a peer cannot stay inside a per-read limit forever and still be
        // inside the bound it was given.
        budget.apply_write(&stream)?;
        stream
            .write_all(&request)
            .map_err(|error| sent_error(error, &budget))?;
        stream.flush().map_err(|error| sent_error(error, &budget))?;

        let mut raw = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            budget.apply_read(&stream)?;
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    raw.extend_from_slice(&buffer[..read]);
                    if raw.len() as u64 > bounds.max_bytes {
                        return Err(NetError::Bounded(Cancelled::exceeded(
                            "the response",
                            raw.len() as u64,
                            bounds.max_bytes,
                        )));
                    }
                }
                Err(error) if is_timeout(&error) => return Err(budget.elapsed_error()),
                Err(error) => return Err(NetError::Unreachable(error.to_string())),
            }
        }
        parse_response(&raw)
    }
}

/// The bound with no declared deadline. Finite, because an unbounded exchange
/// is reachable from a document that merely names a slow address.
const DEFAULT_EXCHANGE_BUDGET: Duration = Duration::from_secs(30);

/// How long an entire exchange may take, and how much of it is left.
///
/// The declared bound covers the whole exchange — resolving the address,
/// establishing the connection, sending the request and receiving the response
/// — and not each socket operation separately. The difference is not
/// theoretical: a peer that answers every read promptly and takes ten times the
/// bound to finish is inside a per-read limit and outside any honest reading of
/// a declared one, and `TcpStream::connect` takes no timeout at all, so a
/// bounded read limit said nothing whatever about resolution or connection.
///
/// Canonical does not decide this. `core.execute`, `core.start` and `core.stop`
/// register a `timeout` parameter; `core.download` and `core.upload` register
/// none, so a network deadline is host policy. This is that policy, stated
/// once, here: one budget, spent by every phase, and exceeding it is
/// [`Cancelled`] — a host limitation, which "never changes LCL meaning".
struct Budget {
    total: Duration,
    started: Instant,
}

impl Budget {
    fn new(total: Duration) -> Budget {
        Budget {
            total,
            started: Instant::now(),
        }
    }

    /// What is left, or `None` when the budget is spent.
    fn remaining(&self) -> Option<Duration> {
        self.total
            .checked_sub(self.started.elapsed())
            .filter(|left| !left.is_zero())
    }

    fn elapsed_error(&self) -> NetError {
        NetError::Bounded(Cancelled::new(format!(
            "the exchange did not finish within the declared bound of {:?}",
            self.total
        )))
    }

    fn apply_read(&self, stream: &TcpStream) -> Result<(), NetError> {
        let left = self.remaining().ok_or_else(|| self.elapsed_error())?;
        stream
            .set_read_timeout(Some(left))
            .map_err(|error| NetError::Unreachable(error.to_string()))
    }

    fn apply_write(&self, stream: &TcpStream) -> Result<(), NetError> {
        let left = self.remaining().ok_or_else(|| self.elapsed_error())?;
        stream
            .set_write_timeout(Some(left))
            .map_err(|error| NetError::Unreachable(error.to_string()))
    }
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// A write failure, told apart from a write that simply ran out of budget.
fn sent_error(error: std::io::Error, budget: &Budget) -> NetError {
    match is_timeout(&error) {
        true => budget.elapsed_error(),
        false => NetError::Unreachable(error.to_string()),
    }
}

/// Resolve and connect, within what is left of the budget.
///
/// Name resolution has no timeout in the standard library, so it runs on its
/// own thread and this one stops *waiting* when the budget is gone. The worker
/// is not abandoned in any harmful sense: it owns nothing but its own attempt,
/// its send fails silently into a dropped receiver, and it closes the socket it
/// may have opened when it finishes.
fn connect_within(address: &Address, budget: &Budget) -> Result<TcpStream, NetError> {
    let left = budget.remaining().ok_or_else(|| budget.elapsed_error())?;
    let host = address.host.clone();
    let port = address.port;
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let resolved = match std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port)) {
            Ok(addresses) => addresses,
            Err(error) => {
                let _ = tx.send(Err(error.to_string()));
                return;
            }
        };
        let mut last = "the address resolved to nothing".to_string();
        for candidate in resolved {
            match TcpStream::connect(candidate) {
                Ok(stream) => {
                    let _ = tx.send(Ok(stream));
                    return;
                }
                Err(error) => last = error.to_string(),
            }
        }
        let _ = tx.send(Err(last));
    });

    match rx.recv_timeout(left) {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(detail)) => Err(NetError::Unreachable(detail)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(NetError::Bounded(Cancelled::new(
            format!(
                "resolving and connecting to {}:{} did not finish within the declared bound of {:?}",
                address.host, address.port, budget.total
            ),
        ))),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(NetError::Unreachable(
            "the connection attempt ended without an answer".to_string(),
        )),
    }
}

/// Split one HTTP/1.1 response into its status and body.
///
/// The length of a message body is not "whatever followed the headers". RFC
/// 9112 §6.3 determines it, and getting that wrong is not a cosmetic error
/// here: `core.download`'s postcondition is that "destination bytes equal
/// received source", so framing bytes kept as content, or a truncated prefix
/// reported as the whole, satisfy that postcondition with the wrong file.
fn parse_response(raw: &[u8]) -> Result<Response, NetError> {
    let separator = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| NetError::Unreachable("the response has no header terminator".into()))?;
    let head = String::from_utf8_lossy(&raw[..separator]);
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| NetError::Unreachable("the response has no status code".into()))?;
    let body = &raw[separator + 4..];

    // Field lines are collected with their duplicates intact. Collapsing them
    // into a map first is what makes two disagreeing lengths look like one.
    let mut lengths: Vec<String> = Vec::new();
    let mut codings: Vec<String> = Vec::new();
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => lengths.push(value.trim().to_string()),
            "transfer-encoding" => codings.push(value.trim().to_ascii_lowercase()),
            _ => {}
        }
    }

    // §6.3 step 3: a message with both is an error for a recipient that is not
    // a proxy. Believing one of the two is precisely the disagreement request
    // smuggling is built on, so neither is believed.
    if !codings.is_empty() && !lengths.is_empty() {
        return Err(NetError::Unreachable(
            "the response declares both Transfer-Encoding and Content-Length, \
             which RFC 9112 6.3 makes an error rather than a choice"
                .into(),
        ));
    }

    if !codings.is_empty() {
        let declared: Vec<String> = codings
            .iter()
            .flat_map(|value| value.split(','))
            .map(|coding| coding.trim().to_string())
            .filter(|coding| !coding.is_empty())
            .collect();
        // std alone cannot decompress, so any coding other than the one this
        // transport implements is reported as a limitation rather than passed
        // through as though it were content.
        if declared != ["chunked"] {
            return Err(NetError::Unreachable(format!(
                "this transport implements only the chunked transfer coding; \
                 the response declared {}",
                declared.join(", ")
            )));
        }
        return Ok(Response {
            status,
            body: decode_chunked(body)?,
        });
    }

    if !lengths.is_empty() {
        // A list of identical values is one length; differing values are not.
        let values: Vec<&str> = lengths
            .iter()
            .flat_map(|value| value.split(','))
            .map(str::trim)
            .collect();
        let first = values[0];
        if values.iter().any(|value| *value != first) {
            return Err(NetError::Unreachable(format!(
                "the response declares disagreeing Content-Length values: {}",
                values.join(", ")
            )));
        }
        let declared: usize = first.parse().map_err(|_| {
            NetError::Unreachable(format!(
                "the response declares a Content-Length of {first:?}"
            ))
        })?;
        // §8: a message that ends before its declared length is incomplete. It
        // is not a shorter message.
        if body.len() < declared {
            return Err(NetError::Unreachable(format!(
                "the response declared {declared} bytes and delivered {}",
                body.len()
            )));
        }
        return Ok(Response {
            status,
            body: body[..declared].to_vec(),
        });
    }

    // §6.3 step 8: for a response, the connection close terminates the message.
    Ok(Response {
        status,
        body: body.to_vec(),
    })
}

/// Decode the chunked transfer coding, RFC 9112 §7.1.
///
/// The sizes, their CRLFs, any chunk extensions and the terminating zero chunk
/// are framing. None of them is content, and a decoder that stops early must
/// say so rather than return what it managed to collect.
fn decode_chunked(mut body: &[u8]) -> Result<Vec<u8>, NetError> {
    fn incomplete() -> NetError {
        NetError::Unreachable("the chunked response ended before its terminating chunk".to_string())
    }
    fn line_end(bytes: &[u8]) -> Option<usize> {
        bytes.windows(2).position(|window| window == b"\r\n")
    }

    let mut decoded = Vec::new();
    loop {
        let end = line_end(body).ok_or_else(incomplete)?;
        let header = String::from_utf8_lossy(&body[..end]);
        // "chunk-size [ chunk-ext ]": the extension is not part of the size.
        let size_text = header.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| {
            NetError::Unreachable(format!(
                "the response declares a chunk size of {size_text:?}"
            ))
        })?;
        body = &body[end + 2..];

        if size == 0 {
            // The trailer section runs to an empty line, which must be present
            // for the message to be complete.
            loop {
                let end = line_end(body).ok_or_else(incomplete)?;
                let trailer = &body[..end];
                body = &body[end + 2..];
                if trailer.is_empty() {
                    return Ok(decoded);
                }
            }
        }

        if body.len() < size + 2 {
            return Err(incomplete());
        }
        decoded.extend_from_slice(&body[..size]);
        if &body[size..size + 2] != b"\r\n" {
            return Err(NetError::Unreachable(
                "a chunk is not terminated by CRLF".to_string(),
            ));
        }
        body = &body[size + 2..];
    }
}

impl Transport for TcpTransport {
    fn get(&mut self, address: &Address, bounds: &Bounds) -> Result<Response, NetError> {
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: */*\r\n\r\n",
            address.target, address.host
        );
        self.exchange(address, request.into_bytes(), bounds)
    }

    fn put(
        &mut self,
        address: &Address,
        body: &[u8],
        bounds: &Bounds,
    ) -> Result<Response, NetError> {
        let mut request = format!(
            "PUT {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            address.target,
            address.host,
            body.len()
        )
        .into_bytes();
        request.extend_from_slice(body);
        self.exchange(address, request, bounds)
    }
}
