//! Which folder a launch opens: the real binary, its real arguments, and the
//! settings file the page writes.
//!
//! A launch that names no project and passes `--default-project` — what the
//! desktop launcher does — opens the default workspace chosen in Settings when
//! that folder exists, and the fallback it was given otherwise. An explicit
//! project or document always wins, and a bare launch from a terminal still
//! opens the working directory.

mod common;

use common::Scratch;
use lcl_spec::json::Json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// A running workspace process, killed when dropped.
struct Launched {
    child: Child,
    log: PathBuf,
}

impl Drop for Launched {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start the binary with `args`, `XDG_CONFIG_HOME` set to `config`, in `cwd`,
/// and nothing else from this environment.
fn launch(args: &[&std::ffi::OsStr], config: &Path, cwd: &Path, scratch: &Scratch) -> Launched {
    let log = scratch.join(&format!("launch-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log);
    let output = std::fs::File::create(&log).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_lcl-workspace"))
        .args(args)
        .arg("--spec")
        .arg(common::canonical_root())
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", config)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn()
        .expect("the workspace starts");
    Launched { child, log }
}

/// The session the launched workspace serves.
fn session(launched: &mut Launched) -> Json {
    let deadline = Instant::now() + Duration::from_secs(20);
    let url = loop {
        let printed = std::fs::read_to_string(&launched.log).unwrap_or_default();
        if let Some(url) = printed
            .lines()
            .find_map(|l| l.trim().strip_prefix("open     "))
        {
            break url.to_string();
        }
        if let Some(status) = launched.child.try_wait().unwrap() {
            panic!("the workspace exited {status}: {printed}");
        }
        assert!(Instant::now() < deadline, "no URL was printed: {printed}");
        std::thread::sleep(Duration::from_millis(20));
    };
    let rest = url.strip_prefix("http://").unwrap();
    let (address, query) = rest.split_once('/').unwrap();
    let token = query.strip_prefix("?t=").unwrap();
    let address: SocketAddr = address.parse().unwrap();
    let reply = common::send(address, "GET", &format!("/api/session?t={token}"), &[], b"");
    assert_eq!(reply.status, 200, "{}", reply.body);
    lcl_spec::json::parse(&reply.body).expect("JSON")
}

fn root(session: &Json) -> PathBuf {
    PathBuf::from(session.get("root").and_then(Json::as_str).expect("a root"))
}

fn notice(session: &Json) -> Option<String> {
    session
        .get("notice")
        .and_then(Json::as_str)
        .map(str::to_string)
}

/// Write a settings file naming `folder` as the default workspace.
fn choose(config: &Path, folder: &Path) {
    std::fs::create_dir_all(config.join("lcl")).unwrap();
    let text = format!(
        "{{\"version\": 1, \"default_workspace\": \"{}\", \"default_extension\": \".lcl\"}}\n",
        lcl_workspace::http::escape_json(&folder.display().to_string())
    );
    std::fs::write(config.join("lcl/workspace-settings.json"), text).unwrap();
}

#[test]
fn a_launch_that_names_no_project_opens_the_chosen_default_workspace() {
    let scratch = Scratch::new("launch-chosen");
    let config = scratch.join("config");
    let fallback = scratch.join("fallback");
    std::fs::create_dir_all(&fallback).unwrap();
    // A name a shell would misread in every way it can: nothing here passes
    // through one, and this proves it.
    let chosen = scratch.join("My LCL $HOME `echo x` 'q' \"d\" ;&* {a,b}");
    std::fs::create_dir_all(&chosen).unwrap();
    choose(&config, &chosen);

    let mut launched = launch(
        &["--default-project".as_ref(), fallback.as_os_str()],
        &config,
        &scratch.path,
        &scratch,
    );
    let session = session(&mut launched);
    assert_eq!(root(&session), chosen.canonicalize().unwrap());
    assert_eq!(notice(&session), None);
}

#[test]
fn an_explicit_project_or_document_wins_over_the_chosen_default() {
    let scratch = Scratch::new("launch-explicit");
    let config = scratch.join("config");
    let chosen = scratch.join("chosen");
    let explicit = scratch.join("explicit project");
    let fallback = scratch.join("fallback");
    for folder in [&chosen, &explicit, &fallback] {
        std::fs::create_dir_all(folder).unwrap();
    }
    choose(&config, &chosen);

    let mut launched = launch(
        &[
            explicit.as_os_str(),
            "--default-project".as_ref(),
            fallback.as_os_str(),
        ],
        &config,
        &scratch.path,
        &scratch,
    );
    assert_eq!(
        root(&session(&mut launched)),
        explicit.canonicalize().unwrap()
    );
    drop(launched);

    let document = scratch.put("papers/report.lcl", &common::example("01_MINIMAL_TASK.lcl"));
    let mut launched = launch(
        &[
            "--document".as_ref(),
            document.as_os_str(),
            "--default-project".as_ref(),
            fallback.as_os_str(),
        ],
        &config,
        &scratch.path,
        &scratch,
    );
    let session = session(&mut launched);
    assert_eq!(
        root(&session),
        scratch.join("papers").canonicalize().unwrap()
    );
    assert_eq!(
        session.get("open").and_then(Json::as_str),
        Some("report.lcl")
    );
}

#[test]
fn a_chosen_default_that_is_gone_opens_the_fallback_and_says_why() {
    let scratch = Scratch::new("launch-gone");
    let config = scratch.join("config");
    let fallback = scratch.join("fallback");
    std::fs::create_dir_all(&fallback).unwrap();
    let gone = scratch.join("moved away");
    choose(&config, &gone);

    let mut launched = launch(
        &["--default-project".as_ref(), fallback.as_os_str()],
        &config,
        &scratch.path,
        &scratch,
    );
    let session = session(&mut launched);
    assert_eq!(root(&session), fallback.canonicalize().unwrap());
    let told = notice(&session).expect("the page is told why");
    assert!(told.contains("does not exist"), "{told}");
    assert!(!gone.exists(), "a launch created the chosen folder");
}

#[test]
fn a_corrupt_settings_file_opens_the_fallback_and_says_so() {
    let scratch = Scratch::new("launch-corrupt");
    let config = scratch.join("config");
    let fallback = scratch.join("fallback");
    std::fs::create_dir_all(&fallback).unwrap();
    std::fs::create_dir_all(config.join("lcl")).unwrap();
    std::fs::write(
        config.join("lcl/workspace-settings.json"),
        "{\"version\": 1, oops",
    )
    .unwrap();

    let mut launched = launch(
        &["--default-project".as_ref(), fallback.as_os_str()],
        &config,
        &scratch.path,
        &scratch,
    );
    let session = session(&mut launched);
    assert_eq!(root(&session), fallback.canonicalize().unwrap());
    assert!(notice(&session).expect("told").contains("not valid JSON"));
}

#[test]
fn a_bare_launch_from_a_terminal_still_opens_the_working_directory() {
    let scratch = Scratch::new("launch-bare");
    let config = scratch.join("config");
    let chosen = scratch.join("chosen");
    let here = scratch.join("where I am");
    std::fs::create_dir_all(&chosen).unwrap();
    std::fs::create_dir_all(&here).unwrap();
    choose(&config, &chosen);

    let mut launched = launch(&[], &config, &here, &scratch);
    assert_eq!(root(&session(&mut launched)), here.canonicalize().unwrap());
}
