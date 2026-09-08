//! Canonical processing step 10: dynamic reachability and control flow.
//!
//! Authority: `05_SEMANTICS/01`, `05_SEMANTICS/08`, `05_SEMANTICS/09` and
//! `block_schemas_v0.1.0.json#/execution_graph_contract`.

mod common;

use common::{execute, execute_with};
use lcl_runtime::{MockHost, RuntimeError};

/// A `kind.task` document with one `IF` whose arms select different actions.
fn branching_document(condition: &str) -> String {
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.branch
    NAME: "Branch"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.flag
    TYPE: BOOLEAN
    VALUE: {condition}

GOAL:
    ID: goal.branch
    ASSERT: TRUE

ACTION:
    ID: action.taken
    OPERATION: core.inspect
    TARGET: REF(input.flag)

ACTION:
    ID: action.other
    OPERATION: core.read
    TARGET: REF(input.flag)

SEQUENCE:
    ID: sequence.branch
    IF (REF(input.flag)) THEN:
        STEP:
            ID: step.taken
            ACTION: REF(action.taken)
    ELSE:
        STEP:
            ID: step.other
            ACTION: REF(action.other)

SUCCESS:
    ID: success.branch
    ALL: [TRUE]

TASK:
    ID: task.branch
    GOAL: REF(goal.branch)
    INPUT: REF(input.flag)
    SEQUENCE: REF(sequence.branch)
    SUCCESS: REF(success.branch)

EXECUTE:
    REFERENCE: REF(task.branch)
"#
    )
}

/// A `kind.task` document iterating a declared collection.
fn loop_document(collection: &str) -> String {
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.loop
    NAME: "Loop"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.items
    TYPE: LIST[INTEGER]
    VALUE: {collection}

GOAL:
    ID: goal.loop
    ASSERT: TRUE

SEQUENCE:
    ID: sequence.loop
    FOR EACH item IN REF(input.items):
        STEP:
            ID: step.item
            ACTION:
                ID: action.item
                OPERATION: core.inspect
                TARGET: REF(item)

SUCCESS:
    ID: success.loop
    ALL: [TRUE]

TASK:
    ID: task.loop
    GOAL: REF(goal.loop)
    INPUT: REF(input.items)
    SEQUENCE: REF(sequence.loop)
    SUCCESS: REF(success.loop)

EXECUTE:
    REFERENCE: REF(task.loop)
"#
    )
}

// ---------------------------------------------------------------------------
// The canonical corpus
// ---------------------------------------------------------------------------

#[test]
fn every_canonical_valid_example_executes() {
    // The full matrix: each valid example carried from bytes to execution.
    let mut executed = 0;
    for name in common::canonical_example_names() {
        let fixture = common::example_fixture(&name);
        let mut host = MockHost::new();
        let execution = common::run(&fixture, &mut host)
            .unwrap_or_else(|e| panic!("{name} must be executable: {e}"));
        assert!(
            execution.diagnostics().is_empty(),
            "{name} produced {:?}",
            execution
                .diagnostics()
                .iter()
                .map(|d| (d.id.to_string(), d.detail.clone()))
                .collect::<Vec<_>>()
        );
        executed += 1;
    }
    assert_eq!(executed, 13, "the package declares 13 valid examples");
}

#[test]
fn a_document_with_no_execute_root_runs_nothing() {
    // `kind.library` and `kind.data` declare no EXECUTE, so the candidate graph
    // is empty and the runtime performs no work and no effect.
    for name in [
        "02_IMPORT_LIBRARY.lcl",
        "10_ACCEPTED_CORE_PROFILES.lcl",
        "11_EXACT_DIVISION_AND_ROUNDING.lcl",
        "13_TYPES_AND_REFERENCE_VALUES.lcl",
    ] {
        let fixture = common::example_fixture(name);
        let mut host = MockHost::new();
        let execution = common::run(&fixture, &mut host).expect("planned");
        assert!(execution.invocations().is_empty(), "{name}");
        assert!(host.requests().is_empty(), "{name} caused an effect");
    }
}

// ---------------------------------------------------------------------------
// Only the accepted graph executes
// ---------------------------------------------------------------------------

