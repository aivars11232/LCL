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
use std::time::Duration;

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
        let timeout = bounds
            .deadline
            .map(|deadline| deadline.as_duration())
            .unwrap_or(Duration::from_secs(30));

        let mut stream = TcpStream::connect((address.host.as_str(), address.port))
            .map_err(|error| NetError::Unreachable(error.to_string()))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|error| NetError::Unreachable(error.to_string()))?;
        stream
            .write_all(&request)
            .map_err(|error| NetError::Unreachable(error.to_string()))?;
        stream
            .flush()
            .map_err(|error| NetError::Unreachable(error.to_string()))?;

        let mut raw = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
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
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(NetError::Bounded(Cancelled::new(
                        "the endpoint stopped answering within the declared timeout",
                    )))
                }
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                    return Err(NetError::Bounded(Cancelled::new(
                        "the endpoint stopped answering within the declared timeout",
                    )))
                }
                Err(error) => return Err(NetError::Unreachable(error.to_string())),
            }
        }
        parse_response(&raw)
    }
}

/// Split one HTTP/1.1 response into its status and body.
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
    Ok(Response {
        status,
        body: raw[separator + 4..].to_vec(),
    })
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
