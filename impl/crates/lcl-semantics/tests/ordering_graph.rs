//! Step 9: the finalized ordering graph.
//!
//! Authority: `block_schemas_v0.1.0.json#/execution_graph_contract` and
//! `05_SEMANTICS/01`.

mod common;

use common::*;
use lcl_semantics::{EdgeReason, Mode, Outcome};

/// A `kind.task` document whose `TASK` runs one `SEQUENCE`.
fn sequence_document(mode: &str, steps: &str, extra: &str) -> String {
    format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n{extra}\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.one\n    MODE: {mode}\n{steps}\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    )
}

fn step(id: &str, action: &str, target: &str, extra: &str) -> String {
    format!(
        "    STEP:\n        ID: {id}\n        ACTION:\n            ID: {action}\n            OPERATION: core.inspect\n            TARGET: REF({target})\n{extra}"
    )
}

fn node_id(planned: &lcl_semantics::Planned, index: usize) -> String {
    planned
        .partial_plan()
        .node(index)
        .and_then(|n| n.id.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Membership is copied, never extended
// ---------------------------------------------------------------------------

#[test]
fn the_plan_has_exactly_the_candidate_graphs_nodes() {
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}",
            step("step.one", "action.one", "data.subject", ""),
            step("step.two", "action.two", "data.subject", "")
        ),
        "",
    );
    let (resolved, checked) = check(&source);
    let planned = preflight()
        .plan(&checked, &resolved, &lcl_semantics::Invocation::new())
        .expect("static stage succeeded");
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
    assert_eq!(
        planned.partial_plan().len(),
        resolved.graph().len(),
        "preflight adds no graph member and removes none"
    );
    for (index, node) in planned.partial_plan().nodes().iter().enumerate() {
        assert_eq!(node.candidate, index);
        let member = resolved.graph().get(index).expect("same graph");
        assert_eq!(node.kind, member.kind);
        assert_eq!(node.block, member.block);
        assert_eq!(node.span, member.span);
        assert_eq!(node.children, member.children);
    }
}

#[test]
fn no_check_reference_or_value_read_adds_membership_or_edges() {
    // Step 9's own rule, stated as a comparison: the same document with and
    // without a VALIDATE, a SUCCESS reference and an OUTPUT read must produce
    // byte-identical node and edge sets.
    let steps = format!(
        "{}{}",
        step("step.one", "action.one", "data.subject", ""),
        step("step.two", "action.two", "data.subject", "")
    );
    let bare = sequence_document("mode.sequential", &steps, "");
    let referenced = sequence_document(
        "mode.sequential",
        &steps,
        "\nVALIDATE:\n    ID: validate.one\n    TARGET: REF(action.one)\n    ASSERT: TRUE\n",
    );

    let bare = plan(&bare);
    let referenced = plan(&referenced);
    assert_eq!(
        referenced.outcome(),
        Outcome::Planned,
        "{:?}",
        ids(&referenced)
    );

    let nodes = |p: &lcl_semantics::Planned| -> Vec<(String, String)> {
        p.partial_plan()
            .nodes()
            .iter()
            .map(|n| (n.block.clone(), n.id.clone().unwrap_or_default()))
            .collect()
    };
    let edges = |p: &lcl_semantics::Planned| -> Vec<(usize, usize, String)> {
        p.partial_plan()
            .edges()
            .iter()
            .map(|e| (e.from, e.to, e.reason.to_string()))
            .collect()
    };

    assert_eq!(nodes(&bare), nodes(&referenced), "a check added a node");
    assert_eq!(edges(&bare), edges(&referenced), "a check added an edge");
    assert!(
        !referenced.partial_plan().checks().is_empty(),
        "the check really was selected, so the comparison is meaningful"
    );
}

// ---------------------------------------------------------------------------
// Sequential order and mode
// ---------------------------------------------------------------------------

