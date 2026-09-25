//! UI-01: execute the actual served frontend against an owned workspace server.
//! Node's controlled DOM is not evidence of a visible browser or desktop launch.

/// The number of editor acceptance cases this suite is known to cover.
///
/// A floor, not an expected total: adding a case must not break the gate, and
/// losing one must.
const EXPECTED_CASES: usize = 56;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "lcl-editor-save-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("cannot create owned editor project: {error}"),
            }
        }
        panic!("cannot reserve an unused editor fixture directory");
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!(
                "owned editor fixture cleanup failed: {}: {error}",
                self.0.display()
            );
        }
    }
}

struct Process {
    child: Child,
    log: PathBuf,
}

impl Process {
    fn start(command: Command, root: &Path, name: &str) -> Self {
        Process::start_with(command, root, name, &[])
    }

    fn start_with(mut command: Command, root: &Path, name: &str, env: &[(&str, PathBuf)]) -> Self {
        let log = root.join(name);
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&log)
            .unwrap();
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", std::env::temp_dir());
        for (key, value) in env {
            command.env(key, value);
        }
        let child = command
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .expect("fixture process must start; Node >=22 is a required test prerequisite");
        Self { child, log }
    }

    fn output(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap()
    }

    fn server_url(&mut self) -> String {
        let start = Instant::now();
        loop {
            let output = self.output();
            if let Some(url) = output
                .lines()
                .find_map(|line| line.trim().strip_prefix("open     "))
            {
                assert!(url.starts_with("http://127.0.0.1:"), "{output}");
                assert!(url.contains("/?t="), "{output}");
                return url.to_string();
            }
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "server exited: {output}"
            );
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "server startup deadline: {output}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn assert_success(&mut self) {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let output = self.output();
                println!("{output}"); // complete subprocess evidence enters the cargo log
                assert!(status.success(), "editor acceptance exited {status}");
                // Nothing failed, nothing was skipped, and the suite did not
                // quietly shrink. A bare expected total goes stale every time a
                // case is added and says nothing about which cases ran; this
                // fails if any case fails, is skipped, or disappears.
                let summary = output
                    .lines()
                    .rev()
                    .find(|line| line.contains(" passed; ") && line.contains(" failed; "))
                    .unwrap_or_else(|| panic!("no summary line in:\n{output}"))
                    .to_string();
                assert!(
                    summary.contains("0 failed; 0 skipped"),
                    "editor acceptance did not pass cleanly: {summary}"
                );
                let passed: usize = summary
                    .split(" passed")
                    .next()
                    .and_then(|n| n.trim().parse().ok())
                    .unwrap_or_else(|| panic!("unreadable summary: {summary}"));
                assert!(
                    passed >= EXPECTED_CASES,
                    "the editor suite ran {passed} cases; it must not lose coverage \
                     below the {EXPECTED_CASES} it had"
                );
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "editor acceptance deadline: {}",
                self.output()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        let _ = self.child.kill(); // only this still-owned child
        let start = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if start.elapsed() < Duration::from_secs(2) => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                other => {
                    let detail = format!(
                        "editor fixture {} cleanup incomplete: {other:?}",
                        self.child.id()
                    );
                    if std::thread::panicking() {
                        eprintln!("{detail}");
                        return;
                    }
                    panic!("{detail}");
                }
            }
        }
    }
}

