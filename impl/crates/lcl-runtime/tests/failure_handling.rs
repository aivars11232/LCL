//! Events, handler selection, `RETRY`, `FALLBACK` and continuation.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/event_model`,
//! `#/failure_lifecycle/retry_safety`, `05_SEMANTICS/06`, `05_SEMANTICS/08` and
//! `06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt`.
//!
//! ## Why every fixture here fails with a host constraint
//!
//! The event mapping "is a bijection between the handler-recoverable errors and
//! the closed canonical core event vocabulary", and only five registered errors
//! are recoverable. `error.execution.action` is **not** one of them: its
//! registered `event` is null, so a plainly failed action raises no event and
//! can select no handler.
//!
//! `error.host.constraint` is recoverable and maps to `event.host_constraint` —
//! which is exactly the event the canonical
//! `08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl` example keys its retry handler
//! on. So a fixture that wants a handler to run makes the host report a
//! limitation, not an ordinary failure.

mod common;

use common::execute_with;
use lcl_runtime::{CapabilityOutcome, MockHost, Observation, RuntimeError};

/// A `kind.task` document whose one action declares a `RETRY` and a handler.
fn retry_document(limit: u32, when: Option<&str>, handler_operation: &str) -> String {
    let when_line = when
        .map(|w| format!("        WHEN: {w}\n"))
        .unwrap_or_default();
    // `core.cancel` registers a required `reason` named parameter, so an
    // invocation site that omits it is `error.operation.parameter` at M4.
    let parameters = if handler_operation == "core.cancel" {
        "    PARAMETER:\n        NAME: reason\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"the handler cancelled it\"\n"
    } else {
        ""
    };
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.retry
    NAME: "Retry"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.endpoint
    TYPE: URI
    VALUE: URI("https://example.invalid/data")

GOAL:
    ID: goal.retry
    ASSERT: TRUE

HANDLER:
    ID: handler.retry
    EVENT: event.host_constraint
    OPERATION: {handler_operation}
{parameters}    LIMIT: {limit}

ACTION:
    ID: action.fetch
    OPERATION: core.inspect
    TARGET: REF(input.endpoint)
    RETRY:
        LIMIT: {limit}
{when_line}        HANDLER: REF(handler.retry)

SUCCESS:
    ID: success.retry
    ALL: [TRUE]

TASK:
    ID: task.retry
    GOAL: REF(goal.retry)
    INPUT: REF(input.endpoint)
    ACTION: REF(action.fetch)
    HANDLER: REF(handler.retry)
    SUCCESS: REF(success.retry)

EXECUTE:
    REFERENCE: REF(task.retry)
"#
    )
}

/// The same retry document, over a row that admits a filesystem effect.
///
/// `core.inspect` is `read_only`, and "read_only requires possible effects
/// exactly {none}", so a host that reported a filesystem effect for it would be
/// reporting one the invocation never resolved — which the boundary now
/// refuses. A test whose subject is a failure *after known effects* therefore
/// needs a row whose own contract admits the effect it is about to observe.
fn writing_retry_document(limit: u32) -> String {
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.retry_write
    NAME: "Retry over a write"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.file
    TYPE: PATH
    VALUE: PATH("/srv/data/report.txt")

GOAL:
    ID: goal.retry
    ASSERT: TRUE

HANDLER:
    ID: handler.retry
    EVENT: event.host_constraint
    OPERATION: core.retry
    LIMIT: {limit}

ACTION:
    ID: action.fetch
    OPERATION: core.write
    TARGET: REF(input.file)
    PARAMETER:
        NAME: content
        TYPE: STRING
        REQUIRED: TRUE
        VALUE: "the written content"
    RETRY:
        LIMIT: {limit}
        HANDLER: REF(handler.retry)

SUCCESS:
    ID: success.retry
    ALL: [TRUE]

TASK:
    ID: task.retry
    GOAL: REF(goal.retry)
    INPUT: REF(input.file)
    ACTION: REF(action.fetch)
    HANDLER: REF(handler.retry)
    SUCCESS: REF(success.retry)

EXECUTE:
    REFERENCE: REF(task.retry)
"#
    )
}

