//! One connection from a device: who it is, then what it may ask.
//!
//! ## The shape of a connection
//!
//! 1. **TLS 1.3**, both ends presenting certificates (see [`crate::tls`]).
//! 2. **`hello`**, the device's first message, naming the protocol version it
//!    speaks and what it came to do:
//!    * `"intent": "pair"` with `"pairing_version": 2`, a `code` from a QR
//!      code and a device `name`. This only **asks** to be trusted: the PC
//!      records a pending request bound to the code and to the fingerprint of
//!      the certificate the device just proved it holds the key for, answers
//!      `pairing_pending` with a verification code, and closes the
//!      connection. The device asks again every few seconds with the same
//!      key and code. Once the person at the PC approved exactly that request
//!      (`lcl-remote approve`, or the workspace's Settings), the next ask is
//!      answered `paired`, the device is trusted and the code is spent (see
//!      [`crate::pairing`]). A `hello` without `pairing_version` 2 is refused
//!      (`pairing_upgrade_required`).
//!    * `"intent": "connect"`, for a device already paired. It is let in only
//!      if its certificate's fingerprint belongs to a device that is paired
//!      and not revoked.
//!
//!    Anything else — another protocol version, a malformed message, an
//!    unknown or revoked certificate, a used, expired or denied code — is
//!    answered with one `error` and the connection is closed.
//! 3. **Requests**, `{"type":"request","id":N,"op":...}`, each answered by one
//!    `response` with the same id. Every operation is one of a fixed list;
//!    there is no operation that runs a command, reads a path the device
//!    names, or reaches anything but the shared projects. Document and engine
//!    operations are the desktop workspace's own routes, called in process,
//!    so a device is refused exactly what the workspace would refuse, and a
//!    run is authorized and permitted exactly as a run from the workspace is.
//!    A remote run also always pauses before every effect — the PC sets that,
//!    not the device — and only the device that started a run may follow it
//!    or answer its pauses.
//!    `unpair` is the one operation about trust itself: a device that forgets
//!    this PC asks to be revoked, and can only ever revoke itself.
//! 4. **Events** the PC sends on its own: a run's progress and its pauses, a
//!    document changing on disk, and the device being revoked, after which
//!    the connection closes.
//!
//! Every second the session checks that its device is still trusted, so a
//! revocation from the command line or the desktop ends a live connection.
//!
//! ## Where each part is
//!
//! This file is a connection from accept to close: what all connections
//! share, and the session that answers one device. `wire` is TLS, the frames
//! and the messages written into them; `admission` is `hello`; `operations`
//! is the list of requests; `documents` and `runs` are what a session holds
//! open and follows; `legacy_tree` is the `tree` answer older apps ask for.

use crate::config::Config;
use crate::devices::{Refusal, Registry};
use crate::identity::Identity;
use crate::pairing::Pairing;
use crate::paths::{self, Paths};
use crate::projects::{Project, Projects};
use crate::service::Slot;
use std::collections::BTreeMap;
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod admission;
mod documents;
mod legacy_tree;
mod operations;
mod runs;
mod wire;

use admission::admit;
use runs::{Owner, Watching};
use wire::{text, End, Wire};

/// The remote protocol this build speaks. Not the LCL language version, and
/// not the engine protocol: those travel separately in `welcome` and `about`.
pub const PROTOCOL: &str = "lcl.remote/1";

/// This service's own version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a device may take to finish TLS and say `hello`.
pub const HELLO_WITHIN: Duration = Duration::from_secs(10);
/// How long a connection may stay silent; devices ping well within it.
const IDLE_LIMIT: Duration = Duration::from_secs(90);
/// How often a read gives way to everything else a session watches.
const POLL: Duration = Duration::from_millis(40);
/// How often trust and watched documents are checked.
const RECHECK: Duration = Duration::from_secs(1);

/// What every connection shares.
pub struct Shared {
    pub paths: Paths,
    pub identity: Identity,
    pub config: Config,
    pub registry: Registry,
    pub pairing: Pairing,
    pub projects: Projects,
    pub tls: Arc<rustls::ServerConfig>,
    /// Connections that are authenticated now, for the status file.
    pub live: Mutex<BTreeMap<u64, Live>>,
    /// Who started each remote run, by (project, run).
    owners: Mutex<BTreeMap<(String, String), Owner>>,
    next: AtomicU64,
    about: Mutex<Option<String>>,
}

/// One authenticated connection, as the status file reports it.
#[derive(Debug, Clone)]
pub struct Live {
    pub device_id: String,
    pub device_name: String,
    pub since: u64,
    pub peer: String,
}

