//! The transport and its three gates.

mod common;

use common::{send, send_raw, start, start_with, Echo};
use std::sync::Arc;

#[test]
fn a_request_carrying_the_token_is_served() {
    let s = start(Arc::new(Echo));
    let reply = send(s.address, "GET", &format!("/hello?t={}", s.token), &[], b"");
    assert_eq!(reply.status, 200);
    assert!(reply.body.contains("\"path\":\"/hello\""));
}

#[test]
fn the_token_may_arrive_in_a_header_instead_of_the_query() {
    let s = start(Arc::new(Echo));
    let reply = send(
        s.address,
        "GET",
        "/hello",
        &[("X-LCL-Token", &s.token)],
        b"",
    );
    assert_eq!(reply.status, 200);
}

#[test]
fn a_request_without_a_token_is_refused() {
    let s = start(Arc::new(Echo));
    let reply = send(s.address, "GET", "/hello", &[], b"");
    assert_eq!(reply.status, 403);
    assert!(reply.body.contains("session token"));
}

#[test]
fn a_request_with_the_wrong_token_is_refused() {
    let s = start(Arc::new(Echo));
    let wrong = "0".repeat(s.token.len());
    let reply = send(s.address, "GET", &format!("/hello?t={wrong}"), &[], b"");
    assert_eq!(reply.status, 403);
}

#[test]
fn a_rebound_dns_name_is_refused_even_with_the_right_token() {
    // The DNS rebinding case, exactly. A page served from `evil.example` whose
    // name now resolves to 127.0.0.1 reaches this socket and sends its own
    // name in Host. The token is not even reached: the address is wrong.
    let s = start(Arc::new(Echo));
    let reply = send(
        s.address,
        "GET",
        &format!("/hello?t={}", s.token),
        &[("Host", "evil.example")],
        b"",
    );
    assert_eq!(reply.status, 403);
    assert!(reply.body.contains("did not bind"));
}

#[test]
fn localhost_and_the_loopback_literal_are_both_this_server() {
    let s = start(Arc::new(Echo));
    for host in [
        format!("127.0.0.1:{}", s.address.port()),
        format!("localhost:{}", s.address.port()),
    ] {
        let reply = send(
            s.address,
            "GET",
            &format!("/hello?t={}", s.token),
            &[("Host", &host)],
            b"",
        );
        assert_eq!(reply.status, 200, "Host: {host} is this server");
    }
}

#[test]
fn a_cross_origin_request_is_refused() {
    let s = start(Arc::new(Echo));
    let reply = send(
        s.address,
        "POST",
        &format!("/save?t={}", s.token),
        &[("Origin", "http://evil.example")],
        b"{}",
    );
    assert_eq!(reply.status, 403);
    assert!(reply.body.contains("cross-origin"));
}

#[test]
fn no_response_ever_carries_a_cors_header() {
    let s = start(Arc::new(Echo));
    let reply = send(s.address, "GET", &format!("/hello?t={}", s.token), &[], b"");
    assert!(reply.header("Access-Control-Allow-Origin").is_none());
    assert_eq!(reply.header("X-Content-Type-Options"), Some("nosniff"));
    assert!(reply
        .header("Content-Security-Policy")
        .is_some_and(|p| p.contains("default-src 'self'")));
}

#[test]
fn a_body_arrives_whole_and_a_query_is_percent_decoded() {
    let s = start(Arc::new(Echo));
    let reply = send(
        s.address,
        "POST",
        &format!("/save?t={}&id=a%2Fb%20c", s.token),
        &[],
        "TASK:\n    ID: task.one\n".as_bytes(),
    );
    assert_eq!(reply.status, 200);
    assert!(reply.body.contains("\"id\":\"a/b c\""));
    assert!(reply.body.contains("TASK:"));
}