#[test]
fn every_executed_invocation_is_a_node_of_the_accepted_plan() {
    // The acceptance criterion, checked directly: "Runtime never executes a
    // node absent from the accepted candidate graph."
    for name in common::canonical_example_names() {
        let fixture = common::example_fixture(&name);
        let mut host = MockHost::new();
        let execution = common::run(&fixture, &mut host).expect("planned");
        let plan = fixture.planned.plan().expect("accepted");
        for record in execution.invocations() {
            assert!(
                record.node < plan.len(),
                "{name}: executed node {} is outside the plan",
                record.node
            );
            let node = plan.node(record.node).expect("a plan node");
            assert_eq!(record.declaration, node.id, "{name}");
        }
    }
}

#[test]
fn a_rejected_preflight_hands_the_runtime_nothing() {
    // Stage monotonicity, made structural: `Planned::plan` returns `None` for a
    // rejected preflight, so there is no plan to execute and no effect can be
    // authorized.
    let source = common::canonical_example("08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl")
        // Remove the OVERRIDE so the ALLOW/FORBID pair becomes a hard conflict.
        .replace(
            "OVERRIDE:\n    ID: override.endpoint\n    WINNER: REF(permission.download)\n    LOSER: REF(rule.default_no_download)\n\n",
            "",
        );
    let fixture = common::fixture_allowing_rejection(&source, lcl_semantics::Invocation::new());
    assert!(
        !common::is_planned(&fixture),
        "the conflict must be rejected"
    );
    let mut host = MockHost::new();
    let refusal = common::run(&fixture, &mut host).expect_err("a rejected plan is not executable");
    assert!(refusal.primary.is_some());
    assert!(
        host.requests().is_empty(),
        "a rejected plan authorizes no effect"
    );
}

// ---------------------------------------------------------------------------
// Branches
// ---------------------------------------------------------------------------

#[test]
fn a_true_condition_selects_its_then_arm_only() {
    // "IF evaluates once upon reachability" and "Reachability follows ... TRUE
    // branches". The unselected arm is never entered, so its action never runs.
    let (execution, host) = execute(&branching_document("TRUE"));
    assert!(execution.diagnostics().is_empty());
    let operations: Vec<&str> = host
        .requests()
        .iter()
        .map(|r| r.operation.as_str())
        .collect();
    assert_eq!(operations, vec!["core.inspect"]);
    let entered: Vec<&str> = execution
        .invocations()
        .iter()
        .filter_map(|r| r.declaration.as_deref())
        .collect();
    assert!(entered.contains(&"step.taken"));
    assert!(!entered.contains(&"step.other"), "{entered:?}");
}

#[test]
fn a_false_condition_selects_its_else_arm_only() {
    let (execution, host) = execute(&branching_document("FALSE"));
    assert!(execution.diagnostics().is_empty());
    let operations: Vec<&str> = host
        .requests()
        .iter()
        .map(|r| r.operation.as_str())
        .collect();
    assert_eq!(operations, vec!["core.read"]);
    let entered: Vec<&str> = execution
        .invocations()
        .iter()
        .filter_map(|r| r.declaration.as_deref())
        .collect();
    assert!(entered.contains(&"step.other"));
    assert!(!entered.contains(&"step.taken"), "{entered:?}");
}

#[test]
fn the_unselected_arm_causes_no_effect_at_all() {
    // Both arms are in the candidate graph as source templates; only one is
    // reachable, and reachability — not membership — decides what runs.
    let (_, host) = execute(&branching_document("TRUE"));
    assert_eq!(host.count("core.read"), 0);
    assert_eq!(host.count("core.inspect"), 1);
}

// ---------------------------------------------------------------------------
// Loops
// ---------------------------------------------------------------------------

#[test]
fn a_loop_iterates_its_finite_snapshot_in_order() {
    // "FOR EACH evaluates its finite snapshot once at reachability. Iterations
    // run sequentially in registered LIST or SET order."
    let (execution, host) = execute(&loop_document("[10, 20, 30]"));
    assert!(execution.diagnostics().is_empty());
    assert_eq!(host.count("core.inspect"), 3);
    let targets: Vec<String> = host
        .requests()
        .iter()
        .map(|r| r.target.as_ref().expect("a target").to_string())
        .collect();
    assert_eq!(targets, vec!["10", "20", "30"], "LIST uses source order");
}

