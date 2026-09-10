//! The headless end-to-end gate: a clean terminal, and nothing but the tool.
//!
//! Every other suite already runs the binary with `env_clear()`. This one asks
//! the harder question: with no environment at all, no working directory the
//! tool may rely on, and no state left over from a previous run, does a project
//! still resolve, run, and produce the same answer?
//!
//! That is the acceptance criterion for the milestone — "Run projects from
//! clean terminal environment and prove CLI/direct engine protocol
//! equivalence" — and it is also the strongest available test of the ambient
//! rule in `05_SEMANTICS/02`. A tool that had come to depend on an inherited
//! variable, a cached path or the shell's working directory would fail here and
//! nowhere else.

mod common;

use common::{canonical_root, example, lcl_in, scratch, write};
use lcl_spec::json::{self, Json};
use std::path::PathBuf;

const SUCCESS: i32 = 0;

/// A complete project: manifest, entry document, an import, and a cache.
fn full_project(name: &str) -> PathBuf {
    let root = scratch(name);
    write(root.join("src/main.lcl"), example("03_IMPORTING_TASK.lcl"));
    write(
        root.join("src/02_IMPORT_LIBRARY.lcl"),
        example("02_IMPORT_LIBRARY.lcl"),
    );
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \
             \"entry\": \"src/main.lcl\",\n  \"cache\": \".lcl-cache\"\n}}\n",
            canonical_root().display().to_string()
        ),
    );
    root
}

/// The whole lifecycle, from an empty environment, in one pass.
#[test]
fn a_project_checks_validates_locks_and_runs_from_a_clean_environment() {
    let root = full_project("clean_lifecycle");

    // No environment variables at all, including no LCL_SPEC: the manifest is
    // the only thing naming a specification package.
    let check = lcl_in(&root, &["check"], &[]);
    assert_eq!(check.code, SUCCESS, "{}{}", check.stdout, check.stderr);

    let validate = lcl_in(&root, &["validate"], &[]);
    assert_eq!(
        validate.code, SUCCESS,
        "{}{}",
        validate.stdout, validate.stderr
    );

    let inspect = lcl_in(&root, &["inspect"], &[]);
    assert_eq!(
        inspect.code, SUCCESS,
        "{}{}",
        inspect.stdout, inspect.stderr
    );
    assert!(inspect.stdout.contains("IMPORT"));

    let lock = lcl_in(&root, &["package", "lock"], &[]);
    assert_eq!(lock.code, SUCCESS, "{}{}", lock.stdout, lock.stderr);

    let verify = lcl_in(&root, &["package", "verify"], &[]);
    assert_eq!(verify.code, SUCCESS, "{}", verify.stdout);

    // The run is expected not to succeed: this example copies a file, and no
    // capability was granted, so the row fails its precondition before any
    // effect. What matters is that it reached completion and said so.
    let run = lcl_in(&root, &["run", "--machine", "--locked"], &[]);
    let value = json::parse(&run.stdout).expect("valid JSON");
    assert_eq!(
        value.get("reached").and_then(Json::as_str),
        Some("completion")
    );
    assert!(value
        .get("completion")
        .and_then(|c| c.get("terminal_status"))
        .and_then(Json::as_str)
        .is_some_and(|s| s.starts_with("status.")));
}

/// The working directory is not an input.
///
/// The same project, addressed by absolute path from three different
/// directories, produces identical output. `05_SEMANTICS/02` says the ambient
/// current directory "does not exist in portable LCL", and this is what that
/// looks like from outside.
#[test]
fn the_working_directory_decides_nothing() {
    let root = full_project("clean_cwd");
    let elsewhere = scratch("clean_cwd_elsewhere");
    let project_flag = root.display().to_string();
    let document = root.join("src/main.lcl").display().to_string();

    let from_inside = lcl_in(&root, &["check", "--machine"], &[]);
    let from_outside = lcl_in(
        &elsewhere,
        &["check", "--machine", "--project", &project_flag, &document],
        &[],
    );
    let from_temp = lcl_in(
        &std::env::temp_dir(),
        &["check", "--machine", "--project", &project_flag, &document],
        &[],
    );

    assert_eq!(from_inside.stdout, from_outside.stdout);
    assert_eq!(from_inside.stdout, from_temp.stdout);
}

/// Running the same project twice from clean environments is byte-identical.
#[test]
fn two_clean_runs_produce_the_same_bytes() {
    let root = full_project("clean_repeat");
    let first = lcl_in(&root, &["run", "--machine"], &[]);
    let second = lcl_in(&root, &["run", "--machine"], &[]);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.code, second.code);
}

