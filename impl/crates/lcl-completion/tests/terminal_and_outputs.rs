//! Step 13: exactly one terminal status, and the declared outputs.

mod common;

use common::*;
use lcl_completion::{Completion, Publication, Reason};

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

#[test]
fn an_invocation_ends_with_exactly_one_terminal_status() {
    let completion = complete(&task_with("", "    ALL: TRUE"));
    let status = completion.terminal_status();
    assert!(
        completion_contracts().is_terminal(status),
        "{status} must be a registered terminal status"
    );
    assert_eq!(status, "status.succeeded");
}

#[test]
fn a_declared_failure_maps_a_legal_terminal_non_success_status() {
    let completion = complete(&task_with(
        "
FAILURE:
    ID: failure.partial
    WHEN: TRUE
    STATUS: status.partial
",
        "    ALL: TRUE",
    ));
    assert_eq!(completion.terminal_status(), "status.partial");
    assert_eq!(
        completion.terminal().reason,
        Reason::DeclaredFailure("failure.partial".to_string())
    );
    // "status.partial is explicitly non-success."
    assert!(!completion.succeeded());
}

#[test]
fn a_status_alias_resolves_to_its_base_before_the_contracts_are_applied() {
    // `event_model.alias_rule`: "Resolve BASE acyclically to one core
    // identifier in the matching domain before diagnostics, event matching,
    // status transitions, and equality."
    let completion = complete(&task_with(
        "
DEFINE:
    ID: outcome.gave_up
    KIND: kind.status
    BASE: status.stopped
    MEANING: \"The run stopped before completing its declared work.\"

FAILURE:
    ID: failure.aliased
    WHEN: TRUE
    STATUS: outcome.gave_up
",
        "    ALL: TRUE",
    ));
    let failure = completion.verdict().failure.as_ref().expect("selected");
    assert_eq!(failure.written_status, "outcome.gave_up");
    assert_eq!(
        failure.requested_status, "status.stopped",
        "an alias inherits its BASE's identity"
    );
    assert_eq!(completion.terminal_status(), "status.stopped");
}

#[test]
fn an_execution_root_cannot_select_skipped_even_through_an_alias() {
    // "A root FAILURE mapping cannot select status.skipped, even through an
    // alias; that status has non-root scope and the illegal request uses
    // error.execution.order."
    let completion = complete(&task_with(
        "
DEFINE:
    ID: outcome.passed_over
    KIND: kind.status
    BASE: status.skipped
    MEANING: \"Not applicable to this invocation.\"

FAILURE:
    ID: failure.skip
    WHEN: TRUE
    STATUS: outcome.passed_over
",
        "    ALL: TRUE",
    ));
    assert!(
        matches!(completion.terminal().reason, Reason::IllegalRequest { .. }),
        "{}",
        completion.serialize()
    );
    assert!(completion
        .diagnostics()
        .iter()
        .any(|d| d.id.as_registry_str() == "error.execution.order"));
    assert_eq!(completion.terminal_status(), "status.failed");
    assert_ne!(completion.terminal_status(), "status.skipped");
}

