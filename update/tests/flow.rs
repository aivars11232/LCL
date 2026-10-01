//! The PC update flow end to end: the real `lcl-update` (a test build, so it
//! accepts a local release server and a throwaway key) checks, downloads and
//! installs synthetic releases through the real `packaging/install.sh`, in a
//! disposable home directory. Nothing here reads or writes the real one.
#![cfg(feature = "test-endpoint")]

use lcl_spec::json::Json;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};

/// A release's assets, and a way to spoil them.
type Assets = Vec<(&'static str, Vec<u8>)>;
type Change<'a> = Box<dyn Fn(&mut Assets) + 'a>;

const SPKI_PREFIX: &str = "3059301306072a8648ce3d020106082a8648ce3d030107034200";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}

struct Signer {
    pair: EcdsaKeyPair,
}

impl Signer {
    fn new() -> Signer {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        Signer {
            pair: EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                .unwrap(),
        }
    }
    fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.pair
            .sign(&SystemRandom::new(), message)
            .unwrap()
            .as_ref()
            .to_vec()
    }
    fn line(&self, id: &str) -> String {
        format!(
            "{id} {SPKI_PREFIX}{}\n",
            hex(self.pair.public_key().as_ref())
        )
    }
}

/// A local stand-in for GitHub Releases: `/api/releases/latest` and
/// `/download/<tag>/<name>`, answered from a table the test changes.
struct Server {
    base: String,
    routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
        let table = Arc::clone(&routes);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok() && line.trim() != "" {
                    line.clear();
                }
                let path = first.split_whitespace().nth(1).unwrap_or("").to_string();
                let body = table.lock().unwrap().get(&path).cloned();
                let answer = match body {
                    Some(body) => {
                        let mut a =
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len())
                                .into_bytes();
                        a.extend_from_slice(&body);
                        a
                    }
                    None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec(),
                };
                let _ = stream.write_all(&answer);
            }
        });
        Server { base, routes }
    }

    fn clear(&self) {
        self.routes.lock().unwrap().clear();
    }

    fn set(&self, path: &str, body: Vec<u8>) {
        self.routes.lock().unwrap().insert(path.to_string(), body);
    }

    /// Publish a release: its listing and its assets.
    fn publish(&self, tag: &str, prerelease: bool, assets: &[(&str, Vec<u8>)]) {
        self.clear();
        let listed: Vec<String> = assets
            .iter()
            .map(|(name, body)| format!(r#"{{"name": "{name}", "size": {}}}"#, body.len()))
            .collect();
        self.set(
            "/api/releases/latest",
            format!(r#"{{"tag_name": "{tag}", "draft": false, "prerelease": {prerelease}, "assets": [{}]}}"#, listed.join(", ")).into_bytes(),
        );
        for (name, body) in assets {
            self.set(&format!("/download/{tag}/{name}"), body.clone());
        }
    }
}

/// One disposable home directory with an installed LCL.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Home {
        let root =
            std::env::temp_dir().join(format!("lcl-update-flow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Home { root }
    }
    fn bin(&self) -> PathBuf {
        self.root.join(".local/bin")
    }
    fn cache(&self) -> PathBuf {
        self.root.join(".cache/lcl/update")
    }
    fn command(&self, program: &Path) -> Command {
        let mut c = Command::new(program);
        c.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &self.root)
            .env("LCL_UPDATE_TEST_KEYS", self.root.join("keys.txt"))
            .env("LCL_UPDATE_SYSTEMCTL", self.root.join("systemctl"));
        c
    }
    /// Run the installed updater; its exit status and `--json` answer.
    fn update(&self, server: &Server, args: &[&str]) -> (bool, Json) {
        let out = self
            .command(&self.bin().join("lcl-update"))
            .env("LCL_UPDATE_TEST_ENDPOINT", &server.base)
            .args(args)
            .arg("--json")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        let json = lcl_spec::json::parse(&text)
            .unwrap_or_else(|e| panic!("{e}: {text} {}", String::from_utf8_lossy(&out.stderr)));
        (out.status.success(), json)
    }
    /// Every file below the installation, with its bytes.
    fn installed(&self) -> BTreeMap<String, Vec<u8>> {
        let mut files = BTreeMap::new();
        for dir in [self.bin(), self.root.join(".local/share/lcl")] {
            walk(&dir, &self.root, &mut files);
        }
        files
    }
    fn systemctl_calls(&self) -> String {
        std::fs::read_to_string(self.root.join("systemctl.log")).unwrap_or_default()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn walk(dir: &Path, root: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, root, into);
        } else {
            into.insert(
                path.strip_prefix(root).unwrap().display().to_string(),
                std::fs::read(&path).unwrap(),
            );
        }
    }
}

