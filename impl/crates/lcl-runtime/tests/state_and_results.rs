//! Invocation identity, the registered status lifecycle, result-record
//! invariants and the diagnostic-selection contract.

mod common;

use common::contracts;
use lcl_lexer::{Position, Span};
use lcl_resolver::SourceId;
use lcl_runtime::diagnostic::{select, Diagnostic};
use lcl_runtime::{
    Bindings, Cause, EffectClass, EffectState, FailurePhase, InvocationId, IterationPath,
    Lifecycle, ObservedEffect, OutputBinding, RecordState, ResultRecord, RuntimeError,
    TransitionRefusal, Value,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

#[test]
fn loop_instances_have_distinct_identities() {
    // `05_SEMANTICS/01`: "Instance identity includes the full enclosing
    // iteration-index path."
    let root = IterationPath::root();
    let first = InvocationId::first(7, root.child(0));
    let second = InvocationId::first(7, root.child(1));
    assert_ne!(first, second, "two loop instances are two invocations");
    assert_eq!(first.node, second.node, "of one source declaration");
}

#[test]
fn retry_attempts_have_distinct_identities_but_one_aggregate() {
    let first = InvocationId::first(3, IterationPath::root());
    let second = first.next_attempt();
    assert_ne!(first, second);
    assert_eq!(
        first.aggregate(),
        second.aggregate(),
        "both attempts belong to one ACTION invocation, which owns the budget"
    );
}

#[test]
fn nested_iteration_paths_order_outermost_first() {
    let root = IterationPath::root();
    let outer = root.child(1);
    let inner = outer.child(0);
    assert_eq!(inner.indexes(), &[1, 0]);
    assert!(inner.starts_with(&outer));
    assert!(!outer.starts_with(&inner));
    assert!(inner.starts_with(&root));
}

#[test]
fn invocation_ids_sort_by_path_then_iteration_then_attempt() {
    let root = IterationPath::root();
    let mut ids = vec![
        InvocationId::new(2, root.child(1), 0),
        InvocationId::new(1, root.clone(), 1),
        InvocationId::new(2, root.child(0), 3),
        InvocationId::new(1, root.clone(), 0),
    ];
    ids.sort();
    assert_eq!(
        ids,
        vec![
            InvocationId::new(1, root.clone(), 0),
            InvocationId::new(1, root.clone(), 1),
            InvocationId::new(2, root.child(0), 3),
            InvocationId::new(2, root.child(1), 0),
        ]
    );
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[test]
fn transitions_follow_the_registrys_allowed_next() {
    let registry = contracts().diagnostics();
    let mut life = Lifecycle::planned(true);
    assert_eq!(life.current(), "status.ready");
    // `status.ready` allows `status.running`.
    assert!(life.transition(registry, "status.running").is_ok());
    // `status.running` allows `status.succeeded`.
    assert!(life.transition(registry, "status.succeeded").is_ok());
    assert!(life.is_terminal(registry));
    // A terminal status has no outgoing transition.
    assert!(matches!(
        life.transition(registry, "status.running"),
        Err(TransitionRefusal::NotAllowed { .. })
    ));
}

#[test]
fn blocked_is_terminal_with_no_outgoing_transition() {
    // "status.blocked is terminal for that invocation and has no outgoing
    // transition."
    let registry = contracts().diagnostics();
    let mut life = Lifecycle::planned(true);
    life.transition(registry, "status.blocked").expect("ready permits blocked");
    assert!(life.is_terminal(registry));
    for status in [
        "status.running",
        "status.succeeded",
        "status.failed",
        "status.ready",
    ] {
        assert!(
            !life.permits(registry, status),
            "blocked must not permit {status}"
        );
    }
}

#[test]
fn an_execution_root_cannot_be_skipped() {
    // "An execution root cannot take this transition." A non-root may.
    let registry = contracts().diagnostics();
    let mut root = Lifecycle::planned(true);
    assert!(matches!(
        root.refuse(registry, "status.skipped"),
        Some(TransitionRefusal::RootCannotSkip)
    ));
    let mut child = Lifecycle::planned(false);
    assert!(child.transition(registry, "status.skipped").is_ok());
    assert_eq!(child.current(), "status.skipped");
}

#[test]
fn an_unregistered_status_is_refused_by_name() {
    let registry = contracts().diagnostics();
    let life = Lifecycle::planned(false);
    assert!(matches!(
        life.refuse(registry, "status.invented"),
        Some(TransitionRefusal::Unregistered(_))
    ));
}

#[test]
fn a_planned_invocation_retains_its_earlier_lifecycle_history() {
    let life = Lifecycle::planned(true);
    assert_eq!(
        life.history(),
        ["status.not_started", "status.validating", "status.ready"]
    );
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

#[test]
fn a_loop_local_resolves_to_its_own_instance() {
    let root = IterationPath::root();
    let mut bindings = Bindings::new();
    bindings.bind_local("file", &root.child(0), Value::Text("a".into()));
    bindings.bind_local("file", &root.child(1), Value::Text("b".into()));
    assert_eq!(
        bindings.local("file", &root.child(0)),
        Some(&Value::Text("a".into()))
    );
    assert_eq!(
        bindings.local("file", &root.child(1)),
        Some(&Value::Text("b".into()))
    );
    // "The local binding is visible only within its body": nothing outside any
    // instance sees one.
    assert_eq!(bindings.local("file", &root), None);
}

#[test]
fn an_inner_loop_sees_the_innermost_binding() {
    let root = IterationPath::root();
    let outer = root.child(0);
    let inner = outer.child(2);
    let mut bindings = Bindings::new();
    bindings.bind_local("item", &outer, Value::Text("outer".into()));
    bindings.bind_local("item", &inner, Value::Text("inner".into()));
    assert_eq!(
        bindings.local("item", &inner),
        Some(&Value::Text("inner".into()))
    );
    assert_eq!(
        bindings.local("item", &outer),
        Some(&Value::Text("outer".into()))
    );
}

#[test]
fn releasing_an_instance_drops_its_locals_and_its_nested_ones() {
    let root = IterationPath::root();
    let outer = root.child(0);
    let inner = outer.child(0);
    let mut bindings = Bindings::new();
    bindings.bind_local("a", &outer, Value::Boolean(true));
    bindings.bind_local("b", &inner, Value::Boolean(false));
    bindings.release_locals(&outer);
    assert_eq!(bindings.local("a", &outer), None);
    assert_eq!(bindings.local("b", &inner), None);
}

#[test]
fn each_loop_instance_binds_its_own_output() {
    // "An ACTION replicated by FOR EACH has a separate OUTPUT binding per full
    // enclosing iteration-index path."
    let root = IterationPath::root();
    let mut bindings = Bindings::new();
    bindings.bind_output("output.one", &root.child(0), Value::Integer(int(1)));
    bindings.bind_output("output.one", &root.child(1), Value::Integer(int(2)));
    assert_eq!(
        bindings.output("output.one", &root.child(0)),
        Some(&Value::Integer(int(1)))
    );
    assert_eq!(
        bindings.output("output.one", &root.child(1)),
        Some(&Value::Integer(int(2)))
    );
}

#[test]
fn a_retry_attempt_starts_with_its_output_unbound() {
    // "Each retry attempt starts with its own selected OUTPUT binding unbound."
    let root = IterationPath::root();
    let mut bindings = Bindings::new();
    bindings.bind_output("output.payload", &root, Value::Text("first".into()));
    bindings.unbind_output("output.payload", &root);
    assert_eq!(bindings.output("output.payload", &root), None);
}

fn int(value: u64) -> lcl_checker::numeric::Decimal {
    lcl_checker::numeric::Decimal::from_integer(lcl_checker::numeric::Integer::from_u64(value))
}

// ---------------------------------------------------------------------------
// Result records
// ---------------------------------------------------------------------------

#[test]
fn a_clean_result_has_no_violations() {
    let record = ResultRecord::new("result.value", "status.succeeded")
        .with_field("value", Value::Text("x".into()));
    assert!(record.violations().is_empty(), "{:?}", record.violations());
    assert!(record.succeeded());
}

#[test]
fn a_pre_effect_failure_forbids_effects_and_output() {
    // "A pre-effect failure requires effect_state none, an empty
    // observed_effects list, and no bound or partial OUTPUT."
    let mut record = ResultRecord::new("result.operation", "status.failed");
    record.failure_phase = FailurePhase::PreEffect;
    record.execution_errors = vec![RuntimeError::ExecutionAction.as_registry_str().to_string()];
    record.effect_state = EffectState::Applied;
    record.output_binding = OutputBinding::Bound;
    record.observed_effects = vec![ObservedEffect {
        class: EffectClass::Filesystem,
        state: RecordState::Applied,
        target: None,
        evidence: Vec::new(),
    }];
    let violations = record.violations();
    assert_eq!(violations.len(), 3, "{violations:?}");
    assert!(violations.iter().any(|v| v.contains("effect_state none")));
    assert!(violations.iter().any(|v| v.contains("observed_effects")));
    assert!(violations.iter().any(|v| v.contains("OUTPUT")));
}

#[test]
fn a_post_effect_failure_requires_a_known_begun_effect() {
    let mut record = ResultRecord::new("result.operation", "status.failed");
    record.failure_phase = FailurePhase::PostEffect;
    record.execution_errors = vec![RuntimeError::ExecutionAction.as_registry_str().to_string()];
    assert!(record
        .violations()
        .iter()
        .any(|v| v.contains("at least one known begun effect")));
}

#[test]
fn failure_phase_none_is_illegal_while_an_error_is_unhandled() {
    // "none is not an allowed failure phase when an unhandled error exists at
    // that producer."
    let mut record = ResultRecord::new("result.value", "status.failed");
    record.execution_errors = vec![RuntimeError::HostConstraint.as_registry_str().to_string()];
    assert!(record
        .violations()
        .iter()
        .any(|v| v.contains("failure_phase none is not allowed")));
}

#[test]
fn status_partial_does_not_imply_partial_effects_or_output() {
    // "status.partial ... does not imply that OUTPUT is partial and does not
    // imply that effects are partial."
    let record = ResultRecord::new("result.operation", "status.partial")
        .with_field("changed", Value::Boolean(true));
    assert!(record.violations().is_empty());
    assert_eq!(record.effect_state, EffectState::None);
    assert_eq!(record.output_binding, OutputBinding::NotRequested);
}

#[test]
fn every_closed_axis_round_trips_through_its_registry_spelling() {
    for phase in FailurePhase::ALL {
        assert_eq!(
            FailurePhase::from_registry_str(phase.as_registry_str()),
            Some(phase)
        );
    }
    for state in EffectState::ALL {
        assert_eq!(
            EffectState::from_registry_str(state.as_registry_str()),
            Some(state)
        );
    }
    for binding in OutputBinding::ALL {
        assert_eq!(
            OutputBinding::from_registry_str(binding.as_registry_str()),
            Some(binding)
        );
    }
    for class in EffectClass::ALL {
        assert_eq!(
            EffectClass::from_registry_str(class.as_registry_str()),
            Some(class)
        );
    }
    for state in RecordState::ALL {
        assert_eq!(
            RecordState::from_registry_str(state.as_registry_str()),
            Some(state)
        );
    }
    // `none` is a sentinel, never an effect class or a record state.
    assert_eq!(EffectClass::from_registry_str("none"), None);
    assert_eq!(RecordState::from_registry_str("none"), None);
}

// ---------------------------------------------------------------------------
// Diagnostic selection
// ---------------------------------------------------------------------------

fn diagnostic(id: RuntimeError, offset: usize, producer: Option<InvocationId>) -> Diagnostic {
    let registered = contracts().error(id);
    Diagnostic {
        id,
        registered_stage: registered.stage,
        resolved_stage: None,
        source: SourceId::new("root.lcl"),
        span: Span::new(offset, offset + 1),
        position: Position {
            offset,
            line: 1,
            column: 1,
        },
        meaning: registered.meaning.clone(),
        default_status: registered.default_status.clone(),
        specificity_rank: registered.specificity_rank,
        event: registered.event.clone(),
        cause: Cause::new("test"),
        producer_path: producer.as_ref().map(|p| p.node),
        producer,
        failure_phase: FailurePhase::PreEffect,
        detail: None,
    }
}

#[test]
fn stable_order_sorts_by_declared_path_then_iteration_then_attempt() {
    // `stable_order`: "canonical source byte offset or declared execution-path
    // order ascending, iteration index ascending, retry-attempt index
    // ascending".
    let root = IterationPath::root();
    let raw = vec![
        diagnostic(
            RuntimeError::ExecutionAction,
            10,
            Some(InvocationId::new(2, root.child(1), 0)),
        ),
        diagnostic(
            RuntimeError::ExecutionAction,
            10,
            Some(InvocationId::new(2, root.child(0), 1)),
        ),
        diagnostic(
            RuntimeError::ExecutionAction,
            10,
            Some(InvocationId::new(1, root.clone(), 0)),
        ),
        diagnostic(
            RuntimeError::ExecutionAction,
            10,
            Some(InvocationId::new(2, root.child(0), 0)),
        ),
    ];
    let ordered = select(raw, contracts().supersedes());
    let keys: Vec<(usize, Vec<usize>, usize)> = ordered
        .iter()
        .map(|d| {
            let p = d.producer.clone().expect("producer");
            (p.node, p.iteration.indexes().to_vec(), p.attempt)
        })
        .collect();
    assert_eq!(
        keys,
        vec![
            (1, vec![], 0),
            (2, vec![0], 0),
            (2, vec![0], 1),
            (2, vec![1], 0),
        ]
    );
}

#[test]
fn stage_order_precedes_every_other_key() {
    // A demand-resolved execution diagnostic at a later byte still sorts after
    // an earlier-stage one, because "stage_order ascending" is the first key.
    let mut early = diagnostic(RuntimeError::ValueUnknown, 900, None);
    early.resolved_stage = None; // registered static_or_expression
    let late = diagnostic(RuntimeError::ExecutionOrder, 10, None);
    let ordered = select(vec![late, early], contracts().supersedes());
    assert_eq!(ordered[0].id, RuntimeError::ValueUnknown);
    assert_eq!(ordered[1].id, RuntimeError::ExecutionOrder);
}

#[test]
fn distinct_iteration_indexes_are_independent_not_duplicates() {
    // `multiplicity_rule`: "Distinct source loci, declaration paths, invocation
    // paths, iteration indexes, or retry-attempt indexes are independent even
    // when their identifiers match."
    let root = IterationPath::root();
    let raw = vec![
        diagnostic(
            RuntimeError::RequiredMissing,
            10,
            Some(InvocationId::new(1, root.child(0), 0)),
        ),
        diagnostic(
            RuntimeError::RequiredMissing,
            10,
            Some(InvocationId::new(1, root.child(1), 0)),
        ),
    ];
    assert_eq!(select(raw, contracts().supersedes()).len(), 2);
}

#[test]
fn one_diagnostic_survives_per_duplicate_key() {
    // `duplicate_rule`: "emit one diagnostic for each duplicate_key ...
    // Discovery count and discovery time do not affect output."
    let root = IterationPath::root();
    let one = diagnostic(
        RuntimeError::HostConstraint,
        10,
        Some(InvocationId::new(1, root.clone(), 0)),
    );
    let raw = vec![one.clone(), one.clone(), one];
    assert_eq!(select(raw, contracts().supersedes()).len(), 1);
}

#[test]
fn selection_is_a_pure_function_of_its_input() {
    let root = IterationPath::root();
    let build = || {
        vec![
            diagnostic(
                RuntimeError::ExecutionOrder,
                20,
                Some(InvocationId::new(3, root.clone(), 0)),
            ),
            diagnostic(
                RuntimeError::RequiredMissing,
                5,
                Some(InvocationId::new(1, root.clone(), 0)),
            ),
        ]
    };
    let first = select(build(), contracts().supersedes());
    let second = select(build(), contracts().supersedes());
    assert_eq!(first, second);
}

#[test]
fn a_demand_resolved_diagnostic_keeps_its_registered_stage_as_evidence() {
    // `resolution_rule`: "Retain the canonical error identifier, registered
    // source-stage metadata, resolved demand stage, and demand locus in
    // evidence."
    let mut demanded = diagnostic(RuntimeError::NumericDivisionByZero, 42, None);
    let registered = demanded.registered_stage;
    demanded.resolved_stage = Some(contracts().demand().resolved_stage);
    demanded.default_status = contracts()
        .demand()
        .status_for(RuntimeError::NumericDivisionByZero)
        .to_string();
    assert_eq!(demanded.registered_stage, registered, "registered stage is retained");
    assert_eq!(demanded.stage(), lcl_diagnostics::Stage::Execution);
    assert_eq!(demanded.default_status, "status.failed");
    assert!(format!("{demanded}").contains("demand-resolved"));
}

#[test]
fn a_result_record_serializes_stably() {
    let mut record = ResultRecord::new("result.command", "status.succeeded");
    record.fields = BTreeMap::from([
        ("stdout".to_string(), Value::Text("out".into())),
        ("exit_code".to_string(), Value::Integer(int(0))),
    ]);
    assert_eq!(record.serialize(), record.serialize());
    assert!(record.serialize().contains("exit_code=0"));
    assert!(record.serialize().contains("stdout=\"out\""));
}
