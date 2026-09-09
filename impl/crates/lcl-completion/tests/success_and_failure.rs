//! Step 12: declared `SUCCESS`, declared `FAILURE`, and required evidence.

mod common;

use common::*;
use lcl_completion::{Completion, Quantifier, Reason};

fn complete(source: &str) -> Completion {
    let fixture = execute(source);
    Completion::of(
        completion_contracts(),
        &fixture.planned,
        &fixture.checked,
        &fixture.resolved,
        &fixture.execution,
    )
    .expect("the fixture executed, so it completes")
}

/// A task that returns an input into an output, plus the blocks under test.
///
/// `success_body` is the whole body of the `SUCCESS` block, so a test chooses
/// its own quantifier.
fn task_with(blocks: &str, success_body: &str) -> String {
    task_document(&format!(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

OUTPUT:
    ID: output.value
    TYPE: INTEGER
    FORMAT: format.plain_text

GOAL:
    ID: goal.copy
    ASSERT: REF(output.value) == 7

ACTION:
    ID: action.copy
    OPERATION: core.return
    TARGET: REF(input.value)
    OUTPUT: REF(output.value)
{blocks}
SUCCESS:
    ID: success.root
{success_body}

TASK:
    ID: task.copy
    GOAL: REF(goal.copy)
    INPUT: REF(input.value)
    ACTION: REF(action.copy)
    OUTPUT: REF(output.value)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.copy)
"
    ))
}

const TWO_CHECKS: &str = "
VERIFY:
    ID: verify.true
    REQUIRED: FALSE
    ASSERT: REF(output.value) == 7

VERIFY:
    ID: verify.false
    REQUIRED: FALSE
    ASSERT: REF(output.value) == 999
";

#[test]
fn all_requires_every_member() {
    let completion = complete(&task_with(
        TWO_CHECKS,
        "    ALL: [REF(verify.true), REF(verify.false)]",
    ));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::All);
    assert!(!success.satisfied());
    assert!(!completion.succeeded());
}

#[test]
fn any_needs_one_member() {
    let completion = complete(&task_with(
        TWO_CHECKS,
        "    ANY: [REF(verify.true), REF(verify.false)]",
    ));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::Any);
    assert!(success.satisfied());
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn none_is_satisfied_when_no_member_holds() {
    let completion = complete(&task_with(TWO_CHECKS, "    NONE: [REF(verify.false)]"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::None);
    assert!(success.satisfied());
    assert!(completion.succeeded(), "{}", completion.serialize());

    let completion = complete(&task_with(TWO_CHECKS, "    NONE: [REF(verify.true)]"));
    assert!(!completion
        .verdict()
        .success
        .as_ref()
        .expect("declared")
        .satisfied());
}

#[test]
fn a_success_member_reads_one_scheduled_result_rather_than_rerunning_it() {
    // "Check references observe one scheduled result rather than rerunning the
    // check." One VERIFY, read once by SUCCESS, appears exactly once in the
    // results list.
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.once
    ASSERT: REF(output.value) == 7
",
        "    ALL: [REF(verify.once), REF(verify.once)]",
    ));
    let runs = completion
        .checks()
        .results()
        .iter()
        .filter(|c| c.id == "verify.once")
        .count();
    assert_eq!(runs, 1, "referencing a check twice does not run it twice");
}

#[test]
fn the_first_true_failure_in_source_order_is_selected() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.first
    WHEN: REF(output.value) == 7
    STATUS: status.partial

FAILURE:
    ID: failure.second
    WHEN: REF(output.value) == 7
    STATUS: status.stopped
",
        "    ALL: TRUE",
    ));
    let failure = completion.verdict().failure.as_ref().expect("one selected");
    assert_eq!(failure.id, "failure.first");
    assert_eq!(completion.terminal_status(), "status.partial");
}

#[test]
fn a_false_when_selects_no_clause() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.never
    WHEN: FALSE
    STATUS: status.failed
",
        "    ALL: TRUE",
    ));
    assert!(
        completion.verdict().failure.is_none(),
        "FALSE does not select a clause"
    );
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn a_selected_failure_prevents_success_even_when_success_is_true() {
    // "A selected declared failure prevents success even when other SUCCESS
    // conditions are TRUE."
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.always
    WHEN: TRUE
    STATUS: status.partial
",
        "    ALL: TRUE",
    ));
    assert!(completion
        .verdict()
        .success
        .as_ref()
        .expect("declared")
        .satisfied());
    assert!(!completion.succeeded());
    assert_eq!(completion.terminal_status(), "status.partial");
}

#[test]
fn a_selected_failure_adds_no_generic_success_diagnostic() {
    // "A selected legal mapping prevents success without adding generic
    // error.success.unsatisfied."
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.always
    WHEN: TRUE
    STATUS: status.partial
",
        "    ALL: TRUE",
    ));
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.success.unsatisfied"),
        "{}",
        completion.serialize()
    );
}