fn state_of(json: &Json) -> (String, Option<String>) {
    let state = json.get("state").unwrap();
    (
        state
            .get("state")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string(),
        state
            .get("error")
            .and_then(|e| e.get("kind"))
            .and_then(Json::as_str)
            .map(str::to_string),
    )
}

/// Write a file with a mode.
///
/// A file that will be run is written by a child process, not by this one.
/// While a process has a file open for writing, a child another thread forks
/// at that moment inherits the descriptor, and running the file then fails
/// with "Text file busy" until that child has exec'd; tests run on several
/// threads and fork all the time. A file this process never opened cannot be
/// caught that way.
fn write(path: &Path, text: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    if mode & 0o111 == 0 {
        std::fs::write(path, text).unwrap();
    } else {
        let written = Command::new("sh")
            .arg("-c")
            .arg(r#"printf '%s' "$1" > "$0""#)
            .arg(path)
            .arg(text)
            .status()
            .expect("sh runs");
        assert!(written.success(), "{} was not written", path.display());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// How a synthetic payload misbehaves.
#[derive(Clone, Copy, PartialEq)]
enum Flaw {
    None,
    /// Its own lcl-update reports another version: fails staged validation.
    WrongStagedVersion,
    /// Carries an icon whose folder is blocked on disk: the real install
    /// fails halfway, after the binaries are published.
    HalfwayInstall,
    /// Its lcl cannot open the package once installed: fails the health check.
    UnhealthyOnceInstalled,
}

/// A release payload directory for `version`, installable by the real
/// install.sh. `lcl_update` is the updater binary it carries, or a stub.
fn payload(dir: &Path, version: &str, lcl_update: Option<&Path>, flaw: Flaw) -> PathBuf {
    let top = dir.join(format!("lcl-{version}-linux-x86_64"));
    let unhealthy = if flaw == Flaw::UnhealthyOnceInstalled {
        r#"case "$0" in */.local/bin/*) exit 3 ;; esac"#
    } else {
        ""
    };
    write(
        &top.join("bin/lcl"),
        // `lcl --version` answers as the real LCL 0.1.0 and 0.1.1 binaries
        // do: the product line, then the protocol and the language. The
        // updater must read the first line, or it refuses every real release.
        &format!("#!/bin/sh\ncase \"$1\" in\n--version) printf 'lcl {version}\\nprotocol lcl.engine/1\\nlanguage 0.1.0\\n' ;;\nspec) {unhealthy}\n [ -d \"$3\" ] || exit 2 ;;\nesac\n"),
        0o755,
    );
    write(
        &top.join("bin/lcl-workspace"),
        &format!("#!/bin/sh\necho workspace {version}\n"),
        0o755,
    );
    write(
        &top.join("bin/lcl-remote"),
        &format!("#!/bin/sh\necho remote {version}\n"),
        0o755,
    );
    match lcl_update {
        Some(binary) => {
            std::fs::copy(binary, top.join("bin/lcl-update")).unwrap();
        }
        None => {
            let reported = if flaw == Flaw::WrongStagedVersion {
                "9.9.9"
            } else {
                version
            };
            write(
                &top.join("bin/lcl-update"),
                &format!("#!/bin/sh\necho \"lcl-update {reported}\"\n"),
                0o755,
            );
        }
    }
    write(
        &top.join("share/LCL_Core_0.1.0/VERSION.txt"),
        &format!("{version}\n"),
        0o644,
    );
    std::fs::copy(repo().join("packaging/install.sh"), top.join("install.sh")).unwrap();
    std::fs::copy(
        repo().join("packaging/lcl.desktop"),
        top.join("share/lcl.desktop"),
    )
    .unwrap();
    std::fs::copy(
        repo().join("packaging/lcl-workspace-launch.in"),
        top.join("share/lcl-workspace-launch.in"),
    )
    .unwrap();
    std::fs::copy(
        repo().join("impl/integration/linux/lcl.xml"),
        top.join("share/lcl.xml"),
    )
    .unwrap();
    if flaw == Flaw::HalfwayInstall {
        write(
            &top.join("share/icons/hicolor/64x64/apps/lcl-workspace.png"),
            "png",
            0o644,
        );
    }
    top
}

fn tarball(top: &Path) -> Vec<u8> {
    let out = top.with_extension("tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(top.parent().unwrap())
        .arg(top.file_name().unwrap())
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::read(out).unwrap()
}

fn manifest(version: &str, artifact: &[u8], architecture: &str) -> Vec<u8> {
    format!(
        r#"{{"format": 1, "product": "lcl", "channel": "stable", "product_version": "{version}",
"release_tag": "v{version}", "source_commit": "{commit}", "published_at": "2026-10-01T12:00:00Z",
"release_notes": "Test release {version}.", "minimum_supported_version": "0.0.1",
"signing_key_id": "test-key",
"pc": {{"artifact_name": "lcl-{version}-linux-x86_64.tar.gz", "size": {size}, "sha256": "{sha}",
"architecture": "{architecture}", "required_updater_version": 1}},
"android": {{"artifact_name": "lcl-android-{version}-7.apk", "size": 10, "sha256": "{b}",
"application_id": "io.lcl.workspace", "version_name": "{version}", "version_code": 7,
"minimum_sdk": 29, "signer_sha256": "{b}"}}}}"#,
        commit = "a".repeat(40),
        size = artifact.len(),
        sha = sha256(artifact),
        b = "b".repeat(64),
    )
    .into_bytes()
}

/// A release newer than the updater under test, whatever the product version
/// is: the next minor version. `later` is newer still.
fn next() -> &'static str {
    static NEXT: OnceLock<String> = OnceLock::new();
    NEXT.get_or_init(|| minor_after(1))
}

fn later() -> &'static str {
    static LATER: OnceLock<String> = OnceLock::new();
    LATER.get_or_init(|| minor_after(2))
}

