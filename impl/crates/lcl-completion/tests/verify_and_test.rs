//! Post-execution `VERIFY` and `TEST`: selection, applicability, assertion.

mod common;

use common::*;
use lcl_completion::{CheckKind, Completion, Selection, SkipReason};

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

/// A task whose single action copies an input to an output, plus whatever
/// completion blocks the test is about.
fn task_with(blocks: &str, success_members: &str) -> String {
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
    ALL: {success_members}

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

#[test]
fn a_targetless_verify_applies_to_the_invocation() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.value
    ASSERT: REF(output.value) == 7
",
        "[REF(verify.value)]",
    ));
    let check = completion
        .checks()
        .result("verify.value")
        .expect("a targetless VERIFY is selected");
    assert_eq!(check.selection, Selection::Targetless);
    assert_eq!(check.kind, CheckKind::Verify);
    assert!(check.held());
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn a_verify_targeting_an_activated_producer_is_selected() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.value
    TARGET: REF(action.copy)
    ASSERT: REF(output.value) == 7
",
        "[REF(verify.value)]",
    ));
    let check = completion
        .checks()
        .result("verify.value")
        .expect("the target ran, so the check is selected");
    assert_eq!(
        check.selection,
        Selection::ActivatedProducer("action.copy".to_string())
    );
}

#[test]
fn a_verify_targeting_a_bound_output_is_selected_as_an_observed_target() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.value
    TARGET: REF(output.value)
    ASSERT: REF(output.value) == 7
",
        "[REF(verify.value)]",
    ));
    let check = completion
        .checks()
        .result("verify.value")
        .expect("a bound output is an observed target");
    assert_eq!(
        check.selection,
        Selection::ObservedTarget("output.value".to_string())
    );
}

#[test]
fn a_verify_targeting_something_the_invocation_never_reached_is_not_selected() {
    // "Targeted post-execution VERIFY applies only to an actually activated
    // producer or an observed target of that invocation."
    let completion = complete(&task_with(
        "
DATA:
    ID: data.untouched
    TYPE: INTEGER
    VALUE: 1

VERIFY:
    ID: verify.absent
    TARGET: REF(data.untouched)
    ASSERT: FALSE
",
        "TRUE",
    ));
    assert!(
        completion.checks().result("verify.absent").is_none(),
        "an unreached target selects nothing at all"
    );
    assert!(
        completion.checks().skipped().is_empty(),
        "unselected is not the same as selected-and-skipped"
    );
    // And because it never ran, its FALSE assertion cannot prevent success.
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn a_false_when_skips_the_check_and_leaves_no_result() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.skipped
    WHEN: FALSE
    ASSERT: FALSE
",
        "TRUE",
    ));
    assert!(
        completion.checks().result("verify.skipped").is_none(),
        "a skipped check has no result"
    );
    let skipped = completion
        .checks()
        .skipped()
        .iter()
        .find(|s| s.id == "verify.skipped")
        .expect("it was selected, then skipped");
    assert_eq!(skipped.reason, SkipReason::WhenFalse);
}

#[test]
fn reading_a_skipped_checks_result_is_missing_and_never_implicit_true() {
    // "A skipped check has no result; an explicit required read of that absent
    // result uses ordinary MISSING behavior, never implicit TRUE."
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.skipped
    WHEN: FALSE
    ASSERT: TRUE
",
        "[REF(verify.skipped)]",
    ));
    let success = completion
        .verdict()
        .success
        .as_ref()
        .expect("the root references a SUCCESS");
    assert_eq!(
        success.members,
        vec![("verify.skipped".to_string(), lcl_runtime::Value::Missing)],
        "a skipped check reads MISSING, not TRUE"
    );
    assert!(
        !completion.succeeded(),
        "a MISSING member cannot satisfy ALL"
    );
}

#[test]
fn an_absent_when_means_applicable() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.plain
    ASSERT: TRUE
",
        "[REF(verify.plain)]",
    ));
    assert!(completion.checks().result("verify.plain").is_some());
    assert!(completion.checks().skipped().is_empty());
}

#[test]
fn a_required_false_verify_emits_verification_failed() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.wrong
    REQUIRED: TRUE
    ASSERT: REF(output.value) == 999
",
        "[REF(verify.wrong)]",
    ));
    let check = completion
        .checks()
        .result("verify.wrong")
        .expect("selected");
    assert!(!check.held());
    assert!(check.blocks());
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.verification.failed"),
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