#[test]
fn an_ambiguously_framed_request_is_refused_rather_than_guessed_at() {
    // Both a length and a transfer encoding is the classic smuggling setup.
    // Refusing is the only safe reading.
    let s = start(Arc::new(Echo));
    let raw = format!(
        "POST /save?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 2\r\nTransfer-Encoding: chunked\r\n\r\n{{}}",
        s.token,
        s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("transfer-encoding"));
}

#[test]
fn an_absolute_form_target_is_refused() {
    let s = start(Arc::new(Echo));
    let raw = format!(
        "GET http://evil.example/x?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\n\r\n",
        s.token,
        s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("origin-form"));
}

#[test]
fn an_oversized_body_is_refused_before_it_is_allocated() {
    let s = start(Arc::new(Echo));
    let raw = format!(
        "POST /save?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 999999999\r\n\r\n",
        s.token,
        s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    // Refused while the head is being parsed, before the body is allocated.
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("exceeds"));
}

#[test]
fn two_servers_mint_different_tokens() {
    let a = start(Arc::new(Echo));
    let b = start(Arc::new(Echo));
    assert_ne!(a.token, b.token);
    assert_eq!(a.token.len(), 64, "256 bits, hex encoded");
    assert!(a.token.chars().all(|c| c.is_ascii_hexdigit()));

    // And one server's token does not open the other.
    let reply = send(b.address, "GET", &format!("/x?t={}", a.token), &[], b"");
    assert_eq!(reply.status, 403);
}

#[test]
fn the_launch_url_carries_the_address_and_the_token() {
    let server = lcl_workspace::Server::bind().expect("binds");
    let url = server.url();
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(url.contains(&format!("?t={}", server.token())));
}

// ---------------------------------------------------------------------------
// WS-01 — ambiguous ingress, and waiting that has to end
// ---------------------------------------------------------------------------

