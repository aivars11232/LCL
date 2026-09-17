//! `TcpTransport` against real HTTP/1.1 responses on a real loopback socket.
//!
//! NET-01 and NET-02. The transport is the one place in the implementation
//! where bytes arrive from outside, and what it decides those bytes *mean* is
//! not a detail: a download's postcondition is that "destination bytes equal
//! received source", so a body that is not the body is a wrong answer that
//! looks like a right one.
//!
//! RFC 9112 §6.3 fixes how a message's length is determined, §7.1 how chunked
//! transfer coding is decoded, and §8 that an incomplete message is an error
//! rather than a shorter message. Each case below serves exactly one real
//! response over a bounded loopback listener and asserts what the transport
//! made of it. Nothing is mocked; a double would only agree with itself.

use lcl_capabilities::bounds::{Bounds, Deadline};
use lcl_capabilities::grant::{Grants, Refusal};
use lcl_capabilities::net::{Address, NetError, Transport};
use lcl_capabilities::TcpTransport;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// Every fixture is bounded: the listener serves exactly one connection and
/// the thread ends, so no case can leave a server behind.
struct Fixture {
    port: u16,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    /// Serve one connection with exactly these raw response bytes, then close.
    fn raw(response: Vec<u8>) -> Fixture {
        Fixture::with(move |mut stream| {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        })
    }

    fn with<F>(handler: F) -> Fixture
    where
        F: FnOnce(TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().expect("bound").port();
        let server = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                handler(stream);
            }
        });
        Fixture {
            port,
            server: Some(server),
        }
    }

    fn address(&self) -> Address {
        Address::parse(&format!("http://127.0.0.1:{}/resource", self.port)).expect("a valid URI")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn transport() -> TcpTransport {
    TcpTransport::new(Grants::none().permit_network_host("127.0.0.1"))
}

fn bounds() -> Bounds {
    // Generous, so that these cases are about framing and not about time.
    Bounds::new().with_deadline(Some(Deadline::from_nanos(10_000_000_000)))
}

// ---------------------------------------------------------------------------
// NET-01 — the body is the body
// ---------------------------------------------------------------------------

/// The control: an ordinary `Content-Length` response.
#[test]
fn a_declared_length_response_yields_exactly_that_body() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".to_vec(),
    );
    let response = transport()
        .get(&fixture.address(), &bounds())
        .expect("an ordinary response");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"hello");
}

/// A zero-length body is a body, and a successful one.
#[test]
fn a_zero_length_response_yields_an_empty_body() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
    );
    let response = transport()
        .get(&fixture.address(), &bounds())
        .expect("an empty response is still a response");
    assert_eq!(response.status, 204);
    assert!(response.body.is_empty());
}

/// Chunked transfer coding must be decoded.
///
/// RFC 9112 §7.1. The chunk sizes, their CRLFs and the terminating zero chunk
/// are framing, not content. Returning them as the body would write the
/// framing into the downloaded file.
#[test]
fn a_chunked_response_is_decoded_rather_than_returned_raw() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
          5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"
            .to_vec(),
    );
    let response = transport()
        .get(&fixture.address(), &bounds())
        .expect("chunked is a supported framing");

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body, b"hello world",
        "the decoded content, with no chunk-size delimiters in it"
    );
}

/// A chunked response whose final zero chunk never arrives is incomplete.
#[test]
fn an_unterminated_chunked_response_is_not_a_completed_transfer() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n\
          5\r\nhello\r\n"
            .to_vec(),
    );
    let result = transport().get(&fixture.address(), &bounds());
    assert!(
        result.is_err(),
        "an incomplete message is an error, not a shorter message: {result:?}"
    );
}

/// A declared length that never fully arrives is incomplete.
///
/// RFC 9112 §8: a message ending before its declared length is an incomplete
/// message. Reporting the truncated prefix as the content would satisfy a
/// download's postcondition with bytes that are not the source.
#[test]
fn a_short_response_body_is_not_a_completed_transfer() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello".to_vec(),
    );
    let result = transport().get(&fixture.address(), &bounds());
    assert!(
        result.is_err(),
        "five bytes are not the eleven that were declared: {result:?}"
    );
}

