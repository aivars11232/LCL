//! The effect boundary: two independent gates, and a host that observes
//! rather than decides.
//!
//! These tests are the structural half of this milestone's safety criterion,
//! "All external effects are represented as capability requests; runtime core
//! itself does not directly touch filesystem/process/network/provider APIs."
//! The behavioural half — that the engine routes every action through here —
//! is proved once the engine exists.

mod common;

use lcl_lexer::Span;
use lcl_resolver::SourceId;
use lcl_runtime::capability::{request, Refusal};
use lcl_runtime::{
    Authorized, CapabilityOutcome, CapabilityRequest, EffectClass, Host, InvocationId,
    IterationPath, MockHost, Observation, Permission, Value,
};
use std::collections::{BTreeMap, BTreeSet};

fn authorized(operation: &str) -> Authorized {
    Authorized {
        operation: operation.to_string(),
        target: Some("PATH(\"/tmp/x\")".to_string()),
        scope: Some("scope.workspace".to_string()),
        permitted_by: vec!["permission.write".to_string()],
        overridden: Vec::new(),
    }
}

fn req(operation: &str, schema: &str, effects: &[&str]) -> CapabilityRequest {
    CapabilityRequest {
        operation: operation.to_string(),
        target: Some(Value::Text("/tmp/x".into())),
        parameters: BTreeMap::new(),
        authorization: authorized(operation),
        category: "mutating".to_string(),
        possible_effects: effects.iter().map(|e| e.to_string()).collect(),
        possible_dependencies: BTreeSet::from(["host".to_string()]),
        result_schema: schema.to_string(),
        invocation: InvocationId::first(1, IterationPath::root()),
        source: SourceId::new("root.lcl"),
        span: Span::new(0, 1),
    }
}

/// A host that records whether it was ever consulted at all.
#[derive(Default)]
struct CountingHost {
    permits_calls: usize,
    invoke_calls: usize,
}

impl Host for CountingHost {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        self.permits_calls += 1;
        Permission::Granted
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        self.invoke_calls += 1;
        CapabilityOutcome::Completed(Observation::none())
    }
}

#[test]
fn lcl_authorization_alone_does_not_force_host_permission() {
    // "LCL authorization does not force host permission."
    let mut host = MockHost::new().deny("core.write", "the host forbids this path");
    let outcome = request(
        &mut host,
        true,
        &req("core.write", "result.operation", &["filesystem"]),
    );
    assert!(matches!(outcome, Err(Refusal::Denied(_))));
    assert_eq!(
        host.requests().len(),
        0,
        "a refused request never reaches invoke"
    );
}

#[test]
fn host_permission_alone_does_not_imply_lcl_authorization() {
    // "Host permission does not imply LCL authorization." The host here grants
    // everything; the language did not authorize it, so it never runs.
    let mut host = CountingHost::default();
    let outcome = request(
        &mut host,
        false,
        &req("core.write", "result.operation", &["filesystem"]),
    );
    assert!(matches!(outcome, Err(Refusal::Unauthorized(_))));
    assert_eq!(
        host.invoke_calls, 0,
        "an unauthorized request must not reach the host at all"
    );
    assert_eq!(
        host.permits_calls, 0,
        "the language gate is checked before the host is consulted"
    );
}

#[test]
fn both_gates_passing_is_what_reaches_the_host() {
    let mut host = CountingHost::default();
    let outcome = request(
        &mut host,
        true,
        &req("core.write", "result.operation", &["filesystem"]),
    );
    assert!(matches!(outcome, Ok(CapabilityOutcome::Completed(_))));
    assert_eq!(host.permits_calls, 1);
    assert_eq!(host.invoke_calls, 1);
}

#[test]
fn an_unavailable_capability_is_distinct_from_a_refused_one() {
    // A host limitation is `error.host.constraint`; a refusal is
    // `error.permission.denied`. Collapsing them would lose the distinction
    // 05_SEMANTICS/09 draws.
    let mut host = MockHost::new().unavailable("core.download", "no network capability");
    let outcome = request(
        &mut host,
        true,
        &req("core.download", "result.transfer", &["network"]),
    );
    assert!(matches!(outcome, Err(Refusal::Unavailable(_))));
}

