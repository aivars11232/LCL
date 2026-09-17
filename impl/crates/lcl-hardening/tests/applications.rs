//! Phase D: the staged application ladder, and the two ways to write the same
//! program.
//!
//! Four projects under `apps/`, each a real project with a manifest, run
//! through the real binary with the capabilities it declares and nothing more:
//!
//! * **small** — one document: inputs, exact arithmetic, two outputs, two
//!   verifications and one terminal status.
//! * **medium** — a project: an imported rule library, a typed object context,
//!   a bounded iteration, evidence, and one granted filesystem effect.
//! * **large** — two phases, an authority override, a retry handler, a declared
//!   domain extension, a ranged read, and a five-part success condition.
//! * **large, decomposed** — the same work, written as two sub-tasks composed
//!   by a parent phase, with the types and rules in imported libraries.
//!
//! The last pair is the comparison the task file asks for: whole-application
//! LCL against task-decomposed LCL, on semantics, diagnostics, outputs,
//! evidence and repeatability.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root is present")
}

fn binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("the test executable has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("lcl")
}

/// A private workspace for one application, emptied before each run.
/// One application at a time.
///
/// An application declares its `WORKSPACE` path in its own source, because
/// `05_SEMANTICS/02` leaves no ambient directory for it to inherit. Two tests
/// driving the same application therefore share one directory on disk, and
/// running them at once would have each emptying the other's workspace.
static WORKSPACES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The absolute `WORKSPACE` prefix the applications declare in their own
/// source. `04_GRAMMAR/08` requires a WORKSPACE PATH to be absolute, so an
/// application in the repository has to name one; a test run never uses it.
const DECLARED_WORKSPACES: &str = "/tmp/lcl-apps/";

/// Everything this test process writes: under the temporary directory the run
/// was given, and private to this process.
fn stage() -> PathBuf {
    std::env::temp_dir().join(format!("lcl-apps-{}", std::process::id()))
}

fn workspace(app: &str) -> PathBuf {
    let root = stage().join("workspaces").join(app);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the workspace is writable");
    root
}

