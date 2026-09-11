//! The installed desktop entry, actually launched.
//!
//! Regression, post-Task-20 finding F11. `packaging/lcl.desktop` was
//! `Exec=@BIN@ %f` with `Terminal=false`, and `install.sh` substituted only the
//! binary path. Three things were wrong with that at once, and none of them was
//! visible to a test that checked the file existed:
//!
//! * the workspace resolves one exact specification package and never searches
//!   for one, so a menu launch with no `LCL_SPEC` in the environment failed
//!   before it served anything;
//! * it opens a browser only under `--open`, and a desktop launch has no
//!   terminal, so the URL it printed went nowhere;
//! * `%f` passes a *document*, and the positional argument it landed on is a
//!   *project directory*, so a file association opened the wrong thing.
//!
//! So these cases install into a disposable home, read the desktop entry that
//! installation actually generated, parse its `Exec` line the way a desktop
//! does, and run it. `xdg-open` is a stub that records the URL rather than
//! opening a browser, and that URL is then fetched over a real socket. What is
//! asserted is what the workspace served, not that a file exists.
//!
//! Graphical confirmation is not claimed here. This is the dispatch, argument
//! and browser-launch seam; a person seeing a window is a separate gate.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root is present")
}

/// The directory this test run's binaries were built into.
fn built() -> PathBuf {
    let mut path = std::env::current_exe().expect("the test executable has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path
}

/// A disposable home, with nothing in it.
fn clean_home(case: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("lcl-launch-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("the temporary home is writable");
    home
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("writable");
    for entry in std::fs::read_dir(from).expect("readable").flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copyable");
        }
    }
}

/// Stage the payload `build_release.sh` assembles, from this build.
///
/// Not the committed tarball: that artifact predates this repair, and a test
/// that installed it would be measuring the old packaging. The layout is the
/// one the release script produces, so `install.sh` runs unmodified.
fn stage_payload(into: &Path) -> PathBuf {
    let packaging = repository().join("packaging");
    let payload = into.join("payload");
    std::fs::create_dir_all(payload.join("bin")).expect("writable");
    std::fs::create_dir_all(payload.join("share")).expect("writable");

    for name in ["lcl", "lcl-workspace"] {
        let from = built().join(name);
        assert!(
            from.is_file(),
            "{} was not built; run the workspace tests with --all-targets",
            from.display()
        );
        std::fs::copy(&from, payload.join("bin").join(name)).expect("copyable");
    }
    copy_tree(
        &repository().join("canonical/LCL_Core_0.1.0"),
        &payload.join("share/LCL_Core_0.1.0"),
    );
    for (from, to) in [
        (packaging.join("install.sh"), payload.join("install.sh")),
        (packaging.join("uninstall.sh"), payload.join("uninstall.sh")),
        (packaging.join("README.md"), payload.join("README.md")),
        (packaging.join("lcl.desktop"), payload.join("share/lcl.desktop")),
        (
            packaging.join("lcl-workspace-launch.in"),
            payload.join("share/lcl-workspace-launch.in"),
        ),
        (
            repository().join("impl/integration/linux/lcl.xml"),
            payload.join("share/lcl.xml"),
        ),
    ] {
        std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("{}: {e}", from.display()));
    }
    for script in ["install.sh", "uninstall.sh"] {
        make_executable(&payload.join(script));
    }
    payload
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).expect("readable").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("writable");
}

/// Run the payload's installer against one disposable home.
///
/// `env_clear`, so nothing this test session happens to export can make the
/// installation look better than it is. `LCL_SPEC` in particular is absent,
/// which is the whole point: the old packaging only worked when it was set.
fn install(home: &Path) -> (PathBuf, Output) {
    let payload = stage_payload(home);
    let output = Command::new(payload.join("install.sh"))
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the installer runs");
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    (payload, output)
}

/// One key from the installed desktop entry.
fn desktop_value(home: &Path, key: &str) -> String {
    let path = home.join(".local/share/applications/lcl-workspace.desktop");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is not readable: {e}", path.display()));
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("the desktop entry declares no {key}:\n{text}"))
        .to_string()
}

/// Split one `Exec` value the way a desktop implementation does.
///
/// Double quotes group, a backslash inside them escapes the next character,
/// and `%f` is the field code that carries one file. Nothing else here needs
/// the full field-code table.
fn exec_argv(exec: &str, document: Option<&Path>) -> Vec<String> {
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => {
                if let Some(escaped) = chars.next() {
                    current.push(escaped);
                }
            }
            ' ' if !quoted => {
                if started || !current.is_empty() {
                    argv.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            other => {
                current.push(other);
                started = true;
            }
        }
    }
    if started || !current.is_empty() {
        argv.push(current);
    }
    // The field code expands to the file, or to nothing at all.
    let mut expanded = Vec::new();
    for word in argv {
        if word == "%f" {
            if let Some(path) = document {
                expanded.push(path.display().to_string());
            }
        } else {
            expanded.push(word);
        }
    }
    expanded
}

