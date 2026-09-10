//! Completion is total: no input makes it panic, hang or fabricate a status.
//!
//! `06_TESTING`: "Malformed/untrusted source must not panic."
//!
//! Completion never sees raw bytes — an execution has already happened — so the
//! untrusted surface here is a *document that reached execution while saying
//! strange things about its own completion*. Those are the inputs below.

mod common;

use common::*;
use lcl_completion::Completion;

fn try_complete(source: &str) -> Option<Completion> {
    let id = lcl_resolver::SourceId::new("root.lcl");
    let unit = lcl_resolver::SourceUnit::new(id, source.as_bytes());
    let resolved = lcl_resolver::Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &lcl_resolver::MemoryProvider::new())
        .ok()?;
    let checked = lcl_checker::Checker::new(static_contracts())
        .check(&resolved)
        .ok()?;
    let planned = lcl_semantics::Preflight::new(preflight_contracts())
        .plan(&checked, &resolved, &lcl_semantics::Invocation::new())
        .ok()?;
    let mut stdlib = lcl_stdlib::Stdlib::load(spec()).ok()?;
    let mut host = lcl_runtime::MockHost::new();
    let execution = lcl_runtime::Runtime::new(runtime_contracts())
        .execute_with(&planned, &checked, &resolved, &mut stdlib, &mut host)
        .ok()?;
    Completion::of(
        completion_contracts(),
        &planned,
        &checked,
        &resolved,
        &execution,
    )
    .ok()
}

/// Documents that are malformed, hostile, or merely strange.
fn corpus() -> Vec<String> {
    let mut out = vec![
        String::new(),
        "\0\0\0".to_string(),
        "LCL:".to_string(),
        "LCL:\n    VERSION: \"0.1.0\"\n".to_string(),
        "\u{feff}LCL:\n    VERSION: \"0.1.0\"\n".to_string(),
        "VERIFY:\n    ID: verify.orphan\n    ASSERT: TRUE\n".to_string(),
        // A completion block referring to nothing that exists.
        task_document(
            "
VERIFY:
    ID: verify.dangling
    TARGET: REF(nothing.at.all)
    ASSERT: REF(also.nothing) == 1
",
        ),
        // A SUCCESS naming itself.
        task_document(
            "
SUCCESS:
    ID: success.self
    ALL: [REF(success.self)]
",
        ),
        // A FAILURE requesting a status that is not a status.
        task_document(
            "
FAILURE:
    ID: failure.nonsense
    WHEN: TRUE
    STATUS: not.a.status
",
        ),
        // An alias chain that closes on itself.
        task_document(
            "
DEFINE:
    ID: outcome.left
    KIND: kind.status
    BASE: outcome.right
    MEANING: \"left\"

DEFINE:
    ID: outcome.right
    KIND: kind.status
    BASE: outcome.left
    MEANING: \"right\"

FAILURE:
    ID: failure.cycle
    WHEN: TRUE
    STATUS: outcome.left
",
        ),
    ];
    // Every canonical example, truncated at each of a spread of byte offsets.
    let source =
        std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"))
            .expect("readable");
    let len = source.len();
    for cut in (0..len).step_by(17) {
        if source.is_char_boundary(cut) {
            out.push(source[..cut].to_string());
        }
    }
    out
}

#[test]
fn no_input_panics_and_every_completion_reports_one_registered_status() {
    for (index, source) in corpus().iter().enumerate() {
        // The point is that this returns at all. A panic fails the test.
        let Some(completion) = try_complete(source) else {
            continue;
        };
        let status = completion.terminal_status();
        assert!(
            completion_contracts().is_terminal(status),
            "case {index}: {status} is not a registered terminal status"
        );
        // And it never fabricates one this layer has no authority to select.
        assert!(
            completion_contracts().permitted_at_root(status),
            "case {index}: {status} is not legal for an execution root"
        );
    }
}

#[test]
fn a_status_alias_cycle_terminates_instead_of_looping() {
    // M3 rejects an alias cycle with error.reference.cycle, so this document
    // should not reach completion at all. The point of the test is that the
    // resolution walk in `terminal` is bounded either way: if the document ever
    // does arrive here, the walk stops instead of spinning.
    let source = task_document(
        "
DEFINE:
    ID: outcome.left
    KIND: kind.status
    BASE: outcome.right
    MEANING: \"left\"

DEFINE:
    ID: outcome.right
    KIND: kind.status
    BASE: outcome.left
    MEANING: \"right\"

FAILURE:
    ID: failure.cycle
    WHEN: TRUE
    STATUS: outcome.left
",
    );
    // Returns rather than hangs. That is the whole assertion.
    let _ = try_complete(&source);
}

#[test]
fn every_completion_serialization_is_valid_utf8_and_bounded() {
    for source in corpus() {
        let Some(completion) = try_complete(&source) else {
            continue;
        };
        let rendered = completion.serialize();
        assert!(rendered.starts_with("COMPLETION\n"));
        assert!(
            rendered.len() < 1_000_000,
            "a completion rendering must stay bounded"
        );
    }
}
