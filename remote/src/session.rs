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

use crate::config::Config;
use crate::devices::{Device, Refusal, Registry};
use crate::frame;
use crate::identity::Identity;
use crate::pairing::{Pairing, Presented, Refusal as PairingRefusal, PAIRING_VERSION};
use crate::paths::{self, Paths};
use crate::projects::{Project, Projects};
use crate::service::Slot;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use lcl_workspace::http::Request;
use lcl_workspace::{Outcome, Route, Routes};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

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

/// The device that started a run: the only one that may follow it or answer
/// its pauses, cancelling included.
///
/// Run ids are counted per project and are easy to guess, and every paired
/// device can reach every shared project, so knowing an id must not be enough.
/// The record binds the run to the device the PC authenticated when the run
/// was started — its id and the fingerprint of the certificate it proved — and
/// to the very routes the run lives in: run ids start again in routes opened
/// afresh, and a record must never answer for a run it did not see started.
struct Owner {
    device: String,
    fingerprint: String,
    routes: Weak<Routes>,
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
}

impl Shared {
    /// The projects shared at this moment (see [`Projects::current`]). Runs
    /// devices started in a project that is no longer shared are cancelled:
    /// nothing can reach them to answer their pauses any more.
    fn current_projects(&self) -> Vec<Project> {
        let (projects, closed) = self.projects.current(&self.paths);
        if !closed.is_empty() {
            let mut owners = self.owners.lock().unwrap_or_else(|e| e.into_inner());
            owners.retain(|(_, run), owner| {
                let Some(routes) = owner.routes.upgrade() else {
                    return false;
                };
                if !closed.iter().any(|c| Arc::ptr_eq(c, &routes)) {
                    return true;
                }
                let _ = route(
                    &routes,
                    "POST",
                    "/api/answer",
                    &[
                        ("run", run.clone()),
                        ("sequence", "0".into()),
                        ("answer", "cancel".into()),
                    ],
                    "",
                );
                false
            });
        }
        projects
    }
}

/// Why a connection ended.
enum End {
    Closed,
    Refused,
}

/// A TLS connection carrying frames.
struct Wire {
    tls: rustls::StreamOwned<rustls::ServerConnection, TcpStream>,
    reader: frame::Reader,
}

impl Wire {
    fn send(&mut self, message: &str) -> Result<(), End> {
        self.tls
            .write_all(&frame::encode(message))
            .map_err(|_| End::Closed)?;
        self.tls.flush().map_err(|_| End::Closed)
    }

    /// The next frame if one arrives within one poll, `Ok(None)` if not.
    fn poll(&mut self) -> Result<Option<String>, End> {
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
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(_) => Err(End::Closed),
        }
    }
}

fn text(value: &str) -> String {
    Node::string(value).compact()
}

fn error_message(code: &str, message: &str) -> String {
    Object::new()
        .with("type", Node::string("error"))
        .with("code", Node::string(code))
        .with("message", Node::string(message))
        .compact()
}

fn response(id: u64, status: u16, body: &str) -> String {
    format!("{{\"type\":\"response\",\"id\":{id},\"status\":{status},\"body\":{body}}}")
}

fn failure(id: u64, status: u16, message: &str) -> String {
    response(
        id,
        status,
        &Object::new().with("error", Node::string(message)).compact(),
    )
}