fn minor_after(step: u64) -> String {
    let v = lcl_update::version::Version::parse(lcl_update::PRODUCT_VERSION).unwrap();
    format!("{}.{}.0", v.major, v.minor + step)
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// A home with LCL 0.1.0 installed by the real install.sh, carrying the real
/// (test) updater, and user data beside it.
fn installed_home(name: &str, signer: &Signer, remote_running: bool) -> Home {
    let home = Home::new(name);
    std::fs::write(home.root.join("keys.txt"), signer.line("test-key")).unwrap();
    write(
        &home.root.join("systemctl"),
        &format!(
            "#!/bin/sh\necho \"$*\" >> '{log}'\ncase \"$2\" in\nis-active) [ -e '{marker}' ] ;;\nstop) rm -f '{marker}' ;;\nstart) : > '{marker}' ;;\n*) exit 1 ;;\nesac\n",
            log = home.root.join("systemctl.log").display(),
            marker = home.root.join("remote-active").display(),
        ),
        0o755,
    );
    if remote_running {
        std::fs::write(home.root.join("remote-active"), "").unwrap();
    }
    let old = payload(
        &home.root.join("old"),
        "0.1.0",
        Some(Path::new(env!("CARGO_BIN_EXE_lcl-update"))),
        Flaw::None,
    );
    // lcl-remote counts as installed only when remote/install.sh put it there.
    write(
        &home.bin().join("lcl-remote"),
        "#!/bin/sh\necho remote 0.1.0\n",
        0o755,
    );
    let status = home
        .command(&old.join("install.sh"))
        .current_dir(&old)
        .status()
        .unwrap();
    assert!(status.success());
    for (file, text) in [
        (".config/lcl/workspace-settings.json", "{\"version\": 1}\n"),
        (".config/lcl/masters/mine.json", "{\"id\": \"mine\"}\n"),
        (
            ".local/state/lcl/remote/devices.json",
            "{\"devices\": []}\n",
        ),
        (".config/lcl/remote/identity.json", "{\"pc\": \"id\"}\n"),
        ("LCL-Projects/Alpha/main.lcl", "LCL:\n"),
    ] {
        write(&home.root.join(file), text, 0o600);
    }
    home
}

fn user_data(home: &Home) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for dir in [".config/lcl", ".local/state/lcl/remote", "LCL-Projects"] {
        walk(&home.root.join(dir), &home.root, &mut files);
    }
    files
}