#[test]
fn each_loop_instance_is_a_distinct_invocation_of_one_declaration() {
    // "Explicit bounded FOR EACH instances ... replicate their source template
    // with distinct invocation identities and are not duplicate activation."
    let (execution, _) = execute(&loop_document("[1, 2]"));
    let instances: Vec<_> = execution
        .invocations()
        .iter()
        .filter(|r| r.declaration.as_deref() == Some("action.item"))
        .collect();
    assert_eq!(instances.len(), 2);
    assert_ne!(instances[0].id, instances[1].id);
    assert_eq!(
        instances[0].node, instances[1].node,
        "one source declaration"
    );
    assert_eq!(instances[0].id.iteration.indexes(), &[0]);
    assert_eq!(instances[1].id.iteration.indexes(), &[1]);
}

#[test]
fn an_empty_snapshot_runs_no_iteration() {
    let (execution, host) = execute(&loop_document("[]"));
    assert!(execution.diagnostics().is_empty());
    assert_eq!(host.count("core.inspect"), 0);
}

#[test]
fn a_loop_body_finishes_before_the_next_iteration_starts() {
    // "each body finishes before the next starts". With one action per body,
    // that is visible as strictly alternating request order across instances.
    let (execution, host) = execute(&loop_document("[1, 2, 3]"));
    assert_eq!(host.requests().len(), 3);
    let iterations: Vec<usize> = host
        .requests()
        .iter()
        .map(|r| r.invocation.iteration.indexes()[0])
        .collect();
    assert_eq!(iterations, vec![0, 1, 2]);
    let _ = execution;
}

// ---------------------------------------------------------------------------
// Applicability
// ---------------------------------------------------------------------------

#[test]
fn closure_029_a_false_applicability_skips_a_non_root_unit() {
    // "A ready non-root optional execution unit has WHEN FALSE => it moves from
    // status.ready to status.skipped without beginning its operation."
    let source = branching_document("TRUE").replace(
        "        STEP:\n            ID: step.taken\n            ACTION: REF(action.taken)",
        "        STEP:\n            ID: step.taken\n            ACTION: REF(action.taken)\n            WHEN: FALSE\n            REQUIRED: FALSE",
    );
    let (execution, host) = execute(&source);
    let skipped = execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some("step.taken"))
        .expect("the step was reached");
    assert_eq!(skipped.status(), "status.skipped");
    assert_eq!(
        host.count("core.inspect"),
        0,
        "a skipped unit never begins its operation"
    );
}

#[test]
fn a_true_applicability_runs_normally() {
    let source = branching_document("TRUE").replace(
        "        STEP:\n            ID: step.taken\n            ACTION: REF(action.taken)",
        "        STEP:\n            ID: step.taken\n            ACTION: REF(action.taken)\n            WHEN: TRUE",
    );
    let (_, host) = execute(&source);
    assert_eq!(host.count("core.inspect"), 1);
}

// ---------------------------------------------------------------------------
// The effect boundary, behaviourally
// ---------------------------------------------------------------------------

#[test]
fn every_effect_crosses_the_host_boundary() {
    // The behavioural half of "All external effects are represented as
    // capability requests": every action that ran appears in the host log, and
    // nothing else does.
    let (execution, host) = execute(&loop_document("[1, 2]"));
    let actions = execution
        .invocations()
        .iter()
        .filter(|r| r.block == "ACTION")
        .count();
    assert_eq!(actions, host.requests().len());
    for request in host.requests() {
        assert!(
            !request.authorization.operation.is_empty(),
            "every request carries the authorization preflight decided"
        );
    }
}

#[test]
fn a_denied_capability_becomes_permission_denied() {
    let host = MockHost::new().deny("core.inspect", "the host forbids it");
    let (execution, _) = execute_with(&loop_document("[1]"), host);
    let primary = execution.primary().expect("a denial is a diagnostic");
    assert_eq!(primary.id, RuntimeError::PermissionDenied);
    assert_eq!(execution.terminal_status(), Some("status.failed"));
}