#[test]
fn sequential_lexical_order_contributes_required_predecessor_edges() {
    // Decision witness CLOSURE-053: children expand in field and list order and
    // "sequential predecessor edges govern execution".
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}{}",
            step("step.one", "action.one", "data.subject", ""),
            step("step.two", "action.two", "data.subject", ""),
            step("step.three", "action.three", "data.subject", "")
        ),
        "",
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));

    let sequential: Vec<(String, String)> = planned
        .partial_plan()
        .edges()
        .iter()
        .filter(|e| e.reason == EdgeReason::Sequential)
        .map(|e| (node_id(&planned, e.from), node_id(&planned, e.to)))
        .collect();
    assert_eq!(
        sequential,
        vec![
            ("step.one".to_string(), "step.two".to_string()),
            ("step.two".to_string(), "step.three".to_string()),
        ]
    );
}

#[test]
fn a_parallel_container_omits_implicit_sequential_edges() {
    // "mode.parallel omits implicit sequential edges but retains BEFORE and
    // AFTER edges."
    let source = sequence_document(
        "mode.parallel",
        &format!(
            "{}{}",
            step("step.one", "action.one", "data.subject", ""),
            step("step.two", "action.two", "data.subject", "")
        ),
        "",
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
    assert!(
        planned
            .partial_plan()
            .edges()
            .iter()
            .all(|e| e.reason != EdgeReason::Sequential),
        "a parallel container contributes no implicit sequential edge"
    );
    let sequence = planned
        .partial_plan()
        .nodes()
        .iter()
        .find(|n| n.block == "SEQUENCE")
        .expect("the SEQUENCE is planned");
    assert_eq!(sequence.mode, Mode::Parallel);
}

#[test]
fn mode_defaults_to_sequential() {
    // "PHASE and SEQUENCE use declared MODE, defaulting to mode.sequential."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.one\n{}{}\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n",
        step("step.one", "action.one", "data.subject", ""),
        step("step.two", "action.two", "data.subject", "")
    );
    let planned = plan(&source);
    let sequence = planned
        .partial_plan()
        .nodes()
        .iter()
        .find(|n| n.block == "SEQUENCE")
        .expect("the SEQUENCE is planned");
    assert_eq!(sequence.mode, Mode::Sequential);
    assert!(planned
        .partial_plan()
        .edges()
        .iter()
        .any(|e| e.reason == EdgeReason::Sequential));
}

// ---------------------------------------------------------------------------
// BEFORE and AFTER
// ---------------------------------------------------------------------------

#[test]
fn explicit_ordering_may_not_reverse_a_required_sequential_edge() {
    // Decision witness CLOSURE-055: "A sequential second sibling requests
    // BEFORE the first sibling → error.execution.order; explicit ordering may
    // not reverse the required sequential edge."
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}",
            step("step.one", "action.one", "data.subject", ""),
            step(
                "step.two",
                "action.two",
                "data.subject",
                "        BEFORE: [REF(step.one)]\n"
            )
        ),
        "",
    );
    let planned = plan(&source);
    assert_eq!(
        ids(&planned),
        vec!["error.execution.order".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        planned.primary().map(|d| d.failure_phase.to_string()),
        Some("pre_effect".to_string()),
        "an ordering failure at graph construction stays pre_effect (CLOSURE-063)"
    );
}

