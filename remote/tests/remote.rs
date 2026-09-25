//! The remote service end to end: a real service, real TLS 1.3 with real
//! certificates on both sides, and a client that speaks the protocol the
//! Android app speaks.

use lcl_remote::identity;
use lcl_remote::pairing::Pairing;
use lcl_remote::paths::{self, Paths};
use lcl_remote::projects::Specs;
use lcl_remote::service::{Options, Service};
use lcl_spec::json::Json;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .unwrap()
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A disposable home for one PC.
struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Home {
        let root = std::env::temp_dir().join(format!(
            "lcl-remote-test-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Home(root)
    }
    fn paths(&self) -> Paths {
        Paths::under(&self.0)
    }
    /// The default workspace every PC here shares.
    fn workspace(&self) -> PathBuf {
        self.paths().builtin_workspace
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A running PC.
struct Pc {
    address: SocketAddr,
    fingerprint: String,
}

fn start(home: &Home) -> Pc {
    let workspace = home.workspace();
    std::fs::create_dir_all(&workspace).unwrap();
    let service = Service::bind(Options {
        paths: home.paths(),
        specs: Specs {
            core: repository().join("canonical/LCL_Core_0.1.0"),
            localized: Some(repository().join("canonical/LCL_Core_0.2.0")),
        },
        listen: Some("127.0.0.1".into()),
        port: Some(0),
        discovery: false,
    })
    .expect("the service binds");
    let pc = Pc {
        address: service.address(),
        fingerprint: service.identity().fingerprint.clone(),
    };
    std::thread::spawn(move || service.serve());
    pc
}

/// A device: its own key and certificate, as a phone's keystore would hold.
#[derive(Clone)]
struct Device {
    certificate: Vec<u8>,
    key: Vec<u8>,
}

fn device() -> Device {
    let (key, certificate) =
        identity::new_certificate("test device", &ring::rand::SystemRandom::new()).unwrap();
    Device { certificate, key }
}

/// One connection, speaking lcl.remote/1.
struct Client {
    tls: rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
    reader: lcl_remote::frame::Reader,
    next: u64,
    /// Events that arrived while waiting for a response.
    events: Vec<Json>,
}

fn connect(pc: &Pc, device: &Device, pinned: &str) -> Result<Client, String> {
    let config =
        lcl_remote::tls::client_config(pinned, device.certificate.clone(), device.key.clone())?;
    let connection = rustls::ClientConnection::new(config, lcl_remote::tls::server_name())
        .map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect(pc.address).map_err(|e| e.to_string())?;
    tcp.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut tls = rustls::StreamOwned::new(connection, tcp);
    let deadline = Instant::now() + Duration::from_secs(10);
    while tls.conn.is_handshaking() {
        match tls.conn.complete_io(&mut tls.sock) {
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if Instant::now() > deadline {
                    return Err("handshake timed out".into());
                }
            }
            Err(e) => return Err(format!("handshake failed: {e}")),
        }
    }
    Ok(Client {
        tls,
        reader: Default::default(),
        next: 1,
        events: Vec::new(),
    })
}

impl Client {
    fn send_raw(&mut self, bytes: &[u8]) {
        self.tls.write_all(bytes).unwrap();
        self.tls.flush().unwrap();
    }

    fn send(&mut self, message: &str) {
        self.send_raw(&lcl_remote::frame::encode(message));
    }

    /// The next message, or `None` if the connection closed or nothing came.
    fn receive(&mut self, within: Duration) -> Option<Json> {
        let deadline = Instant::now() + within;
        loop {
            if let Ok(Some(frame)) = self.reader.next_frame() {
                return Some(lcl_spec::json::parse(&frame).expect("the PC sends JSON"));
            }
            if Instant::now() > deadline {
                return None;
            }
            let mut buffer = [0u8; 16384];
            match self.tls.read(&mut buffer) {
                Ok(0) => return None,
                Ok(n) => self.reader.feed(&buffer[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return None,
            }
        }
    }

    fn hello(&mut self, fields: &str) -> Json {
        self.send(&format!(
            "{{\"type\":\"hello\",\"protocol\":\"lcl.remote/1\",{fields}}}"
        ));
        self.receive(Duration::from_secs(10))
            .expect("an answer to hello")
    }

    /// One request; events that arrive first are kept for later.
    fn request(&mut self, op: &str, fields: &str) -> (u64, Json) {
        let id = self.next;
        self.next += 1;
        let extra = if fields.is_empty() {
            String::new()
        } else {
            format!(",{fields}")
        };
        self.send(&format!(
            "{{\"type\":\"request\",\"id\":{id},\"op\":\"{op}\"{extra}}}"
        ));
        loop {
            let message = self.receive(Duration::from_secs(20)).expect("a response");
            if message.get("type").and_then(Json::as_str) == Some("response") {
                assert_eq!(message.get("id").and_then(Json::as_u64), Some(id));
                let status = message.get("status").and_then(Json::as_u64).unwrap();
                return (status, message.get("body").cloned().unwrap());
            }
            self.events.push(message);
        }
    }

    /// Wait for an event matching `wanted`, looking at kept ones first.
    fn event(&mut self, within: Duration, wanted: impl Fn(&Json) -> bool) -> Option<Json> {
        if let Some(i) = self.events.iter().position(&wanted) {
            return Some(self.events.remove(i));
        }
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            let message = self.receive(deadline - Instant::now())?;
            if wanted(&message) {
                return Some(message);
            }
            self.events.push(message);
        }
        None
    }
}

fn s(value: &Json, key: &str) -> String {
    value
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("no {key} in {value:?}"))
        .to_string()
}

fn json_string(text: &str) -> String {
    lcl_protocol::json::Node::string(text).compact()
}

fn code(home: &Home, ttl: u64) -> String {
    Pairing::new(&home.paths())
        .create(paths::now(), ttl)
        .unwrap()
        .1
}

/// Pair a device and return its authenticated connection and device id.
fn pair(home: &Home, pc: &Pc, device: &Device, name: &str) -> (Client, String) {
    let code = code(home, 300);
    let mut client = connect(pc, device, &pc.fingerprint).expect("TLS to the pinned PC");
    let paired = client.hello(&format!(
        "\"intent\":\"pair\",\"code\":\"{code}\",\"name\":{}",
        json_string(name)
    ));
    assert_eq!(s(&paired, "type"), "paired", "{paired:?}");
    let welcome = client.receive(Duration::from_secs(5)).unwrap();
    assert_eq!(s(&welcome, "type"), "welcome");
    let id = s(welcome.get("device").unwrap(), "id");
    (client, id)
}

fn reconnect(pc: &Pc, device: &Device) -> (Client, Json) {
    let mut client = connect(pc, device, &pc.fingerprint).expect("TLS to the pinned PC");
    let answer = client.hello("\"intent\":\"connect\"");
    (client, answer)
}

fn project(client: &mut Client) -> String {
    let (status, body) = client.request("projects", "");
    assert_eq!(status, 200, "{body:?}");
    let projects = body.get("projects").and_then(Json::as_array).unwrap();
    s(&projects[0], "id")
}

const VALID: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: remote.valid\n    NAME: \"Remote\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 1\n";

// ---------------------------------------------------------------------------
// Pairing and trust
// ---------------------------------------------------------------------------

#[test]
fn a_valid_code_pairs_and_trust_survives_reconnects_and_a_pc_restart() {
    let home = Home::new("persist");
    let pc = start(&home);
    let phone = device();
    let (client, device_id) = pair(&home, &pc, &phone, "Test phone");
    drop(client); // the app closes: no Forget, no unpairing

    let (_client, welcome) = reconnect(&pc, &phone);
    assert_eq!(
        s(&welcome, "type"),
        "welcome",
        "reconnecting needed a new QR code: {welcome:?}"
    );
    assert_eq!(s(welcome.get("device").unwrap(), "id"), device_id);

    // The PC restarts: same identity files, a new process and a new port.
    let restarted = start(&home);
    assert_eq!(
        restarted.fingerprint, pc.fingerprint,
        "the PC's identity changed across a restart"
    );
    let (_client, welcome) = reconnect(&restarted, &phone);
    assert_eq!(
        s(&welcome, "type"),
        "welcome",
        "a PC restart broke the pairing: {welcome:?}"
    );
}

#[test]
fn a_pairing_code_names_the_port_the_service_really_listens_on() {
    // `serve --port` (here: a port the system chose) must reach new QR codes,
    // or phones would be sent to the configured default.
    let home = Home::new("port");
    let pc = start(&home);
    let status = home.paths().state.join("status.json");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !status.exists() {
        assert!(Instant::now() < deadline, "the service wrote no status");
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lcl-remote"))
        .args(["pair", "--json"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", home.0.join("config"))
        .env("XDG_STATE_HOME", home.0.join("state"))
        .env("XDG_DATA_HOME", home.0.join("data"))
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("no network address") {
        eprintln!("skipped: this machine has no network address to put in a code");
        return;
    }
    assert!(output.status.success(), "{stderr}");
    let reply = lcl_spec::json::parse(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    let addresses = reply.get("addresses").and_then(Json::as_array).unwrap();
    assert!(!addresses.is_empty());
    let port = format!(":{}", pc.address.port());
    for address in addresses {
        let address = address.as_str().unwrap();
        assert!(address.ends_with(&port), "{address} is not on {port}");
    }
}

#[test]
fn an_expired_or_reused_code_is_refused() {
    let home = Home::new("codes");
    let pc = start(&home);
    let expired = code(&home, 0);
    let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
    let answer = client.hello(&format!(
        "\"intent\":\"pair\",\"code\":\"{expired}\",\"name\":\"late\""
    ));
    assert_eq!(s(&answer, "code"), "pairing_refused");
    assert!(s(&answer, "message").contains("expired"));

    let used = code(&home, 300);
    let mut first = connect(&pc, &device(), &pc.fingerprint).unwrap();
    assert_eq!(
        s(
            &first.hello(&format!(
                "\"intent\":\"pair\",\"code\":\"{used}\",\"name\":\"first\""
            )),
            "type"
        ),
        "paired"
    );
    let mut second = connect(&pc, &device(), &pc.fingerprint).unwrap();
    let answer = second.hello(&format!(
        "\"intent\":\"pair\",\"code\":\"{used}\",\"name\":\"replay\""
    ));
    assert_eq!(
        s(&answer, "code"),
        "pairing_refused",
        "a QR code paired twice"
    );
    assert!(s(&answer, "message").contains("already used"));
}

#[test]
fn the_wrong_pc_is_refused_before_anything_is_said() {
    let home = Home::new("wrongpc");
    let pc = start(&home);
    let impostor_pin = "0".repeat(64);
    assert!(
        connect(&pc, &device(), &impostor_pin).is_err(),
        "a PC with another certificate was accepted"
    );
}

#[test]
fn malformed_or_unsupported_hellos_are_refused_and_closed() {
    let home = Home::new("hello");
    let pc = start(&home);
    for (hello, expected) in [
        (
            "{\"type\":\"hello\",\"protocol\":\"lcl.remote/2\",\"intent\":\"connect\"}",
            "unsupported_protocol",
        ),
        (
            "{\"type\":\"hello\",\"protocol\":\"lcl.remote/1\"}",
            "malformed",
        ),
        ("not json", "malformed"),
        (
            "{\"type\":\"request\",\"id\":1,\"op\":\"projects\"}",
            "malformed",
        ),
    ] {
        let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
        client.send(hello);
        let answer = client.receive(Duration::from_secs(5)).expect("an error");
        assert_eq!(s(&answer, "code"), expected, "{hello}");
        assert!(
            client.receive(Duration::from_secs(2)).is_none(),
            "the connection stayed open after {hello}"
        );
    }
    // An oversized frame closes the connection before anything is allocated.
    let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
    client.send_raw(&u32::MAX.to_be_bytes());
    assert!(client.receive(Duration::from_secs(3)).is_none());
}

#[test]
fn an_unpaired_device_gets_nothing() {
    let home = Home::new("unpaired");
    let pc = start(&home);
    let (_c, answer) = reconnect(&pc, &device());
    assert_eq!(s(&answer, "code"), "not_paired");
}

#[test]
fn revoking_one_device_disconnects_it_for_good_and_leaves_the_other() {
    let home = Home::new("revoke");
    let pc = start(&home);
    let (a, b) = (device(), device());
    let (mut live_a, a_id) = pair(&home, &pc, &a, "Phone A");
    let (_live_b, _b_id) = pair(&home, &pc, &b, "Phone B");

    lcl_remote::devices::Registry::new(&home.paths())
        .revoke(&a_id, paths::now())
        .unwrap();
    let event = live_a.event(Duration::from_secs(5), |m| {
        m.get("event").and_then(Json::as_str) == Some("revoked")
    });
    assert!(
        event.is_some(),
        "a revoked device's live connection was not ended"
    );
    assert!(live_a.receive(Duration::from_secs(2)).is_none());

    let (_c, answer) = reconnect(&pc, &a);
    assert_eq!(
        s(&answer, "code"),
        "revoked",
        "a revoked device reconnected"
    );
    let (_c, answer) = reconnect(&pc, &b);
    assert_eq!(s(&answer, "type"), "welcome", "revoking A revoked B");

    // Only a new QR code trusts A again.
    let (_c, again) = pair(&home, &pc, &a, "Phone A again");
    assert_ne!(again, a_id);
}

#[test]
fn a_device_that_forgets_the_pc_ends_its_own_trust_and_nobody_else_s() {
    let home = Home::new("unpair");
    let pc = start(&home);
    let (a, b) = (device(), device());
    let (mut live_a, a_id) = pair(&home, &pc, &a, "Phone A");
    let (_live_b, b_id) = pair(&home, &pc, &b, "Phone B");

    // A names B in the message; only the certificate decides who is asking.
    let (status, body) = live_a.request("unpair", &format!("\"device\":\"{b_id}\""));
    assert_eq!(status, 200, "{body:?}");
    let event = live_a.event(Duration::from_secs(5), |m| {
        m.get("event").and_then(Json::as_str) == Some("revoked")
    });
    assert!(event.is_some(), "the unpaired device stayed connected");

    let listed = lcl_remote::devices::Registry::new(&home.paths())
        .list()
        .unwrap();
    let revoked: Vec<&str> = listed
        .iter()
        .filter(|d| d.revoked_at.is_some())
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(revoked, vec![a_id.as_str()]);
    let (_c, answer) = reconnect(&pc, &a);
    assert_eq!(s(&answer, "code"), "revoked");
    let (_c, answer) = reconnect(&pc, &b);
    assert_eq!(s(&answer, "type"), "welcome", "A's unpair ended B's trust");
}

#[test]
fn a_device_cannot_claim_to_be_another() {
    let home = Home::new("impersonate");
    let pc = start(&home);
    let (a, b) = (device(), device());
    let (_a, _a_id) = pair(&home, &pc, &a, "A");
    let (_b, b_id) = pair(&home, &pc, &b, "B");
    let mut client = connect(&pc, &a, &pc.fingerprint).unwrap();
    let answer = client.hello(&format!("\"intent\":\"connect\",\"device\":\"{b_id}\""));
    assert_eq!(s(&answer, "code"), "identity_mismatch");
}

// ---------------------------------------------------------------------------
// Projects and documents
// ---------------------------------------------------------------------------

#[test]
fn projects_trees_and_both_endings_are_served() {
    let home = Home::new("tree");
    std::fs::create_dir_all(home.workspace()).unwrap();
    std::fs::write(home.workspace().join("classic.lcl"), VALID).unwrap();
    std::fs::write(home.workspace().join("shared.lcl.txt"), VALID).unwrap();
    std::fs::write(home.workspace().join("notes.txt"), "plain text\n").unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "tree");
    let project = project(&mut client);
    let (status, tree) = client.request("tree", &format!("\"project\":\"{project}\""));
    assert_eq!(status, 200);
    let ids: Vec<String> = tree
        .get("entries")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|e| s(e, "id"))
        .collect();
    assert_eq!(
        ids,
        vec!["classic.lcl", "shared.lcl.txt"],
        "ordinary .txt must not be listed"
    );
    for id in ["classic.lcl", "shared.lcl.txt"] {
        let (status, doc) = client.request(
            "open",
            &format!("\"project\":\"{project}\",\"document\":\"{id}\""),
        );
        assert_eq!(status, 200);
        assert_eq!(s(&doc, "text"), VALID);
    }
    // Creating: the PC's default ending, and an explicit one kept.
    for (name, expected) in [("fresh", "fresh.lcl"), ("chosen.lcl.txt", "chosen.lcl.txt")] {
        let (status, created) = client.request(
            "create",
            &format!(
                "\"project\":\"{project}\",\"name\":\"{name}\",\"text\":{}",
                json_string(VALID)
            ),
        );
        assert_eq!(status, 200, "{created:?}");
        assert_eq!(s(&created, "id"), expected);
        assert!(home.workspace().join(expected).is_file());
    }
}

#[test]
fn a_save_lands_only_on_the_revision_it_started_from() {
    let home = Home::new("save");
    std::fs::create_dir_all(home.workspace()).unwrap();
    std::fs::write(home.workspace().join("doc.lcl"), VALID).unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "save");
    let project = project(&mut client);
    let (_, opened) = client.request(
        "open",
        &format!("\"project\":\"{project}\",\"document\":\"doc.lcl\""),
    );
    let base = s(&opened, "digest");

    let edited = VALID.replace("VALUE: 1", "VALUE: 2");
    let (status, saved) = client.request(
        "save",
        &format!(
            "\"project\":\"{project}\",\"document\":\"doc.lcl\",\"base\":\"{base}\",\"text\":{}",
            json_string(&edited)
        ),
    );
    assert_eq!(status, 200, "{saved:?}");
    assert_eq!(
        std::fs::read_to_string(home.workspace().join("doc.lcl")).unwrap(),
        edited
    );

    // A second save from the old revision is refused and writes nothing.
    let stale = VALID.replace("VALUE: 1", "VALUE: 3");
    let (status, conflict) = client.request(
        "save",
        &format!(
            "\"project\":\"{project}\",\"document\":\"doc.lcl\",\"base\":\"{base}\",\"text\":{}",
            json_string(&stale)
        ),
    );
    assert_eq!(status, 409, "{conflict:?}");
    assert_eq!(conflict.get("conflict").and_then(Json::as_bool), Some(true));
    assert_eq!(
        s(&conflict, "text"),
        edited,
        "the conflict must hand back what is on the PC"
    );
    assert_eq!(
        std::fs::read_to_string(home.workspace().join("doc.lcl")).unwrap(),
        edited,
        "a stale save overwrote newer content"
    );
}

#[test]
fn an_edit_made_on_the_pc_reaches_the_device() {
    let home = Home::new("pcedit");
    std::fs::create_dir_all(home.workspace()).unwrap();
    std::fs::write(home.workspace().join("doc.lcl"), VALID).unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "watch");
    let project = project(&mut client);
    client.request(
        "open",
        &format!("\"project\":\"{project}\",\"document\":\"doc.lcl\""),
    );
    let changed = VALID.replace("VALUE: 1", "VALUE: 9");
    std::fs::write(home.workspace().join("doc.lcl"), &changed).unwrap();
    let event = client
        .event(Duration::from_secs(5), |m| {
            m.get("event").and_then(Json::as_str) == Some("document_changed")
        })
        .expect("the device was not told about the edit on the PC");
    assert_eq!(s(&event, "document"), "doc.lcl");
    assert_eq!(
        s(&event, "digest"),
        lcl_spec::sha256::hex_digest(changed.as_bytes())
    );
}

#[test]
fn paths_outside_the_project_and_unknown_operations_are_refused() {
    let home = Home::new("paths");
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "paths");
    let project = project(&mut client);
    for doc in ["../../../etc/passwd", "/etc/passwd", "../outside.lcl"] {
        let (status, body) = client.request(
            "open",
            &format!(
                "\"project\":\"{project}\",\"document\":{}",
                json_string(doc)
            ),
        );
        assert!(status >= 400, "{doc} was served: {body:?}");
    }
    let (status, _) = client.request(
        "create",
        &format!("\"project\":\"{project}\",\"name\":\"../escape\",\"text\":\"x\""),
    );
    assert!(status >= 400);
    for op in ["shell", "exec", "read_file", "folder"] {
        let (status, body) = client.request(op, &format!("\"project\":\"{project}\""));
        assert_eq!(status, 400, "{op}: {body:?}");
        assert!(s(&body, "error").contains("unknown operation"));
    }
    let (status, _) = client.request("tree", "\"project\":\"not-a-project\"");
    assert_eq!(status, 404);
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

#[test]
fn check_validate_and_inspect_are_the_engines_reports() {
    let home = Home::new("engine");
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "engine");
    let project = project(&mut client);
    for op in ["check", "validate", "inspect"] {
        let (status, report) = client.request(
            op,
            &format!(
                "\"project\":\"{project}\",\"document\":\"x.lcl\",\"text\":{}",
                json_string(VALID)
            ),
        );
        assert_eq!(status, 200, "{op}: {report:?}");
        assert_eq!(s(&report, "outcome"), "accepted", "{op}");
    }
    let broken = VALID.replace("SPECIFICATION:", "SPECIFICATON:");
    let (_, report) = client.request(
        "check",
        &format!(
            "\"project\":\"{project}\",\"document\":\"x.lcl\",\"text\":{}",
            json_string(&broken)
        ),
    );
    assert_ne!(s(&report, "outcome"), "accepted");
    assert!(!report
        .get("diagnostics")
        .and_then(Json::as_array)
        .unwrap()
        .is_empty());
    let (status, tokens) = client.request(
        "tokens",
        &format!(
            "\"project\":\"{project}\",\"document\":\"x.lcl\",\"text\":{}",
            json_string(VALID)
        ),
    );
    assert_eq!(status, 200);
    assert!(!tokens
        .get("tokens")
        .and_then(Json::as_array)
        .unwrap()
        .is_empty());
}

/// The users manual's backup example, pointed at `folder`.
fn backup_document(folder: &Path) -> String {
    std::fs::read_to_string(repository().join("users_manual/solutions/11/todo_write.lcl"))
        .unwrap()
        .replace("/tmp/lcl-manual/todo", &folder.display().to_string())
}

fn run_until_end(client: &mut Client, answer: &str) -> (Vec<String>, Json) {
    let mut names = Vec::new();
    let mut report = Json::Null;
    loop {
        let event = client
            .event(Duration::from_secs(20), |m| {
                m.get("event").and_then(Json::as_str) == Some("run")
            })
            .expect("the run went quiet");
        let name = s(&event, "name");
        names.push(name.clone());
        let data = event.get("data").cloned().unwrap();
        match name.as_str() {
            "paused" => {
                let project = s(&event, "project");
                let run = s(&event, "run");
                let sequence = data.get("sequence").and_then(Json::as_u64).unwrap();
                let (status, _) = client.request("answer", &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"sequence\":{sequence},\"answer\":\"{answer}\""));
                assert_eq!(status, 200);
            }
            "report" => report = data,
            "end" => return (names, report),
            _ => {}
        }
    }
}