/// Contradictory framing is refused rather than guessed at.
///
/// RFC 9112 §6.3: a message with both `Transfer-Encoding` and `Content-Length`
/// must be treated as an error by a recipient that is not a proxy. Choosing
/// one of the two is exactly the disagreement request smuggling relies on.
#[test]
fn contradictory_framing_is_refused_rather_than_guessed() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\
          Connection: close\r\n\r\n5\r\nhello\r\n0\r\n\r\n"
            .to_vec(),
    );
    let result = transport().get(&fixture.address(), &bounds());
    assert!(
        result.is_err(),
        "two framings that disagree are an error, not a choice: {result:?}"
    );
}

/// A transfer coding this transport does not implement is reported as such.
#[test]
fn an_unsupported_transfer_coding_is_refused() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\nConnection: close\r\n\r\n\
          5\r\nhello\r\n0\r\n\r\n"
            .to_vec(),
    );
    let result = transport().get(&fixture.address(), &bounds());
    assert!(
        result.is_err(),
        "a coding that is not implemented is not silently ignored: {result:?}"
    );
}

/// A response with no length and no coding is delimited by the close.
///
/// RFC 9112 §6.3 item 8: for a response, a connection close is a valid message
/// terminator when nothing else determines the length.
#[test]
fn a_close_delimited_response_is_read_to_the_close() {
    let fixture = Fixture::raw(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nhello".to_vec());
    let response = transport()
        .get(&fixture.address(), &bounds())
        .expect("close-delimited is a legal framing for a response");
    assert_eq!(response.body, b"hello");
}

/// A non-2xx response is still a response, and its status is reported as it
/// stands. What that means for an operation is the operation's contract to
/// decide, not the transport's.
#[test]
fn a_non_success_status_is_reported_with_its_body() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot there"
            .to_vec(),
    );
    let response = transport()
        .get(&fixture.address(), &bounds())
        .expect("a 404 is an answer");
    assert_eq!(response.status, 404);
    assert_eq!(response.body, b"not there");
}

// ---------------------------------------------------------------------------
// NET-02 — what the declared bound actually covers
// ---------------------------------------------------------------------------