/// Serve one accepted TCP connection until it ends. `slot` is its place among
/// the connections served, given back when this returns, however it returns.
pub fn serve(tcp: TcpStream, shared: Arc<Shared>, mut slot: Slot) {
    let peer = tcp
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let _ = tcp.set_nodelay(true);
    if tcp.set_read_timeout(Some(POLL)).is_err()
        || tcp
            .set_write_timeout(Some(Duration::from_secs(20)))
            .is_err()
    {
        return;
    }
    let Ok(connection) = rustls::ServerConnection::new(Arc::clone(&shared.tls)) else {
        return;
    };
    // Until the device is authenticated it may send one small frame, hello.
    let mut wire = Wire {
        tls: rustls::StreamOwned::new(connection, tcp),
        reader: frame::Reader::with_limit(frame::MAX_HELLO_FRAME),
    };

    // The handshake, bounded.
    let deadline = Instant::now() + HELLO_WITHIN;
    while wire.tls.conn.is_handshaking() {
        match wire.tls.conn.complete_io(&mut wire.tls.sock) {
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() > deadline {
                    return;
                }
            }
            Err(_) => return,
        }
    }
    let Some(certificate) = wire.tls.conn.peer_certificates().and_then(|c| c.first()) else {
        return;
    };
    let fingerprint = crate::identity::fingerprint(certificate.as_ref());

    // hello, bounded.
    let hello = loop {
        match wire.poll() {
            Ok(Some(frame)) => break frame,
            Ok(None) if Instant::now() < deadline => continue,
            _ => return,
        }
    };
    let Some(device) = admit(&mut wire, &shared, &hello, &fingerprint) else {
        wire.tls.conn.send_close_notify();
        let _ = wire.tls.flush();
        return;
    };
    slot.authenticated();
    wire.reader.set_limit(frame::MAX_FRAME);

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
    wire.tls.conn.send_close_notify();
    let _ = wire.tls.flush();
}

/// Decide who a device is from its `hello`, and tell it.
fn admit(wire: &mut Wire, shared: &Shared, hello: &str, fingerprint: &str) -> Option<Device> {
    let refuse = |wire: &mut Wire, code: &str, message: &str| {
        let _ = wire.send(&error_message(code, message));
        None
    };
    let Ok(hello) = lcl_spec::json::parse(hello) else {
        return refuse(wire, "malformed", "the first message is not JSON");
    };
    let field = |k: &str| hello.get(k).and_then(Json::as_str);
    if field("type") != Some("hello") {
        return refuse(wire, "malformed", "the first message must be hello");
    }
    if field("protocol") != Some(PROTOCOL) {
        return refuse(
            wire,
            "unsupported_protocol",
            &format!("this PC speaks {PROTOCOL} only"),
        );
    }
    let now = paths::now();
    let device = match field("intent") {
        Some("pair") => {
            // Only the flow in which the PC approves each new device. A hello
            // from before it — or one that leaves the version out — is refused
            // here, whatever client sent it.
            match hello.get("pairing_version").and_then(Json::as_u64) {
                Some(PAIRING_VERSION) => {}
                Some(version) if version > PAIRING_VERSION => {
                    return refuse(
                        wire,
                        "unsupported_pairing_version",
                        &format!("this PC pairs with pairing version {PAIRING_VERSION}; update LCL on the PC"),
                    )
                }
                _ => {
                    return refuse(
                        wire,
                        "pairing_upgrade_required",
                        "this device uses the older pairing flow, which this PC no longer accepts; update LCL for Android and scan a new QR code",
                    )
                }
            }
            let (Some(code), Some(name)) = (field("code"), field("name")) else {
                return refuse(wire, "malformed", "pairing needs a code and a device name");
            };
            let presented = shared.pairing.present(
                code,
                fingerprint,
                name,
                &shared.identity.fingerprint,
                &shared.registry,
                PROTOCOL,
                now,
            );
            match presented {
                // Not trusted: the device is told what to show and to ask
                // again, and the connection ends. It holds no session, and no
                // connection is kept open while the person decides.
                Ok(Presented::Pending(candidate)) => {
                    let _ = wire.send(
                        &Object::new()
                            .with("type", Node::string("pairing_pending"))
                            .with("request", Node::string(&candidate.request))
                            .with("verification", Node::string(&candidate.verification))
                            .with("expires", Node::u64(candidate.expires))
                            .with("pc", pc_node(&shared.identity))
                            .compact(),
                    );
                    return None;
                }
                Ok(Presented::Paired(device)) => {
                    let paired = Object::new()
                        .with("type", Node::string("paired"))
                        .with("device", device_node(&device))
                        .with("pc", pc_node(&shared.identity))
                        .compact();
                    wire.send(&paired).ok()?;
                    device
                }
                Err(PairingRefusal::Denied) => {
                    return refuse(wire, "pairing_denied", &PairingRefusal::Denied.to_string())
                }
                Err(PairingRefusal::Busy) => {
                    return refuse(wire, "pairing_busy", &PairingRefusal::Busy.to_string())
                }
                Err(PairingRefusal::Unreadable(e)) => return refuse(wire, "unavailable", &e),
                Err(refusal) => return refuse(wire, "pairing_refused", &refusal.to_string()),
            }
        }
        Some("connect") => match shared.registry.authorize(fingerprint) {
            Ok(device) => {
                // A device may say which record it believes it is; it must be
                // the one its certificate belongs to.
                if let Some(claimed) = field("device") {
                    if claimed != device.id {
                        return refuse(
                            wire,
                            "identity_mismatch",
                            "this certificate belongs to another device",
                        );
                    }
                }
                device
            }
            Err(Refusal::Unknown) => {
                return refuse(
                    wire,
                    "not_paired",
                    "this device is not paired with this PC; scan a new QR code",
                )
            }
            Err(Refusal::Revoked) => {
                return refuse(
                    wire,
                    "revoked",
                    "this PC revoked this device; pair it again with a new QR code",
                )
            }
            Err(Refusal::Unreadable(e)) => return refuse(wire, "unavailable", &e),
        },
        _ => {
            return refuse(
                wire,
                "malformed",
                "hello must say whether it is to pair or to connect",
            )
        }
    };
    let welcome = Object::new()
        .with("type", Node::string("welcome"))
        .with("protocol", Node::string(PROTOCOL))
        .with("pc", pc_node(&shared.identity))
        .with("device", device_node(&device))
        .compact();
    wire.send(&welcome).ok()?;
    Some(device)
}

