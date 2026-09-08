//! Totality: the runtime returns for every input and never panics.
//!
//! `06_TESTING` discipline from the execution contract: "Malformed/untrusted
//! source must not panic." A runtime is the last layer where that matters most,
//! because it is the one that acts.
//!
//! Every case here is *adversarial by construction*: hostile shapes, extreme
//! nesting, pathological values and hosts that behave badly. None may panic,
//! hang, or leave the engine without a verdict.

mod common;

use lcl_runtime::{
    CapabilityOutcome, CapabilityRequest, Host, MockHost, Observation, Permission, Runtime,
    RuntimeError,
};

/// A host that always reports a limitation, for adversarial runs.
#[derive(Default)]
struct HostileHost {
    calls: usize,
}

impl Host for HostileHost {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        self.calls += 1;
        // Alternate refusal kinds, so the runtime meets both mappings.
        if self.calls % 2 == 0 {
            Permission::Denied("no".to_string())
        } else {
            Permission::Unavailable("no".to_string())
        }
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        unreachable!("never granted")
    }
}

/// A host that claims effects outside the operation's possible-effect set and
/// returns fields no schema declares.
#[derive(Default)]
struct LyingHost;

impl Host for LyingHost {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        Permission::Granted
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        CapabilityOutcome::Completed(
            Observation::none()
                .with("not_a_registered_field", lcl_runtime::Value::Boolean(true))
                .with_effect(lcl_runtime::ObservedEffect {
                    class: lcl_runtime::EffectClass::Process,
                    state: lcl_runtime::RecordState::Indeterminate,
                    target: None,
                    evidence: Vec::new(),
                }),
        )
    }
}

// ---------------------------------------------------------------------------
// Adversarial sources
// ---------------------------------------------------------------------------

#[test]
fn a_deeply_nested_expression_costs_heap_not_stack() {
    // The M2 stack-safety repair made the parser iterative; the evaluator has
    // its own depth budget, so a pathological expression is refused rather than
    // crashing the process.
    for depth in [64usize, 256, 1024] {
        let expression = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        let source = common::data_document(&[("data.subject", "INTEGER", &expression)]);
        let resolved = common::resolve(&source);
        // Whatever the earlier stages decide, nothing panics.
        let _ = common::check(&resolved);
    }
}

#[test]
fn a_deeply_nested_document_does_not_exhaust_the_stack() {
    // Nested control forms, which the engine walks iteratively.
    let mut body = String::from(
        "        STEP:\n            ID: step.deep\n            ACTION: REF(action.deep)\n",
    );
    for _ in 0..40 {
        let indented: String = body
            .lines()
            .map(|line| format!("    {line}\n"))
            .collect::<String>();
        body = format!("        IF (TRUE) THEN:\n{indented}");
    }
    let source = format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.deep
    NAME: "Deep"
    VERSION: "1.0.0"
    KIND: kind.task

GOAL:
    ID: goal.deep
    ASSERT: TRUE

ACTION:
    ID: action.deep
    OPERATION: core.inspect
    TARGET: REF(goal.deep)

SEQUENCE:
    ID: sequence.deep
{body}
SUCCESS:
    ID: success.deep
    ALL: [TRUE]

TASK:
    ID: task.deep
    GOAL: REF(goal.deep)
    SEQUENCE: REF(sequence.deep)
    SUCCESS: REF(success.deep)

EXECUTE:
    REFERENCE: REF(task.deep)
"#
    );
    // Whatever the earlier stages decide — deep nesting eventually meets the
    // lexer's own indentation rules — no stage panics. The test is totality,
    // not acceptance, so an early-stage refusal is a passing outcome.
    let unit =
        lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new("deep.lcl"), source.as_bytes());
    let Ok(resolved) =
        lcl_resolver::Resolver::new(common::rules(), common::grammar(), common::lexicon())
            .resolve(&unit, &lcl_resolver::MemoryProvider::new())
    else {
        return;
    };
    if resolved.primary().is_some() {
        return;
    }
    let checked = common::check(&resolved);
    if checked.primary().is_none() {
        let planned = lcl_semantics::Preflight::new(common::preflight_contracts()).plan(
            &checked,
            &resolved,
            &lcl_semantics::Invocation::new(),
        );
        if let Ok(planned) = planned {
            let mut host = MockHost::new();
            let _ =
                Runtime::new(common::contracts()).execute(&planned, &checked, &resolved, &mut host);
        }
    }
}

