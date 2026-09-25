//! The running service: accept connections, answer discovery, report status.

use crate::config::Config;
use crate::identity::Identity;
use crate::paths::{self, Paths};
use crate::projects::{Projects, Specs};
use crate::session::{self, Shared};
use lcl_protocol::json::{Node, Object};
use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The most connections served at once.
const MAX_CONNECTIONS: usize = 32;

/// How the service was asked to run.
pub struct Options {
    pub paths: Paths,
    pub specs: Specs,
    pub listen: Option<String>,
    pub port: Option<u16>,
    pub discovery: bool,
}

/// A bound service, not yet serving.
pub struct Service {
    listener: TcpListener,
    shared: Arc<Shared>,
    discovery: Option<UdpSocket>,
}

impl Service {
    /// Load the identity (making it on first start), bind, and get ready.
    pub fn bind(options: Options) -> Result<Service, String> {
        let identity = Identity::load_or_create(&options.paths)?;
        let config = Config::load(&options.paths)?;
        let listen = options.listen.unwrap_or_else(|| config.listen.clone());
        let port = options.port.unwrap_or(config.port);
        let listener = TcpListener::bind((listen.as_str(), port))
            .map_err(|e| format!("could not listen on {listen}:{port}: {e}"))?;
        let discovery = if options.discovery {
            // Not fatal: a PC reached by address or link still works.
            UdpSocket::bind(("0.0.0.0", config.discovery_port)).ok()
        } else {
            None
        };
        let projects = Projects::new(options.specs, &options.paths);
        let shared = Arc::new(Shared::new(options.paths, identity, config, projects)?);
        Ok(Service {
            listener,
            shared,
            discovery,
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("a bound listener has an address")
    }

    pub fn identity(&self) -> &Identity {
        &self.shared.identity
    }

    /// Serve until the process ends.
    pub fn serve(self) -> Result<(), String> {
        let port = self.address().port();
        if let Some(socket) = self.discovery {
            let pc_id = self.shared.identity.pc_id.clone();
            std::thread::spawn(move || answer_discovery(socket, &pc_id, port));
        }
        let status = Arc::clone(&self.shared);
        std::thread::spawn(move || loop {
            write_status(&status, port);
            std::thread::sleep(Duration::from_secs(2));
        });
        let live = Arc::new(AtomicUsize::new(0));
        for stream in self.listener.incoming() {
            let Ok(stream) = stream else { continue };
            if live.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
                continue; // dropped: the device retries
            }
            live.fetch_add(1, Ordering::SeqCst);
            let shared = Arc::clone(&self.shared);
            let live = Arc::clone(&live);
            std::thread::spawn(move || {
                session::serve(stream, shared);
                live.fetch_sub(1, Ordering::SeqCst);
            });
        }
        Ok(())
    }
}

/// Answer `LCL-DISCOVER 1 <pc id>` with `LCL-HERE 1 <pc id> <port>`.
///
/// Discovery only says where to try. Who answers is proved by the TLS
/// connection that follows, which a device refuses unless the PC presents the
/// certificate it pinned; a spoofed answer can waste a connection attempt and
/// nothing more.
fn answer_discovery(socket: UdpSocket, pc_id: &str, port: u16) {
    let expected = format!("LCL-DISCOVER 1 {pc_id}");
    let reply = format!("LCL-HERE 1 {pc_id} {port}");
    let mut buffer = [0u8; 256];
    loop {
        let Ok((n, from)) = socket.recv_from(&mut buffer) else {
            continue;
        };
        if buffer[..n] == *expected.as_bytes() {
            let _ = socket.send_to(reply.as_bytes(), from);
        }
    }
}

/// The status file the command line and the desktop read: whether the
/// service is up, where, and which devices are connected now.
fn write_status(shared: &Shared, port: u16) {
    let live = shared
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let sessions = live.values().map(|l| {
        Object::new()
            .with("device", Node::string(&l.device_id))
            .with("name", Node::string(&l.device_name))
            .with("since", Node::u64(l.since))
            .with("peer", Node::string(&l.peer))
            .into()
    });
    let text = Object::new()
        .with("version", Node::u64(1))
        .with("pid", Node::u64(u64::from(std::process::id())))
        .with("port", Node::u64(u64::from(port)))
        .with("updated", Node::u64(paths::now()))
        .with("fingerprint", Node::string(&shared.identity.fingerprint))
        .with("sessions", Node::array(sessions))
        .pretty();
    let _ = paths::write_private(&shared.paths.state.join("status.json"), text.as_bytes());
}

/// This PC's IPv4 addresses on the networks it routes through, for pairing
/// links. Finding the route sends nothing: a UDP socket is only connected.
pub fn local_addresses() -> Vec<String> {
    let mut out = Vec::new();
    for target in [
        "192.0.2.1:9",
        "10.254.254.254:9",
        "172.31.254.254:9",
        "192.168.254.254:9",
    ] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(target).is_ok() {
            if let Ok(local) = socket.local_addr() {
                let ip = local.ip();
                if !ip.is_loopback() && !ip.is_unspecified() && !out.contains(&ip.to_string()) {
                    out.push(ip.to_string());
                }
            }
        }
    }
    out
}
