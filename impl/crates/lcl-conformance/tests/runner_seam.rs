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
