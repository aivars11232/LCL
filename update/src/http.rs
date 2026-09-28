//! HTTPS GET, and nothing more, for the release source.
//!
//! TLS is `rustls` on `ring`, trusting the system's root certificates
//! (`rustls-native-certs`); a certificate that does not verify is a refusal,
//! never a retry without verification. This module only frames HTTP/1.1: one
//! GET per connection, a body framed by Content-Length or chunked encoding (an
//! unframed body could be cut short unnoticed), a size limit enforced while
//! reading, and at most five redirects, each to an https URL. Plain http is
//! accepted only when the caller says so, which only a test build does.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;
const MAX_HEADERS: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Why a request failed, in the three kinds a person is told apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The network or the host could not be reached.
    Offline(String),
    /// The host answered with an error status.
    Status(u16),
    /// The answer, the redirect or the TLS identity could not be trusted.
    Invalid(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Offline(why) => write!(f, "could not reach the update server: {why}"),
            Failure::Status(code) => write!(f, "the update server answered HTTP {code}"),
            Failure::Invalid(why) => write!(f, "the update server's answer was refused: {why}"),
        }
    }
}

/// An absolute http(s) URL, as far as this client needs one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Url {
    https: bool,
    host: String,
    port: u16,
    /// Path and query, starting with `/`.
    path: String,
}

impl Url {
    fn parse(text: &str, allow_http: bool) -> Result<Url, Failure> {
        let invalid = || Failure::Invalid(format!("{text:?} is not an acceptable URL"));
        let (https, rest) = if let Some(rest) = text.strip_prefix("https://") {
            (true, rest)
        } else if let Some(rest) = text.strip_prefix("http://").filter(|_| allow_http) {
            (false, rest)
        } else {
            return Err(invalid());
        };
        let (authority, path) = match rest.find('/') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, "/"),
        };
        if authority.contains('@') || path.bytes().any(|b| b <= b' ' || b == 0x7f) {
            return Err(invalid());
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().map_err(|_| invalid())?),
            None => (authority, if https { 443 } else { 80 }),
        };
        let host_ok = !host.is_empty()
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
        if !host_ok {
            return Err(invalid());
        }
        Ok(Url {
            https,
            host: host.to_ascii_lowercase(),
            port,
            path: path.to_string(),
        })
    }
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn tls_config() -> Result<Arc<rustls::ClientConfig>, Failure> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let found = rustls_native_certs::load_native_certs();
            let mut roots = rustls::RootCertStore::empty();
            for certificate in found.certs {
                let _ = roots.add(certificate);
            }
            if roots.is_empty() {
                return Err("this system has no trusted root certificates".to_string());
            }
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .with_root_certificates(roots)
                .with_no_client_auth();
            Ok(Arc::new(config))
        })
        .clone()
        .map_err(Failure::Invalid)
}

fn connect(url: &Url) -> Result<Box<dyn Stream>, Failure> {
    let addresses: Vec<_> = (url.host.as_str(), url.port)
        .to_socket_addrs()
        .map_err(|e| Failure::Offline(format!("{}: {e}", url.host)))?
        .collect();
    let mut last = format!("{}: no address", url.host);
    let mut tcp = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                tcp = Some(stream);
                break;
            }
            Err(e) => last = format!("{}: {e}", url.host),
        }
    }
    let tcp = tcp.ok_or(Failure::Offline(last))?;
    tcp.set_read_timeout(Some(IO_TIMEOUT))
        .and_then(|_| tcp.set_write_timeout(Some(IO_TIMEOUT)))
        .map_err(|e| Failure::Offline(e.to_string()))?;
    if !url.https {
        return Ok(Box::new(tcp));
    }
    let name = rustls::pki_types::ServerName::try_from(url.host.clone())
        .map_err(|e| Failure::Invalid(e.to_string()))?;
    let connection = rustls::ClientConnection::new(tls_config()?, name)
        .map_err(|e| Failure::Invalid(format!("TLS: {e}")))?;
    Ok(Box::new(rustls::StreamOwned::new(connection, tcp)))
}

