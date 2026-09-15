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
use std::os::unix::process::CommandExt;
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

/// One case at a time.
///
/// Each home below holds a staged payload and an installation of it, and a
/// payload carries this build's two binaries. Those are unstripped debug
/// builds, so one home is a few hundred megabytes and eight running at once
/// would fill a `tmpfs`. It did, once, which is why this exists.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A disposable home that removes itself.
///
/// Removal is on `Drop` rather than only at the start of the next run, so a
/// suite that finishes leaves nothing behind. A panicking test still drops its
/// guard while unwinding.
struct Home {
    path: PathBuf,
    _one_at_a_time: std::sync::MutexGuard<'static, ()>,
}

impl Home {
    fn new(case: &str) -> Home {
        // A poisoned lock means an earlier case panicked, which this one has
        // no reason to inherit.
        let guard = ONE_AT_A_TIME
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let path = std::env::temp_dir().join(format!("lcl-launch-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temporary home is writable");
        Home {
            path,
            _one_at_a_time: guard,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl std::ops::Deref for Home {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Home {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<std::ffi::OsStr> for Home {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.path.as_os_str()
    }
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
        (
            packaging.join("lcl.desktop"),
            payload.join("share/lcl.desktop"),
        ),
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
    copy_tree(&packaging.join("icons"), &payload.join("share/icons"));
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
    let output = run_installer(&payload, home);
    (payload, output)
}

/// Run one staged payload's installer against one disposable home.
fn run_installer(payload: &Path, home: &Path) -> Output {
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
    output
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

/// Split one `Exec` value the way the Desktop Entry Specification says to.
///
/// The value is a string first, so `\s`, `\n`, `\t`, `\r` and `\\` are
/// unescaped before anything else: "this escape rule is applied before the
/// quoting rule". Any other string escape is invalid, and GLib refuses to load
/// an entry holding one. Then double quotes group and a backslash inside them
/// escapes the next character. Last come field codes: `%f` carries one file and
/// `%%` is a literal percent sign. Nothing else here needs the full table.
fn exec_argv(exec: &str, document: Option<&Path>) -> Vec<String> {
    let mut unescaped = String::new();
    let mut chars = exec.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            unescaped.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => unescaped.push(' '),
            Some('n') => unescaped.push('\n'),
            Some('t') => unescaped.push('\t'),
            Some('r') => unescaped.push('\r'),
            Some('\\') => unescaped.push('\\'),
            other => panic!("the Exec value holds an invalid string escape {other:?}: {exec}"),
        }
    }

    let mut argv = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = unescaped.chars();
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
            expanded.push(word.replace("%%", "%"));
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
    // Quoted, so a home whose path holds a space is still one redirection.
    let script = format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", recorded.display());
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
        // The launcher is a shell script and the workspace is *its* child, so
        // killing the script on its own leaves a server behind holding a
        // socket and a few hundred megabytes. `launch` puts the script into a
        // new process group, which makes the whole tree addressable at once.
        let group = self.child.id();
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{group}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
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
        // Its own process group, so `Launched::drop` can end the launcher and
        // the workspace it starts together.
        .process_group(0)
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
    let home = Home::new("entry");
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
    let home = Home::new("menu");
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
    let home = Home::new("assoc");
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
    let home = Home::new("spaces");
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
    let home = Home::new("project");
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
    let home = Home::new("nospec");
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
    let home = Home::new("missing");
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
    let home = Home::new("lifecycle");
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
        !home
            .join(".local/share/applications/lcl-workspace.desktop")
            .exists(),
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

// ---------------------------------------------------------------------------
// B1: the installed application icon
// ---------------------------------------------------------------------------

/// Every installed icon file this product owns, under one home.
fn installed_icons(home: &Path) -> Vec<PathBuf> {
    let theme = home.join(".local/share/icons/hicolor");
    let mut found = Vec::new();
    let mut stack = vec![theme];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                stack.push(entry.path());
            } else {
                found.push(entry.path());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn the_desktop_entry_names_an_icon_that_is_actually_installed() {
    let home = Home::new("icon");
    install(&home);

    // A desktop entry names an icon by theme name, not by path, and the theme
    // resolves it. So the assertion is that the name resolves, in the isolated
    // data directory this installation wrote to.
    let name = desktop_value(&home, "Icon");
    assert_eq!(name, "lcl-workspace");
    assert!(
        !name.contains('/'),
        "an Icon key is a theme name, not a path: {name}"
    );

    let theme = home.join(".local/share/icons/hicolor");
    let mut resolved = Vec::new();
    for size in [16, 24, 32, 48, 64, 128, 256] {
        let path = theme.join(format!("{size}x{size}/apps/{name}.png"));
        if path.is_file() {
            resolved.push(path);
        }
    }
    assert!(
        resolved.len() >= 5,
        "the icon name resolved at only {} sizes under {}",
        resolved.len(),
        theme.display()
    );

    // Every one of them is a real decodable image with the dimensions its
    // directory claims, not a placeholder that happens to sit at the path.
    for path in &resolved {
        let bytes = std::fs::read(path).expect("readable");
        assert_eq!(
            &bytes[..8],
            b"\x89PNG\r\n\x1a\n",
            "{} is not a PNG",
            path.display()
        );
        let number = |at: usize| {
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let (width, height, colour) = (number(16), number(20), bytes[25]);
        let declared: u32 = path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .and_then(|n| n.split('x').next())
            .and_then(|n| n.parse().ok())
            .expect("a sized theme directory");
        assert_eq!(
            (width, height),
            (declared, declared),
            "{} does not hold a {declared}-pixel icon",
            path.display()
        );
        assert_eq!(colour, 6, "{} lost its alpha channel", path.display());
    }
}

#[test]
fn the_document_icon_uses_the_same_source_as_the_application() {
    let home = Home::new("docicon");
    install(&home);
    let theme = home.join(".local/share/icons/hicolor");
    // The media type registered by this installation is `text/x-lcl`, and a
    // theme names its icon after the type with the slash replaced.
    let document = theme.join("48x48/mimetypes/text-x-lcl.png");
    let application = theme.join("48x48/apps/lcl-workspace.png");
    assert!(document.is_file(), "no document icon was installed");
    assert_eq!(
        std::fs::read(&document).expect("readable"),
        std::fs::read(&application).expect("readable"),
        "the document and application icons must come from one source"
    );
}

#[test]
fn uninstall_takes_only_this_products_icons() {
    let home = Home::new("iconlife");
    let (payload, _) = install(&home);
    let theme = home.join(".local/share/icons/hicolor");

    let ours = installed_icons(&home);
    assert!(!ours.is_empty(), "nothing was installed to remove");

    // Another application's icon, in the same shared directory. A recursive
    // removal would take it, and that is the failure this asserts against.
    let stranger = theme.join("48x48/apps/someone-elses-app.png");
    std::fs::write(&stranger, b"not ours").expect("writable");

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

    assert!(
        stranger.is_file(),
        "uninstall removed another application's icon from the shared theme"
    );
    for path in ours {
        assert!(!path.exists(), "{} survived the uninstall", path.display());
    }
    assert!(
        theme.join("48x48/apps").is_dir(),
        "a shared theme directory holding another application's icon must stay"
    );

    // Reinstall restores exactly what was removed, alongside the stranger.
    install(&home);
    assert!(theme.join("48x48/apps/lcl-workspace.png").is_file());
    assert!(stranger.is_file());
}

#[test]
fn an_empty_theme_directory_is_removed_but_the_shared_root_is_never_recursive() {
    // With nothing else in the theme, uninstall should leave no empty skeleton
    // behind either. The two cases together state the whole rule: remove our
    // files, remove the directories only we filled, never remove the rest.
    let home = Home::new("icontidy");
    let (payload, _) = install(&home);
    let theme = home.join(".local/share/icons/hicolor");
    assert!(theme.join("256x256/apps/lcl-workspace.png").is_file());

    let output = Command::new(payload.join("uninstall.sh"))
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the uninstaller runs");
    assert!(output.status.success());
    assert!(
        !theme.exists(),
        "an installation that filled the theme by itself should leave none of \
         it behind: {} is still there",
        theme.display()
    );
}

// ---------------------------------------------------------------------------
// The `.lcl.txt` ending, through the installed association
// ---------------------------------------------------------------------------

#[test]
fn a_text_ending_document_opens_through_the_desktop_entry() {
    let home = Home::new("textdoc");
    install(&home);
    // The same source, under the ending a newly created document is given.
    let path = document(&home, "papers/report.lcl.txt");

    let launched = launch(&home, Some(&path));
    let url = await_url(&launched);

    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    assert!(
        body.contains("\"open\": \"report.lcl.txt\""),
        "a .lcl.txt document must open exactly like a .lcl one:\n{body}"
    );

    // And it is listed, so the person can see it in the project.
    let (status, tree) = get(&url, "/api/documents");
    assert_eq!(status, 200);
    assert!(
        tree.contains("report.lcl.txt"),
        "the created ending must appear in the project tree:\n{tree}"
    );
}

#[test]
fn the_installed_media_type_claims_both_endings_and_not_plain_text() {
    let home = Home::new("mime");
    install(&home);

    // `update-mime-database` compiles the installed package into `globs2`,
    // which is the table a desktop consults. Asserting on it rather than on the
    // XML proves the registration survived compilation. This is the *user*
    // table, which only adds: `text/plain` lives in the system database, which
    // this installation never touches.
    let globs = home.join(".local/share/mime/globs2");
    if !globs.is_file() {
        eprintln!("update-mime-database produced no globs2; skipping the compiled check");
        return;
    }
    let table = std::fs::read_to_string(&globs).expect("readable");
    let ours: Vec<&str> = table
        .lines()
        .filter(|line| line.contains("text/x-lcl"))
        .collect();
    assert!(
        ours.iter().any(|l| l.ends_with("*.lcl")),
        "the compiled table does not claim *.lcl: {ours:?}"
    );
    assert!(
        ours.iter().any(|l| l.ends_with("*.lcl.txt")),
        "the compiled table does not claim *.lcl.txt: {ours:?}"
    );
    assert!(
        !ours.iter().any(|l| l.ends_with("*.txt")),
        "LCL must not claim every text file: {ours:?}"
    );
    assert!(
        !table.contains("text/plain"),
        "this installation must not redefine plain text: {table}"
    );
}

#[test]
fn an_ordinary_text_file_is_still_ordinary_after_installation() {
    // The end-to-end question, asked of the desktop's own resolver rather than
    // of a table: which media type does each of these files actually get?
    //
    // `*.lcl.txt` and `*.txt` overlap, and the specification resolves an
    // overlap by the longer pattern, so the answers below are the whole reason
    // the two-part ending is safe to register.
    let home = Home::new("mimequery");
    install(&home);
    if Command::new("xdg-mime")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("xdg-mime is absent; skipping the resolver check");
        return;
    }

    let files = home.join("files");
    std::fs::create_dir_all(&files).expect("writable");
    for (name, contents) in [
        ("a.lcl", "LCL:\n"),
        ("b.lcl.txt", "LCL:\n"),
        ("c.txt", "just text\n"),
        // A plain text file whose bytes happen to begin like a document. The
        // glob decides before the magic rule is reached, so this stays text.
        ("d.txt", "LCL:\n"),
        ("e.md", "# not a document\n"),
    ] {
        std::fs::write(files.join(name), contents).expect("writable");
    }

    let query = |name: &str| -> String {
        let output = Command::new("xdg-mime")
            .args(["query", "filetype"])
            .arg(files.join(name))
            .env("HOME", home.as_ref() as &Path)
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("xdg-mime runs");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    assert_eq!(query("a.lcl"), "text/x-lcl");
    assert_eq!(query("b.lcl.txt"), "text/x-lcl");
    assert_eq!(
        query("c.txt"),
        "text/plain",
        "an ordinary text file must stay an ordinary text file"
    );
    assert_eq!(
        query("d.txt"),
        "text/plain",
        "a .txt file is not an LCL document just because of what is inside it"
    );
    assert_eq!(query("e.md"), "text/markdown");
}

#[test]
fn installation_does_not_take_over_the_operators_default_applications() {
    let home = Home::new("defaults");
    install(&home);
    for path in [
        ".config/mimeapps.list",
        ".local/share/applications/mimeapps.list",
    ] {
        assert!(
            !home.join(path).exists(),
            "{path} was written; installation must not choose default applications"
        );
    }
}

// ---------------------------------------------------------------------------
// LCL-REPAIR-01: install paths, the shared theme and localized documents
// ---------------------------------------------------------------------------

#[test]
fn an_installation_under_a_home_whose_path_holds_spaces_launches() {
    // Regression, B-01. The launcher's assignments were bare words, so an
    // installation path holding a space became a shorter assignment followed by
    // a command, and a menu launch never reached the workspace.
    let home = Home::new("home with spaces");
    install(&home);

    let launched = launch(&home, None);
    let url = await_url(&launched);
    let (status, body) = get(&url, "/api/session");
    assert_eq!(status, 200, "the session route refused: {body}");
    let expected = home.join(".local/share/lcl/workspace");
    assert!(
        body.contains(&format!("\"root\": \"{}\"", expected.display())),
        "the launcher lost the installed default project:\n{body}"
    );
}

/// A path holding what means something to a shell, to `awk -v`, to a desktop
/// entry's string escapes and quoting, and to its field codes.
const HOSTILE: &str = r#"it's "q" $x `y` b\tz & 50% ;*"#;

#[test]
fn install_paths_reach_the_workspace_exactly() {
    // Regression, B-01 and B-02. Each character of HOSTILE broke one layer:
    // the launcher's bare assignments, `awk -v` turning `\t` into a tab and
    // dropping the desktop entry's escapes, or `%` read as a field code.
    let home = Home::new(HOSTILE);
    let payload = stage_payload(&home);
    // A 0.2.0 payload, so the localized package path is substituted as well.
    copy_tree(
        &repository().join("canonical/LCL_Core_0.2.0"),
        &payload.join("share/LCL_Core_0.2.0"),
    );
    run_installer(&payload, &home);

    let bin = home.join(".local/bin");
    let data = home.join(".local/share/lcl");
    let text = |path: &Path| path.display().to_string();
    assert_eq!(
        exec_argv(&desktop_value(&home, "Exec"), None),
        [text(&bin.join("lcl-workspace-launch"))],
        "the desktop entry must name the installed launcher as one argument"
    );

    // The installed workspace becomes a stub recording its arguments, each one
    // NUL-terminated, so what is compared is the exact argument vector.
    let record = home.join("argv");
    std::fs::write(
        bin.join("lcl-workspace"),
        "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$LCL_ARGV_RECORD\"\n",
    )
    .expect("writable");
    let recorded = |file: Option<&Path>| -> Vec<String> {
        let _ = std::fs::remove_file(&record);
        let argv = exec_argv(&desktop_value(&home, "Exec"), file);
        let output = Command::new(&argv[0])
            .args(&argv[1..])
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .env("LCL_ARGV_RECORD", &record)
            .current_dir(std::env::temp_dir())
            .output()
            .expect("the launcher runs");
        assert!(
            output.status.success(),
            "the launcher failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            std::fs::read_to_string(home.join(".local/state/lcl/launch.log")).unwrap_or_default()
        );
        let bytes = std::fs::read(&record).expect("the stub recorded its arguments");
        String::from_utf8(bytes)
            .expect("UTF-8 arguments")
            .split_terminator('\0')
            .map(String::from)
            .collect()
    };

    let head = [
        "--spec".to_string(),
        text(&data.join("LCL_Core_0.1.0")),
        "--open".to_string(),
        "--localized-spec".to_string(),
        text(&data.join("LCL_Core_0.2.0")),
    ];
    assert_eq!(
        recorded(None),
        [&head[..], &[text(&data.join("workspace"))]].concat(),
        "a menu launch must pass every installed path unchanged"
    );
    let doc = document(&home, "doc.lcl");
    assert_eq!(
        recorded(Some(&doc)),
        [&head[..], &["--document".to_string(), text(&doc)]].concat(),
        "a file association must pass every installed path unchanged"
    );
}

#[test]
fn uninstall_leaves_every_theme_directory_it_did_not_fill() {
    // Regression, B-03. Uninstall offered every directory of the shared theme
    // to `rmdir`, so another application's empty directory went with ours.
    let home = Home::new("iconshared");
    let (payload, _) = install(&home);
    let theme = home.join(".local/share/icons/hicolor");
    let strangers = ["scalable/apps", "22x22/apps", "48x48/places"];
    for relative in strangers {
        std::fs::create_dir_all(theme.join(relative)).expect("writable");
    }

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

    for relative in strangers {
        assert!(
            theme.join(relative).is_dir(),
            "uninstall removed {relative}, a theme directory it did not create"
        );
    }
    assert!(
        installed_icons(&home).is_empty(),
        "an icon of this product survived the uninstall"
    );
    for relative in ["48x48/apps", "48x48/mimetypes", "256x256"] {
        assert!(
            !theme.join(relative).exists(),
            "{relative} held only this product's icons and should be gone"
        );
    }
}

#[test]
fn a_localized_document_is_recognised_by_its_first_bytes() {
    // Regression, B-04. The magic rule matched only `LCL:`, but an LCL 0.2.0
    // document may begin with its directive: "@locale, one SPACE, one locale
    // tag and LINE FEED at byte offset zero" (02_LEXICAL/13). Magic decides
    // only where no glob does, so these files have no extension.
    let home = Home::new("mimemagic");
    install(&home);

    let magic = home.join(".local/share/mime/magic");
    if !magic.is_file() {
        eprintln!("update-mime-database produced no magic; skipping the compiled check");
        return;
    }
    let compiled = std::fs::read(&magic).expect("readable");
    for value in ["LCL:", "@locale "] {
        assert!(
            compiled
                .windows(value.len())
                .any(|bytes| bytes == value.as_bytes()),
            "the compiled magic does not match {value:?}"
        );
    }

    if Command::new("gio")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("gio is absent; skipping the resolver check");
        return;
    }
    let files = home.join("files");
    std::fs::create_dir_all(&files).expect("writable");
    let sources =
        repository().join("canonical/LCL_Core_0.2.0/09_CONFORMANCE/LOCALIZATION_FIXTURES/sources");
    for (name, fixture) in [
        ("canonical", "canonical_en.lcl"),
        ("directive", "explicit_lv.lcl"),
        ("detected", "auto_lv.lcl"),
        // A glob still decides first, so a `.txt` file stays plain text.
        ("notes.txt", "explicit_lv.lcl"),
    ] {
        std::fs::copy(sources.join(fixture), files.join(name)).expect("copyable");
    }
    std::fs::write(files.join("prose"), "just text\n").expect("writable");

    let query = |name: &str| -> String {
        let output = Command::new("gio")
            .args(["info", "-a", "standard::content-type"])
            .arg(files.join(name))
            .env_clear()
            .env("HOME", &home)
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("gio runs");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.trim().strip_prefix("standard::content-type: "))
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(query("canonical"), "text/x-lcl");
    assert_eq!(
        query("directive"),
        "text/x-lcl",
        "a document opening with its locale directive is an LCL document"
    );
    assert_eq!(query("detected"), "text/x-lcl");
    assert_eq!(query("notes.txt"), "text/plain");
    assert_eq!(query("prose"), "text/plain");
}
