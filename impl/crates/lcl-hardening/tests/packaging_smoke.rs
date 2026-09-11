//! Phase E: install it somewhere clean, use it, remove it.
//!
//! `LCL_RELEASE_BASELINE.md` 5.6. The question is not whether the build works
//! here; it is whether the *payload* works anywhere. So the install goes into a
//! temporary home, every command runs with an empty environment and a working
//! directory outside the repository, and one test greps every installed file
//! for the repository path: an artifact that still knows where it was built is
//! not installable.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root is present")
}

/// The most recent release tarball, which `packaging/build_release.sh` wrote.
fn tarball() -> Option<PathBuf> {
    let releases = repository().join("releases");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(releases)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("lcl-") && n.ends_with("-linux-x86_64.tar.gz"))
        })
        .collect();
    candidates.sort();
    candidates.pop()
}

/// A temporary home, with nothing in it.
fn clean_home(case: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("lcl-install-{case}"));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("the temporary home is writable");
    home
}

/// Unpack the payload into `home` and run its installer against that home.
fn install(home: &Path) -> (PathBuf, Output) {
    let tarball = tarball().expect("run packaging/build_release.sh first");
    let unpacked = home.join("payload");
    std::fs::create_dir_all(&unpacked).expect("writable");
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&unpacked)
        .status()
        .expect("tar runs");
    assert!(status.success(), "the tarball did not unpack");

    let payload = std::fs::read_dir(&unpacked)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("the tarball holds one directory");

    let output = Command::new(payload.join("install.sh"))
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the installer runs");
    (payload, output)
}

/// One installed command, with an empty environment beyond the home.
fn installed(home: &Path, args: &[&str]) -> Output {
    Command::new(home.join(".local/bin/lcl"))
        .args(args)
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("LCL_SPEC", home.join(".local/share/lcl/LCL_Core_0.1.0"))
        .current_dir(std::env::temp_dir())
        .output()
        .expect("the installed binary runs")
}

#[test]
fn a_clean_install_puts_everything_where_it_said_it_would() {
    let home = clean_home("paths");
    let (_, output) = install(&home);
    assert!(
        output.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for path in [
        ".local/bin/lcl",
        ".local/bin/lcl-workspace",
        ".local/share/lcl/LCL_Core_0.1.0/VERSION.txt",
        ".local/share/applications/lcl-workspace.desktop",
        ".local/share/mime/packages/lcl.xml",
    ] {
        assert!(home.join(path).exists(), "{path} was not installed");
    }
}

#[test]
fn the_installed_tool_runs_with_no_environment_and_no_repository() {
    let home = clean_home("runs");
    install(&home);

    let version = installed(&home, &["version"]);
    assert!(version.status.success());
    let text = String::from_utf8_lossy(&version.stdout);
    assert!(text.contains("lcl 0.1.0"), "{text}");
    assert!(text.contains("language 0.1.0"), "{text}");

    // The package it loads is the approved one, verified by its own anchor.
    let spec = installed(&home, &["spec"]);
    assert!(
        spec.status.success(),
        "{}",
        String::from_utf8_lossy(&spec.stderr)
    );
    let text = String::from_utf8_lossy(&spec.stdout);
    assert!(text.contains("authoritative"), "{text}");
    assert!(
        text.contains(lcl_spec::APPROVED_PACKAGE.identity_digest),
        "the installed package is not the approved one: {text}"
    );
}

#[test]
fn the_installed_tool_judges_a_document_the_same_way() {
    let home = clean_home("judges");
    install(&home);
    let document = home.join("example.lcl");
    std::fs::write(
        &document,
        std::fs::read(
            repository().join("canonical/LCL_Core_0.1.0/08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"),
        )
        .expect("the example is readable"),
    )
    .expect("writable");

    let run = installed(&home, &["run", document.to_str().expect("utf-8")]);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the installed tool did not run the example: {}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("status.succeeded"));
}

#[test]
fn nothing_installed_refers_to_where_it_was_built() {
    let home = clean_home("paths-clean");
    install(&home);
    let repository = repository();
    let needle = repository.to_str().expect("utf-8").as_bytes();
    let mut checked = 0usize;
    let mut offenders = Vec::new();
    let mut stack = vec![home.join(".local")];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            // The specification package is the release's own content and is
            // installed verbatim; it names no build path, and re-reading every
            // one of its 176 files here would be measuring the package rather
            // than the install.
            if path.to_string_lossy().contains("LCL_Core_0.1.0") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("readable");
            checked += 1;
            if bytes.windows(needle.len()).any(|window| window == needle) {
                offenders.push(path);
            }
        }
    }
    assert!(checked > 0, "nothing was checked");
    assert!(
        offenders.is_empty(),
        "installed files still name the build directory: {offenders:?}"
    );
}

#[test]
fn uninstall_removes_everything_and_reinstall_works() {
    let home = clean_home("uninstall");
    let (payload, _) = install(&home);

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

    for path in [
        ".local/bin/lcl",
        ".local/bin/lcl-workspace",
        ".local/share/lcl/LCL_Core_0.1.0",
        ".local/share/applications/lcl-workspace.desktop",
        ".local/share/mime/packages/lcl.xml",
    ] {
        assert!(!home.join(path).exists(), "{path} survived the uninstall");
    }

    // Recovery: installing again over the removed state works.
    let (_, output) = install(&home);
    assert!(
        output.status.success(),
        "reinstall failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.join(".local/bin/lcl").exists());
}
