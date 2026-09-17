//! Multi-file projects, reproducibility, and the capability boundary.
//!
//! The two acceptance criteria here are "version/checksum/import behavior is
//! reproducible" and, underneath it, the two-gate rule the capability contract
//! states: "Host permission does not imply LCL authorization. LCL authorization
//! does not force host permission. Both gates must pass for an effect."
//!
//! The capability cases are the ones worth reading closely. A tool that
//! defaulted to granting what a document asked for would make the host gate
//! decorative, and every run would silently carry the operator's whole machine.

mod common;

use common::{canonical_root, example, lcl_in, scratch, write};
use lcl_spec::json::{self, Json};
use std::path::{Path, PathBuf};

const SUCCESS: i32 = 0;
const NOT_SUCCEEDED: i32 = 2;
const ENVIRONMENT: i32 = 4;

/// A project whose root document imports a sibling.
fn importing_project(name: &str) -> PathBuf {
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

fn units_of(machine_output: &str) -> Vec<String> {
    json::parse(machine_output)
        .expect("valid JSON")
        .get("units")
        .and_then(Json::as_array)
        .expect("units")
        .iter()
        .filter_map(|u| u.get("id").and_then(Json::as_str))
        .map(str::to_string)
        .collect()
}

#[test]
fn a_multi_file_project_loads_both_units() {
    let root = importing_project("project_imports");
    let run = lcl_in(&root, &["check", "--machine"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert_eq!(
        units_of(&run.stdout),
        vec![
            "src/main.lcl".to_string(),
            "src/02_IMPORT_LIBRARY.lcl".to_string()
        ]
    );
}

/// A file nothing imports never appears as an input.
///
/// The acceptance criterion states it as "project loading never includes
/// unreferenced ambient files as normative input". Here it is at the tool
/// level: the project holds a third document, and the record of what was loaded
/// does not mention it.
#[test]
fn an_unreferenced_document_never_enters_the_project() {
    let root = importing_project("project_ambient");
    write(root.join("src/stray.lcl"), example("01_MINIMAL_TASK.lcl"));
    write(
        root.join("src/also_stray.lcl"),
        example("12_SET_SORTING.lcl"),
    );

    let run = lcl_in(&root, &["check", "--machine"], &[]);
    let units = units_of(&run.stdout);
    assert_eq!(units.len(), 2, "{units:?}");
    assert!(!units.iter().any(|u| u.contains("stray")), "{units:?}");
}

/// The same project resolves to the same bytes twice.
#[test]
fn resolution_is_reproducible() {
    let root = importing_project("project_reproducible");
    let first = lcl_in(&root, &["check", "--machine"], &[]);
    let second = lcl_in(&root, &["check", "--machine"], &[]);
    assert_eq!(first.stdout, second.stdout);
}

/// A project's identities do not depend on where the project lives.
#[test]
fn identity_is_independent_of_the_projects_location() {
    let a = lcl_in(
        &importing_project("project_place_a"),
        &["check", "--machine"],
        &[],
    );
    let b = lcl_in(
        &importing_project("project_place_b"),
        &["check", "--machine"],
        &[],
    );
    assert_eq!(units_of(&a.stdout), units_of(&b.stdout));
}

/// Locking records what was loaded; verifying compares against it.
#[test]
fn lock_then_verify_agrees() {
    let root = importing_project("project_lock");
    let locked = lcl_in(&root, &["package", "lock"], &[]);
    assert_eq!(locked.code, SUCCESS, "{}{}", locked.stdout, locked.stderr);
    assert!(root.join("lcl.lock").is_file());

    let verified = lcl_in(&root, &["package", "verify"], &[]);
    assert_eq!(verified.code, SUCCESS, "{}", verified.stdout);
    assert!(verified.stdout.contains("matches what was loaded"));
}

/// An edited import is drift, and drift is reported rather than absorbed.
#[test]
fn an_edited_import_is_reported_as_drift() {
    let root = importing_project("project_drift");
    assert_eq!(lcl_in(&root, &["package", "lock"], &[]).code, SUCCESS);

    let import = root.join("src/02_IMPORT_LIBRARY.lcl");
    let edited = format!("{}\n", std::fs::read_to_string(&import).unwrap());
    write(&import, edited);

    let verified = lcl_in(&root, &["package", "verify"], &[]);
    assert_eq!(verified.code, ENVIRONMENT);
    assert!(verified.stdout.contains("02_IMPORT_LIBRARY.lcl changed"));
}

/// `--locked` refuses to proceed on drift.
#[test]
fn locked_refuses_a_changed_project() {
    let root = importing_project("project_locked");
    assert_eq!(lcl_in(&root, &["package", "lock"], &[]).code, SUCCESS);
    assert_eq!(lcl_in(&root, &["check", "--locked"], &[]).code, SUCCESS);

    let import = root.join("src/02_IMPORT_LIBRARY.lcl");
    let edited = format!("{}\n", std::fs::read_to_string(&import).unwrap());
    write(&import, edited);

    let run = lcl_in(&root, &["check", "--locked"], &[]);
    assert_eq!(run.code, ENVIRONMENT);
    assert!(run.stderr.contains("does not describe what was loaded"));
}

/// `--locked` with no lock file says so rather than proceeding.
#[test]
fn locked_without_a_lock_file_stops() {
    let root = importing_project("project_locked_missing");
    let run = lcl_in(&root, &["check", "--locked"], &[]);
    assert_eq!(run.code, ENVIRONMENT);
    assert!(run.stderr.contains("package lock"));
}

/// Vendoring puts bytes in the cache and reports the checksum to declare.
#[test]
fn vendoring_caches_a_source_by_checksum() {
    let root = importing_project("project_vendor");
    let source = root.join("src/02_IMPORT_LIBRARY.lcl");

    let run = lcl_in(
        &root,
        &[
            "package",
            "vendor",
            "https://example.invalid/lib.lcl",
            &source.display().to_string(),
        ],
        &[],
    );
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);

    let digest = lcl_spec::sha256::hex_digest(example("02_IMPORT_LIBRARY.lcl").as_bytes());
    assert!(run.stdout.contains(&digest), "{}", run.stdout);
    assert!(
        run.stdout.contains("declare CHECKSUM"),
        "the operator is told what the document must declare"
    );

    let listed = lcl_in(&root, &["package", "list"], &[]);
    assert_eq!(listed.code, SUCCESS);
    assert!(listed.stdout.contains("https://example.invalid/lib.lcl"));
    assert!(listed.stdout.contains(&digest));
}

/// A corrupted cache is reported by `package list`.
#[test]
fn a_corrupted_cache_is_reported() {
    let root = importing_project("project_cache_corrupt");
    let source = root.join("src/02_IMPORT_LIBRARY.lcl");
    assert_eq!(
        lcl_in(
            &root,
            &[
                "package",
                "vendor",
                "https://example.invalid/lib.lcl",
                &source.display().to_string(),
            ],
            &[],
        )
        .code,
        SUCCESS
    );

    let digest = lcl_spec::sha256::hex_digest(example("02_IMPORT_LIBRARY.lcl").as_bytes());
    write(root.join(".lcl-cache/sha256").join(&digest), "tampered");

    let listed = lcl_in(&root, &["package", "list"], &[]);
    assert_eq!(listed.code, ENVIRONMENT);
    assert!(listed.stdout.contains("fault"));
}

// ---------------------------------------------------------------------------
// The capability boundary
// ---------------------------------------------------------------------------

/// A document whose ACTION writes one file, so a run needs a real capability.
fn writing_project(name: &str, target: &Path) -> PathBuf {
    let source = format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\
         \n\
         SPECIFICATION:\n    ID: example.write\n    NAME: \"Write one file\"\n    \
         VERSION: \"1.0.0\"\n    KIND: kind.task\n\
         \n\
         DATA:\n    ID: data.content\n    TYPE: STRING\n    VALUE: \"written by lcl\"\n\
         \n\
         OUTPUT:\n    ID: output.written\n    TYPE: PATH\n    FORMAT: format.plain_text\n\
         \n\
         GOAL:\n    ID: goal.write\n    ASSERT: TRUE\n\
         \n\
         ALLOW:\n    ID: allow.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    AUTHORITY: 900\n\
         \n\
         ACTION:\n    ID: action.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    PARAMETER:\n        NAME: content\n        \
         TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: REF(data.content)\n    \
         PARAMETER:\n        NAME: create_if_missing\n        TYPE: BOOLEAN\n        \
         REQUIRED: FALSE\n        VALUE: TRUE\n    \
         OUTPUT: REF(output.written)\n\
         \n\
         SUCCESS:\n    ID: success.write\n    ALL: [REF(output.written)]\n\
         \n\
         TASK:\n    ID: task.write\n    GOAL: REF(goal.write)\n    \
         ACTION: REF(action.write)\n    OUTPUT: REF(output.written)\n    \
         SUCCESS: REF(success.write)\n\
         \n\
         EXECUTE:\n    REFERENCE: REF(task.write)\n",
        target = target.display().to_string()
    );
    let root = scratch(name);
    write(root.join("main.lcl"), source);
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \
             \"entry\": \"main.lcl\"\n}}\n",
            canonical_root().display().to_string()
        ),
    );
    root
}