impl Shared {
    pub fn new(
        paths: Paths,
        identity: Identity,
        config: Config,
        projects: Projects,
    ) -> Result<Shared, String> {
        let tls = crate::tls::server_config(&identity)?;
        Ok(Shared {
            registry: Registry::new(&paths),
            pairing: Pairing::new(&paths),
            paths,
            identity,
            config,
            projects,
            tls,
            live: Mutex::new(BTreeMap::new()),
            owners: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            about: Mutex::new(None),
        })
    }

    /// The projects shared at this moment (see [`Projects::current`]). Runs
    /// devices started in a project that is no longer shared are cancelled:
    /// nothing can reach them to answer their pauses any more.
    fn current_projects(&self) -> Vec<Project> {
        let (projects, closed) = self.projects.current(&self.paths);
        if !closed.is_empty() {
            self.cancel_runs_in(&closed);
        }
        projects
    }
}

/// Serve one accepted TCP connection until it ends. `slot` is its place among
/// the connections served, given back when this returns, however it returns.
pub fn serve(tcp: TcpStream, shared: Arc<Shared>, mut slot: Slot) {
    let peer = tcp
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let Some(mut wire) = Wire::accept(tcp, &shared.tls) else {
        return;
    };

    // The handshake and hello, bounded together.
    let deadline = Instant::now() + HELLO_WITHIN;
    let Some(fingerprint) = wire.handshake(deadline) else {
        return;
    };
    let Some(hello) = wire.first_frame(deadline) else {
        return;
    };
    let Some(device) = admit(&mut wire, &shared, &hello, &fingerprint) else {
        wire.close();
        return;
    };
    slot.authenticated();
    wire.allow_requests();

    let key = shared.next.fetch_add(1, Ordering::Relaxed);
    shared
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            key,
            Live {
                device_id: device.id.clone(),
                device_name: device.name.clone(),
                since: paths::now(),
                peer,
            },
        );
    let _ = shared.registry.touch(&device.id, paths::now());
    let mut session = Session {
        shared: Arc::clone(&shared),
        device: device.id.clone(),
        fingerprint,
        watched: BTreeMap::new(),
        runs: Vec::new(),
        closings_seen: shared.projects.closings(),
    };
    let _ = session.serve(&mut wire);
    shared
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&key);
    let _ = shared.registry.touch(&device.id, paths::now());
    wire.close();
}

struct Session {
    shared: Arc<Shared>,
    /// The device this connection authenticated as, and its certificate.
    device: String,
    fingerprint: String,
    /// Open documents: (project, document) → the digest last known on disk.
    watched: BTreeMap<(String, String), Option<String>>,
    runs: Vec<Watching>,
    /// [`Projects::closings`] when this session last let go of what belongs
    /// to projects no longer shared.
    closings_seen: u64,
}

impl Session {
    /// Answer the device's requests until the connection ends. Between them
    /// the device is sent what its runs say, and every second it is checked
    /// that it is still trusted, and told of open documents that changed.
    fn serve(&mut self, wire: &mut Wire) -> Result<(), End> {
        let mut heard = Instant::now();
        let mut checked = Instant::now();
        loop {
            if let Some(frame) = wire.poll()? {
                heard = Instant::now();
                let reply = self.handle(&frame).ok_or(End::Refused)?;
                wire.send(&reply)?;
            }
            self.forward_runs(wire)?;
            if checked.elapsed() >= RECHECK {
                checked = Instant::now();
                match self.shared.registry.authorize(&self.fingerprint) {
                    Ok(_) => {}
                    Err(refusal) => {
                        let reason = match refusal {
                            Refusal::Revoked => "revoked",
                            _ => "unavailable",
                        };
                        let _ = wire.send(&format!(
                            "{{\"type\":\"event\",\"event\":{}}}",
                            text(reason)
                        ));
                        return Err(End::Refused);
                    }
                }
                self.forget_unshared(wire)?;
                self.check_documents(wire)?;
            }
            if heard.elapsed() > IDLE_LIMIT {
                return Err(End::Closed);
            }
        }
    }

    fn projects(&self) -> Vec<Project> {
        self.shared.current_projects()
    }

    /// Let go of what belongs to a project this PC no longer shares: its open
    /// documents are no longer watched, and each run the device was following
    /// there — stopped by [`Shared::current_projects`] — is reported ended,
    /// and nothing more of it is sent.
    fn forget_unshared(&mut self, wire: &mut Wire) -> Result<(), End> {
        let seen = self.shared.projects.closings();
        let shared: Vec<String> = self.projects().into_iter().map(|p| p.id).collect();
        self.closings_seen = seen;
        self.watched
            .retain(|(project, _), _| shared.contains(project));
        self.end_unshared_runs(&shared, wire)
    }
}