/// `expression_demand_resolution`: its context is a demand "during a reachable
/// invocation, condition, verification, or completion step", its eligible map
/// gives `error.operator.operand` to "a registered SUM, MIN, or MAX reduction
/// [receiving] an empty material collection whose member type and function
/// signature are already valid", and its `resolved_stage` is `execution`.
///
/// Regression, `LCL-TASK-0020` defect 2. The evaluator raised the fault
/// correctly and this layer discarded it, recording UNKNOWN and nothing else,
/// so the registered diagnostic the registry names for this demand was never
/// emitted by anyone. CLOSURE-015 failed on it.
#[test]
fn a_reduction_over_an_empty_collection_emits_its_registered_identifier() {
    let completion = complete(&task_with(
        "
DATA:
    ID: data.empty
    TYPE: LIST[INTEGER]
    VALUE: []

VERIFY:
    ID: verify.sum
    REQUIRED: TRUE
    ASSERT: SUM(REF(data.empty)) == 0
",
        "[REF(verify.sum)]",
    ));
    let emitted: Vec<&lcl_completion::Diagnostic> = completion
        .diagnostics()
        .iter()
        .filter(|d| d.id.as_registry_str() == "error.operator.operand")
        .collect();
    assert_eq!(
        emitted.len(),
        1,
        "the demand fault is emitted exactly once: {}",
        completion.serialize()
    );
    let diagnostic = emitted[0];
    // "Retain the canonical error identifier, registered source-stage
    // metadata, resolved demand stage, and demand locus in evidence."
    assert_eq!(
        diagnostic.registered_stage.as_registry_str(),
        "static_or_expression"
    );
    assert_eq!(diagnostic.stage().as_registry_str(), "execution");
    assert_eq!(diagnostic.default_status, "status.failed");
    assert_eq!(diagnostic.declaration.as_deref(), Some("verify.sum"));
    assert!(!completion.succeeded());
}

/// The same reduction over a nonempty collection raises nothing, so the
/// diagnostic above is evidence of the empty case and not of reductions.
#[test]
fn a_reduction_over_a_nonempty_collection_raises_nothing() {
    let completion = complete(&task_with(
        "
DATA:
    ID: data.two
    TYPE: LIST[INTEGER]
    VALUE: [1, 2]

VERIFY:
    ID: verify.sum
    REQUIRED: TRUE
    ASSERT: SUM(REF(data.two)) == 3
",
        "[REF(verify.sum)]",
    ));
    assert!(
        completion.diagnostics().is_empty(),
        "{}",
        completion.serialize()
    );
    assert!(completion.succeeded());
}

#[test]
fn an_optional_false_verify_keeps_its_outcome_and_emits_nothing() {
    // "Optional FALSE checks retain their Boolean domain outcome without
    // emitting a required-check failure."
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.optional
    REQUIRED: FALSE
    ASSERT: REF(output.value) == 999
",
        "TRUE",
    ));
    let check = completion
        .checks()
        .result("verify.optional")
        .expect("REQUIRED controls blocking, not whether a selected check runs");
    assert!(!check.held(), "the Boolean outcome is retained");
    assert!(!check.blocks());
    assert!(
        !completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.verification.failed"),
        "an optional FALSE check raises no required-check failure"
    );
    assert!(
        completion.succeeded(),
        "and it does not prevent success: {}",
        completion.serialize()
    );
}

#[test]
fn a_prerequisite_is_evaluated_before_the_check_that_reads_it() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.dependent
    ASSERT: REF(verify.base) == TRUE

VERIFY:
    ID: verify.base
    ASSERT: REF(output.value) == 7
",
        "[REF(verify.dependent)]",
    ));
    let order: Vec<&str> = completion
        .checks()
        .results()
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let base = order.iter().position(|id| *id == "verify.base");
    let dependent = order.iter().position(|id| *id == "verify.dependent");
    assert!(
        base < dependent,
        "a prerequisite evaluates first: {order:?}"
    );
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn a_prerequisite_cycle_uses_reference_cycle_and_evaluates_nothing() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.left
    ASSERT: REF(verify.right) == TRUE

VERIFY:
    ID: verify.right
    ASSERT: REF(verify.left) == TRUE
",
        "TRUE",
    ));
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.reference.cycle"),
        "{}",
        completion.serialize()
    );
    assert!(
        completion.checks().results().is_empty(),
        "a chain with no first element evaluates none of its members"
    );
}