/// A directory holding a stub `xdg-open` that records its argument.
///
/// The launcher asks the desktop to open a URL. On a test machine that must
/// not become a browser window, and the URL is the thing worth asserting
/// about, so the stub writes it down and exits.
fn stub_opener(home: &Path) -> PathBuf {
    let dir = home.join("stub-bin");
    std::fs::create_dir_all(&dir).expect("writable");
    let recorded = home.join("opened-url");
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$1\" > {}\n",
        recorded.display()
    );
    let path = dir.join("xdg-open");
    std::fs::write(&path, script).expect("writable");
    make_executable(&path);
    dir
}

/// A launched workspace, killed when the test is done with it.
struct Launched {
    child: std::process::Child,
    home: PathBuf,
}

impl Drop for Launched {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Launched {
    /// The URL the launcher handed to `xdg-open`, once it has.
    fn opened_url(&self) -> Option<String> {
        std::fs::read_to_string(self.home.join("opened-url")).ok()
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.home.join(".local/state/lcl/launch.log")).unwrap_or_default()
    }
}

/// Run the installed desktop entry exactly as a desktop would.
fn launch(home: &Path, document: Option<&Path>) -> Launched {
    let exec = desktop_value(home, "Exec");
    let argv = exec_argv(&exec, document);
    assert!(!argv.is_empty(), "the Exec line parsed to nothing: {exec}");
    let stub = stub_opener(home);
    let child = Command::new(&argv[0])
        .args(&argv[1..])
        .env_clear()
        .env("HOME", home)
        .env("PATH", format!("{}:/usr/bin:/bin", stub.display()))
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("the desktop entry's Exec did not start: {exec}: {e}"));
    Launched {
        child,
        home: home.to_path_buf(),
    }
}

/// Wait for the launcher to hand a URL to the desktop.
fn await_url(launched: &Launched) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if let Some(url) = launched.opened_url() {
            if url.starts_with("http://") {
                return url;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "the launcher never opened a URL. Its log said:\n{}",
        launched.log()
    );
}

/// `http://127.0.0.1:PORT/?t=TOKEN` split into what a request needs.
fn address_and_token(url: &str) -> (String, String) {
    let rest = url.strip_prefix("http://").expect("a loopback URL");
    let (address, query) = rest.split_once('/').expect("a path");
    let token = query
        .split_once("t=")
        .map(|(_, t)| t.to_string())
        .expect("the URL carries the session token");
    (address.to_string(), token)
}

/// One GET against the running workspace, over a real socket.
fn get(url: &str, path: &str) -> (u16, String) {
    let (address, token) = address_and_token(url);
    let mut stream = TcpStream::connect(&address).expect("the workspace is listening");
    let request = format!(
        "GET {path}{}t={token} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\n\r\n",
        if path.contains('?') { "&" } else { "?" }
    );
    stream.write_all(request.as_bytes()).expect("request sent");
    stream.flush().expect("flushed");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("reply read");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, body.to_string())
}

/// A `.lcl` document written under `home`, with a real canonical example in it.
fn document(home: &Path, relative: &str) -> PathBuf {
    let path = home.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("writable");
    let source = std::fs::read(
        repository().join("canonical/LCL_Core_0.1.0/08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"),
    )
    .expect("the example is readable");
    std::fs::write(&path, source).expect("writable");
    path
}

// ---------------------------------------------------------------------------

#[test]
fn the_installed_desktop_entry_runs_a_launcher_that_exists() {
    let home = clean_home("entry");
    install(&home);

    let exec = desktop_value(&home, "Exec");
    assert!(
        exec.ends_with(" %f"),
        "the entry must still accept a file: {exec}"
    );
    let argv = exec_argv(&exec, None);
    let launcher = PathBuf::from(&argv[0]);
    assert!(
        launcher.is_file(),
        "the Exec line names {}, which was not installed",
        launcher.display()
    );
    assert_eq!(
        launcher,
        home.join(".local/bin/lcl-workspace-launch"),
        "the entry must name the installed launcher by absolute path"
    );
    // The media type registration is preserved, and no default was claimed.
    assert_eq!(desktop_value(&home, "MimeType"), "text/x-lcl;");
    assert!(
        !home.join(".config/mimeapps.list").exists(),
        "installation must not change the operator's default applications"
    );
}

#[test]
fn a_menu_launch_with_no_document_opens_the_default_project() {
    let home = clean_home("menu");
    install(&home);

    let launched = launch(&home, None);
    let url = await_url(&launched);

    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    let expected = home.join(".local/share/lcl/workspace");
    assert!(
        body.contains(&format!("\"root\": \"{}\"", expected.display())),
        "a menu launch must open the stated default project, not an ambient \
         directory. Session was:\n{body}"
    );
    assert!(
        body.contains("\"open\": null"),
        "a launch with no document opens no document:\n{body}"
    );
    assert!(
        expected.is_dir(),
        "the default project directory must be created, not assumed"
    );
}