fn io(e: std::io::Error) -> Failure {
    // A certificate or handshake problem surfaces from rustls as an I/O error
    // carrying the TLS error; that is never "offline".
    match e.kind() {
        std::io::ErrorKind::InvalidData => Failure::Invalid(format!("TLS: {e}")),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            Failure::Offline(format!("timed out: {e}"))
        }
        _ => Failure::Offline(e.to_string()),
    }
}

/// GET `url` and return its body, at most `limit` bytes. `accept` is the
/// Accept header; `progress(read, total)` is called as the body arrives.
pub fn get(
    url: &str,
    accept: &str,
    limit: u64,
    allow_http: bool,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, Failure> {
    let mut url = Url::parse(url, allow_http)?;
    for _ in 0..=MAX_REDIRECTS {
        let mut stream = connect(&url)?;
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: lcl-update/{}\r\nAccept: {accept}\r\n\
             Accept-Encoding: identity\r\nConnection: close\r\n\r\n",
            url.path,
            url.host,
            crate::PRODUCT_VERSION
        );
        stream.write_all(request.as_bytes()).map_err(io)?;
        stream.flush().map_err(io)?;
        let mut reader = BufReader::new(stream);
        let (status, headers) = read_head(&mut reader)?;
        let header = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
        };
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = header("location")
                .ok_or_else(|| Failure::Invalid("a redirect without a Location".to_string()))?;
            url = Url::parse(location, allow_http)?;
            continue;
        }
        if status != 200 {
            return Err(Failure::Status(status));
        }
        let chunked =
            header("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"));
        let length = match header("content-length") {
            Some(v) => Some(
                v.trim()
                    .parse::<u64>()
                    .map_err(|_| Failure::Invalid("a bad Content-Length".to_string()))?,
            ),
            None => None,
        };
        return if chunked {
            read_chunked(&mut reader, limit, progress)
        } else if let Some(length) = length {
            if length > limit {
                return Err(Failure::Invalid(format!(
                    "{length} bytes is more than the {limit} expected"
                )));
            }
            read_exact(&mut reader, length, progress)
        } else {
            Err(Failure::Invalid("a body with no length".to_string()))
        };
    }
    Err(Failure::Invalid("too many redirects".to_string()))
}

type Head = (u16, Vec<(String, String)>);

