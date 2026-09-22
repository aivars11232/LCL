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

/// One control operation over the root task itself.
fn controlled_root(operation: &str, parameter: &str) -> Completion {
    complete(&task_document(&format!(
        "
GOAL:
    ID: goal.control
    ASSERT: TRUE

ACTION:
    ID: action.control
    OPERATION: {operation}
    TARGET: REF(task.control){parameter}

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.control
    GOAL: REF(goal.control)
    ACTION: REF(action.control)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.control)
"
    )))
}

#[test]
fn a_committed_cancel_ends_the_root_cancelled_and_evidences_its_reason() {
    // "Cancellation yields status.cancelled", `error.cancelled` is "Invoking
    // authority cancelled execution", and the row's postconditions are "target
    // reaches status.cancelled" and "reason is evidenced".
    let completion = controlled_root(
        "core.cancel",
        "\n    PARAMETER:\n        NAME: reason\n        TYPE: STRING\n        \
         REQUIRED: TRUE\n        VALUE: \"the owner cancelled it\"",
    );
    assert_eq!(completion.terminal().status, "status.cancelled");
    assert!(
        matches!(
            &completion.terminal().reason,
            Reason::PrimaryDiagnostic(primary) if primary.id == "error.cancelled"
        ),
        "{}",
        completion.serialize()
    );
}

#[test]
fn a_committed_stop_ends_the_root_stopped() {
    // "core.stop yields status.stopped unless another declared failure status
    // applies."
    let completion = controlled_root("core.stop", "");
    assert_eq!(completion.terminal().status, "status.stopped");
    assert_eq!(completion.terminal().reason, Reason::DeclaredStop);
}

/// One `core.verify` that names `evidence.item` through its evidence parameter.
fn verified_with_evidence(value: &str, required: &str) -> Completion {
    complete(&task_document(&format!(
        "
DATA:
    ID: data.number
    TYPE: INTEGER
    VALUE: 3

DATA:
    ID: data.members
    TYPE: LIST[INTEGER]
    VALUE: [1, 2]

EVIDENCE:
    ID: evidence.item
    TYPE: INTEGER
    VALUE: {value}
    REQUIRED: {required}

GOAL:
    ID: goal.verify
    ASSERT: TRUE

ACTION:
    ID: action.verify
    OPERATION: core.verify
    TARGET: REF(data.number)
    PARAMETER:
        NAME: assertion
        TYPE: BOOLEAN
        REQUIRED: TRUE
        VALUE: REF(data.number) == 3
    PARAMETER:
        NAME: evidence
        TYPE: LIST[REFERENCE[REF(evidence.item)]]
        REQUIRED: FALSE
        VALUE: [REF(evidence.item)]

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.verify
    GOAL: REF(goal.verify)
    ACTION: REF(action.verify)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.verify)
"
    )))
}

#[test]
fn evidence_an_operation_parameter_named_is_required_too() {
    // "It is collected because something that ran named it." An activated
    // ACTION names the evidence it requires in its own EVIDENCE field and in
    // an `evidence` parameter, which `core.verify` calls "Required evidence
    // declarations".
    let completion = verified_with_evidence("REF(data.members)[5]", "TRUE");
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());

    // The same parameter, resolved, imposes nothing.
    let resolved = verified_with_evidence("REF(data.members)[1]", "TRUE");
    assert!(
        resolved.diagnostics().is_empty(),
        "{}",
        resolved.serialize()
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

/// `05_SEMANTICS/10`: a FAILURE condition "follows ordinary diagnostic handling
/// before a later clause is considered."
///
/// Regression, PRETEST-01 F05. `Err(_) => continue` discarded the demand
/// fault, so the later clause silently selected `status.stopped`.
#[test]
fn a_faulted_failure_condition_reports_its_diagnostic_before_a_later_clause() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.fault
    WHEN: 1 / (REF(output.value) - 7) == 1
    STATUS: status.partial

FAILURE:
    ID: failure.later
    WHEN: TRUE
    STATUS: status.stopped
",
        "    ALL: TRUE",
    ));
    let ids: Vec<&str> = completion
        .diagnostics()
        .iter()
        .map(|d| d.id.as_registry_str())
        .collect();
    assert_eq!(
        ids,
        vec!["error.numeric.division_by_zero"],
        "{}",
        completion.serialize()
    );
    assert_ne!(completion.terminal_status(), "status.stopped");
    assert!(!completion.succeeded());
}