fn unavailable(reason: &str) -> CapabilityOutcome {
    CapabilityOutcome::Unavailable(reason.to_string())
}

fn completed() -> CapabilityOutcome {
    CapabilityOutcome::Completed(Observation::none())
}

// ---------------------------------------------------------------------------
// The event model
// ---------------------------------------------------------------------------

#[test]
fn a_plainly_failed_action_raises_no_event() {
    // `error.execution.action` has a null registered event, so it "raises no
    // event and cannot select a declared recovery handler".
    let host = MockHost::new().script(
        "core.inspect",
        vec![MockHost::failed_before_effect("it just failed")],
    );
    let (execution, host) = execute_with(&retry_document(2, None, "core.retry"), host);
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::ExecutionAction)
    );
    assert!(
        execution.events().is_empty(),
        "a null mapping raises nothing"
    );
    assert_eq!(
        host.count("core.inspect"),
        1,
        "no handler could authorize a retry"
    );
}

#[test]
fn a_host_constraint_raises_exactly_one_event_occurrence() {
    let host = MockHost::new().script("core.inspect", vec![unavailable("no capability")]);
    let (execution, _) = execute_with(&retry_document(0, None, "core.retry"), host);
    let events: Vec<&str> = execution
        .events()
        .iter()
        .map(|e| e.event.as_str())
        .collect();
    assert_eq!(events, vec!["event.host_constraint"]);
}

// ---------------------------------------------------------------------------
// RETRY
// ---------------------------------------------------------------------------

#[test]
fn closure_022_a_successful_first_retry_makes_two_attempts() {
    // "ACTION RETRY LIMIT 2; initial failure then successful first retry =>
    // Two actual attempts, no third attempt, no exhaustion."
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, host) = execute_with(&retry_document(2, None, "core.retry"), host);
    assert_eq!(host.count("core.inspect"), 2, "two actual attempts");
    assert!(
        !execution
            .diagnostics()
            .iter()
            .any(|d| d.id == RuntimeError::RetryExhausted),
        "no exhaustion"
    );
    let attempts: Vec<usize> = execution
        .invocations()
        .iter()
        .filter(|r| r.declaration.as_deref() == Some("action.fetch"))
        .map(|r| r.id.attempt)
        .collect();
    assert_eq!(attempts, vec![0, 1]);
}

#[test]
fn closure_023_a_false_retry_condition_makes_one_attempt_without_exhaustion() {
    // "ACTION RETRY LIMIT 2 WHEN FALSE after first failure => One attempt;
    // previous failure remains unhandled; no success or count exhaustion."
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, host) = execute_with(&retry_document(2, Some("FALSE"), "core.retry"), host);
    assert_eq!(host.count("core.inspect"), 1, "one attempt");
    assert!(
        !execution
            .diagnostics()
            .iter()
            .any(|d| d.id == RuntimeError::RetryExhausted),
        "a FALSE WHEN is not exhaustion"
    );
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::HostConstraint)
    );
}

