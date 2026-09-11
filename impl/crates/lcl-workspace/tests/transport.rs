//! The transport and its three gates.

mod common;

use common::{send, send_raw, start, Echo};
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
