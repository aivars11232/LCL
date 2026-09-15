//! The release script, run for real against disposable source trees.
//!
//! LCL-CLOSE-03 Phase A, findings B5 and BUILD-SAFETY. `packaging/build_release.sh`
//! recorded a source manifest and then compiled and packaged the live checkout
//! anyway. A failed `git ls-files` was hidden behind a successful pipeline and
//! could leave an empty or partial source set. The output directory was
//! created with `mkdir -p` over whatever was already there, and a directory
//! named by `LCL_BUILD_DIR` was removed with `rm -rf`.
//!
//! Each case copies the real script into a small disposable tree and runs it.
//! `cargo`, `git` and `rustc` are stubs first on `PATH`, and `gzip` is a
//! pass-through that can be told to fail. The stub compiler writes executables
//! that report the source bytes they were compiled from and, when asked, edits
//! the live tree while it compiles, so a case shows which bytes reached the
//! payload in seconds and without a release build.
//!
//! That is the script's handling of its inputs, outputs and failures. It is not
//! the real compiler path: the actual candidate build, its rebuild with no Git
//! metadata and the installed payload are separate LCL-CLOSE-03 gates.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// The name every artifact of the fixture's product carries.
const NAME: &str = "lcl-0.1.0-linux-x86_64";

/// The fixture's one compiled source file, as it was captured.
const CAPTURED: &str = "captured source\n";

/// The commit the stub `git` reports.
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// Enough of `git` for the release script, with every call logged.
const STUB_GIT: &str = r#"#!/bin/sh
if [ "$1" = "-C" ]; then cd "$2" || exit 1; shift 2; fi
echo "git $*" >> "$STUB_LOG"
case "$1" in
ls-files)
    list=$(find . -type f ! -path './.git/*' ! -path './impl/target/*' | sed 's|^\./||')
    if [ -n "${STUB_GIT_EXTRA:-}" ]; then
        list="$list
$STUB_GIT_EXTRA"
    fi
    case "${STUB_GIT_LS_FILES:-}" in
    fail) printf '%s\n' "$list" | head -n 2; exit 128 ;;
    empty) exit 0 ;;
    esac
    printf '%s\n' "$list" | LC_ALL=C sort
    ;;
rev-parse) echo 0123456789abcdef0123456789abcdef01234567 ;;
status | diff) ;;
*) exit 1 ;;
esac
"#;

/// A compiler that writes two executables reporting the source they came from.
///
/// It refuses a target directory that already holds anything, so a build that
/// reused earlier output fails instead of passing.
const STUB_CARGO: &str = r#"#!/bin/sh
echo "cargo $* cwd=$PWD target=${CARGO_TARGET_DIR:-}" >> "$STUB_LOG"
if [ "$1" = "--version" ]; then echo "cargo 0.0.0-stub"; exit 0; fi
if [ -n "${STUB_MUTATE_ORIGIN:-}" ]; then
    echo "mutated after capture" > "$STUB_MUTATE_ORIGIN/impl/marker.rs"
    echo "mutated after capture" > "$STUB_MUTATE_ORIGIN/packaging/install.sh"