#[test]
fn closure_024_exhaustion_requires_exactly_one_plus_limit_failed_attempts() {
    // "ACTION RETRY LIMIT 2; all three attempts fail => error.retry.exhausted
    // after exactly three actual unsuccessful attempts."
    let host = MockHost::new().script(
        "core.inspect",
        vec![
            unavailable("1"),
            unavailable("2"),
            unavailable("3"),
            completed(),
        ],
    );
    let (execution, host) = execute_with(&retry_document(2, None, "core.retry"), host);
    assert_eq!(host.count("core.inspect"), 3, "exactly 1 + LIMIT attempts");
    assert!(
        execution
            .diagnostics()
            .iter()
            .any(|d| d.id == RuntimeError::RetryExhausted),
        "three failed attempts is exhaustion: {:?}",
        execution
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_zero_limit_permits_no_additional_attempt() {
    // "Total attempts are at most 1 + LIMIT."
    let host = MockHost::new().script("core.inspect", vec![unavailable("1"), completed()]);
    let (execution, host) = execute_with(&retry_document(0, None, "core.retry"), host);
    assert_eq!(host.count("core.inspect"), 1);
    assert!(execution
        .diagnostics()
        .iter()
        .any(|d| d.id == RuntimeError::RetryExhausted));
}

#[test]
fn an_ordinary_failure_after_known_effects_authorizes_no_retry() {
    // A failure with known effects is `error.execution.action`, which raises no
    // event, so nothing can authorize another attempt over an applied effect.
    let host = MockHost::new().script(
        "core.write",
        vec![
            CapabilityOutcome::Failed {
                detail: "failed after writing".to_string(),
                observation: Observation::none().with_effect(lcl_runtime::ObservedEffect {
                    class: lcl_runtime::EffectClass::Filesystem,
                    state: lcl_runtime::RecordState::Applied,
                    target: None,
                    evidence: Vec::new(),
                }),
            },
            completed(),
        ],
    );
    let (execution, host) = execute_with(&writing_retry_document(2), host);
    assert_eq!(host.count("core.write"), 1);
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::ExecutionAction)
    );
    // The producer result is post-effect and consistent.
    let action = execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some("action.fetch"))
        .expect("the action ran");
    let result = action.result.as_ref().expect("a result");
    assert_eq!(result.failure_phase, lcl_runtime::FailurePhase::PostEffect);
    assert!(result.violations().is_empty(), "{:?}", result.violations());
}

#[test]
fn a_declared_delay_is_handed_to_the_host_before_the_next_attempt() {
    // "apply DELAY before beginning the next attempt." The runtime has no
    // clock, so the declared duration crosses the boundary instead.
    let source = retry_document(2, None, "core.retry").replace(
        "        HANDLER: REF(handler.retry)",
        "        DELAY: DURATION(1, unit.second)\n        HANDLER: REF(handler.retry)",
    );
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (_, host) = execute_with(&source, host);
    assert_eq!(
        host.delays().len(),
        1,
        "one delay for one additional attempt"
    );
}

#[test]
fn closure_059_each_attempt_starts_with_its_output_unbound() {
    // "Each retry attempt starts with its own selected OUTPUT binding unbound.
    // Prior attempt bindings remain ordered local evidence and are not reused
    // as the next attempt output."
    let source = retry_document(2, None, "core.retry")
        .replace(
            "GOAL:\n    ID: goal.retry",
            "OUTPUT:\n    ID: output.payload\n    TYPE: STRING\n    FORMAT: format.plain_text\n    PROPERTY: value\n\nGOAL:\n    ID: goal.retry",
        )
        .replace(
            "    TARGET: REF(input.endpoint)\n    RETRY:",
            "    TARGET: REF(input.endpoint)\n    OUTPUT: REF(output.payload)\n    RETRY:",
        );
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, _) = execute_with(&source, host);
    let attempts: Vec<_> = execution
        .invocations()
        .iter()
        .filter(|r| r.declaration.as_deref() == Some("action.fetch"))
        .collect();
    assert_eq!(attempts.len(), 2);
    let first = attempts[0].result.as_ref().expect("a result");
    assert_eq!(first.output_binding, lcl_runtime::OutputBinding::Unbound);
    // "A pre-effect failure requires ... no bound or partial OUTPUT."
    assert!(first.violations().is_empty(), "{:?}", first.violations());
}

// ---------------------------------------------------------------------------
// Handler selection
// ---------------------------------------------------------------------------

#[test]
fn a_handler_whose_event_does_not_match_is_not_selected() {
    let source = retry_document(2, None, "core.retry")
        .replace("EVENT: event.host_constraint", "EVENT: event.missing");
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, host) = execute_with(&source, host);
    assert_eq!(host.count("core.inspect"), 1, "no handler matched");
    assert_eq!(execution.events().len(), 1, "the event was still raised");
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::HostConstraint)
    );
}