/// Regression, PRETEST-01 F05 (same root). A root `SUCCESS` expression whose
/// demand faulted was recorded as UNKNOWN and the fault was discarded.
#[test]
fn a_faulted_success_expression_reports_its_diagnostic() {
    let completion = complete(&task_with("", "    ALL: 1 / (REF(output.value) - 7) == 1"));
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.numeric.division_by_zero"),
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

// ---------------------------------------------------------------------------
// C-01 — an empty SUCCESS list has no members
// ---------------------------------------------------------------------------
//
// `field_signatures_v0.1.0.json#/value_kind_registry/boolean_or_reference_list`
// is "A boolean_expression **or a possibly empty LIST** containing only REF
// values whose targets evaluate to BOOLEAN or UNKNOWN". So `ALL: []` is
// written, legal, and a list of nothing.
//
// The collection was recognised by whether it yielded any reference, so an
// empty list yielded none and fell through to the scalar branch — where `[]`
// was demanded as a single expression and became ONE member whose value is a
// LIST. A list is not a Boolean, so the quantifier saw one unknown member and
// answered UNKNOWN, where zero members answer vacuously.

#[test]
fn an_empty_all_has_no_members_and_holds_vacuously() {
    let completion = complete(&task_with(TWO_CHECKS, "    ALL: []"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::All);
    assert!(
        success.members.is_empty(),
        "an empty list is zero members, not one list-valued member: {:?}",
        success.members
    );
    assert!(
        success.satisfied(),
        "every member of nothing holds: {}",
        completion.serialize()
    );
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn an_empty_any_has_no_members_and_does_not_hold() {
    let completion = complete(&task_with(TWO_CHECKS, "    ANY: []"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::Any);
    assert!(success.members.is_empty(), "{:?}", success.members);
    assert!(
        !success.satisfied(),
        "no member of nothing holds: {}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

#[test]
fn an_empty_none_has_no_members_and_holds_vacuously() {
    let completion = complete(&task_with(TWO_CHECKS, "    NONE: []"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.quantifier, Quantifier::None);
    assert!(success.members.is_empty(), "{:?}", success.members);
    assert!(
        success.satisfied(),
        "no member of nothing holds, which is what NONE asks: {}",
        completion.serialize()
    );
    assert!(completion.succeeded(), "{}", completion.serialize());
}

/// The scalar control: a Boolean expression is still one member, not a list.
#[test]
fn a_scalar_boolean_success_is_one_member() {
    let completion = complete(&task_with(TWO_CHECKS, "    ALL: TRUE"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.members.len(), 1, "{:?}", success.members);
    assert!(success.satisfied(), "{}", completion.serialize());

    let completion = complete(&task_with(TWO_CHECKS, "    ALL: FALSE"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.members.len(), 1, "{:?}", success.members);
    assert!(!success.satisfied());
}

/// The nonempty control: real references are still read as members.
#[test]
fn a_nonempty_reference_list_is_still_its_members() {
    let completion = complete(&task_with(TWO_CHECKS, "    ALL: [REF(verify.true)]"));
    let success = completion.verdict().success.as_ref().expect("declared");
    assert_eq!(success.members.len(), 1, "{:?}", success.members);
    assert!(success.satisfied(), "{}", completion.serialize());
}

// ---------------------------------------------------------------------------
// C-02 — a failed condition is not stepped over
// ---------------------------------------------------------------------------
//
// `05_SEMANTICS/10` orders these: "An existing primary unhandled diagnostic
// always fixes status by its resolved default_status ... A FAILURE mapping
// cannot override that diagnostic. **Otherwise** evaluate applicable FAILURE
// clauses in source declaration order and select the first whose WHEN is
// TRUE." And a MISSING or UNKNOWN condition "is a required-condition failure
// ... and follows ordinary diagnostic handling before a later clause is
// considered."
//
// So the scan proceeds on FALSE, and stops on a condition that failed. This
// step runs after execution, where handler selection has already finished, so
// a diagnostic raised here is unhandled by construction and fixes the status —
// and no later clause may map one over it.
//
// The existing tests below assert the terminal status, which precedence
// already protected. What they do not see is that the later clause was still
// *selected*, carrying its own requested status, classification and required
// evidence into the verdict. That is what these check.

#[test]
fn a_missing_failure_condition_selects_no_later_clause() {
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
    ERROR: error.execution.action
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
    assert!(
        completion.verdict().failure.is_none(),
        "a clause after a failed condition was selected: {:?}",
        completion.verdict().failure
    );
    assert_ne!(completion.terminal_status(), "status.stopped");
}

#[test]
fn an_unknown_failure_condition_selects_no_later_clause() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.unknown
    WHEN: UNKNOWN AND TRUE
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
            .any(|d| d.id.as_registry_str() == "error.value.unknown"),
        "{}",
        completion.serialize()
    );
    assert!(
        completion.verdict().failure.is_none(),
        "a clause after an UNKNOWN condition was selected: {:?}",
        completion.verdict().failure
    );
}

#[test]
fn a_faulted_failure_condition_selects_no_later_clause() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.fault
    WHEN: 1 / (REF(output.value) - 7) == 1
    STATUS: status.partial

FAILURE:
    ID: failure.later
    WHEN: TRUE
    STATUS: status.stopped
",
        "    ALL: TRUE",
    ));
    assert!(
        completion.verdict().failure.is_none(),
        "a clause after a faulted condition was selected: {:?}",
        completion.verdict().failure
    );
    // The original diagnostic keeps its own identity, stage and source.
    let fault = completion
        .diagnostics()
        .iter()
        .find(|d| d.id.as_registry_str() == "error.numeric.division_by_zero")
        .expect("the evaluator's own diagnostic");
    assert_eq!(fault.source.as_str(), "root.lcl");
}

/// The control: FALSE does not select a clause and does not stop the scan.
#[test]
fn a_false_condition_still_lets_a_later_clause_be_selected() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.first
    WHEN: FALSE
    STATUS: status.partial

FAILURE:
    ID: failure.later
    WHEN: TRUE
    STATUS: status.stopped
",
        "    ALL: TRUE",
    ));
    let failure = completion
        .verdict()
        .failure
        .as_ref()
        .expect("FALSE does not select a clause, and the next one is TRUE");
    assert_eq!(failure.id, "failure.later");
    assert_eq!(completion.terminal_status(), "status.stopped");
}

// ---------------------------------------------------------------------------
// C-03 — a declared checksum or provenance has a form, and must have it
// ---------------------------------------------------------------------------
//
// "EVIDENCE must be observable, typed, and traceable; declared required
// provenance/checksum must resolve." What "resolve" means for each is settled
// by the value kind the field is declared with:
//
//   CHECKSUM   `sha256_string` — "A STRING containing 'sha256:' followed by
//              exactly 64 lowercase hexadecimal digits."
//   PROVENANCE `string_uri_or_evidence_reference` — "One STRING, URI, or REF
//              resolving to EVIDENCE."
//
// Both were accepted on being merely non-empty, so `CHECKSUM: "x"` made a
// required evidence declaration traceable and a `PROVENANCE` REF that resolves
// to nothing — or to a declaration that is not EVIDENCE — did too.

/// One required EVIDENCE declaration with the given extra fields.
fn evidence_with(fields: &str) -> Completion {
    complete(&task_document(&format!(
        "
DATA:
    ID: data.number
    TYPE: INTEGER
    VALUE: 3

EVIDENCE:
    ID: evidence.item
    TYPE: INTEGER
    VALUE: 3
    REQUIRED: TRUE
{fields}
GOAL:
    ID: goal.verify
    ASSERT: TRUE

ACTION:
    ID: action.verify
    OPERATION: core.verify
    TARGET: REF(data.number)
    PARAMETER:
        NAME: assertion
        TYPE: BOOLEAN
        REQUIRED: TRUE
        VALUE: REF(data.number) == 3
    PARAMETER:
        NAME: evidence
        TYPE: LIST[REFERENCE[REF(evidence.item)]]
        REQUIRED: FALSE
        VALUE: [REF(evidence.item)]

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.verify
    GOAL: REF(goal.verify)
    ACTION: REF(action.verify)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.verify)
"
    )))
}