#[test]
fn a_primary_execution_diagnostic_is_not_overridden_by_a_true_failure_clause() {
    // The load-bearing rule of this milestone. "An existing primary unhandled
    // diagnostic always fixes status by its resolved default_status ... A
    // FAILURE mapping cannot override that diagnostic."
    let source = task_document(
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
    ID: goal.divide
    ASSERT: REF(output.value) == 0

ACTION:
    ID: action.divide
    OPERATION: core.calculate
    TARGET: REF(input.value)
    PARAMETER:
        NAME: expression
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: \"REF(input.value) / 0\"
    OUTPUT: REF(output.value)

FAILURE:
    ID: failure.partial
    WHEN: TRUE
    STATUS: status.partial

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.divide
    GOAL: REF(goal.divide)
    INPUT: REF(input.value)
    ACTION: REF(action.divide)
    OUTPUT: REF(output.value)
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.divide)
",
    );
    let fixture = execute(&source);
    let primary = fixture
        .execution
        .primary()
        .expect("the division produced an execution diagnostic");
    let expected_status = primary.default_status.clone();
    let primary_id = primary.id.as_registry_str().to_string();

    let completion = Completion::of(
        completion_contracts(),
        &fixture.planned,
        &fixture.checked,
        &fixture.resolved,
        &fixture.execution,
    )
    .expect("completes");

    match &completion.terminal().reason {
        Reason::PrimaryDiagnostic(p) => {
            assert_eq!(p.id, primary_id);
        }
        other => panic!("the primary diagnostic must fix the status, got {other:?}"),
    }
    assert_eq!(
        completion.terminal_status(),
        expected_status,
        "a TRUE FAILURE clause must not override the primary diagnostic"
    );
    assert_ne!(
        completion.terminal_status(),
        "status.partial",
        "the declared mapping requested status.partial and must not win"
    );
}

#[test]
fn a_bound_root_output_is_published() {
    let completion = complete(&task_with("", "    ALL: TRUE"));
    let record = completion
        .outputs()
        .get("output.value")
        .expect("the root declares this output");
    assert!(record.required, "OUTPUT.REQUIRED defaults TRUE");
    assert!(
        matches!(record.publication, Publication::Published(_)),
        "{}",
        completion.serialize()
    );
    assert!(completion.outputs().complete());
}

#[test]
fn a_required_root_output_that_never_bound_prevents_success() {
    // "At a TASK execution root, status.succeeded is legal only when ... all
    // required outputs are fully bound and valid."
    let source = task_document(
        "
INPUT:
    ID: input.value
    TYPE: INTEGER
    VALUE: 7

OUTPUT:
    ID: output.value
    TYPE: INTEGER
    FORMAT: format.plain_text

OUTPUT:
    ID: output.never
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

SUCCESS:
    ID: success.root
    ALL: TRUE

TASK:
    ID: task.copy
    GOAL: REF(goal.copy)
    INPUT: REF(input.value)
    ACTION: REF(action.copy)
    OUTPUT: [REF(output.value), REF(output.never)]
    SUCCESS: REF(success.root)

EXECUTE:
    REFERENCE: REF(task.copy)
",
    );
    let completion = complete(&source);
    let record = completion
        .outputs()
        .get("output.never")
        .expect("the root's export list names it");
    assert_eq!(record.publication, Publication::Unbound);
    assert!(record.blocks());
    assert!(!completion.outputs().complete());
    assert!(
        !completion.succeeded(),
        "an unbound required output prevents success: {}",
        completion.serialize()
    );
    assert!(completion
        .diagnostics()
        .iter()
        .any(|d| d.id.as_registry_str() == "error.success.unsatisfied"));
}

#[test]
fn a_task_output_list_selects_requirements_and_never_reassigns() {
    // "TASK and EXECUTE OUTPUT lists select requirements or exports; they do
    // not produce or reassign values."
    let completion = complete(&task_with("", "    ALL: TRUE"));
    let published = match &completion
        .outputs()
        .get("output.value")
        .expect("declared")
        .publication
    {
        Publication::Published(value) => value.clone(),
        other => panic!("expected a published output, got {other:?}"),
    };
    assert!(
        completion.observation().observed("output.value"),
        "the export names an output the invocation actually bound"
    );
    // The published value is the producer's binding, unchanged by the export.
    assert_eq!(
        published,
        *execute(&task_with("", "    ALL: TRUE"))
            .execution
            .bindings()
            .output("output.value", &lcl_runtime::IterationPath::root())
            .expect("the ACTION bound it")
    );
}

#[test]
fn completion_is_byte_identical_across_identical_runs() {
    let source = task_with(
        "
VERIFY:
    ID: verify.value
    ASSERT: REF(output.value) == 7
",
        "    ALL: [REF(verify.value)]",
    );
    let first = complete(&source).serialize();
    let second = complete(&source).serialize();
    assert_eq!(first, second);
}
