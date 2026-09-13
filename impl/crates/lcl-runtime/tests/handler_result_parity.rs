//! HANDLER-01: a handler's result is a result, and obeys its result schema.
//!
//! A handler runs because something failed, and what it produces decides
//! whether the failure was recovered. That makes it the one place where a
//! malformed "success" is most expensive: it is believed precisely when the
//! engine is deciding whether to keep or discard a registered diagnostic.
//!
//! `05_SEMANTICS/05` is explicit that "A producer status.succeeded means its
//! invocation contract completed", and the result contract closes the field set
//! each schema admits. The ordinary dispatch path checks that closed set before
//! anything is bound. This asks whether the handler path does, using a host
//! that claims a completed operation while returning an observation its
//! registered schema does not admit.
//!
//! These are separate from `failure_handling.rs`, which covers which handler is
//! selected and when. This covers what the selected handler is allowed to
//! return.

mod common;

use common::execute_with;
use lcl_runtime::{CapabilityOutcome, Host, MockHost, Observation, Value};

/// What a fixture host claims its handler operation produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// A completed `core.read` with no fields at all. `result.value` requires
    /// `value` for a success.
    MissingRequiredField,
    /// A completed `core.read` carrying a field the schema does not admit.
    ForbiddenField,
    /// A completed `core.read` whose `evidence` is a STRING where the schema
    /// registers `LIST[REFERENCE[EVIDENCE]]`.
    WrongFieldType,
    /// A well-formed success, so the fixture proves it is not refusing
    /// everything.
    WellFormed,
}

/// A host that fails the first invocation with a recoverable limitation, so a
/// handler is selected, and then answers the handler's own operation with
/// whatever this fixture claims.
struct ClaimingHost {
    claim: Claim,
    invocations: Vec<String>,
}

impl ClaimingHost {
    fn new(claim: Claim) -> ClaimingHost {
        ClaimingHost {
            claim,
            invocations: Vec::new(),
        }
    }
}

impl Host for ClaimingHost {
    fn permits(&mut self, _request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        lcl_runtime::Permission::Granted
    }

    fn invoke(&mut self, request: &lcl_runtime::CapabilityRequest) -> CapabilityOutcome {
        self.invocations.push(request.operation.clone());
        // The action always reports a limitation. `error.host.constraint` is
        // recoverable and raises the event this fixture's handler keys on, and
        // keeping the action failing throughout means the only well-formed
        // result in the execution is the handler's own — which is what these
        // cases are about.
        if request.operation != "core.read" {
            return CapabilityOutcome::Unavailable("the fixture host declines".to_string());
        }
        match self.claim {
            Claim::MissingRequiredField => CapabilityOutcome::Completed(Observation::none()),
            Claim::ForbiddenField => CapabilityOutcome::Completed(
                Observation::none()
                    .with("value", Value::Text("recovered".to_string()))
                    .with("evidence", Value::List(Vec::new()))
                    .with("exit_code", Value::Text("not in result.value".to_string())),
            ),
            Claim::WrongFieldType => CapabilityOutcome::Completed(
                Observation::none()
                    .with("value", Value::Text("recovered".to_string()))
                    .with(
                        "evidence",
                        Value::Text("not a list of evidence".to_string()),
                    ),
            ),
            // `result.value` admits `value` zero-or-one and requires
            // `evidence` exactly once — an empty list is legal, and
            // "status.succeeded requires value exactly once".
            Claim::WellFormed => CapabilityOutcome::Completed(
                Observation::none()
                    .with("value", Value::Text("recovered".to_string()))
                    .with("evidence", Value::List(Vec::new())),
            ),
        }
    }

    fn delay(&mut self, _duration: &Value) {}
}

/// One document whose action fails recoverably and whose handler reads a file.
///
/// `core.read` produces `result.value`, whose success requires a `value` field
/// and whose field set is closed — so a claimed success without it, or with a
/// field it forbids, is malformed in a way the registry can state exactly.
fn handled_document() -> String {
    r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.handled
    NAME: "Handled failure"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.endpoint
    TYPE: URI
    VALUE: URI("https://example.invalid/data")

DATA:
    ID: data.recovery
    TYPE: PATH
    VALUE: PATH("/srv/data/recovery.txt")

GOAL:
    ID: goal.handled
    ASSERT: TRUE

HANDLER:
    ID: handler.read
    EVENT: event.host_constraint
    OPERATION: core.read
    TARGET: REF(data.recovery)
    LIMIT: 1

ACTION:
    ID: action.fetch
    OPERATION: core.inspect
    TARGET: REF(input.endpoint)
    RETRY:
        LIMIT: 2
        HANDLER: REF(handler.read)

SUCCESS:
    ID: success.handled
    ALL: [TRUE]

TASK:
    ID: task.handled
    GOAL: REF(goal.handled)
    INPUT: REF(input.endpoint)
    ACTION: REF(action.fetch)
    HANDLER: REF(handler.read)
    SUCCESS: REF(success.handled)

EXECUTE:
    REFERENCE: REF(task.handled)
"#
    .to_string()
}