fn read_line(reader: &mut dyn BufRead, budget: &mut usize) -> Result<String, Failure> {
    let mut line = Vec::new();
    let read = reader
        .take(*budget as u64)
        .read_until(b'\n', &mut line)
        .map_err(io)?;
    *budget = budget.saturating_sub(read);
    if !line.ends_with(b"\n") {
        return Err(Failure::Invalid(
            "a truncated or oversized header".to_string(),
        ));
    }
    let line = String::from_utf8(line)
        .map_err(|_| Failure::Invalid("a header that is not UTF-8".to_string()))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn read_head(reader: &mut dyn BufRead) -> Result<Head, Failure> {
    let mut budget = MAX_HEADERS;
    let status_line = read_line(reader, &mut budget)?;
    let status = status_line
        .strip_prefix("HTTP/1.1 ")
        .or_else(|| status_line.strip_prefix("HTTP/1.0 "))
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| Failure::Invalid("not an HTTP response".to_string()))?;
    let mut headers = Vec::new();
    loop {
        let line = read_line(reader, &mut budget)?;
        if line.is_empty() {
            return Ok((status, headers));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| Failure::Invalid("a malformed header".to_string()))?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
}

fn read_exact(
    reader: &mut dyn Read,
    length: u64,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, Failure> {
    let mut body = Vec::with_capacity(length.min(1 << 20) as usize);
    let mut buffer = [0u8; 64 * 1024];
    while (body.len() as u64) < length {
        let want = buffer.len().min((length - body.len() as u64) as usize);
        let n = reader.read(&mut buffer[..want]).map_err(io)?;
        if n == 0 {
            return Err(Failure::Offline("the connection closed early".to_string()));
        }
        body.extend_from_slice(&buffer[..n]);
        progress(body.len() as u64, Some(length));
    }
    Ok(body)
}

fn read_chunked(
    reader: &mut dyn BufRead,
    limit: u64,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, Failure> {
    let mut body = Vec::new();
    loop {
        let mut budget = 1024;
        let line = read_line(reader, &mut budget)?;
        let size = u64::from_str_radix(line.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| Failure::Invalid("a bad chunk size".to_string()))?;
        if size == 0 {
            let mut budget = MAX_HEADERS;
            while !read_line(reader, &mut budget)?.is_empty() {}
            return Ok(body);
        }
        if body.len() as u64 + size > limit {
            return Err(Failure::Invalid(format!(
                "more than the {limit} bytes expected"
            )));
        }
        let chunk = read_exact(reader, size, &mut |_, _| {})?;
        body.extend_from_slice(&chunk);
        progress(body.len() as u64, None);
        let mut budget = 2;
        if !read_line(reader, &mut budget)?.is_empty() {
            return Err(Failure::Invalid("a chunk longer than it said".to_string()));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A local plain-http server answering each request by its path, until
    /// dropped. Each answer is the whole raw response.
    pub struct TestServer {
        pub base: String,
    }

    pub fn serve(routes: Vec<(String, Vec<u8>)>) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok() && line.trim() != "" {
                    line.clear();
                }
                let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
                let answer = routes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, a)| a.clone())
                    .unwrap_or_else(|| {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()
                    });
                let _ = stream.write_all(&answer);
            }
        });
        TestServer { base }
    }

    pub fn ok(body: &[u8]) -> Vec<u8> {
        let mut answer =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        answer.extend_from_slice(body);
        answer
    }

    #[test]
    fn bodies_are_read_by_their_framing_and_redirects_followed() {
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n".to_vec();
        let server = serve(vec![
            ("/plain".into(), ok(b"body")),
            ("/chunked".into(), chunked),
            (
                "/moved".into(),
                b"HTTP/1.1 302 Found\r\nLocation: /plain\r\nContent-Length: 0\r\n\r\n".to_vec(),
            ),
            ("/unframed".into(), b"HTTP/1.1 200 OK\r\n\r\nbody".to_vec()),
            (
                "/short".into(),
                b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nbody".to_vec(),
            ),
        ]);
        let get = |path: &str, limit: u64| {
            get(
                &format!("{}{path}", server.base),
                "*/*",
                limit,
                true,
                &mut |_, _| {},
            )
        };
        assert_eq!(get("/plain", 100).unwrap(), b"body");
        assert_eq!(get("/chunked", 100).unwrap(), b"hello world");
        assert_eq!(get("/missing", 100).unwrap_err(), Failure::Status(404));
        assert!(
            matches!(get("/plain", 3).unwrap_err(), Failure::Invalid(_)),
            "over the limit"
        );
        assert!(
            matches!(get("/chunked", 7).unwrap_err(), Failure::Invalid(_)),
            "over the limit"
        );
        assert!(matches!(
            get("/unframed", 100).unwrap_err(),
            Failure::Invalid(_)
        ));
        assert!(matches!(
            get("/short", 100).unwrap_err(),
            Failure::Offline(_)
        ));
        // A relative Location is not an absolute https URL.
        assert!(matches!(
            get("/moved", 100).unwrap_err(),
            Failure::Invalid(_)
        ));
    }

    #[test]
    fn only_https_unless_a_test_allows_http_and_nothing_else() {
        for url in [
            "http://example.org/x",
            "ftp://example.org/x",
            "file:///etc/passwd",
            "https://user@host/x",
            "https://ho st/x",
        ] {
            assert!(
                matches!(
                    get(url, "*/*", 10, false, &mut |_, _| {}).unwrap_err(),
                    Failure::Invalid(_)
                ),
                "{url}"
            );
        }
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let offline = get(
            &format!("http://127.0.0.1:{port}/x"),
            "*/*",
            10,
            true,
            &mut |_, _| {},
        );
        assert!(matches!(offline.unwrap_err(), Failure::Offline(_)));
    }
}