#[test]
fn a_file_association_opens_that_document_in_its_own_project() {
    let home = clean_home("assoc");
    install(&home);
    let path = document(&home, "papers/report.lcl");

    let launched = launch(&home, Some(&path));
    let url = await_url(&launched);

    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    assert!(
        body.contains(&format!("\"root\": \"{}\"", home.join("papers").display())),
        "a document's project is its own directory when no ancestor declares \
         one:\n{body}"
    );
    assert!(
        body.contains("\"open\": \"report.lcl\""),
        "the associated document must be the one the workspace opens:\n{body}"
    );

    // And the page itself is served, which is what the browser was handed.
    let (status, page) = get(&url, "/");
    assert_eq!(status, 200);
    assert!(page.contains("<title>LCL Workspace</title>"), "{page}");
}

#[test]
fn a_document_whose_path_holds_spaces_opens() {
    let home = clean_home("spaces");
    install(&home);
    let path = document(&home, "my papers/first draft.lcl");

    let launched = launch(&home, Some(&path));
    let url = await_url(&launched);

    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    assert!(
        body.contains("\"open\": \"first draft.lcl\""),
        "a path with spaces must arrive as one argument:\n{body}"
    );
}

#[test]
fn a_document_inside_a_project_opens_against_that_project() {
    let home = clean_home("project");
    install(&home);
    let root = home.join("work");
    let path = document(&home, "work/src/main.lcl");
    std::fs::write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
            home.join(".local/share/lcl/LCL_Core_0.1.0")
                .display()
                .to_string()
        ),
    )
    .expect("writable");

    let launched = launch(&home, Some(&path));
    let url = await_url(&launched);

    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    assert!(
        body.contains(&format!("\"root\": \"{}\"", root.display())),
        "the nearest ancestor declaring a project is the root:\n{body}"
    );
    assert!(
        body.contains("\"open\": \"src/main.lcl\""),
        "the document keeps its root-relative identity:\n{body}"
    );
}

#[test]
fn the_launcher_needs_no_spec_in_the_environment() {
    // The environment every launch above ran in had no LCL_SPEC, which is the
    // condition the old packaging failed under. This states it as its own
    // assertion rather than leaving it implicit in the others.
    let home = clean_home("nospec");
    install(&home);
    let launcher = std::fs::read_to_string(home.join(".local/bin/lcl-workspace-launch"))
        .expect("the launcher is installed");
    assert!(
        launcher.contains(
            &home
                .join(".local/share/lcl/LCL_Core_0.1.0")
                .display()
                .to_string()
        ),
        "the launcher must carry the installed package's absolute path"
    );
    assert!(
        !launcher.contains("@SPEC@") && !launcher.contains("@BIN@"),
        "every placeholder must be substituted at install time"
    );

    let launched = launch(&home, None);
    let url = await_url(&launched);
    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200);
    assert!(
        body.contains("\"authority\": \"authoritative\""),
        "the launched workspace loaded the approved package:\n{body}"
    );
}

#[test]
fn a_missing_document_fails_visibly_instead_of_silently() {
    let home = clean_home("missing");
    install(&home);
    let exec = desktop_value(&home, "Exec");
    let argv = exec_argv(&exec, Some(Path::new("/nonexistent/document.lcl")));
    let stub = stub_opener(&home);

    let output = Command::new(&argv[0])
        .args(&argv[1..])
        .env_clear()
        .env("HOME", &home)
        .env("PATH", format!("{}:/usr/bin:/bin", stub.display()))
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the launcher runs");

    assert!(
        !output.status.success(),
        "a launch that opened nothing must not report success"
    );
    let log = std::fs::read_to_string(home.join(".local/state/lcl/launch.log"))
        .expect("the launcher records why it stopped");
    assert!(
        log.contains("/nonexistent/document.lcl"),
        "the record must name what could not be opened:\n{log}"
    );
    assert!(
        std::fs::read_to_string(home.join("opened-url")).is_err(),
        "a failed launch must not hand a URL to the desktop"
    );
}

#[test]
fn uninstall_removes_the_launcher_and_reinstall_restores_it() {
    let home = clean_home("lifecycle");
    let (payload, _) = install(&home);
    let launcher = home.join(".local/bin/lcl-workspace-launch");
    assert!(launcher.is_file());

    // A project the operator made. Uninstall must not take it away.
    let mine = home.join(".local/share/lcl/workspace/mine.lcl");
    std::fs::create_dir_all(mine.parent().expect("a parent")).expect("writable");
    std::fs::write(&mine, "LCL:\n").expect("writable");

    let output = Command::new(payload.join("uninstall.sh"))
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the uninstaller runs");
    assert!(
        output.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!launcher.exists(), "the launcher survived the uninstall");
    assert!(
        !home.join(".local/share/applications/lcl-workspace.desktop").exists(),
        "the desktop entry survived the uninstall"
    );
    assert!(
        mine.is_file(),
        "uninstall removed the operator's own documents"
    );

    install(&home);
    assert!(launcher.is_file(), "reinstall did not restore the launcher");
    let launched = launch(&home, None);
    await_url(&launched);
}