#[test]
fn a_failure_error_field_is_classification_and_emits_nothing() {
    // "Its optional ERROR is a classification of that declared failure, not
    // emission of a new diagnostic at an arbitrary stage, and therefore raises
    // no event by itself."
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.classified
    WHEN: TRUE
    STATUS: status.partial
    ERROR: error.host.constraint
",
        "    ALL: TRUE",
    ));
    let failure = completion.verdict().failure.as_ref().expect("selected");
    assert_eq!(
        failure.classification.as_deref(),
        Some("error.host.constraint")
    );
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.host.constraint"),
        "ERROR classifies; it does not emit"
    );
}

#[test]
fn a_missing_failure_condition_uses_required_missing_and_considers_no_later_clause() {
    // A skipped check is the canonical way a required read yields MISSING: it
    // is statically a BOOLEAN, and "A skipped check has no result; an explicit
    // required read of that absent result uses ordinary MISSING behavior".
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.skipped
    WHEN: FALSE
    ASSERT: TRUE

FAILURE:
    ID: failure.missing
    WHEN: REF(verify.skipped)
    STATUS: status.partial

FAILURE:
    ID: failure.later
    WHEN: TRUE
    STATUS: status.stopped
",
        "    ALL: TRUE",
    ));
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.required.missing"),
        "{}",
        completion.serialize()
    );
    // "MISSING or UNKNOWN is a required-condition failure ... and follows
    // ordinary diagnostic handling before a later clause is considered." The
    // emitted diagnostic fixes the status, so the later clause never maps one.
    assert_ne!(completion.terminal_status(), "status.stopped");
}

#[test]
fn an_unknown_failure_condition_uses_value_unknown() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.unknown
    WHEN: UNKNOWN AND TRUE
    STATUS: status.partial
",
        "    ALL: TRUE",
    ));
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.value.unknown"),
        "{}",
        completion.serialize()
    );
}

#[test]
fn required_evidence_that_does_not_resolve_uses_evidence_missing() {
    let completion = complete(&task_with(
        "
EVIDENCE:
    ID: evidence.absent
    TYPE: STRING
    REQUIRED: TRUE

VERIFY:
    ID: verify.value
    ASSERT: REF(output.value) == 7
    EVIDENCE: REF(evidence.absent)
",
        "    ALL: [REF(verify.value)]",
    ));
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "{}",
        completion.serialize()
    );
    assert!(
        !completion.succeeded(),
        "root success needs all required evidence to exist"
    );
}

#[test]
fn resolved_evidence_satisfies_its_obligation() {
    let completion = complete(&task_with(
        "
EVIDENCE:
    ID: evidence.present
    TYPE: STRING
    VALUE: \"the action returned the input\"
    PROVENANCE: \"Declared directly in this specification.\"

VERIFY:
    ID: verify.value
    ASSERT: REF(output.value) == 7
    EVIDENCE: REF(evidence.present)
",
        "    ALL: [REF(verify.value)]",
    ));
    let record = completion
        .evidence()
        .record("evidence.present")
        .expect("collected because a check that ran named it");
    assert!(record.satisfied(), "{}", completion.serialize());
    assert!(completion.evidence().complete());
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn optional_evidence_that_does_not_resolve_emits_nothing() {
    let completion = complete(&task_with(
        "
EVIDENCE:
    ID: evidence.optional
    TYPE: STRING
    REQUIRED: FALSE

VERIFY:
    ID: verify.value
    ASSERT: REF(output.value) == 7
    EVIDENCE: REF(evidence.optional)
",
        "    ALL: [REF(verify.value)]",
    ));
    let record = completion
        .evidence()
        .record("evidence.optional")
        .expect("collected");
    assert!(!record.satisfied());
    assert!(!record.blocks(), "an optional obligation does not block");
    assert!(!completion
        .diagnostics()
        .iter()
        .any(|d| d.id.as_registry_str() == "error.evidence.missing"),);
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn evidence_a_skipped_check_declared_is_not_required() {
    // A skipped check produced no result, so it imposes no evidence obligation.
    let completion = complete(&task_with(
        "
EVIDENCE:
    ID: evidence.unused
    TYPE: STRING
    REQUIRED: TRUE

VERIFY:
    ID: verify.skipped
    WHEN: FALSE
    ASSERT: TRUE
    EVIDENCE: REF(evidence.unused)
",
        "    ALL: TRUE",
    ));
    assert!(
        completion.evidence().record("evidence.unused").is_none(),
        "{}",
        completion.serialize()
    );
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn an_unsatisfied_success_with_nothing_else_uses_success_unsatisfied() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.optional
    REQUIRED: FALSE
    ASSERT: FALSE
",
        "    ALL: [REF(verify.optional)]",
    ));
    assert_eq!(completion.terminal().reason, Reason::SuccessUnsatisfied);
    assert!(completion
        .diagnostics()
        .iter()
        .any(|d| d.id.as_registry_str() == "error.success.unsatisfied"));
    assert_eq!(completion.terminal_status(), "status.failed");
}