#[test]
fn a_false_handler_condition_does_not_match() {
    // "its WHEN is absent or evaluates TRUE".
    let source = retry_document(2, None, "core.retry").replace(
        "    OPERATION: core.retry\n    LIMIT: 2",
        "    OPERATION: core.retry\n    WHEN: FALSE\n    LIMIT: 2",
    );
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (_, host) = execute_with(&source, host);
    assert_eq!(host.count("core.inspect"), 1);
}

#[test]
fn a_declared_but_unattached_handler_is_never_selected() {
    // "A declared but unattached HANDLER is never selected by an event
    // attachment."
    let source = retry_document(2, None, "core.retry")
        .replace("        HANDLER: REF(handler.retry)\n", "")
        .replace("    HANDLER: REF(handler.retry)\n", "");
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, host) = execute_with(&source, host);
    assert_eq!(host.count("core.inspect"), 1, "no attachment, no selection");
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::HostConstraint)
    );
}

#[test]
fn a_recovering_handler_removes_the_originating_diagnostic_from_primary() {
    // "The originating diagnostic is recovered exactly when that handler
    // invocation's own result ... records status.succeeded." A recovered
    // diagnostic is not primary, but "Recovery never erases the original
    // diagnostic or its evidence".
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient"), completed()]);
    let (execution, _) = execute_with(&retry_document(2, None, "core.retry"), host);
    assert!(
        !execution.diagnostics().is_empty(),
        "the original diagnostic is retained as evidence"
    );
    assert_eq!(
        execution.primary(),
        None,
        "a recovered diagnostic is not primary"
    );
    assert_eq!(execution.terminal_status(), None);
}

#[test]
fn every_raised_event_is_disposed_of() {
    // `multiplicity_rule`: "At most one event-selected handler activation
    // occurs for one raised diagnostic."
    let host = MockHost::new().script(
        "core.inspect",
        vec![unavailable("1"), unavailable("2"), unavailable("3")],
    );
    let (execution, _) = execute_with(&retry_document(2, None, "core.retry"), host);
    for record in execution.events() {
        assert!(
            !matches!(record.disposition, lcl_runtime::Disposition::Pending),
            "every raised event is disposed of"
        );
    }
}

// ---------------------------------------------------------------------------
// Non-reentrancy
// ---------------------------------------------------------------------------

#[test]
fn a_diagnostic_raised_inside_a_handler_raises_no_event() {
    // `non_reentrancy_rule`: diagnostics raised "while executing a handler
    // invocation or its FALLBACK for the same originating diagnostic ... raise
    // no event." Here the handler's own operation is unavailable too, which
    // would otherwise raise a second `event.host_constraint` and recurse.
    let source = retry_document(1, None, "core.stop");
    let host = MockHost::new()
        .script("core.inspect", vec![unavailable("primary")])
        .unavailable("core.stop", "the handler's operation is unavailable too");
    let (execution, _) = execute_with(&source, host);
    assert_eq!(
        execution.events().len(),
        1,
        "only the originating event was raised"
    );
}

// ---------------------------------------------------------------------------
// Control operations
// ---------------------------------------------------------------------------

#[test]
fn a_stop_handler_takes_the_invocation_to_status_stopped() {
    // "core.stop yields status.stopped unless another declared failure status
    // applies."
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient")]);
    let (execution, _) = execute_with(&retry_document(1, None, "core.stop"), host);
    let action = execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some("action.fetch"))
        .expect("the action ran");
    assert_eq!(action.status(), "status.stopped");
}

#[test]
fn a_cancel_handler_takes_the_invocation_to_status_cancelled() {
    // "Cancellation yields status.cancelled."
    let host = MockHost::new().script("core.inspect", vec![unavailable("transient")]);
    let (execution, _) = execute_with(&retry_document(1, None, "core.cancel"), host);
    let action = execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some("action.fetch"))
        .expect("the action ran");
    assert_eq!(action.status(), "status.cancelled");
}