/// Every valid canonical example runs from a clean environment.
///
/// Thirteen documents, each carried from bytes to exactly one terminal status
/// through the real binary. A suite that only ran the succeeding ones would be
/// choosing its evidence, so all thirteen are run and the reasons are asserted.
///
/// Five succeed. That is fewer than the seven the facade's own suite reaches,
/// and the difference is the host, not a defect: that suite installs every
/// registered profile against a deterministic in-memory host, while the CLI
/// grants nothing at all unless a flag says so. Two examples that reach mock
/// fixtures therefore report a host limitation here instead.
///
/// The second assertion is the load-bearing one. Every example that does not
/// succeed must fail for a *host* reason — an uninstalled implementation
/// profile, a capability nobody granted, or a datum nobody supplied — and never
/// because the engine could not lex, parse, resolve or check it. A regression
/// that broke the language would show up as a diagnostic outside that set.
#[test]
fn every_canonical_example_runs_through_the_binary() {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let root = scratch("clean_examples");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();

    // Every example beside its siblings, because two of them import.
    for name in &names {
        write(
            root.join(name),
            std::fs::read(dir.join(name)).expect("readable"),
        );
    }
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
            canonical_root().display().to_string()
        ),
    );

    // The only identifiers a closed host may produce here. Each names a
    // limitation of this machine or of what the caller supplied, never a defect
    // the engine found in the document.
    const HOST_REASONS: &[&str] = &[
        // "Host limitations produce error.host.constraint and never change LCL
        // meaning."
        "error.host.constraint",
        // A row whose implementation-profile role has no installed profile
        // fails its precondition before any effect.
        "error.operation.precondition",
        // A required datum nobody supplied reads MISSING and blocks.
        "error.required.missing",
    ];

    let mut succeeded = 0usize;
    for name in &names {
        let run = lcl_in(&root, &["run", "--machine", name], &[]);
        let value = json::parse(&run.stdout).expect("valid JSON");
        assert_eq!(
            value.get("reached").and_then(Json::as_str),
            Some("completion"),
            "{name} reached completion: {}",
            run.stderr
        );
        let status = value
            .get("completion")
            .and_then(|c| c.get("terminal_status"))
            .and_then(Json::as_str)
            .expect("one terminal status");

        if status == "status.succeeded" {
            succeeded += 1;
            assert_eq!(run.code, SUCCESS, "{name}");
            continue;
        }

        assert_eq!(run.code, 2, "{name} -> {status}");
        let primary = value
            .get("diagnostics")
            .and_then(Json::as_array)
            .expect("diagnostics")
            .iter()
            .find(|d| d.get("primary").and_then(Json::as_bool) == Some(true))
            .and_then(|d| d.get("id"))
            .and_then(Json::as_str)
            .expect("a primary diagnostic explains a non-success");
        assert!(
            HOST_REASONS.contains(&primary),
            "{name} did not succeed for a host reason, but for {primary}"
        );
    }

    assert_eq!(names.len(), 13);
    assert_eq!(
        succeeded, 5,
        "the five examples that need nothing outside the language"
    );
}

/// A document outside any project still works, with an explicit package.
#[test]
fn a_single_document_works_without_a_project() {
    let root = scratch("clean_no_project");
    write(root.join("solo.lcl"), example("01_MINIMAL_TASK.lcl"));

    let run = lcl_in(
        &root,
        &[
            "run",
            "--spec",
            &canonical_root().display().to_string(),
            "solo.lcl",
        ],
        &[],
    );
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("status.succeeded"));
}

/// Nothing is written unless something was asked for.
///
/// After a full read-only lifecycle, the project holds exactly the files it
/// started with. A tool that dropped a cache, a log or a temporary file into a
/// user's project would be doing something the user did not ask for.
#[test]
fn read_only_commands_write_nothing() {
    let root = full_project("clean_no_writes");
    let before = listing(&root);

    for args in [
        vec!["check"],
        vec!["validate"],
        vec!["inspect"],
        vec!["run"],
        vec!["spec"],
        vec!["syntax"],
    ] {
        assert!(lcl_in(&root, &args, &[]).code < 3, "{args:?} ran");
    }

    assert_eq!(before, listing(&root), "no file appeared or disappeared");
}

/// Every file under `dir`, relative and sorted.
fn listing(dir: &std::path::Path) -> Vec<String> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("readable")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                out.push(
                    path.strip_prefix(base)
                        .expect("inside")
                        .display()
                        .to_string(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}