const GOOD_SUM: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn a_declared_checksum_must_have_the_registered_form() {
    for bad in [
        "\"not-a-checksum\"",
        "\"x\"",
        // No prefix.
        "\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"",
        // Too short.
        "\"sha256:0123456789abcdef\"",
        // Uppercase is not "lowercase hexadecimal digits".
        "\"sha256:0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef\"",
        // Not hexadecimal.
        "\"sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz\"",
    ] {
        let completion = evidence_with(&format!("    CHECKSUM: {bad}\n"));
        assert!(
            completion
                .diagnostics()
                .iter()
                .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
            "CHECKSUM {bad} was accepted as traceable: {}",
            completion.serialize()
        );
    }
}

/// The control: a well-formed checksum resolves, and success is not blocked.
#[test]
fn a_well_formed_checksum_resolves() {
    let completion = evidence_with(&format!("    CHECKSUM: \"{GOOD_SUM}\"\n"));
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "{}",
        completion.serialize()
    );
    assert!(
        completion
            .serialize()
            .contains("evidence.item required=true satisfied=true"),
        "{}",
        completion.serialize()
    );
}

/// A `PROVENANCE` reference that does not name EVIDENCE is already refused,
/// one stage earlier.
///
/// `string_uri_or_evidence_reference` is "One STRING, URI, or REF resolving to
/// EVIDENCE", and resolution enforces the reference form: a REF to a DATA
/// declaration is `error.reference.kind` — "`data.number` resolves to a DATA,
/// but EVIDENCE.PROVENANCE accepts EVIDENCE" — so such a document never
/// reaches completion at all. Recorded here so the guard is not mistaken for
/// a gap and re-implemented above it.
#[test]
fn a_string_provenance_resolves() {
    let completion = evidence_with("    PROVENANCE: \"the release owner's record\"\n");
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "{}",
        completion.serialize()
    );
    assert!(
        completion
            .serialize()
            .contains("evidence.item required=true satisfied=true"),
        "{}",
        completion.serialize()
    );
}