fi
if [ -n "${STUB_CARGO_FAIL:-}" ]; then echo "error: stub compile failure" >&2; exit 101; fi
[ -n "${CARGO_TARGET_DIR:-}" ] || exit 97
if [ -d "$CARGO_TARGET_DIR" ] && [ -n "$(ls -A "$CARGO_TARGET_DIR")" ]; then exit 98; fi
mkdir -p "$CARGO_TARGET_DIR/release" || exit 96
source=$(cat marker.rs) || exit 95
{
    echo '#!/bin/sh'
    echo 'case "$1" in'
    echo 'version) echo "lcl 0.1.0"; echo "language 0.1.0"'
    echo '    case " $* " in *" --localized-spec "*) named=yes ;; *) named=${LCL_LOCALIZED_SPEC:+yes} ;; esac'
    echo '    if [ -n "$named" ] || [ -n "${STUB_LCL_CLAIMS_0_2_0:-}" ]; then echo "language 0.2.0"; fi'
    echo '    echo "protocol lcl.engine/1"; echo "compiled-from '"$source"'" ;;'
    echo 'spec) echo "  identity stub-identity" ;;'
    echo 'check) echo "  \"identity_digest\": \"0123abcd\"" ;;'
    echo 'esac'
} > "$CARGO_TARGET_DIR/release/lcl"
chmod 0755 "$CARGO_TARGET_DIR/release/lcl"
if [ "${STUB_CARGO_OMIT:-}" != "lcl-workspace" ]; then
    { echo '#!/bin/sh'; echo 'echo workspace'; } > "$CARGO_TARGET_DIR/release/lcl-workspace"
    chmod 0755 "$CARGO_TARGET_DIR/release/lcl-workspace"
fi
"#;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root is present")
}

/// One case's disposable directory, removed when the case ends.
///
/// Every run of the script gets its `TMPDIR` and `HOME` inside it, so the
/// staging a failed run keeps as evidence goes with the case.
struct Case {
    path: PathBuf,
}