fn device_node(device: &Device) -> Node {
    Object::new()
        .with("id", Node::string(&device.id))
        .with("name", Node::string(&device.name))
        .into()
}

fn pc_node(identity: &Identity) -> Node {
    Object::new()
        .with("id", Node::string(&identity.pc_id))
        .with("name", Node::string(&identity.name))
        .with("fingerprint", Node::string(&identity.fingerprint))
        .into()
}

/// A run this session started, and how much of it the device has been sent.
struct Watching {
    project: String,
    run: String,
    routes: Arc<Routes>,
    sent: usize,
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

    /// Answer one request. `None` for a message that is not one, which closes
    /// the connection: a peer that speaks the protocol wrongly is not guessed at.
    fn handle(&mut self, frame: &str) -> Option<String> {
        let message = lcl_spec::json::parse(frame).ok()?;
        if message.get("type").and_then(Json::as_str) != Some("request") {
            return None;
        }
        let id = message.get("id").and_then(Json::as_u64)?;
        let op = message.get("op").and_then(Json::as_str)?;
        let param = |k: &str| message.get(k).and_then(Json::as_str);
        Some(match op {
            "ping" => response(
                id,
                200,
                &Object::new()
                    .with("pong", Node::u64(paths::now()))
                    .compact(),
            ),
            "about" => response(id, 200, &self.about()),
            // The device forgot this PC and asks to be trusted no longer. It
            // can only ever end its own trust: the device is the one this
            // connection's certificate proves, never one the message names.
            "unpair" => match self.shared.registry.authorize(&self.fingerprint) {
                Ok(device) => match self.shared.registry.revoke(&device.id, paths::now()) {
                    Ok(_) => response(id, 200, "{\"unpaired\":true}"),
                    Err(detail) => failure(id, 500, &detail),
                },
                Err(_) => failure(id, 403, "this device is not trusted here"),
            },
            "projects" => {
                let projects = self.projects();
                let items = projects.iter().map(|p| {
                    Object::new()
                        .with("id", Node::string(&p.id))
                        .with("name", Node::string(&p.name))
                        .with("root", Node::string(p.root.display().to_string()))
                        .with("default", Node::Bool(p.default))
                        .into()
                });
                response(
                    id,
                    200,
                    &Object::new().with("projects", Node::array(items)).compact(),
                )
            }
            _ => {
                let opened = match param("project") {
                    Some(project) => self.shared.projects.open(&self.shared.paths, project),
                    None => Ok(None),
                };
                let (project, routes) = match opened {
                    Ok(Some(opened)) => opened,
                    Ok(None) => {
                        return Some(failure(id, 404, "no such project is shared by this PC"))
                    }
                    Err(e) => return Some(failure(id, 500, &e)),
                };
                self.project_op(id, op, &project, &routes, &message)
            }
        })
    }

