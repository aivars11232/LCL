//! UI-01: execute the actual served frontend against an owned workspace server.
//! Node's controlled DOM is not evidence of a visible browser or desktop launch.

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
    fn start(mut command: Command, root: &Path, name: &str) -> Self {
        let log = root.join(name);
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&log)
            .unwrap();
        let child = command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("TMPDIR", std::env::temp_dir())
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
                assert!(output.contains("11 passed; 0 failed; 0 skipped"));
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
    let mut command = Command::new(&binary);
    command.arg(&project.0).arg("--spec").arg(spec);
    let mut server = Process::start(command, &project.0, "server.log");
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
