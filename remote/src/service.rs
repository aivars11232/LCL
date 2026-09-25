//! The running service: accept connections, answer discovery, report status.

use crate::config::Config;
use crate::identity::Identity;
use crate::paths::{self, Paths};
use crate::projects::{Projects, Specs};
use crate::session::{self, Shared};
use lcl_protocol::json::{Node, Object};
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, TcpListener, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The most connections served at once.
pub const MAX_CONNECTIONS: usize = 32;
/// The most of them that may be unauthenticated at once — still in the TLS
/// handshake or not yet through `hello` — whoever they come from.
pub const MAX_UNAUTHENTICATED: usize = 8;
/// The most unauthenticated connections from any one address at once.
pub const MAX_UNAUTHENTICATED_PER_ADDRESS: usize = 4;

/// The connections being served, counted so that a peer nobody has
/// authenticated gets far less than one that is.
///
/// Every connection holds one [`Slot`] from the moment it is accepted until
/// its thread is done with it, however that ends — refused, timed out, closed
/// or panicking — because the slot is given back when it is dropped. Until the
/// device has proved who it is, the slot also counts against a small allowance
/// for unauthenticated connections, overall and per address, so peers that
/// connect and say nothing can hold a few slots for at most
/// [`crate::session::HELLO_WITHIN`] each and never crowd out paired devices'
/// sessions.
#[derive(Default)]
pub struct Slots {
    counts: Mutex<Counts>,
}

#[derive(Default)]
struct Counts {
    total: usize,
    unauthenticated: usize,
    by_address: BTreeMap<IpAddr, usize>,
}

impl Slots {
    /// A slot for a new, unauthenticated connection from `address`, or
    /// `None` when taking one would exceed a limit.
    pub fn take(self: &Arc<Self>, address: IpAddr) -> Option<Slot> {
        let mut counts = self.counts();
        let from_address = counts.by_address.get(&address).copied().unwrap_or(0);
        if counts.total >= MAX_CONNECTIONS
            || counts.unauthenticated >= MAX_UNAUTHENTICATED
            || from_address >= MAX_UNAUTHENTICATED_PER_ADDRESS
        {
            return None;
        }
        counts.total += 1;
        counts.unauthenticated += 1;
        counts.by_address.insert(address, from_address + 1);
        Some(Slot {
            slots: Arc::clone(self),
            address,
            unauthenticated: true,
        })
    }

    /// How many connections hold a slot, and how many of them are still
    /// unauthenticated.
    pub fn in_use(&self) -> (usize, usize) {
        let counts = self.counts();
        (counts.total, counts.unauthenticated)
    }

    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.counts.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// One connection's place, given back when dropped.
pub struct Slot {
    slots: Arc<Slots>,
    address: IpAddr,
    unauthenticated: bool,
}

impl Slot {
    /// The device has proved who it is: this connection no longer counts
    /// against the unauthenticated allowance.
    pub fn authenticated(&mut self) {
        if self.unauthenticated {
            self.unauthenticated = false;
            self.slots.counts().release_unauthenticated(self.address);
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut counts = self.slots.counts();
        if self.unauthenticated {
            counts.release_unauthenticated(self.address);
        }
        counts.total -= 1;
    }
}

impl Counts {
    fn release_unauthenticated(&mut self, address: IpAddr) {
        self.unauthenticated -= 1;
        match self.by_address.get_mut(&address) {
            Some(n) if *n > 1 => *n -= 1,
            _ => {
                self.by_address.remove(&address);
            }
        }
    }
}

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
        let slots = Arc::new(Slots::default());
        for stream in self.listener.incoming() {
            let Ok(stream) = stream else { continue };
            // No slot, no service: the connection is dropped at once, and
            // a device retries.
            let Some(slot) = stream.peer_addr().ok().and_then(|p| slots.take(p.ip())) else {
                continue;
            };
            let shared = Arc::clone(&self.shared);
            // A thread that cannot be started drops the connection and its
            // slot with it.
            let _ = std::thread::Builder::new().spawn(move || session::serve(stream, shared, slot));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn address(last: u8) -> IpAddr {
        IpAddr::from([192, 0, 2, last])
    }

    #[test]
    fn unauthenticated_connections_get_a_small_allowance_overall_and_per_address() {
        let slots = Arc::new(Slots::default());
        let one: Vec<Slot> = (0..MAX_UNAUTHENTICATED_PER_ADDRESS)
            .map(|_| slots.take(address(1)).unwrap())
            .collect();
        assert!(slots.take(address(1)).is_none(), "one address took more");
        let others: Vec<Slot> = (0..MAX_UNAUTHENTICATED - MAX_UNAUTHENTICATED_PER_ADDRESS)
            .map(|i| slots.take(address(10 + i as u8)).unwrap())
            .collect();
        assert!(
            slots.take(address(99)).is_none(),
            "the allowance overflowed"
        );
        assert_eq!(slots.in_use(), (MAX_UNAUTHENTICATED, MAX_UNAUTHENTICATED));
        drop(one);
        drop(others);
        assert_eq!(slots.in_use(), (0, 0));
    }

    #[test]
    fn authenticated_connections_leave_the_allowance_for_new_ones() {
        let slots = Arc::new(Slots::default());
        let mut sessions = Vec::new();
        while sessions.len() < MAX_CONNECTIONS {
            let mut slot = slots
                .take(address(1))
                .expect("room while every earlier one is authenticated");
            slot.authenticated();
            sessions.push(slot);
        }
        assert_eq!(slots.in_use(), (MAX_CONNECTIONS, 0));
        assert!(
            slots.take(address(2)).is_none(),
            "more than MAX_CONNECTIONS"
        );
        sessions.pop();
        assert!(slots.take(address(2)).is_some());
    }

    #[test]
    fn a_slot_is_given_back_however_its_connection_ends() {
        let slots = Arc::new(Slots::default());
        let mut authenticated = slots.take(address(1)).unwrap();
        authenticated.authenticated();
        let unauthenticated = slots.take(address(1)).unwrap();
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _held = (authenticated, unauthenticated);
            panic!("a session thread fails");
        }));
        assert!(failed.is_err());
        assert_eq!(slots.in_use(), (0, 0), "a failing session kept its slot");
        let again: Vec<Slot> = (0..MAX_UNAUTHENTICATED_PER_ADDRESS)
            .map(|_| slots.take(address(1)).unwrap())
            .collect();
        assert_eq!(again.len(), MAX_UNAUTHENTICATED_PER_ADDRESS);
    }
}
