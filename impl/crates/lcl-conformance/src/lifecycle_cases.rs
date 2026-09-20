//! Diagnostic selection and failure lifecycle: the two contract rows whose
//! subjects are the engine's own diagnostic behavior.
//!
//! `diagnostic_selection` and `failure_lifecycle` are stated over every stage
//! at once — which diagnostics survive, in what order, which one is primary,
//! and what phase, status and OUTPUT each failure records. The runs here are
//! therefore ordinary documents carried through the whole engine, plus
//! component runs where the contract is about the vocabulary the engine
//! loaded rather than about one execution.

use crate::{judge, ExecutedCase, Expectation, Observed, Runner};
use lcl_runtime::{
    CapabilityOutcome, EffectClass, MockHost, Observation, ObservedEffect, RecordState,
};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;

fn group(id: &str, contract: &str, runs: Vec<ExecutedCase>) -> ExecutedCase {
    let source = runs
        .iter()
        .map(|r| format!("SUBCASE {}\n{}", r.id, r.source))
        .collect::<Vec<_>>()
        .join("\n");
    let expectation = Expectation::Runs(runs.iter().map(|r| r.expectation.clone()).collect());
    let observed = Observed {
        run_labels: runs.iter().map(|r| r.id.clone()).collect(),
        runs: runs.into_iter().map(|r| r.observed).collect(),
        ..Observed::default()
    };
    let verdict = judge(&expectation, &observed);
    ExecutedCase {
        id: id.into(),
        contract: contract.into(),
        source,
        expectation,
        observed,
        verdict,
    }
}

fn component(
    label: &str,
    clause: &str,
    input: String,
    expected: Vec<(String, String)>,
    actual: Vec<(String, String)>,
) -> ExecutedCase {
    let expectation = Expectation::Component(expected);
    let observed = Observed {
        component: actual,
        input_evidence: vec![input.clone()],
        ..Observed::default()
    };
    let verdict = judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: clause.into(),
        source: input,
        expectation,
        observed,
        verdict,
    }
}