#[test]
fn the_mock_host_is_deterministic_for_identical_requests() {
    let run = || {
        let mut host = MockHost::new();
        let request_one = req("core.inspect", "result.value", &[]);
        let a = request(&mut host, true, &request_one).expect("granted");
        let b = request(&mut host, true, &request_one).expect("granted");
        (a, b)
    };
    assert_eq!(run(), run(), "the same requests produce the same outcomes");
}

#[test]
fn scripted_outcomes_are_consumed_in_invocation_order() {
    // This is how an attempt sequence is expressed without nondeterminism:
    // CLOSURE-022 is "initial failure then successful first retry".
    let mut host = MockHost::new().script(
        "core.download",
        vec![
            MockHost::failed_before_effect("first attempt failed"),
            MockHost::completed(),
        ],
    );
    let r = req("core.download", "result.transfer", &["network"]);
    let first = request(&mut host, true, &r).expect("granted");
    let second = request(&mut host, true, &r).expect("granted");
    let third = request(&mut host, true, &r).expect("granted");
    assert!(matches!(first, CapabilityOutcome::Failed { .. }));
    assert!(matches!(second, CapabilityOutcome::Completed(_)));
    // After the script is exhausted the default applies.
    assert!(matches!(third, CapabilityOutcome::Completed(_)));
    assert_eq!(host.count("core.download"), 3);
}

#[test]
fn a_read_only_operation_records_no_effect() {
    // A read_only row has `possible_effects` exactly `{none}`, which the axes
    // loader represents as an empty set. No record is ever created for the
    // `none` sentinel.
    let mut host = MockHost::new();
    let outcome =
        request(&mut host, true, &req("core.inspect", "result.value", &[])).expect("granted");
    match outcome {
        CapabilityOutcome::Completed(observation) => {
            assert!(observation.effects.is_empty());
            assert!(observation.proven_effect_free);
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn a_mutating_operation_records_an_effect_inside_its_possible_set() {
    // "The class is within the operation invocation's resolved possible-effect
    // set."
    let mut host = MockHost::new();
    let outcome = request(
        &mut host,
        true,
        &req("core.write", "result.operation", &["filesystem"]),
    )
    .expect("granted");
    match outcome {
        CapabilityOutcome::Completed(observation) => {
            assert_eq!(observation.effects.len(), 1);
            assert_eq!(observation.effects[0].class, EffectClass::Filesystem);
            assert!(!observation.proven_effect_free);
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn default_observations_have_their_registered_schema_shape() {
    let mut host = MockHost::new();
    let command = request(
        &mut host,
        true,
        &req("core.execute", "result.command", &["process"]),
    )
    .expect("granted");
    match command {
        CapabilityOutcome::Completed(observation) => {
            // "In non_graph mode, started and completed are always present."
            assert_eq!(
                observation.fields.get("mode"),
                Some(&Value::Identifier("non_graph".into()))
            );
            assert_eq!(
                observation.fields.get("started"),
                Some(&Value::Boolean(true))
            );
            assert_eq!(
                observation.fields.get("completed"),
                Some(&Value::Boolean(true))
            );
            // "exit_code is present exactly when completed is TRUE."
            assert!(observation.fields.contains_key("exit_code"));
            // "stdout and stderr are present even when empty".
            assert_eq!(
                observation.fields.get("stdout"),
                Some(&Value::Text(String::new()))
            );
            assert_eq!(
                observation.fields.get("stderr"),
                Some(&Value::Text(String::new()))
            );
        }
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn the_host_cannot_return_a_status_or_an_error_identifier() {
    // A structural check on the boundary contract: "It may not smuggle hidden
    // semantic decisions back into the runtime." An `Observation` carries only
    // fields, effects and an effect-freedom proof — there is no field a host
    // could write `status.succeeded` or `error.host.constraint` into.
    let observation = Observation::none().with("value", Value::Text("x".into()));
    let rendered = format!("{observation:?}");
    assert!(!rendered.contains("status."), "{rendered}");
    assert!(!rendered.contains("error."), "{rendered}");
}

#[test]
fn a_delay_is_handed_to_the_host_and_never_waited_on_here() {
    // `RETRY.DELAY` is an effect on the host's timeline. The runtime core has
    // no clock, so determinism does not depend on how long a host waits.
    let mut host = MockHost::new();
    let duration = Value::Constructed {
        constructor: "DURATION".to_string(),
        text: "1 unit.second".to_string(),
    };
    host.delay(&duration);
    assert_eq!(host.delays(), &[duration]);
}
