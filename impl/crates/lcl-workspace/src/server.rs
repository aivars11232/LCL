//! The loopback server, and the three gates every request passes.
//!
//! ## What this process can do, and why that matters
//!
//! It reads and writes files in a project, and it can run an LCL document with
//! host capabilities granted. Anything that can talk to this socket can do the
//! same. So the socket is not a convenience surface, and it is treated as the
//! boundary it actually is:
//!
//! 1. **It binds `127.0.0.1` only**, on an ephemeral port. Nothing off this
//!    machine can reach it, because nothing off this machine can route to it.
//! 2. **It requires a session token** minted at launch, on every single
//!    request. A process without the token gets nothing, even from loopback.
//! 3. **It checks `Host` and `Origin`** against the address it actually bound.
//!    This is the defence against DNS rebinding: a page on `evil.example`
//!    whose name resolves to `127.0.0.1` still sends `Host: evil.example`, and
//!    that is not the address this server bound, so the request is refused
//!    before a handler ever sees it.
//!
//! The responses add `Content-Security-Policy: default-src 'self'`, no CORS
//! headers at all, and `X-Content-Type-Options: nosniff`.
//!
//! ## Threads
//!
//! One thread per connection, with a hard ceiling on how many may exist at
//! once. The engine itself is never shared across them: contract 5.3 requires
//! observable meaning not to depend on thread scheduling, and the way this
//! server honours that is structural, by giving each request its own engine
//! call and never letting two runs share mutable state.

use crate::http::{EventStream, Request, RequestError, Response};
use std::io::BufReader;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// The most connections served at once. A workspace is one browser tab.
const MAX_CONNECTIONS: usize = 64;

/// How long an unfinished request may take to arrive, in total.
///
/// The ceiling above is right, and on its own it is also the attack: a peer
/// that opens a connection and says nothing holds a thread and one of those
/// slots for as long as it likes, needs no token to do it, and sixty-four of
/// them take the workspace away from the person it belongs to. A bound makes
/// the ceiling recoverable instead of permanent.
///
/// This is a bound on the whole request — line, headers and body — and not on
/// each idle wait within it. As a per-read timeout it stopped a peer that said
/// nothing and did nothing about a peer that said one byte just inside it and
/// then another, which resets a per-read timeout for as long as the peer cares
/// to keep going. Token validation happens after reading, so all of that is
/// pre-authentication.
///
/// It applies only while the request is still being read. Once a complete
/// request has passed all three gates the bound is lifted, because a debugging
/// session's event stream is *meant* to stay connected — and disconnecting one
/// every few seconds would be this repair breaking the feature it protects.
///
/// Ten seconds is far longer than any legitimate client on loopback needs,
/// including an editor saving the largest document this server accepts.
pub const INGRESS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// What a route did with a request.
pub enum Outcome {
    /// One complete response.
    Reply(Response),
    /// The route takes the connection over as an event stream.
    Stream(Box<dyn FnOnce(EventStream) + Send>),
}

/// Anything that can answer a request.
pub trait Route: Send + Sync + 'static {
    fn handle(&self, request: &Request) -> Outcome;
}

/// A running workspace server.
pub struct Server {
    listener: TcpListener,
    address: SocketAddr,
    token: String,
    log: bool,
    ingress: std::time::Duration,
}

impl Server {
    /// Bind loopback on an ephemeral port and mint a session token.
    pub fn bind() -> std::io::Result<Server> {
        Server::bind_to(0)
    }

    /// Bind loopback on an exact port. Port 0 asks the OS for a free one.
    pub fn bind_to(port: u16) -> std::io::Result<Server> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        let address = listener.local_addr()?;
        Ok(Server {
            listener,
            address,
            token: mint_token(),
            log: false,
            ingress: INGRESS_TIMEOUT,
        })
    }

    /// Print one line per request to standard error.
    ///
    /// Off by default. An operator debugging a workspace wants to see what the
    /// page actually asked for, and so does anyone proving that a browser
    /// reached the engine rather than merely rendering a shell. The token is
    /// never printed.
    pub fn logging(mut self, log: bool) -> Server {
        self.log = log;
        self
    }

    /// Use a different total ingress budget than [`INGRESS_TIMEOUT`].
    ///
    /// For a test that has to prove the bound *ends* a slow request: waiting
    /// the production budget would make the suite sleep for ten seconds per
    /// case, and shortening the production constant to avoid that would be
    /// changing the policy to suit the test. This changes only what this one
    /// server was told to allow. The product does not call it, so the shipped
    /// budget is the one above.
    pub fn ingress_budget(mut self, budget: std::time::Duration) -> Server {
        self.ingress = budget;
        self
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// The URL to open, token included.
    pub fn url(&self) -> String {
        format!("http://{}/?t={}", self.address, self.token)
    }

    /// Serve until the listener fails.
    pub fn serve(self, route: Arc<dyn Route>) -> std::io::Result<()> {
        let live = Arc::new(AtomicUsize::new(0));
        let budget = self.ingress;
        let expected = Arc::new(Expected {
            token: self.token.clone(),
            address: self.address,
            log: self.log,
        });
        for stream in self.listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                // One failed accept is not a reason to stop serving.
                Err(_) => continue,
            };
            if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                // Refuse rather than queue without bound.
                let mut stream = stream;
                let _ = Response::error(503, "too many connections").write(&mut stream);
                continue;
            }
            live.fetch_add(1, Ordering::SeqCst);
            let route = Arc::clone(&route);
            let expected = Arc::clone(&expected);
            let live = Arc::clone(&live);
            std::thread::spawn(move || {
                serve_one(stream, route.as_ref(), expected.as_ref(), budget);
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
        Ok(())
    }
}

