//! HTTP/1.1, as much of it as a loopback workspace needs and no more.
//!
//! ## Why this is written out
//!
//! `impl/Cargo.toml` states the workspace's dependency policy: "std only. The
//! spec authority loader is the trust root for every later layer, so it carries
//! no third-party supply-chain surface." That policy is why `lcl-cli` writes out
//! its own argument parser and `lcl-protocol` its own JSON writer, and the same
//! reasoning applies with more force here, because this is the one component
//! that opens a socket.
//!
//! ## What it deliberately does not do
//!
//! No chunked transfer encoding, no keep-alive pipelining, no compression, no
//! HTTPS, no proxy support and no request smuggling surface: a request whose
//! framing is ambiguous is refused rather than guessed at. Every response
//! carries an exact `Content-Length`, and the connection closes after it.
//!
//! Sizes are bounded before anything is allocated. A request line, a header
//! block and a body each have a hard ceiling, so a peer cannot make this
//! process grow by talking to it.

use std::collections::BTreeMap;
use std::io::{self, BufReader, Read, Write};
use std::net::TcpStream;

/// The longest request line accepted, including the target.
pub const MAX_REQUEST_LINE: usize = 8 * 1024;
/// The longest header block accepted.
pub const MAX_HEADERS: usize = 32 * 1024;
/// The largest body accepted. A saved document is a body, so this is the
/// effective document size ceiling for the editor.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

/// Header fields whose repetition this server refuses.
///
/// Each one decides either how the message is framed or whether the request is
/// admitted at all, and for those a second value is a contradiction rather than
/// a continuation. Fields that are genuinely list-valued are unaffected.
const AMBIGUOUS_WHEN_REPEATED: [&str; 5] = [
    "host",
    "origin",
    "content-length",
    "transfer-encoding",
    "x-lcl-token",
];

/// Why a request could not be read.
#[derive(Debug)]
pub enum RequestError {
    /// The peer closed before sending anything. Ordinary, not an error to log.
    Closed,
    /// The request was malformed, oversized or used an unsupported framing.
    Malformed(String),
    Io(io::Error),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Closed => f.write_str("connection closed"),
            RequestError::Malformed(detail) => write!(f, "malformed request: {detail}"),
            RequestError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

/// One parsed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path, percent-decoded, without the query string.
    pub path: String,
    /// Query parameters, percent-decoded.
    pub query: BTreeMap<String, String>,
    /// Header names lowercased, because HTTP header names are case-insensitive.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(String::as_str)
    }

    /// The body as text, when it is valid UTF-8.
    ///
    /// `02_LEXICAL/01` requires LCL source to be UTF-8, and a document that is
    /// not is refused rather than repaired.
    pub fn text(&self) -> Result<&str, RequestError> {
        std::str::from_utf8(&self.body)
            .map_err(|e| RequestError::Malformed(format!("body is not UTF-8: {e}")))
    }

    /// Read one request from a stream.
    pub fn read(stream: &mut BufReader<TcpStream>) -> Result<Request, RequestError> {
        let line = read_line(stream, MAX_REQUEST_LINE)?;
        if line.is_empty() {
            return Err(RequestError::Closed);
        }
        let mut parts = line.split(' ');
        let method = parts
            .next()
            .ok_or_else(|| RequestError::Malformed("no method".into()))?
            .to_string();
        let target = parts
            .next()
            .ok_or_else(|| RequestError::Malformed("no target".into()))?
            .to_string();
        let version = parts
            .next()
            .ok_or_else(|| RequestError::Malformed("no version".into()))?;
        if !version.starts_with("HTTP/1.") {
            return Err(RequestError::Malformed(format!(
                "unsupported version {version}"
            )));
        }
        // Origin-form only. An absolute-form target is what a proxy sends, and
        // this server is never behind one.
        if !target.starts_with('/') {
            return Err(RequestError::Malformed(
                "only origin-form targets are accepted".into(),
            ));
        }

        let (raw_path, raw_query) = match target.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (target.as_str(), None),
        };
        let path = percent_decode(raw_path)?;
        let mut query = BTreeMap::new();
        if let Some(raw) = raw_query {
            for pair in raw.split('&').filter(|p| !p.is_empty()) {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                query.insert(percent_decode(k)?, percent_decode(v)?);
            }
        }

        let mut headers = BTreeMap::new();
        let mut header_bytes = 0usize;
        loop {
            let line = read_line(stream, MAX_REQUEST_LINE)?;
            if line.is_empty() {
                break;
            }
            header_bytes = header_bytes.saturating_add(line.len());
            if header_bytes > MAX_HEADERS {
                return Err(RequestError::Malformed("header block too large".into()));
            }
            let (name, value) = line
                .split_once(':')
                .ok_or_else(|| RequestError::Malformed("header without a colon".into()))?;
            let name = name.trim().to_ascii_lowercase();
            // A repeated field that decides framing or passes a gate is an
            // ambiguity, not a correction. Keeping the last one resolves the
            // disagreement — which is the whole trick: two lengths decide where
            // this body ends and the next request begins, and a second `Host`
            // or `Origin` lets a request name something this server did not
            // bind *and* something it did, so the gate is satisfied by being
            // offered a choice. Other fields keep their previous behaviour;
            // only these decide admission.
            if AMBIGUOUS_WHEN_REPEATED.contains(&name.as_str()) && headers.contains_key(&name) {
                return Err(RequestError::Malformed(format!(
                    "{name} appears more than once; this server refuses an \
                     ambiguous request rather than choosing which one to believe"
                )));
            }
            headers.insert(name, value.trim().to_string());
        }

        // Exact framing only. A request carrying both a length and a transfer
        // encoding, or a length that is not a number, is refused: guessing
        // which one to believe is precisely how request smuggling works.
        if headers.contains_key("transfer-encoding") {
            return Err(RequestError::Malformed(
                "transfer-encoding is not accepted; send Content-Length".into(),
            ));
        }
        let length = match headers.get("content-length") {
            Some(raw) => raw
                .parse::<usize>()
                .map_err(|_| RequestError::Malformed("content-length is not a number".into()))?,
            None => 0,
        };
        if length > MAX_BODY {
            return Err(RequestError::Malformed(format!(
                "body of {length} bytes exceeds the {MAX_BODY}-byte limit"
            )));
        }
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).map_err(RequestError::Io)?;

        Ok(Request {
            method,
            path,
            query,
            headers,
            body,
        })
    }
}