    fn projects(&self) -> Vec<Project> {
        self.shared.current_projects()
    }

    fn project_op(
        &mut self,
        id: u64,
        op: &str,
        project: &Project,
        routes: &Arc<Routes>,
        message: &Json,
    ) -> String {
        let param = |k: &str| message.get(k).and_then(Json::as_str);
        let document = param("document");
        let need = |value: Option<&str>, what: &str| {
            value
                .map(str::to_string)
                .ok_or(format!("{what} is required"))
        };
        let call = |method: &str, path: &str, query: &[(&str, String)], body: &str| {
            route(routes, method, path, query, body)
        };
        let result: Result<(u16, String), String> = (|| {
            Ok(match op {
                "session" => call("GET", "/api/session", &[], ""),
                "tree" => call("GET", "/api/documents", &[], ""),
                "settings" => call("GET", "/api/settings", &[], ""),
                "open" => {
                    let doc = need(document, "a document")?;
                    let (status, body) = call("GET", "/api/document", &[("id", doc.clone())], "");
                    if status == 200 {
                        let digest = lcl_spec::json::parse(&body).ok().and_then(|b| {
                            b.get("digest").and_then(Json::as_str).map(str::to_string)
                        });
                        self.watched.insert((project.id.clone(), doc), digest);
                    }
                    (status, body)
                }
                "close" => {
                    self.watched
                        .remove(&(project.id.clone(), need(document, "a document")?));
                    (200, "{}".to_string())
                }
                "save" => {
                    let doc = need(document, "a document")?;
                    let base = need(param("base"), "the revision the edit started from (base)")?;
                    let body = need(param("text"), "the text")?;
                    self.save(routes, project, &doc, &base, &body)
                }
                "create" => {
                    let name = need(param("name"), "a name")?;
                    let (status, reply) = call(
                        "POST",
                        "/api/document",
                        &[("id", name)],
                        &need(param("text"), "the text")?,
                    );
                    if status == 200 {
                        if let Ok(created) = lcl_spec::json::parse(&reply) {
                            if let (Some(doc), Some(digest)) = (
                                created.get("id").and_then(Json::as_str),
                                created.get("digest").and_then(Json::as_str),
                            ) {
                                self.watched.insert(
                                    (project.id.clone(), doc.to_string()),
                                    Some(digest.to_string()),
                                );
                            }
                        }
                    }
                    (status, reply)
                }
                "delete" => {
                    let doc = need(document, "a document")?;
                    let (status, reply) = call(
                        "DELETE",
                        "/api/document",
                        &[
                            ("id", doc.clone()),
                            ("digest", need(param("digest"), "the digest")?),
                        ],
                        "",
                    );
                    if status == 200 {
                        self.watched.remove(&(project.id.clone(), doc));
                    }
                    (status, reply)
                }
                "tokens" | "check" | "validate" | "inspect" => {
                    let path = format!("/api/{op}");
                    call(
                        "POST",
                        &path,
                        &[("id", need(document, "a document")?)],
                        &need(param("text"), "the text")?,
                    )
                }
                "run" => self.run(routes, project, message)?,
                // Resume a run's events after a reconnect, from the first one
                // the device has not seen. The log is append-only, so events
                // from that index on are exactly what it missed.
                "follow" => {
                    let run = need(param("run"), "a run")?;
                    let from = message.get("from").and_then(Json::as_u64).unwrap_or(0) as usize;
                    if routes.run_events(&run, 0).is_none() {
                        return Err(format!("no run {run} is known to this project any more"));
                    }
                    if !self.owns(project, routes, &run) {
                        return Ok(not_yours(&run));
                    }
                    if !self
                        .runs
                        .iter()
                        .any(|w| w.project == project.id && w.run == run)
                    {
                        self.runs.push(Watching {
                            project: project.id.clone(),
                            run,
                            routes: Arc::clone(routes),
                            sent: from,
                        });
                    }
                    (200, "{\"following\":true}".to_string())
                }
                "answer" => {
                    let run = need(param("run"), "a run")?;
                    // An answer to a run this device did not start is refused
                    // before it reaches the run: continue, deny and cancel alike.
                    if routes.run_events(&run, 0).is_some() && !self.owns(project, routes, &run) {
                        return Ok(not_yours(&run));
                    }
                    call(
                        "POST",
                        "/api/answer",
                        &[
                            ("run", run),
                            (
                                "sequence",
                                message
                                    .get("sequence")
                                    .and_then(Json::as_u64)
                                    .map(|n| n.to_string())
                                    .unwrap_or_default(),
                            ),
                            ("answer", need(param("answer"), "an answer")?),
                        ],
                        "",
                    )
                }
                other => return Err(format!("unknown operation {other}")),
            })
        })();
        match result {
            Ok((status, body)) => response(id, status, &body),
            Err(detail) => failure(id, 400, &detail),
        }
    }

