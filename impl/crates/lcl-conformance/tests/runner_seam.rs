//! The runner reports what happened, and reports it the same way twice.
//!
//! These are tests of the *instrument*, not of the language. A case suite is
//! only worth as much as the runner under it, so the runner's own behaviour —
//! that it reaches the stage it says it reached, that it never invents a
//! verdict, that two runs of one source agree — is checked first.

mod common;

use common::*;
use lcl_conformance::{judge, Expectation, Reached, Verdict};

#[test]
fn a_well_formed_document_reaches_completion() {
    let runner = runner();
    let observed = runner.run(&assertion_task("", "REF(output.seed) == 1"));
    assert_eq!(observed.reached, Reached::Completion, "{observed:?}");
    assert_eq!(observed.primary, None);
    assert_eq!(
        observed.terminal_status.as_deref(),
        Some("status.succeeded")
    );
}

#[test]
fn a_lexically_invalid_source_stops_at_the_lexical_stage() {
    let runner = runner();
    // A mixed-case keyword is `error.keyword.case`, a lexical defect.
    let observed = runner.run("lcl:\n    VERSION: \"0.1.0\"\n");
    assert_eq!(observed.reached, Reached::Lexical);
    assert!(observed.primary.is_some());
}

#[test]
fn a_source_that_is_not_lcl_at_all_still_returns() {
    let runner = runner();
    for source in ["", "\0\0", "not lcl", "\u{feff}"] {
        let observed = runner.run(source);
        // The assertion is that this returned rather than panicked.
        assert!(observed.reached <= Reached::Completion);
    }
}

#[test]
fn the_runner_reports_check_outcomes_including_skipped_ones() {
    let runner = runner();
    let source = task_document(
        "
INPUT:
    ID: input.seed
    TYPE: INTEGER
    VALUE: 1

OUTPUT:
    ID: output.seed
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.case
    ASSERT: REF(output.seed) == 1

ACTION:
    ID: action.seed
    OPERATION: core.return
    TARGET: REF(input.seed)
    OUTPUT: REF(output.seed)

VERIFY:
    ID: verify.ran
    REQUIRED: FALSE
    ASSERT: REF(output.seed) == 1

VERIFY:
    ID: verify.skipped
    WHEN: FALSE
    ASSERT: TRUE

SUCCESS:
    ID: success.case
    ALL: TRUE

TASK:
    ID: task.case
    GOAL: REF(goal.case)
    INPUT: REF(input.seed)
    ACTION: REF(action.seed)
    OUTPUT: REF(output.seed)
    SUCCESS: REF(success.case)

EXECUTE:
    REFERENCE: REF(task.case)
",
    );
    let observed = runner.run(&source);
    assert!(observed
        .checks
        .contains(&("verify.ran".to_string(), "TRUE".to_string())));
    assert!(
        observed
            .checks
            .contains(&("verify.skipped".to_string(), "SKIPPED".to_string())),
        "a skipped check is reported as absent, not omitted: {:?}",
        observed.checks
    );
}

#[test]
fn the_same_source_observes_identically_twice() {
    let runner = runner();
    let source = assertion_task("", "REF(output.seed) == 1");
    assert_eq!(runner.run(&source), runner.run(&source));
}

#[test]
fn reusing_one_runner_leaves_cases_independent() {
    // The runner lends one assembled operation surface to every run. A case
    // that fails must not colour the next one.
    let runner = runner();
    let good = assertion_task("", "REF(output.seed) == 1");
    let bad = assertion_task("", "REF(output.seed) == 999");
    let first = runner.run(&good);
    let _ = runner.run(&bad);
    let again = runner.run(&good);
    assert_eq!(first, again, "an intervening failure changed a later run");
}

#[test]
fn an_expectation_decides_the_verdict_and_the_runner_never_does() {
    let runner = runner();
    let observed = runner.run(&assertion_task("", "REF(output.seed) == 1"));
    assert_eq!(judge(&Expectation::Accepts, &observed), Verdict::Passed);
    assert_eq!(
        judge(
            &Expectation::Rejects("error.type.mismatch".to_string()),
            &observed
        ),
        Verdict::Failed,
        "the same observation fails a different expectation"
    );
}