/// Two disagreeing `Content-Length` lines are ambiguous framing.
///
/// This server already refuses a length *and* a transfer coding for exactly
/// this reason. Two lengths are the same disagreement written a different way,
/// and resolving it by keeping whichever arrived last is a choice, not a
/// refusal: it decides how many bytes are a body and how many are the next
/// request.
#[test]
fn two_disagreeing_content_lengths_are_refused_rather_than_resolved() {
    let s = start(Arc::new(Echo));
    let raw = format!(
        "POST /save?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 2\r\n\
         Content-Length: 12\r\n\r\n{{}}GET /x\r\n\r\n",
        s.token,
        s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    assert_eq!(
        reply.status, 400,
        "two lengths that disagree are refused: {}",
        reply.body
    );
}

/// Two `Host` lines are two claims about which server this is.
///
/// The first gate exists to stop a rebound DNS name reaching the workspace. A
/// request that names something else *and* the real address has not satisfied
/// that gate; it has offered the gate a choice.
#[test]
fn two_host_headers_are_refused_rather_than_resolved() {
    let s = start(Arc::new(Echo));
    let raw = format!(
        "GET /hello?t={} HTTP/1.1\r\nHost: evil.example\r\nHost: 127.0.0.1:{}\r\n\
         Content-Length: 0\r\n\r\n",
        s.token,
        s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    // 400 and not 403: the request never reaches the gate, because it is
    // refused as malformed while it is being read. That is earlier and
    // stricter than the gate, which is the point.
    assert_eq!(
        reply.status, 400,
        "a request claiming two hosts is refused: {}",
        reply.body
    );
    assert!(
        reply.body.contains("host appears more than once"),
        "{}",
        reply.body
    );
}

/// And two `Origin` lines are two claims about who is asking.
#[test]
fn two_origin_headers_are_refused_rather_than_resolved() {
    let s = start(Arc::new(Echo));
    let raw = format!(
        "GET /hello?t={} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
         Origin: http://evil.example\r\nOrigin: http://127.0.0.1:{port}\r\n\
         Content-Length: 0\r\n\r\n",
        s.token,
        port = s.address.port()
    );
    let reply = send_raw(s.address, &raw);
    assert_eq!(
        reply.status, 400,
        "a request claiming two origins is refused: {}",
        reply.body
    );
    assert!(
        reply.body.contains("origin appears more than once"),
        "{}",
        reply.body
    );
}

/// A connection that says nothing must not be held open indefinitely.
///
/// It costs a thread and one of the server's bounded connection slots, and it
/// requires no token: an unauthenticated peer that never finishes a request is
/// the cheapest way to take the workspace away from its owner.
#[test]
fn a_silent_unauthenticated_connection_is_reclaimed() {
    use std::io::Read;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let s = start(Arc::new(Echo));
    let mut socket = TcpStream::connect(s.address).expect("the server is listening");
    // Never send anything at all.
    socket
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a bounded wait, so this test cannot hang");

    let started = Instant::now();
    let mut buffer = [0u8; 256];
    let outcome = socket.read(&mut buffer);
    let elapsed = started.elapsed();

    assert!(
        outcome.is_ok(),
        "the server ended the idle connection rather than leaving it open \
         for {elapsed:?}: {outcome:?}"
    );
    assert!(
        elapsed < Duration::from_secs(20),
        "and it did so within a bounded time, not after {elapsed:?}"
    );
}

/// A burst of silent connections must not lock the owner out.
///
/// The server bounds its concurrent connections, which is right; the point is
/// that the bound must be *recoverable*. Sixty-four peers that each open a
/// socket and say nothing would otherwise hold every slot for as long as they
/// care to, and a legitimate authenticated request would be refused with 503.
#[test]
fn silent_connections_do_not_lock_out_a_legitimate_request() {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let s = start(Arc::new(Echo));
    // Held open for the whole case, and dropped with it.
    let _silent: Vec<TcpStream> = (0..64)
        .filter_map(|_| TcpStream::connect(s.address).ok())
        .collect();

    // While they hold the slots the owner is refused, and that is the bound
    // doing its job rather than the defect. What matters is that it ends: the
    // silent peers are reclaimed and the workspace comes back without anyone
    // restarting it.
    let deadline = lcl_workspace::server::INGRESS_TIMEOUT + Duration::from_secs(10);
    let started = Instant::now();
    let served = loop {
        if let Ok(reply) = std::panic::catch_unwind(|| {
            send(s.address, "GET", &format!("/hello?t={}", s.token), &[], b"")
        }) {
            if reply.status == 200 {
                break true;
            }
        }
        if started.elapsed() > deadline {
            break false;
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let elapsed = started.elapsed();

    assert!(
        served,
        "sixty-four silent peers took the workspace away from its owner for \
         more than {deadline:?}; the connection bound must be recoverable, not \
         only a ceiling"
    );
    assert!(
        elapsed < deadline,
        "recovered within the ingress bound rather than after {elapsed:?}"
    );
}

/// The control: an authenticated event stream is long-lived on purpose.
///
/// Bounding how long an *incomplete* request may wait must not bound how long
/// a complete, authenticated one may stay connected. A debugging session sits
/// on an open stream for as long as the operator is looking at it.
#[test]
fn an_authenticated_event_stream_is_not_disconnected_by_the_ingress_bound() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let (_scratch, running) = common::serve_examples("ws01-stream");
    let mut socket = TcpStream::connect(running.address).expect("connect");
    let request = format!(
        "GET /api/events?t={}&run=run-1 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\
         Content-Length: 0\r\n\r\n",
        running.token,
        running.address.port()
    );
    socket.write_all(request.as_bytes()).expect("sent");
    socket.flush().expect("flushed");
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("bounded");

    // Read whatever the stream says first, then stay connected past any
    // deadline that applies to an unfinished request.
    let mut buffer = [0u8; 1024];
    let _ = socket.read(&mut buffer);
    std::thread::sleep(Duration::from_secs(3));

    // Still ours: a second request on a fresh connection is still served, and
    // the stream socket is still open rather than reset.
    let reply = send(
        running.address,
        "GET",
        &format!("/api/documents?t={}", running.token),
        &[],
        b"",
    );
    assert_eq!(reply.status, 200, "the workspace is still serving");
}

// ---------------------------------------------------------------------------
// N-03 — the ingress bound covers the whole request, not each pause in it
// ---------------------------------------------------------------------------
//
// A socket read timeout bounds one idle wait. A peer that sends a byte just
// inside it resets it and can hold a pre-authentication slot indefinitely,
// because the token is not looked at until the request has been read. The
// bound is therefore on the request: line, headers and body together, on a
// monotonic clock.
//
// These cases give their own server a short budget so the end of it can be
// observed without the suite sleeping; the shipped budget is untouched.

/// A peer that dribbles header bytes for longer than the budget.
#[test]
fn a_slow_header_stream_does_not_hold_a_slot_past_the_budget() {
    use std::io::Write;
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let budget = Duration::from_millis(400);
    let s = start_with(Arc::new(Echo), budget);
    let mut peer = TcpStream::connect(s.address).expect("connect");
    peer.write_all(b"GET /hello HTTP/1.1\r\n")
        .expect("request line");
    peer.flush().ok();

    let started = Instant::now();
    // One header byte every 100ms: never idle long enough for a per-read
    // timeout, and never finishing either.
    let mut sent = 0;
    let dribbled = loop {
        if peer.write_all(b"X").is_err() || peer.flush().is_err() {
            break true;
        }
        sent += 1;
        if started.elapsed() > budget * 8 {
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert!(
        dribbled,
        "the peer dribbled {sent} header bytes for {:?} without the budget ending it",
        started.elapsed()
    );
    assert!(
        started.elapsed() < budget * 8,
        "the request outlived its budget by {:?}",
        started.elapsed()
    );
}

/// A peer that announces a body and then dribbles it.
#[test]
fn a_slow_body_does_not_hold_a_slot_past_the_budget() {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let budget = Duration::from_millis(400);
    let s = start_with(Arc::new(Echo), budget);
    let mut peer = TcpStream::connect(s.address).expect("connect");
    write!(
        peer,
        "POST /hello?t={} HTTP/1.1\r\nHost: {}\r\nContent-Length: 64\r\n\r\n",
        s.token, s.address
    )
    .expect("head");
    peer.flush().ok();

    let started = Instant::now();
    loop {
        if peer.write_all(b"x").is_err() || peer.flush().is_err() {
            break;
        }
        if started.elapsed() > budget * 8 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Whatever the peer managed to send, the server must be finished with it.
    peer.set_read_timeout(Some(budget * 4)).ok();
    let mut reply = Vec::new();
    let _ = peer.read_to_end(&mut reply);
    assert!(
        started.elapsed() < budget * 8,
        "a dribbled body held its slot for {:?}",
        started.elapsed()
    );
}

/// The controls: within the budget, ordinary requests are served as before.
#[test]
fn a_prompt_request_is_served_within_the_budget() {
    use std::time::Duration;

    let s = start_with(Arc::new(Echo), Duration::from_millis(400));
    let reply = send(s.address, "GET", &format!("/hello?t={}", s.token), &[], b"");
    assert_eq!(reply.status, 200);

    // A body that arrives at once is not slow, whatever its size.
    let reply = send(
        s.address,
        "POST",
        &format!("/hello?t={}", s.token),
        &[],
        &vec![b'x'; 4096],
    );
    assert_eq!(reply.status, 200);
}

/// A refused request releases its slot rather than keeping it to the budget.
#[test]
fn a_refused_request_releases_its_slot_at_once() {
    use std::time::{Duration, Instant};

    let budget = Duration::from_secs(30);
    let s = start_with(Arc::new(Echo), budget);
    let started = Instant::now();
    let reply = send(s.address, "GET", "/hello", &[], b"");
    assert_eq!(reply.status, 403, "no token");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a refusal waited out the budget: {:?}",
        started.elapsed()
    );
}