#[test]
fn a_test_declaration_is_not_run_as_an_ambient_task() {
    // "TEST declarations are activated only as an explicit TEST root or
    // operation target, never as ambient tasks."
    let completion = complete(&task_with(
        "
TEST:
    ID: test.ambient
    ASSERT: FALSE
",
        "TRUE",
    ));
    assert!(
        completion.checks().result("test.ambient").is_none(),
        "a TASK root runs no ambient TEST"
    );
    assert!(
        completion.succeeded(),
        "so its FALSE assertion cannot prevent success: {}",
        completion.serialize()
    );
}

#[test]
fn an_explicit_test_root_evaluates_its_assertion() {
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

TEST:
    ID: test.root
    ASSERT: REF(input.value) == 7

EXECUTE:
    REFERENCE: REF(test.root)
",
    );
    let completion = complete(&source);
    let check = completion
        .checks()
        .result("test.root")
        .expect("an explicit TEST root is selected");
    assert_eq!(check.selection, Selection::TestRoot);
    assert_eq!(check.kind, CheckKind::Test);
    assert!(check.held());
    assert!(completion.succeeded(), "{}", completion.serialize());
}

#[test]
fn a_test_root_compares_expected_against_actual_with_strict_equality() {
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

TEST:
    ID: test.compare
    EXPECTED: 7
    ACTUAL: REF(input.value)

EXECUTE:
    REFERENCE: REF(test.compare)
",
    );
    let completion = complete(&source);
    assert!(completion
        .checks()
        .result("test.compare")
        .expect("selected")
        .held());

    let mismatched = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

TEST:
    ID: test.compare
    EXPECTED: 8
    ACTUAL: REF(input.value)

EXECUTE:
    REFERENCE: REF(test.compare)
",
    );
    let completion = complete(&mismatched);
    let check = completion
        .checks()
        .result("test.compare")
        .expect("selected");
    assert!(!check.held());
    assert!(
        completion
            .diagnostics()
            .iter()
            .any(|d| d.id.as_registry_str() == "error.verification.failed"),
        "a required TEST root that failed uses error.verification.failed: {}",
        completion.serialize()
    );
}

#[test]
fn a_test_root_with_both_forms_requires_both_to_hold() {
    // "if both are present they must both hold."
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

TEST:
    ID: test.both
    ASSERT: TRUE
    EXPECTED: 8
    ACTUAL: REF(input.value)

EXECUTE:
    REFERENCE: REF(test.both)
",
    );
    let completion = complete(&source);
    assert!(
        !completion
            .checks()
            .result("test.both")
            .expect("selected")
            .held(),
        "a TRUE ASSERT does not rescue a FALSE comparison"
    );
}

/// Every registered identifier completion emitted, in emission order.
fn emitted(completion: &Completion) -> Vec<&'static str> {
    completion
        .diagnostics()
        .iter()
        .map(|d| d.id.as_registry_str())
        .collect()
}

/// `05_SEMANTICS/10`, DEMAND: "Required demanded MISSING and UNKNOWN use
/// error.required.missing and error.value.unknown." FAILURE: only "A required
/// post-execution FALSE VERIFY or TEST assertion uses
/// error.verification.failed."
///
/// Regression, PRETEST-01 F04. A required UNKNOWN assertion emitted
/// `error.verification.failed`.
#[test]
fn a_required_unknown_verify_uses_value_unknown_not_verification_failed() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.unknown
    REQUIRED: TRUE
    ASSERT: UNKNOWN AND TRUE
",
        "[REF(verify.unknown)]",
    ));
    assert_eq!(
        emitted(&completion),
        vec!["error.value.unknown"],
        "{}",
        completion.serialize()
    );
    assert!(completion
        .checks()
        .result("verify.unknown")
        .expect("selected")
        .blocks());
    assert!(!completion.succeeded());
}

/// Regression, PRETEST-01 F04. A required MISSING assertion emitted
/// `error.required.missing` and then a second, generic
/// `error.verification.failed` for the same cause.
#[test]
fn a_required_missing_verify_uses_required_missing_only() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.skipped
    REQUIRED: FALSE
    WHEN: FALSE
    ASSERT: TRUE

VERIFY:
    ID: verify.missing
    REQUIRED: TRUE
    ASSERT: REF(verify.skipped)
