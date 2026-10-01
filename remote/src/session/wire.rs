//! The connection underneath a session: TLS carrying frames, and the messages
//! the PC writes into them.

use super::POLL;
use crate::frame;
use lcl_protocol::json::{Node, Object};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Why a connection ended.
pub(super) enum End {
    Closed,
    Refused,
}

/// A TLS connection carrying frames.
pub(super) struct Wire {
    tls: rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
    reader: frame::Reader,
}

impl Wire {
    /// The PC's end of TLS over an accepted connection, the handshake still to
    /// come. Until the device is authenticated it may send one small frame,
    /// hello.
    pub(super) fn accept(tcp: TcpStream, tls: &Arc<rustls::ServerConfig>) -> Option<Wire> {
        let _ = tcp.set_nodelay(true);
        if tcp.set_read_timeout(Some(POLL)).is_err()
            || tcp
                .set_write_timeout(Some(Duration::from_secs(20)))
                .is_err()
        {
            return None;
        }
        let connection = rustls::ServerConnection::new(Arc::clone(tls)).ok()?;
        Some(Wire {
            tls: rustls::StreamOwned::new(connection, tcp),
            reader: frame::Reader::with_limit(frame::MAX_HELLO_FRAME),
        })
    }

    /// Finish the TLS handshake by `deadline`. The fingerprint of the
    /// certificate the device proved it holds the key for, or `None` if the
    /// handshake did not finish.
    pub(super) fn handshake(&mut self, deadline: Instant) -> Option<String> {
        while self.tls.conn.is_handshaking() {
            match self.tls.conn.complete_io(&mut self.tls.sock) {
                Ok(_) => {}
                Err(e) if nothing_yet(&e) => {
                    if Instant::now() > deadline {
                        return None;
                    }
                }
                Err(_) => return None,
            }
        }
        let certificate = self.tls.conn.peer_certificates().and_then(|c| c.first())?;
        Some(crate::identity::fingerprint(certificate.as_ref()))
    }

    /// The device's first frame, if it arrives by `deadline`.
    pub(super) fn first_frame(&mut self, deadline: Instant) -> Option<String> {
        loop {
            match self.poll() {
                Ok(Some(frame)) => return Some(frame),
                Ok(None) if Instant::now() < deadline => continue,
                _ => return None,
            }
        }
    }

    /// The device is authenticated: from here on it may send whole requests.
    pub(super) fn allow_requests(&mut self) {
        self.reader.set_limit(frame::MAX_FRAME);
    }

    pub(super) fn send(&mut self, message: &str) -> Result<(), End> {
        self.tls
            .write_all(&frame::encode(message))
            .map_err(|_| End::Closed)?;
        self.tls.flush().map_err(|_| End::Closed)
    }

    /// The next frame if one arrives within one poll, `Ok(None)` if not.
    pub(super) fn poll(&mut self) -> Result<Option<String>, End> {
        if let Some(frame) = self.reader.next_frame().map_err(|_| End::Refused)? {
            return Ok(Some(frame));
        }
        let mut buffer = [0u8; 16 * 1024];
        match self.tls.read(&mut buffer) {
            Ok(0) => Err(End::Closed),
            Ok(n) => {
                self.reader.feed(&buffer[..n]);
                self.reader.next_frame().map_err(|_| End::Refused)
            }
            Err(e) if nothing_yet(&e) => Ok(None),
            Err(_) => Err(End::Closed),
        }
    }

    /// Tell the device the connection is over.
    pub(super) fn close(&mut self) {
        self.tls.conn.send_close_notify();
        let _ = self.tls.flush();
    }
}

/// Whether a read only ran out of time, with nothing wrong with the connection.
fn nothing_yet(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// `value` as a JSON string.
pub(super) fn text(value: &str) -> String {
    Node::string(value).compact()
}

/// The one message a refused `hello` is answered with.
pub(super) fn error_message(code: &str, message: &str) -> String {
    Object::new()
        .with("type", Node::string("error"))
        .with("code", Node::string(code))
        .with("message", Node::string(message))
        .compact()
}

/// The answer to request `id`: a status and a JSON body.
pub(super) fn response(id: u64, status: u16, body: &str) -> String {
    format!("{{\"type\":\"response\",\"id\":{id},\"status\":{status},\"body\":{body}}}")
}

/// The answer to a request that failed, saying why.
pub(super) fn failure(id: u64, status: u16, message: &str) -> String {
    response(id, status, &error_body(message).compact())
}

/// A body that says what went wrong, to which more can be added.
pub(super) fn error_body(message: impl Into<String>) -> Object {
    Object::new().with("error", Node::string(message))
}