/// A private copy of one application whose declared WORKSPACE is under
/// [`stage`], with nothing else changed.
///
/// Each copy sits beside a link to the repository's `canonical/`, so the
/// manifest's relative package path resolves exactly as it does in the
/// repository.
fn project(app: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let copy = stage().join(format!(
        "copy-{}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&copy);
    std::fs::create_dir_all(copy.join("apps")).expect("writable");
    std::os::unix::fs::symlink(repository().join("canonical"), copy.join("canonical"))
        .expect("the package link");
    let copied = Command::new("cp")
        .arg("-R")
        .arg(repository().join("apps").join(app))
        .arg(copy.join("apps"))
        .status()
        .expect("cp runs");
    assert!(copied.success(), "could not copy {app}");

    let workspaces = format!("{}/", stage().join("workspaces").display());
    assert!(
        !workspaces.contains(['"', '\\']),
        "the temporary directory cannot be spelled in an LCL string: {workspaces}"
    );
    let project = copy.join("apps").join(app);
    for entry in std::fs::read_dir(project.join("src")).expect("sources") {
        let path = entry.expect("an entry").path();
        let text = std::fs::read_to_string(&path).expect("a source");
        if text.contains(DECLARED_WORKSPACES) {
            std::fs::write(&path, text.replace(DECLARED_WORKSPACES, &workspaces))
                .expect("writable");
        }
    }
    project
}

/// Run one application, granting exactly what its own declaration needs.
///
/// `env_clear` and a working directory outside the project prove the tool
/// discovered nothing: the project root comes from the argument, and the
/// specification package from the manifest.
fn run(app: &str, grants: &[(&str, PathBuf)]) -> String {
    let project = project(app);
    let mut command = Command::new(binary());
    command
        .arg("run")
        .arg("--machine")
        .arg("--project")
        .arg(&project)
        .current_dir(std::env::temp_dir())
        .env_clear();
    for (flag, path) in grants {
        command.arg(flag).arg(path);
    }
    let output = command.output().expect("the binary runs");
    // The copy, not the package its link names: removal never follows a link.
    let _ = std::fs::remove_dir_all(project.parent().and_then(Path::parent).expect("a copy"));
    String::from_utf8(output.stdout).expect("machine output is UTF-8")
}

/// The fields a comparison is made on: what the engine decided, not how it was
/// written.
#[derive(Debug, PartialEq, Eq)]
struct Observable {
    reached: String,
    outcome: String,
    terminal_status: Option<String>,
    /// Check id to recorded outcome.
    checks: BTreeMap<String, String>,
    /// Output id to published value.
    outputs: BTreeMap<String, String>,
    /// Evidence id to whether it resolved.
    evidence: BTreeMap<String, bool>,
    diagnostics: Vec<String>,
}

fn observable(machine: &str) -> Observable {
    let json = lcl_spec::json::parse(machine).expect("the machine output is JSON");
    let text = |node: Option<&lcl_spec::json::Json>| {
        node.and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let completion = json.get("completion");
    let mut checks = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    let mut evidence = BTreeMap::new();
    if let Some(completion) = completion {
        for entry in completion
            .get("checks")
            .and_then(|c| c.as_array())
            .unwrap_or(&[])
        {
            checks.insert(text(entry.get("id")), text(entry.get("outcome")));
        }
        for entry in completion
            .get("outputs")
            .and_then(|o| o.as_array())
            .unwrap_or(&[])
        {
            outputs.insert(text(entry.get("id")), text(entry.get("value")));
        }
        for entry in completion
            .get("evidence")
            .and_then(|e| e.as_array())
            .unwrap_or(&[])
        {
            // `provision` says how the evidence resolved: by VALUE, by SOURCE,
            // or not at all.
            let provision = text(entry.get("provision"));
            evidence.insert(text(entry.get("id")), !provision.is_empty());
        }
    }
    Observable {
        reached: text(json.get("reached")),
        outcome: text(json.get("outcome")),
        terminal_status: completion
            .and_then(|c| c.get("terminal_status"))
            .and_then(|s| s.as_str())
            .map(str::to_string),
        checks,
        outputs,
        evidence,
        diagnostics: json
            .get("diagnostics")
            .and_then(|d| d.as_array())
            .unwrap_or(&[])
            .iter()
            .map(|d| text(d.get("id")))
            .collect(),
    }
}

fn assert_succeeded(app: &str, observed: &Observable) {
    assert_eq!(
        observed.terminal_status.as_deref(),
        Some("status.succeeded"),
        "{app} did not succeed: {:?}",
        observed.diagnostics
    );
    assert!(
        observed.diagnostics.is_empty(),
        "{app} succeeded with diagnostics: {:?}",
        observed.diagnostics
    );
    assert!(!observed.checks.is_empty(), "{app} recorded no check");
    for (id, outcome) in &observed.checks {
        assert_eq!(outcome, "TRUE", "{app} check {id} was {outcome}");
    }
}

/// LCL-REPAIR-02, finding B-08: each manifest names its package relative to
/// the project root, which `lcl-project` joins to the root and never to a
/// working directory, so a checkout finds its own package wherever it lives.
#[test]
fn every_application_names_its_package_relative_to_its_root() {
    let package = repository()
        .join("canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("the package is present");
    for app in [
        "small-invoice-total",
        "medium-release-notes",
        "large-release-pipeline",
        "large-release-pipeline-decomposed",
    ] {
        let root = repository().join("apps").join(app);
        let manifest = std::fs::read_to_string(root.join("lcl.project.json")).expect("a manifest");
        let json = lcl_spec::json::parse(&manifest).expect("the manifest is JSON");
        let spec = json.get("spec").and_then(|s| s.as_str()).expect("a spec");
        assert!(!Path::new(spec).is_absolute(), "{app}: {spec}");
        assert_eq!(
            root.join(spec).canonicalize().ok(),
            Some(package.clone()),
            "{app}: {spec}"
        );
    }
}

/// A copy of one application and its package under another absolute root
/// loads the copied package, not the one it was copied from.
#[test]
fn a_moved_checkout_loads_its_own_package() {
    let moved = std::env::temp_dir().join(format!("lcl-moved-checkout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&moved);
    for (from, into) in [
        ("apps/small-invoice-total", "apps"),
        ("canonical/LCL_Core_0.1.0", "canonical"),
    ] {
        std::fs::create_dir_all(moved.join(into)).expect("writable");
        let copied = Command::new("cp")
            .arg("-R")
            .arg(repository().join(from))
            .arg(moved.join(into))
            .status()
            .expect("cp runs");
        assert!(copied.success(), "could not copy {from}");
    }

    let output = Command::new(binary())
        .arg("check")
        .arg("--machine")
        .arg("--project")
        .arg(moved.join("apps/small-invoice-total"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .output()
        .expect("the binary runs");
    let machine = String::from_utf8_lossy(&output.stdout).into_owned();
    let loaded = lcl_spec::json::parse(&machine)
        .ok()
        .and_then(|json| Some(json.get("spec")?.get("root")?.as_str()?.to_string()))
        .and_then(|root| Path::new(&root).canonicalize().ok());
    let expected = moved.join("canonical/LCL_Core_0.1.0").canonicalize().ok();
    let _ = std::fs::remove_dir_all(&moved);
    assert!(
        output.status.success(),
        "{machine}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        expected.is_some() && loaded == expected,
        "loaded {loaded:?}, not {expected:?}:\n{machine}"
    );
}

#[test]
fn the_small_application_succeeds() {
    let observed = observable(&run("small-invoice-total", &[]));
    assert_succeeded("small", &observed);
    assert_eq!(
        observed.outputs.get("output.subtotal").map(String::as_str),
        Some("5525")
    );
    assert_eq!(
        observed.outputs.get("output.total").map(String::as_str),
        Some("6630")
    );
}

#[test]
fn the_medium_application_succeeds_and_performs_its_one_granted_effect() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    let root = workspace("medium-release-notes");
    let observed = observable(&run(
        "medium-release-notes",
        &[("--allow-write", root.clone())],
    ));
    assert_succeeded("medium", &observed);
    assert_eq!(
        observed.outputs.get("output.headline").map(String::as_str),
        Some("\"LCL\"")
    );
    assert!(
        root.join("NOTES.txt").exists(),
        "the medium application's declared effect did not happen"
    );
    assert_eq!(
        observed.evidence.get("evidence.headline"),
        Some(&true),
        "declared evidence did not resolve"
    );
}

#[test]
fn the_large_application_succeeds() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    let root = workspace("large-release-pipeline");
    let observed = observable(&run(
        "large-release-pipeline",
        &[
            ("--allow-write", root.clone()),
            ("--allow-read", root.clone()),
        ],
    ));
    assert_succeeded("large", &observed);
    // The ranged read: three scalars of "lcl 0.1.0".
    assert_eq!(
        observed.outputs.get("output.head").map(String::as_str),
        Some("\"lcl\"")
    );
    assert!(root.join("MANIFEST.txt").exists());
}

#[test]
fn the_decomposed_application_succeeds() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    let root = workspace("large-release-pipeline-decomposed");
    let observed = observable(&run(
        "large-release-pipeline-decomposed",
        &[
            ("--allow-write", root.clone()),
            ("--allow-read", root.clone()),
        ],
    ));
    assert_succeeded("decomposed", &observed);
    assert!(root.join("MANIFEST.txt").exists());
}

#[test]
fn whole_and_decomposed_agree_on_everything_but_where_they_wrote() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    // The comparison the task file asks for. Both programs declare the same
    // goal, the same five checks, the same evidence and the same three
    // outputs; one writes them as a single task with two phases, the other as
    // two sub-tasks composed by a parent phase, with the types and rules in
    // imported libraries.
    let whole_root = workspace("large-release-pipeline");
    let whole = observable(&run(
        "large-release-pipeline",
        &[
            ("--allow-write", whole_root.clone()),
            ("--allow-read", whole_root.clone()),
        ],
    ));
    let decomposed_root = workspace("large-release-pipeline-decomposed");
    let decomposed = observable(&run(
        "large-release-pipeline-decomposed",
        &[
            ("--allow-write", decomposed_root.clone()),
            ("--allow-read", decomposed_root.clone()),
        ],
    ));

    assert_eq!(whole.reached, decomposed.reached, "stages reached differ");
    assert_eq!(whole.outcome, decomposed.outcome, "outcomes differ");
    assert_eq!(
        whole.terminal_status, decomposed.terminal_status,
        "terminal statuses differ"
    );
    assert_eq!(whole.checks, decomposed.checks, "check outcomes differ");
    assert_eq!(whole.evidence, decomposed.evidence, "evidence differs");
    assert_eq!(
        whole.diagnostics, decomposed.diagnostics,
        "diagnostics differ"
    );

    // The only expected difference is the workspace each was told to use, so
    // the comparison names it rather than ignoring the outputs.
    let mut expected = whole.outputs.clone();
    let published = expected
        .get_mut("output.manifest")
        .expect("the manifest is an output");
    *published = published.replace(
        "large-release-pipeline",
        "large-release-pipeline-decomposed",
    );
    assert_eq!(
        expected, decomposed.outputs,
        "outputs differ by more than the declared workspace"
    );
}

#[test]
fn every_application_repeats_itself_exactly() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    let grants: [(&str, &[&str]); 4] = [
        ("small-invoice-total", &[]),
        ("medium-release-notes", &["--allow-write"]),
        ("large-release-pipeline", &["--allow-write", "--allow-read"]),
        (
            "large-release-pipeline-decomposed",
            &["--allow-write", "--allow-read"],
        ),
    ];
    for (app, flags) in grants {
        let mut runs = Vec::new();
        for _ in 0..2 {
            let root = workspace(app);
            let granted: Vec<(&str, PathBuf)> =
                flags.iter().map(|flag| (*flag, root.clone())).collect();
            runs.push(observable(&run(app, &granted)));
        }
        assert_eq!(runs[0], runs[1], "{app} produced two different results");
    }
}

/// PRETEST-04 F24: an application's workspace belongs to this test run, under
/// the temporary directory it was given, never a fixed path every run shares.
#[test]
fn application_workspaces_belong_to_this_run() {
    let _serial = WORKSPACES.lock().unwrap_or_else(|e| e.into_inner());
    let root = workspace("medium-release-notes");
    assert!(
        root.starts_with(std::env::temp_dir()) && !root.starts_with("/tmp/lcl-apps"),
        "{}",
        root.display()
    );
}
