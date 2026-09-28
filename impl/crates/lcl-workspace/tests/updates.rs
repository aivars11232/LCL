//! Settings → Updates: the workspace runs the updater installed beside it and
//! passes its answers on; it installs only a staged update, and only by
//! handing over to the updater and closing.

mod common;

use common::{send, Running, Scratch};
use lcl_spec::json::Json;
use lcl_workspace::updates::Updater;
use lcl_workspace::{Routes, Workspace};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn request(running: &Running, method: &str, target: &str) -> common::Reply {
    send(
        running.address,
        method,
        &format!("{target}?t={}", running.token),
        &[],
        b"",
    )
}

fn serve(folder: &Scratch, updater: Option<Updater>) -> Running {
    let workspace =
        Workspace::open(&folder.path, common::canonical_root()).expect("the workspace opens");
    common::start(Arc::new(
        Routes::new(Arc::new(workspace)).with_updater(updater),
    ))
}

/// A stand-in updater: it logs its arguments and answers with the state
/// named in `state.txt`.
fn fake(dir: &Scratch) -> std::path::PathBuf {
    let program = dir.join("lcl-update");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{log}'\n[ -e '{silent}' ] && exit 1\nprintf '{{\"state\": {{\"state\": \"%s\"}}, \"installed\": \"0.1.0\"}}\\n' \"$(cat '{state}')\"\n",
            log = dir.join("calls.log").display(),
            silent = dir.join("silent").display(),
            state = dir.join("state.txt").display(),
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

fn calls(dir: &Scratch) -> String {
    std::fs::read_to_string(dir.join("calls.log")).unwrap_or_default()
}

fn state_of(reply: &common::Reply) -> String {
    let json = lcl_spec::json::parse(&reply.body).unwrap_or_else(|e| panic!("{e}: {}", reply.body));
    json.get("state")
        .and_then(|s| s.get("state"))
        .and_then(Json::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn a_window_without_an_updater_says_so() {
    let folder = Scratch::new("updates-none");
    let running = serve(&folder, None);
    for (method, path) in [
        ("GET", "/api/update"),
        ("POST", "/api/update/check"),
        ("POST", "/api/update/install"),
    ] {
        let reply = request(&running, method, path);
        assert_eq!(reply.status, 404, "{path}");
        assert!(reply.body.contains("no updater"), "{}", reply.body);
    }
}

#[test]
fn status_check_download_and_install_go_through_the_installed_updater() {
    let folder = Scratch::new("updates-folder");
    let dir = Scratch::new("updates-fake");
    let program = fake(&dir);
    std::fs::write(dir.join("state.txt"), "update_available").unwrap();
    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    let running = serve(
        &folder,
        Some(Updater::new(
            program,
            Arc::new(move || flag.store(true, Ordering::SeqCst)),
        )),
    );

    let reply = request(&running, "GET", "/api/update");
    assert_eq!(reply.status, 200);
    assert_eq!(state_of(&reply), "update_available");
    let reply = request(&running, "POST", "/api/update/check");
    assert_eq!(reply.status, 200);
    assert_eq!(calls(&dir), "status --json\ncheck --json\n");

    // A download runs on by itself; the page follows it through the status.
    let reply = request(&running, "POST", "/api/update/download");
    assert_eq!(reply.status, 202);
    let started = Instant::now();
    while !calls(&dir).contains("download --json") {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the download never started"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    // Nothing staged: nothing installed, and the workspace stays open.
    let reply = request(&running, "POST", "/api/update/install");
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(!quit.load(Ordering::SeqCst));
    assert!(!calls(&dir).contains("apply"));

    // Staged: the updater takes over, waiting for this process, and the
    // workspace closes so it can be replaced.
    std::fs::write(dir.join("state.txt"), "ready_to_install").unwrap();
    let reply = request(&running, "POST", "/api/update/install");
    assert_eq!(reply.status, 202, "{}", reply.body);
    let wanted = format!("apply --wait-pid {} --relaunch", std::process::id());
    let started = Instant::now();
    while !calls(&dir).contains(&wanted) {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{}",
            calls(&dir)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(quit.load(Ordering::SeqCst));

    // An updater that answers nothing is an error, not an empty state.
    std::fs::write(dir.join("silent"), "").unwrap();
    assert_eq!(request(&running, "GET", "/api/update").status, 502);
}