#[test]
fn an_unavailable_capability_becomes_a_host_constraint() {
    let host = MockHost::new().unavailable("core.inspect", "no such capability");
    let (execution, _) = execute_with(&loop_document("[1]"), host);
    let primary = execution.primary().expect("a limitation is a diagnostic");
    assert_eq!(primary.id, RuntimeError::HostConstraint);
    // Its registered default_status is status.blocked.
    assert_eq!(execution.terminal_status(), Some("status.blocked"));
}

#[test]
fn a_host_constraint_raises_its_registered_event() {
    // "Emission of a diagnostic is the only producer of an event."
    let host = MockHost::new().unavailable("core.inspect", "no such capability");
    let (execution, _) = execute_with(&loop_document("[1]"), host);
    let events: Vec<&str> = execution
        .events()
        .iter()
        .map(|e| e.event.as_str())
        .collect();
    assert_eq!(events, vec!["event.host_constraint"]);
}

#[test]
fn a_successful_run_raises_no_event() {
    // "No timer, host signal, external notification, or successful completion
    // raises an event."
    let (execution, _) = execute(&loop_document("[1, 2]"));
    assert!(execution.events().is_empty());
}

// ---------------------------------------------------------------------------
// Output binding
// ---------------------------------------------------------------------------

#[test]
fn a_producer_binds_its_selected_output_through_the_registered_projection() {
    // 01_MINIMAL_TASK selects `output.value` with `PROPERTY: value`, which is a
    // projectable field of `result.value`.
    let fixture = common::example_fixture("01_MINIMAL_TASK.lcl");
    let mut host = MockHost::new();
    let execution = common::run(&fixture, &mut host).expect("planned");
    let action = execution
        .invocations()
        .iter()
        .find(|r| r.block == "ACTION")
        .expect("the action ran");
    let result = action.result.as_ref().expect("a producer result");
    assert_eq!(
        result.output_binding,
        lcl_runtime::OutputBinding::Bound,
        "{result}"
    );
    assert!(
        execution
            .bindings()
            .output("output.value", &lcl_runtime::IterationPath::root())
            .is_some(),
        "the projection is bound"
    );
}

#[test]
fn closure_058_each_loop_instance_binds_its_own_output() {
    // "An ACTION replicated by FOR EACH has a separate OUTPUT binding per full
    // enclosing iteration-index path."
    let source = loop_document("[1, 2]")
        .replace(
            "GOAL:\n    ID: goal.loop",
            "OUTPUT:\n    ID: output.item\n    TYPE: STRING\n    FORMAT: format.plain_text\n    PROPERTY: value\n\nGOAL:\n    ID: goal.loop",
        )
        .replace(
            "                TARGET: REF(item)",
            "                TARGET: REF(item)\n                OUTPUT: REF(output.item)",
        );
    let (execution, _) = execute(&source);
    assert!(
        execution.diagnostics().is_empty(),
        "{:?}",
        execution.diagnostics()
    );
    let root = lcl_runtime::IterationPath::root();
    assert!(execution
        .bindings()
        .output("output.item", &root.child(0))
        .is_some());
    assert!(execution
        .bindings()
        .output("output.item", &root.child(1))
        .is_some());
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn the_same_program_and_host_produce_identical_observable_output() {
    let run = || {
        let (execution, _) = execute(&loop_document("[1, 2, 3]"));
        execution.serialize()
    };
    assert_eq!(run(), run());
}

#[test]
fn every_canonical_example_executes_identically_on_repeat() {
    for name in common::canonical_example_names() {
        let once = {
            let fixture = common::example_fixture(&name);
            let mut host = MockHost::new();
            common::run(&fixture, &mut host)
                .expect("planned")
                .serialize()
        };
        let twice = {
            let fixture = common::example_fixture(&name);
            let mut host = MockHost::new();
            common::run(&fixture, &mut host)
                .expect("planned")
                .serialize()
        };
        assert_eq!(once, twice, "{name} is not deterministic");
    }
}

#[test]
fn execution_is_bounded() {
    // Evidence that the run terminated inside the engine's step budget rather
    // than by luck.
    let (execution, _) = execute(&loop_document("[1, 2, 3]"));
    assert!(execution.steps() > 0);
    assert!(execution.steps() < 1_000);
}