#[test]
fn a_run_pauses_before_each_effect_and_the_device_decides() {
    let home = Home::new("run");
    let folder = home.0.join("todo");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("todo.txt"), "Buy milk\n").unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "runner");
    let project = project(&mut client);
    let grants = format!(
        "{{\"read\":[{0}],\"write\":[{0}]}}",
        json_string(&folder.display().to_string())
    );
    let text = json_string(&backup_document(&folder));

    let (status, started) = client.request("run", &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants}"));
    assert_eq!(status, 200, "{started:?}");
    let (names, report) = run_until_end(&mut client, "continue");
    assert!(
        names.iter().filter(|n| *n == "paused").count() >= 2,
        "no pause before the effects: {names:?}"
    );
    let completion = report.get("completion").expect("a completion");
    assert_eq!(
        s(completion, "terminal_status"),
        "status.succeeded",
        "{report:?}"
    );
    assert!(folder.join("todo_backup.txt").is_file());
    assert!(std::fs::read_to_string(folder.join("todo.txt"))
        .unwrap()
        .contains("Learn LCL"));
}

#[test]
fn a_run_survives_the_connection_dropping_and_is_followed_after_reconnecting() {
    let home = Home::new("follow");
    let folder = home.0.join("todo");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("todo.txt"), "Buy milk\n").unwrap();
    let pc = start(&home);
    let phone = device();
    let (mut client, _) = pair(&home, &pc, &phone, "runner");
    let project = project(&mut client);
    let grants = format!(
        "{{\"read\":[{0}],\"write\":[{0}]}}",
        json_string(&folder.display().to_string())
    );
    let text = json_string(&backup_document(&folder));
    let (_, started) = client.request("run", &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants}"));
    let run = s(&started, "run");

    // Count events up to the first pause, then lose the connection there.
    let mut seen = 0u64;
    loop {
        let event = client
            .event(Duration::from_secs(20), |m| {
                m.get("event").and_then(Json::as_str) == Some("run")
            })
            .expect("run events");
        seen += 1;
        if s(&event, "name") == "paused" {
            break;
        }
    }
    drop(client);

    // Back, without a QR code: follow the run from the first event missed.
    let (mut client, welcome) = reconnect(&pc, &phone);
    assert_eq!(s(&welcome, "type"), "welcome");
    let (status, body) = client.request(
        "follow",
        &format!(
            "\"project\":\"{project}\",\"run\":\"{run}\",\"from\":{}",
            seen - 1
        ),
    );
    assert_eq!(status, 200, "{body:?}");
    let (names, report) = run_until_end(&mut client, "continue");
    assert_eq!(
        names.first().map(String::as_str),
        Some("paused"),
        "the pending pause was not replayed: {names:?}"
    );
    let completion = report.get("completion").expect("a completion");
    assert_eq!(s(completion, "terminal_status"), "status.succeeded");
}