    /// Save only over the revision the device started from.
    fn save(
        &mut self,
        routes: &Routes,
        project: &Project,
        doc: &str,
        base: &str,
        text: &str,
    ) -> (u16, String) {
        use lcl_workspace::{DocumentError as D, WorkspaceError as W};
        let workspace = routes.workspace();
        match workspace.save_expecting(doc, text, base) {
            Ok(saved) => {
                self.watched.insert(
                    (project.id.clone(), doc.to_string()),
                    Some(saved.digest.clone()),
                );
                let body = Object::new()
                    .with("id", Node::string(&saved.id))
                    .with("digest", Node::string(&saved.digest))
                    .with("bytes", Node::usize(saved.text.len()))
                    .with(
                        "final_line_feed_added",
                        Node::Bool(saved.text.len() != text.len()),
                    )
                    .compact();
                (200, body)
            }
            Err(W::Document(D::Changed(_))) => {
                // Stop, and hand back what is on disk so the device can
                // reconcile. Nothing was written.
                let mut body = Object::new()
                    .with("error", Node::string(format!("{doc} changed on this PC since the revision this edit started from; nothing was saved")))
                    .with("conflict", Node::Bool(true));
                if let Ok(current) = workspace.read(doc) {
                    body = body
                        .with("text", Node::string(current.text))
                        .with("digest", Node::string(current.digest));
                }
                (409, body.compact())
            }
            Err(W::Document(D::NotFound(_))) => (
                404,
                Object::new()
                    .with(
                        "error",
                        Node::string(format!("{doc} no longer exists on this PC")),
                    )
                    .compact(),
            ),
            Err(W::Document(D::Superseded(_))) => (
                409,
                Object::new()
                    .with(
                        "error",
                        Node::string(format!(
                            "{doc} was saved again while this save was in flight"
                        )),
                    )
                    .with("conflict", Node::Bool(true))
                    .compact(),
            ),
            Err(e) => (
                422,
                Object::new()
                    .with("error", Node::string(e.to_string()))
                    .compact(),
            ),
        }
    }