#[test]
fn a_before_edge_is_added_in_a_parallel_container() {
    // "mode.parallel omits implicit sequential edges but retains BEFORE and
    // AFTER edges."
    let source = sequence_document(
        "mode.parallel",
        &format!(
            "{}{}",
            step(
                "step.one",
                "action.one",
                "data.subject",
                "        BEFORE: [REF(step.two)]\n"
            ),
            step("step.two", "action.two", "data.subject", "")
        ),
        "",
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
    let before: Vec<(String, String)> = planned
        .partial_plan()
        .edges()
        .iter()
        .filter(|e| e.reason == EdgeReason::Before)
        .map(|e| (node_id(&planned, e.from), node_id(&planned, e.to)))
        .collect();
    assert_eq!(
        before,
        vec![("step.one".to_string(), "step.two".to_string())]
    );
}

#[test]
fn an_ordering_endpoint_must_be_a_distinct_sibling() {
    // "Endpoints must be distinct sibling execution units in the same container
    // and iteration context; other endpoints ... use error.execution.order."
    let source = sequence_document(
        "mode.parallel",
        &format!(
            "{}{}",
            step(
                "step.one",
                "action.one",
                "data.subject",
                "        AFTER: [REF(task.one)]\n"
            ),
            step("step.two", "action.two", "data.subject", "")
        ),
        "",
    );
    let planned = plan(&source);
    assert_eq!(ids(&planned), vec!["error.execution.order".to_string()]);
}

// ---------------------------------------------------------------------------
// Activation identity and output ownership
// ---------------------------------------------------------------------------

#[test]
fn two_activation_paths_to_one_action_use_error_execution_order() {
    // Decision witness CLOSURE-054.
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}",
            step("step.one", "action.one", "data.subject", ""),
            "    STEP:\n        ID: step.two\n        ACTION: REF(action.one)\n"
        ),
        "",
    );
    let planned = plan(&source);
    assert!(
        ids(&planned).contains(&"error.execution.order".to_string()),
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn two_actions_producing_one_output_use_error_execution_order() {
    // Decision witness CLOSURE-056: "Two ACTION declarations select the same
    // OUTPUT, including mutually exclusive branches → error.execution.order
    // before effects."
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}",
            step(
                "step.one",
                "action.one",
                "data.subject",
                "            OUTPUT: REF(output.one)\n"
            ),
            step(
                "step.two",
                "action.two",
                "data.subject",
                "            OUTPUT: REF(output.one)\n"
            )
        ),
        "\nOUTPUT:\n    ID: output.one\n    TYPE: STRING\n    FORMAT: format.plain_text\n",
    );
    let planned = plan(&source);
    assert!(
        ids(&planned).contains(&"error.execution.order".to_string()),
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Parallel independence
// ---------------------------------------------------------------------------

#[test]
fn unordered_parallel_siblings_writing_one_destination_are_not_independent() {
    // "Every included pair must have proven independence: no conflicting
    // writes, read/write dependency, shared mutable binding, or conflicting
    // output destination."
    let source = sequence_document(
        "mode.parallel",
        &format!(
            "{}{}",
            step(
                "step.one",
                "action.one",
                "data.subject",
                "            OUTPUT: REF(output.one)\n"
            ),
            step(
                "step.two",
                "action.two",
                "data.subject",
                "            OUTPUT: REF(output.one)\n"
            )
        ),
        "\nOUTPUT:\n    ID: output.one\n    TYPE: STRING\n    FORMAT: format.plain_text\n",
    );
    let planned = plan(&source);
    assert!(
        ids(&planned).contains(&"error.execution.order".to_string()),
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Order stability
// ---------------------------------------------------------------------------

#[test]
fn the_topological_order_is_stable_and_respects_every_edge() {
    let source = sequence_document(
        "mode.sequential",
        &format!(
            "{}{}{}",
            step("step.one", "action.one", "data.subject", ""),
            step("step.two", "action.two", "data.subject", ""),
            step("step.three", "action.three", "data.subject", "")
        ),
        "",
    );
    let first = plan(&source);
    let second = plan(&source);
    assert_eq!(first.partial_plan().order(), second.partial_plan().order());

    let order = first.partial_plan().order();
    let position = |node: usize| order.iter().position(|n| *n == node).expect("in order");
    for edge in first.partial_plan().edges() {
        assert!(
            position(edge.from) < position(edge.to),
            "every edge must be respected by the order"
        );
    }
    assert_eq!(
        order.len(),
        first.partial_plan().len(),
        "every node is placed exactly once"
    );
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

#[test]
fn a_planned_action_records_its_exact_authorized_shape() {
    let source = sequence_document(
        "mode.sequential",
        &step("step.one", "action.one", "data.subject", ""),
        "",
    );
    let planned = plan(&source);
    let action = planned
        .partial_plan()
        .nodes()
        .iter()
        .find(|n| n.block == "ACTION")
        .expect("the ACTION is planned");
    let authorization = action
        .authorization
        .as_ref()
        .expect("a planned ACTION carries its authorization");
    assert_eq!(authorization.operation, "core.inspect");
    assert_eq!(authorization.target.as_deref(), Some("data.subject"));
}

#[test]
fn an_action_a_winning_forbid_prohibits_is_permission_denied() {
    // "FORBID blocks matching action even when an ACTION requires it unless a
    // valid OVERRIDE resolves the exact conflict." A FORBID of strictly higher
    // authority wins outright, leaving the action unauthorized.
    let source = sequence_document(
        "mode.sequential",
        &step("step.one", "action.one", "data.subject", ""),
        "\nFORBID:\n    ID: rule.no_inspect\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n    AUTHORITY: 900\n",
    );
    let planned = plan(&source);
    assert_eq!(
        ids(&planned),
        vec!["error.permission.denied".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert!(planned.plan().is_none(), "a denied action plans nothing");
}

#[test]
fn an_overridden_forbid_leaves_the_action_authorized() {
    let source = sequence_document(
        "mode.sequential",
        &step("step.one", "action.one", "data.subject", ""),
        "\nALLOW:\n    ID: permission.inspect\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nFORBID:\n    ID: rule.no_inspect\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nOVERRIDE:\n    ID: override.inspect\n    WINNER: REF(permission.inspect)\n    LOSER: REF(rule.no_inspect)\n",
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
    let action = planned
        .partial_plan()
        .nodes()
        .iter()
        .find(|n| n.block == "ACTION")
        .expect("planned");
    let authorization = action.authorization.as_ref().expect("authorized");
    assert_eq!(
        authorization.overridden,
        vec!["rule.no_inspect".to_string()]
    );
    assert!(authorization
        .permitted_by
        .contains(&"permission.inspect".to_string()));
}

/// A prohibited graph-target reference cycle is refused before axis resolution.
///
/// `05_SEMANTICS/11`: "For a referenced TASK, PHASE, SEQUENCE, ACTION, or TEST,
/// a prohibited reference cycle emits error.reference.cycle and fails before
/// axis resolution", and `06_STANDARD_LIBRARY/10` says the same of
/// `core.retry`'s wrapped ACTION.
#[test]
fn a_delegating_target_that_leads_back_to_itself_is_a_reference_cycle() {
    let delegating = |operation: &str, extra: &str, target: &str| {
        format!(
            "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n\
             \nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\
             \nACTION:\n    ID: action.one\n    OPERATION: {operation}\n    TARGET: REF({target}){extra}\n\
             \nACTION:\n    ID: action.two\n    OPERATION: {operation}\n    TARGET: REF(action.one){extra}\n\
             \nACTION:\n    ID: action.three\n    OPERATION: core.return\n    TARGET: REF(data.subject)\n\
             \nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\
             \nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: [REF(action.one), REF(action.two)]\n    SUCCESS: REF(success.one)\n\
             \nEXECUTE:\n    REFERENCE: REF(task.one)\n"
        )
    };
    const COMPARISON: &str = "\n    PARAMETER:\n        NAME: expected\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 3\n    PARAMETER:\n        NAME: actual\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: REF(data.subject)";
    const LIMIT: &str = "\n    PARAMETER:\n        NAME: limit\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 1";

    for (operation, extra) in [
        ("core.execute", ""),
        ("core.test", COMPARISON),
        ("core.retry", LIMIT),
    ] {
        // action.one -> action.two -> action.one
        let planned = plan(&delegating(operation, extra, "action.two"));
        assert!(
            ids(&planned).contains(&"error.reference.cycle".to_string()),
            "{operation} cycle: {:?}",
            ids(&planned)
        );
        // The control: the same shape, delegating to a unit that leads
        // nowhere back.
        let planned = plan(&delegating(operation, extra, "action.three"));
        assert!(
            !ids(&planned).contains(&"error.reference.cycle".to_string()),
            "{operation} without a cycle must not be refused: {:?}",
            ids(&planned)
        );
    }
}