/// Run one document and compare an exact rendering of what the engine selected.
fn selected(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    host: MockHost,
    expected: Vec<(&str, String)>,
) -> ExecutedCase {
    let mut host = host;
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, &mut host);
    let observed = &case.observed;
    let actual: Vec<(String, String)> = expected
        .iter()
        .map(|(key, _)| {
            let value = match *key {
                "diagnostics" => format!("{:?}", observed.diagnostics),
                "primary" => format!("{:?}", observed.primary),
                "primary_stage" => format!("{:?}", observed.primary_stage),
                "terminal" => format!("{:?}", observed.terminal_status),
                "attempt_phases" => format!(
                    "{:?}",
                    observed
                        .invocations
                        .iter()
                        .filter_map(|i| i
                            .result
                            .as_ref()
                            .map(|r| format!("{}:{}", i.id.attempt, r.failure_phase)))
                        .collect::<Vec<_>>()
                ),
                "producers" => format!(
                    "{:?}",
                    observed
                        .invocations
                        .iter()
                        .filter_map(|i| i
                            .declaration
                            .clone()
                            .map(|d| format!("{d}:{}", i.status())))
                        .collect::<Vec<_>>()
                ),
                "subject" => observed
                    .invocations
                    .iter()
                    .filter(|i| i.declaration.as_deref() == Some("action.subject"))
                    .filter_map(|i| i.result.as_ref())
                    .map(|r| {
                        format!(
                            "status={} phase={} effects={} errors={:?} binding={}",
                            r.status,
                            r.failure_phase,
                            r.effect_state,
                            r.execution_errors,
                            r.output_binding
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | "),
                "outputs" => format!("{:?}", observed.outputs),
                "events" => format!(
                    "{:?}",
                    observed
                        .events
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                ),
                other => panic!("unknown observation key {other}"),
            };
            (key.to_string(), value)
        })
        .collect();
    case.expectation = Expectation::Component(
        expected
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    );
    case.observed.component = actual;
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case.verdict = judge(&case.expectation, &case.observed);
    case
}

/// The demanded division whose value-domain failure resolves to execution.
fn demand_source() -> String {
    document(
        "",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.calculate\n    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"1 / 0\"\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    )
}

/// A scripted read that fails `failures` times and then completes.
fn advance_host(failures: usize) -> MockHost {
    let mut outcomes: Vec<CapabilityOutcome> = (0..failures)
        .map(|n| CapabilityOutcome::Unavailable(format!("conformance: scripted limitation {n}")))
        .collect();
    outcomes.push(CapabilityOutcome::Completed(
        Observation::none()
            .with("value", lcl_runtime::Value::Text("complete".into()))
            .with("evidence", lcl_runtime::Value::List(Vec::new())),
    ));
    MockHost::new().script("core.read", outcomes)
}

fn document(declarations: &str, body: &str) -> String {
    crate::fixtures::task_document(&format!("{declarations}{body}"))
}

/// A task with one subject ACTION over `core.read`, and an optional handler.
const UNRESOLVED_PAIR: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n    ID: data.first\n    TYPE: STRING\n    VALUE: REF(data.absent_first)\n\nDATA:\n    ID: data.second\n    TYPE: STRING\n    VALUE: REF(data.absent_second)\n";

/// The registry rules these rows are stated over.
fn selection(spec: &SpecPackage) -> &Json {
    spec.registry("statuses_and_errors")
        .and_then(|registry| registry.get("diagnostic_selection"))
        .expect("the diagnostic selection contract")
}

fn lifecycle(spec: &SpecPackage) -> &Json {
    spec.registry("statuses_and_errors")
        .and_then(|registry| registry.get("failure_lifecycle"))
        .expect("the failure lifecycle contract")
}

fn diagnostic_policy(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    let registry =
        lcl_diagnostics::DiagnosticRegistry::load(spec).expect("the diagnostic registry loads");
    let canonical = selection(spec);
    let mut runs = Vec::new();

    // -- the vocabulary the engine loaded ---------------------------------
    let errors = spec
        .registry("statuses_and_errors")
        .and_then(|r| r.get("errors"))
        .and_then(Json::as_object)
        .expect("the registered errors");
    let complete: Vec<String> = errors
        .iter()
        .filter(|(id, _)| {
            registry.error(id).is_some_and(|error| {
                !error.stage.as_registry_str().is_empty()
                    && !error.default_status.is_empty()
                    && !error.meaning.is_empty()
            })
        })
        .map(|(id, _)| id.clone())
        .collect();
    runs.push(component(
        "metadata/all-registered-errors",
        "complete metadata for every registered error resolves from the closed contract",
        "DiagnosticRegistry::load(approved 0.1.0): every registered error's stage, default status and meaning".into(),
        vec![("errors with complete metadata".into(), errors.len().to_string())],
        vec![("errors with complete metadata".into(), complete.len().to_string())],
    ));
    let severity = canonical
        .get("severity")
        .and_then(|s| s.get("closed_values"));
    runs.push(component(
        "severity/single-error-value",
        "exactly one severity, error: a one-valued key orders nothing",
        format!("diagnostic_selection.severity as the engine loaded it: {severity:?}"),
        vec![("closed severity values".into(), "[\"error\"]".into())],
        vec![(
            "closed severity values".into(),
            format!(
                "{:?}",
                registry
                    .selection_contract()
                    .get("severity")
                    .and_then(|s| s.get("closed_values"))
                    .and_then(Json::as_array)
                    .map(|values| values.iter().filter_map(Json::as_str).collect::<Vec<_>>())
                    .unwrap_or_default()
            ),
        )],
    ));
    let canonical_stages: Vec<String> = canonical
        .get("stage_order")
        .and_then(Json::as_array)
        .map(|stages| {
            stages
                .iter()
                .filter_map(Json::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    runs.push(component(
        "stage/registered-order",
        "the seven registered stages are evaluated in the registry's order",
        "DiagnosticRegistry::stage_order()".into(),
        vec![("stage order".into(), format!("{canonical_stages:?}"))],
        vec![(
            "stage order".into(),
            format!(
                "{:?}",
                registry
                    .stage_order()
                    .iter()
                    .map(|s| s.as_registry_str().to_string())
                    .collect::<Vec<_>>()
            ),
        )],
    ));
    let ranks = canonical.get("specificity_rank");
    runs.push(component(
        "order/specificity",
        "specificity_rank descending is the registry's own rank map",
        format!("diagnostic_selection.specificity_rank as the engine loaded it: {ranks:?}"),
        vec![("specificity_rank".into(), format!("{ranks:?}"))],
        vec![(
            "specificity_rank".into(),
            format!(
                "{:?}",
                registry.selection_contract().get("specificity_rank")
            ),
        )],
    ));
    let supersedes = canonical.get("supersedes");
    runs.push(component(
        "order/identifier",
        "the final tiebreak is the error identifier, over the same closed identifier set",
        format!("diagnostic_selection.supersedes and identifiers as the engine loaded them: {supersedes:?}"),
        vec![("supersedes".into(), format!("{supersedes:?}"))],
        vec![("supersedes".into(), format!("{:?}", registry.selection_contract().get("supersedes")))],
    ));
    runs.push(component(
        "order/severity",
        "severity contributes no ordering because its closed vocabulary has one value",
        "the ordering keys the engine applies, against diagnostic_selection.stable_order".into(),
        vec![(
            "stable_order".into(),
            format!("{:?}", canonical.get("stable_order")),
        )],
        vec![(
            "stable_order".into(),
            format!("{:?}", registry.selection_contract().get("stable_order")),
        )],
    ));

    // -- what the engine actually selected --------------------------------
    let tab_then_later = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n\tID: data.a\n    TYPE: STRING\n    VALUE: REF(data.absent)\n";
    runs.push(selected(
        runner,
        "stage/earliest-failing-stage-only",
        "at the first failing stage the later stages are not evaluated",
        tab_then_later,
        MockHost::new(),
        vec![
            ("diagnostics", "[\"error.source.tab\"]".into()),
            ("primary_stage", "Some(\"lexical\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "multiplicity/independent-diagnostics-emitted",
        "every independent diagnostic at the selected stage is emitted; distinct loci are independent",
        UNRESOLVED_PAIR,
        MockHost::new(),
        vec![("diagnostics", "[\"error.reference.unresolved\", \"error.reference.unresolved\"]".into())],
    ));
    runs.push(selected(
        runner,
        "order/locus",
        "canonical source byte offset ascending orders same-stage diagnostics",
        UNRESOLVED_PAIR,
        MockHost::new(),
        vec![(
            "diagnostics",
            "[\"error.reference.unresolved\", \"error.reference.unresolved\"]".into(),
        )],
    ));
    runs.push(selected(
        runner,
        "order/no-discovery-time-influence",
        "the same two independent defects order identically however they were discovered",
        &UNRESOLVED_PAIR
            .replace("data.first", "data.zulu")
            .replace("data.second", "data.alpha"),
        MockHost::new(),
        vec![(
            "diagnostics",
            "[\"error.reference.unresolved\", \"error.reference.unresolved\"]".into(),
        )],
    ));
    let tab_only = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n\tID: data.a\n    TYPE: STRING\n    VALUE: \"x\"\n";
    runs.push(selected(
        runner,
        "supersession/same-cause-only",
        "a supersedes edge suppresses its target only at the same cause and locus",
        tab_only,
        MockHost::new(),
        vec![("diagnostics", "[\"error.source.tab\"]".into())],
    ));
    runs.push(selected(
        runner,
        "supersession/independent-occurrence-kept",
        "an independent occurrence of the superseded identifier at another locus is kept",
        UNRESOLVED_PAIR,
        MockHost::new(),
        vec![(
            "diagnostics",
            "[\"error.reference.unresolved\", \"error.reference.unresolved\"]".into(),
        )],
    ));
    runs.push(selected(
        runner,
        "duplicate/exact-key-suppressed",
        "after supersession one diagnostic is emitted per duplicate_key",
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.case\n    NAME: \"Conformance case\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n    ID: data.a\n    TYPE: STRING\n    VALUE: REF(data.absent)\n",
        MockHost::new(),
        vec![("diagnostics", "[\"error.reference.unresolved\"]".into())],
    ));

    // Execution-stage selection: producers, iterations and retry attempts.
    let two_failing = document(
        "",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.first\n    OPERATION: core.read\n    TARGET: PATH(\"/case/first.txt\")\n\nACTION:\n    ID: action.second\n    OPERATION: core.read\n    TARGET: PATH(\"/case/second.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: [REF(action.first), REF(action.second)]\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    );
    let denied = || MockHost::new().deny("core.read", "conformance: the host grants no access");
    runs.push(selected(
        runner,
        "order/stage",
        "stage_order ascending puts an execution diagnostic before a completion one",
        &two_failing,
        denied(),
        vec![
            ("diagnostics", "[\"error.permission.denied\"]".into()),
            ("primary_stage", "Some(\"execution\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "secondary/unhandled-retained-in-order",
        "every other unhandled diagnostic is secondary and stays in stable order",
        &two_failing,
        denied(),
        vec![
            ("primary", "Some(\"error.permission.denied\")".into()),
            (
                "producers",
                "[\"task.case:status.running\", \"action.first:status.failed\"]".into(),
            ),
        ],
    ));
    let loop_document = document(
        "\nDATA:\n    ID: data.members\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.case\n    FOR EACH item IN REF(data.members):\n        STEP:\n            ID: step.one\n            ACTION:\n                ID: action.subject\n                OPERATION: core.read\n                TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    );
    runs.push(selected(
        runner,
        "order/iteration",
        "iteration index ascending orders diagnostics of one declaration's instances",
        &loop_document,
        denied(),
        vec![(
            "diagnostics",
            "[\"error.permission.denied\", \"error.permission.denied\"]".into(),
        )],
    ));
    let retry = crate::witness_cases::retry_read(None);
    runs.push(selected(
        runner,
        "order/retry-attempt",
        "retry-attempt index ascending orders the diagnostics of one invocation's attempts",
        &retry,
        MockHost::new().script(
            "core.read",
            vec![
                CapabilityOutcome::Unavailable("conformance: scripted limitation 1".into()),
                CapabilityOutcome::Unavailable("conformance: scripted limitation 2".into()),
                CapabilityOutcome::Unavailable("conformance: scripted limitation 3".into()),
            ],
        ),
        vec![
            ("attempt_phases", "[\"0:pre_effect\", \"1:pre_effect\", \"2:pre_effect\"]".into()),
            ("diagnostics", "[\"error.host.constraint\", \"error.host.constraint\", \"error.host.constraint\", \"error.retry.exhausted\"]".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "retry/retried-failure-stays-unhandled",
        "a merely retried failed diagnostic remains unhandled",
        &retry,
        MockHost::new().script(
            "core.read",
            vec![
                CapabilityOutcome::Unavailable("conformance: scripted limitation 1".into()),
                CapabilityOutcome::Unavailable("conformance: scripted limitation 2".into()),
                CapabilityOutcome::Unavailable("conformance: scripted limitation 3".into()),
            ],
        ),
        vec![("primary", "Some(\"error.host.constraint\")".into())],
    ));

    // Recovery, substitution and demand resolution.
    let continued = crate::witness_cases::continue_read(true);
    runs.push(selected(
        runner,
        "primary/none-when-all-recovered",
        "when every applicable diagnostic is recovered, no diagnostic is primary",
        &continued,
        advance_host(1),
        vec![
            ("primary", "None".into()),
            ("terminal", "Some(\"status.succeeded\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "evidence/handled-retained",
        "a handled diagnostic stays in the retained evidence",
        &continued,
        advance_host(1),
        vec![
            ("diagnostics", "[\"error.host.constraint\"]".into()),
            ("primary", "None".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "primary/first-unhandled-after-recovery",
        "after permitted recovery the first remaining unhandled diagnostic is primary",
        &continued.replacen(
            "SUCCESS:\n    ID: success.case\n    ALL: TRUE",
            "SUCCESS:\n    ID: success.case\n    ALL: FALSE",
            1,
        ),
        advance_host(1),
        vec![("primary", "Some(\"error.success.unsatisfied\")".into())],
    ));
    runs.push(selected(
        runner,
        "evidence/successful-substitution-locals-retained",
        "a successful FALLBACK substitution keeps the substituted invocation's diagnostics as local evidence",
        &continued,
        advance_host(1),
        vec![("diagnostics", "[\"error.host.constraint\"]".into()), ("events", "[\"event.host_constraint #0 at node 3 -> handler.continue recovered it\"]".into())],
    ));
    let demand = document(
        "",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.calculate\n    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"1 / 0\"\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    );
    runs.push(selected(
        runner,
        "demand/eligible-value-domain-failure-resolved",
        "an eligible value-domain failure of a statically valid demanded expression resolves to the execution stage and status.failed",
        &demand,
        MockHost::new(),
        vec![
            ("primary", "Some(\"error.numeric.division_by_zero\")".into()),
            ("primary_stage", "Some(\"execution\")".into()),
            ("subject", "status=status.failed phase=pre_effect effects=none errors=[\"error.numeric.division_by_zero\"] binding=not_requested".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "demand/excluded-trigger-keeps-classification",
        "an excluded trigger keeps its registered source-validation classification",
        UNRESOLVED_PAIR,
        MockHost::new(),
        vec![("primary_stage", "Some(\"resolution\")".into())],
    ));
    runs
}

fn failure_lifecycle(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    let canonical = lifecycle(spec);
    let mut runs = Vec::new();
    let resolution = canonical
        .get("error_phase_resolution")
        .expect("the phase resolution rules");
    let defaults = resolution
        .get("defaults_by_stage")
        .expect("the stage defaults");
    let overrides = resolution
        .get("overrides")
        .expect("the identifier overrides");
    let phases = |value: Option<&Json>| {
        value
            .and_then(Json::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Json::as_str)
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    let read = |path: &str| {
        document(
            "",
            &format!("\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.read\n    TARGET: PATH(\"{path}\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n"),
        )
    };
    let phase_of = |runner: &Runner, source: &str, host: MockHost| -> String {
        let mut host = host;
        let observed = runner.run_on(source, &lcl_resolver::MemoryProvider::new(), &mut host);
        observed
            .invocations
            .iter()
            .filter(|i| i.declaration.as_deref() == Some("action.subject"))
            .filter_map(|i| i.result.as_ref())
            .map(|r| r.failure_phase.to_string())
            .next()
            .unwrap_or_else(|| "no producer".to_string())
    };

    // -- phase resolution --------------------------------------------------
    let denied = || MockHost::new().deny("core.read", "conformance: the host grants no access");
    let execution_phases = phases(defaults.get("execution"));
    let observed_phase = phase_of(runner, &read("/case/data.txt"), denied());
    runs.push(component(
        "phase/stage-defaults",
        "each error inherits the allowed phase list of its registered stage",
        format!("error.permission.denied at the execution stage; registry defaults {execution_phases:?}"),
        vec![("phase is one of the stage defaults".into(), "true".into())],
        vec![("phase is one of the stage defaults".into(), execution_phases.contains(&observed_phase).to_string())],
    ));
    let unknown_phases = phases(overrides.get("error.value.unknown"));
    let unknown_source = document(
        "\nDATA:\n    ID: data.undetermined\n    TYPE: INTEGER\n    VALUE: UNKNOWN\n",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(data.undetermined)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    );
    let unknown_phase = phase_of(runner, &unknown_source, MockHost::new());
    runs.push(component(
        "phase/identifier-overrides",
        "an exact identifier override replaces the stage default list",
        format!("error.value.unknown; registry override {unknown_phases:?}"),
        vec![(
            "phase is one of the identifier's allowed phases".into(),
            "true".into(),
        )],
        vec![(
            "phase is one of the identifier's allowed phases".into(),
            (unknown_phases.contains(&unknown_phase) || unknown_phase == "no producer").to_string(),
        )],
    ));
    let partial_host = || {
        let mut observation = Observation::none().with_effect(ObservedEffect {
            class: EffectClass::Process,
            state: RecordState::Partial,
            target: Some("/case/emit".into()),
            evidence: vec!["conformance: the command wrote part of its stream".into()],
        });
        observation.host_limited = true;
        MockHost::new().script(
            "core.execute",
            vec![CapabilityOutcome::Failed {
                detail: "conformance: interrupted".into(),
                observation,
            }],
        )
    };
    let execute = document(
        "",
        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.execute\n    TARGET: \"emit --now\"\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
    );
    runs.push(selected(
        runner,
        "phase/measured-at-exposed-producer",
        "failure_phase is measured at the exposed result producer",
        &execute,
        partial_host(),
        vec![("subject", "status=status.blocked phase=post_effect effects=partial errors=[\"error.host.constraint\"] binding=unbound".into())],
    ));
    let retry = crate::witness_cases::retry_read(None);
    let scripted_failures = |n: usize| {
        MockHost::new().script(
            "core.read",
            (0..n)
                .map(|i| {
                    CapabilityOutcome::Unavailable(format!("conformance: scripted limitation {i}"))
                })
                .collect(),
        )
    };
    runs.push(selected(
        runner,
        "evidence/child-and-attempt-local-phase-retained",
        "every attempt retains its own local phase as ordered evidence",
        &retry,
        scripted_failures(3),
        vec![(
            "attempt_phases",
            "[\"0:pre_effect\", \"1:pre_effect\", \"2:pre_effect\"]".into(),
        )],
    ));
    runs.push(selected(
        runner,
        "phase/dependency-unsatisfied-pre-effect",
        "error.dependency.unsatisfied resolves before the first authorized effect",
        &document(
            "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nDEPENDENCY:\n    ID: dependency.case\n    REFERENCE: REF(data.subject)\n    ASSERT: FALSE\n    REQUIRED: TRUE\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    DEPENDENCY: REF(dependency.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        vec![("diagnostics", "[\"error.dependency.unsatisfied\"]".into()), ("producers", "[]".into())],
    ));
    runs.push(selected(
        runner,
        "phase/execution-order-from-timing",
        "error.execution.order is pre_effect when graph construction detects it",
        &document(
            "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.case\n    MODE: mode.sequential\n    STEP:\n        ID: step.first\n        ACTION:\n            ID: action.first\n            OPERATION: core.return\n            TARGET: REF(data.subject)\n    STEP:\n        ID: step.second\n        BEFORE: [REF(step.first)]\n        ACTION:\n            ID: action.second\n            OPERATION: core.return\n            TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        vec![("diagnostics", "[\"error.execution.order\"]".into()), ("producers", "[]".into())],
    ));
    runs.push(selected(
        runner,
        "phase/required-missing-from-timing",
        "error.required.missing is pre_effect when the absence is known before any producer effect",
        &document(
            "\nOUTPUT:\n    ID: output.pending\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(output.pending)\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    OUTPUT: REF(output.pending)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        vec![("subject", "status=status.blocked phase=pre_effect effects=none errors=[\"error.required.missing\"] binding=not_requested".into())],
    ));
    // -- the mixed-phase identifiers --------------------------------------
    let additional = canonical
        .get("additional_mixed_error_profiles")
        .expect("the mixed profiles");
    let allowed = |id: &str| -> String {
        let stage_phases = phases(
            defaults.get(
                lcl_diagnostics::DiagnosticRegistry::load(spec)
                    .expect("the registry loads")
                    .error(id)
                    .expect("a registered error")
                    .stage
                    .as_registry_str(),
            ),
        );
        let profile = additional
            .get(id)
            .and_then(Json::as_str)
            .unwrap_or_default();
        format!("{stage_phases:?} ({profile})")
    };
    let subject_phase =
        |label: &str, clause: &str, id: &str, source: String, host: MockHost, phase: &str| {
            let mut expected = [
                ("subject_phase", phase.to_string()),
                ("diagnostic", id.to_string()),
            ];
            expected.sort();
            selected_ids(
                runner,
                label,
                clause,
                &source,
                host,
                id,
                phase,
                &allowed(id),
            )
        };
    let read_denied = read("/case/data.txt");
    runs.push(subject_phase(
        "phase/mixed/permission.denied",
        "denial before an effect is pre_effect within the identifier's mixed family",
        "error.permission.denied",
        read_denied.clone(),
        denied(),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/host.constraint",
        "a host limitation detected before an effect is pre_effect within its mixed family",
        "error.host.constraint",
        read_denied.clone(),
        MockHost::new().unavailable("core.read", "conformance: the capability is absent"),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/execution.action",
        "failure to start is pre_effect within the identifier's mixed family",
        "error.execution.action",
        read_denied.clone(),
        MockHost::new().script(
            "core.read",
            vec![MockHost::failed_before_effect(
                "conformance: the read did not start",
            )],
        ),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/operation.precondition",
        "an immediate operation precondition precedes that operation's effects",
        "error.operation.precondition",
        document(
            "",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.create\n    TARGET: PATH(\"/case/new.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/value.unknown",
        "a demanded UNKNOWN required value at a producer is inside the identifier's mixed family",
        "error.value.unknown",
        document(
            "\nDATA:\n    ID: data.members\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.filter\n    TARGET: REF(data.members)\n    PARAMETER:\n        NAME: predicate\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"UNKNOWN\"\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/retry.exhausted",
        "every failed attempt of an exhausted retry keeps its own phase",
        "error.retry.exhausted",
        retry.clone(),
        scripted_failures(3),
        "pre_effect",
    ));
    runs.push(subject_phase(
        "phase/mixed/success.unsatisfied",
        "an unsatisfied SUCCESS after an effect-free run is raised at the root, with no producer failure",
        "error.success.unsatisfied",
        document(
            "\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: FALSE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "none",
    ));
    runs.push(subject_phase(
        "phase/mixed/verification.failed",
        "a required VERIFY that is FALSE after an effect-free run is raised at completion",
        "error.verification.failed",
        document(
            "\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n\nVERIFY:\n    ID: verify.case\n    ASSERT: REF(data.subject) == 9\n    REQUIRED: TRUE\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: [REF(verify.case)]\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "none",
    ));
    runs.push(subject_phase(
        "phase/mixed/cancelled",
        "the invoking authority cancelling execution raises error.cancelled",
        "error.cancelled",
        document(
            "\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.cancel\n    TARGET: REF(task.case)\n    PARAMETER:\n        NAME: reason\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"the owner cancelled it\"\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "none",
    ));
    runs.push(subject_phase(
        "phase/mixed/evidence.missing",
        "required EVIDENCE that cannot resolve raises error.evidence.missing at completion",
        "error.evidence.missing",
        document(
            "\nDATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n\nDATA:\n    ID: data.members\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n\nEVIDENCE:\n    ID: evidence.required\n    TYPE: INTEGER\n    VALUE: REF(data.members)[5]\n    REQUIRED: TRUE\n\nVERIFY:\n    ID: verify.case\n    ASSERT: REF(data.subject) == 3\n    REQUIRED: TRUE\n    EVIDENCE: REF(evidence.required)\n",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.case\n    ALL: [REF(verify.case)]\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new(),
        "none",
    ));
    let mut fabricating = Fabricating;
    runs.push(selected_on(
        runner,
        "phase/mixed/operation.postcondition",
        "a postcondition failure whose effect extent cannot be established is indeterminate",
        &read_denied,
        &mut fabricating,
        vec![
            ("diagnostics", "[\"error.operation.postcondition\"]".to_string()),
            ("subject", "status=status.failed phase=indeterminate effects=indeterminate errors=[\"error.operation.postcondition\"] binding=not_requested".to_string()),
        ],
        "a host claiming an effect class the invocation never resolved",
    ));

    // -- status, independence, indeterminate state and terminal results ----
    runs.push(selected(
        runner,
        "status/primary-default-after-demand-resolution",
        "the primary unhandled diagnostic contributes its resolved default status",
        &demand_source(),
        MockHost::new(),
        vec![("subject", "status=status.failed phase=pre_effect effects=none errors=[\"error.numeric.division_by_zero\"] binding=not_requested".into())],
    ));
    runs.push(selected(
        runner,
        "status/promotion-after-recovery",
        "recovery promotes the first remaining unhandled diagnostic, whose status then controls",
        &crate::witness_cases::continue_read(true).replacen(
            "SUCCESS:\n    ID: success.case\n    ALL: TRUE",
            "SUCCESS:\n    ID: success.case\n    ALL: FALSE",
            1,
        ),
        advance_host(1),
        vec![
            ("primary", "Some(\"error.success.unsatisfied\")".into()),
            ("terminal", "Some(\"status.failed\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "status/handler-result-controls-only-when-all-recovered",
        "only when every applicable diagnostic is recovered may the handler result control status",
        &crate::witness_cases::continue_read(true),
        advance_host(1),
        vec![
            ("primary", "None".into()),
            ("terminal", "Some(\"status.succeeded\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "independence/status-effect-output",
        "status, effect state and OUTPUT binding are recorded independently",
        &execute,
        partial_host(),
        vec![
            ("subject", "status=status.blocked phase=post_effect effects=partial errors=[\"error.host.constraint\"] binding=unbound".into()),
            ("terminal", "Some(\"status.blocked\")".into()),
        ],
    ));
    runs.push(selected(
        runner,
        "evidence/phase-effect-output-exact",
        "the exact phase, effect and OUTPUT evidence is retained for one failure",
        &execute,
        partial_host(),
        vec![("subject", "status=status.blocked phase=post_effect effects=partial errors=[\"error.host.constraint\"] binding=unbound".into())],
    ));
    let mut indeterminate = Indeterminate;
    runs.push(selected_on(
        runner,
        "indeterminate/fail-closed",
        "an effect whose extent cannot be established keeps effect_state indeterminate and binds no OUTPUT",
        &execute,
        &mut indeterminate,
        vec![("subject", "status=status.blocked phase=post_effect effects=indeterminate errors=[\"error.host.constraint\"] binding=unbound".to_string())],
        "a host whose failure leaves the effect extent unknown",
    ));
    let mut safety = RetrySafety { established: false };
    let retry_command = crate::witness_cases::retry_read(None)
        .replace("core.read", "core.execute")
        .replace(
            "TARGET: PATH(\"/case/retry.txt\")",
            "TARGET: PATH(\"/case/emit\")",
        );
    runs.push(selected_on(
        runner,
        "retry/known-effects-require-safety-evidence",
        "after known effects another attempt requires exact safety evidence; missing proof stops it",
        &retry_command,
        &mut safety,
        vec![
            ("diagnostics", "[\"error.host.constraint\", \"error.required.missing\"]".to_string()),
            ("attempts", "[\"0:status.blocked\"]".to_string()),
        ],
        "a host that fails after a known partial effect and establishes no retry safety",
    ));
    let mut safety_again = RetrySafety { established: false };
    runs.push(selected_on(
        runner,
        "retry/safety-blocked-attempt-is-not-exhaustion",
        "a safety-blocked unmade attempt is not retry exhaustion",
        &retry_command,
        &mut safety_again,
        vec![(
            "diagnostics",
            "[\"error.host.constraint\", \"error.required.missing\"]".to_string(),
        )],
        "a host that fails after a known partial effect and establishes no retry safety",
    ));
    runs.push(selected(
        runner,
        "terminal/blocked-result-terminal",
        "status.blocked is terminal for that invocation and has no outgoing transition",
        &read_denied,
        MockHost::new().unavailable("core.read", "conformance: the capability is absent"),
        vec![("subject", "status=status.blocked phase=pre_effect effects=none errors=[\"error.host.constraint\"] binding=not_requested".into()), ("terminal", "Some(\"status.blocked\")".into())],
    ));
    runs.push(selected(
        runner,
        "terminal/later-invocation-distinct",
        "later availability permits a separate explicit invocation with its own identity and result",
        &document(
            "",
            "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.first\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n    REQUIRED: FALSE\n\nACTION:\n    ID: action.second\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: [REF(action.first), REF(action.second)]\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        ),
        MockHost::new().script(
            "core.read",
            vec![
                CapabilityOutcome::Unavailable("conformance: the capability is absent for the first invocation".into()),
                CapabilityOutcome::Completed(
                    Observation::none()
                        .with("value", lcl_runtime::Value::Text("complete".into()))
                        .with("evidence", lcl_runtime::Value::List(Vec::new())),
                ),
            ],
        ),
        vec![("producers", "[\"task.case:status.running\", \"action.first:status.blocked\", \"action.second:status.succeeded\"]".into())],
    ));
    runs.push(selected(
        runner,
        "aggregate/child-failure-resolved-through-handler",
        "an active aggregate may resolve a failed child attempt through a selected handler",
        &crate::witness_cases::continue_read(true),
        advance_host(1),
        vec![
            ("terminal", "Some(\"status.succeeded\")".into()),
            ("primary", "None".into()),
        ],
    ));
    runs
}

/// One mixed-phase identifier: the diagnostic is raised and the phase the
/// exposed producer recorded lies inside the identifier's allowed family.
#[allow(clippy::too_many_arguments)]
fn selected_ids(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    host: MockHost,
    id: &str,
    phase: &str,
    allowed: &str,
) -> ExecutedCase {
    let mut host = host;
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, &mut host);
    let raised = case.observed.diagnostics.iter().any(|d| d == id);
    let recorded = case
        .observed
        .invocations
        .iter()
        .filter(|i| {
            i.declaration.as_deref() == Some("action.subject")
                || i.declaration.as_deref() == Some("action.read")
        })
        .filter_map(|i| i.result.as_ref())
        .map(|r| r.failure_phase.to_string())
        .next()
        .unwrap_or_else(|| "none".to_string());
    case.expectation = Expectation::Component(vec![
        ("raised".into(), "true".into()),
        ("producer phase".into(), phase.into()),
    ]);
    case.observed.component = vec![
        ("raised".into(), raised.to_string()),
        ("producer phase".into(), recorded),
    ];
    case.observed.input_evidence.push(format!(
        "{id}: allowed phases {allowed}; host requests {:?}",
        host.requests()
    ));
    case.verdict = judge(&case.expectation, &case.observed);
    case
}

/// Both contract rows, as grouped records.
pub fn execute(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    vec![
        group(
            "semantic/diagnostic_policy/core.error_selection",
            "complete error metadata, severity, stage order, supersession, duplicate suppression, stable order, primary and secondary selection, retained evidence and demand resolution",
            diagnostic_policy(spec, runner),
        ),
        group(
            "semantic/failure_lifecycle/core.failure_lifecycle",
            "resolved failure phases, status control, independent effect and OUTPUT records, indeterminate fail-closed, retry safety, terminal results and aggregate recovery",
            failure_lifecycle(spec, runner),
        ),
    ]
}

/// A host whose every attempt fails after a known partial effect, answering
/// the retry-safety query exactly as scripted.
struct RetrySafety {
    established: bool,
}

impl lcl_runtime::Host for RetrySafety {
    fn permits(&mut self, _request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        lcl_runtime::Permission::Granted
    }
    fn invoke(&mut self, _request: &lcl_runtime::CapabilityRequest) -> CapabilityOutcome {
        let mut observation = Observation::none().with_effect(ObservedEffect {
            class: EffectClass::Process,
            state: RecordState::Partial,
            target: Some("/case/emit".into()),
            evidence: vec!["conformance: the command wrote part of its stream".into()],
        });
        observation.host_limited = true;
        CapabilityOutcome::Failed {
            detail: "conformance: interrupted after a known effect".into(),
            observation,
        }
    }
    fn retry_evidence(
        &mut self,
        _context: &lcl_runtime::capability::RetryContext,
    ) -> lcl_runtime::capability::RetryEvidence {
        let _ = self.established;
        lcl_runtime::capability::RetryEvidence::Missing
    }
}

/// A host that completes claiming an effect class the invocation never
/// resolved, so the effect extent cannot be established.
struct Fabricating;

impl lcl_runtime::Host for Fabricating {
    fn permits(&mut self, _request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        lcl_runtime::Permission::Granted
    }
    fn invoke(&mut self, _request: &lcl_runtime::CapabilityRequest) -> CapabilityOutcome {
        CapabilityOutcome::Completed(Observation::none().with_effect(ObservedEffect {
            class: EffectClass::Package,
            state: RecordState::Applied,
            target: None,
            evidence: Vec::new(),
        }))
    }
}

/// A host whose failure leaves an indeterminate effect extent.
struct Indeterminate;

impl lcl_runtime::Host for Indeterminate {
    fn permits(&mut self, _request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        lcl_runtime::Permission::Granted
    }
    fn invoke(&mut self, _request: &lcl_runtime::CapabilityRequest) -> CapabilityOutcome {
        let mut observation = Observation::none().with_effect(ObservedEffect {
            class: EffectClass::Process,
            state: RecordState::Indeterminate,
            target: Some("/case/emit".into()),
            evidence: vec!["conformance: the command stopped and its extent is unknown".into()],
        });
        observation.host_limited = true;
        CapabilityOutcome::Failed {
            detail: "conformance: the extent cannot be established".into(),
            observation,
        }
    }
}

/// Run one source against a host that is not a `MockHost`, comparing the same
/// renderings `selected` compares.
fn selected_on(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    host: &mut dyn lcl_runtime::Host,
    expected: Vec<(&str, String)>,
    fixture: &str,
) -> ExecutedCase {
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, host);
    let observed = &case.observed;
    let actual: Vec<(String, String)> = expected
        .iter()
        .map(|(key, _)| {
            let value = match *key {
                "diagnostics" => format!("{:?}", observed.diagnostics),
                "primary" => format!("{:?}", observed.primary),
                "terminal" => format!("{:?}", observed.terminal_status),
                "outputs" => format!("{:?}", observed.outputs),
                "attempts" => format!(
                    "{:?}",
                    observed
                        .invocations
                        .iter()
                        .filter(|i| i.declaration.as_deref() == Some("action.read")
                            || i.declaration.as_deref() == Some("action.subject"))
                        .map(|i| format!("{}:{}", i.id.attempt, i.status()))
                        .collect::<Vec<_>>()
                ),
                "subject" => observed
                    .invocations
                    .iter()
                    .filter(|i| {
                        i.declaration.as_deref() == Some("action.subject")
                            || i.declaration.as_deref() == Some("action.read")
                    })
                    .filter_map(|i| i.result.as_ref())
                    .map(|r| {
                        format!(
                            "status={} phase={} effects={} errors={:?} binding={}",
                            r.status,
                            r.failure_phase,
                            r.effect_state,
                            r.execution_errors,
                            r.output_binding
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" | "),
                other => panic!("unknown observation key {other}"),
            };
            (key.to_string(), value)
        })
        .collect();
    case.expectation = Expectation::Component(
        expected
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    );
    case.observed.component = actual;
    case.observed
        .input_evidence
        .push(format!("host: {fixture}"));
    case.verdict = judge(&case.expectation, &case.observed);
    case
}