#[test]
fn an_executed_case_carries_its_source_and_its_observation() {
    let runner = runner();
    let source = assertion_task("", "REF(output.seed) == 1");
    let case = runner.execute("SEAM-001", "seam", &source, Expectation::Accepts);
    assert_eq!(
        case.source, source,
        "the exact input is retained as evidence"
    );
    assert_eq!(case.observed.reached, Reached::Completion);
    assert!(case.passed());
    assert!(case.serialize().contains("SEAM-001"));
}

#[test]
fn exact_bytes_reach_the_lexer_without_lossy_decoding() {
    let observed = runner().run_input(
        &lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new("bytes.lcl"), b"\xff\n"),
        &lcl_resolver::MemoryProvider::new(),
        &lcl_semantics::Invocation::new(),
        &mut lcl_runtime::MockHost::new(),
    );
    assert_eq!(observed.reached, Reached::Lexical);
    assert_eq!(observed.primary.as_deref(), Some("error.encoding.invalid"));
    assert!(observed.invocations.is_empty());
}

#[test]
fn supplied_values_and_real_imports_use_the_same_pipeline() {
    let source = assertion_task(
        "\nIMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n",
        "REF(input.seed) == 7 AND REF(lib.data.value) == 2",
    ).replace("ASSERT: REF(output.seed) == 1", "ASSERT: REF(output.seed) == 7");
    let library = data_document(&[("data.value", "INTEGER", "2")])
        .replace("KIND: kind.data", "KIND: kind.library");
    let mut provider = lcl_resolver::MemoryProvider::new();
    provider.insert("lib.lcl", library.as_bytes());
    let engine = runner();
    let invocation = lcl_semantics::Invocation::new().with(
        "input.seed",
        lcl_runtime::Value::Integer(lcl_checker::numeric::Decimal::parse_integer("7").unwrap()),
    );
    let run = |source: &str| {
        engine.run_input(
            &lcl_resolver::SourceUnit::new(
                lcl_resolver::SourceId::new("case.lcl"),
                source.as_bytes(),
            ),
            &provider,
            &invocation,
            &mut lcl_runtime::MockHost::new(),
        )
    };
    // 05_SEMANTICS/06: explicit VALUE precedes resolved invocation data.
    let explicit = run(&source);
    assert_eq!(
        judge(
            &Expectation::Output {
                id: "output.seed".into(),
                value: "1".into()
            },
            &explicit
        ),
        Verdict::Passed
    );
    assert_eq!(
        explicit.primary.as_deref(),
        Some("error.verification.failed")
    );
    let observed = run(&source.replace("    VALUE: 1\n", "    REQUIRED: FALSE\n    DEFAULT: 1\n"));
    assert_eq!(observed.primary, None, "{}", observed.serialize());
    assert_eq!(
        judge(
            &Expectation::Check {
                id: "verify.case".into(),
                outcome: "TRUE".into()
            },
            &observed
        ),
        Verdict::Passed
    );
}

#[test]
fn attempt_assertions_reject_missing_or_extra_execution() {
    let observed = runner().run(&assertion_task("", "REF(output.seed) == 1"));
    let expected = Expectation::All(vec![
        Expectation::Accepts,
        Expectation::Attempts {
            declaration: "action.seed".into(),
            statuses: vec!["status.succeeded".into()],
        },
        Expectation::AttemptField {
            declaration: "action.seed".into(),
            attempt: 0,
            field: "value".into(),
            value: "1".into(),
        },
        Expectation::Output {
            id: "output.seed".into(),
            value: "1".into(),
        },
    ]);
    assert_eq!(judge(&expected, &observed), Verdict::Passed);
    for statuses in [
        vec![],
        vec!["status.succeeded".into(), "status.succeeded".into()],
    ] {
        assert_eq!(
            judge(
                &Expectation::Attempts {
                    declaration: "action.seed".into(),
                    statuses
                },
                &observed
            ),
            Verdict::Failed
        );
    }
    assert_eq!(
        judge(&Expectation::Recovered("action.seed".into()), &observed),
        Verdict::Failed
    );
    assert_eq!(judge(&Expectation::All(vec![]), &observed), Verdict::Failed);
}