/// Without a grant, the effect does not happen, and the run says why.
///
/// The refusal arrives one step earlier than a host refusal would. A profile is
/// a claim that an implementation exists, and the CLI installs the filesystem
/// profiles only when it was actually given a filesystem, so `core.write`'s
/// `implementation_profile` role has no profile to select and the row fails its
/// precondition — `error.operation.precondition` — before any effect. The
/// document's own `ALLOW` authorized the operation; nothing on this machine
/// offered to carry it out.
#[test]
fn a_run_without_a_grant_performs_no_effect() {
    let target = scratch("capability_denied_target").join("written.txt");
    let root = writing_project("capability_denied", &target);

    let run = lcl_in(&root, &["run", "--machine"], &[]);
    assert_eq!(run.code, NOT_SUCCEEDED, "{}{}", run.stdout, run.stderr);
    assert!(
        !target.exists(),
        "nothing was written without an explicit grant"
    );

    let value = json::parse(&run.stdout).expect("valid JSON");
    let ids: Vec<&str> = value
        .get("diagnostics")
        .and_then(Json::as_array)
        .expect("diagnostics")
        .iter()
        .filter_map(|d| d.get("id").and_then(Json::as_str))
        .collect();
    assert!(
        ids.contains(&"error.operation.precondition"),
        "the row fails its precondition before effects: {ids:?}"
    );
}

