//! Step 8: selection, applicability, prerequisites and blocking semantics of
//! pre-effect `VALIDATE`.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/check_selection_contract`.

mod common;

use common::*;
use lcl_semantics::{Outcome, Selection, Value};

const SUBJECT: &str = "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n";

fn check<'a>(planned: &'a lcl_semantics::Planned, id: &str) -> &'a lcl_semantics::CheckResult {
    planned
        .partial_plan()
        .checks()
        .iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| {
            panic!(
                "`{id}` must be selected; selected: {:?}",
                planned
                    .partial_plan()
                    .checks()
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

#[test]
fn a_targetless_validate_applies_to_the_invocation() {
    // "A targetless clause applies to that invocation."
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert_eq!(result.selection, Selection::Targetless);
    assert_eq!(result.outcome, Some(Value::Boolean(true)));
}

#[test]
fn a_validate_targeting_a_graph_member_is_selected() {
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    TARGET: REF(action.one)\n    ASSERT: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert!(matches!(result.selection, Selection::GraphMember(_)));
}

#[test]
fn a_validate_targeting_a_referenced_data_declaration_is_selected() {
    // "a data/output declaration explicitly referenced by that graph".
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    TARGET: REF(data.subject)\n    ASSERT: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert!(matches!(
        result.selection,
        Selection::ReferencedDeclaration(_)
    ));
}

#[test]
fn a_validate_targeting_an_unreached_declaration_is_not_selected() {
    // "Match declaration identity or material identity ...; never expand
    // ambient resources."
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.unrelated\n    TYPE: STRING\n    VALUE: \"y\"\n\nVALIDATE:\n    ID: validate.one\n    TARGET: REF(data.unrelated)\n    ASSERT: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert!(
        planned.partial_plan().checks().is_empty(),
        "an unreached target selects nothing: {:?}",
        planned
            .partial_plan()
            .checks()
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "an unselected FALSE check must not fail the invocation"
    );
}

#[test]
fn a_material_target_a_graph_action_acts_on_selects_the_check() {
    // Decision witness CLOSURE-065: "An ACTION acts on PATH("/a"); a VERIFY
    // observes the same PATH value. The exact observed material target selects
    // ... without requiring a DATA declaration or ambient resource expansion."
    // The same selection rule governs pre-effect VALIDATE.
    let source = format!(
        "{HEADER}\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nWORKSPACE:\n    ID: workspace.one\n    PATH: PATH(\"/ws\")\n    MODE: mode.read_write\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: PATH(\"/ws/a\")\n\nVALIDATE:\n    ID: validate.one\n    TARGET: PATH(\"/ws/a\")\n    ASSERT: TRUE\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert_eq!(
        result.selection,
        Selection::MaterialTarget("PATH(\"/ws/a\")".to_string())
    );
}

// ---------------------------------------------------------------------------
// Applicability
// ---------------------------------------------------------------------------

#[test]
fn an_absent_when_means_true() {
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(ids(&planned), vec!["error.validation.failed".to_string()]);
}

#[test]
fn a_false_when_skips_applicability_and_leaves_no_result() {
    // "A skipped check has no result; an explicit required read of that absent
    // result uses ordinary MISSING behavior, never implicit TRUE."
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: FALSE\n    WHEN: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert_eq!(result.outcome, None, "a skipped check has no result");
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "an inapplicable FALSE check does not block"
    );
}

#[test]
fn a_when_that_cannot_be_decided_does_not_make_the_check_applicable() {
    // "a WHEN evaluating MISSING or UNKNOWN does not match".
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.gate\n    TYPE: BOOLEAN\n    SOURCE: PATH(\"/ws/gate\")\n\nVALIDATE:\n    ID: validate.one\n    ASSERT: FALSE\n    WHEN: REF(input.gate)\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(check(&planned, "validate.one").outcome, None);
    assert_eq!(planned.outcome(), Outcome::Planned);
}

// ---------------------------------------------------------------------------
// REQUIRED controls blocking, not running
// ---------------------------------------------------------------------------

#[test]
fn a_required_false_assertion_uses_error_validation_failed() {
    // "A required FALSE VALIDATE assertion uses error.validation.failed."
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: FALSE\n    REQUIRED: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(ids(&planned), vec!["error.validation.failed".to_string()]);
    assert_eq!(planned.terminal_status(), Some("status.invalid"));
    assert_eq!(
        planned.primary().map(|d| d.failure_phase.to_string()),
        Some("pre_effect".to_string())
    );
}

#[test]
fn an_optional_false_check_still_runs_and_records_its_outcome() {
    // "Optional FALSE checks retain their Boolean domain outcome without
    // emitting a required-check failure." And: "REQUIRED controls whether a
    // FALSE result blocks, not whether a selected check runs."
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: FALSE\n    REQUIRED: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let result = check(&planned, "validate.one");
    assert_eq!(
        result.outcome,
        Some(Value::Boolean(false)),
        "the check ran and kept its Boolean outcome"
    );
    assert!(!result.required);
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "an optional FALSE check does not block"
    );
}