#[test]
fn a_device_cannot_get_an_effect_the_pc_did_not_permit() {
    let home = Home::new("nogrant");
    let folder = home.0.join("todo");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("todo.txt"), "Buy milk\n").unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "runner");
    let project = project(&mut client);
    let text = json_string(&backup_document(&folder));

    // No host grant: even with every pause approved, nothing is written.
    let (status, _) = client.request(
        "run",
        &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text}"),
    );
    assert_eq!(status, 200);
    let (_, report) = run_until_end(&mut client, "continue");
    let terminal = report.get("completion").map(|c| s(c, "terminal_status"));
    assert_ne!(
        terminal.as_deref(),
        Some("status.succeeded"),
        "an effect happened without a host grant"
    );
    assert!(!folder.join("todo_backup.txt").exists());

    // Granted, but the device denies at the pause: still nothing is written.
    let grants = format!(
        "{{\"read\":[{0}],\"write\":[{0}]}}",
        json_string(&folder.display().to_string())
    );
    client.request("run", &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants}"));
    let (_, report) = run_until_end(&mut client, "deny");
    let terminal = report.get("completion").map(|c| s(c, "terminal_status"));
    assert_ne!(terminal.as_deref(), Some("status.succeeded"));
    assert!(
        !folder.join("todo_backup.txt").exists(),
        "a denied effect happened"
    );
}