/// What every request is checked against.
struct Expected {
    token: String,
    address: SocketAddr,
    log: bool,
}

impl Expected {
    /// Whether this request may be handled at all.
    ///
    /// Returns the refusal to send, or `None` when the request passes.
    fn refuse(&self, request: &Request) -> Option<Response> {
        // Gate 1: the Host header names the address we actually bound.
        let host = request.header("host").unwrap_or_default();
        let port = self.address.port();
        let permitted = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
        if !permitted.iter().any(|h| h == host) {
            return Some(Response::error(
                403,
                "this request was addressed to a name this server did not bind",
            ));
        }

        // Gate 2: an Origin, when the browser sends one, is this same origin.
        if let Some(origin) = request.header("origin") {
            let permitted = [
                format!("http://127.0.0.1:{port}"),
                format!("http://localhost:{port}"),
            ];
            if !permitted.iter().any(|o| o == origin) {
                return Some(Response::error(403, "cross-origin requests are refused"));
            }
        }

        // Gate 3: the session token, from the header or the query string.
        let supplied = request
            .header("x-lcl-token")
            .or_else(|| request.param("t"))
            .unwrap_or_default();
        if !constant_time_eq(supplied.as_bytes(), self.token.as_bytes()) {
            return Some(Response::error(403, "a valid session token is required"));
        }

        None
    }
}

fn serve_one(
    stream: TcpStream,
    route: &dyn Route,
    expected: &Expected,
    budget: std::time::Duration,
) {
    let Ok(peer) = stream.peer_addr() else {
        return;
    };
    // Belt and braces: the listener is loopback-bound, so this cannot fail, but
    // a check costs nothing and states the invariant where it can be read.
    if !peer.ip().is_loopback() {
        return;
    }

    // Bounded until the request is complete and admitted. The budget starts
    // here, before a byte is read, and every read inside `Request::read` is
    // given what is left of it.
    let deadline = crate::http::Deadline::starting_now(budget);

    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut write_half = stream;

    let request = match Request::read(&mut reader, deadline) {
        Ok(request) => request,
        Err(RequestError::Closed) => return,
        Err(e) => {
            let _ = Response::error(400, &e.to_string()).write(&mut write_half);
            return;
        }
    };

    if expected.log {
        // The path and method only. A query string carries the session token,
        // and a log that printed it would be a log that leaks it.
        eprintln!("  {} {}", request.method, request.path);
    }

    if let Some(refusal) = expected.refuse(&request) {
        if expected.log {
            eprintln!("  refused {} {}", request.method, request.path);
        }
        let _ = refusal.write(&mut write_half);
        return;
    }

    // Admitted. The ingress bound existed to stop an unfinished request from
    // holding a slot; this one is finished, and an event stream it may turn
    // into is long-lived by design.
    let _ = write_half.set_read_timeout(None);

    match route.handle(&request) {
        Outcome::Reply(response) => {
            let _ = response.write(&mut write_half);
        }
        Outcome::Stream(take_over) => {
            if let Ok(events) = EventStream::open(write_half) {
                take_over(events)
            }
        }
    }
}

/// Compare two byte strings without returning early on the first difference.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (x, y) in a.iter().zip(b) {
        difference |= x ^ y;
    }
    difference == 0
}

/// Mint a 256-bit session token, hex encoded.
///
/// The operating system's entropy source first. `RandomState` is the fallback:
/// it is seeded by the OS per process and is what `std`'s own hash maps rely on
/// for collision resistance, so it is a real source rather than a pretend one.
fn mint_token() -> String {
    let mut bytes = [0u8; 32];
    if read_urandom(&mut bytes).is_err() {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};
        for chunk in bytes.chunks_mut(8) {
            let state = RandomState::new();
            let mut hasher = state.build_hasher();
            std::time::SystemTime::now().hash(&mut hasher);
            std::process::id().hash(&mut hasher);
            // A fresh address is different every iteration.
            (&chunk as *const _ as usize).hash(&mut hasher);
            let value = hasher.finish().to_le_bytes();
            chunk.copy_from_slice(&value[..chunk.len()]);
        }
    }
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn read_urandom(buffer: &mut [u8]) -> std::io::Result<()> {
    use std::io::Read;
    let mut file = std::fs::File::open("/dev/urandom")?;
    file.read_exact(buffer)
}