/// A stand-in `lcl-remote`, so the Android-devices routes run a real program
/// with the real arguments: Phone A is paired; a pairing code brings two
/// pairing requests — Phone B's (`c0ffee01`) and a stranger's (`badd0000`,
/// whose name is markup) — and only `approve c0ffee01` trusts Phone B, only
/// `deny badd0000` denies the stranger; revoking `aa11` revokes Phone A. The
/// controlled mode of editor_save.cjs answers the same way.
fn stand_in_remote(project: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = project.join(".remote");
    std::fs::create_dir(&dir).unwrap();
    let script = dir.join("lcl-remote");
    std::fs::write(
        &script,
        r#"#!/bin/sh
dir=$(dirname "$0")
case "$1 $2" in
"devices --json")
    revoked=null
    [ -f "$dir/revoked" ] && revoked=1790000300
    new=
    [ -f "$dir/approved" ] && new=',{"id":"bb22","name":"Phone B","fingerprint":"b","paired_at":1790000200,"last_seen":1790000200,"revoked_at":null,"online":true}'
    printf '{"service_running":true,"devices":[{"id":"aa11","name":"Phone A","fingerprint":"a","paired_at":1790000000,"last_seen":1790000100,"revoked_at":%s,"online":false}%s]}
' "$revoked" "$new"
    ;;
"pair --json")
    : >"$dir/issued"
    printf '{"payload":"LCLPAIR|v=2&c=standin","svg":"<svg/>","expires":%s,"addresses":["192.0.2.1:47300"],"pc":"Stand-in PC","fingerprint":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}
' "$(($(date +%s) + 300))"
    ;;
"pending --json")
    expires=$(($(date +%s) + 240))
    list=
    if [ -f "$dir/issued" ] && [ ! -f "$dir/approved" ]; then
        list='{"request":"c0ffee01","name":"Phone B","fingerprint":"bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000bbbb0000","verification":"abcd-ef12-3456","created":1790000150,"expires":'$expires',"status":"pending"}'
        [ -f "$dir/denied" ] || list="$list"',{"request":"badd0000","name":"<img src=x onerror=alert(1)>","fingerprint":"eeee0000eeee0000eeee0000eeee0000eeee0000eeee0000eeee0000eeee0000","verification":"9999-0000-1111","created":1790000140,"expires":'$expires',"status":"pending"}'
    fi
    printf '{"service_running":true,"requests":[%s]}
' "$list"
    ;;
"approve c0ffee01")
    : >"$dir/approved"
    echo "approved Phone B (request c0ffee01, verification code abcd-ef12-3456)"
    ;;
"deny badd0000")
    : >"$dir/denied"
    echo "denied the stranger (request badd0000); it is not trusted"
    ;;
"revoke aa11")
    : >"$dir/revoked"
    echo "revoked Phone A (aa11); it is disconnected and must pair again"
    ;;
*)
    echo "lcl-remote: unexpected arguments: $*" >&2
    exit 2
    ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[test]
fn production_editor_save_logic_survives_real_http() {
    let (binary, spec) = match (
        std::env::var_os("LCL_EDITOR_WORKSPACE_BIN"),
        std::env::var_os("LCL_EDITOR_SPEC"),
    ) {
        (Some(binary), Some(spec)) => (PathBuf::from(binary), PathBuf::from(spec)),
        (None, None) => (
            PathBuf::from(env!("CARGO_BIN_EXE_lcl-workspace")),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../canonical/LCL_Core_0.1.0"),
        ),
        _ => panic!(
            "exact candidate tests require BOTH LCL_EDITOR_WORKSPACE_BIN and LCL_EDITOR_SPEC"
        ),
    };
    let binary = binary.canonicalize().unwrap();
    let spec = spec.canonicalize().unwrap();
    println!(
        "workspace binary: {}; sha256 {}",
        binary.display(),
        lcl_spec::sha256::hex_digest(&std::fs::read(&binary).unwrap())
    );
    println!("explicit spec: {}", spec.display());
    let project = Project::new();
    let remote = stand_in_remote(&project.0);
    let mut command = Command::new(&binary);
    command.arg(&project.0).arg("--spec").arg(spec);
    // The workspace settings this server reads and writes: its own, inside a
    // dot directory of the project, which the project tree never lists. The
    // Android-devices routes run a stand-in lcl-remote from another one.
    let mut server = Process::start_with(
        command,
        &project.0,
        "server.log",
        &[
            ("XDG_CONFIG_HOME", project.0.join(".config")),
            ("LCL_REMOTE_BIN", remote),
        ],
    );
    let url = server.server_url();
    let mut command = Command::new("node");
    command
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/editor_save.cjs"))
        .arg("--server")
        .arg(url)
        .arg("--project")
        .arg(&project.0);
    let mut acceptance = Process::start(command, &project.0, "editor.log");
    acceptance.assert_success();
}