/// Optional evidence with the same defect does not block success.
#[test]
fn an_optional_declarations_malformed_checksum_does_not_block() {
    let completion = complete(&task_document(
        "
DATA:
    ID: data.number
    TYPE: INTEGER
    VALUE: 3

EVIDENCE:
    ID: evidence.item
    TYPE: INTEGER
    VALUE: 3
    REQUIRED: FALSE
    CHECKSUM: \"not-a-checksum\"

GOAL:
    ID: goal.verify
    ASSERT: TRUE

ACTION:
    ID: action.verify
    OPERATION: core.verify
    TARGET: REF(data.number)
    PARAMETER:
        NAME: assertion
        TYPE: BOOLEAN
        REQUIRED: TRUE
        VALUE: REF(data.number) == 3
    PARAMETER:
        NAME: evidence
        TYPE: LIST[REFERENCE[REF(evidence.item)]]
        REQUIRED: FALSE
        VALUE: [REF(evidence.item)]

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.verify
    GOAL: REF(goal.verify)
    ACTION: REF(action.verify)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.verify)
",
    ));
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "an optional declaration does not block on its checksum: {}",
        completion.serialize()
    );
}

/// A required EVIDENCE whose SOURCE names something this run observed, and one
/// whose SOURCE names something it did not.
///
/// "EVIDENCE must be **observable**, typed, and traceable", and root success
/// requires that "all required evidence exists". A rendered `SOURCE` string is
/// syntax; on its own it establishes nothing, and treating it as sufficient is
/// inferring existence from the fact that someone wrote it down.
///
/// This layer performs no effect and does not fetch anything. What it consults
/// is what the execution already observed — `Observation` calls that "the
/// whole admissible target universe for a targeted post-execution check.
/// Anything else is not observed."
fn evidence_sourced_from(reference: &str, required: &str) -> Completion {
    complete(&task_document(&format!(
        "
INPUT:
    ID: input.path
    TYPE: PATH
    VALUE: PATH(\"/srv/data/report.txt\")

OUTPUT:
    ID: output.path
    TYPE: PATH
    FORMAT: format.plain_text

OUTPUT:
    ID: output.unbound
    TYPE: PATH
    FORMAT: format.plain_text

EVIDENCE:
    ID: evidence.item
    TYPE: PATH
    SOURCE: REF({reference})
    REQUIRED: {required}

GOAL:
    ID: goal.copy
    ASSERT: TRUE

ACTION:
    ID: action.copy
    OPERATION: core.return
    TARGET: REF(input.path)
    OUTPUT: REF(output.path)
    EVIDENCE: REF(evidence.item)

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.copy
    GOAL: REF(goal.copy)
    INPUT: REF(input.path)
    ACTION: REF(action.copy)
    OUTPUT: REF(output.path)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.copy)
"
    )))
}