/// With the grant, the same document performs the effect.
///
/// The pair is the point: one flag is the whole difference between a run that
/// touches the filesystem and one that cannot, and the document is byte for
/// byte the same in both.
#[test]
fn a_run_with_a_grant_performs_the_effect() {
    let dir = scratch("capability_granted_target");
    let target = dir.join("written.txt");
    let root = writing_project("capability_granted", &target);

    let run = lcl_in(
        &root,
        &["run", "--allow-write", &dir.display().to_string()],
        &[],
    );
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert_eq!(
        std::fs::read_to_string(&target).expect("the file was written"),
        "written by lcl"
    );
    assert!(run.stdout.contains("status.succeeded"));
}

/// A grant for a different directory does not authorize this one.
#[test]
fn a_grant_elsewhere_does_not_authorize_this_target() {
    let dir = scratch("capability_wrong_grant_target");
    let elsewhere = scratch("capability_wrong_grant_other");
    let target = dir.join("written.txt");
    let root = writing_project("capability_wrong_grant", &target);

    let run = lcl_in(
        &root,
        &["run", "--allow-write", &elsewhere.display().to_string()],
        &[],
    );
    assert_eq!(run.code, NOT_SUCCEEDED, "{}{}", run.stdout, run.stderr);
    assert!(!target.exists(), "the grant named a different directory");
}

/// A staged command never reaches the host at all.
///
/// `validate` stops at step 9, which is before effects by construction. The
/// file must not exist afterwards even with a grant on the command line.
#[test]
fn validate_performs_no_effect_even_when_granted() {
    let dir = scratch("capability_validate_target");
    let target = dir.join("written.txt");
    let root = writing_project("capability_validate", &target);

    let run = lcl_in(
        &root,
        &["validate", "--allow-write", &dir.display().to_string()],
        &[],
    );
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert!(!target.exists(), "preflight performs no effect");
}

// ---------------------------------------------------------------------------
// PRETEST-04: one document root, bounded reads, contained writes
// ---------------------------------------------------------------------------

/// One byte over the product read limit.
const OVERSIZED: u64 = lcl_project::MAX_FILE_BYTES + 1;

/// One sparse file of `OVERSIZED` bytes.
fn oversized(path: &Path) {
    std::fs::File::create(path)
        .and_then(|file| file.set_len(OVERSIZED))
        .expect("an oversized fixture");
}

/// F20 and F21: a document inside a project is judged in that project however
/// it is named, and a relative path is resolved once.
///
/// `lcl-workspace --document` opens the nearest enclosing project, so the
/// command line must too: otherwise the same file receives a different root,
/// identity and specification package depending on which product opened it.
#[test]
fn a_nested_document_is_judged_in_its_enclosing_project() {
    let root = importing_project("project_nested_document");
    let elsewhere = scratch("project_nested_document_elsewhere");
    let from_entry = lcl_in(&root, &["check", "--machine"], &[]);
    assert_eq!(
        from_entry.code, SUCCESS,
        "{}{}",
        from_entry.stdout, from_entry.stderr
    );

    let absolute = root.join("src/main.lcl").display().to_string();
    for (cwd, document) in [
        (root.as_path(), "src/main.lcl"),
        (root.join("src").as_path(), "main.lcl"),
        (elsewhere.as_path(), absolute.as_str()),
    ] {
        let run = lcl_in(cwd, &["check", "--machine", document], &[]);
        assert_eq!(
            run.code,
            SUCCESS,
            "{document} from {}: {}{}",
            cwd.display(),
            run.stdout,
            run.stderr
        );
        assert_eq!(
            run.stdout,
            from_entry.stdout,
            "{document} from {}",
            cwd.display()
        );
    }
}