    /// Start a run through the workspace's own run route, and follow it.
    fn run(
        &mut self,
        routes: &Arc<Routes>,
        project: &Project,
        message: &Json,
    ) -> Result<(u16, String), String> {
        let doc = message
            .get("document")
            .and_then(Json::as_str)
            .ok_or("a document is required")?;
        let text = message
            .get("text")
            .and_then(Json::as_str)
            .ok_or("the text is required")?;
        let lines = |value: Option<&Json>| -> Result<String, String> {
            match value {
                None | Some(Json::Null) => Ok(String::new()),
                Some(v) => {
                    let items = v
                        .as_array()
                        .ok_or("grants and inputs are lists of strings")?;
                    let strings: Option<Vec<&str>> = items.iter().map(Json::as_str).collect();
                    let strings = strings.ok_or("grants and inputs are lists of strings")?;
                    if strings.iter().any(|s| s.contains('\n')) {
                        return Err("a grant or input may not contain a line break".into());
                    }
                    Ok(strings.join("\n"))
                }
            }
        };
        let grants = message.get("grants");
        let mut query = vec![("id", doc.to_string())];
        for (key, name) in [
            ("allow_read", "read"),
            ("allow_write", "write"),
            ("allow_program", "program"),
            ("allow_host", "host"),
        ] {
            let value = lines(grants.and_then(|g| g.get(name)))?;
            if !value.is_empty() {
                query.push((key, value));
            }
        }
        let inputs = lines(message.get("inputs"))?;
        if !inputs.is_empty() {
            query.push(("input", inputs));
        }
        // A remote run pauses before every effect, always. The PC enforces it
        // rather than trusting the device to ask: whatever `break_effects` a
        // request carries is ignored, so no paired device can have an effect
        // happen that the person holding it did not approve at its pause.
        query.push(("break_effects", "1".into()));
        if message.get("break_operations").and_then(Json::as_bool) == Some(true) {
            query.push(("break_operations", "1".into()));
        }
        let (status, body) = route(routes, "POST", "/api/run", &query, text);
        if status == 200 {
            if let Some(run) = lcl_spec::json::parse(&body)
                .ok()
                .and_then(|b| b.get("run").and_then(Json::as_str).map(str::to_string))
            {
                let mut owners = self.shared.owners.lock().unwrap_or_else(|e| e.into_inner());
                // Records of runs their routes no longer keep answer nothing.
                owners.retain(|(_, run), owner| {
                    owner
                        .routes
                        .upgrade()
                        .is_some_and(|routes| routes.run_events(run, 0).is_some())
                });
                owners.insert(
                    (project.id.clone(), run.clone()),
                    Owner {
                        device: self.device.clone(),
                        fingerprint: self.fingerprint.clone(),
                        routes: Arc::downgrade(routes),
                    },
                );
                drop(owners);
                self.runs.push(Watching {
                    project: project.id.clone(),
                    run,
                    routes: Arc::clone(routes),
                    sent: 0,
                });
            }
        }
        Ok((status, body))
    }

    /// Whether this device started `run`, in these very routes.
    fn owns(&self, project: &Project, routes: &Arc<Routes>, run: &str) -> bool {
        let owners = self.shared.owners.lock().unwrap_or_else(|e| e.into_inner());
        owners
            .get(&(project.id.clone(), run.to_string()))
            .is_some_and(|owner| {
                owner.device == self.device
                    && owner.fingerprint == self.fingerprint
                    && Weak::ptr_eq(&owner.routes, &Arc::downgrade(routes))
            })
    }