#[test]
fn source_stage_evidence_is_separate_from_semantic_execution() {
    let engine = runner();
    let source = data_document(&[("data.value", "INTEGER", "1")]);
    let clean = engine.run_source(source.as_bytes());
    assert_eq!(
        judge(&Expectation::SourcePass(Reached::Grammar), &clean),
        Verdict::Passed
    );
    assert_eq!(judge(&Expectation::Accepts, &clean), Verdict::Failed);
    assert!(clean.invocations.is_empty());
    let malformed = source.replace("TYPE: INTEGER", "TYPE: 17");
    let observed = engine.run_source(malformed.as_bytes());
    assert_eq!(
        judge(&Expectation::SourcePass(Reached::Lexical), &observed),
        Verdict::Passed
    );
    assert_eq!(
        judge(&Expectation::SourcePass(Reached::Grammar), &observed),
        Verdict::Failed
    );
    assert!(observed.diagnostics.contains(&"error.field.type".into()));
    let encoding = engine.run_source(b"\xff\n");
    assert_eq!(encoding.primary.as_deref(), Some("error.encoding.invalid"));
    assert!(encoding.input_evidence[0].contains("255"));
    assert_eq!(
        judge(&Expectation::SourcePass(Reached::Lexical), &encoding),
        Verdict::Failed
    );
    let bytes = std::fs::read(
        canonical_root().join("08_EXAMPLES/INVALID/10_CONTEXT_WITHOUT_SCOPE.invalid.lcl"),
    )
    .unwrap();
    let context = engine.run_source(&bytes);
    // The higher-authority block-kind rule is primary; the missing SCOPE
    // diagnostic pinned by the illustrative example must remain in evidence.
    assert_eq!(context.primary.as_deref(), Some("error.block.context"));
    assert!(context.diagnostics.contains(&"error.field.required".into()));
}

#[test]
fn rendered_evidence_contains_exact_sources_and_invocation_data() {
    let source = data_document(&[("data.value", "INTEGER", "1")]);
    let provider = lcl_resolver::MemoryProvider::new().with("unused.lcl", b"exact import bytes\n");
    let invocation = lcl_semantics::Invocation::new().with(
        "unused.input",
        lcl_runtime::Value::Text("supplied text".into()),
    );
    let observed = runner().run_input(
        &lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new("root.lcl"), source.as_bytes()),
        &provider,
        &invocation,
        &mut lcl_runtime::MockHost::new(),
    );
    let rendered = observed.serialize();
    assert!(rendered.contains("root.lcl"));
    assert!(rendered.contains("unused.lcl"));
    assert!(rendered.contains("101, 120, 97, 99, 116"));
    assert!(rendered.contains("supplied text"));
    let executed = runner().execute("source-text", "instrument", &source, Expectation::Accepts);
    assert!(executed
        .serialize()
        .contains(&format!("source: {source:?}")));
}

#[test]
fn a_source_rejected_before_resolution_reports_the_registered_stage_name() {
    let runner = runner();
    // The canonical invalid example repeats a non-repeatable field.
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/INVALID/11_DUPLICATE_FIELD.invalid.lcl"),
    )
    .unwrap();
    let grammar = runner.run(&source);
    assert_eq!(grammar.reached, Reached::Grammar, "{grammar:?}");
    assert_eq!(grammar.primary.as_deref(), Some("error.field.duplicate"));
    assert_eq!(grammar.primary_stage.as_deref(), Some("grammar_or_schema"));
    assert_eq!(
        judge(
            &Expectation::RejectsAtStage("grammar_or_schema".to_string()),
            &grammar
        ),
        Verdict::Passed
    );
    // A mixed-case keyword stops at the lexical stage, spelled as registered.
    let lexical = runner.run("lcl:\n    VERSION: \"0.1.0\"\n");
    assert_eq!(lexical.primary_stage.as_deref(), Some("lexical"));
}