/// Read one CRLF-terminated line, without its terminator.
fn read_line(stream: &mut BufReader<TcpStream>, limit: usize) -> Result<String, RequestError> {
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => {
                if raw.is_empty() {
                    return Ok(String::new());
                }
                return Err(RequestError::Malformed("truncated line".into()));
            }
            Ok(_) => {}
            Err(e) => return Err(RequestError::Io(e)),
        }
        if byte[0] == b'\n' {
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }
            return String::from_utf8(raw)
                .map_err(|_| RequestError::Malformed("line is not UTF-8".into()));
        }
        raw.push(byte[0]);
        if raw.len() > limit {
            return Err(RequestError::Malformed("line too long".into()));
        }
    }
}

/// Decode `%XX` escapes and `+` in a target or query value.
fn percent_decode(raw: &str) -> Result<String, RequestError> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = bytes
                    .get(i + 1..i + 3)
                    .ok_or_else(|| RequestError::Malformed("truncated percent escape".into()))?;
                let text = std::str::from_utf8(hex)
                    .map_err(|_| RequestError::Malformed("bad percent escape".into()))?;
                let value = u8::from_str_radix(text, 16)
                    .map_err(|_| RequestError::Malformed("bad percent escape".into()))?;
                out.push(value);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| RequestError::Malformed("target is not UTF-8".into()))
}

/// What a handler returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(body: String) -> Response {
        Response {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    pub fn html(body: &str) -> Response {
        Response {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn css(body: &str) -> Response {
        Response {
            status: 200,
            content_type: "text/css; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn javascript(body: &str) -> Response {
        Response {
            status: 200,
            content_type: "text/javascript; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    /// One image, as bytes.
    ///
    /// The body was already `Vec<u8>`, so this adds a content type and nothing
    /// else. That matters: a PNG pushed through `&str` would have to be valid
    /// UTF-8, which no PNG is, and the lossy conversion that makes it compile
    /// is the one that corrupts every byte a decoder needs.
    pub fn png(body: &'static [u8]) -> Response {
        Response {
            status: 200,
            content_type: "image/png",
            body: body.to_vec(),
        }
    }

    /// A refusal, as JSON so the frontend reads every reply the same way.
    ///
    /// The detail is the server's own words about the request. It never carries
    /// an LCL diagnostic: those are engine truth and travel in a report.
    pub fn error(status: u16, detail: &str) -> Response {
        let escaped = crate::http::escape_json(detail);
        Response {
            status,
            content_type: "application/json; charset=utf-8",
            body: format!("{{\n  \"error\": \"{escaped}\"\n}}\n").into_bytes(),
        }
    }

    /// Write this response and close.
    pub fn write(&self, stream: &mut TcpStream) -> io::Result<()> {
        let head = format!(
            "HTTP/1.1 {} {}\r\n\
             Content-Type: {}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             Cache-Control: no-store\r\n\
             X-Content-Type-Options: nosniff\r\n\
             Content-Security-Policy: default-src 'self'; style-src 'self'; \
             script-src 'self'; img-src 'self' data:; connect-src 'self'; \
             form-action 'none'; frame-ancestors 'none'; base-uri 'none'\r\n\
             Referrer-Policy: no-referrer\r\n\
             \r\n",
            self.status,
            reason(self.status),
            self.content_type,
            self.body.len(),
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
    }
}

/// The reason phrase for each status this server emits.
fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Content",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Escape one string for a JSON string literal.
///
/// The same rules `lcl-protocol`'s writer uses. This exists because a refusal
/// is produced before any engine record and therefore before that writer is
/// reachable.
pub fn escape_json(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// A server-sent-events stream, for a run in progress.
///
/// One direction, text framing, no handshake: everything a progress feed needs
/// and nothing a WebSocket would have added.
pub struct EventStream {
    stream: TcpStream,
}

impl EventStream {
    /// Take over a connection and send the SSE response head.
    pub fn open(mut stream: TcpStream) -> io::Result<EventStream> {
        let head = "HTTP/1.1 200 OK\r\n\
                    Content-Type: text/event-stream; charset=utf-8\r\n\
                    Cache-Control: no-store\r\n\
                    Connection: close\r\n\
                    X-Content-Type-Options: nosniff\r\n\
                    \r\n";
        stream.write_all(head.as_bytes())?;
        stream.flush()?;
        Ok(EventStream { stream })
    }

    /// Send one named event carrying one JSON payload.
    ///
    /// A payload containing a line feed would break SSE framing, so every line
    /// is written as its own `data:` field, which is what the format requires.
    pub fn send(&mut self, event: &str, payload: &str) -> io::Result<()> {
        let mut frame = String::with_capacity(payload.len() + 32);
        frame.push_str("event: ");
        frame.push_str(event);
        frame.push('\n');
        for line in payload.split('\n') {
            frame.push_str("data: ");
            frame.push_str(line);
            frame.push('\n');
        }
        frame.push('\n');
        self.stream.write_all(frame.as_bytes())?;
        self.stream.flush()
    }
}