#[test]
fn a_required_check_demanding_a_missing_value_blocks() {
    // "Required demanded MISSING and UNKNOWN use error.required.missing and
    // error.value.unknown."
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: BOOLEAN\n    SOURCE: PATH(\"/ws/in\")\n\nVALIDATE:\n    ID: validate.one\n    ASSERT: REF(input.one)\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(ids(&planned), vec!["error.required.missing".to_string()]);
    assert_eq!(planned.terminal_status(), Some("status.blocked"));
}

#[test]
fn a_required_check_demanding_an_unknown_value_blocks() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    DEFAULT: TRUE\n\nVALIDATE:\n    ID: validate.one\n    ASSERT: REF(input.one)\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &lcl_semantics::Invocation::new().with("input.one", Value::Unknown),
    );
    assert_eq!(ids(&planned), vec!["error.value.unknown".to_string()]);
    assert_eq!(planned.terminal_status(), Some("status.blocked"));
}

// ---------------------------------------------------------------------------
// Prerequisites
// ---------------------------------------------------------------------------

#[test]
fn an_optional_validate_referenced_by_success_is_still_evaluated_before_effects() {
    // Decision witness CLOSURE-064: "An optional VALIDATE is a prerequisite
    // referenced by TASK SUCCESS. Evaluate the selected applicable VALIDATE
    // before effects; REQUIRED FALSE does not suppress evaluation or emit a
    // required-check failure."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nVALIDATE:\n    ID: validate.optional\n    ASSERT: FALSE\n    REQUIRED: FALSE\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(validate.optional)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    let result = check(&planned, "validate.optional");
    assert_eq!(
        result.outcome,
        Some(Value::Boolean(false)),
        "the optional prerequisite was evaluated before effects"
    );
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "REQUIRED FALSE emits no required-check failure"
    );
}

#[test]
fn a_prerequisite_is_evaluated_before_the_check_that_reads_it() {
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.second\n    ASSERT: REF(validate.first)\n\nVALIDATE:\n    ID: validate.first\n    ASSERT: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let order: Vec<String> = planned
        .partial_plan()
        .checks()
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let first = order.iter().position(|id| id == "validate.first");
    let second = order.iter().position(|id| id == "validate.second");
    assert!(
        first < second,
        "a prerequisite must be evaluated first, got {order:?}"
    );
}

#[test]
fn a_prerequisite_cycle_uses_error_reference_cycle() {
    // "A prerequisite cycle uses error.reference.cycle."
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: REF(validate.two)\n\nVALIDATE:\n    ID: validate.two\n    ASSERT: REF(validate.one)\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert!(!ids(&planned).is_empty());
    assert!(
        ids(&planned).iter().all(|id| id == "error.reference.cycle"),
        "{:?}",
        ids(&planned)
    );
}

// ---------------------------------------------------------------------------
// No deferral past effects
// ---------------------------------------------------------------------------

#[test]
fn a_check_cannot_defer_itself_past_effects() {
    // "Every selected pre-effect VALIDATE ... must be evaluable before effects
    // and cannot depend on future OUTPUT or a post-execution check." A check
    // reading an unbound OUTPUT reads MISSING, and a required one blocks rather
    // than postponing itself.
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nOUTPUT:\n    ID: output.one\n    TYPE: STRING\n    FORMAT: format.plain_text\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n    OUTPUT: REF(output.one)\n\nVALIDATE:\n    ID: validate.one\n    ASSERT: REF(output.one) == \"done\"\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(
        planned.outcome(),
        Outcome::Rejected,
        "a pre-effect check depending on a future OUTPUT must fail here, not defer"
    );
    assert_eq!(planned.terminal_status(), Some("status.blocked"));
}