// ---------------------------------------------------------------------------
// Determinism under failure
// ---------------------------------------------------------------------------

#[test]
fn a_failing_run_is_deterministic() {
    let run = || {
        let host = MockHost::new().script(
            "core.inspect",
            vec![unavailable("1"), unavailable("2"), unavailable("3")],
        );
        let (execution, _) = execute_with(&retry_document(2, None, "core.retry"), host);
        execution.serialize()
    };
    assert_eq!(run(), run());
}

// ---------------------------------------------------------------------------
// Continuation and FALLBACK
// ---------------------------------------------------------------------------

/// Two sibling steps in one sequential group, the first of which fails.
fn two_step_document(handler_operation: &str, fallback: Option<&str>) -> String {
    let fallback_line = fallback
        .map(|f| format!("    FALLBACK: {f}\n"))
        .unwrap_or_default();
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.continue
    NAME: "Continue"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.endpoint
    TYPE: URI
    VALUE: URI("https://example.invalid/data")

GOAL:
    ID: goal.continue
    ASSERT: TRUE

HANDLER:
    ID: handler.advance
    EVENT: event.host_constraint
    OPERATION: {handler_operation}
{fallback_line}
ACTION:
    ID: action.first
    OPERATION: core.inspect
    TARGET: REF(input.endpoint)

ACTION:
    ID: action.second
    OPERATION: core.read
    TARGET: REF(input.endpoint)

SEQUENCE:
    ID: sequence.continue
    STEP:
        ID: step.first
        ACTION: REF(action.first)
        HANDLER: REF(handler.advance)
    STEP:
        ID: step.second
        ACTION: REF(action.second)

SUCCESS:
    ID: success.continue
    ALL: [TRUE]

TASK:
    ID: task.continue
    GOAL: REF(goal.continue)
    INPUT: REF(input.endpoint)
    SEQUENCE: REF(sequence.continue)
    SUCCESS: REF(success.continue)

EXECUTE:
    REFERENCE: REF(task.continue)
"#
    )
}

#[test]
fn an_unrecovered_required_failure_stops_the_sequential_group() {
    // "During execution, failure of a required action invokes applicable
    // handler/retry; otherwise status.failed." Without recovery, what follows
    // is not reached — which is what makes `core.continue` mean something.
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    // The handler names an event that does not match, so nothing recovers.
    let source = two_step_document("core.continue", None)
        .replace("EVENT: event.host_constraint", "EVENT: event.missing");
    let (execution, host) = execute_with(&source, host);
    assert_eq!(host.count("core.read"), 0, "the successor was not reached");
    assert_eq!(
        execution.primary().map(|d| d.id),
        Some(RuntimeError::HostConstraint)
    );
}

#[test]
fn closure_027_a_successful_continue_recovers_and_advances_together() {
    // "Selected event handler invokes core.continue and completes successfully
    // => Recover the originating diagnostic and advance to the declared
    // successor together."
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    let (execution, host) = execute_with(&two_step_document("core.continue", None), host);
    // Recovered: the originating diagnostic is no longer primary.
    assert_eq!(execution.primary(), None, "the diagnostic was recovered");
    // Advanced: the declared successor ran.
    assert_eq!(host.count("core.read"), 1, "the successor was reached");
    // "Recovery never erases the original diagnostic or its evidence."
    assert!(!execution.diagnostics().is_empty());
}