#[test]
fn a_check_offers_only_a_newer_signed_stable_release() {
    let signer = Signer::new();
    let server = Server::start();
    let home = installed_home("check", &signer, false);
    let version = |v: &str| -> Vec<(&'static str, Vec<u8>)> {
        let artifact = format!("artifact {v}").into_bytes();
        let m = manifest(v, &artifact, "x86_64-linux");
        let sig = signer.sign(&m);
        let name: &'static str = Box::leak(format!("lcl-{v}-linux-x86_64.tar.gz").into_boxed_str());
        vec![
            ("update-manifest.json", m),
            ("update-manifest.sig", sig),
            (name, artifact),
        ]
    };

    // No release at all, then the same, an older and a pre-release version.
    let (ok, json) = home.update(&server, &["check"]);
    assert!(ok);
    assert_eq!(state_of(&json).0, "up_to_date");
    for (v, pre) in [("0.1.0", false), ("0.0.9", false), (later(), true)] {
        server.publish(&format!("v{v}"), pre, &version(v));
        assert_eq!(
            state_of(&home.update(&server, &["check"]).1).0,
            "up_to_date",
            "{v}"
        );
    }

    // A newer one, signed: offered, with what a person is shown.
    server.publish(leak(format!("v{}", next())), false, &version(next()));
    let (ok, json) = home.update(&server, &["check"]);
    assert!(ok);
    assert_eq!(state_of(&json).0, "update_available");
    let available = json.get("state").unwrap().get("available").unwrap();
    assert_eq!(
        available.get("version").and_then(Json::as_str),
        Some(next())
    );
    assert_eq!(
        available.get("release_notes").and_then(Json::as_str),
        Some(format!("Test release {}.", next()).as_str())
    );
    assert_eq!(json.get("check_due").and_then(Json::as_bool), Some(false));

    // Unsigned, signed by an unknown key, altered after signing, malformed.
    let stranger = Signer::new();
    let cases: Vec<(&str, Change, &str)> = vec![
        (
            "unsigned",
            Box::new(|a| {
                a.remove(1);
            }),
            "invalid",
        ),
        (
            "wrong key",
            Box::new(|a| a[1].1 = stranger.sign(&a[0].1)),
            "verification",
        ),
        (
            "altered",
            Box::new(|a| {
                a[0].1 = String::from_utf8(a[0].1.clone())
                    .unwrap()
                    .replace("Test release", "Evil release")
                    .into_bytes()
            }),
            "verification",
        ),
        (
            "malformed",
            Box::new(|a| {
                a[0].1 = b"{\"format\": 1}".to_vec();
                a[1].1 = signer.sign(&a[0].1);
            }),
            "verification",
        ),
        (
            "wrong architecture",
            Box::new(|a| {
                a[0].1 = manifest(
                    next(),
                    format!("artifact {}", next()).as_bytes(),
                    "aarch64-linux",
                );
                a[1].1 = signer.sign(&a[0].1);
            }),
            "unsupported",
        ),
    ];
    for (what, change, kind) in cases {
        let mut assets = version(next());
        change(&mut assets);
        server.publish(leak(format!("v{}", next())), false, &assets);
        let (ok, json) = home.update(&server, &["check"]);
        assert!(!ok, "{what}");
        assert_eq!(
            state_of(&json),
            ("failed".to_string(), Some(kind.to_string())),
            "{what}"
        );
    }

    // No network: said so, and nothing else changes.
    drop(server);
    let dead = Server {
        base: "http://127.0.0.1:9".to_string(),
        routes: Arc::default(),
    };
    let (ok, json) = home.update(&dead, &["check"]);
    assert!(!ok);
    assert_eq!(state_of(&json).0, "offline");
}