",
        "[REF(verify.missing)]",
    ));
    assert_eq!(
        emitted(&completion),
        vec!["error.required.missing"],
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

/// Regression, PRETEST-01 F04. A demand fault kept its registered identifier
/// but was followed by a misleading `error.verification.failed`.
#[test]
fn a_required_verify_fault_keeps_only_the_evaluators_diagnostic() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.fault
    REQUIRED: TRUE
    ASSERT: 1 / (REF(output.value) - 7) == 1
",
        "[REF(verify.fault)]",
    ));
    assert_eq!(
        emitted(&completion),
        vec!["error.numeric.division_by_zero"],
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

/// The same outcomes on an optional check block nothing and emit nothing
/// for UNKNOWN: "Optional FALSE checks retain their Boolean domain outcome".
#[test]
fn an_optional_unknown_verify_emits_nothing() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.unknown
    REQUIRED: FALSE
    ASSERT: UNKNOWN AND TRUE
",
        "TRUE",
    ));
    assert!(
        emitted(&completion).is_empty(),
        "{}",
        completion.serialize()
    );
}

/// Regression, PRETEST-01 F05 (same root in check applicability). A faulted
/// `WHEN` skipped the check silently and nobody reported the fault.
#[test]
fn a_faulted_verify_when_reports_the_evaluators_diagnostic() {
    let completion = complete(&task_with(
        "
VERIFY:
    ID: verify.guarded
    WHEN: 1 / (REF(output.value) - 7) == 1
    ASSERT: TRUE
",
        "TRUE",
    ));
    assert_eq!(
        emitted(&completion),
        vec!["error.numeric.division_by_zero"],
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

/// Regression, PRETEST-01 F04 on a `TEST` root, which shares the required-check
/// emission. Its demand fault was discarded and replaced by
/// `error.verification.failed`.
#[test]
fn a_test_root_fault_keeps_only_the_evaluators_diagnostic() {
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

TEST:
    ID: test.fault
    ASSERT: 1 / (REF(input.value) - 7) == 1

EXECUTE:
    REFERENCE: REF(test.fault)
",
    );
    let completion = complete(&source);
    assert_eq!(
        emitted(&completion),
        vec!["error.numeric.division_by_zero"],
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}

/// Regression, FINAL-01 (PRETEST-05 O1). `expression_demand_resolution` makes
/// `error.pattern.mismatch` demand-eligible: "A dynamically supplied value
/// fails a declared GLOB or REGEX value constraint". Completion did not mirror
/// that identifier, so the demand fault was dropped and the required check
/// reported a generic `error.value.unknown` in its place.
#[test]
fn a_required_verify_keeps_a_demanded_pattern_mismatch() {
    let source = task_document(
        "
DEFINE:
    ID: type.coded
    KIND: kind.type
    BASE: OBJECT
    FIELD:
        NAME: code
        TYPE: STRING
        REQUIRED: TRUE
        PATTERN: GLOB(\"a*\")

DEFINE:
    ID: const.fallback
    KIND: kind.constant
    TYPE: OBJECT[REF(type.coded)]
    VALUE:
        code: \"abc\"

INPUT:
    ID: input.coded
    TYPE: OBJECT[REF(type.coded)]
    REQUIRED: FALSE
    DEFAULT: REF(const.fallback)

GOAL:
    ID: goal.coded
    ASSERT: TRUE

ACTION:
    ID: action.coded
    OPERATION: core.inspect
    TARGET: REF(goal.coded)

VERIFY:
    ID: verify.code
    REQUIRED: TRUE
    ASSERT: REF(input.coded).code == \"b\"

SUCCESS:
    ID: success.root
    ALL: [REF(verify.code)]

TASK:
    ID: task.coded
    GOAL: REF(goal.coded)
    INPUT: REF(input.coded)
    ACTION: REF(action.coded)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.coded)
",
    );
    let supplied = lcl_runtime::Value::Object(
        [("code".to_string(), lcl_runtime::Value::Text("b".into()))]
            .into_iter()
            .collect(),
    );
    let fixture = execute_with(
        &source,
        lcl_semantics::Invocation::new().with("input.coded", supplied),
    );
    let completion = Completion::of(
        completion_contracts(),
        &fixture.planned,
        &fixture.checked,
        &fixture.resolved,
        &fixture.execution,
    )
    .expect("the fixture executed, so it completes");
    assert_eq!(
        emitted(&completion),
        vec!["error.pattern.mismatch"],
        "{}",
        completion.serialize()
    );
    assert!(!completion.succeeded());
}