#[test]
fn a_failed_handler_does_not_advance() {
    // "A failed handler does not advance." `core.continue` here cannot resolve
    // a successor, because the failing unit is the last of its group.
    let source = two_step_document("core.continue", None).replace(
        "    STEP:\n        ID: step.second\n        ACTION: REF(action.second)\n",
        "",
    );
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    let (execution, host) = execute_with(&source, host);
    assert_eq!(host.count("core.read"), 0);
    // "an absent successor or ambiguous parallel continuation uses
    // error.execution.order"
    assert!(
        execution
            .diagnostics()
            .iter()
            .any(|d| d.id == RuntimeError::ExecutionOrder),
        "{:?}",
        execution
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_successful_fallback_substitutes_for_a_failed_primary() {
    // "A successful fallback substitutes its success for the failed primary in
    // the selected handler's result and recovers the original diagnostic."
    //
    // The primary is `core.continue` on the *last* step of its group, which
    // "creates no successor" and therefore cannot proceed; the FALLBACK is
    // `core.stop`, whose required target the handler-context binding supplies.
    let source = last_step_document("core.continue", Some("core.stop"));
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    let (execution, _) = execute_with(&source, host);
    assert_eq!(
        execution.primary(),
        None,
        "the fallback's success recovers the originating diagnostic"
    );
    let action = execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some("action.only"))
        .expect("the action ran");
    assert_eq!(
        action.status(),
        "status.stopped",
        "the substituted fallback's control operation took effect"
    );
}

#[test]
fn without_a_successful_substitution_the_diagnostic_stays_unhandled() {
    // "Without successful substitution the original diagnostic stays
    // unhandled." Both the primary and the fallback are `core.continue` on a
    // last step, so neither can proceed.
    let source = last_step_document("core.continue", Some("core.continue"));
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    let (execution, _) = execute_with(&source, host);
    // The originating diagnostic is still there and still unhandled. It is not
    // necessarily *primary*: the handler's own failure is an independent
    // diagnostic at the same locus, and `stable_order`'s last key is "error
    // identifier by Unicode scalar value ascending", which puts
    // error.execution.order ahead of error.host.constraint.
    assert!(execution
        .diagnostics()
        .iter()
        .any(|d| d.id == RuntimeError::HostConstraint));
    assert!(execution.primary().is_some(), "nothing was recovered");
}

#[test]
fn a_handler_with_no_fallback_leaves_its_failure_unhandled() {
    let source = last_step_document("core.continue", None);
    let host = MockHost::new().script("core.inspect", vec![unavailable("down")]);
    let (execution, _) = execute_with(&source, host);
    assert!(execution.primary().is_some(), "nothing was recovered");
    // Both the originating diagnostic and the handler's own failure are
    // retained: "Recovery and outcome substitution never erase diagnostics or
    // evidence."
    assert!(execution
        .diagnostics()
        .iter()
        .any(|d| d.id == RuntimeError::HostConstraint));
    assert!(execution
        .diagnostics()
        .iter()
        .any(|d| d.id == RuntimeError::ExecutionOrder));
}

/// One step that is the last of its group, so `core.continue` cannot proceed.
fn last_step_document(handler_operation: &str, fallback: Option<&str>) -> String {
    let fallback_line = fallback
        .map(|f| format!("    FALLBACK: {f}\n"))
        .unwrap_or_default();
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.fallback
    NAME: "Fallback"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.endpoint
    TYPE: URI
    VALUE: URI("https://example.invalid/data")

GOAL:
    ID: goal.fallback
    ASSERT: TRUE

HANDLER:
    ID: handler.only
    EVENT: event.host_constraint
    OPERATION: {handler_operation}
{fallback_line}
ACTION:
    ID: action.only
    OPERATION: core.inspect
    TARGET: REF(input.endpoint)

SEQUENCE:
    ID: sequence.fallback
    STEP:
        ID: step.only
        ACTION: REF(action.only)
        HANDLER: REF(handler.only)

SUCCESS:
    ID: success.fallback
    ALL: [TRUE]

TASK:
    ID: task.fallback
    GOAL: REF(goal.fallback)
    INPUT: REF(input.endpoint)
    SEQUENCE: REF(sequence.fallback)
    SUCCESS: REF(success.fallback)

EXECUTE:
    REFERENCE: REF(task.fallback)
"#
    )
}