impl Case {
    fn new(name: &str) -> Case {
        let path = std::env::temp_dir().join(format!("lcl-release-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        for dir in ["tmp", "home", "out"] {
            std::fs::create_dir_all(path.join(dir)).expect("the case directory is writable");
        }
        Case { path }
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl std::ops::Deref for Case {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

fn s(path: &Path) -> &str {
    path.to_str().expect("fixture paths are UTF-8")
}

fn write(path: &Path, contents: &str, mode: u32) {
    std::fs::create_dir_all(path.parent().expect("a file has a parent")).expect("writable");
    std::fs::write(path, contents).expect("writable");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .expect("the mode can be set");
}

/// A source tree holding every input the release script reads, with the real
/// script in it and the stub tools beside it.
fn origin(case: &Case) -> PathBuf {
    let root = case.join("origin");
    let script = std::fs::read_to_string(repository().join("packaging/build_release.sh"))
        .expect("the release script is present");
    write(&root.join("packaging/build_release.sh"), &script, 0o755);
    for (path, contents, mode) in [
        (".git/HEAD", "ref: refs/heads/main\n", 0o644),
        (
            "impl/Cargo.toml",
            "[workspace.package]\nversion = \"0.1.0\"\nrust-version = \"1.75\"\n",
            0o644,
        ),
        ("impl/Cargo.lock", "version = 3\n", 0o644),
        ("impl/marker.rs", CAPTURED, 0o644),
        ("impl/target/release/lcl", "#!/bin/sh\necho stale\n", 0o755),
        ("impl/integration/linux/lcl.xml", "<mime-info/>\n", 0o644),
        ("canonical/LCL_Core_0.1.0/VERSION.txt", "0.1.0\n", 0o644),
        ("packaging/install.sh", "#!/bin/sh\necho install\n", 0o755),
        (
            "packaging/uninstall.sh",
            "#!/bin/sh\necho uninstall\n",
            0o755,
        ),
        ("packaging/lcl.desktop", "[Desktop Entry]\n", 0o644),
        ("packaging/lcl-workspace-launch.in", "#!/bin/sh\n", 0o644),
        ("packaging/README.md", "# LCL\n", 0o644),
        (
            "packaging/icons/hicolor/16x16/apps/lcl-workspace.png",
            "icon\n",
            0o644,
        ),
        ("assets/brand/lcl-logo-master.png", "master\n", 0o644),
        (
            "assets/brand/BRAND_ASSETS.sha256",
            "0  assets/brand/lcl-logo-master.png\n",
            0o644,
        ),
        (
            "releases/candidates/earlier/keep.txt",
            "an earlier candidate\n",
            0o644,
        ),
    ] {
        write(&root.join(path), contents, mode);
    }

    let tools = case.join("tools");
    write(&tools.join("git"), STUB_GIT, 0o755);
    write(&tools.join("cargo"), STUB_CARGO, 0o755);
    write(
        &tools.join("rustc"),
        "#!/bin/sh\necho \"rustc 0.0.0-stub\"\n",
        0o755,
    );
    let gzip = Command::new("sh")
        .args(["-c", "command -v gzip"])
        .output()
        .expect("sh runs");
    let gzip = String::from_utf8(gzip.stdout).expect("a UTF-8 path");
    assert!(!gzip.trim().is_empty(), "gzip is not installed");
    write(
        &tools.join("gzip"),
        &format!(
            "#!/bin/sh\nif [ -n \"${{STUB_GZIP_FAIL:-}}\" ]; then echo 'gzip: stub failure' >&2; exit 1; fi\nexec {} \"$@\"\n",
            gzip.trim()
        ),
        0o755,
    );
    root
}

/// One run of the script: what it printed, and what the stub tools recorded.
struct Run {
    output: Output,
    log: String,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn succeeded(&self) {
        assert!(
            self.output.status.success(),
            "the script failed:\n{}\n{}",
            self.stdout(),
            self.stderr()
        );
    }

    fn compiled(&self) -> bool {
        self.log.lines().any(|line| line.starts_with("cargo build"))
    }

    /// Refused, naming `subject`, before compiling and before any candidate.
    fn refused(&self, subject: &str, candidate: &Path) {
        let stderr = self.stderr();
        assert!(
            !self.output.status.success(),
            "accepted:\n{}",
            self.stdout()
        );
        assert!(
            stderr.contains("refused:") && stderr.contains(subject),
            "the refusal does not name {subject}:\n{stderr}"
        );
        assert!(!self.compiled(), "compiled anyway:\n{}", self.log);
        assert!(
            candidate.symlink_metadata().is_err(),
            "a candidate appeared at {}",
            candidate.display()
        );
    }
}

/// Run the release script under `root` with the stub tools first on `PATH`.
///
/// Nothing is inherited but `PATH`, so an `LCL_*` variable in the environment
/// running the tests cannot change a case.
fn run(case: &Case, root: &Path, args: &[&str], env: &[(&str, &str)], label: &str) -> Run {
    let log = case.join(format!("{label}.stub.log"));
    let path = format!(
        "{}:{}",
        case.join("tools").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut command = Command::new("sh");
    command
        .arg(root.join("packaging/build_release.sh"))
        .args(args)
        .env_clear()
        .env("PATH", path)
        .env("HOME", case.join("home"))
        .env("TMPDIR", case.join("tmp"))
        .env("STUB_LOG", &log)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let output = bounded(command.spawn().expect("sh starts"));
    Run {
        output,
        log: std::fs::read_to_string(&log).unwrap_or_default(),
    }
}

/// Wait for the script, killing its whole process group past a fixed bound.
///
/// The group is only signalled while the child is still unreaped, so its id
/// cannot yet belong to anything else.
fn bounded(mut child: Child) -> Output {
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let out = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let err = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the script can be waited on") {
            break status;
        }
        if Instant::now() > deadline {
            let group = format!("-{}", child.id());
            let _ = Command::new("kill")
                .args(["-s", "KILL", "--", &group])
                .status();
            let _ = child.wait();
            panic!("the release script exceeded 120 s and its process group was killed");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: out.join().expect("the stdout reader finishes"),
        stderr: err.join().expect("the stderr reader finishes"),
    }
}

fn build(case: &Case, root: &Path, out: &Path, env: &[(&str, &str)], label: &str) -> Run {
    let mut all = vec![("LCL_RELEASE_OUT", s(out))];
    all.extend_from_slice(env);
    run(case, root, &[], &all, label)
}

fn reconstruct(case: &Case, script_root: &Path, archive: &Path, into: &Path, label: &str) -> Run {
    run(
        case,
        script_root,
        &["--reconstruct", s(archive), s(into)],
        &[],
        label,
    )
}

fn sha256(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum runs");
    assert!(output.status.success(), "cannot hash {}", path.display());
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .expect("a digest")
        .to_owned()
}

/// Run a trusted shell command on files this test owns: the script, then its
/// positional arguments.
fn shell(script_and_args: &[&str]) {
    let status = Command::new("sh")
        .arg("-c")
        .arg(script_and_args[0])
        .arg("sh")
        .args(&script_and_args[1..])
        .status()
        .expect("sh runs");
    assert!(status.success(), "{script_and_args:?} failed");
}

fn files_under(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .expect("readable")
        .flatten()
        .map(|entry| {
            if entry.file_type().expect("typed").is_dir() {
                files_under(&entry.path())
            } else {
                1
            }
        })
        .sum()
}

/// Add one well-formed inventory entry for `path`, keeping the inventory in
/// path order, so the only thing wrong with it is the entry itself.
fn add_entry(export: &Path, path: &str) {
    let file = export.join("SOURCE_INVENTORY.tsv");
    let mut lines: Vec<String> = std::fs::read_to_string(&file)
        .expect("an inventory")
        .lines()
        .map(str::to_owned)
        .collect();
    lines.push(format!("file\t0644\t6\t{}\t{path}", "0".repeat(64)));
    lines.sort_by(|a, b| a.rsplit('\t').next().cmp(&b.rsplit('\t').next()));
    write(&file, &(lines.join("\n") + "\n"), 0o644);
}

#[test]
fn a_live_change_after_capture_reaches_neither_the_build_nor_the_payload() {
    let case = Case::new("live");
    let root = origin(&case);
    let install_sh = std::fs::read(root.join("packaging/install.sh")).expect("readable");
    let candidate = case.join("out/candidate");

    let built = build(
        &case,
        &root,
        &candidate,
        &[("STUB_MUTATE_ORIGIN", s(&root))],
        "build",
    );
    built.succeeded();
    assert_eq!(
        std::fs::read_to_string(root.join("impl/marker.rs")).expect("readable"),
        "mutated after capture\n",
        "the live tree was not actually changed during the build"
    );
    let compile = built
        .log
        .lines()
        .find(|line| line.starts_with("cargo build"))
        .expect("the compiler ran");
    assert!(
        !compile.contains(s(&root)),
        "compiled inside the live tree: {compile}"
    );

    let unpacked = case.join("unpacked");
    std::fs::create_dir_all(&unpacked).expect("writable");
    shell(&[
        r#"tar -xzf "$1" -C "$2""#,
        s(&candidate.join(format!("{NAME}.tar.gz"))),
        s(&unpacked),
    ]);
    let payload = unpacked.join(NAME);
    let version = Command::new(payload.join("bin/lcl"))
        .arg("version")
        .output()
        .expect("the payload's lcl runs");
    let version = String::from_utf8_lossy(&version.stdout);
    assert!(
        version.contains("compiled-from captured source"),
        "the payload was compiled from other bytes: {version}"
    );
    assert_eq!(
        std::fs::read(payload.join("install.sh")).expect("readable"),
        install_sh,
        "the payload copied the live install.sh"
    );

    let member = Command::new("tar")
        .arg("-xzOf")
        .arg(candidate.join(format!("{NAME}-source.tar.gz")))
        .arg("impl/marker.rs")
        .output()
        .expect("tar runs");
    assert_eq!(
        member.stdout,
        CAPTURED.as_bytes(),
        "the source archive holds other bytes"
    );

    let captured = case.join("captured.rs");
    write(&captured, CAPTURED, 0o644);
    let expected = format!(
        "file\t0644\t{}\t{}\timpl/marker.rs",
        CAPTURED.len(),
        sha256(&captured)
    );
    let inventory = std::fs::read_to_string(candidate.join("SOURCE_INVENTORY.tsv"))
        .expect("the candidate has an inventory");
    assert!(
        inventory.lines().any(|line| line == expected),
        "{inventory}"
    );
    assert!(
        inventory.lines().any(
            |line| line.starts_with("file\t0755\t") && line.ends_with("\tpackaging/install.sh")
        ),
        "an executable was not inventoried as one:\n{inventory}"
    );
    assert!(
        !inventory.contains("releases/") && !inventory.contains("impl/target/"),
        "an output was inventoried as source:\n{inventory}"
    );

    assert_eq!(
        std::fs::read_to_string(root.join("releases/candidates/earlier/keep.txt"))
            .expect("readable"),
        "an earlier candidate\n"
    );
    assert_eq!(
        std::fs::read_dir(case.join("tmp"))
            .expect("readable")
            .count(),
        0,
        "a successful build left its staging behind"
    );
}

#[test]
fn a_source_export_reconstructs_and_rebuilds_with_no_git_metadata() {
    let case = Case::new("export");
    let root = origin(&case);
    let first = case.join("out/first");
    build(&case, &root, &first, &[], "first").succeeded();
    let inventory =
        std::fs::read(first.join("SOURCE_INVENTORY.tsv")).expect("the candidate has an inventory");
    let entries = inventory.iter().filter(|byte| **byte == b'\n').count();
    let id = sha256(&first.join("SOURCE_INVENTORY.tsv"));

    let export = case.join("export");
    let reconstructed = reconstruct(
        &case,
        &root,
        &first.join(format!("{NAME}-source.tar.gz")),
        &export,
        "reconstruct",
    );
    reconstructed.succeeded();
    assert!(
        reconstructed.stdout().contains(&id),
        "the reconstruction did not report source {id}:\n{}",
        reconstructed.stdout()
    );
    assert!(
        export.join(".git").symlink_metadata().is_err(),
        "the export carries Git metadata"
    );
    assert_eq!(
        std::fs::read(export.join("impl/marker.rs")).expect("readable"),
        CAPTURED.as_bytes()
    );
    let mode = std::fs::metadata(export.join("packaging/install.sh"))
        .expect("present")
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111, "an executable lost its mode");
    assert_eq!(
        files_under(&export),
        entries + 1,
        "the export holds other files than its inventory names"
    );

    let second = case.join("out/second");
    let rebuilt = build(&case, &export, &second, &[], "second");
    rebuilt.succeeded();
    assert!(
        !rebuilt.log.lines().any(|line| line.starts_with("git ")),
        "a build with no Git metadata called git:\n{}",
        rebuilt.log
    );
    assert_eq!(
        std::fs::read(second.join("SOURCE_INVENTORY.tsv")).expect("an inventory"),
        inventory,
        "the rebuild is not bound to the same source"
    );
    assert_eq!(
        files_under(&export),
        entries + 1,
        "the build wrote into the export"
    );

    let recorded =
        std::fs::read_to_string(first.join("lcl-0.1.0-PROVENANCE.txt")).expect("provenance");
    assert!(recorded.contains(COMMIT), "{recorded}");
    let rebuilt_provenance =
        std::fs::read_to_string(second.join("lcl-0.1.0-PROVENANCE.txt")).expect("provenance");
    assert!(
        rebuilt_provenance.contains("source export without git metadata"),
        "{rebuilt_provenance}"
    );
    assert!(
        !rebuilt_provenance.contains(COMMIT),
        "a build with no Git metadata claimed a commit:\n{rebuilt_provenance}"
    );
}

/// LCL-REPAIR-02, finding B-05: a candidate's provenance names only the
/// languages the built tool reports with exactly the packages its payload
/// carries, so an inherited `LCL_LOCALIZED_SPEC` adds nothing.
#[test]
fn an_inherited_localized_spec_adds_no_language_to_a_0_1_0_candidate() {
    let case = Case::new("inherited");
    let root = origin(&case);
    let candidate = case.join("out/candidate");
    build(
        &case,
        &root,
        &candidate,
        &[("LCL_LOCALIZED_SPEC", "/elsewhere/LCL_Core_0.2.0")],
        "inherited",
    )
    .succeeded();
    let recorded =
        std::fs::read_to_string(candidate.join("lcl-0.1.0-PROVENANCE.txt")).expect("provenance");
    assert!(recorded.contains("language version: 0.1.0\n"), "{recorded}");
}

#[test]
fn a_tool_claiming_a_language_its_payload_lacks_is_refused() {
    let case = Case::new("claimed");
    let root = origin(&case);
    let candidate = case.join("out/candidate");
    let claimed = build(
        &case,
        &root,
        &candidate,
        &[("STUB_LCL_CLAIMS_0_2_0", "yes")],
        "claimed",
    );
    assert!(
        !claimed.output.status.success(),
        "recorded a language the payload does not carry:\n{}",
        claimed.stdout()
    );
    assert!(
        claimed.stderr().contains("refused:") && claimed.stderr().contains("language"),
        "{}",
        claimed.stderr()
    );
    assert!(
        candidate.symlink_metadata().is_err(),
        "a candidate was published"
    );
}

#[test]
fn a_0_2_0_candidate_records_both_languages() {
    let case = Case::new("localized");
    let root = origin(&case);
    write(
        &root.join("canonical/LCL_Core_0.2.0/VERSION.txt"),
        "0.2.0\n",
        0o644,
    );
    let candidate = case.join("out/candidate");
    build(
        &case,
        &root,
        &candidate,
        &[("LCL_RELEASE_VERSION", "0.2.0")],
        "localized",
    )
    .succeeded();
    let recorded =
        std::fs::read_to_string(candidate.join("lcl-0.2.0-PROVENANCE.txt")).expect("provenance");
    assert!(
        recorded.contains("language version: 0.1.0 0.2.0\n"),
        "{recorded}"
    );
}

#[test]
fn a_failed_empty_incomplete_or_unreadable_source_set_is_refused_before_building() {
    for (label, env, unreadable, subject) in [
        (
            "enumeration",
            ("STUB_GIT_LS_FILES", "fail"),
            false,
            "git ls-files",
        ),
        (
            "empty",
            ("STUB_GIT_LS_FILES", "empty"),
            false,
            "no source files",
        ),
        (
            "missing",
            ("STUB_GIT_EXTRA", "impl/deleted.rs"),
            false,
            "impl/deleted.rs",
        ),
        ("unreadable", ("STUB_NOTHING", ""), true, "impl/marker.rs"),
    ] {
        let case = Case::new(&format!("sources-{label}"));
        let root = origin(&case);
        if unreadable {
            std::fs::set_permissions(
                root.join("impl/marker.rs"),
                std::fs::Permissions::from_mode(0o000),
            )
            .expect("the mode can be set");
        }
        let candidate = case.join("out/candidate");
        build(&case, &root, &candidate, &[env], label).refused(subject, &candidate);
    }
}

#[test]
fn a_changed_incomplete_or_unsafe_export_is_refused_before_building() {
    type Tamper = fn(&Path);
    let case = Case::new("tamper");
    let root = origin(&case);
    let first = case.join("out/first");
    build(&case, &root, &first, &[], "first").succeeded();
    let archive = first.join(format!("{NAME}-source.tar.gz"));

    let cases: [(&str, &str, Tamper); 9] = [
        ("changed", "impl/marker.rs", |export| {
            write(&export.join("impl/marker.rs"), "changed source\n", 0o644)
        }),
        ("unexpected", "impl/extra.rs", |export| {
            write(&export.join("impl/extra.rs"), "extra\n", 0o644)
        }),
        ("missing", "packaging/README.md", |export| {
            std::fs::remove_file(export.join("packaging/README.md")).expect("removable")
        }),
        ("mode", "impl/marker.rs", |export| {
            std::fs::set_permissions(
                export.join("impl/marker.rs"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("the mode can be set")
        }),
        ("empty-inventory", "SOURCE_INVENTORY.tsv", |export| {
            write(&export.join("SOURCE_INVENTORY.tsv"), "", 0o644)
        }),
        ("no-inventory", "SOURCE_INVENTORY.tsv", |export| {
            std::fs::remove_file(export.join("SOURCE_INVENTORY.tsv")).expect("removable")
        }),
        ("absolute", "/etc/hostname", |export| {
            add_entry(export, "/etc/hostname")
        }),
        ("traversal", "../outside", |export| {
            add_entry(export, "../outside")
        }),
        ("duplicate", "impl/marker.rs", |export| {
            add_entry(export, "impl/marker.rs")
        }),
    ];
    for (label, subject, tamper) in cases {
        let export = case.join(format!("export-{label}"));
        reconstruct(
            &case,
            &root,
            &archive,
            &export,
            &format!("reconstruct-{label}"),
        )
        .succeeded();
        tamper(&export);
        let candidate = case.join(format!("out/{label}"));
        build(&case, &export, &candidate, &[], label).refused(subject, &candidate);
    }
}

#[test]
fn unsafe_archive_members_are_refused_and_nothing_is_extracted() {
    let case = Case::new("members");
    let root = origin(&case);
    let first = case.join("out/first");
    build(&case, &root, &first, &[], "first").succeeded();
    let plain = case.join("plain.tar");
    shell(&[
        r#"gzip -dc "$1" > "$2""#,
        s(&first.join(format!("{NAME}-source.tar.gz"))),
        s(&plain),
    ]);

    write(&case.join("absolute.txt"), "absolute\n", 0o644);
    write(&case.join("work/escape.txt"), "escape\n", 0o644);
    std::fs::create_dir_all(case.join("work/inner")).expect("writable");
    write(&case.join("dup/impl/marker.rs"), "second copy\n", 0o644);
    std::fs::create_dir_all(case.join("outside")).expect("writable");
    std::fs::create_dir_all(case.join("link")).expect("writable");
    std::os::unix::fs::symlink(case.join("outside"), case.join("link/evil"))
        .expect("a symlink can be made");
    write(&case.join("through/evil/pwned.txt"), "pwned\n", 0o644);

    for (label, append) in [
        ("absolute", r#"tar -rPf "$1" "$2/absolute.txt""#),
        (
            "traversal",
            r#"tar -rPf "$1" -C "$2/work/inner" ../escape.txt"#,
        ),
        ("duplicate", r#"tar -rf "$1" -C "$2/dup" impl/marker.rs"#),
        (
            "link",
            r#"tar -rf "$1" -C "$2/link" evil && tar -rf "$1" -C "$2/through" evil/pwned.txt"#,
        ),
    ] {
        let tar = case.join(format!("{label}.tar"));
        std::fs::copy(&plain, &tar).expect("copyable");
        shell(&[append, s(&tar), s(&case)]);
        shell(&[r#"gzip -c "$1" > "$1.gz""#, s(&tar)]);
        let into = case.join(format!("reconstructed-{label}"));
        let refused = reconstruct(
            &case,
            &root,
            &case.join(format!("{label}.tar.gz")),
            &into,
            label,
        );
        assert!(
            !refused.output.status.success(),
            "{label}: accepted:\n{}",
            refused.stdout()
        );
        assert!(
            refused.stderr().contains("refused:"),
            "{label}: {}",
            refused.stderr()
        );
        assert!(
            into.symlink_metadata().is_err(),
            "{label}: extracted into {}",
            into.display()
        );
    }
    assert!(
        case.join("escape.txt").symlink_metadata().is_err(),
        "a traversal member escaped"
    );
    assert!(
        case.join("outside/pwned.txt").symlink_metadata().is_err(),
        "a member was written through a link"
    );
    assert_eq!(
        std::fs::read_to_string(case.join("absolute.txt")).expect("readable"),
        "absolute\n"
    );

    shell(&[r#"gzip -c "$1" > "$1.gz""#, s(&plain)]);
    reconstruct(
        &case,
        &root,
        &case.join("plain.tar.gz"),
        &case.join("reconstructed-control"),
        "control",
    )
    .succeeded();
}

#[test]
fn a_failed_compile_copy_or_archive_step_is_visible_and_publishes_nothing() {
    for (label, env) in [
        ("compile", ("STUB_CARGO_FAIL", "1")),
        ("copy", ("STUB_CARGO_OMIT", "lcl-workspace")),
        ("archive", ("STUB_GZIP_FAIL", "1")),
    ] {
        let case = Case::new(&format!("failure-{label}"));
        let root = origin(&case);
        let candidate = case.join("out/candidate");
        let failed = build(&case, &root, &candidate, &[env], label);
        assert!(
            !failed.output.status.success(),
            "{label}: reported success:\n{}",
            failed.stdout()
        );
        assert!(
            candidate.symlink_metadata().is_err(),
            "{label}: a candidate was published"
        );
        assert!(
            !failed.stdout().contains("wrote "),
            "{label}: claimed an artifact:\n{}",
            failed.stdout()
        );
        assert!(
            failed.stderr().contains("evidence kept in"),
            "{label}: {}",
            failed.stderr()
        );
        assert_eq!(
            std::fs::read_dir(case.join("tmp"))
                .expect("readable")
                .count(),
            1,
            "{label}: the staging evidence was not kept"
        );
    }
}

#[test]
fn removed_options_and_unusable_outputs_are_refused_without_touching_anything() {
    let case = Case::new("outputs");
    let root = origin(&case);
    let foreign = case.join("foreign");
    write(&foreign.join("keep.txt"), "not the script's\n", 0o644);
    std::os::unix::fs::symlink(case.join("nowhere"), case.join("dangling"))
        .expect("a symlink can be made");
    let files_before = files_under(&root);

    for (key, value) in [("LCL_BUILD_DIR", s(&foreign)), ("LCL_KEEP_BUILD", "1")] {
        let refused = run(&case, &root, &[], &[(key, value)], key);
        assert!(
            !refused.output.status.success(),
            "{key}: accepted:\n{}",
            refused.stdout()
        );
        assert!(
            refused.stderr().contains("refused:") && refused.stderr().contains(key),
            "{key}: {}",
            refused.stderr()
        );
        assert!(!refused.compiled(), "{key}: compiled anyway");
    }

    for (label, out) in [
        (
            "earlier-candidate",
            root.join("releases/candidates/earlier"),
        ),
        ("source-tree", root.clone()),
        ("home", case.join("home")),
        ("dangling-link", case.join("dangling")),
        ("missing-parent", case.join("missing/candidate")),
        ("inside-source", root.join("packaging/candidate")),
    ] {
        let refused = build(&case, &root, &out, &[], label);
        assert!(
            !refused.output.status.success(),
            "{label}: accepted:\n{}",
            refused.stdout()
        );
        assert!(
            refused.stderr().contains("refused:"),
            "{label}: {}",
            refused.stderr()
        );
        assert!(!refused.compiled(), "{label}: compiled anyway");
    }

    assert_eq!(
        std::fs::read_to_string(foreign.join("keep.txt")).expect("the foreign directory survived"),
        "not the script's\n"
    );
    assert_eq!(
        files_under(&root),
        files_before,
        "a refused run wrote into the source tree"
    );
    assert_eq!(
        std::fs::read_dir(case.join("home"))
            .expect("readable")
            .count(),
        0,
        "a refused run wrote into the home directory"
    );
    assert!(case.join("nowhere").symlink_metadata().is_err());
    assert!(case.join("missing").symlink_metadata().is_err());
}