#[test]
fn required_evidence_from_an_observed_source_is_established() {
    // `output.value` was bound by the action that ran.
    let completion = evidence_sourced_from("output.path", "TRUE");
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "{}",
        completion.serialize()
    );
    assert!(
        completion
            .serialize()
            .contains("evidence.item required=true satisfied=true"),
        "{}",
        completion.serialize()
    );
}

#[test]
fn required_evidence_from_an_unobserved_source_is_not_established() {
    // `output.unbound` is declared and nothing bound it, so nothing observed
    // it: "an output not yet bound yields MISSING".
    let completion = evidence_sourced_from("output.unbound", "TRUE");
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "a source nothing observed established required evidence: {}",
        completion.serialize()
    );
    assert!(!completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn optional_evidence_from_an_unobserved_source_does_not_block() {
    let completion = evidence_sourced_from("output.unbound", "FALSE");
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "an optional declaration does not block: {}",
        completion.serialize()
    );
}

// ---------------------------------------------------------------------------
// V-01 — the case report 03 did not cover: an inner TASK's required evidence
// ---------------------------------------------------------------------------
//
// Report 03 established that a delegated TASK's own `SUCCESS` and its own
// required `OUTPUT` are root obligations and do not govern the row that
// delegated to it: `05_SEMANTICS/10` scopes both to "a TASK **execution
// root**", and a delegated TASK is a child invocation.
//
// Required EVIDENCE is not scoped the same way, and this records what it
// actually does rather than assuming it follows. Evidence "is collected
// because something that ran named it" — so an inner ACTION that ran inside
// the graph and named its evidence contributes that obligation to the
// completion of the run that contains it. The distinction is real: SUCCESS is
// evaluated *at* a root, while evidence is collected *from what ran*.
//
// This is behaviour, not an interpretation's authority. It is recorded for the
// independent language review named in the assignment, not as a settled
// reading of the hierarchy.

#[test]
fn an_inner_tasks_required_evidence_is_collected_from_what_ran() {
    let completion = complete(&task_document(
        "
INPUT:
    ID: input.path
    TYPE: PATH
    VALUE: PATH(\"/srv/data/report.txt\")

OUTPUT:
    ID: output.path
    TYPE: PATH
    FORMAT: format.plain_text

OUTPUT:
    ID: output.unbound
    TYPE: PATH
    FORMAT: format.plain_text

EVIDENCE:
    ID: evidence.inner
    TYPE: PATH
    SOURCE: REF(output.unbound)
    REQUIRED: TRUE

GOAL:
    ID: goal.inner
    ASSERT: TRUE

ACTION:
    ID: action.inner
    OPERATION: core.return
    TARGET: REF(input.path)
    OUTPUT: REF(output.path)
    EVIDENCE: REF(evidence.inner)

SUCCESS:
    ID: success.inner
    ALL: TRUE

TASK:
    ID: task.inner
    GOAL: REF(goal.inner)
    INPUT: REF(input.path)
    ACTION: REF(action.inner)
    OUTPUT: REF(output.path)
    SUCCESS: REF(success.inner)

ACTION:
    ID: action.wrapper
    OPERATION: core.execute
    TARGET: REF(task.inner)

GOAL:
    ID: goal.outer
    ASSERT: TRUE

SUCCESS:
    ID: success.outer
    ALL: TRUE

TASK:
    ID: task.outer
    GOAL: REF(goal.outer)
    ACTION: REF(action.wrapper)
    SUCCESS: REF(success.outer)

EXECUTE:
    REFERENCE: REF(task.outer)
",
    ));

    // The inner action ran, under the wrapper's delegation.
    assert!(
        completion.observation().activated("action.inner"),
        "the delegated action ran: {}",
        completion.observation().serialize()
    );
    // And the evidence it named is collected and unestablished, so the run
    // that contains it does not succeed. Evidence is collected from what ran;
    // it is not scoped to a root the way SUCCESS is.
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.evidence.missing"),
        "evidence named by an action that ran is collected: {}",
        completion.serialize()
    );
    assert!(!completion.succeeded(), "{}", completion.serialize());
}
