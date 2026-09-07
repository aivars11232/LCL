//! The canonical decision witnesses this milestone owns.
//!
//! `09_CONFORMANCE/CASES/language_decision_cases_v0.1.0.json` records 66
//! witnesses. They are declared `"executed": false` in the package, because the
//! bare-language release had no implementation to execute them. This file
//! executes the ones whose contract is preflight's, so each stops being a
//! description and becomes a test.
//!
//! Witnesses owned by later milestones — retry attempts, handler activation,
//! post-execution VERIFY, terminal status — are deliberately absent. Claiming
//! them here would be a false claim about what has been executed.

mod common;

use common::*;
use lcl_resolver::MemoryProvider;
use lcl_semantics::{EdgeReason, Invocation, Outcome};

fn node_id(planned: &lcl_semantics::Planned, index: usize) -> String {
    planned
        .partial_plan()
        .node(index)
        .and_then(|n| n.id.clone())
        .unwrap_or_default()
}

#[test]
fn closure_053_children_expand_in_field_and_list_order() {
    // "TASK has ACTION field then PHASE field, each with a reference LIST.
    // Children expand in that field order and left-to-right LIST order;
    // sequential predecessor edges govern execution."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nACTION:\n    ID: action.two\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nPHASE:\n    ID: phase.one\n    STEP:\n        ID: step.one\n        ACTION:\n            ID: action.three\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: [REF(action.one), REF(action.two)]\n    PHASE: REF(phase.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));

    let children: Vec<String> = planned
        .partial_plan()
        .node(0)
        .expect("root")
        .children
        .iter()
        .map(|c| node_id(&planned, *c))
        .collect();
    assert_eq!(
        children,
        vec![
            "action.one".to_string(),
            "action.two".to_string(),
            "phase.one".to_string()
        ],
        "field order then left-to-right LIST order"
    );

    let sequential: Vec<(String, String)> = planned
        .partial_plan()
        .edges()
        .iter()
        .filter(|e| e.reason == EdgeReason::Sequential)
        .map(|e| (node_id(&planned, e.from), node_id(&planned, e.to)))
        .collect();
    assert!(
        sequential.contains(&("action.one".to_string(), "action.two".to_string())),
        "sequential predecessor edges govern execution: {sequential:?}"
    );
}

#[test]
fn closure_060_an_imported_check_is_not_activated_by_import_alone() {
    // "An imported document contains an unrelated targetless VERIFY FALSE.
    // Import alone does not activate the check. Explicit check prerequisites
    // can select it."
    //
    // Stated for VERIFY, whose activation is M8's; the same selection sentence
    // governs pre-effect VALIDATE, which is this layer's — "Unrelated imported
    // checks never run merely because their document was imported."
    let library = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: lib.doc\n    NAME: \"Library\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nDATA:\n    ID: data.lib\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nVALIDATE:\n    ID: validate.unrelated\n    ASSERT: FALSE\n";
    let root = format!(
        "{HEADER}\nIMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );

    let provider = MemoryProvider::new().with("lib.lcl", library.as_bytes());
    let resolved = resolver()
        .resolve(&unit("root.lcl", &root), &provider)
        .expect("earlier stages pass");
    assert!(
        resolved.diagnostics().is_empty(),
        "the fixture must resolve: {:?}",
        resolved
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let checked = checker().check(&resolved).expect("resolution succeeded");
    let planned = preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .expect("the static stage succeeded");

    assert!(
        planned
            .partial_plan()
            .checks()
            .iter()
            .all(|c| !c.id.contains("unrelated")),
        "an unrelated imported check must not be selected: {:?}",
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
        "an unactivated FALSE check must not fail the invocation: {:?}",
        ids(&planned)
    );
}

#[test]
fn closure_062_a_root_without_a_success_field_invents_none() {
    // "EXECUTE selects an ACTION with no TASK SUCCESS field. Completion
    // requires its applicable required graph/check/rule/evidence/OUTPUT
    // obligations; no unavailable SUCCESS field is invented."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nEXECUTE:\n    REFERENCE: REF(action.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "an ACTION root with no SUCCESS plans: {:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let plan = planned.plan().expect("planned");
    assert_eq!(plan.len(), 1, "the ACTION root is the whole graph");
    assert_eq!(plan.nodes()[0].id.as_deref(), Some("action.one"));
    assert!(
        plan.checks().is_empty(),
        "no SUCCESS field was invented, so no check was selected from one"
    );
}

#[test]
fn closure_063_an_ordering_failure_before_effects_stays_pre_effect() {
    // "Graph construction yields error.execution.order while the invocation is
    // ready. Transition to status.failed is permitted before effects;
    // failure_phase remains producer-relative pre_effect."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.one\n    STEP:\n        ID: step.one\n        ACTION:\n            ID: action.one\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n    STEP:\n        ID: step.two\n        BEFORE: [REF(step.one)]\n        ACTION:\n            ID: action.two\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    let primary = planned.primary().expect("an ordering failure");
    assert_eq!(primary.id.to_string(), "error.execution.order");
    assert_eq!(primary.failure_phase.to_string(), "pre_effect");
    assert_eq!(
        primary.default_status, "status.failed",
        "the registered default status is unchanged by being decided early"
    );
}

#[test]
fn closure_066_a_check_targeting_an_unselected_branch_producer_is_inapplicable() {
    // "IF selects its first branch; targeted VERIFY names the unselected second
    // branch producer. That targeted VERIFY is inapplicable and has no result;
    // an explicit required read of its result yields MISSING, not TRUE."
    //
    // Branch *selection* is dynamic and M6's. What this layer owns, and what
    // this test pins, is the half that is decidable before effects: a check
    // targeting a declaration outside the candidate graph is not selected, and
    // its absent result is never read as an implicit TRUE.
    let source = task_document(
        "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nDATA:\n    ID: data.other\n    TYPE: STRING\n    VALUE: \"y\"\n\nVALIDATE:\n    ID: validate.unselected\n    TARGET: REF(data.other)\n    ASSERT: FALSE\n",
    );
    let planned = plan(&source);
    assert!(
        planned
            .partial_plan()
            .checks()
            .iter()
            .all(|c| c.id != "validate.unselected"),
        "a check whose target the graph does not reach is not selected"
    );
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "its FALSE assertion is never read as a result, and never as TRUE"
    );
}