/// NET-02: a declared bound covers the whole exchange.
///
/// Every read arrives well inside the bound and the transfer as a whole takes
/// many times it. A per-read limit cannot see that, which is why this used to
/// complete in 2.003 s against a bound of 300 ms.
///
/// The policy this asserts is the owner's, recorded because canonical does not
/// decide it: `core.execute`, `core.start` and `core.stop` register a `timeout`
/// parameter and `core.download`/`core.upload` register none, so a network
/// deadline is host policy. It is a **total budget** across resolution,
/// connection, send and receive, with remaining-time accounting.
#[test]
fn a_declared_bound_covers_the_whole_transfer_and_not_each_read() {
    let declared = Duration::from_millis(300);
    let fixture = Fixture::with(|mut stream| {
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        let _ =
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n");
        let _ = stream.flush();
        // Twenty bytes, one every 100ms: each read is timely, the whole is not.
        for _ in 0..20 {
            if stream.write_all(b"x").is_err() {
                return;
            }
            let _ = stream.flush();
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    let started = Instant::now();
    let result = transport().get(
        &fixture.address(),
        &Bounds::new().with_deadline(Some(Deadline::from_nanos(declared.as_nanos()))),
    );
    let elapsed = started.elapsed();

    match result {
        Err(NetError::Bounded(_)) => {}
        other => panic!(
            "a transfer still running after {elapsed:?} against a declared bound \
             of {declared:?} must be stopped as a host limitation: {other:?}"
        ),
    }
    assert!(
        elapsed < declared * 4,
        "and stopped near the bound rather than after {elapsed:?}"
    );
}

/// The control: a prompt exchange well inside its bound still completes.
///
/// A budget that stopped everything would pass the case above and fail this one.
#[test]
fn a_prompt_exchange_completes_well_inside_its_bound() {
    let fixture = Fixture::raw(
        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".to_vec(),
    );
    let response = transport()
        .get(
            &fixture.address(),
            &Bounds::new().with_deadline(Some(Deadline::from_nanos(
                Duration::from_secs(5).as_nanos(),
            ))),
        )
        .expect("a prompt loopback exchange is not a timeout");
    assert_eq!(response.body, b"hello");
}

/// A connection that cannot be established is bounded too.
///
/// `TcpStream::connect` takes no timeout of its own, so before this the
/// resolution and connection phases were unbounded however small the declared
/// bound was. The address is in the documentation-only TEST-NET-1 range, which
/// does not answer.
#[test]
fn an_unanswering_connection_is_stopped_by_the_declared_bound() {
    let declared = Duration::from_millis(400);
    let address =
        Address::parse("http://192.0.2.1:80/resource").expect("a valid documentation address");
    let mut transport = TcpTransport::new(Grants::none().permit_network_host("192.0.2.1"));

    let started = Instant::now();
    let result = transport.get(
        &address,
        &Bounds::new().with_deadline(Some(Deadline::from_nanos(declared.as_nanos()))),
    );
    let elapsed = started.elapsed();

    assert!(
        result.is_err(),
        "an address that never answers is not a body"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "and the wait ended near the declared bound rather than after {elapsed:?}"
    );
}

// ---------------------------------------------------------------------------
// PRETEST-04 F25: valid URI forms
// ---------------------------------------------------------------------------

/// Every valid URI form becomes an address this transport can reach, or is
/// refused as a host limitation. None is a malformed address, because the
/// `URI` literal profile already accepted it.
#[test]
fn valid_uri_forms_are_addresses_or_host_limitations() {
    let parsed = |uri: &str| Address::parse(uri).unwrap_or_else(|e| panic!("{uri}: {e:?}"));
    let address = parsed("http://user@[2001:db8::1]:8080/a/b?c=d");
    assert_eq!(
        (address.host.as_str(), address.port, address.target.as_str()),
        ("[2001:db8::1]", 8080, "/a/b?c=d")
    );
    let address = parsed("http://[::1]/x");
    assert_eq!((address.host.as_str(), address.port), ("[::1]", 80));
    let address = parsed("http://example.invalid?q=1");
    assert_eq!(
        (address.host.as_str(), address.port, address.target.as_str()),
        ("example.invalid", 80, "/?q=1")
    );
    assert_eq!(parsed("http://example.invalid:/x").port, 80);
    assert!(parsed("HTTPS://example.invalid/x").is_secure());

    for uri in [
        "ftp://example.invalid/x",
        "ftp://example.invalid:21/x",
        "mailto:someone@example.invalid",
        "urn:example:lib",
        "file:///etc/hosts",
        "http://example.invalid:65536/x",
    ] {
        match Address::parse(uri) {
            Err(NetError::Refused(Refusal::Unavailable(_))) => {}
            other => panic!("{uri}: {other:?}"),
        }
    }
}

/// An IP-literal host is connected to without its brackets.
#[test]
fn an_ipv6_literal_host_is_reached() {
    let Ok(listener) = TcpListener::bind("[::1]:0") else {
        return; // this machine has no IPv6 loopback to reach
    };
    let port = listener.local_addr().expect("bound").port();
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        }
    });
    let address = Address::parse(&format!("http://[::1]:{port}/resource")).expect("valid");
    let result = TcpTransport::new(Grants::none().permit_network_host("[::1]"))
        .get(&address, &bounds())
        .map(|response| response.body);
    // A client that never connected would leave the listener waiting forever.
    let _ = TcpStream::connect(("::1", port));
    let _ = server.join();
    assert_eq!(result, Ok(b"ok".to_vec()));
}
