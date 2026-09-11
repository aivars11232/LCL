//! Phase C: the escapes, at the product boundary rather than the unit one.
//!
//! `5.7 Host/capability separation`: "Host permission does not imply LCL
//! authorization. LCL authorization does not force host permission. Both gates
//! must pass for an effect."
//!
//! `lcl-capabilities` already tests its own grant arithmetic. What this suite
//! asks is the product question: a document written by an adversary, run
//! through the whole engine against a real host boundary, must not reach
//! anything it was not granted, and a prohibition in the document must not be
//! opened by a grant.

use lcl_capabilities::Grants;
use lcl_hardening::{empty_provider, engine, unit};
use lcl_protocol::Inputs;
use lcl_stdlib::fixtures::MemoryFileSystem;
use lcl_stdlib::HostAdapter;

const HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: security.case\n    \
                      NAME: \"Security\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n";

/// A task whose one action reads a path, plus whatever rule the case is about.
fn read_task(target: &str, rules: &str) -> String {
    format!(
        "{HEADER}{rules}
DATA:
    ID: data.target
    TYPE: PATH
    VALUE: PATH({target:?})

GOAL:
    ID: goal.one
    ASSERT: TRUE

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: REF(data.target)

VERIFY:
    ID: verify.one
    ASSERT: TRUE

SUCCESS:
    ID: success.one
    ALL: [REF(verify.one)]

TASK:
    ID: task.one
    GOAL: REF(goal.one)
    ACTION: REF(action.read)
    SUCCESS: REF(success.one)

EXECUTE:
    REFERENCE: REF(task.one)
"
    )
}

/// A task whose one action writes a path.
fn write_task(target: &str, rules: &str) -> String {
    format!(
        "{HEADER}{rules}
DATA:
    ID: data.target
    TYPE: PATH
    VALUE: PATH({target:?})

GOAL:
    ID: goal.one
    ASSERT: TRUE

ACTION:
    ID: action.write
    OPERATION: core.create
    TARGET: REF(data.target)
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: \"x\"

VERIFY:
    ID: verify.one
    ASSERT: TRUE

SUCCESS:
    ID: success.one
    ALL: [REF(verify.one)]

TASK:
    ID: task.one
    GOAL: REF(goal.one)
    ACTION: REF(action.write)
    SUCCESS: REF(success.one)

EXECUTE:
    REFERENCE: REF(task.one)
"
    )
}

/// Run one document against a filesystem granted exactly what it was built with.
fn run(source: &str, filesystem: MemoryFileSystem) -> lcl_protocol::Report {
    let engine = engine();
    let grants = filesystem.grants().clone();
    let mut adapter = HostAdapter::new(grants).with_filesystem(filesystem);
    let mut stdlib = engine.stdlib().expect("the standard library assembles");
    engine.run(
        &unit(source),
        &empty_provider(),
        &Inputs::new(),
        &mut stdlib,
        &mut adapter,
    )
}

/// Every effect the host reported, across every invocation.
///
/// This is engine truth about what actually crossed the boundary, which is a
/// stronger claim than "the operation failed": an operation can fail after
/// changing something, and this suite is about whether anything changed.
fn effects(report: &lcl_protocol::Report) -> Vec<String> {
    report
        .execution
        .iter()
        .flat_map(|execution| execution.invocations.iter())
        .flat_map(|invocation| invocation.effects.iter().cloned())
        .collect()
}

fn errors(report: &lcl_protocol::Report) -> Vec<String> {
    report.diagnostics.iter().map(|d| d.id.clone()).collect()
}

#[test]
fn a_granted_read_inside_its_scope_succeeds_so_the_refusals_are_about_the_escape() {
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", *b"abcd");
    let report = run(&read_task("/srv/data/a.txt", ""), filesystem);
    assert_eq!(
        report.terminal_status(),
        Some("status.succeeded"),
        "{:?}",
        errors(&report)
    );
}

#[test]
fn no_spelling_of_a_path_outside_the_grant_is_reachable() {
    // Every one of these resolves outside `/srv/data`, and each is a spelling a
    // naive containment check gets wrong.
    let escapes = [
        "/etc/passwd",
        "/srv/data/../../etc/passwd",
        "/srv/data/./../../etc/passwd",
        "/srv/data/sub/../../../etc/passwd",
        "/srv/data/..",
        "//srv//data//..//..//etc//passwd",
    ];
    for escape in escapes {
        let filesystem = MemoryFileSystem::new()
            .with_read_scope("/srv/data")
            .with_file("/srv/data/a.txt", *b"abcd")
            .with_file("/etc/passwd", *b"root:x:0:0");
        let report = run(&read_task(escape, ""), filesystem);
        assert_ne!(
            report.terminal_status(),
            Some("status.succeeded"),
            "{escape} was read despite being outside the grant"
        );
        assert!(
            errors(&report)
                .iter()
                .any(|id| id == "error.permission.denied"),
            "{escape} was refused, but not as a permission denial: {:?}",
            errors(&report)
        );
    }
}

#[test]
fn a_read_grant_does_not_carry_a_write_grant() {
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", *b"abcd");
    let report = run(&write_task("/srv/data/new.txt", ""), filesystem);
    assert_ne!(report.terminal_status(), Some("status.succeeded"));
    assert!(
        effects(&report).is_empty(),
        "an effect crossed the boundary under a read-only grant: {:?}",
        effects(&report)
    );
}

#[test]
fn nothing_is_permitted_by_default() {
    let filesystem = MemoryFileSystem::new().with_file("/srv/data/a.txt", *b"abcd");
    let report = run(&read_task("/srv/data/a.txt", ""), filesystem);
    assert_ne!(
        report.terminal_status(),
        Some("status.succeeded"),
        "an ungranted read succeeded"
    );
}

#[test]
fn a_prohibition_in_the_document_is_not_opened_by_a_host_grant() {
    // `block_schemas_v0.1.0.json#/schemas/FORBID`: "Hard prohibition", which
    // `#/schemas/ALLOW` "never defeats by itself". The host is told yes and the
    // language still says no, before the host is ever consulted.
    let forbid = "\nFORBID:\n    ID: forbid.write\n    OPERATION: core.create\n    \
                  REASON: \"the case forbids it\"\n";
    let filesystem = MemoryFileSystem::new().with_scope("/srv/data");
    let report = run(&write_task("/srv/data/new.txt", forbid), filesystem);
    assert_ne!(
        report.terminal_status(),
        Some("status.succeeded"),
        "a FORBIDden effect completed with the host grant given"
    );
    assert!(
        effects(&report).is_empty(),
        "a FORBIDden effect reached the host: {:?}",
        effects(&report)
    );
}

#[test]
fn an_ungranted_program_and_an_ungranted_host_are_both_refused() {
    let grants = Grants::none();
    assert!(grants
        .decide(&lcl_capabilities::Grant::RunProgram("sh".to_string()))
        .is_err());
    assert!(grants
        .decide(&lcl_capabilities::Grant::Network {
            host: "example.invalid".to_string(),
            secure: true,
        })
        .is_err());
}
