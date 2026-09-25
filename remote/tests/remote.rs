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
#[derive(Clone)]
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

    /// Whether the PC closes this connection within `within`, rather than
    /// keeping it open. Whatever arrives meanwhile is kept in the reader.
    fn closed_within(&mut self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        let mut buffer = [0u8; 16384];
        while Instant::now() < deadline {
            match self.tls.read(&mut buffer) {
                Ok(0) => return true,
                Ok(n) => self.reader.feed(&buffer[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return true,
            }
        }
        false
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

/// A device asks to pair with `code`, as the app does when Pair is pressed:
/// the PC's answer. A pending answer ends the connection.
fn ask(pc: &Pc, device: &Device, code: &str, name: &str) -> Json {
    let mut client = connect(pc, device, &pc.fingerprint).expect("TLS to the pinned PC");
    client.hello(&format!(
        "\"intent\":\"pair\",\"pairing_version\":2,\"code\":\"{code}\",\"name\":{}",
        json_string(name)
    ))
}

/// The person at the PC approves one request, through the PC's own pairing
/// state (what `lcl-remote approve` does).
fn approve(home: &Home, request: &str) {
    Pairing::new(&home.paths())
        .approve(request, paths::now())
        .unwrap();
}

/// The device asks again after its request was approved: it is paired, and
/// this connection is its first session.
fn finish(pc: &Pc, device: &Device, code: &str, name: &str) -> (Client, String) {
    let mut client = connect(pc, device, &pc.fingerprint).expect("TLS to the pinned PC");
    let paired = client.hello(&format!(
        "\"intent\":\"pair\",\"pairing_version\":2,\"code\":\"{code}\",\"name\":{}",
        json_string(name)
    ));
    assert_eq!(s(&paired, "type"), "paired", "{paired:?}");
    let welcome = client.receive(Duration::from_secs(5)).unwrap();
    assert_eq!(s(&welcome, "type"), "welcome");
    let id = s(welcome.get("device").unwrap(), "id");
    (client, id)
}

/// Pair a device as a person does — it asks, the PC approves that request,
/// it asks again — and return its authenticated connection and device id.
fn pair(home: &Home, pc: &Pc, device: &Device, name: &str) -> (Client, String) {
    let code = code(home, 300);
    let pending = ask(pc, device, &code, name);
    assert_eq!(s(&pending, "type"), "pairing_pending", "{pending:?}");
    approve(home, &s(&pending, "request"));
    finish(pc, device, &code, name)
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
    // Plain pairing text, not a link: nothing a camera app would open.
    let payload = s(&reply, "payload");
    assert!(payload.starts_with("LCLPAIR|v=2&"), "{payload}");
    assert!(!payload.contains("://"), "{payload}");
    assert!(reply.get("link").is_none(), "a link is still offered");
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
    let answer = ask(&pc, &device(), &expired, "late");
    assert_eq!(s(&answer, "code"), "pairing_refused");
    assert!(s(&answer, "message").contains("expired"));

    let used = code(&home, 300);
    let first = device();
    let pending = ask(&pc, &first, &used, "first");
    approve(&home, &s(&pending, "request"));
    finish(&pc, &first, &used, "first");
    let answer = ask(&pc, &device(), &used, "replay");
    assert_eq!(
        s(&answer, "code"),
        "pairing_refused",
        "a QR code paired twice"
    );
    assert!(s(&answer, "message").contains("already used"));
}

// ---------------------------------------------------------------------------
// A11: a pairing code asks; only the PC's approval trusts
// ---------------------------------------------------------------------------

/// The devices the PC trusts, straight from its registry.
fn trusted(home: &Home) -> Vec<lcl_remote::devices::Device> {
    lcl_remote::devices::Registry::new(&home.paths())
        .list()
        .unwrap()
        .into_iter()
        .filter(|d| !d.is_revoked())
        .collect()
}

#[test]
fn a11_r1_a_valid_code_alone_does_not_create_trust() {
    let home = Home::new("a11-r1");
    let pc = start(&home);
    let code = code(&home, 300);
    let stranger = device();
    let mut client = connect(&pc, &stranger, &pc.fingerprint).unwrap();
    let answer = client.hello(&format!(
        "\"intent\":\"pair\",\"pairing_version\":2,\"code\":\"{code}\",\"name\":\"whoever holds the code\""
    ));
    assert_eq!(
        s(&answer, "type"),
        "pairing_pending",
        "a valid code alone was answered with {answer:?}"
    );
    assert!(
        client.closed_within(Duration::from_secs(3)),
        "a pending candidate kept its connection"
    );
    assert!(
        trusted(&home).is_empty(),
        "a valid code alone made a trusted device: {:?}",
        trusted(&home)
    );
    let (_c, refused) = reconnect(&pc, &stranger);
    assert_eq!(
        s(&refused, "code"),
        "not_paired",
        "a candidate nobody approved connected: {refused:?}"
    );
    // Asking again changes nothing: still the same pending request.
    let again = ask(&pc, &stranger, &code, "whoever holds the code");
    assert_eq!(s(&again, "type"), "pairing_pending");
    assert_eq!(s(&again, "request"), s(&answer, "request"));
    assert!(trusted(&home).is_empty());
}

/// What the phone computes and shows for its own request: the PC's
/// fingerprint, the code it scanned, and its own certificate.
fn phone_verification(pc: &Pc, code: &str, device: &Device) -> String {
    lcl_remote::pairing::verification(
        &pc.fingerprint,
        &lcl_spec::sha256::hex_digest(code.as_bytes()),
        &identity::fingerprint(&device.certificate),
    )
}

/// `lcl-remote pending --json`, as the person at the PC reads it.
fn pending_requests(home: &Home) -> Vec<Json> {
    let output = cli(home, &["pending", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let listed = lcl_spec::json::parse(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    listed
        .get("requests")
        .and_then(Json::as_array)
        .unwrap()
        .to_vec()
}

fn cli_ok(home: &Home, args: &[&str]) -> String {
    let output = cli(home, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn cli_refused(home: &Home, args: &[&str]) -> String {
    let output = cli(home, args);
    assert!(!output.status.success(), "{args:?} was accepted");
    String::from_utf8(output.stderr).unwrap()
}

#[test]
fn a11_r2_approval_trusts_only_the_exact_certificate() {
    let home = Home::new("a11-r2");
    let pc = start(&home);
    let code = code(&home, 300);
    let (a, b) = (device(), device());
    let pending = ask(&pc, &a, &code, "Phone A");
    let request = s(&pending, "request");
    assert_eq!(
        s(&pending, "verification"),
        phone_verification(&pc, &code, &a)
    );
    let said = cli_ok(&home, &["approve", &request]);
    assert!(said.contains("approved Phone A"), "{said}");
    assert!(trusted(&home).is_empty(), "approval alone made a record");

    // Another key with the same code, naming A's request: nothing.
    let mut client = connect(&pc, &b, &pc.fingerprint).unwrap();
    let answer = client.hello(&format!(
        "\"intent\":\"pair\",\"pairing_version\":2,\"code\":\"{code}\",\"name\":\"Phone A\",\"request\":\"{request}\""
    ));
    assert_eq!(s(&answer, "code"), "pairing_refused", "{answer:?}");
    // A with another code of this PC: a new request, not A's approval.
    let other_code = self::code(&home, 300);
    let elsewhere = ask(&pc, &a, &other_code, "Phone A");
    assert_eq!(s(&elsewhere, "type"), "pairing_pending", "{elsewhere:?}");
    assert_ne!(s(&elsewhere, "request"), request);
    // A at another PC, with this PC's code: that PC never issued it.
    let other_home = Home::new("a11-r2-other");
    let other_pc = start(&other_home);
    let there = ask(&other_pc, &a, &code, "Phone A");
    assert_eq!(s(&there, "code"), "pairing_refused", "{there:?}");
    assert!(trusted(&other_home).is_empty());

    // A itself, with the approved code: trusted, and only A.
    let (_live, id) = finish(&pc, &a, &code, "Phone A");
    let devices = trusted(&home);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, id);
    assert_eq!(
        devices[0].fingerprint,
        identity::fingerprint(&a.certificate)
    );
    let (_c, welcome) = reconnect(&pc, &a);
    assert_eq!(s(&welcome, "type"), "welcome");
    let (_c, refused) = reconnect(&pc, &b);
    assert_eq!(s(&refused, "code"), "not_paired");
    // The request that paired is not pending any more.
    let still: Vec<String> = pending_requests(&home)
        .iter()
        .map(|r| s(r, "request"))
        .collect();
    assert!(!still.contains(&request), "{still:?}");
}

#[test]
fn a11_r3_a_stolen_code_used_first_still_trusts_nobody_but_the_approved_phone() {
    let home = Home::new("a11-r3");
    let pc = start(&home);
    let code = code(&home, 300);
    let (attacker, phone) = (device(), device());

    // The attacker asks first; the phone second; and both at once, too.
    let first = ask(&pc, &attacker, &code, "Pixel 8");
    assert_eq!(s(&first, "type"), "pairing_pending", "{first:?}");
    let racing: Vec<_> = [attacker.clone(), phone.clone()]
        .into_iter()
        .map(|d| {
            let (pc, code) = (pc.clone(), code.clone());
            std::thread::spawn(move || ask(&pc, &d, &code, "Pixel 8"))
        })
        .collect();
    for answer in racing {
        let answer = answer.join().unwrap();
        assert_eq!(s(&answer, "type"), "pairing_pending", "{answer:?}");
    }
    assert!(trusted(&home).is_empty(), "a race made a trusted device");
    let (_c, refused) = reconnect(&pc, &attacker);
    assert_eq!(s(&refused, "code"), "not_paired");

    // The PC lists both — the same name, told apart by their verification
    // codes — and never the code itself.
    let listed = pending_requests(&home);
    assert_eq!(listed.len(), 2, "{listed:?}");
    let output = cli_ok(&home, &["pending", "--json"]) + &cli_ok(&home, &["pending"]);
    assert!(!output.contains(&code), "pending shows the pairing code");
    let mine = phone_verification(&pc, &code, &phone);
    assert_ne!(mine, phone_verification(&pc, &code, &attacker));
    let chosen: Vec<&Json> = listed
        .iter()
        .filter(|r| s(r, "verification") == mine)
        .collect();
    assert_eq!(chosen.len(), 1);
    assert_eq!(
        s(chosen[0], "fingerprint"),
        identity::fingerprint(&phone.certificate)
    );
    cli_ok(&home, &["approve", &s(chosen[0], "request")]);

    let (_live, id) = finish(&pc, &phone, &code, "Pixel 8");
    let again = ask(&pc, &attacker, &code, "Pixel 8");
    assert_eq!(s(&again, "code"), "pairing_refused", "{again:?}");
    let (_c, refused) = reconnect(&pc, &attacker);
    assert_eq!(s(&refused, "code"), "not_paired");
    let late = ask(&pc, &device(), &code, "Pixel 8");
    assert_eq!(s(&late, "code"), "pairing_refused", "{late:?}");
    let devices = trusted(&home);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].id, id);
    assert!(pending_requests(&home).is_empty());
}

#[test]
fn a11_r4_denying_a_stranger_leaves_the_code_to_the_real_phone() {
    let home = Home::new("a11-r4");
    let pc = start(&home);
    let code = code(&home, 300);
    let (attacker, phone) = (device(), device());
    let stranger = ask(&pc, &attacker, &code, "Phone");
    let said = cli_ok(&home, &["deny", &s(&stranger, "request")]);
    assert!(said.contains("denied"), "{said}");
    let again = ask(&pc, &attacker, &code, "Phone");
    assert_eq!(s(&again, "code"), "pairing_denied", "{again:?}");
    assert!(pending_requests(&home).is_empty());

    let mine = ask(&pc, &phone, &code, "Phone");
    assert_eq!(s(&mine, "type"), "pairing_pending", "{mine:?}");
    cli_ok(&home, &["approve", &s(&mine, "request")]);
    let (_live, id) = finish(&pc, &phone, &code, "Phone");
    assert_eq!(trusted(&home).len(), 1);
    assert_eq!(trusted(&home)[0].id, id);
    assert!(cli_refused(&home, &["approve", &s(&stranger, "request")]).contains("denied"));
}

#[test]
fn a11_r5_one_code_never_approves_two_devices() {
    let home = Home::new("a11-r5");
    let pc = start(&home);
    let code = code(&home, 300);
    let (a, b) = (device(), device());
    let first = s(&ask(&pc, &a, &code, "A"), "request");
    let second = s(&ask(&pc, &b, &code, "B"), "request");
    cli_ok(&home, &["approve", &first]);
    let refused = cli_refused(&home, &["approve", &second]);
    assert!(refused.contains("another device was approved"), "{refused}");
    assert!(cli_refused(&home, &["approve", &first]).contains("already approved"));
    finish(&pc, &a, &code, "A");
    let answer = ask(&pc, &b, &code, "B");
    assert_eq!(s(&answer, "code"), "pairing_refused", "{answer:?}");
    assert!(cli_refused(&home, &["approve", &second]).contains("another device was approved"));
    assert_eq!(trusted(&home).len(), 1);
    // Unknown request ids approve nothing either.
    assert!(cli_refused(&home, &["approve", "0000000000000000"]).contains("no pairing request"));
}

#[test]
fn a11_r6_pending_and_approved_requests_expire_with_their_code() {
    let home = Home::new("a11-r6");
    let pc = start(&home);
    let (a, b) = (device(), device());
    let never = code(&home, 4);
    let decided = code(&home, 4);
    let undecided = s(&ask(&pc, &a, &never, "A"), "request");
    let approved = s(&ask(&pc, &b, &decided, "B"), "request");
    cli_ok(&home, &["approve", &approved]);
    std::thread::sleep(Duration::from_millis(5_100));
    assert!(cli_refused(&home, &["approve", &undecided]).contains("expired"));
    let late = ask(&pc, &a, &never, "A");
    assert_eq!(s(&late, "code"), "pairing_refused", "{late:?}");
    assert!(s(&late, "message").contains("expired"));
    // Approved in time, but the phone came back too late: no trust.
    let late = ask(&pc, &b, &decided, "B");
    assert_eq!(s(&late, "code"), "pairing_refused", "{late:?}");
    assert!(s(&late, "message").contains("expired"));
    assert!(trusted(&home).is_empty());
    assert!(pending_requests(&home).is_empty());
}

#[test]
fn a11_r7_the_older_pairing_flow_is_refused_by_the_pc() {
    let home = Home::new("a11-r7");
    let pc = start(&home);
    let code = code(&home, 300);
    for (version, expected) in [
        ("", "pairing_upgrade_required"),
        (",\"pairing_version\":1", "pairing_upgrade_required"),
        (",\"pairing_version\":\"2\"", "pairing_upgrade_required"),
        (",\"pairing_version\":null", "pairing_upgrade_required"),
        (",\"pairing_version\":3", "unsupported_pairing_version"),
    ] {
        let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
        let answer = client.hello(&format!(
            "\"intent\":\"pair\",\"code\":\"{code}\",\"name\":\"old app\"{version}"
        ));
        assert_eq!(s(&answer, "code"), expected, "{version}: {answer:?}");
        assert!(client.closed_within(Duration::from_secs(3)));
    }
    assert!(trusted(&home).is_empty());
    assert!(
        pending_requests(&home).is_empty(),
        "an old-flow hello was recorded"
    );
    // The code was not touched: the current flow still asks with it.
    let answer = ask(&pc, &device(), &code, "new app");
    assert_eq!(s(&answer, "type"), "pairing_pending", "{answer:?}");
}

#[test]
fn a11_r8_a_device_paired_before_a11_reconnects_with_no_qr_code_or_approval() {
    // The files a PC paired before A11 has: devices.json as it wrote it, and
    // a version 1 pairing.json whose code the device used.
    let home = Home::new("a11-r8");
    let phone = device();
    let fingerprint = identity::fingerprint(&phone.certificate);
    let paths = home.paths();
    lcl_remote::paths::write_private(
        &paths.config.join("devices.json"),
        format!(
            "{{\n  \"version\": 1,\n  \"devices\": [\n    {{\n      \"id\": \"5a4e3c2b1a0f9e8d\",\n      \"name\": \"Pixel 7\",\n      \"fingerprint\": \"{fingerprint}\",\n      \"paired_at\": 1790000000,\n      \"last_seen\": 1790000100,\n      \"protocol\": \"lcl.remote/1\",\n      \"revoked_at\": null\n    }}\n  ]\n}}\n"
        )
        .as_bytes(),
    )
    .unwrap();
    let now = paths::now();
    lcl_remote::paths::write_private(
        &paths.state.join("pairing.json"),
        format!(
            "{{\n  \"version\": 1,\n  \"challenges\": [\n    {{\n      \"id\": \"0011223344556677\",\n      \"hash\": \"{}\",\n      \"created\": {},\n      \"expires\": {},\n      \"consumed_at\": {},\n      \"consumed_by\": \"{fingerprint}\"\n    }}\n  ]\n}}\n",
            "0".repeat(64),
            now - 600,
            now - 300,
            now - 590
        )
        .as_bytes(),
    )
    .unwrap();
    let pc = start(&home);
    let (mut client, welcome) = reconnect(&pc, &phone);
    assert_eq!(s(&welcome, "type"), "welcome", "{welcome:?}");
    assert_eq!(s(welcome.get("device").unwrap(), "id"), "5a4e3c2b1a0f9e8d");
    let (status, body) = client.request("projects", "");
    assert_eq!(status, 200, "{body:?}");
    let mut named = connect(&pc, &phone, &pc.fingerprint).unwrap();
    let answer = named.hello("\"intent\":\"connect\",\"device\":\"5a4e3c2b1a0f9e8d\"");
    assert_eq!(s(&answer, "type"), "welcome", "{answer:?}");
    assert!(pending_requests(&home).is_empty());
    // The old pairing state is still read: new pairing works beside it.
    let fresh = code(&home, 300);
    let answer = ask(&pc, &device(), &fresh, "New phone");
    assert_eq!(s(&answer, "type"), "pairing_pending", "{answer:?}");
}

#[test]
fn a11_r9_pending_requests_are_deduplicated_and_bounded() {
    use lcl_remote::pairing::MAX_PENDING_PER_CHALLENGE;
    let home = Home::new("a11-r9");
    let pc = start(&home);
    let code = code(&home, 300);
    let phone = device();
    let first = s(&ask(&pc, &phone, &code, "Phone"), "request");
    for _ in 0..5 {
        assert_eq!(s(&ask(&pc, &phone, &code, "Phone"), "request"), first);
    }
    assert_eq!(pending_requests(&home).len(), 1);
    for i in 1..MAX_PENDING_PER_CHALLENGE {
        let answer = ask(&pc, &device(), &code, &format!("Phone {i}"));
        assert_eq!(s(&answer, "type"), "pairing_pending", "{answer:?}");
    }
    let full = ask(&pc, &device(), &code, "one too many");
    assert_eq!(s(&full, "code"), "pairing_busy", "{full:?}");
    assert_eq!(pending_requests(&home).len(), MAX_PENDING_PER_CHALLENGE);
    // Those already waiting are still answered, the same as before.
    assert_eq!(s(&ask(&pc, &phone, &code, "Phone"), "request"), first);
    // A request that is not a hello a PC can read fails closed.
    let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
    let answer = client.hello("\"intent\":\"pair\",\"pairing_version\":2,\"code\":7");
    assert_eq!(s(&answer, "code"), "malformed", "{answer:?}");
    assert!(trusted(&home).is_empty());
    let stored = std::fs::read_to_string(home.paths().state.join("pairing.json")).unwrap();
    assert!(!stored.contains(&code), "the pairing code is stored");
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

/// A12: the app makes a new key for every pairing, so a phone that pairs this
/// PC again while it is still trusted becomes a second record, and the first
/// stays trusted — the PC cannot tell a replacement from a second phone, and
/// must not guess by name. Only the old key itself can end the old record: a
/// connection it authenticates, then `unpair`. That ends exactly that record,
/// never the new key or another phone, and from then on the old key is refused
/// as `revoked`, which is how the phone learns the retirement is done.
#[test]
fn a12_a_replaced_key_stays_trusted_until_it_unpairs_itself_and_nothing_else_ends() {
    let home = Home::new("a12-replace");
    let pc = start(&home);
    let (old, tablet) = (device(), device());
    let (_old_live, old_id) = pair(&home, &pc, &old, "Pixel");
    // Another phone with the same name: a separate device, not a replacement.
    let (_tablet_live, tablet_id) = pair(&home, &pc, &tablet, "Pixel");
    // The first phone pairs again: a new key, a new QR code, a new approval.
    let new = device();
    let (mut new_live, new_id) = pair(&home, &pc, &new, "Pixel");
    let ids = |home: &Home| {
        let mut ids: Vec<String> = trusted(home).into_iter().map(|d| d.id).collect();
        ids.sort();
        ids
    };
    let mut all = vec![old_id.clone(), tablet_id.clone(), new_id.clone()];
    all.sort();
    assert_eq!(
        ids(&home),
        all,
        "the PC trusts the old key beside the new one"
    );

    // The old key retires itself.
    let (mut retiring, welcome) = reconnect(&pc, &old);
    assert_eq!(s(&welcome, "type"), "welcome", "{welcome:?}");
    assert_eq!(s(welcome.get("device").unwrap(), "id"), old_id);
    let (status, body) = retiring.request("unpair", "");
    assert_eq!(status, 200, "{body:?}");
    assert_eq!(body.get("unpaired").and_then(Json::as_bool), Some(true));
    let mut left = vec![tablet_id.clone(), new_id.clone()];
    left.sort();
    assert_eq!(ids(&home), left, "unpair ended another record than its own");
    let (_c, answer) = reconnect(&pc, &old);
    assert_eq!(
        s(&answer, "code"),
        "revoked",
        "the retired key still connects"
    );

    // The new pairing and the other phone are untouched.
    let (status, _) = new_live.request("ping", "");
    assert_eq!(status, 200);
    let (_c, answer) = reconnect(&pc, &tablet);
    assert_eq!(
        s(&answer, "type"),
        "welcome",
        "retiring one phone's key ended another phone"
    );

    // Forget on the phone afterwards: its new key unpairs too, and nothing of
    // that phone is trusted any more.
    let (status, _) = new_live.request("unpair", "");
    assert_eq!(status, 200);
    assert_eq!(ids(&home), vec![tablet_id]);
}

// ---------------------------------------------------------------------------
// Peers nobody has authenticated
// ---------------------------------------------------------------------------

/// Whether the PC closes a plain TCP connection within `within`.
fn tcp_closed_within(tcp: &mut TcpStream, within: Duration) -> bool {
    tcp.set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let deadline = Instant::now() + within;
    let mut buffer = [0u8; 1024];
    while Instant::now() < deadline {
        match tcp.read(&mut buffer) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return true,
        }
    }
    false
}

#[test]
fn a_first_message_larger_than_a_hello_is_refused_at_once() {
    use lcl_remote::frame::MAX_HELLO_FRAME;
    let home = Home::new("bighello");
    let pc = start(&home);
    let phone = device();
    let (paired, _) = pair(&home, &pc, &phone, "Phone");
    drop(paired);

    // A paired device's own hello, padded past what a hello needs: closed,
    // and not welcomed.
    let mut client = connect(&pc, &phone, &pc.fingerprint).unwrap();
    let padding = "x".repeat(MAX_HELLO_FRAME);
    client.send(&format!(
        "{{\"type\":\"hello\",\"protocol\":\"lcl.remote/1\",\"intent\":\"connect\",\"padding\":\"{padding}\"}}"
    ));
    assert!(
        client.closed_within(Duration::from_secs(5)),
        "an oversized hello was not refused"
    );
    let answered = client.reader.next_frame();
    assert!(
        matches!(answered, Ok(None)),
        "an oversized hello was answered: {answered:?}"
    );

    // Only a length announced, and no body: closed at once, not kept
    // waiting for bytes that would have to be buffered.
    let mut client = connect(&pc, &phone, &pc.fingerprint).unwrap();
    client.send_raw(&(MAX_HELLO_FRAME as u32 + 1).to_be_bytes());
    assert!(
        client.closed_within(Duration::from_secs(2)),
        "a peer announcing a large first message was kept waiting for it"
    );

    // Once authenticated, a document-sized frame is read whole.
    let (mut client, welcome) = reconnect(&pc, &phone);
    assert_eq!(s(&welcome, "type"), "welcome");
    let (status, body) =
        client.request("ping", &format!("\"padding\":\"{}\"", "y".repeat(1 << 20)));
    assert_eq!(status, 200, "{body:?}");
}

#[test]
fn silent_slow_and_broken_peers_are_closed_by_the_hello_deadline() {
    let home = Home::new("slow");
    let pc = start(&home);
    let within = lcl_remote::session::HELLO_WITHIN + Duration::from_secs(3);
    // Side by side, so the test waits for one deadline and not three.
    let silent_tcp = {
        let pc = pc.clone();
        std::thread::spawn(move || {
            let mut tcp = TcpStream::connect(pc.address).unwrap();
            tcp_closed_within(&mut tcp, within)
        })
    };
    let silent_tls = {
        let pc = pc.clone();
        std::thread::spawn(move || {
            let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
            client.closed_within(within)
        })
    };
    let partial = {
        let pc = pc.clone();
        std::thread::spawn(move || {
            let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
            client.send_raw(&[0, 0, 0, 100, b'{', b'"', b't', b'y', b'p', b'e', b'"']);
            client.closed_within(within)
        })
    };
    assert!(
        silent_tcp.join().unwrap(),
        "a connection that said nothing stayed open"
    );
    assert!(
        silent_tls.join().unwrap(),
        "a device that never said hello stayed connected"
    );
    assert!(
        partial.join().unwrap(),
        "a hello that never finished kept the connection open"
    );

    // A first frame that is not UTF-8 closes the connection at once.
    let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
    client.send_raw(&[0, 0, 0, 2, 0xff, 0xfe]);
    assert!(client.closed_within(Duration::from_secs(2)));
    assert!(matches!(client.reader.next_frame(), Ok(None)));
}

#[test]
fn every_refused_connection_gives_its_slot_back() {
    let home = Home::new("slots");
    let pc = start(&home);
    let phone = device();
    let (paired, _) = pair(&home, &pc, &phone, "Phone");
    drop(paired);
    // A phone waiting for approval asks every few seconds; each ask is a
    // connection of its own, closed once answered.
    let (waiting, code) = (device(), code(&home, 300));
    // More refusals than there are slots of any kind, one after another.
    for i in 0..lcl_remote::service::MAX_CONNECTIONS + 8 {
        let answer = ask(&pc, &waiting, &code, "waiting");
        assert_eq!(s(&answer, "type"), "pairing_pending", "attempt {i}");
        let mut client = connect(&pc, &device(), &pc.fingerprint).unwrap();
        client.send("not json");
        assert!(
            client.closed_within(Duration::from_secs(5)),
            "refused connection {i} stayed open"
        );
        let (_c, answer) = reconnect(&pc, &device());
        assert_eq!(s(&answer, "code"), "not_paired", "attempt {i}");
        drop(TcpStream::connect(pc.address).unwrap()); // comes and goes at once
    }
    let (_c, welcome) = reconnect(&pc, &phone);
    assert_eq!(s(&welcome, "type"), "welcome", "slots were not given back");
}

#[test]
fn unauthenticated_peers_cannot_crowd_out_the_rest() {
    use lcl_remote::service::MAX_UNAUTHENTICATED_PER_ADDRESS;
    let home = Home::new("crowd");
    let pc = start(&home);
    let phone = device();
    let (mut live, _) = pair(&home, &pc, &phone, "Phone");

    // Peers that connect and say nothing, as many as one address may have.
    let idle: Vec<TcpStream> = (0..MAX_UNAUTHENTICATED_PER_ADDRESS)
        .map(|_| TcpStream::connect(pc.address).unwrap())
        .collect();
    // One more from the same address is closed at once, not left to wait.
    let mut extra = TcpStream::connect(pc.address).unwrap();
    assert!(
        tcp_closed_within(&mut extra, Duration::from_secs(2)),
        "an address held more unauthenticated connections than it may"
    );
    // The authenticated session is untouched by all of it.
    let (status, body) = live.request("ping", "");
    assert_eq!(status, 200, "{body:?}");

    // When they leave, their slots are free again at once.
    drop(idle);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match connect(&pc, &phone, &pc.fingerprint) {
            Ok(mut client) => {
                let answer = client.hello("\"intent\":\"connect\"");
                assert_eq!(s(&answer, "type"), "welcome", "{answer:?}");
                break;
            }
            Err(e) => {
                assert!(Instant::now() < deadline, "the slots never came back: {e}");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
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
fn two_devices_saving_from_one_revision_cannot_both_land() {
    let home = Home::new("twosavers");
    std::fs::create_dir_all(home.workspace()).unwrap();
    std::fs::write(home.workspace().join("doc.lcl"), VALID).unwrap();
    let pc = start(&home);
    let (mut a, _) = pair(&home, &pc, &device(), "Phone A");
    let (mut b, _) = pair(&home, &pc, &device(), "Phone B");
    let project = project(&mut a);
    let open = format!("\"project\":\"{project}\",\"document\":\"doc.lcl\"");
    let base = s(&a.request("open", &open).1, "digest");
    assert_eq!(s(&b.request("open", &open).1, "digest"), base);

    // Both save at once, each over the revision it opened.
    let save = |mut client: Client, value: &str| {
        let text = VALID.replace("VALUE: 1", value);
        let fields = format!(
            "\"project\":\"{project}\",\"document\":\"doc.lcl\",\"base\":\"{base}\",\"text\":{}",
            json_string(&text)
        );
        std::thread::spawn(move || (client.request("save", &fields).0, text))
    };
    let a = save(a, "VALUE: 2");
    let b = save(b, "VALUE: 3");
    let (a, b) = (a.join().unwrap(), b.join().unwrap());
    let mut statuses = [a.0, b.0];
    statuses.sort_unstable();
    assert_eq!(statuses, [200, 409], "A: {}, B: {}", a.0, b.0);
    let landed = if a.0 == 200 { a.1 } else { b.1 };
    assert_eq!(
        std::fs::read_to_string(home.workspace().join("doc.lcl")).unwrap(),
        landed
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

/// `lcl-remote` itself, run beside the service on this PC's files, as a person
/// runs it.
fn cli(home: &Home, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_lcl-remote"))
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", home.0.join("config"))
        .env("XDG_STATE_HOME", home.0.join("state"))
        .env("XDG_DATA_HOME", home.0.join("data"))
        .output()
        .unwrap()
}

fn listed(client: &mut Client) -> Vec<String> {
    let (status, body) = client.request("projects", "");
    assert_eq!(status, 200, "{body:?}");
    body.get("projects")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|p| s(p, "id"))
        .collect()
}

#[test]
fn a_project_stops_being_shared_at_once_without_a_restart() {
    let home = Home::new("unshare");
    let extra = home.0.join("extra");
    std::fs::create_dir_all(&extra).unwrap();
    std::fs::write(extra.join("doc.lcl"), VALID).unwrap();
    let folder = home.0.join("todo");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("todo.txt"), "Buy milk\n").unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "sharer");
    let folder_name = extra.display().to_string();

    // Shared while the service runs: offered from the next request on.
    let added = cli(&home, &["projects", "add", &folder_name]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    let id = lcl_remote::projects::project_id(&extra.canonicalize().unwrap());
    assert!(
        listed(&mut client).contains(&id),
        "a new share was not offered"
    );
    let target = format!("\"project\":\"{id}\"");
    let (status, body) = client.request("open", &format!("{target},\"document\":\"doc.lcl\""));
    assert_eq!(status, 200, "{body:?}");
    // A run in it, held at its first pause: its routes are open and cached.
    let grants = format!(
        "{{\"read\":[{0}],\"write\":[{0}]}}",
        json_string(&folder.display().to_string())
    );
    let text = json_string(&backup_document(&folder));
    let run_fields =
        format!("{target},\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants}");
    let (status, started) = client.request("run", &run_fields);
    assert_eq!(status, 200, "{started:?}");
    let run = s(&started, "run");
    let this_run = |name: &'static str| {
        let run = run.clone();
        move |m: &Json| {
            m.get("event").and_then(Json::as_str) == Some("run")
                && m.get("run").and_then(Json::as_str) == Some(run.as_str())
                && m.get("name").and_then(Json::as_str) == Some(name)
        }
    };
    let paused = client
        .event(Duration::from_secs(20), this_run("paused"))
        .expect("the run pauses before its first effect");
    let sequence = paused
        .get("data")
        .and_then(|d| d.get("sequence"))
        .and_then(Json::as_u64)
        .unwrap();

    // Unshared while the service runs, and no restart.
    let removed = cli(&home, &["projects", "remove", &folder_name]);
    assert!(
        removed.status.success(),
        "{}",
        String::from_utf8_lossy(&removed.stderr)
    );
    assert!(
        !listed(&mut client).contains(&id),
        "an unshared project is still offered"
    );
    let base = lcl_spec::sha256::hex_digest(VALID.as_bytes());
    for (op, fields) in [
        ("tree", String::new()),
        ("open", ",\"document\":\"doc.lcl\"".to_string()),
        (
            "save",
            format!(",\"document\":\"doc.lcl\",\"base\":\"{base}\",\"text\":\"changed\\n\""),
        ),
        (
            "check",
            format!(",\"document\":\"doc.lcl\",\"text\":{}", json_string(VALID)),
        ),
        ("follow", format!(",\"run\":\"{run}\",\"from\":0")),
        (
            "answer",
            format!(",\"run\":\"{run}\",\"sequence\":{sequence},\"answer\":\"continue\""),
        ),
    ] {
        let (status, body) = client.request(op, &format!("{target}{fields}"));
        assert_eq!(status, 404, "{op} reached an unshared project: {body:?}");
    }
    let (status, body) = client.request("run", &run_fields);
    assert_eq!(
        status, 404,
        "a run started in an unshared project: {body:?}"
    );
    assert_eq!(
        std::fs::read_to_string(extra.join("doc.lcl")).unwrap(),
        VALID,
        "a save reached an unshared project"
    );
    // The run it held is stopped, and the device is told; its effect never
    // happened.
    let failed = client
        .event(Duration::from_secs(5), this_run("failed"))
        .expect("the device was not told its run was stopped");
    assert!(s(failed.get("data").unwrap(), "error").contains("no longer shared"));
    assert!(client
        .event(Duration::from_secs(5), this_run("end"))
        .is_some());
    assert!(!folder.join("todo_backup.txt").exists());
    assert_eq!(
        std::fs::read_to_string(folder.join("todo.txt")).unwrap(),
        "Buy milk\n"
    );

    // Shared again: offered again, opened afresh.
    assert!(cli(&home, &["projects", "add", &folder_name])
        .status
        .success());
    assert!(listed(&mut client).contains(&id));
    let (status, body) = client.request("open", &format!("{target},\"document\":\"doc.lcl\""));
    assert_eq!(status, 200, "{body:?}");
}

#[test]
fn a_link_out_of_the_project_is_neither_read_nor_written() {
    let home = Home::new("symlink");
    std::fs::create_dir_all(home.workspace()).unwrap();
    let outside = home.0.join("outside.lcl");
    std::fs::write(&outside, VALID).unwrap();
    std::os::unix::fs::symlink(&outside, home.workspace().join("escape.lcl")).unwrap();
    let pc = start(&home);
    let (mut client, _) = pair(&home, &pc, &device(), "links");
    let project = project(&mut client);
    let (status, body) = client.request(
        "open",
        &format!("\"project\":\"{project}\",\"document\":\"escape.lcl\""),
    );
    assert!(
        status >= 400,
        "a link out of the project was read: {body:?}"
    );
    let base = lcl_spec::sha256::hex_digest(VALID.as_bytes());
    let (status, body) = client.request(
        "save",
        &format!("\"project\":\"{project}\",\"document\":\"escape.lcl\",\"base\":\"{base}\",\"text\":\"changed\\n\""),
    );
    assert!(
        status >= 400,
        "a link out of the project was written: {body:?}"
    );
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), VALID);
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

#[test]
fn only_the_device_that_started_a_run_can_follow_or_answer_it() {
    let home = Home::new("owner");
    let folder = home.0.join("todo");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("todo.txt"), "Buy milk\n").unwrap();
    let pc = start(&home);
    let (mut a, _) = pair(&home, &pc, &device(), "Phone A");
    let (mut b, _) = pair(&home, &pc, &device(), "Phone B");
    let project = project(&mut a);
    let grants = format!(
        "{{\"read\":[{0}],\"write\":[{0}]}}",
        json_string(&folder.display().to_string())
    );
    let text = json_string(&backup_document(&folder));
    let (status, started) = a.request("run", &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants}"));
    assert_eq!(status, 200, "{started:?}");
    let run = s(&started, "run");
    let is_run = |m: &Json| m.get("event").and_then(Json::as_str) == Some("run");
    let paused = a
        .event(Duration::from_secs(20), |m| {
            is_run(m) && m.get("name").and_then(Json::as_str) == Some("paused")
        })
        .expect("A's run pauses before its first effect");
    let sequence = paused
        .get("data")
        .and_then(|d| d.get("sequence"))
        .and_then(Json::as_u64)
        .unwrap();
    a.events.retain(|m| !is_run(m));

    // B is paired and knows the run's id, and still gets nothing of it.
    let (status, body) = b.request(
        "follow",
        &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"from\":0"),
    );
    assert_eq!(status, 403, "B followed A's run: {body:?}");
    for answer in ["continue", "deny", "cancel"] {
        let (status, body) = b.request(
            "answer",
            &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"sequence\":{sequence},\"answer\":\"{answer}\""),
        );
        assert_eq!(status, 403, "B answered {answer} for A's run: {body:?}");
    }
    let leaked = b.event(Duration::from_secs(1), is_run);
    assert!(leaked.is_none(), "B was sent A's run: {leaked:?}");
    assert!(
        !b.events.iter().any(is_run),
        "B was sent A's run: {:?}",
        b.events
    );
    // A's run is exactly where B found it: paused, nothing done, not cancelled.
    let moved = a.event(Duration::from_millis(500), is_run);
    assert!(moved.is_none(), "B's answer moved A's run: {moved:?}");
    assert!(!folder.join("todo_backup.txt").exists());

    // A still follows and answers its own run, to the end.
    let (status, body) = a.request(
        "follow",
        &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"from\":0"),
    );
    assert_eq!(status, 200, "{body:?}");
    let (status, body) = a.request(
        "answer",
        &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"sequence\":{sequence},\"answer\":\"continue\""),
    );
    assert_eq!(status, 200, "{body:?}");
    let (_, report) = run_until_end(&mut a, "continue");
    let completion = report.get("completion").expect("a completion");
    assert_eq!(s(completion, "terminal_status"), "status.succeeded");
    assert!(folder.join("todo_backup.txt").is_file());
}

#[test]
fn a_device_cannot_switch_off_the_pause_before_an_effect() {
    let home = Home::new("mustpause");
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

    // Granted, and asking not to be asked: the PC pauses all the same.
    let (status, started) = client.request("run", &format!("\"project\":\"{project}\",\"document\":\"backup.lcl\",\"text\":{text},\"grants\":{grants},\"break_effects\":false"));
    assert_eq!(status, 200, "{started:?}");
    let is_run = |m: &Json| m.get("event").and_then(Json::as_str) == Some("run");
    // `operation` only says which operation is being dispatched; the host
    // gate, where an effect is permitted or not, comes after it.
    let first = client
        .event(Duration::from_secs(20), |m| {
            is_run(m) && m.get("name").and_then(Json::as_str) != Some("operation")
        })
        .expect("the run went quiet");
    assert_eq!(
        s(&first, "name"),
        "paused",
        "a run went ahead without pausing: {first:?}"
    );
    let data = first.get("data").unwrap();
    assert_eq!(s(data, "kind"), "effect");

    // Held there: nothing happens while the device has not answered. What
    // arrived before the pause was already passed over; only what follows it
    // counts.
    client.events.retain(|m| !is_run(m));
    let moved = client.event(Duration::from_millis(1500), is_run);
    assert!(
        moved.is_none(),
        "the run moved on without an answer: {moved:?}"
    );
    assert!(!folder.join("todo_backup.txt").exists());
    assert_eq!(
        std::fs::read_to_string(folder.join("todo.txt")).unwrap(),
        "Buy milk\n"
    );

    // Only answers move it, and every effect has its own pause before it.
    let sequence = data.get("sequence").and_then(Json::as_u64).unwrap();
    let run = s(&first, "run");
    let (status, _) = client.request(
        "answer",
        &format!("\"project\":\"{project}\",\"run\":\"{run}\",\"sequence\":{sequence},\"answer\":\"continue\""),
    );
    assert_eq!(status, 200);
    let (rest, report) = run_until_end(&mut client, "continue");
    let mut paused = true;
    let mut effects = 0;
    for name in &rest {
        match name.as_str() {
            "paused" => paused = true,
            "effect" => {
                assert!(paused, "an effect without its own pause: {rest:?}");
                paused = false;
                effects += 1;
            }
            _ => {}
        }
    }
    assert!(effects >= 2, "{rest:?}");
    let completion = report.get("completion").expect("a completion");
    assert_eq!(s(completion, "terminal_status"), "status.succeeded");
    assert!(folder.join("todo_backup.txt").is_file());
}