/// F17: a configured cache that will not open is a configuration fault, not a
/// project without a cache.
#[test]
fn a_configured_cache_that_will_not_open_stops_the_command() {
    let root = importing_project("project_cache_unopenable");
    write(root.join(".lcl-cache/index"), "lcl-cache/99\n");
    let run = lcl_in(&root, &["check"], &[]);
    assert_eq!(run.code, ENVIRONMENT, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("cache"), "{}", run.stderr);
}

/// F18: a vendored URI must be one the cache index can hold as one entry.
#[test]
fn a_vendored_uri_cannot_inject_a_cache_entry() {
    let root = importing_project("project_vendor_injection");
    let source = root.join("src/02_IMPORT_LIBRARY.lcl");
    let forged = format!(
        "https://example.invalid/lib.lcl\n{}  https://example.invalid/forged.lcl",
        "0".repeat(64)
    );
    for uri in [
        forged.as_str(),
        "https://example.invalid/a b.lcl",
        "not a uri",
    ] {
        let run = lcl_in(
            &root,
            &["package", "vendor", uri, &source.display().to_string()],
            &[],
        );
        assert_eq!(run.code, 3, "{uri:?}: {}{}", run.stdout, run.stderr);
    }
    assert!(
        !root.join(".lcl-cache/index").exists(),
        "nothing was cached"
    );
}

/// F19: vendored bytes and the manifest are read within a product limit.
#[test]
fn oversized_project_inputs_are_refused_before_they_are_read() {
    let root = importing_project("project_oversized_vendor");
    let huge = root.join("huge.lcl");
    oversized(&huge);
    let run = lcl_in(
        &root,
        &[
            "package",
            "vendor",
            "https://example.invalid/huge.lcl",
            &huge.display().to_string(),
        ],
        &[],
    );
    assert_eq!(run.code, ENVIRONMENT, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("limit"), "{}", run.stderr);

    let manifest = scratch("project_oversized_manifest");
    oversized(&manifest.join("lcl.project.json"));
    let run = lcl_in(&manifest, &["check"], &[]);
    assert_eq!(run.code, ENVIRONMENT, "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("limit"), "{}", run.stderr);
}

/// F22: a manifest may name a lock outside the project for reading, but
/// `package lock` writes only inside the project.
#[test]
fn package_lock_never_writes_outside_the_project() {
    let root = importing_project("project_lock_escape");
    let outside = scratch("project_lock_escape_outside");
    let manifest = std::fs::read_to_string(root.join("lcl.project.json")).unwrap();
    let external = outside.join("external.lock");
    write(
        root.join("lcl.project.json"),
        manifest.replace(
            "\"cache\"",
            &format!(
                "\"lock\": {:?},\n  \"cache\"",
                external.display().to_string()
            ),
        ),
    );
    let run = lcl_in(&root, &["package", "lock"], &[]);
    assert_eq!(run.code, ENVIRONMENT, "{}{}", run.stdout, run.stderr);
    assert!(
        !external.exists(),
        "the lock was written outside the project"
    );

    // Reading an external lock stays legitimate.
    std::fs::write(root.join("lcl.project.json"), &manifest).unwrap();
    assert_eq!(lcl_in(&root, &["package", "lock"], &[]).code, SUCCESS);
    std::fs::rename(root.join("lcl.lock"), &external).unwrap();
    write(
        root.join("lcl.project.json"),
        manifest.replace(
            "\"cache\"",
            &format!(
                "\"lock\": {:?},\n  \"cache\"",
                external.display().to_string()
            ),
        ),
    );
    let verified = lcl_in(&root, &["package", "verify"], &[]);
    assert_eq!(
        verified.code, SUCCESS,
        "{}{}",
        verified.stdout, verified.stderr
    );

    // A link inside the project is judged where it leads.
    std::fs::write(root.join("lcl.project.json"), &manifest).unwrap();
    let target = outside.join("linked.lock");
    std::os::unix::fs::symlink(&target, root.join("lcl.lock")).unwrap();
    let run = lcl_in(&root, &["package", "lock"], &[]);
    assert_eq!(run.code, ENVIRONMENT, "{}{}", run.stdout, run.stderr);
    assert!(!target.exists(), "the lock was written through a link");
}