    fn forward_runs(&mut self, wire: &mut Wire) -> Result<(), End> {
        // Routes were closed somewhere since this session last looked: some
        // run followed here may be in them, and nothing more of it may pass.
        if self.shared.projects.closings() != self.closings_seen {
            self.forget_unshared(wire)?;
        }
        let mut index = 0;
        while index < self.runs.len() {
            let watching = &mut self.runs[index];
            let (events, finished) = watching
                .routes
                .run_events(&watching.run, watching.sent)
                .unwrap_or((Vec::new(), true));
            for (name, payload) in &events {
                let data = if payload.trim().is_empty() {
                    "{}"
                } else {
                    payload.as_str()
                };
                wire.send(&format!(
                    "{{\"type\":\"event\",\"event\":\"run\",\"project\":{},\"run\":{},\"name\":{},\"data\":{}}}",
                    text(&watching.project),
                    text(&watching.run),
                    text(name),
                    data
                ))?;
            }
            watching.sent += events.len();
            if finished && events.is_empty() {
                wire.send(&format!(
                    "{{\"type\":\"event\",\"event\":\"run\",\"project\":{},\"run\":{},\"name\":\"end\",\"data\":{{}}}}",
                    text(&watching.project),
                    text(&watching.run)
                ))?;
                self.runs.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(())
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
        let mut index = 0;
        while index < self.runs.len() {
            let watching = &self.runs[index];
            if shared.contains(&watching.project)
                && self
                    .shared
                    .projects
                    .is_open(&watching.project, &watching.routes)
            {
                index += 1;
                continue;
            }
            let gone = self.runs.remove(index);
            let stopped = Object::new()
                .with(
                    "error",
                    Node::string(
                        "this project is no longer shared by this PC, so the run was stopped",
                    ),
                )
                .compact();
            for (name, data) in [("failed", stopped.as_str()), ("end", "{}")] {
                wire.send(&format!(
                    "{{\"type\":\"event\",\"event\":\"run\",\"project\":{},\"run\":{},\"name\":{},\"data\":{}}}",
                    text(&gone.project),
                    text(&gone.run),
                    text(name),
                    data
                ))?;
            }
        }
        Ok(())
    }

    /// Tell the device about open documents that changed on this PC.
    fn check_documents(&mut self, wire: &mut Wire) -> Result<(), End> {
        for ((project, doc), known) in self.watched.iter_mut() {
            let Ok(Some((_, routes))) = self.shared.projects.open(&self.shared.paths, project)
            else {
                continue;
            };
            let now = routes.workspace().read(doc).ok().map(|d| d.digest);
            if &now != known {
                *known = now.clone();
                wire.send(
                    &Object::new()
                        .with("type", Node::string("event"))
                        .with("event", Node::string("document_changed"))
                        .with("project", Node::string(project))
                        .with("document", Node::string(doc))
                        .with("digest", Node::optional(now))
                        .compact(),
                )?;
            }
        }
        Ok(())
    }

    /// Who this PC is and what it runs, from the engine itself.
    fn about(&self) -> String {
        let mut cached = self.shared.about.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(about) = cached.as_ref() {
            return about.clone();
        }
        let spec = |path: Option<&std::path::Path>, localized: bool| -> Node {
            let Some(path) = path else { return Node::Null };
            let engine = if localized {
                lcl_protocol::Engine::open_localized(path, &[])
            } else {
                lcl_protocol::Engine::open(path)
            };
            match engine {
                Ok(engine) => {
                    let record = engine.spec_record();
                    Object::new()
                        .with("formal_version", Node::string(&record.formal_version))
                        .with("identity_digest", Node::string(&record.identity_digest))
                        .with("authority", Node::string(&record.authority))
                        .into()
                }
                Err(e) => Object::new()
                    .with("error", Node::string(e.to_string()))
                    .into(),
            }
        };
        let specs = self.shared.projects.specs();
        let about = Object::new()
            .with("pc", pc_node(&self.shared.identity))
            .with("service", Node::string(format!("lcl-remote {VERSION}")))
            .with("protocol", Node::string(PROTOCOL))
            .with("engine_protocol", Node::string(lcl_protocol::PROTOCOL))
            .with("core", spec(Some(&specs.core), false))
            .with("localized", spec(specs.localized.as_deref(), true))
            .compact();
        *cached = Some(about.clone());
        about
    }
}

/// The refusal a device gets for a run another device started. It is the same
/// whether the run is another device's or was never this one's, so it tells
/// the asker nothing about whose it is.
fn not_yours(run: &str) -> (u16, String) {
    (
        403,
        Object::new()
            .with(
                "error",
                Node::string(format!(
                    "run {run} was not started by this device; only the device that started a run can follow or answer it"
                )),
            )
            .compact(),
    )
}

/// Call one workspace route in process. The route table is the workspace's
/// own; nothing here reaches past it.
fn route(
    routes: &Routes,
    method: &str,
    path: &str,
    query: &[(&str, String)],
    body: &str,
) -> (u16, String) {
    let request = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
    };
    match routes.handle(&request) {
        Outcome::Reply(response) => (
            response.status,
            String::from_utf8(response.body).unwrap_or_else(|_| "{}".into()),
        ),
        Outcome::Stream(_) => (
            400,
            "{\"error\":\"streams are not available remotely\"}".into(),
        ),
    }
}