fn run(claim: Claim) -> lcl_runtime::Execution {
    let fixture = common::fixture(&handled_document());
    let mut host = ClaimingHost::new(claim);
    lcl_runtime::Runtime::new(common::contracts())
        .execute(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut host,
        )
        .expect("preflight planned it")
}

/// What handler selection did with the raised event.
///
/// This, and not the invocation list, is where the answer lives: a handler's
/// own result is not published as another invocation record, and
/// `Disposition::Selected { recovered, .. }` is "true exactly when the handler
/// invocation ... recorded `status.succeeded`". So whether a malformed result
/// was believed is precisely whether `recovered` is true.
fn recovered(execution: &lcl_runtime::Execution) -> bool {
    execution.events().iter().any(|event| {
        matches!(
            &event.disposition,
            lcl_runtime::Disposition::Selected {
                recovered: true,
                ..
            }
        )
    })
}

/// The diagnostics the execution kept.
fn diagnostics(execution: &lcl_runtime::Execution) -> Vec<String> {
    execution
        .diagnostics()
        .iter()
        .map(|d| d.id.as_registry_str().to_string())
        .collect()
}

/// A handler was selected at all, so a case that asserts non-recovery is not
/// passing because nothing ran.
fn selected(execution: &lcl_runtime::Execution) -> bool {
    execution.events().iter().any(|event| {
        matches!(
            &event.disposition,
            lcl_runtime::Disposition::Selected { .. }
        )
    })
}

/// A handler result missing a field its schema requires for a success is not a
/// success, and cannot therefore be a recovery.
#[test]
fn a_handler_result_missing_a_required_field_is_not_accepted_as_a_recovery() {
    let execution = run(Claim::MissingRequiredField);
    assert!(
        selected(&execution),
        "the fixture must actually select a handler, or this proves nothing"
    );
    assert!(
        !recovered(&execution),
        "a completed `result.value` carrying no `value` at all recovered the \
         diagnostic; its schema requires that field for a success. \
         diagnostics: {:?}",
        diagnostics(&execution)
    );
}

/// A handler result carrying a field its schema forbids is malformed, and a
/// closed field set that is only closed on one path is not closed.
#[test]
fn a_handler_result_carrying_a_forbidden_field_is_not_accepted() {
    let execution = run(Claim::ForbiddenField);
    assert!(selected(&execution), "a handler must have been selected");
    assert!(
        !recovered(&execution),
        "a `result.value` carrying a field that schema forbids recovered the \
         diagnostic; a closed field set that is closed on only one path is not \
         closed. diagnostics: {:?}",
        diagnostics(&execution)
    );
}

/// And one whose field is the wrong family is malformed for the same reason.
///
/// `result.value` registers `evidence` as `LIST[REFERENCE[EVIDENCE]]`. A STRING
/// there is not a narrower list; it is a different kind of thing.
#[test]
fn a_handler_result_with_a_wrong_field_family_is_not_accepted() {
    let execution = run(Claim::WrongFieldType);
    assert!(selected(&execution), "a handler must have been selected");
    assert!(
        !recovered(&execution),
        "a `result.value` whose `evidence` is a STRING recovered the \
         diagnostic. diagnostics: {:?}",
        diagnostics(&execution)
    );
}

/// The control. A well-formed handler result is still a successful recovery,
/// so none of the above is passing by refusing everything.
#[test]
fn a_well_formed_handler_result_still_recovers() {
    let execution = run(Claim::WellFormed);
    assert!(
        recovered(&execution),
        "a handler result that satisfies its schema must still recover, or \
         this repair has simply stopped handlers working. diagnostics: {:?}",
        diagnostics(&execution)
    );
}

/// The ordinary dispatch path already refuses a malformed success. This is the
/// control that says so, so the comparison between the two paths is evidenced
/// rather than asserted.
#[test]
fn the_ordinary_dispatch_path_refuses_the_same_malformed_success() {
    let source = r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.direct
    NAME: "Direct"
    VERSION: "1.0.0"
    KIND: kind.task

DATA:
    ID: data.subject
    TYPE: PATH
    VALUE: PATH("/srv/data/recovery.txt")

GOAL:
    ID: goal.direct
    ASSERT: TRUE

ACTION:
    ID: action.read
    OPERATION: core.read
    TARGET: REF(data.subject)

SUCCESS:
    ID: success.direct
    ALL: [TRUE]

TASK:
    ID: task.direct
    GOAL: REF(goal.direct)
    ACTION: REF(action.read)
    SUCCESS: REF(success.direct)

EXECUTE:
    REFERENCE: REF(task.direct)
"#;
    // A host that claims a completed read with no fields at all.
    let host = MockHost::new().script(
        "core.read",
        vec![CapabilityOutcome::Completed(Observation::none())],
    );
    let (execution, _host) = execute_with(source, host);
    let accepted = execution
        .invocations()
        .iter()
        .filter_map(|invocation| invocation.result.as_ref())
        .any(|result| result.status == "status.succeeded" && result.execution_errors.is_empty());
    assert!(
        !accepted,
        "the ordinary dispatch path must already refuse a completed \
         `result.value` with no `value`: {:?}",
        diagnostics(&execution)
    );
}