#[test]
fn a_download_is_staged_only_when_it_is_exactly_what_was_signed() {
    let signer = Signer::new();
    let server = Server::start();
    let home = installed_home("download", &signer, false);
    let before = home.installed();
    let publish = |artifact: &[u8], served: &[u8]| {
        let m = manifest(next(), artifact, "x86_64-linux");
        server.publish(
            leak(format!("v{}", next())),
            false,
            &[
                ("update-manifest.json", m.clone()),
                ("update-manifest.sig", signer.sign(&m)),
                (
                    leak(format!("lcl-{}-linux-x86_64.tar.gz", next())),
                    served.to_vec(),
                ),
            ],
        );
    };
    let good = tarball(&payload(&home.root.join("new"), next(), None, Flaw::None));

    // The artifact served is not the one signed (same size, other bytes).
    let mut altered = good.clone();
    let last = altered.len() - 1;
    altered[last] ^= 0xff;
    publish(&good, &altered);
    assert!(home.update(&server, &["check"]).0);
    let (ok, json) = home.update(&server, &["download"]);
    assert!(!ok);
    assert_eq!(
        state_of(&json),
        ("failed".to_string(), Some("verification".to_string()))
    );
    assert!(!home.cache().join("staging").exists());

    // The release lists another size than the manifest signed.
    publish(&good, &good[..good.len() - 1]);
    let (ok, json) = home.update(&server, &["check"]);
    assert!(!ok);
    assert_eq!(state_of(&json).1.as_deref(), Some("invalid"));

    // A payload whose own binaries do not report the signed version.
    let wrong = tarball(&payload(
        &home.root.join("wrong"),
        next(),
        None,
        Flaw::WrongStagedVersion,
    ));
    publish(&wrong, &wrong);
    assert!(home.update(&server, &["check"]).0);
    let (ok, json) = home.update(&server, &["download"]);
    assert!(!ok);
    assert_eq!(state_of(&json).1.as_deref(), Some("verification"));
    assert!(!home.cache().join("staging").exists());
    assert_eq!(
        home.installed(),
        before,
        "a refused download changes nothing installed"
    );
}