#[test]
fn hostile_byte_sequences_never_reach_the_runtime_by_panicking() {
    // Every one of these fails at some earlier stage. The point is that it
    // fails, rather than crashing on the way.
    for source in [
        "",
        "\u{0}",
        "LCL:",
        "LCL:\n\tVERSION: \"0.1.0\"\n",
        "LCL:\n    VERSION: \"0.1.0\"\n\u{FEFF}",
        &"[".repeat(5000),
        &"LCL:\n    VERSION: \"0.1.0\"\n".repeat(500),
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: \u{1F600}\n",
    ] {
        let unit = lcl_resolver::SourceUnit::new(
            lcl_resolver::SourceId::new("hostile.lcl"),
            source.as_bytes(),
        );
        let outcome =
            lcl_resolver::Resolver::new(common::rules(), common::grammar(), common::lexicon())
                .resolve(&unit, &lcl_resolver::MemoryProvider::new());
        // Either an earlier stage refused it, or it resolved; neither panics.
        if let Ok(resolved) = outcome {
            let _ = common::check(&resolved);
        }
    }
}

// ---------------------------------------------------------------------------
// Adversarial hosts
// ---------------------------------------------------------------------------

#[test]
fn a_hostile_host_produces_diagnostics_not_panics() {
    let fixture = common::example_fixture("04_AUTOMATED_CODING_TASK.lcl");
    let mut host = HostileHost::default();
    let execution = Runtime::new(common::contracts())
        .execute(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut host,
        )
        .expect("planned");
    assert!(!execution.diagnostics().is_empty());
    // Every diagnostic is a registered identifier with registry metadata.
    for diagnostic in execution.diagnostics() {
        assert!(matches!(
            diagnostic.id,
            RuntimeError::PermissionDenied | RuntimeError::HostConstraint
        ));
        assert!(!diagnostic.meaning.is_empty());
        assert!(!diagnostic.default_status.is_empty());
    }
}

#[test]
fn a_lying_host_cannot_corrupt_the_result_record() {
    // A host that reports an unregistered field and an out-of-set effect still
    // yields a result whose cross-axis invariants hold: the runtime, not the
    // host, decides phase, effect state and OUTPUT binding.
    let fixture = common::example_fixture("01_MINIMAL_TASK.lcl");
    let mut host = LyingHost;
    let execution = Runtime::new(common::contracts())
        .execute(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut host,
        )
        .expect("planned");
    for record in execution.invocations() {
        if let Some(result) = &record.result {
            assert!(
                result.violations().is_empty(),
                "{}: {:?}",
                record.id,
                result.violations()
            );
        }
    }
}

#[test]
fn every_canonical_example_survives_every_hostile_host() {
    for name in common::canonical_example_names() {
        for kind in 0..3 {
            let fixture = common::example_fixture(&name);
            let execution = match kind {
                0 => {
                    let mut host = HostileHost::default();
                    Runtime::new(common::contracts()).execute(
                        &fixture.planned,
                        &fixture.checked,
                        &fixture.resolved,
                        &mut host,
                    )
                }
                1 => {
                    let mut host = LyingHost;
                    Runtime::new(common::contracts()).execute(
                        &fixture.planned,
                        &fixture.checked,
                        &fixture.resolved,
                        &mut host,
                    )
                }
                _ => {
                    let mut host = MockHost::new();
                    Runtime::new(common::contracts()).execute(
                        &fixture.planned,
                        &fixture.checked,
                        &fixture.resolved,
                        &mut host,
                    )
                }
            }
            .expect("planned");
            // The run terminated inside its budget, whatever the host did.
            assert!(execution.steps() < 1_000_000, "{name} kind {kind}");
            for record in execution.invocations() {
                if let Some(result) = &record.result {
                    assert!(
                        result.violations().is_empty(),
                        "{name} kind {kind} {}: {:?}",
                        record.id,
                        result.violations()
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Two identifiers this milestone mirrors and never emits
// ---------------------------------------------------------------------------

#[test]
fn the_runtime_never_emits_a_preflight_owned_identifier() {
    // `error.dependency.unsatisfied` and `error.scope.violation` are "pre_effect
    // only" and belong to M5. They are mirrored for stage parity and must never
    // be raised here, under any host.
    for name in common::canonical_example_names() {
        for hostile in [false, true] {
            let fixture = common::example_fixture(&name);
            let execution = if hostile {
                let mut host = HostileHost::default();
                Runtime::new(common::contracts()).execute(
                    &fixture.planned,
                    &fixture.checked,
                    &fixture.resolved,
                    &mut host,
                )
            } else {
                let mut host = MockHost::new();
                Runtime::new(common::contracts()).execute(
                    &fixture.planned,
                    &fixture.checked,
                    &fixture.resolved,
                    &mut host,
                )
            }
            .expect("planned");
            for diagnostic in execution.diagnostics() {
                assert!(
                    !diagnostic.id.is_elsewhere(),
                    "{name} emitted {}, which another milestone decides",
                    diagnostic.id
                );
            }
        }
    }
}

#[test]
fn the_runtime_never_emits_the_deferred_determinism_identifier() {
    // `error.determinism.mismatch` is deferred to M7 by name, exactly as M5
    // deferred it. It is not even in this layer's mirrored set.
    assert!(RuntimeError::from_registry_str("error.determinism.mismatch").is_none());
}
