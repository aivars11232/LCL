//! Shared test helpers: a real server on a real socket, and a client for it.
//!
//! Nothing here mocks the transport. Every test drives an actual `TcpStream`
//! against an actual bound listener, because the three gates under test are
//! properties of what crosses a socket and a mock would only prove that the
//! mock agrees with itself.

#![allow(dead_code)]

use lcl_workspace::http::{Request, Response};
use lcl_workspace::server::{Outcome, Route, Server};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

/// One raw HTTP reply.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Send one request and read the whole reply.
pub fn send(
    address: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Reply {
    let mut stream = TcpStream::connect(address).expect("the server is listening");
    let mut request = format!("{method} {target} HTTP/1.1\r\n");
    let mut saw_host = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("host") {
            saw_host = true;
        }
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    if !saw_host {
        request.push_str(&format!("Host: 127.0.0.1:{}\r\n", address.port()));
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    stream.write_all(request.as_bytes()).expect("request sent");
    stream.write_all(body).expect("body sent");
    stream.flush().expect("flushed");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("reply read");
    parse_reply(&raw)
}

/// One reply whose body was never decoded.
#[derive(Debug, Clone)]
pub struct BinaryReply {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// GET one target and keep the reply body as the bytes that crossed the socket.
///
/// [`send`] renders a body as text, which is right for JSON and wrong for a
/// PNG: `from_utf8_lossy` replaces every byte a decoder needs with U+FFFD, so a
/// test built on it would pass against an image no browser could read. The
/// whole question about an image route is whether the exact bytes arrive.
pub fn send_binary(address: SocketAddr, target: &str) -> BinaryReply {
    let mut stream = TcpStream::connect(address).expect("the server is listening");
    let request = format!(
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\n\r\n",
        address.port()
    );
    stream.write_all(request.as_bytes()).expect("request sent");
    stream.flush().expect("flushed");

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("reply read");
    let separator = b"\r\n\r\n";
    let cut = raw
        .windows(separator.len())
        .position(|window| window == separator)
        .expect("a reply has a head and a body");
    let head = String::from_utf8_lossy(&raw[..cut]).to_string();
    let body = raw[cut + separator.len()..].to_vec();

    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let content_type = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim().to_string())
        .unwrap_or_default();
    BinaryReply {
        status,
        content_type,
        body,
    }
}

/// Send a request with a completely hand-written head, framing included.
pub fn send_raw(address: SocketAddr, raw: &str) -> Reply {
    let mut stream = TcpStream::connect(address).expect("the server is listening");
    stream.write_all(raw.as_bytes()).expect("request sent");
    stream.flush().expect("flushed");
    let mut out = Vec::new();
    stream.read_to_end(&mut out).expect("reply read");
    parse_reply(&out)
}

fn parse_reply(raw: &[u8]) -> Reply {
    let text = String::from_utf8_lossy(raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_string(),
    }
}

/// A route that echoes what it received, so transport tests can see it.
pub struct Echo;

impl Route for Echo {
    fn handle(&self, request: &Request) -> Outcome {
        let body = format!(
            "{{\"method\":\"{}\",\"path\":\"{}\",\"query\":{},\"body\":\"{}\"}}",
            request.method,
            lcl_workspace::http::escape_json(&request.path),
            {
                let pairs: Vec<String> = request
                    .query
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "\"{}\":\"{}\"",
                            lcl_workspace::http::escape_json(k),
                            lcl_workspace::http::escape_json(v)
                        )
                    })
                    .collect();
                format!("{{{}}}", pairs.join(","))
            },
            lcl_workspace::http::escape_json(&String::from_utf8_lossy(&request.body)),
        );
        Outcome::Reply(Response::json(body))
    }
}

/// A bound, serving workspace and the facts a test needs about it.
pub struct Running {
    pub address: SocketAddr,
    pub token: String,
}

/// Start a server on an ephemeral port with the given route.
pub fn start(route: Arc<dyn Route>) -> Running {
    start_with(route, lcl_workspace::server::INGRESS_TIMEOUT)
}

/// The same, with a stated ingress budget.
///
/// A case that has to watch the bound *end* a request cannot wait the shipped
/// ten seconds per case, and shortening the shipped constant to suit the suite
/// would be changing the policy rather than testing it. Only this server is
/// told to allow less.
pub fn start_with(route: Arc<dyn Route>, budget: std::time::Duration) -> Running {
    let server = Server::bind()
        .expect("loopback binds")
        .ingress_budget(budget);
    let address = server.address();
    let token = server.token().to_string();
    std::thread::spawn(move || {
        let _ = server.serve(route);
    });
    // The listener exists before `bind` returned, so a connect cannot race it.
    Running { address, token }
}

// ---------------------------------------------------------------------------
// Projects on disk
// ---------------------------------------------------------------------------

/// A scratch directory that removes itself.
pub struct Scratch {
    pub path: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Scratch {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lcl-workspace-test-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch directory");
        Scratch { path }
    }

    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    /// Write one file, creating parents.
    pub fn put(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(&path, contents).expect("write");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The bytes of one canonical valid example.
pub fn example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the example is readable")
}

/// Every canonical valid example, in file-name order.
pub fn valid_examples() -> Vec<String> {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();
    names
}

/// A scratch project holding every canonical valid example, already open.
pub fn project_of_examples(name: &str) -> (Scratch, lcl_workspace::Workspace) {
    let scratch = Scratch::new(name);
    for example_name in valid_examples() {
        scratch.put(&example_name, &example(&example_name));
    }
    let workspace = lcl_workspace::Workspace::create(&scratch.path, canonical_root())
        .expect("a new project opens");
    (scratch, workspace)
}

/// A scratch project served by a real, bound workspace server.
///
/// The whole stack: a project on disk, an engine over the canonical package,
/// the route table and a listening socket. Tests drive it with `send`.
pub fn serve_examples(name: &str) -> (Scratch, Running) {
    let scratch = Scratch::new(name);
    for example_name in valid_examples() {
        scratch.put(&example_name, &example(&example_name));
    }
    let workspace = lcl_workspace::Workspace::create(&scratch.path, canonical_root())
        .expect("a new project opens");
    let running = start(Arc::new(lcl_workspace::Routes::new(Arc::new(workspace))));
    (scratch, running)
}

/// GET one route and parse its JSON body with the trust root's reader.
pub fn get_json(running: &Running, path: &str, params: &[(&str, &str)]) -> lcl_spec::json::Json {
    let mut target = format!("{path}?t={}", running.token);
    for (k, v) in params {
        target.push_str(&format!("&{k}={v}"));
    }
    let reply = send(running.address, "GET", &target, &[], b"");
    assert_eq!(reply.status, 200, "GET {path} failed: {}", reply.body);
    lcl_spec::json::parse(&reply.body).expect("a reply is JSON")
}