#[test]
fn an_update_installs_keeps_user_data_restarts_only_a_running_service_and_cleans_up() {
    let signer = Signer::new();
    let server = Server::start();
    let home = installed_home("apply", &signer, true);
    let data = user_data(&home);
    let artifact = tarball(&payload(&home.root.join("new"), next(), None, Flaw::None));
    let m = manifest(next(), &artifact, "x86_64-linux");
    server.publish(
        leak(format!("v{}", next())),
        false,
        &[
            ("update-manifest.json", m.clone()),
            ("update-manifest.sig", signer.sign(&m)),
            (
                leak(format!("lcl-{}-linux-x86_64.tar.gz", next())),
                artifact,
            ),
        ],
    );
    assert!(home.update(&server, &["check"]).0);
    let (ok, json) = home.update(&server, &["download"]);
    assert!(ok, "{json:?}");
    assert_eq!(state_of(&json).0, "ready_to_install");
    let (ok, json) = home.update(&server, &["apply"]);
    assert!(ok, "{json:?}");
    assert_eq!(state_of(&json).0, "up_to_date");
    // Nothing is left to offer: the update found is gone, and what was
    // installed is recorded.
    let state = json.get("state").unwrap();
    assert_eq!(state.get("available"), Some(&Json::Null));
    assert_eq!(
        state
            .get("updated")
            .and_then(|u| u.get("version"))
            .and_then(Json::as_str),
        Some(next())
    );

    let version = |program: &str, arg: &str| {
        String::from_utf8(
            home.command(&home.bin().join(program))
                .arg(arg)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
    };
    assert_eq!(
        version("lcl", "--version").lines().next(),
        Some(format!("lcl {}", next()).as_str()),
        "the binaries were replaced"
    );
    assert_eq!(
        version("lcl-update", "version").trim(),
        format!("lcl-update {}", next())
    );
    assert_eq!(
        version("lcl-remote", "").trim(),
        format!("remote {}", next()),
        "an installed remote service is updated too"
    );
    assert_eq!(
        user_data(&home),
        data,
        "projects, settings, Masters, identity and devices are untouched"
    );
    assert_eq!(home.systemctl_calls(), "--user is-active --quiet lcl-remote.service\n--user stop --quiet lcl-remote.service\n--user start --quiet lcl-remote.service\n--user is-active --quiet lcl-remote.service\n");
    assert!(
        !home.cache().join("staging").exists() && !home.cache().join("rollback").exists(),
        "the cache is emptied"
    );
}

#[test]
fn a_failed_installation_is_rolled_back_and_a_stopped_service_stays_stopped() {
    let signer = Signer::new();
    for flaw in [Flaw::HalfwayInstall, Flaw::UnhealthyOnceInstalled] {
        let server = Server::start();
        let home = installed_home(
            if flaw == Flaw::HalfwayInstall {
                "halfway"
            } else {
                "unhealthy"
            },
            &signer,
            false,
        );
        let before = home.installed();
        let data = user_data(&home);
        let artifact = tarball(&payload(&home.root.join("new"), next(), None, flaw));
        let m = manifest(next(), &artifact, "x86_64-linux");
        server.publish(
            leak(format!("v{}", next())),
            false,
            &[
                ("update-manifest.json", m.clone()),
                ("update-manifest.sig", signer.sign(&m)),
                (
                    leak(format!("lcl-{}-linux-x86_64.tar.gz", next())),
                    artifact,
                ),
            ],
        );
        assert!(home.update(&server, &["check"]).0);
        assert!(home.update(&server, &["download"]).0);
        if flaw == Flaw::HalfwayInstall {
            // A file where the icon's folder must go: only the real install
            // meets it, after it has published the binaries.
            write(
                &home.root.join(".local/share/icons/hicolor/64x64"),
                "in the way",
                0o644,
            );
        }
        let (ok, json) = home.update(&server, &["apply"]);
        assert!(!ok);
        assert_eq!(state_of(&json).1.as_deref(), Some("install"));
        let message = json
            .get("state")
            .unwrap()
            .get("error")
            .unwrap()
            .get("message")
            .and_then(Json::as_str)
            .unwrap()
            .to_string();
        assert!(
            message.contains("previous version was restored"),
            "{message}"
        );
        assert_eq!(
            home.installed(),
            before,
            "the previous installation is back, byte for byte"
        );
        assert_eq!(user_data(&home), data);
        assert_eq!(
            home.systemctl_calls(),
            "--user is-active --quiet lcl-remote.service\n",
            "a stopped service is never started or enabled"
        );
        assert!(!home.cache().join("rollback").exists());
    }
}

/// Stand-ins for `mv` and `sync` in `dir` that log every call to
/// `dir/calls.log` and then run the real command; `mv_first` runs before the
/// real `mv`, with its arguments. The PATH that puts them first.
fn logging_tools(dir: &Path, mv_first: &str) -> std::ffi::OsString {
    let real = |name: &str| {
        let out = Command::new("sh")
            .args(["-c", &format!("command -v {name}")])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let log = dir.join("calls.log");
    write(
        &dir.join("mv"),
        &format!(
            "#!/bin/sh\necho \"mv $*\" >> '{log}'\n{mv_first}\nexec '{mv}' \"$@\"\n",
            log = log.display(),
            mv = real("mv"),
        ),
        0o755,
    );
    write(
        &dir.join("sync"),
        &format!(
            "#!/bin/sh\necho \"sync $*\" >> '{log}'\nexec '{sync}' \"$@\"\n",
            log = log.display(),
            sync = real("sync"),
        ),
        0o755,
    );
    format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()).into()
}

/// Install `version` into `home` with `path` as PATH; whether it succeeded.
fn install(home: &Home, version: &str, path: Option<&std::ffi::OsStr>) -> bool {
    let top = payload(
        &home.root.join(format!("payload-{version}")),
        version,
        None,
        Flaw::None,
    );
    // Through sh: executing a file just written races, in a test process
    // that forks on other threads, with ETXTBSY.
    let mut command = home.command(Path::new("sh"));
    command.arg(top.join("install.sh"));
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command
        .current_dir(&top)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn package_dir_entries(home: &Home) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(home.root.join(".local/share/lcl"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_package_directory_is_exchanged_in_one_step_and_every_rename_is_flushed() {
    let home = Home::new("exchange");
    assert!(install(&home, "0.1.0", None));
    let tools = home.root.join("tools");
    let path = logging_tools(&tools, "");
    assert!(install(&home, "0.2.0", Some(&path)));

    let package = home.root.join(".local/share/lcl/LCL_Core_0.1.0");
    assert_eq!(
        std::fs::read_to_string(package.join("VERSION.txt")).unwrap(),
        "0.2.0\n"
    );
    assert_eq!(package_dir_entries(&home), ["LCL_Core_0.1.0"]);
    let calls = std::fs::read_to_string(tools.join("calls.log")).unwrap();
    let lines: Vec<&str> = calls.lines().collect();
    let pkg = package.display().to_string();
    let exchanged = lines
        .iter()
        .position(|l| l.starts_with("mv --exchange -T ") && l.ends_with(&format!(" {pkg}")))
        .unwrap_or_else(|| panic!("no exchange:\n{calls}"));
    // The installed package was never renamed aside, so its name was never
    // missing; the rename was flushed; so was the copy before it.
    assert!(
        !lines.iter().any(|l| l.starts_with(&format!("mv {pkg} "))),
        "{calls}"
    );
    let parent = format!("sync -- {}", package.parent().unwrap().display());
    assert!(lines[exchanged..].contains(&parent.as_str()), "{calls}");
    assert!(
        lines[..exchanged]
            .iter()
            .any(|l| l.starts_with("sync -- ") && l.contains("LCL_Core_0.1.0.new.")),
        "{calls}"
    );
    // Every file published beside the package is flushed, then renamed, and
    // the rename flushed.
    let launcher = home.bin().join("lcl-workspace-launch");
    let renamed = lines
        .iter()
        .position(|l| l.starts_with("mv -f ") && l.ends_with(&format!(" {}", launcher.display())))
        .unwrap_or_else(|| panic!("{calls}"));
    assert!(lines[renamed - 1].starts_with("sync -- "), "{calls}");
    assert_eq!(
        lines[renamed + 1],
        format!("sync -- {}", home.bin().display()),
        "{calls}"
    );
}

#[test]
fn an_install_stopped_right_after_the_exchange_leaves_a_complete_package() {
    let home = Home::new("exchange-crash");
    assert!(install(&home, "0.1.0", None));
    let tools = home.root.join("tools");
    // Power lost, as far as the installer can tell, the moment the exchange
    // is done: nothing after it runs.
    let path = logging_tools(
        &tools,
        &format!(
            "case \"$1\" in --exchange) '{mv}' \"$@\"; kill -9 $PPID; exit 0 ;; esac",
            mv = String::from_utf8(
                Command::new("sh")
                    .args(["-c", "command -v mv"])
                    .output()
                    .unwrap()
                    .stdout
            )
            .unwrap()
            .trim()
        ),
    );
    assert!(!install(&home, "0.2.0", Some(&path)));
    let package = home.root.join(".local/share/lcl/LCL_Core_0.1.0");
    assert_eq!(
        std::fs::read_to_string(package.join("VERSION.txt")).unwrap(),
        "0.2.0\n"
    );
    // Running the install again finishes the job.
    assert!(install(&home, "0.2.0", None));
    assert_eq!(
        std::fs::read_to_string(package.join("VERSION.txt")).unwrap(),
        "0.2.0\n"
    );
}

#[test]
fn without_an_exchanging_mv_the_package_is_still_replaced_and_flushed() {
    let home = Home::new("no-exchange");
    assert!(install(&home, "0.1.0", None));
    let tools = home.root.join("tools");
    let path = logging_tools(
        &tools,
        "case \"$1\" in --exchange) echo \"mv: unrecognized option '--exchange'\" >&2; exit 1 ;; esac",
    );
    assert!(install(&home, "0.2.0", Some(&path)));
    let package = home.root.join(".local/share/lcl/LCL_Core_0.1.0");
    assert_eq!(
        std::fs::read_to_string(package.join("VERSION.txt")).unwrap(),
        "0.2.0\n"
    );
    assert_eq!(package_dir_entries(&home), ["LCL_Core_0.1.0"]);
    let calls = std::fs::read_to_string(tools.join("calls.log")).unwrap();
    let pkg = package.display().to_string();
    let aside = calls
        .lines()
        .position(|l| l.starts_with(&format!("mv {pkg} {pkg}.old.")))
        .unwrap_or_else(|| panic!("{calls}"));
    let lines: Vec<&str> = calls.lines().collect();
    assert!(
        lines[aside + 1].starts_with(&format!("mv {pkg}.new.")),
        "{calls}"
    );
    assert_eq!(
        lines[aside + 2],
        format!("sync -- {}", package.parent().unwrap().display()),
        "{calls}"
    );
}

#[test]
fn relative_or_empty_xdg_locations_are_ignored_by_the_installer() {
    let home = Home::new("xdg");
    let top = payload(&home.root.join("payload"), "0.1.0", None, Flaw::None);
    let elsewhere = home.root.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let run = |args: &[&str], home_var: &Path| {
        home.command(Path::new("sh"))
            .arg(top.join("install.sh"))
            .env("HOME", home_var)
            .env("XDG_BIN_HOME", "relative/bin")
            .env("XDG_DATA_HOME", "")
            .args(args)
            .current_dir(&elsewhere)
            .output()
            .unwrap()
    };
    let listed = run(&["--list"], &home.root);
    assert!(listed.status.success());
    let listed = String::from_utf8(listed.stdout).unwrap();
    assert!(!listed.is_empty());
    for line in listed.lines() {
        assert!(
            line.starts_with(&format!("{}/.local/", home.root.display())),
            "{line}"
        );
    }
    assert!(run(&[], &home.root).status.success());
    assert!(home.bin().join("lcl").is_file());
    assert!(home.root.join(".local/share/lcl/LCL_Core_0.1.0").is_dir());
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
    // HOME itself must be absolute.
    let refused = run(&["--list"], Path::new("relative-home"));
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("HOME is not set to an absolute path")
    );
}
