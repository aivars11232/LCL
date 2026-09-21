//! The requirement-clause sub-runs of the operation rows.
//!
//! The parent module derives the runs every row shares from registry facts:
//! binding, defaults, bounds, determinism and profile selection. The catalog
//! requirement of each `operation_binding`, `operation_errors` and
//! `operation_effects` entry also names clauses no registry column lists — "the
//! destination is absent unless overwrite is TRUE", "exercise every registered
//! failure path" — and the reviewed mapping pins one sub-run per clause. This
//! module executes those clauses, each with its exact input, host and
//! canonical expectation.
//!
//! Hosts are declared per run. The shipped adapters answer where they perform
//! the row; `lcl-runtime`'s deterministic `MockHost` supplies refusals,
//! limitations, failures and scripted observations the adapters cannot be made
//! to produce on demand; and the D3 fixture profiles (LCL-CLOSE-02 decision D3)
//! install conformance-only profile roles for the six rows no shipped adapter
//! performs. Every observation still comes from the real engine.

use super::{attempt, document, with_action, Row};
use crate::{ExecutedCase, Expectation, Runner};
use lcl_capabilities::{AddressClass, Dependency, Determinism, Effect, Profile, Role, TargetClass};
use lcl_runtime::{
    CapabilityOutcome, EffectClass, MockHost, Observation, ObservedEffect, RecordState,
};
use lcl_spec::SpecPackage;

/// The rows no shipped adapter performs (LCL-CLOSE-02 decision D3).
pub(super) const FIXTURE_ROWS: [&str; 6] = [
    "core.analyze",
    "core.convert",
    "core.generate",
    "core.install",
    "core.report",
    "core.uninstall",
];

/// The two rows whose storage profile decides how the write is performed.
pub(super) const STORE_ROWS: [&str; 2] = ["core.memory_write", "core.state_update"];

/// A conformance-only *externally backed* storage profile for each store row.
///
/// `operations_v0.1.0.json` gives `core.memory_write` and `core.state_update`
/// `possible_dependencies: ["host"]`, and `axis_contract.implementation_profile`
/// lets a profile's axes "narrow the row's but never widen" them. The shipped
/// profile narrows the dependency away and says so — "Write the declared
/// engine-owned store in place within one invocation; the invocation selects no
/// external dependency" — so its store cannot be limited, cannot fail and
/// cannot leave its postcondition unsatisfied. The profile here is the other
/// store the row's own maxima describe: one whose persistence is a host's, with
/// exactly the row's single effect class and its registered `host` dependency.
///
/// It is conformance-only in the same sense as the D3 fixture profiles: nothing
/// ships it, and it fabricates no observation. What it changes is which of the
/// row's two registered kinds of storage the engine is asked to perform.
pub(super) fn external_store_profiles() -> Vec<Profile> {
    [
        ("core.memory_write", AddressClass::Memory, Effect::Memory),
        ("core.state_update", AddressClass::State, Effect::State),
    ]
    .into_iter()
    .map(|(operation, class, effect)| {
        Profile::builder(
            operation,
            Role::new("storage"),
            "conformance.external_store",
            "1",
        )
        .serving(TargetClass::Only(vec![class]))
        .determinism(
            Determinism::Deterministic,
            "conformance fixture: the request, the store snapshot and one immutable \
             implementation version fix the post-state the profile persists",
        )
        .axes(lcl_capabilities::profile::axes(
            &[Dependency::Host],
            &[effect],
        ))
        .resolving(
            "Persist the declared store through the host that backs it; the invocation \
             selects the row's host dependency and exactly the row's one effect class.",
        )
    })
    .collect()
}

/// The engines one row's clauses run on.
pub(super) struct Runners<'a> {
    /// Every profile the shipped adapters declare.
    pub shipped: &'a Runner,
    /// The shipped profiles plus the D3 fixture profiles.
    pub fixture: Runner,
    /// The shipped profiles with the two storage roles served by an externally
    /// backed profile instead of the in-place one. See [`external_store_profiles`].
    pub store: Runner,
    pub spec: &'a SpecPackage,
}

impl Runners<'_> {
    /// The engine whose installed profiles let this row reach its host.
    fn reaching(&self, operation: &str) -> &Runner {
        if FIXTURE_ROWS.contains(&operation) {
            &self.fixture
        } else {
            self.shipped
        }
    }
}

/// The conformance-only profiles of the D3 fixture capabilities.
///
/// Each serves exactly one row the shipped adapters do not perform, declares
/// the concrete axes that row's own invocation rule resolves, and claims no
/// class outside the row's maximum.
pub(super) fn fixture_profiles() -> Vec<Profile> {
    let fixture = |operation: &str,
                   role: &str,
                   category: Determinism,
                   deps: &[Dependency],
                   effects: &[Effect]| {
        Profile::builder(operation, Role::new(role), "conformance.fixture", "1")
            .serving(TargetClass::Any)
            .determinism(
                category,
                "conformance fixture: a fixed scripted observation for the exact request",
            )
            .axes(lcl_capabilities::profile::axes(deps, effects))
            .resolving("the row's own invocation rule over the fixture's address classes")
    };
    use Determinism::{Deterministic, Nondeterministic};
    vec![
        fixture(
            "core.analyze",
            "analysis",
            Nondeterministic,
            &[Dependency::Host, Dependency::Model],
            &[],
        ),
        fixture(
            "core.report",
            "reporting",
            Nondeterministic,
            &[Dependency::Model],
            &[],
        ),
        fixture(
            "core.generate",
            "generation",
            Nondeterministic,
            &[Dependency::Host, Dependency::Model],
            &[Effect::Filesystem],
        ),
        fixture(
            "core.convert",
            "conversion",
            Deterministic,
            &[Dependency::Host],
            &[Effect::Filesystem],
        ),
        fixture(
            "core.install",
            "package",
            Deterministic,
            &[Dependency::Host],
            &[Effect::Package, Effect::Process],
        ),
        fixture(
            "core.uninstall",
            "package",
            Deterministic,
            &[Dependency::Host],
            &[Effect::Package, Effect::Process],
        ),
    ]
}

/// The registered default status of one error.
fn default_status(spec: &SpecPackage, error: &str) -> String {
    lcl_diagnostics::DiagnosticRegistry::load(spec)
        .expect("the diagnostic registry loads")
        .error(error)
        .unwrap_or_else(|| panic!("{error} is registered"))
        .default_status
        .clone()
}

/// The status one error resolves to at a producer: the registered default,
/// or the `expression_demand_resolution` status where that closed map makes
/// the identifier eligible after preflight.
fn resolved_status(spec: &SpecPackage, error: &str) -> String {
    let demand = spec
        .registry("statuses_and_errors")
        .and_then(|registry| registry.get("diagnostic_selection"))
        .and_then(|selection| selection.get("expression_demand_resolution"));
    let eligible = demand
        .and_then(|demand| demand.get("eligible_errors"))
        .and_then(lcl_spec::json::Json::as_object)
        .is_some_and(|errors| errors.iter().any(|(id, _)| id == error));
    if !eligible {
        return default_status(spec, error);
    }
    demand
        .and_then(|demand| demand.get("default_status_overrides"))
        .and_then(|overrides| overrides.get(error))
        .or_else(|| demand.and_then(|demand| demand.get("default_status")))
        .and_then(lcl_spec::json::Json::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The subject invocation failed with exactly this error, phase and effect
/// state, and its one attempt took the error's registered default status.
fn failed_with(spec: &SpecPackage, error: &str, phase: &str, effect_state: &str) -> Expectation {
    Expectation::All(vec![
        Expectation::Diagnostic(error.into()),
        Expectation::Attempts {
            declaration: "action.subject".into(),
            statuses: vec![resolved_status(spec, error)],
        },
        attempt("failure_phase", phase.into()),
        attempt("effect_state", effect_state.into()),
    ])
}

/// Run one source against a host, retaining the host's exact requests.
fn on_mock(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    expectation: Expectation,
    mut host: MockHost,
    fixture: &str,
) -> ExecutedCase {
    let mut case = runner.execute_on(label, clause, source, expectation, &mut host);
    case.observed
        .input_evidence
        .push(format!("host: lcl-runtime MockHost; {fixture}"));
    case.observed
        .input_evidence
        .push(format!("actual host requests: {:?}", host.requests()));
    case
}

/// The subject action with its TARGET replaced.
fn retargeted(row: &Row, target: &str) -> String {
    row.action
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("TARGET:") {
                format!("    TARGET: {target}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The effect class a scripted host reports outside what the invocation
/// resolved.
fn unresolved_class(operation: &str) -> EffectClass {
    if matches!(operation, "core.install" | "core.uninstall") {
        EffectClass::Message
    } else {
        EffectClass::Package
    }
}

/// The rows whose fixture invocation never crosses the capability boundary.
/// Their LCL authorization is refused by a matching `FORBID`.
const INTERNAL_ROWS: [&str; 5] = [
    "core.memory_write",
    "core.state_update",
    "core.stop",
    "core.validate",
    "core.verify",
];

/// `error/permission.denied`: "Required access or an effect is unauthorized or
/// prohibited."
fn permission_denied(runners: &Runners<'_>, row: &Row) -> ExecutedCase {
    let spec = runners.spec;
    if INTERNAL_ROWS.contains(&row.operation) {
        // 05_SEMANTICS/03: "FORBID blocks matching action even when an ACTION
        // requires it unless a valid OVERRIDE resolves the exact conflict."
        let target = row
            .action
            .lines()
            .find_map(|line| line.trim_start().strip_prefix("TARGET: "))
            .expect("the fixture declares a TARGET");
        let source = document(row).replacen(
            "\nACTION:\n    ID: action.subject",
            &format!(
                "\nFORBID:\n    ID: forbid.subject\n    OPERATION: {}\n    TARGET: {target}\n\
                 \nACTION:\n    ID: action.subject",
                row.operation
            ),
            1,
        );
        return runners.shipped.execute(
            "error/permission.denied",
            "05_SEMANTICS/03: FORBID blocks the matching required ACTION before any invocation",
            &source,
            Expectation::All(vec![
                Expectation::Rejects("error.permission.denied".into()),
                Expectation::Attempts {
                    declaration: "action.subject".into(),
                    statuses: Vec::new(),
                },
            ]),
        );
    }
    on_mock(
        runners.reaching(row.operation),
        "error/permission.denied",
        "the host refuses the resolved request: error.permission.denied before effects",
        &document(row),
        failed_with(spec, "error.permission.denied", "pre_effect", "none"),
        MockHost::new().deny(row.operation, "conformance: the host grants no access"),
        &format!("deny {}", row.operation),
    )
}

/// The subject action a host-reaching generic run invokes.
///
/// `core.stop` names an internal execution unit in the shared fixture, which
/// the runtime transitions itself; its process form reaches the host.
fn reaching_source(row: &Row) -> String {
    if row.operation == "core.stop" {
        with_action(row, &retargeted(row, "REF(data.command)"))
    } else {
        document(row)
    }
}

/// `error/host.constraint`: "A host/provider limitation outside portable LCL
/// prevents execution."
fn host_constraint(runners: &Runners<'_>, runner: &Runner, row: &Row) -> ExecutedCase {
    on_mock(
        runner,
        "error/host.constraint",
        "09: host limitations produce error.host.constraint and never change LCL meaning",
        &reaching_source(row),
        failed_with(runners.spec, "error.host.constraint", "pre_effect", "none"),
        MockHost::new().unavailable(row.operation, "conformance: the capability is absent"),
        &format!("unavailable {}", row.operation),
    )
}

/// `error/execution.action`: "failure to start is pre_effect".
fn execution_action(runners: &Runners<'_>, runner: &Runner, row: &Row) -> ExecutedCase {
    on_mock(
        runner,
        "error/execution.action",
        "failure_lifecycle: a host failure proven before any effect is error.execution.action, pre_effect",
        &reaching_source(row),
        failed_with(runners.spec, "error.execution.action", "pre_effect", "none"),
        MockHost::new().script(
            row.operation,
            vec![MockHost::failed_before_effect("conformance: the action did not start")],
        ),
        &format!("{} fails before any effect", row.operation),
    )
}

/// `error/operation.postcondition`: the host claims an effect the invocation
/// never resolved, so the completed operation cannot satisfy its contract and
/// the effect extent is not established.
fn postcondition(runners: &Runners<'_>, runner: &Runner, row: &Row) -> ExecutedCase {
    let class = unresolved_class(row.operation);
    on_mock(
        runner,
        "error/operation.postcondition",
        "failure_lifecycle: a postcondition fails when the effect extent cannot be established",
        &reaching_source(row),
        failed_with(
            runners.spec,
            "error.operation.postcondition",
            "indeterminate",
            "indeterminate",
        ),
        MockHost::new().script(
            row.operation,
            vec![CapabilityOutcome::Completed(
                Observation::none().with_effect(ObservedEffect {
                    class,
                    state: RecordState::Applied,
                    target: None,
                    evidence: Vec::new(),
                }),
            )],
        ),
        &format!(
            "{} completes claiming an unresolved {class:?} effect",
            row.operation
        ),
    )
}

/// `positional-earliest-stage` for a row whose fixture names no parameter:
/// NAME is required by the field registry, so an unnamed PARAMETER stops at
/// grammar before operation binding.
fn positional(runners: &Runners<'_>, row: &Row) -> ExecutedCase {
    let action = format!(
        "{}\n    PARAMETER:\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"x\"",
        row.action
    );
    runners.shipped.execute(
        "positional-earliest-stage",
        "10_CORE_OPERATION_PARAMETER_RULES: no positional parameters exist; NAME is required",
        &with_action(row, &action),
        Expectation::Rejects("error.field.required".into()),
    )
}

/// `forbidden-state-target`: "MEMORY and STATE targets are prohibited; use
/// core.memory_write or core.state_update."
fn forbidden_state_target(runners: &Runners<'_>, row: &Row) -> ExecutedCase {
    runners.shipped.execute(
        "forbidden-state-target",
        "10_CORE_OPERATION_PARAMETER_RULES: a STATE target is prohibited before effects",
        &with_action(row, &retargeted(row, "REF(state.revision)")),
        failed_with(
            runners.spec,
            "error.operation.precondition",
            "pre_effect",
            "none",
        ),
    )
}

/// `unresolved-target` for a row whose TARGET is optional and unused by its
/// fixture.
fn unresolved_optional_target(runners: &Runners<'_>, row: &Row) -> ExecutedCase {
    let action = row.action.replacen(
        &format!("OPERATION: {}", row.operation),
        &format!("OPERATION: {}\n    TARGET: REF(data.absent)", row.operation),
        1,
    );
    runners.shipped.execute(
        "unresolved-target",
        "an operation field whose REFERENCE does not resolve exactly once is error.reference.unresolved",
        &with_action(row, &action),
        Expectation::Rejects("error.reference.unresolved".into()),
    )
}

/// One profile scenario for one required role: `(label suffix, profiles)`.
fn profile_scenarios(
    base: &[Profile],
    operation: &str,
    role: &str,
    outside: Effect,
) -> Vec<(&'static str, Vec<Profile>)> {
    let is_role = |p: &Profile| p.operation_id == operation && p.profile_role.as_str() == role;
    let selected = base
        .iter()
        .find(|p| is_role(p))
        .unwrap_or_else(|| panic!("{operation} installs a {role} profile"))
        .clone();
    let without: Vec<Profile> = base.iter().filter(|p| !is_role(p)).cloned().collect();
    let with = |profile: Profile| {
        let mut all = without.clone();
        all.push(profile);
        all
    };
    let mut second = selected.clone();
    second.implementation_id = "conformance.fixture.second".into();
    let mut incomplete = selected.clone();
    incomplete.implementation_id.clear();
    let mut ambiguous = without.clone();
    ambiguous.push(selected.clone());
    ambiguous.push(second);
    vec![
        ("missing", without.clone()),
        ("incomplete", with(incomplete)),
        ("ambiguous", ambiguous),
        (
            "out-of-bounds",
            with(
                selected
                    .clone()
                    .axes(selected.axes.clone().with_effect(outside)),
            ),
        ),
    ]
}

/// `precondition/<prefix>profile-<scenario>`: "A missing, ambiguous,
/// incomplete, or out-of-bounds required profile role emits
/// error.operation.precondition before effects."
fn profile_preconditions(
    runners: &Runners<'_>,
    row: &Row,
    role: &str,
    prefix: &str,
    outside: Effect,
) -> Vec<ExecutedCase> {
    let base = if FIXTURE_ROWS.contains(&row.operation) {
        Runner::shipped_profiles()
            .into_iter()
            .chain(fixture_profiles())
            .collect::<Vec<_>>()
    } else {
        Runner::shipped_profiles()
    };
    profile_scenarios(&base, row.operation, role, outside)
        .into_iter()
        .map(|(scenario, profiles)| {
            let runner = Runner::with_profiles(runners.spec, profiles.clone())
                .expect("the engine assembles with the scenario catalog");
            on_mock(
                &runner,
                &format!("precondition/{prefix}profile-{scenario}"),
                "axis_contract.implementation_profile.failure: error.operation.precondition before effects",
                &document(row),
                failed_with(runners.spec, "error.operation.precondition", "pre_effect", "none"),
                MockHost::new(),
                &format!(
                    "installed {role} profiles: {:?}",
                    profiles
                        .iter()
                        .filter(|p| p.operation_id == row.operation && p.profile_role.as_str() == role)
                        .collect::<Vec<_>>()
                ),
            )
        })
        .collect()
}

/// The shipped profiles without every profile of one row and role.
fn shipped_without(operation: &str, role: &str) -> Vec<Profile> {
    Runner::shipped_profiles()
        .into_iter()
        .filter(|p| !(p.operation_id == operation && p.profile_role.as_str() == role))
        .collect()
}

/// `error/operation.precondition` through a required profile role the
/// installed catalog cannot supply.
fn missing_role(runners: &Runners<'_>, row: &Row, role: &str, source: String) -> ExecutedCase {
    let runner = Runner::with_profiles(runners.spec, shipped_without(row.operation, role))
        .expect("the engine assembles without the role");
    on_mock(
        &runner,
        "error/operation.precondition",
        "a required profile role that resolves no profile is error.operation.precondition before effects",
        &source,
        failed_with(runners.spec, "error.operation.precondition", "pre_effect", "none"),
        MockHost::new(),
        &format!("no {role} profile is installed for {}", row.operation),
    )
}

/// Every clause run of one row's `operation_errors` group that the parent
/// module does not derive.
/// A refusal that must precede the boundary: the exact diagnostic, no attempt,
/// and no request of any kind recorded by the host.
///
/// The request list is part of the compared evidence, not only of the input
/// record, so a run that refused *after* asking would fail here.
fn refused_before_request(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    diagnostic: &str,
) -> ExecutedCase {
    let mut host = MockHost::new();
    let mut observed = runner.run_on(source, &lcl_resolver::MemoryProvider::new(), &mut host);
    let requests: Vec<String> = host
        .requests()
        .iter()
        .map(|r| r.operation.clone())
        .collect();
    observed.component = vec![("requests".to_string(), format!("{requests:?}"))];
    observed.input_evidence.push(
        "host: lcl-runtime MockHost; it answers nothing, and records whatever it is asked"
            .to_string(),
    );
    let expectation = Expectation::All(vec![
        Expectation::Rejects(diagnostic.into()),
        Expectation::Attempts {
            declaration: "action.subject".into(),
            statuses: Vec::new(),
        },
        Expectation::Component(vec![("requests".to_string(), "[]".to_string())]),
    ]);
    let verdict = crate::judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: clause.into(),
        source: source.to_string(),
        expectation,
        observed,
        verdict,
    }
}

/// The three analytical rows observe an addressable operand through the host
/// that owns it, and their registered access failures are that host's answers.
///
/// `operations_v0.1.0.json`: `core.compare` "Resolve[s] host or network
/// independently for the target and against operands"; `core.validate`
/// "Resolve[s] host or network only for addressable targets"; `core.verify`
/// "Resolve[s] observation dependencies ... from the target and evidence
/// sources". Each run keeps the row's own fixture and only moves its target to
/// the declared PATH, so what changes is the operand's address class and
/// nothing else.
fn observed_operand(row: &Row) -> String {
    with_action(row, &retargeted(row, "REF(data.path)"))
}

fn observation_failures(runners: &Runners<'_>, row: &Row) -> Vec<ExecutedCase> {
    let spec = runners.spec;
    let shipped = runners.shipped;
    let source = observed_operand(row);
    let (unauthorized, limited) = match row.operation {
        "core.compare" => ("path/unauthorized-access", "path/host-constraint"),
        _ => ("error/permission.denied", "error/host.constraint"),
    };
    let mut runs = vec![on_mock(
        shipped,
        limited,
        "a host limitation prevents the observation of an addressable operand: error.host.constraint",
        &source,
        failed_with(spec, "error.host.constraint", "pre_effect", "none"),
        MockHost::new().unavailable(row.operation, "conformance: the capability is absent"),
        &format!("unavailable {}", row.operation),
    )];
    if row.operation == "core.compare" {
        runs.push(on_mock(
            shipped,
            unauthorized,
            "the host refuses access to an addressable operand: error.permission.denied before any comparison",
            &source,
            failed_with(spec, "error.permission.denied", "pre_effect", "none"),
            MockHost::new().deny(row.operation, "conformance: the host grants no access"),
            &format!("deny {}", row.operation),
        ));
        // "both operands are accessible": the implementation that observes the
        // operand is the one that discovers it cannot, and refuses with the
        // identifier the row's own `errors` list admits.
        runs.push(on_mock(
            shipped,
            "precondition/inaccessible-operand",
            "an operand the observing implementation cannot access fails the row's accessibility precondition before any comparison",
            &source,
            failed_with(spec, "error.operation.precondition", "pre_effect", "none"),
            MockHost::new().script(
                row.operation,
                vec![CapabilityOutcome::Refused {
                    error: lcl_runtime::RuntimeError::OperationPrecondition,
                    cause: "operand".to_string(),
                    detail: "conformance: the operand at that address cannot be accessed"
                        .to_string(),
                    observation: Observation::none(),
                }],
            ),
            "the observing implementation refuses: the operand is not accessible",
        ));
    }
    runs
}

/// A custom operation whose declared dependency admits permitted variation
/// while it asserts `DETERMINISTIC TRUE`.
pub(super) const VARYING_OPERATION: &str = concat!(
    "\nDEFINE:\n    ID: op.inferred\n    KIND: kind.operation\n    ",
    "MEANING: \"Infer a summary of the subject.\"\n    SIDE_EFFECT: FALSE\n    ",
    "DETERMINISTIC: TRUE\n    DEPENDENCY: [model]\n    PARAMETER:\n        NAME: subject\n        ",
    "TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: STRING\n",
);

/// Run one source and report exactly what the subject attempt produced: its
/// status, its result's field names, and the effect classes it recorded.
///
/// The three are compared as one component, because "graph mode never
/// synthesizes started, completed, exit_code, stdout, or stderr" is an
/// assertion about which fields are *absent*, which a field-by-field
/// expectation cannot make.
fn subject_observation(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    mut host: MockHost,
    expected: &[(&str, &str)],
) -> ExecutedCase {
    let observed_run = runner.run_on(source, &lcl_resolver::MemoryProvider::new(), &mut host);
    let subject = observed_run
        .invocations
        .iter()
        .find(|record| record.declaration.as_deref() == Some("action.subject"));
    let result = subject.and_then(|record| record.result.as_ref());
    let fields = result
        .map(|r| {
            let mut names: Vec<&str> = r.fields.keys().map(String::as_str).collect();
            names.sort_unstable();
            format!("[{}]", names.join(", "))
        })
        .unwrap_or_else(|| "[]".to_string());
    let effects = result
        .map(|r| {
            let mut classes: Vec<&str> = r
                .observed_effects
                .iter()
                .map(|effect| effect.class.as_registry_str())
                .collect();
            classes.sort_unstable();
            classes.dedup();
            format!("[{}]", classes.join(", "))
        })
        .unwrap_or_else(|| "[]".to_string());
    let status = subject
        .map(|record| record.status().to_string())
        .unwrap_or_else(|| "none".to_string());
    // Which declarations the graph itself invoked, as its own evidence.
    let graphed = {
        let mut ran: Vec<String> = observed_run
            .invocations
            .iter()
            .filter(|record| !record.id.iteration.is_root())
            .filter_map(|record| record.declaration.clone())
            .collect();
        ran.sort();
        ran.dedup();
        format!("[{}]", ran.join(", "))
    };
    let actual: Vec<(String, String)> = vec![
        ("status".to_string(), status),
        ("fields".to_string(), fields),
        ("effects".to_string(), effects),
        ("graph invoked".to_string(), graphed),
    ];
    let expectation = Expectation::Component(
        expected
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    );
    let mut observed = observed_run;
    observed.component = actual;
    observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    let verdict = crate::judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: clause.into(),
        source: source.to_string(),
        expectation,
        observed,
        verdict,
    }
}

// ---------------------------------------------------------------------------
// Graph mode: core.execute and core.test over a referenced execution unit
// ---------------------------------------------------------------------------

/// The subject action, retargeted at another declaration the document carries.
///
/// `operations_v0.1.0.json`: "non_graph applies exactly to PATH, URI, or STRING
/// targets and graph applies exactly to REFERENCE[TASK|PHASE|SEQUENCE|ACTION|
/// TEST] targets."
fn graph_subject(row: &Row, target: &str, extra: &str) -> String {
    with_action(
        row,
        &format!(
            "OPERATION: {}\n    TARGET: REF({target}){extra}",
            row.operation
        ),
    )
}

/// The subject action executing itself, which is the prohibited cycle.
fn graph_cycle(row: &Row) -> String {
    graph_subject(row, "action.subject", graph_comparison(row))
}

/// The comparison form `core.test` must still supply beside a graph TARGET.
///
/// "A TASK or ACTION TARGET is executed before the supplied comparison and
/// TARGET alone is not a complete test."
fn graph_comparison(row: &Row) -> &'static str {
    if row.operation == "core.test" {
        concat!(
            "\n    PARAMETER:\n        NAME: expected\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 3",
            "\n    PARAMETER:\n        NAME: actual\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: REF(data.number)"
        )
    } else {
        ""
    }
}

/// A SEQUENCE of two returning actions, as a graph target that exposes two
/// material primary results.
const TWO_PRIMARIES: &str = concat!(
    "\nACTION:\n    ID: action.first\n    OPERATION: core.return\n    TARGET: REF(data.number)\n",
    "\nACTION:\n    ID: action.second\n    OPERATION: core.return\n    TARGET: REF(data.text)\n",
    "\nSEQUENCE:\n    ID: sequence.pair\n    MODE: mode.sequential\n    STEP:\n        ID: step.first\n        ACTION: REF(action.first)\n    STEP:\n        ID: step.second\n        ACTION: REF(action.second)\n",
);

/// One action that writes the engine's own MEMORY store, as a graph target
/// with exactly one concrete effect and no host of its own.
const WRITING_GRAPH: &str = concat!(
    "\nACTION:\n    ID: action.writer\n    OPERATION: core.memory_write\n    TARGET: REF(memory.notes)\n    ",
    "PARAMETER:\n        NAME: value\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"graph\"\n",
);

/// One action whose write crosses to a host, as a graph target whose failure
/// is the host's.
const CROSSING_GRAPH: &str = concat!(
    "\nACTION:\n    ID: action.writer\n    OPERATION: core.write\n    TARGET: REF(data.path)\n    ",
    "PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"graph\"\n",
);

/// `with_declarations`, plus membership: a graph target is a declaration the
/// executing document already carries, so the fixture names it in the task.
///
/// "Reachability does not expand SCOPE or graph membership": a reference from
/// `core.execute` or `core.test` does not make the unit a member, so the
/// fixture that means to execute one declares it as one.
fn with_member(row: &Row, action: &str, declarations: &str, member: &str) -> String {
    with_declarations(row, action, declarations).replacen(
        "ACTION: [REF(action.subject), REF(action.other)]",
        &format!("ACTION: [REF(action.subject), REF(action.other), REF({member})]"),
        1,
    )
}

/// The same, for a `SEQUENCE` the task runs beside its actions.
fn with_sequence_member(row: &Row, action: &str, declarations: &str, member: &str) -> String {
    with_declarations(row, action, declarations).replacen(
        "ACTION: [REF(action.subject), REF(action.other)]",
        &format!("ACTION: [REF(action.subject), REF(action.other)]\n    SEQUENCE: REF({member})"),
        1,
    )
}

/// The graph sub-runs of `core.execute` and `core.test`.
fn graph(runners: &Runners<'_>, row: &Row, family: &str) -> Vec<ExecutedCase> {
    let shipped = runners.shipped;
    let execute = row.operation == "core.execute";
    let comparison = graph_comparison(row);
    let mut runs = Vec::new();

    // The graph target every positive run uses: `action.other` returns
    // `data.number`, so the completed graph exposes exactly one material
    // primary result.
    let one_primary = graph_subject(row, "action.other", comparison);
    let writing = with_member(
        row,
        &format!(
            "OPERATION: {}\n    TARGET: REF(action.writer){comparison}",
            row.operation
        ),
        WRITING_GRAPH,
        "action.writer",
    );

    if family == "binding" {
        let label = if execute {
            "binding/graph-target"
        } else {
            "binding/graph-target-executes-before-comparison"
        };
        runs.push(subject_observation(
            shipped,
            label,
            "a REFERENCE[TASK|ACTION] TARGET is executed as a graph before the row's own result",
            &one_primary,
            MockHost::new(),
            &[
                ("status", "status.succeeded"),
                (
                    "fields",
                    if execute {
                        "[mode, value]"
                    } else {
                        "[actual, evidence, expected, passed]"
                    },
                ),
                ("effects", "[]"),
                ("graph invoked", "[action.other]"),
            ],
        ));
    }

    if family == "effects" {
        runs.push(shipped.execute(
            "mode/graph-cycle-rejected-before-axes",
            "a prohibited reference cycle emits error.reference.cycle and fails before axis resolution",
            &graph_cycle(row),
            Expectation::All(vec![
                Expectation::Rejects("error.reference.cycle".into()),
                Expectation::Attempts {
                    declaration: "action.subject".into(),
                    statuses: Vec::new(),
                },
            ]),
        ));
        runs.push(subject_observation(
            shipped,
            if execute {
                "mode/graph-transitive-axes"
            } else {
                "mode/transitive-axes-normalized"
            },
            "the invocation's effects are the graph's normalized transitive union, and nothing else",
            &writing,
            MockHost::new(),
            &[
                ("status", "status.succeeded"),
                (
                    "fields",
                    if execute {
                        "[mode, value]"
                    } else {
                        "[actual, evidence, expected, passed]"
                    },
                ),
                ("effects", "[memory]"),
                ("graph invoked", "[action.writer]"),
            ],
        ));
        if execute {
            runs.push(subject_observation(
                shipped,
                "mode/graph-no-local-process-effect",
                "a referenced execution unit has no mandatory local process effect",
                &one_primary,
                MockHost::new(),
                &[
                    ("status", "status.succeeded"),
                    ("fields", "[mode, value]"),
                    ("effects", "[]"),
                    ("graph invoked", "[action.other]"),
                ],
            ));
            runs.push(subject_observation(
                shipped,
                "result/graph-no-native-observations",
                "graph mode never synthesizes started, completed, exit_code, stdout or stderr",
                &one_primary,
                MockHost::new(),
                &[
                    ("status", "status.succeeded"),
                    ("fields", "[mode, value]"),
                    ("effects", "[]"),
                    ("graph invoked", "[action.other]"),
                ],
            ));
            runs.push(subject_observation(
                shipped,
                "result/graph-value-single-primary",
                "value is present exactly when the completed graph exposes one material primary result",
                &with_sequence_member(
                    row,
                    "OPERATION: core.execute\n    TARGET: REF(sequence.pair)",
                    TWO_PRIMARIES,
                    "sequence.pair",
                ),
                MockHost::new(),
                &[
                    ("status", "status.succeeded"),
                    ("fields", "[mode]"),
                    ("effects", "[]"),
                    (
                        "graph invoked",
                        "[action.first, action.second, sequence.pair, step.first, step.second]",
                    ),
                ],
            ));
        } else {
            runs.push(subject_observation(
                shipped,
                "mode/graph-executes-before-comparison",
                "the graph runs first, and the comparison reads the store it left",
                &with_member(
                    row,
                    concat!(
                        "OPERATION: core.test\n    TARGET: REF(action.writer)",
                        "\n    PARAMETER:\n        NAME: expected\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"graph\"",
                        "\n    PARAMETER:\n        NAME: actual\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: REF(memory.notes)"
                    ),
                    WRITING_GRAPH,
                    "action.writer",
                ),
                MockHost::new(),
                &[
                    ("status", "status.succeeded"),
                    ("fields", "[actual, evidence, expected, passed]"),
                    ("effects", "[memory]"),
                    ("graph invoked", "[action.writer]"),
                ],
            ));
        }
        runs.push(graph_category(runners, row));
    }

    if family == "errors" {
        runs.push(shipped.execute(
            if execute {
                "path/reference-cycle"
            } else {
                "path/prohibited-graph-cycle"
            },
            "a prohibited graph reference cycle is error.reference.cycle",
            &graph_cycle(row),
            Expectation::Rejects("error.reference.cycle".into()),
        ));
        // A graph member that cannot reach its host: the graph's own
        // identifier stays in the retained evidence, which is the union.
        let crossing = with_member(
            row,
            &format!(
                "OPERATION: {}\n    TARGET: REF(action.writer){comparison}",
                row.operation
            ),
            CROSSING_GRAPH,
            "action.writer",
        );
        runs.push(on_mock(
            shipped,
            if execute {
                "path/graph-error-union"
            } else {
                "path/host-constraint"
            },
            "an applicable error of the referenced graph is unioned with the row's own retained evidence",
            &crossing,
            Expectation::Diagnostic("error.host.constraint".into()),
            MockHost::new().unavailable("core.write", "conformance: no filesystem is installed"),
            "the graph's write cannot reach a filesystem",
        ));
        if execute {
            // The graph's own action declares the retry contract, so the
            // exhaustion is the graph's: `core.execute`'s closed errors list
            // does not admit error.retry.exhausted, and it reaches this row
            // only as the union's retained evidence.
            const RETRYING_GRAPH: &str = concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.coverage\n    NAME: \"Coverage\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n",
                "\nDATA:\n    ID: data.path\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/report.txt\")\n",
                "\nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n",
                "\nHANDLER:\n    ID: handler.retry\n    EVENT: event.host_constraint\n    OPERATION: core.retry\n    LIMIT: 1\n",
                "\nACTION:\n    ID: action.retried\n    OPERATION: core.write\n    TARGET: REF(data.path)\n    ",
                "PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"graph\"\n    ",
                "RETRY:\n        LIMIT: 1\n        HANDLER: REF(handler.retry)\n",
                "\nACTION:\n    ID: action.subject\n    OPERATION: core.execute\n    TARGET: REF(action.retried)\n",
                "\nSUCCESS:\n    ID: success.subject\n    ALL: TRUE\n",
                "\nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    ACTION: [REF(action.subject), REF(action.retried)]\n    HANDLER: REF(handler.retry)\n    SUCCESS: REF(success.subject)\n",
                "\nEXECUTE:\n    REFERENCE: REF(task.subject)\n",
            );
            runs.push(on_mock(
                shipped,
                "path/graph-retry-exhausted-only-through-union",
                "error.retry.exhausted belongs to the graph's own attempts, and reaches this row only through the union",
                RETRYING_GRAPH,
                Expectation::All(vec![
                    // The graph's own attempts exhausted its declared RETRY...
                    Expectation::Diagnostic("error.retry.exhausted".into()),
                    // ...and this row names only what its own closed errors
                    // list admits for a graph that did not complete.
                    Expectation::Diagnostic("error.execution.action".into()),
                    Expectation::Attempts {
                        declaration: "action.subject".into(),
                        statuses: vec!["status.failed".into()],
                    },
                ]),
                MockHost::new().script(
                    "core.write",
                    vec![
                        CapabilityOutcome::Unavailable("conformance: scripted limitation 1".into()),
                        CapabilityOutcome::Unavailable("conformance: scripted limitation 2".into()),
                    ],
                ),
                "every attempt of the graph's write is refused by the host",
            ));
        } else {
            runs.push(on_mock(
                shipped,
                "path/graph-error-union",
                "an applicable error of the referenced graph is unioned with the row's own retained evidence",
                &crossing,
                Expectation::Diagnostic("error.permission.denied".into()),
                MockHost::new().deny("core.write", "conformance: the host grants no access"),
                "the graph's write is refused",
            ));
        }
    }
    runs
}

/// `mode/graph-category-copied`: the row copies the executed graph's final
/// category.
///
/// `05_SEMANTICS/11`: "core.test is deterministic in comparison-only mode and
/// otherwise copies the referenced graph category. core.execute copies its
/// execution-profile category in non-graph mode and its graph category in graph
/// mode", and `graph_resolution`: "The graph is deterministic exactly when every
/// reachable resolved operation is deterministic".
///
/// The two inputs are real: the engine executes each document and records which
/// declarations the graph invoked, and the production `ProfileCatalog` resolves
/// both the graph's category and the row's from it.
fn graph_category(runners: &Runners<'_>, row: &Row) -> ExecutedCase {
    use lcl_capabilities::Determinism;
    let comparison = graph_comparison(row);
    let asking = concat!(
        "\nACTION:\n    ID: action.asker\n    OPERATION: core.ask\n    TARGET: REF(data.text)\n    ",
        "PARAMETER:\n        NAME: question\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"Which environment?\"\n    ",
        "PARAMETER:\n        NAME: expected_type\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"STRING\"\n",
    );
    let catalog = lcl_stdlib::Stdlib::load(runners.spec)
        .expect("the operation surface assembles")
        .with_profiles(Runner::shipped_profiles());
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    let mut sources = Vec::new();
    for (label, target, declarations, members, deterministic_graph) in [
        (
            "deterministic",
            "action.other",
            String::new(),
            vec!["core.return"],
            true,
        ),
        (
            "nondeterministic",
            "action.asker",
            asking.to_string(),
            vec!["core.ask"],
            false,
        ),
    ] {
        let source = with_declarations(
            row,
            &format!(
                "OPERATION: {}\n    TARGET: REF({target}){comparison}",
                row.operation
            ),
            &declarations,
        );
        // The engine runs it, and says which operations the graph reached.
        let observed = runners.shipped.run_on(
            &source,
            &lcl_resolver::MemoryProvider::new(),
            &mut MockHost::new(),
        );
        let reached: Vec<String> = observed
            .invocations
            .iter()
            .filter(|record| record.declaration.as_deref() == Some(target))
            .map(|record| record.id.to_string())
            .collect();
        // "The graph is deterministic exactly when every reachable resolved
        // operation is deterministic."
        let graph = members.iter().all(|member| {
            catalog
                .catalog()
                .resolve_determinism(member, &[], None)
                .is_deterministic()
        });
        let resolved = catalog
            .catalog()
            .resolve_determinism(
                row.operation,
                &[],
                Some(if graph {
                    Determinism::Deterministic
                } else {
                    Determinism::Nondeterministic
                }),
            )
            .is_deterministic();
        expected.push((label.to_string(), deterministic_graph.to_string()));
        actual.push((label.to_string(), resolved.to_string()));
        sources.push(format!(
            "{label}: the graph invoked {reached:?} running {members:?}; graph category deterministic={graph}"
        ));
    }
    let expectation = Expectation::Component(expected);
    let mut observed = crate::Observed {
        component: actual,
        ..crate::Observed::default()
    };
    observed.input_evidence = sources;
    let verdict = crate::judge(&expectation, &observed);
    ExecutedCase {
        id: "mode/graph-category-copied".into(),
        contract: "graph mode copies the executed graph's final determinism category".into(),
        source: String::new(),
        expectation,
        observed,
        verdict,
    }
}

/// The sub-run label each probe's reviewed mapping pins for the scope run.
///
/// The three rows whose catalog requirement names a diagnostic *path* use that
/// wording; `core.ask` states its own authorization contract over the responder
/// and the request; every other row exercises the registered failure.
fn scope_label(op: &str) -> &'static str {
    match op {
        "core.compare" | "core.read" | "core.inspect" => "path/scope-violation",
        "core.ask" => "path/out-of-scope-responder-or-request",
        _ => "error/scope.violation",
    }
}

/// The subject document with a `SCOPE` the action's own TARGET is outside of.
///
/// `scope.narrow` includes exactly one entity — the subject goal, which no row
/// targets — so whatever the row does target is outside it. The task itself is
/// not that entity: `core.stop` targets `REF(task.subject)`, and a scope that
/// admitted it would let that row pass unrefused. `02_LEXICAL/06`: SCOPE
/// declares "the exact set of entities to which a clause may apply".
fn out_of_scope(row: &Row) -> String {
    let scoped = with_action(
        row,
        &format!("{}\n    SCOPE: REF(scope.narrow)", row.action),
    );
    scoped.replacen(
        "\nACTION:\n    ID: action.subject",
        "\nSCOPE:\n    ID: scope.narrow\n    INCLUDE: [REF(goal.subject)]\n\nACTION:\n    ID: action.subject",
        1,
    )
}

/// `error.scope.violation`: "An action targets an entity outside applicable
/// SCOPE".
///
/// `05_SEMANTICS/09`: it "is pre_effect only: ... effective scope resolves at
/// processing step 6 before the first authorized effect". So the refusal is the
/// whole of the run's evidence: the action is never authorized, no attempt is
/// recorded, and no request crosses the boundary. A host that answered would
/// prove the opposite, so the run installs one that would refuse to notice —
/// `MockHost::new()` records every request it is given.
fn scope_violation(runners: &Runners<'_>, row: &Row, label: &str) -> ExecutedCase {
    on_mock(
        runners.reaching(row.operation),
        label,
        "05_SEMANTICS/02 and statuses_and_errors: an action targeting an entity outside its applicable SCOPE is refused with error.scope.violation before the first authorized effect",
        &out_of_scope(row),
        Expectation::All(vec![
            Expectation::Rejects("error.scope.violation".into()),
            Expectation::Attempts {
                declaration: "action.subject".into(),
                statuses: Vec::new(),
            },
        ]),
        MockHost::new(),
        "no host answer is reached: the refusal precedes authorization to act",
    )
}

pub(super) fn errors(
    runners: &Runners<'_>,
    row: &Row,
    contract: &lcl_stdlib::OperationContract,
) -> Vec<ExecutedCase> {
    let op = row.operation;
    let mut runs = Vec::new();
    // Every row whose closed `errors` list admits error.scope.violation is
    // exercised on it. The run exists because the registry lists it, not
    // because a hand-kept list of operations names it.
    if contract.admits_error("error.scope.violation") {
        runs.push(scope_violation(runners, row, scope_label(op)));
    }
    let generic = |name: &str| -> bool {
        match name {
            "permission" => matches!(
                op,
                "core.analyze"
                    | "core.append"
                    | "core.convert"
                    | "core.copy"
                    | "core.create"
                    | "core.delete"
                    | "core.download"
                    | "core.execute"
                    | "core.generate"
                    | "core.install"
                    | "core.memory_write"
                    | "core.modify"
                    | "core.move"
                    | "core.publish"
                    | "core.rename"
                    | "core.report"
                    | "core.send"
                    | "core.start"
                    | "core.state_update"
                    | "core.stop"
                    | "core.uninstall"
                    | "core.upload"
                    | "core.validate"
                    | "core.verify"
                    | "core.write"
            ),
            "host" => matches!(
                op,
                "core.analyze"
                    | "core.append"
                    | "core.memory_write"
                    | "core.state_update"
                    | "core.convert"
                    | "core.copy"
                    | "core.create"
                    | "core.delete"
                    | "core.download"
                    | "core.execute"
                    | "core.generate"
                    | "core.install"
                    | "core.modify"
                    | "core.move"
                    | "core.publish"
                    | "core.rename"
                    | "core.report"
                    | "core.send"
                    | "core.start"
                    | "core.stop"
                    | "core.uninstall"
                    | "core.upload"
                    | "core.write"
            ),
            "action" | "postcondition" => matches!(
                op,
                "core.append"
                    | "core.memory_write"
                    | "core.state_update"
                    | "core.convert"
                    | "core.copy"
                    | "core.create"
                    | "core.delete"
                    | "core.download"
                    | "core.execute"
                    | "core.generate"
                    | "core.install"
                    | "core.modify"
                    | "core.move"
                    | "core.publish"
                    | "core.rename"
                    | "core.send"
                    | "core.start"
                    | "core.stop"
                    | "core.uninstall"
                    | "core.upload"
                    | "core.write"
            ),
            "positional" => matches!(
                op,
                "core.continue"
                    | "core.delete"
                    | "core.execute"
                    | "core.inspect"
                    | "core.install"
                    | "core.read"
                    | "core.report"
                    | "core.return"
                    | "core.sort"
                    | "core.start"
                    | "core.stop"
                    | "core.uninstall"
                    | "core.validate"
            ),
            "state" => matches!(
                op,
                "core.append"
                    | "core.create"
                    | "core.delete"
                    | "core.generate"
                    | "core.modify"
                    | "core.move"
                    | "core.rename"
                    | "core.write"
            ),
            "unresolved" => matches!(op, "core.calculate" | "core.test"),
            _ => unreachable!(),
        }
    };
    if generic("permission") {
        runs.push(permission_denied(runners, row));
    }
    // A store row's three host-side failures are the externally backed
    // storage profile's; every other row reaches its own host as before.
    let reaching = if STORE_ROWS.contains(&op) {
        &runners.store
    } else {
        runners.reaching(op)
    };
    if generic("host") {
        runs.push(host_constraint(runners, reaching, row));
    }
    if generic("action") {
        runs.push(execution_action(runners, reaching, row));
    }
    if generic("postcondition") {
        runs.push(postcondition(runners, reaching, row));
    }
    if generic("positional") {
        runs.push(positional(runners, row));
    }
    if generic("state") {
        runs.push(forbidden_state_target(runners, row));
    }
    if generic("unresolved") {
        runs.push(unresolved_optional_target(runners, row));
    }
    if matches!(op, "core.compare" | "core.validate" | "core.verify") {
        runs.extend(observation_failures(runners, row));
    }
    if matches!(op, "core.execute" | "core.test") {
        runs.extend(graph(runners, row, "errors"));
    }
    if op == "core.validate" {
        // 05_SEMANTICS/11: "Validation emits error.determinism.mismatch exactly
        // when DETERMINISTIC TRUE is declared and that resolved contract is
        // nondeterministic." core.validate "check[s] syntax, type, reference,
        // dependency, and constraints", and its postcondition is that "all
        // detected failures use registered error identifiers", so the finding
        // is recorded in the result rather than raised as a diagnostic.
        runs.push(runners.shipped.execute(
            "error/determinism.mismatch",
            "a kind.operation asserting DETERMINISTIC TRUE over a dependency that admits permitted variation is a detected determinism mismatch",
            &with_declarations(
                row,
                "OPERATION: core.validate\n    TARGET: REF(op.inferred)",
                VARYING_OPERATION,
            ),
            Expectation::All(vec![
                Expectation::Attempts {
                    declaration: "action.subject".into(),
                    statuses: vec!["status.succeeded".into()],
                },
                attempt("valid", "FALSE".into()),
                attempt("errors", "[error.determinism.mismatch]".into()),
                Expectation::NoDiagnostic("error.determinism.mismatch".into()),
            ]),
        ));
    }
    match op {
        "core.analyze" => runs.extend(profile_preconditions(
            runners,
            row,
            "analysis",
            "",
            Effect::Filesystem,
        )),
        "core.report" => runs.extend(profile_preconditions(
            runners,
            row,
            "reporting",
            "",
            Effect::Filesystem,
        )),
        "core.verify" => runs.extend(profile_preconditions(
            runners,
            row,
            "verification",
            "",
            Effect::Filesystem,
        )),
        "core.publish" => runs.extend(profile_preconditions(
            runners,
            row,
            "publication",
            "",
            Effect::Process,
        )),
        // `precondition/profile-out-of-bounds` is not authored for core.execute:
        // its row maxima admit every registered effect class and every
        // registered dependency, so no profile can declare axes outside them.
        "core.execute" => runs.extend(
            profile_preconditions(runners, row, "execution", "", Effect::Memory)
                .into_iter()
                .take(3),
        ),
        "core.download" => {
            runs.extend(profile_preconditions(
                runners,
                row,
                "source",
                "source-",
                Effect::Process,
            ));
            runs.extend(profile_preconditions(
                runners,
                row,
                "transfer",
                "transfer-",
                Effect::Process,
            ));
        }
        _ => {}
    }
    match op {
        "core.convert" | "core.install" | "core.uninstall" => {
            // The shipped engine installs no conversion or package profile.
            runs.push(on_mock(
                runners.shipped,
                "error/operation.precondition",
                "a required profile role that resolves no profile is error.operation.precondition before effects",
                &document(row),
                failed_with(runners.spec, "error.operation.precondition", "pre_effect", "none"),
                MockHost::new(),
                "shipped profiles only",
            ));
        }
        "core.send" => runs.push(missing_role(runners, row, "transport", document(row))),
        "core.start" => runs.push(missing_role(runners, row, "start", document(row))),
        "core.stop" => runs.push(missing_role(runners, row, "stop", reaching_source(row))),
        _ => {}
    }
    runs
}

// ---------------------------------------------------------------------------
// operation_effects: observable post-state and resolved axes
// ---------------------------------------------------------------------------

/// The canonical rendering of one resolved axis set: the exclusive sentinel
/// when no concrete class was selected.
fn axis_set<'a>(classes: impl IntoIterator<Item = &'a String>, sentinel: &str) -> String {
    let mut sorted: Vec<&str> = classes.into_iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.is_empty() {
        format!("[{sentinel}]")
    } else {
        format!("[{}]", sorted.join(", "))
    }
}

/// Run one source on a scripted host and compare the axes every request that
/// crossed the boundary actually resolved, and the effect classes the subject
/// attempt recorded.
fn resolved_axes(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    host: MockHost,
    expected_requests: &[(&str, &str)],
    expected_effects: &str,
) -> ExecutedCase {
    let mut host = host;
    let observed = runner.run_on(source, &lcl_resolver::MemoryProvider::new(), &mut host);
    let requests: Vec<String> = host
        .requests()
        .iter()
        .map(|r| {
            format!(
                "{} deps={} effects={}",
                r.operation,
                axis_set(&r.possible_dependencies, "declared_state_only"),
                axis_set(&r.possible_effects, "none")
            )
        })
        .collect();
    let effects: Vec<String> = observed
        .invocations
        .iter()
        .filter(|i| i.declaration.as_deref() == Some("action.subject"))
        .filter_map(|i| i.result.as_ref())
        .flat_map(|r| {
            r.observed_effects
                .iter()
                .map(|e| e.class.as_registry_str().to_string())
        })
        .collect();
    let operation = source
        .split("ID: action.subject\n    OPERATION: ")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .unwrap_or_default()
        .to_string();
    let expected = vec![
        (
            "requests".to_string(),
            format!(
                "{:?}",
                expected_requests
                    .iter()
                    .map(|(deps, effects)| format!("{operation} deps={deps} effects={effects}"))
                    .collect::<Vec<_>>()
            ),
        ),
        ("subject_effects".to_string(), expected_effects.to_string()),
    ];
    let actual = vec![
        ("requests".to_string(), format!("{requests:?}")),
        ("subject_effects".to_string(), axis_set(&effects, "none")),
    ];
    let expectation = Expectation::Component(expected);
    let mut observed = observed;
    observed.component = actual;
    observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    let verdict = crate::judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: clause.into(),
        source: source.to_string(),
        expectation,
        observed,
        verdict,
    }
}

/// One in-memory filesystem snapshot, as exact UTF-8 text per path.
fn snapshot(filesystem: &super::SharedFs) -> String {
    let fs = filesystem.0.borrow();
    let files: std::collections::BTreeMap<String, String> = fs
        .paths()
        .iter()
        .map(|p| {
            (
                p.to_string_lossy().to_string(),
                String::from_utf8_lossy(fs.file(p).unwrap()).to_string(),
            )
        })
        .collect();
    format!("{files:?}")
}

/// Run one source on the shipped filesystem adapter over a seeded in-memory
/// filesystem, then compare the engine assertions and the exact final files.
fn on_files(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    seed: &[(&str, &str)],
    engine: Vec<Expectation>,
    expected_files: &[(&str, &str)],
) -> ExecutedCase {
    let mut memory = lcl_stdlib::MemoryFileSystem::new().with_scope("/srv/data");
    for (path, content) in seed {
        memory = memory.with_file(*path, *content);
    }
    let filesystem = super::SharedFs(std::rc::Rc::new(std::cell::RefCell::new(memory)));
    let mut host = super::host().with_filesystem(filesystem.clone());
    let initial = snapshot(&filesystem);
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, &mut host);
    let expected: std::collections::BTreeMap<String, String> = expected_files
        .iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect();
    let mut assertions = engine;
    assertions.push(Expectation::Component(vec![(
        "filesystem".into(),
        format!("{expected:?}"),
    )]));
    case.expectation = Expectation::All(assertions);
    case.observed.component = vec![("filesystem".into(), snapshot(&filesystem))];
    case.observed.input_evidence.push(format!(
        "host: shipped HostAdapter over an in-memory filesystem scoped /srv/data; initial files {initial}; requests {:?}",
        host.requests()
    ));
    case.verdict = crate::judge(&case.expectation, &case.observed);
    case
}

/// The subject attempt succeeded.
fn succeeded() -> Expectation {
    Expectation::Attempts {
        declaration: "action.subject".into(),
        statuses: vec!["status.succeeded".into()],
    }
}

/// The subject attempt failed with this error before any effect.
fn refused_before_effects(spec: &SpecPackage, error: &str) -> Vec<Expectation> {
    match failed_with(spec, error, "pre_effect", "none") {
        Expectation::All(parts) => parts,
        _ => unreachable!(),
    }
}

const REPORT: (&str, &str) = ("/srv/data/report.txt", "content");
const OTHER: (&str, &str) = ("/srv/data/other.txt", "old");

/// The subject action of `row` with its TARGET replaced and one parameter
/// block appended or replaced.
fn action_with(row: &Row, target: Option<&str>, extra: &str) -> String {
    let base = match target {
        Some(target) => retargeted(row, target),
        None => row.action.to_string(),
    };
    format!("{base}{extra}")
}

/// A named PARAMETER block, as the fixture writes one.
fn named(name: &str, ty: &str, required: &str, value: &str) -> String {
    format!(
        "\n    PARAMETER:\n        NAME: {name}\n        TYPE: {ty}\n        REQUIRED: {required}\n        VALUE: {value}"
    )
}

/// The canonical invocation axes of each row's shared fixture invocation, as
/// `(requests crossing the boundary, subject effect classes)`.
fn fixture_axes(op: &str) -> Option<(Vec<(&'static str, &'static str)>, &'static str)> {
    let crossing = |deps: &'static str, effects: &'static str, subject: &'static str| {
        Some((vec![(deps, effects)], subject))
    };
    match op {
        // read_only rows over a PATH: "Resolve host for PATH ... The invocation
        // has no effects."
        "core.read" | "core.inspect" => crossing("[host]", "[none]", "[none]"),
        // "Resolve exactly one immutable analysis profile ... then resolve
        // model from the selected analytical method and host or network from an
        // addressable target": the fixture target is a material STRING.
        "core.analyze" => crossing("[model]", "[none]", "[none]"),
        "core.report" => crossing("[model]", "[none]", "[none]"),
        // PATH mutation adds host dependency and filesystem effect.
        "core.create" | "core.write" | "core.append" | "core.modify" | "core.delete"
        | "core.rename" => crossing("[host]", "[filesystem]", "[filesystem]"),
        // PATH source and PATH destination.
        "core.copy" | "core.move" => crossing("[host]", "[filesystem]", "[filesystem]"),
        "core.generate" => crossing("[host, model]", "[filesystem]", "[filesystem]"),
        // A remote source adds network dependency and network effect; the
        // destination PATH adds host dependency and filesystem effect.
        "core.download" => crossing("[host, network]", "[filesystem, network]", "[filesystem]"),
        // A PATH source adds host dependency but no filesystem effect; a URI
        // destination adds network dependency and network effect.
        "core.upload" | "core.publish" => crossing("[host, network]", "[network]", "[network]"),
        // Every non-graph invocation has the process effect.
        "core.execute" | "core.start" => crossing("[host]", "[process]", "[process]"),
        // Package state from the selected transaction.
        "core.install" | "core.uninstall" => crossing("[host]", "[package, process]", "[package]"),
        // Host and optional network from the URI recipient; the message effect.
        "core.send" => crossing("[host, network]", "[message]", "[message]"),
        // One authoritative human responder; the request is a message effect.
        "core.ask" => crossing("[human]", "[message]", "[message]"),
        // declared_state_only rows reach no capability and change nothing.
        "core.calculate" | "core.select" | "core.filter" | "core.sort" | "core.group"
        | "core.return" | "core.compare" | "core.validate" | "core.verify" | "core.test" => {
            Some((Vec::new(), "[none]"))
        }
        // The engine-owned stores: the storage profile narrows the host
        // dependency to none and keeps the row's one effect class.
        "core.memory_write" => Some((Vec::new(), "[memory]")),
        "core.state_update" => Some((Vec::new(), "[state]")),
        // Internal execution-unit state.
        "core.cancel" | "core.stop" => Some((Vec::new(), "[state]")),
        _ => None,
    }
}

/// `resolution/exact-invocation-axes` for the rows whose shared fixture
/// invocation completes.
fn exact_axes(runners: &Runners<'_>, row: &Row) -> Option<ExecutedCase> {
    let (requests, effects) = fixture_axes(row.operation)?;
    Some(resolved_axes(
        runners.reaching(row.operation),
        "resolution/exact-invocation-axes",
        "operations axis_contract: the invocation resolves exactly its selected classes, never the row maximum",
        &document(row),
        MockHost::new(),
        &requests,
        effects,
    ))
}

/// `address/*` for the rows whose TARGET is a mutable address: one run per
/// address class.
fn target_address_classes(runners: &Runners<'_>, row: &Row) -> Vec<ExecutedCase> {
    let runner = runners.reaching(row.operation);
    let generative = row.operation == "core.generate";
    let model = |deps: &'static str| -> &'static str {
        match (generative, deps) {
            (false, d) => d,
            (true, "[host]") => "[host, model]",
            (true, "[network]") => "[model, network]",
            (true, _) => "[model]",
        }
    };
    let output =
        "\nOUTPUT:\n    ID: output.addressed\n    TYPE: STRING\n    FORMAT: format.plain_text\n";
    // A row with a required profile role reaches its host for a URI or an
    // OUTPUT address only through a profile serving that class; the shipped
    // filesystem profiles serve PATH alone.
    // A D3 fixture row's own fixture profile already serves every class.
    let role = (!FIXTURE_ROWS.contains(&row.operation))
        .then(|| {
            runners
                .fixture
                .stdlib_catalog()
                .required_roles(row.operation, None)
                .into_iter()
                .next()
        })
        .flatten();
    let serving = |class: AddressClass,
                   deps: &[Dependency],
                   effects: &[Effect]|
     -> Option<Runner> {
        let role = role.clone()?;
        let mut profiles = Runner::shipped_profiles();
        profiles.extend(fixture_profiles());
        profiles.push(
            Profile::builder(row.operation, role, "conformance.fixture.address", "1")
                .serving(TargetClass::Only(vec![class]))
                .determinism(
                    Determinism::Deterministic,
                    "conformance fixture: the exact address and declared content fix the post-state",
                )
                .axes(lcl_capabilities::profile::axes(deps, effects))
                .resolving("the row's address-class rule for exactly this class"),
        );
        Some(Runner::with_profiles(runners.spec, profiles).expect("the engine assembles"))
    };
    let uri_runner = serving(
        AddressClass::Uri,
        &[Dependency::Network],
        &[Effect::Network],
    );
    let output_runner = serving(AddressClass::Output, &[], &[Effect::State]);
    vec![
        resolved_axes(
            runner,
            "address/path-host-filesystem",
            "PATH mutation adds host dependency and filesystem effect",
            &document(row),
            MockHost::new(),
            &[(model("[host]"), "[filesystem]")],
            "[filesystem]",
        ),
        resolved_axes(
            uri_runner.as_ref().unwrap_or(runner),
            "address/uri-network-network",
            "URI mutation adds network dependency and network effect",
            &with_action(row, &retargeted(row, "REF(data.uri)")),
            MockHost::new(),
            &[(model("[network]"), "[network]")],
            "[network]",
        ),
        resolved_axes(
            output_runner.as_ref().unwrap_or(runner),
            "address/output-state",
            "OUTPUT mutation adds state effect and its required dependency",
            &with_action(row, &retargeted(row, "REF(output.addressed)")).replacen(
                "\nACTION:\n    ID: action.subject",
                &format!("{output}\nACTION:\n    ID: action.subject"),
                1,
            ),
            MockHost::new(),
            &[(model("[declared_state_only]"), "[state]")],
            "[state]",
        ),
    ]
}

/// Every clause run of one row's `operation_effects` group that the parent
/// module does not derive.
pub(super) fn effects(runners: &Runners<'_>, row: &Row) -> Vec<ExecutedCase> {
    if matches!(row.operation, "core.execute" | "core.test") {
        let mut graph_runs = graph(runners, row, "effects");
        graph_runs.extend(effects_inner(runners, row));
        return graph_runs;
    }
    effects_inner(runners, row)
}

fn effects_inner(runners: &Runners<'_>, row: &Row) -> Vec<ExecutedCase> {
    let op = row.operation;
    let spec = runners.spec;
    let shipped = runners.shipped;
    let mut runs = Vec::new();
    if !matches!(op, "core.continue" | "core.retry") {
        runs.extend(exact_axes(runners, row));
    }
    if matches!(
        op,
        "core.append"
            | "core.create"
            | "core.delete"
            | "core.generate"
            | "core.modify"
            | "core.rename"
            | "core.write"
    ) {
        runs.extend(target_address_classes(runners, row));
    }
    let memory_target = |label: &str| {
        on_files(
            shipped,
            label,
            "precondition: the target is not MEMORY or STATE",
            &with_action(row, &retargeted(row, "REF(memory.notes)")),
            &[REPORT],
            refused_before_effects(spec, "error.operation.precondition"),
            &[REPORT],
        )
    };
    let absent = "PATH(\"/srv/data/absent.txt\")";
    match op {
        "core.read" | "core.inspect" => {
            runs.push(on_files(
                shipped,
                "precondition/0",
                "precondition: the target exists (and is readable)",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: target bytes/state unchanged",
                &document(row),
                &[REPORT],
                vec![succeeded()],
                &[REPORT],
            ));
            if op == "core.read" {
                runs.push(on_files(
                    shipped,
                    "postcondition/1",
                    "postcondition: the result is the exact requested representation",
                    &document(row),
                    &[REPORT],
                    vec![succeeded(), attempt("value", "\"content\"".into())],
                    &[REPORT],
                ));
            }
        }
        "core.create" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target does not exist when fail_if_exists is TRUE",
                &with_action(row, &retargeted(row, "REF(data.path)")),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/2",
                "precondition: at least content or target_type is supplied",
                &with_action(row, "OPERATION: core.create\n    TARGET: REF(data.other)"),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: the target exists",
                &document(row),
                &[REPORT],
                vec![succeeded()],
                &[REPORT, ("/srv/data/other.txt", "new")],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/1",
                "postcondition: content/type matches parameters",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.create\n    TARGET: REF(data.other){}",
                        named("content", "STRING", "FALSE", "\"exact created content\"")
                    ),
                ),
                &[REPORT],
                vec![succeeded(), attempt("changed", "TRUE".into())],
                &[REPORT, ("/srv/data/other.txt", "exact created content")],
            ));
        }
        "core.delete" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target exists unless require_exists is FALSE",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/2",
                "precondition: recursive deletion is explicitly authorized when needed",
                &with_action(row, &retargeted(row, "PATH(\"/srv/data/tree\")")),
                &[REPORT, ("/srv/data/tree/leaf.txt", "leaf")],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT, ("/srv/data/tree/leaf.txt", "leaf")],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: the target is absent",
                &document(row),
                &[REPORT],
                vec![succeeded(), attempt("changed", "TRUE".into())],
                &[],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/1",
                "postcondition: no out-of-scope target changed",
                &document(row),
                &[REPORT, OTHER, ("/srv/outside/kept.txt", "kept")],
                vec![succeeded()],
                &[OTHER, ("/srv/outside/kept.txt", "kept")],
            ));
        }
        "core.append" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target exists and supports append",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: content is the exact suffix; the prefix is unchanged",
                &document(row),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/report.txt", "contentmore")],
            ));
        }
        "core.write" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target exists unless create_if_missing is TRUE",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: the complete target content equals the content parameter",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.write\n    TARGET: REF(data.path){}",
                        named("content", "STRING", "TRUE", "\"replaced\"")
                    ),
                ),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/report.txt", "replaced")],
            ));
        }
        "core.modify" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target exists",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/2",
                "precondition: expected_before matches when supplied",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("expected_before", "STRING", "FALSE", "\"not the content\""),
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: only the declared target changes",
                &document(row),
                &[REPORT, OTHER],
                vec![succeeded()],
                &[OTHER, ("/srv/data/report.txt", "changed")],
            ));
        }
        "core.rename" => {
            runs.push(memory_target("precondition/0"));
            runs.push(on_files(
                shipped,
                "precondition/1",
                "precondition: the target exists",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/2",
                "precondition: new_name is legal (no path separator)",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", "\"nested/renamed.txt\"")
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/3",
                "precondition: new_name differs from the current target name",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", "\"report.txt\"")
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "precondition/4",
                "precondition: the renamed destination is absent unless overwrite is TRUE",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", "\"other.txt\"")
                    ),
                ),
                &[REPORT, OTHER],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT, OTHER],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/0",
                "postcondition: the target is addressable by new_name and not by its prior name",
                &document(row),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/renamed.txt", "content")],
            ));
            runs.push(on_files(
                shipped,
                "postcondition/1",
                "postcondition: content and non-name properties are preserved",
                &document(row),
                &[REPORT, OTHER],
                vec![succeeded(), attempt("changed", "TRUE".into())],
                &[OTHER, ("/srv/data/renamed.txt", "content")],
            ));
        }
        _ => {}
    }
    runs
}

/// The `error/operation.precondition` runs of the transfer rows whose
/// registered source precondition the shared fixture can falsify.
pub(super) fn transfer_source_preconditions(
    runners: &Runners<'_>,
    row: &Row,
) -> Option<ExecutedCase> {
    matches!(row.operation, "core.copy" | "core.upload").then(|| {
        on_files(
            runners.shipped,
            "error/operation.precondition",
            "precondition: the source exists",
            &with_action(row, &retargeted(row, "PATH(\"/srv/data/absent.txt\")")),
            &[REPORT],
            refused_before_effects(runners.spec, "error.operation.precondition"),
            &[REPORT],
        )
    })
}

// ---------------------------------------------------------------------------
// operation_binding: exact binding, defaults and binding-time rejections
// ---------------------------------------------------------------------------

/// Run one source on a scripted host and compare named parameters of the
/// first request for `operation` that crossed the boundary, after the
/// invocation completed.
fn bound_parameters(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    operation: &str,
    expected: &[(&str, &str)],
) -> ExecutedCase {
    let mut host = MockHost::new();
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, &mut host);
    let request = host.requests().iter().find(|r| r.operation == operation);
    let actual: Vec<(String, String)> = expected
        .iter()
        .map(|(name, _)| {
            (
                format!("parameter/{name}"),
                request
                    .and_then(|r| r.parameters.get(*name))
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<not bound in any request>".into()),
            )
        })
        .collect();
    case.expectation = Expectation::All(vec![
        succeeded(),
        Expectation::Component(
            expected
                .iter()
                .map(|(name, value)| (format!("parameter/{name}"), value.to_string()))
                .collect(),
        ),
    ]);
    case.observed.component = actual;
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case.verdict = crate::judge(&case.expectation, &case.observed);
    case
}

/// `binding/default/<name>`: the registered non-null default is the exact
/// value the invocation binds when the optional parameter is omitted.
fn default_binding(
    runners: &Runners<'_>,
    row: &Row,
    name: &str,
    rendered: &str,
    source: String,
) -> ExecutedCase {
    bound_parameters(
        runners.reaching(row.operation),
        &format!("binding/default/{name}"),
        "parameter_default_encoding: a non-null default applies only to a MISSING optional parameter",
        &source,
        row.operation,
        &[(name, rendered)],
    )
}

/// `binding/<role>-profile-role`: exactly one installed immutable profile for
/// the named role is selected and the invocation reaches its host.
fn profile_role(
    runners: &Runners<'_>,
    row: &Row,
    label: &str,
    role: &str,
    class: AddressClass,
    source: String,
) -> ExecutedCase {
    let runner = runners.reaching(row.operation);
    let catalog = runner.stdlib_catalog();
    let selection = lcl_capabilities::Selection {
        operation: row.operation,
        role: Role::new(role),
        target_class: class,
        implementation: None,
    };
    let selected = match catalog.select(&selection) {
        Ok(_) => "exactly one".to_string(),
        Err(fault) => format!("refused: {fault}"),
    };
    let mut case = on_mock(
        runner,
        label,
        "implementation_profile.selection: exactly one immutable profile per required role before effects",
        &source,
        Expectation::Accepts,
        MockHost::new(),
        &format!("ProfileCatalog::select({selection:?}) over the installed catalog"),
    );
    case.expectation = Expectation::All(vec![
        succeeded(),
        Expectation::NoDiagnostic("error.operation.precondition".into()),
        Expectation::Component(vec![("selected_role_profile".into(), "exactly one".into())]),
    ]);
    case.observed.component = vec![("selected_role_profile".into(), selected)];
    case.verdict = crate::judge(&case.expectation, &case.observed);
    case
}

/// A D3 fixture row completes on its scripted fixture capability.
fn fixture_success(
    runners: &Runners<'_>,
    row: &Row,
    observation: Observation,
    fields: &[(&str, &str)],
) -> ExecutedCase {
    let mut assertions = vec![
        succeeded(),
        Expectation::NoDiagnostic("error.operation.precondition".into()),
        Expectation::NoDiagnostic("error.host.constraint".into()),
    ];
    assertions.extend(
        fields
            .iter()
            .map(|(name, value)| attempt(name, value.to_string())),
    );
    on_mock(
        &runners.fixture,
        "fixture/success",
        "LCL-CLOSE-02 D3: a conformance-only deterministic fixture capability completes the row",
        &document(row),
        Expectation::All(assertions),
        MockHost::new().script(
            row.operation,
            vec![CapabilityOutcome::Completed(observation.clone())],
        ),
        &format!("D3 fixture completion {observation:?}"),
    )
}

/// The same rejection a binding clause names, evidenced against the shared
/// filesystem fixture.
fn store_target_rejected(
    runners: &Runners<'_>,
    row: &Row,
    label: &str,
    target: &str,
) -> ExecutedCase {
    on_files(
        runners.shipped,
        label,
        "MEMORY and STATE targets are prohibited; use core.memory_write or core.state_update",
        &with_action(row, &retargeted(row, target)),
        &[REPORT],
        refused_before_effects(runners.spec, "error.operation.precondition"),
        &[REPORT],
    )
}

/// A document whose subject invocation needs extra declarations.
fn with_declarations(row: &Row, action: &str, declarations: &str) -> String {
    with_action(row, action).replacen(
        "\nACTION:\n    ID: action.subject",
        &format!("{declarations}\nACTION:\n    ID: action.subject"),
        1,
    )
}

/// A pure custom operation's contract, as a DEFINE block.
fn pure_operation(id: &str, parameter: &str, result: &str, meaning: &str) -> String {
    format!(
        "\nDEFINE:\n    ID: {id}\n    KIND: kind.operation\n    MEANING: \"{meaning}\"\n    SIDE_EFFECT: FALSE\n    \
         DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: {parameter}\n        \
         REQUIRED: TRUE\n    RESULT:\n        TYPE: {result}\n"
    )
}

/// An engine with the shipped profiles and exactly these pure custom
/// operation implementations installed.
fn with_pure(
    runners: &Runners<'_>,
    operations: Vec<(&'static str, lcl_stdlib::PureOperation)>,
) -> Runner {
    Runner::with_surface(runners.spec, move |stdlib| {
        operations.into_iter().fold(
            stdlib.with_profiles(Runner::shipped_profiles()),
            |stdlib, (id, implementation)| stdlib.with_pure_operation(id, implementation),
        )
    })
    .expect("the engine assembles with pure operation implementations")
}

fn integer_of(value: &lcl_runtime::Value) -> Option<i64> {
    match value {
        lcl_runtime::Value::Integer(number) => number.to_i64(),
        _ => None,
    }
}

/// Every clause run of one row's `operation_binding` group that the parent
/// module does not derive.
pub(super) fn binding(runners: &Runners<'_>, row: &Row) -> Vec<ExecutedCase> {
    use lcl_runtime::Value;
    let op = row.operation;
    let spec = runners.spec;
    let shipped = runners.shipped;
    let doc = || document(row);
    let mut runs = Vec::new();
    if matches!(op, "core.execute" | "core.test") {
        runs.extend(graph(runners, row, "binding"));
    }
    let absent = "PATH(\"/srv/data/absent.txt\")";
    let text = |s: &str| Value::Text(s.to_string());
    match op {
        "core.analyze" => runs.push(fixture_success(
            runners,
            row,
            lcl_stdlib::schema::value(Value::Object(
                [
                    ("facts".to_string(), Value::List(vec![text("content")])),
                    (
                        "inferences".to_string(),
                        Value::List(vec![text("neutral tone")]),
                    ),
                ]
                .into_iter()
                .collect(),
            )),
            &[(
                "value",
                "{facts: [\"content\"], inferences: [\"neutral tone\"]}",
            )],
        )),
        "core.report" => {
            runs.push(default_binding(
                runners,
                row,
                "format",
                "format.plain_text",
                doc(),
            ));
            runs.push(default_binding(
                runners,
                row,
                "include_evidence",
                "TRUE",
                doc(),
            ));
            runs.push(fixture_success(
                runners,
                row,
                lcl_stdlib::schema::value(text(
                    "facts: content; inference: none; missing: none; unknown: none",
                )),
                &[(
                    "value",
                    "\"facts: content; inference: none; missing: none; unknown: none\"",
                )],
            ));
        }
        "core.convert" => {
            runs.push(default_binding(runners, row, "preserve", "[]", doc()));
            runs.push(fixture_success(
                runners,
                row,
                lcl_stdlib::schema::operation_with_value(
                    Value::Constructed {
                        constructor: "PATH".into(),
                        text: "/srv/data/report.txt".into(),
                    },
                    Value::Boolean(true),
                    text("{\"content\":\"content\"}"),
                ),
                &[
                    ("changed", "TRUE"),
                    ("value", "\"{\\\"content\\\":\\\"content\\\"}\""),
                ],
            ));
        }
        "core.install" => runs.push(fixture_success(
            runners,
            row,
            lcl_stdlib::schema::operation(text("content"), Value::Boolean(true)),
            &[("changed", "TRUE")],
        )),
        "core.uninstall" => {
            runs.push(default_binding(runners, row, "purge_data", "FALSE", doc()));
            runs.push(fixture_success(
                runners,
                row,
                lcl_stdlib::schema::operation(text("content"), Value::Boolean(true)),
                &[("changed", "TRUE")],
            ));
        }
        "core.generate" => {
            runs.push(default_binding(runners, row, "variation", "{}", doc()));
            runs.push(bound_parameters(
                &runners.fixture,
                "binding/format",
                "bind the exact declared artifact format",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("format", "STRING", "FALSE", "\"format.json\""),
                    ),
                ),
                op,
                &[("format", "\"format.json\"")],
            ));
            runs.push(bound_parameters(
                &runners.fixture,
                "binding/variation-bounds",
                "bind the exact declared acceptable-variation bounds",
                &with_action(
                    row,
                    &action_with(row, None, "\n    PARAMETER:\n        NAME: variation\n        TYPE: OBJECT\n        REQUIRED: FALSE\n        VALUE:\n            tone: \"neutral\""),
                ),
                op,
                &[("variation", "{tone: \"neutral\"}")],
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/generation-profile-role",
                "generation",
                AddressClass::Path,
                doc(),
            ));
            runs.push(store_target_rejected(
                runners,
                row,
                "binding/memory-target-rejected",
                "REF(memory.notes)",
            ));
            runs.push(store_target_rejected(
                runners,
                row,
                "binding/state-target-rejected",
                "REF(state.revision)",
            ));
            runs.push(fixture_success(
                runners,
                row,
                lcl_stdlib::schema::operation_with_value(
                    Value::Constructed {
                        constructor: "PATH".into(),
                        text: "/srv/data/other.txt".into(),
                    },
                    Value::Boolean(true),
                    text("a short summary"),
                ),
                &[("changed", "TRUE"), ("value", "\"a short summary\"")],
            ));
        }
        "core.append" | "core.write" | "core.modify" | "core.rename" | "core.create"
        | "core.delete" => {
            runs.push(store_target_rejected(
                runners,
                row,
                "binding/memory-target-rejected",
                "REF(memory.notes)",
            ));
            runs.push(store_target_rejected(
                runners,
                row,
                "binding/state-target-rejected",
                "REF(state.revision)",
            ));
        }
        _ => {}
    }
    match op {
        "core.ask" => {
            runs.push(refused_before_request(
                runners.reaching("core.ask"),
                "binding/authorized-in-scope-before-message",
                "06_STANDARD_LIBRARY/03: \"The request and responder must be authorized and in scope; violations use error.permission.denied or error.scope.violation.\" The scope refusal precedes the message effect, so no question is ever put",
                &out_of_scope(row),
                "error.scope.violation",
            ));
            runs.push(on_files(
                shipped,
                "binding/authoritative-responder",
                "resolve one authoritative human responder and bind its answer",
                &doc(),
                &[REPORT],
                vec![succeeded(), attempt("value", "\"staging\"".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/options-compatible",
                "every option compatible with expected_type binds; the answer equals one listed option",
                &with_action(
                    row,
                    &action_with(row, None, &named("options", "LIST[STRING]", "FALSE", "[\"staging\", \"production\"]")),
                ),
                &[REPORT],
                vec![succeeded(), attempt("value", "\"staging\"".into())],
                &[REPORT],
            ));
        }
        "core.calculate" => {
            let calculate = |expression: &str, target: Option<&str>| {
                let target = target
                    .map(|t| format!("\n    TARGET: {t}"))
                    .unwrap_or_default();
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.calculate{target}{}",
                        named("expression", "STRING", "TRUE", expression)
                    ),
                )
            };
            runs.push(on_files(
                shipped,
                "binding/default/bindings",
                "an omitted bindings OBJECT binds the registered default {}: the fragment evaluates with no local binding",
                &calculate("\"REF(data.number) + 1\"", None),
                &[REPORT],
                vec![succeeded(), attempt("value", "4".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/exists-exception",
                "evaluation_contract: EXISTS consumes MISSING without error.required.missing",
                &calculate("\"EXISTS(target)\"", None),
                &[REPORT],
                vec![succeeded(), attempt("value", "FALSE".into())],
                &[REPORT],
            ));
            runs.push(shipped.execute(
                "binding/final-missing-rejected",
                "a required final MISSING result uses error.required.missing",
                &calculate("\"target\"", None),
                Expectation::Diagnostic("error.required.missing".into()),
            ));
            runs.push(shipped.execute(
                "binding/final-unknown-rejected",
                "a required final UNKNOWN result uses error.value.unknown",
                &with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.calculate\n    TARGET: REF(data.undetermined){}",
                        named("expression", "STRING", "TRUE", "\"target\"")
                    ),
                    "\nDATA:\n    ID: data.undetermined\n    TYPE: INTEGER\n    VALUE: UNKNOWN\n",
                ),
                Expectation::Diagnostic("error.value.unknown".into()),
            ));
            runs.push(on_files(
                shipped,
                "binding/skipped-operand-exception",
                "evaluation_contract: an operand in a skipped Boolean branch is not consumed",
                &calculate("\"FALSE AND target\"", None),
                &[REPORT],
                vec![succeeded(), attempt("value", "FALSE".into())],
                &[REPORT],
            ));
        }
        "core.compare" => {
            let compare = |target: &str, against: (&str, &str), criteria: Option<&str>| {
                let criteria = criteria
                    .map(|c| named("criteria", "STRING", "FALSE", c))
                    .unwrap_or_default();
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.compare\n    TARGET: {target}{}{criteria}",
                        named("against", against.0, "TRUE", against.1)
                    ),
                )
            };
            runs.push(on_files(
                shipped,
                "binding/default/criteria",
                "omitted criteria defaults to the registered == comparison",
                &compare("REF(data.number)", ("INTEGER", "3"), None),
                &[REPORT],
                vec![succeeded(), attempt("value", "TRUE".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/omitted-criteria-strict-equality",
                "== strict equality: different material static types are unequal, no coercion",
                &compare("REF(data.number)", ("STRING", "\"3\""), None),
                &[REPORT],
                vec![succeeded(), attempt("value", "FALSE".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/supplied-criteria-exact",
                "a supplied criterion binds exactly: 3 < 4",
                &compare("REF(data.number)", ("INTEGER", "4"), Some("\"<\"")),
                &[REPORT],
                vec![succeeded(), attempt("value", "TRUE".into())],
                &[REPORT],
            ));
        }
        "core.continue" => runs.push(
            crate::witness_cases::Probe::new(
                crate::witness_cases::continue_read(true),
                Expectation::All(vec![
                    Expectation::Accepts,
                    Expectation::Recovered("action.read".into()),
                    Expectation::Attempts {
                        declaration: "action.next".into(),
                        statuses: vec!["status.succeeded".into()],
                    },
                ]),
            )
            .with_read_failures(1)
            .execute(
                shipped,
                "binding/success-path",
                "core.continue binds its handler-context target and advances",
            ),
        ),
        "core.retry" => runs.push(
            crate::witness_cases::Probe::new(
                crate::witness_cases::retry_read(None),
                Expectation::All(vec![
                    Expectation::Accepts,
                    Expectation::Attempts {
                        declaration: "action.read".into(),
                        statuses: vec!["status.blocked".into(), "status.succeeded".into()],
                    },
                    Expectation::NoDiagnostic("error.retry.exhausted".into()),
                ]),
            )
            .with_read_failures(1)
            .execute(
                shipped,
                "binding/success-path",
                "core.retry binds its handler-context target and limit",
            ),
        ),
        "core.copy" | "core.move" => {
            let role = if op == "core.copy" { "copy" } else { "move" };
            runs.push(default_binding(runners, row, "overwrite", "FALSE", doc()));
            runs.push(profile_role(
                runners,
                row,
                &format!("binding/{role}-profile-role"),
                role,
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/destination-absent-unless-overwrite",
                "the destination must be absent unless overwrite is TRUE",
                &doc(),
                &[REPORT, OTHER],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT, OTHER],
            ));
            if op == "core.copy" {
                runs.push(on_files(
                    shipped,
                    "binding/overwrite-true-replaces",
                    "overwrite TRUE permits replacing the destination with the exact source",
                    &with_action(
                        row,
                        &action_with(row, None, &named("overwrite", "BOOLEAN", "FALSE", "TRUE")),
                    ),
                    &[REPORT, OTHER],
                    vec![succeeded()],
                    &[("/srv/data/other.txt", "content"), REPORT],
                ));
            } else {
                runs.push(on_files(
                    shipped,
                    "binding/distinct-addresses",
                    "distinct resolved source and destination relocate the content",
                    &doc(),
                    &[REPORT],
                    vec![succeeded()],
                    &[("/srv/data/other.txt", "content")],
                ));
                runs.push(on_files(
                    shipped,
                    "binding/overwrite-never-permits-same-address",
                    "overwrite never permits a source to be its own destination",
                    &with_action(
                        row,
                        &format!(
                            "OPERATION: core.move\n    TARGET: REF(data.path){}{}",
                            named("destination", "PATH", "TRUE", "REF(data.path)"),
                            named("overwrite", "BOOLEAN", "FALSE", "TRUE")
                        ),
                    ),
                    &[REPORT],
                    refused_before_effects(spec, "error.operation.precondition"),
                    &[REPORT],
                ));
                runs.push(store_target_rejected(
                    runners,
                    row,
                    "binding/memory-source-rejected",
                    "REF(memory.notes)",
                ));
                runs.push(store_target_rejected(
                    runners,
                    row,
                    "binding/state-source-rejected",
                    "REF(state.revision)",
                ));
            }
        }
        "core.create" => {
            runs.push(default_binding(
                runners,
                row,
                "fail_if_exists",
                "TRUE",
                doc(),
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/target-profile-role",
                "target",
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/fail-if-exists-false-reconciles",
                "fail_if_exists FALSE reconciles an existing target to the exact declared post-state",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.create\n    TARGET: REF(data.path){}{}",
                        named("content", "STRING", "FALSE", "\"new\""),
                        named("fail_if_exists", "BOOLEAN", "FALSE", "FALSE")
                    ),
                ),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/report.txt", "new")],
            ));
            runs.push(on_files(
                shipped,
                "binding/fail-if-exists-true-requires-absent",
                "fail_if_exists TRUE requires an absent target",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.create\n    TARGET: REF(data.path){}{}",
                        named("content", "STRING", "FALSE", "\"new\""),
                        named("fail_if_exists", "BOOLEAN", "FALSE", "TRUE")
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
        }
        "core.delete" => {
            runs.push(default_binding(runners, row, "recursive", "FALSE", doc()));
            runs.push(default_binding(
                runners,
                row,
                "require_exists",
                "TRUE",
                doc(),
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/delete-profile-role",
                "delete",
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/recursive-policy",
                "recursive TRUE authorizes deleting a directory and its children",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.delete\n    TARGET: PATH(\"/srv/data/tree\"){}",
                        named("recursive", "BOOLEAN", "FALSE", "TRUE")
                    ),
                ),
                &[REPORT, ("/srv/data/tree/leaf.txt", "leaf")],
                vec![succeeded()],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/require-exists-false-permits-absent",
                "require_exists FALSE permits an already-absent target",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.delete\n    TARGET: {absent}{}",
                        named("require_exists", "BOOLEAN", "FALSE", "FALSE")
                    ),
                ),
                &[REPORT],
                vec![succeeded(), attempt("changed", "FALSE".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/require-exists-true-rejects-absent",
                "require_exists TRUE rejects an absent target",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.delete\n    TARGET: {absent}{}",
                        named("require_exists", "BOOLEAN", "FALSE", "TRUE")
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
        }
        "core.download" | "core.upload" => {
            runs.push(default_binding(runners, row, "overwrite", "FALSE", doc()));
            let sum = lcl_spec::sha256::hex_digest(if op == "core.download" {
                b"remote"
            } else {
                b"content"
            });
            runs.push(bound_parameters(
                shipped,
                "binding/checksum",
                "bind the exact expected checksum",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("checksum", "STRING", "FALSE", &format!("\"sha256:{sum}\"")),
                    ),
                ),
                op,
                &[("checksum", &format!("\"sha256:{sum}\""))],
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/transfer-profile-role",
                "transfer",
                if op == "core.download" {
                    AddressClass::Uri
                } else {
                    AddressClass::Path
                },
                doc(),
            ));
            if op == "core.download" {
                runs.push(profile_role(
                    runners,
                    row,
                    "binding/source-profile-role",
                    "source",
                    AddressClass::Uri,
                    doc(),
                ));
                runs.push(on_files(
                    shipped,
                    "binding/destination-absent-unless-overwrite",
                    "the destination must be absent unless overwrite is TRUE, before the transfer",
                    &doc(),
                    &[REPORT, OTHER],
                    refused_before_effects(spec, "error.operation.precondition"),
                    &[REPORT, OTHER],
                ));
            } else {
                runs.push(on_files(
                    shipped,
                    "binding/destination-absent-unless-overwrite",
                    "the destination must be absent unless overwrite is TRUE",
                    &with_action(
                        row,
                        &format!(
                            "OPERATION: core.upload\n    TARGET: REF(data.path){}",
                            named("destination", "PATH", "TRUE", "REF(data.other)")
                        ),
                    ),
                    &[REPORT, OTHER],
                    refused_before_effects(spec, "error.operation.precondition"),
                    &[REPORT, OTHER],
                ));
            }
        }
        "core.execute" | "core.start" => {
            runs.push(default_binding(runners, row, "arguments", "[]", doc()));
            runs.push(default_binding(runners, row, "environment", "{}", doc()));
            if op == "core.execute" {
                runs.push(on_mock(
                    shipped,
                    "binding/executable-target",
                    "a STRING executable target binds non_graph mode",
                    &doc(),
                    Expectation::All(vec![succeeded(), attempt("mode", "non_graph".into())]),
                    MockHost::new(),
                    "default completion",
                ));
                runs.push(profile_role(
                    runners,
                    row,
                    "binding/execution-profile-selection",
                    "execution",
                    AddressClass::Material,
                    doc(),
                ));
                runs.push(shipped.execute(
                    "binding/only-registered-parameters",
                    "only arguments, working_directory, environment and timeout bind",
                    &with_action(
                        row,
                        &action_with(row, None, &named("shell", "STRING", "FALSE", "\"bash\"")),
                    ),
                    Expectation::Rejects("error.operation.parameter".into()),
                ));
            }
        }
        "core.stop" => runs.push(default_binding(
            runners,
            row,
            "force",
            "FALSE",
            reaching_source(row),
        )),
        "core.filter" | "core.select" => {
            let runner = with_pure(
                runners,
                vec![(
                    "member.positive",
                    Box::new(|member| {
                        Ok(lcl_runtime::Value::Boolean(
                            integer_of(member).is_some_and(|n| n > 1),
                        ))
                    }),
                )],
            );
            let source = with_declarations(
                row,
                &format!(
                    "OPERATION: {op}\n    TARGET: REF(data.list)\n    PARAMETER:\n        NAME: predicate\n        TYPE: REFERENCE[REF(member.positive)]\n        REQUIRED: TRUE\n        VALUE: REF(member.positive)"
                ),
                &pure_operation("member.positive", "INTEGER", "BOOLEAN", "Whether the member is greater than one."),
            );
            let items = if op == "core.filter" {
                attempt("items", "[3, 2]".into())
            } else {
                // Selection order and cardinality are unpinned; every member
                // returned is TRUE, which the effects group checks exactly.
                Expectation::NoDiagnostic("error.operation.precondition".into())
            };
            runs.push(on_mock(
                &runner,
                "binding/predicate-reference-contract",
                "a predicate REFERENCE resolves to a deterministic side-effect-free BOOLEAN kind.operation",
                &source,
                Expectation::All(vec![succeeded(), items]),
                MockHost::new(),
                "pure implementation member.positive: INTEGER -> member > 1",
            ));
            if op == "core.filter" {
                runs.push(shipped.execute(
                    "binding/set-input-rejected",
                    "SET input is error.type.mismatch before effects",
                    &with_declarations(
                        row,
                        &format!(
                            "OPERATION: core.filter\n    TARGET: REF(data.set){}",
                            named("predicate", "STRING", "TRUE", "\"item > 1\"")
                        ),
                        "\nDATA:\n    ID: data.set\n    TYPE: SET[INTEGER]\n    VALUE: [3, 1, 2]\n",
                    ),
                    Expectation::Diagnostic("error.type.mismatch".into()),
                ));
            }
        }
        "core.group" => {
            let runner = with_pure(
                runners,
                vec![("group.identity", Box::new(|member| Ok(member.clone())))],
            );
            runs.push(on_mock(
                &runner,
                "binding/key-reference-contract",
                "a key REFERENCE resolves to a deterministic side-effect-free kind.operation with one material result",
                &with_action(
                    row,
                    "OPERATION: core.group\n    TARGET: REF(data.list)\n    PARAMETER:\n        NAME: key\n        TYPE: REFERENCE[REF(group.identity)]\n        REQUIRED: TRUE\n        VALUE: REF(group.identity)",
                ),
                Expectation::All(vec![
                    succeeded(),
                    attempt("value", "[{items: [3], key: 3}, {items: [1], key: 1}, {items: [2], key: 2}]".into()),
                ]),
                MockHost::new(),
                "pure implementation group.identity: member -> member",
            ));
            runs.push(shipped.execute(
                "binding/set-input-rejected",
                "SET input is error.type.mismatch before effects",
                &with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.group\n    TARGET: REF(data.set){}",
                        named("key", "STRING", "TRUE", "\"tag\"")
                    ),
                    "\nDATA:\n    ID: data.set\n    TYPE: SET[INTEGER]\n    VALUE: [3, 1, 2]\n",
                ),
                Expectation::Diagnostic("error.type.mismatch".into()),
            ));
        }
        "core.inspect" => {
            runs.push(default_binding(runners, row, "depth", "1", doc()));
            runs.push(on_files(
                shipped,
                "binding/required-existing-target",
                "bind one required existing target: an absent target fails its precondition",
                &with_action(row, &retargeted(row, absent)),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
        }
        "core.memory_write" => {
            let pair = "\nDEFINE:\n    ID: type.pair\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: first\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    FIELD:\n        NAME: second\n        TYPE: INTEGER\n        REQUIRED: TRUE\n\
                        \nMEMORY:\n    ID: memory.pair\n    TYPE: OBJECT[REF(type.pair)]\n    SCOPE: REF(scope.task)\n    MODE: mode.read_write\n    VALUE:\n        first: 1\n        second: 2\n";
            runs.push(on_files(
                shipped,
                "binding/default/merge",
                "merge defaults to FALSE: the declared value replaces the stored value exactly",
                &doc(),
                &[REPORT],
                vec![succeeded(), attempt("changed", "TRUE".into())],
                &[REPORT],
            ));
            runs.push(shipped.execute(
                "binding/merge-false-type-match",
                "merge FALSE requires value to match the declared MEMORY type",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.memory_write\n    TARGET: REF(memory.notes){}",
                        named("value", "INTEGER", "TRUE", "3")
                    ),
                ),
                refused_before_effects_expectation(spec),
            ));
            runs.push(shipped.execute(
                "binding/merge-true-merged-object-type-match",
                "merge TRUE checks the computed merged OBJECT, not the patch alone, against the declared type",
                &with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.memory_write\n    TARGET: REF(memory.pair)\n    PARAMETER:\n        NAME: value\n        TYPE: OBJECT\n        REQUIRED: TRUE\n        VALUE:\n            second: 3{}",
                        named("merge", "BOOLEAN", "FALSE", "TRUE")
                    ),
                    pair,
                ),
                Expectation::All(vec![succeeded(), attempt("changed", "TRUE".into())]),
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/storage-profile-role",
                "storage",
                AddressClass::Memory,
                doc(),
            ));
        }
        "core.modify" => {
            runs.push(profile_role(
                runners,
                row,
                "binding/change-profile-role",
                "change",
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/expected-before-guard",
                "a matching expected_before guard binds and the change applies",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("expected_before", "STRING", "FALSE", "\"content\""),
                    ),
                ),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/report.txt", "changed")],
            ));
            runs.push(bound_parameters(
                shipped,
                "binding/selection",
                "bind the exact bounded selection",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("selection", "STRING", "FALSE", "\"line 1\""),
                    ),
                ),
                op,
                &[("selection", "\"line 1\"")],
            ));
        }
        "core.publish" => {
            runs.push(default_binding(runners, row, "replace", "FALSE", doc()));
            runs.push(profile_role(
                runners,
                row,
                "binding/publication-profile-role",
                "publication",
                AddressClass::Path,
                doc(),
            ));
            runs.push(bound_parameters(
                shipped,
                "binding/visibility",
                "bind the exact visibility",
                &doc(),
                op,
                &[("visibility", "\"public\"")],
            ));
            runs.push(on_files(
                shipped,
                "binding/destination-absent-unless-replace",
                "the destination must be absent unless replace is TRUE",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.publish\n    TARGET: REF(data.path){}{}",
                        named("destination", "PATH", "TRUE", "REF(data.other)"),
                        named("visibility", "STRING", "TRUE", "\"public\"")
                    ),
                ),
                &[REPORT, OTHER],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT, OTHER],
            ));
        }
        "core.rename" => {
            runs.push(default_binding(runners, row, "overwrite", "FALSE", doc()));
            runs.push(profile_role(
                runners,
                row,
                "binding/rename-profile-role",
                "rename",
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/destination-absent-unless-overwrite",
                "the renamed destination must be absent unless overwrite is TRUE",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", "\"other.txt\"")
                    ),
                ),
                &[REPORT, OTHER],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT, OTHER],
            ));
            runs.push(on_files(
                shipped,
                "binding/new-name-differs",
                "new_name must differ from the current name",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", "\"report.txt\"")
                    ),
                ),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
        }
        "core.sort" => {
            let sort = |target: &str, extra: &str| {
                with_action(
                    row,
                    &format!("OPERATION: core.sort\n    TARGET: {target}{extra}"),
                )
            };
            runs.push(shipped.execute(
                "binding/comparator-rejected",
                "comparator is unregistered: error.operation.parameter",
                &sort(
                    "REF(data.list)",
                    &named("comparator", "STRING", "FALSE", "\"numeric\""),
                ),
                Expectation::Rejects("error.operation.parameter".into()),
            ));
            runs.push(shipped.execute(
                "binding/stable-rejected",
                "stable is unregistered: error.operation.parameter",
                &sort(
                    "REF(data.list)",
                    &named("stable", "BOOLEAN", "FALSE", "TRUE"),
                ),
                Expectation::Rejects("error.operation.parameter".into()),
            ));
            runs.push(on_files(
                shipped,
                "binding/default/direction",
                "direction defaults to ascending",
                &sort("REF(data.list)", ""),
                &[REPORT],
                vec![succeeded(), attempt("items", "[1, 2, 3]".into())],
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "binding/list-target",
                "a LIST[T] target is accepted directly",
                &sort(
                    "REF(data.list)",
                    &named("direction", "ENUM", "FALSE", "descending"),
                ),
                &[REPORT],
                vec![succeeded(), attempt("items", "[3, 2, 1]".into())],
                &[REPORT],
            ));
            runs.push(shipped.execute(
                "binding/set-target",
                "a SET[T] target is accepted directly and returns LIST[T]",
                &with_declarations(
                    row,
                    "OPERATION: core.sort\n    TARGET: REF(data.set)",
                    "\nDATA:\n    ID: data.set\n    TYPE: SET[INTEGER]\n    VALUE: [3, 1, 2]\n",
                ),
                Expectation::All(vec![succeeded(), attempt("items", "[1, 2, 3]".into())]),
            ));
            let runner = with_pure(
                runners,
                vec![(
                    "key.negated",
                    Box::new(|member| {
                        integer_of(member)
                            .map(|n| {
                                lcl_runtime::Value::Integer(
                                    lcl_checker::numeric::Decimal::parse_integer(
                                        (-n).to_string().trim_start_matches('-'),
                                    )
                                    .map(|d| if n > 0 { d.negated() } else { d })
                                    .unwrap(),
                                )
                            })
                            .ok_or_else(|| "expected an INTEGER member".to_string())
                    }),
                )],
            );
            runs.push(on_mock(
                &runner,
                "binding/key-operation-reference-contract",
                "a key REFERENCE resolves DETERMINISTIC TRUE, SIDE_EFFECT FALSE, declared_state_only",
                &with_declarations(
                    row,
                    "OPERATION: core.sort\n    TARGET: REF(data.list)\n    PARAMETER:\n        NAME: key\n        TYPE: REFERENCE[REF(key.negated)]\n        REQUIRED: FALSE\n        VALUE: REF(key.negated)",
                    &pure_operation("key.negated", "INTEGER", "INTEGER", "Return the negated member."),
                ),
                Expectation::All(vec![succeeded(), attempt("items", "[3, 2, 1]".into())]),
                MockHost::new(),
                "pure implementation key.negated: member -> -member",
            ));
        }
        "core.test" => {
            let test = |extra: &str, target: Option<&str>| {
                let target = target
                    .map(|t| format!("\n    TARGET: {t}"))
                    .unwrap_or_default();
                with_action(row, &format!("OPERATION: core.test{target}{extra}"))
            };
            let passed = |label: &str, clause: &str, source: String| {
                shipped.execute(
                    label,
                    clause,
                    &source,
                    Expectation::All(vec![succeeded(), attempt("passed", "TRUE".into())]),
                )
            };
            let rejected = |label: &str, clause: &str, source: String| {
                shipped.execute(
                    label,
                    clause,
                    &source,
                    Expectation::Rejects("error.block.conditional_requirement".into()),
                )
            };
            // Which comparison form a site supplies is structure; whether its
            // TARGET is a material value is a type question, so the two halves
            // of the form rule are refused under their own identifiers.
            let mistyped = |label: &str, clause: &str, source: String| {
                shipped.execute(
                    label,
                    clause,
                    &source,
                    Expectation::Rejects("error.operation.parameter".into()),
                )
            };
            runs.push(passed(
                "binding/assertion-form",
                "the assertion comparison form",
                test(
                    &named("assertion", "BOOLEAN", "FALSE", "REF(data.number) == 3"),
                    None,
                ),
            ));
            runs.push(passed(
                "binding/expected-actual-form",
                "expected with the actual parameter under registered ==",
                test(
                    &format!(
                        "{}{}",
                        named("expected", "INTEGER", "FALSE", "3"),
                        named("actual", "INTEGER", "FALSE", "REF(data.number)")
                    ),
                    None,
                ),
            ));
            runs.push(passed(
                "binding/expected-target-form",
                "expected with a material-value TARGET as the actual source",
                test(
                    &named("expected", "INTEGER", "FALSE", "3"),
                    Some("REF(data.number)"),
                ),
            ));
            runs.push(rejected(
                "binding/target-alone-rejected",
                "TARGET alone is not a complete test",
                test("", Some("REF(data.number)")),
            ));
            runs.push(mistyped(
                "binding/target-with-actual-rejected",
                "a material-value TARGET cannot accompany actual",
                test(
                    &format!(
                        "{}{}",
                        named("expected", "INTEGER", "FALSE", "3"),
                        named("actual", "INTEGER", "FALSE", "3")
                    ),
                    Some("REF(data.number)"),
                ),
            ));
            runs.push(mistyped(
                "binding/target-with-assertion-rejected",
                "a material-value TARGET cannot accompany assertion",
                test(
                    &named("assertion", "BOOLEAN", "FALSE", "TRUE"),
                    Some("REF(data.number)"),
                ),
            ));
        }
        "core.validate" => {
            runs.push(shipped.execute(
                "binding/default/rules",
                "rules defaults to the empty LIST: no rule finding",
                &doc(),
                Expectation::All(vec![
                    succeeded(),
                    attempt("valid", "TRUE".into()),
                    attempt("errors", "[]".into()),
                ]),
            ));
            runs.push(shipped.execute(
                "binding/rules-reference-validate-declaration",
                "each rules REFERENCE resolves to an applicable VALIDATE declaration whose outcome is a finding",
                &with_declarations(
                    row,
                    "OPERATION: core.validate\n    TARGET: REF(data.number)\n    PARAMETER:\n        NAME: rules\n        TYPE: LIST[REFERENCE[REF(validate.small)]]\n        REQUIRED: FALSE\n        VALUE: [REF(validate.small)]",
                    "\nVALIDATE:\n    ID: validate.small\n    ASSERT: REF(data.number) < 2\n    REQUIRED: FALSE\n",
                ),
                Expectation::All(vec![
                    succeeded(),
                    attempt("valid", "FALSE".into()),
                    attempt("errors", "[error.validation.failed]".into()),
                ]),
            ));
            runs.push(shipped.execute(
                "binding/schema-reference-object-type",
                "a schema REFERENCE resolves to a kind.type whose resolved type is OBJECT and applies to the target",
                &with_action(
                    row,
                    "OPERATION: core.validate\n    TARGET: REF(data.tagged)\n    PARAMETER:\n        NAME: schema\n        TYPE: REFERENCE[REF(type.tagged)]\n        REQUIRED: FALSE\n        VALUE: REF(type.tagged)",
                ),
                Expectation::All(vec![succeeded(), attempt("valid", "TRUE".into())]),
            ));
        }
        "core.verify" => {
            let verify_action = |assertion: &str, extra: &str| {
                format!(
                    "OPERATION: core.verify\n    TARGET: REF(data.number){}{extra}",
                    named("assertion", "BOOLEAN", "TRUE", assertion)
                )
            };
            let verify =
                |assertion: &str, extra: &str| with_action(row, &verify_action(assertion, extra));
            let flag = "\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: FALSE\n";
            runs.push(on_mock(
                shipped,
                "binding/assertion-reference-invokes-nothing",
                "an assertion REFERENCE reads a declared BOOLEAN snapshot and invokes no operation or profile",
                &with_declarations(row, &format!("OPERATION: core.verify\n    TARGET: REF(data.number){}", named("assertion", "BOOLEAN", "TRUE", "REF(data.flag)")), flag),
                Expectation::All(vec![succeeded(), attempt("verified", "FALSE".into())]),
                MockHost::new(),
                "no host capability is expected to be requested",
            ));
            runs.push(shipped.execute(
                "binding/boolean-expression-assertion",
                "an inline BOOLEAN_EXPRESSION assertion",
                &verify("REF(data.number) == 3", ""),
                Expectation::All(vec![succeeded(), attempt("verified", "TRUE".into())]),
            ));
            runs.push(shipped.execute(
                "binding/reference-boolean-assertion",
                "a REFERENCE[BOOLEAN] assertion resolves one declared BOOLEAN value",
                &with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.verify\n    TARGET: REF(data.number){}",
                        named("assertion", "BOOLEAN", "TRUE", "REF(data.flag)")
                    ),
                    flag,
                ),
                Expectation::All(vec![succeeded(), attempt("verified", "FALSE".into())]),
            ));
            runs.push(shipped.execute(
                "binding/default/evidence",
                "evidence defaults to the empty LIST",
                &doc(),
                Expectation::All(vec![succeeded(), attempt("evidence", "[]".into())]),
            ));
            runs.push(shipped.execute(
                "binding/evidence-references",
                "the evidence LIST[REFERENCE[EVIDENCE]] binds exactly and is recorded",
                &with_declarations(
                    row,
                    &verify_action("REF(data.number) == 3", "\n    PARAMETER:\n        NAME: evidence\n        TYPE: LIST[REFERENCE[REF(evidence.note)]]\n        REQUIRED: FALSE\n        VALUE: [REF(evidence.note)]"),
                    "\nEVIDENCE:\n    ID: evidence.note\n    TYPE: STRING\n    VALUE: \"observed\"\n",
                ),
                Expectation::All(vec![succeeded(), attempt("evidence", "[REF(evidence.note)]".into())]),
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/verification-profile-role",
                "verification",
                AddressClass::Material,
                doc(),
            ));
        }
        "core.write" => {
            runs.push(default_binding(
                runners,
                row,
                "create_if_missing",
                "FALSE",
                doc(),
            ));
            runs.push(profile_role(
                runners,
                row,
                "binding/write-profile-role",
                "write",
                AddressClass::Path,
                doc(),
            ));
            runs.push(on_files(
                shipped,
                "binding/create-if-missing-policy",
                "create_if_missing TRUE permits creating an absent target",
                &with_action(
                    row,
                    &format!(
                        "OPERATION: core.write\n    TARGET: {absent}{}{}",
                        named("content", "STRING", "TRUE", "\"created\""),
                        named("create_if_missing", "BOOLEAN", "FALSE", "TRUE")
                    ),
                ),
                &[REPORT],
                vec![succeeded()],
                &[("/srv/data/absent.txt", "created"), REPORT],
            ));
        }
        _ => {}
    }
    runs
}

/// The engine assertions of a failure with `error.operation.precondition`
/// before effects.
fn refused_before_effects_expectation(spec: &SpecPackage) -> Expectation {
    Expectation::All(refused_before_effects(spec, "error.operation.precondition"))
}

// ---------------------------------------------------------------------------
// Operation-specific error paths and effects: the declared-state rows
// ---------------------------------------------------------------------------

/// The subject invocation completed and recorded these schema-local fields.
fn completed_with(fields: &[(&str, &str)]) -> Expectation {
    let mut parts = vec![succeeded()];
    parts.extend(
        fields
            .iter()
            .map(|(name, value)| attempt(name, value.to_string())),
    );
    Expectation::All(parts)
}

/// The subject invocation failed with `error` and changed nothing: no effect
/// and no request crossed the boundary.
fn failed_without_effects(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    error: &str,
) -> ExecutedCase {
    let mut host = MockHost::new();
    let mut case = runner.execute_on(label, clause, source, Expectation::Accepts, &mut host);
    // A failure raised before the subject is invoked has no attempt record;
    // either way no effect may have begun and no request may have crossed.
    let effects: Vec<String> = case
        .observed
        .invocations
        .iter()
        .filter(|i| i.declaration.as_deref() == Some("action.subject"))
        .filter_map(|i| i.result.as_ref())
        .map(|r| r.effect_state.to_string())
        .filter(|state| state != "none")
        .collect();
    case.expectation = Expectation::All(vec![
        Expectation::Diagnostic(error.into()),
        Expectation::Component(vec![
            ("requests".into(), "0".into()),
            ("subject_effect_states_other_than_none".into(), "[]".into()),
        ]),
    ]);
    case.observed.component = vec![
        ("requests".into(), host.requests().len().to_string()),
        (
            "subject_effect_states_other_than_none".into(),
            format!("{effects:?}"),
        ),
    ];
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case.verdict = crate::judge(&case.expectation, &case.observed);
    case
}

/// Declared-state rows: no request crosses the boundary and the subject
/// records no effect, in a completed invocation.
fn declared_state_only(runner: &Runner, label: &str, clause: &str, source: &str) -> ExecutedCase {
    resolved_axes(
        runner,
        label,
        clause,
        source,
        MockHost::new(),
        &[],
        "[none]",
    )
}

pub(super) fn specific(runners: &Runners<'_>, row: &Row, family: &str) -> Vec<ExecutedCase> {
    let op = row.operation;
    let shipped = runners.shipped;
    let spec = runners.spec;
    let mut runs = Vec::new();
    let errors = family == "errors";
    let effects = family == "effects";
    let dec = |id: &str, ty: &str, value: &str| {
        format!("\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n")
    };
    match (op, family) {
        ("core.calculate", _) => {
            let calc = |expression: &str, target: Option<&str>, declarations: &str| {
                let target = target
                    .map(|t| format!("\n    TARGET: {t}"))
                    .unwrap_or_default();
                with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.calculate{target}{}",
                        named("expression", "STRING", "TRUE", expression)
                    ),
                    declarations,
                )
            };
            let reject = |label: &str, clause: &str, source: String, error: &str| {
                failed_without_effects(shipped, label, clause, &source, error)
            };
            if errors {
                runs.push(shipped.execute(
                    "error/reference.kind",
                    "a REFERENCE expression must resolve to DEFINE kind.constant STRING; another kind is error.reference.kind",
                    &with_action(
                        row,
                        "OPERATION: core.calculate\n    PARAMETER:\n        NAME: expression\n        TYPE: REFERENCE[REF(data.text)]\n        REQUIRED: TRUE\n        VALUE: REF(data.text)",
                    ),
                    Expectation::Diagnostic("error.reference.kind".into()),
                ));
                // `expression_fragment_contract/environment`: "REF references
                // resolve in the enclosing document ... Bindings are immutable
                // snapshots; an unknown binding name produces
                // error.reference.unresolved." The identifier is registered at
                // the resolution stage and mirrored by the resolver alone, so
                // the reference this row's fragment reads is resolved where
                // that layer owns it: the `bindings` OBJECT the fragment's
                // environment is built from.
                //
                // A bare name *inside* the fragment string is a separate case
                // this build does not resolve statically; it is recorded as a
                // residual rather than answered at execution, because naming a
                // resolution-stage identifier in the runtime would mirror it in
                // a layer that does not own it.
                runs.push(shipped.execute(
                    "path/unresolved-expression-reference",
                    "a fragment environment whose binding reference resolves to no declaration is error.reference.unresolved",
                    &with_action(
                        row,
                        concat!(
                            "OPERATION: core.calculate",
                            "\n    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"amount + 1\"",
                            "\n    PARAMETER:\n        NAME: bindings\n        TYPE: OBJECT\n        REQUIRED: FALSE\n        VALUE:\n            amount: REF(data.absent)",
                        ),
                    ),
                    Expectation::All(vec![
                        Expectation::Rejects("error.reference.unresolved".into()),
                        Expectation::Attempts {
                            declaration: "action.subject".into(),
                            statuses: Vec::new(),
                        },
                    ]),
                ));
                runs.push(reject(
                    "path/division-operand",
                    "division of a non-numeric operand",
                    calc("\"1 / \\\"a\\\"\"", None, ""),
                    "error.operator.operand",
                ));
                runs.push(reject(
                    "path/division-by-zero",
                    "a zero denominator",
                    calc("\"1 / 0\"", None, ""),
                    "error.numeric.division_by_zero",
                ));
                runs.push(reject(
                    "path/non-terminating-quotient",
                    "an exact division without a finite base-10 result",
                    calc("\"1 / 3\"", None, ""),
                    "error.numeric.non_terminating",
                ));
                runs.push(reject(
                    "path/unit-mismatch",
                    "exact-unit mismatch between MEASURE operands",
                    calc(
                        "\"MEASURE(1, unit.meter) / MEASURE(1, unit.second)\"",
                        None,
                        "",
                    ),
                    "error.numeric.unit_mismatch",
                ));
                runs.push(reject(
                    "path/declared-bound",
                    "a value outside its declared FIELD bound is error.value.out_of_range",
                    calc("\"REF(data.bounded).ratio\"", None, "\nDEFINE:\n    ID: type.bounded\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: ratio\n        TYPE: DECIMAL\n        REQUIRED: TRUE\n        MAXIMUM: 1\n\nDATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF(type.bounded)]\n    VALUE:\n        ratio: 3 / 2\n"),
                    "error.value.out_of_range",
                ));
                runs.push(reject(
                    "path/host-capacity",
                    "an exact quotient larger than the host materializes is error.host.constraint",
                    calc(&format!("\"1 / 1.{}1\"", "0".repeat(4096)), None, ""),
                    "error.host.constraint",
                ));
                runs.push(reject(
                    "path/demanded-missing-operand",
                    "a consumed MISSING operand",
                    calc("\"target + 1\"", None, ""),
                    "error.required.missing",
                ));
                runs.push(reject(
                    "path/demanded-unknown-operand",
                    "an UNKNOWN operand yields UNKNOWN; the required material result rejects it",
                    calc(
                        "\"target + 1\"",
                        Some("REF(data.undetermined)"),
                        &dec("data.undetermined", "INTEGER", "UNKNOWN"),
                    ),
                    "error.value.unknown",
                ));
                runs.push(reject(
                    "path/required-unknown-result",
                    "a required final UNKNOWN result",
                    calc("\"UNKNOWN\"", None, ""),
                    "error.value.unknown",
                ));
            }
            if effects {
                runs.push(reject(
                    "precondition/0",
                    "precondition: expression and bindings type-check",
                    calc("\"1 + \\\"a\\\"\"", None, ""),
                    "error.operator.operand",
                ));
                runs.push(on_files(
                    shipped,
                    "postcondition/0",
                    "postcondition: the result equals the exact expression semantics",
                    &calc("\"2 * 3 + 1\"", None, ""),
                    &[REPORT],
                    vec![completed_with(&[("value", "7")])],
                    &[REPORT],
                ));
            }
        }
        ("core.compare", _) => {
            let compare = |target: &str,
                           against: (&str, &str),
                           criteria: Option<&str>,
                           declarations: &str| {
                let criteria = criteria
                    .map(|c| named("criteria", "STRING", "FALSE", c))
                    .unwrap_or_default();
                with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.compare\n    TARGET: {target}{}{criteria}",
                        named("against", against.0, "TRUE", against.1)
                    ),
                    declarations,
                )
            };
            let unknown = dec("data.undetermined", "INTEGER", "UNKNOWN");
            let missing = dec("data.nums", "LIST[INTEGER]", "[1, 2]");
            if errors {
                runs.push(failed_without_effects(
                    shipped,
                    "path/matches-resource-limit",
                    "MATCHES resource exhaustion is error.pattern.resource_limit",
                    &compare(
                        "\"a\"",
                        ("REGEX", "REGEX(\"a{200000}\")"),
                        Some("\"MATCHES\""),
                        "",
                    ),
                    "error.pattern.resource_limit",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/non-equality-missing",
                    "a non-==/!= criterion encountering MISSING",
                    &compare(
                        "REF(data.nums)[5]",
                        ("INTEGER", "2"),
                        Some("\"<\""),
                        &missing,
                    ),
                    "error.required.missing",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/propagated-unknown",
                    "a criterion result remaining UNKNOWN",
                    &compare(
                        "REF(data.undetermined)",
                        ("INTEGER", "2"),
                        Some("\"<\""),
                        &unknown,
                    ),
                    "error.value.unknown",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/unsupported-operator-operands",
                    "unsupported registered-operator operands",
                    &compare("REF(data.number)", ("STRING", "\"a\""), Some("\"<\""), ""),
                    "error.operator.operand",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/type-mismatch",
                    "a criteria REFERENCE resolving to a value of neither admitted form",
                    &with_action(
                        row,
                        &format!(
                            "OPERATION: core.compare\n    TARGET: REF(data.number){}\n    PARAMETER:\n        NAME: criteria\n        TYPE: REFERENCE[REF(data.list)]\n        REQUIRED: FALSE\n        VALUE: REF(data.list)",
                            named("against", "INTEGER", "TRUE", "3")
                        ),
                    ),
                    "error.type.mismatch",
                ));
            }
            if effects {
                runs.push(failed_without_effects(
                    shipped,
                    "criteria/non-equality-missing",
                    "a non-==/!= criterion encountering MISSING uses error.required.missing",
                    &compare(
                        "REF(data.nums)[5]",
                        ("INTEGER", "2"),
                        Some("\">=\""),
                        &missing,
                    ),
                    "error.required.missing",
                ));
                runs.push(on_files(
                    shipped,
                    "criteria/omitted-strict-equality",
                    "omitted criteria is registered == strict equality: MISSING equals itself",
                    &compare("REF(data.nums)[5]", ("INTEGER", "2"), None, &missing),
                    &[REPORT],
                    vec![completed_with(&[("value", "FALSE")])],
                    &[REPORT],
                ));
                runs.push(on_files(
                    shipped,
                    "criteria/supplied-registered-rules",
                    "a supplied criterion follows its registered operator: CONTAINS",
                    &compare("REF(data.list)", ("INTEGER", "1"), Some("\"CONTAINS\""), ""),
                    &[REPORT],
                    vec![completed_with(&[("value", "TRUE")])],
                    &[REPORT],
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "criteria/unknown-result-value-unknown",
                    "a criterion result remaining UNKNOWN uses error.value.unknown",
                    &compare(
                        "REF(data.undetermined)",
                        ("INTEGER", "2"),
                        Some("\">\""),
                        &unknown,
                    ),
                    "error.value.unknown",
                ));
            }
        }
        ("core.filter" | "core.select", _) => {
            let predicate_ref = |id: &str| {
                format!("\n    PARAMETER:\n        NAME: predicate\n        TYPE: REFERENCE[REF({id})]\n        REQUIRED: TRUE\n        VALUE: REF({id})")
            };
            let with_predicate = |predicate: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!("OPERATION: {op}\n    TARGET: REF(data.list){predicate}"),
                    declarations,
                )
            };
            let fragment = |text: &str| named("predicate", "STRING", "TRUE", text);
            let positive = pure_operation(
                "member.positive",
                "INTEGER",
                "BOOLEAN",
                "Whether the member is greater than one.",
            );
            let pure_runner = || {
                with_pure(
                    runners,
                    vec![(
                        "member.positive",
                        Box::new(|m: &lcl_runtime::Value| {
                            Ok(lcl_runtime::Value::Boolean(
                                integer_of(m).is_some_and(|n| n > 1),
                            ))
                        }) as lcl_stdlib::PureOperation,
                    )],
                )
            };
            if errors {
                runs.push(failed_without_effects(
                    shipped,
                    "error/operator.operand",
                    "a predicate with unsupported operator operands",
                    &with_predicate(&fragment("\"item > \\\"a\\\"\""), ""),
                    "error.operator.operand",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "error/reference.kind",
                    "a predicate REFERENCE resolving to a declaration other than kind.operation",
                    &with_predicate(&predicate_ref("data.number"), ""),
                    "error.reference.kind",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/missing-predicate",
                    "a predicate result of MISSING",
                    &with_predicate(&fragment("\"MISSING\""), ""),
                    "error.required.missing",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/unknown-predicate",
                    "a predicate result of UNKNOWN",
                    &with_predicate(&fragment("\"UNKNOWN\""), ""),
                    "error.value.unknown",
                ));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("member.failing", Box::new(|_: &lcl_runtime::Value| Err("the predicate implementation cannot evaluate this member".to_string())) as lcl_stdlib::PureOperation)]),
                    "path/referenced-predicate-error-union",
                    "an applicable error of the referenced predicate operation is unioned with the row's own",
                    &with_predicate(&predicate_ref("member.failing"), &pure_operation("member.failing", "INTEGER", "BOOLEAN", "A predicate whose implementation fails.")),
                    "error.operation.precondition",
                ));
                if op == "core.filter" {
                    runs.push(failed_without_effects(
                        shipped,
                        "path/set-target-type-mismatch",
                        "a SET target is error.type.mismatch before effects",
                        &with_declarations(
                            row,
                            &format!(
                                "OPERATION: core.filter\n    TARGET: REF(data.set){}",
                                fragment("\"item > 1\"")
                            ),
                            &dec("data.set", "SET[INTEGER]", "[3, 1, 2]"),
                        ),
                        "error.type.mismatch",
                    ));
                    runs.push(shipped.execute(
                        "path/unresolved-predicate-reference",
                        "a predicate REFERENCE that does not resolve exactly once",
                        &with_predicate(&predicate_ref("member.absent"), ""),
                        Expectation::Rejects("error.reference.unresolved".into()),
                    ));
                    runs.push(failed_without_effects(
                        &pure_runner(),
                        "precondition/predicate-contract",
                        "a predicate kind.operation whose PARAMETER does not accept the member type fails its contract precondition",
                        &with_predicate(&predicate_ref("member.textual"), &pure_operation("member.textual", "STRING", "BOOLEAN", "A predicate over STRING members.")),
                        "error.operation.precondition",
                    ));
                } else {
                    runs.push(failed_without_effects(
                        &pure_runner(),
                        "error/operation.precondition",
                        "predicate accepts the member type: a STRING-parameter predicate over INTEGER members",
                        &with_predicate(&predicate_ref("member.textual"), &pure_operation("member.textual", "STRING", "BOOLEAN", "A predicate over STRING members.")),
                        "error.operation.precondition",
                    ));
                }
            }
            if effects {
                runs.push(declared_state_only(
                    &pure_runner(),
                    "axes/declared-state-only",
                    "a predicate REFERENCE invocation resolves declared_state_only and none",
                    &with_predicate(&predicate_ref("member.positive"), &positive),
                ));
                runs.push(failed_without_effects(
                    &pure_runner(),
                    "axes/predicate-adding-axis-rejected",
                    "a predicate reference that would add a dependency is rejected",
                    &with_predicate(
                        &predicate_ref("member.remote"),
                        "\nDEFINE:\n    ID: member.remote\n    KIND: kind.operation\n    MEANING: \"A predicate that reads the host.\"\n    SIDE_EFFECT: FALSE\n    DEPENDENCY: [host]\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: BOOLEAN\n",
                    ),
                    "error.operation.precondition",
                ));
                if op == "core.filter" {
                    runs.push(on_files(
                        shipped,
                        "result/all-true-members-in-source-order",
                        "all and only TRUE members in exact LIST source order",
                        &with_declarations(
                            row,
                            &format!(
                                "OPERATION: core.filter\n    TARGET: REF(data.members){}",
                                fragment("\"item > 1\"")
                            ),
                            &dec("data.members", "LIST[INTEGER]", "[2, 1, 3, 2]"),
                        ),
                        &[REPORT],
                        vec![completed_with(&[("items", "[2, 3, 2]"), ("count", "3")])],
                        &[REPORT],
                    ));
                } else {
                    let (mut host, source) = (
                        MockHost::new(),
                        with_declarations(
                            row,
                            &format!(
                                "OPERATION: core.select\n    TARGET: REF(data.members){}",
                                fragment("\"item > 1\"")
                            ),
                            &dec("data.members", "LIST[INTEGER]", "[2, 1, 3, 2]"),
                        ),
                    );
                    let observed =
                        shipped.run_on(&source, &lcl_resolver::MemoryProvider::new(), &mut host);
                    let items: Vec<String> = observed
                        .invocations
                        .iter()
                        .filter(|i| i.declaration.as_deref() == Some("action.subject"))
                        .filter_map(|i| i.result.as_ref())
                        .filter_map(|r| match r.field("items") {
                            Some(lcl_runtime::Value::List(items)) => {
                                Some(items.iter().map(ToString::to_string).collect())
                            }
                            _ => None,
                        })
                        .next()
                        .unwrap_or_default();
                    // Occurrences: 2@0 TRUE, 1@1 FALSE, 3@2 TRUE, 2@3 TRUE.
                    let true_counts = [("2", 2usize), ("3", 1)];
                    let checks = [
                        ("cardinality-bounds", items.len() <= 3),
                        (
                            "no-repeated-occurrence",
                            true_counts
                                .iter()
                                .all(|(v, n)| items.iter().filter(|i| i == v).count() <= *n),
                        ),
                        (
                            "true-members-only",
                            items.iter().all(|i| i == "2" || i == "3"),
                        ),
                    ];
                    for (name, holds) in checks {
                        let mut case = shipped.execute(
                            &format!("result/{name}"),
                            "core.select nondeterministic selection bounds",
                            &source,
                            Expectation::Accepts,
                        );
                        case.expectation = Expectation::All(vec![
                            succeeded(),
                            Expectation::Component(vec![("holds".into(), "true".into())]),
                        ]);
                        case.observed.component = vec![("holds".into(), holds.to_string())];
                        case.observed.input_evidence.push(format!(
                            "selected items {items:?}; TRUE occurrences: 2 twice, 3 once"
                        ));
                        case.verdict = crate::judge(&case.expectation, &case.observed);
                        runs.push(case);
                    }
                }
            }
        }
        ("core.group", _) => {
            let key_ref = |id: &str| {
                format!("\n    PARAMETER:\n        NAME: key\n        TYPE: REFERENCE[REF({id})]\n        REQUIRED: TRUE\n        VALUE: REF({id})")
            };
            let group = |target: &str, key: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!("OPERATION: core.group\n    TARGET: {target}{key}"),
                    declarations,
                )
            };
            let identity = || {
                with_pure(
                    runners,
                    vec![(
                        "group.identity",
                        Box::new(|m: &lcl_runtime::Value| Ok(m.clone()))
                            as lcl_stdlib::PureOperation,
                    )],
                )
            };
            let words = dec(
                "data.words",
                "LIST[STRING]",
                "[\"beta\", \"alpha\", \"blue\"]",
            );
            let initial = || {
                with_pure(
                    runners,
                    vec![(
                        "key.initial",
                        Box::new(|m: &lcl_runtime::Value| match m {
                            lcl_runtime::Value::Text(t) => Ok(lcl_runtime::Value::Text(
                                t.chars().next().map(String::from).unwrap_or_default(),
                            )),
                            other => Err(format!("expected STRING, found {}", other.family())),
                        }) as lcl_stdlib::PureOperation,
                    )],
                )
            };
            let initial_decl = pure_operation(
                "key.initial",
                "STRING",
                "STRING",
                "Return the first character of the member.",
            );
            if errors {
                runs.push(failed_without_effects(
                    shipped,
                    "error/reference.kind",
                    "a key REFERENCE resolving to a declaration other than kind.operation",
                    &group("REF(data.list)", &key_ref("data.number"), ""),
                    "error.reference.kind",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/missing-key",
                    "a key result of MISSING for a member",
                    &group("REF(data.maybe)", &named("key", "STRING", "TRUE", "\"note\""), "\nDEFINE:\n    ID: type.noted\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: name\n        TYPE: STRING\n        REQUIRED: TRUE\n    FIELD:\n        NAME: note\n        TYPE: STRING\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.bare\n    TYPE: OBJECT[REF(type.noted)]\n    VALUE:\n        name: \"bare\"\n\nDATA:\n    ID: data.maybe\n    TYPE: LIST[OBJECT[REF(type.noted)]]\n    VALUE: [REF(data.bare)]\n"),
                    "error.required.missing",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/unknown-key",
                    "a key result of UNKNOWN for a member",
                    &group("REF(data.maybe)", &named("key", "STRING", "TRUE", "\"note\""), "\nDEFINE:\n    ID: type.noted\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: note\n        TYPE: STRING\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.unsure\n    TYPE: OBJECT[REF(type.noted)]\n    VALUE:\n        note: UNKNOWN\n\nDATA:\n    ID: data.maybe\n    TYPE: LIST[OBJECT[REF(type.noted)]]\n    VALUE: [REF(data.unsure)]\n"),
                    "error.value.unknown",
                ));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("key.failing", Box::new(|_: &lcl_runtime::Value| Err("the key implementation cannot evaluate this member".to_string())) as lcl_stdlib::PureOperation)]),
                    "path/referenced-key-operation-error-union",
                    "an applicable error of the referenced key operation is unioned with the row's own",
                    &group("REF(data.list)", &key_ref("key.failing"), &pure_operation("key.failing", "INTEGER", "INTEGER", "A key operation whose implementation fails.")),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/set-target-type-mismatch",
                    "a SET target is error.type.mismatch before effects",
                    &group(
                        "REF(data.set)",
                        &named("key", "STRING", "TRUE", "\"tag\""),
                        &dec("data.set", "SET[INTEGER]", "[3, 1, 2]"),
                    ),
                    "error.type.mismatch",
                ));
                runs.push(shipped.execute(
                    "path/unresolved-key-reference",
                    "a key REFERENCE that does not resolve exactly once",
                    &group("REF(data.list)", &key_ref("key.absent"), ""),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
                runs.push(failed_without_effects(
                    &identity(),
                    "precondition/key-contract",
                    "a malformed STRING key path is error.operation.precondition",
                    &group(
                        "REF(data.objects)",
                        &named("key", "STRING", "TRUE", "\"tag..name\""),
                        "",
                    ),
                    "error.operation.precondition",
                ));
            }
            if effects {
                runs.push(declared_state_only(
                    &identity(),
                    "axes/declared-state-only",
                    "a key REFERENCE invocation resolves declared_state_only and none",
                    &group("REF(data.list)", &key_ref("group.identity"), ""),
                ));
                runs.push(failed_without_effects(
                    &identity(),
                    "axes/key-adding-axis-rejected",
                    "a key reference that would add a dependency is rejected",
                    &group(
                        "REF(data.list)",
                        &key_ref("key.remote"),
                        "\nDEFINE:\n    ID: key.remote\n    KIND: kind.operation\n    MEANING: \"A key that reads the host.\"\n    SIDE_EFFECT: FALSE\n    DEPENDENCY: [host]\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: INTEGER\n",
                    ),
                    "error.operation.precondition",
                ));
                let grouped =
                    "[{items: [\"beta\", \"blue\"], key: \"b\"}, {items: [\"alpha\"], key: \"a\"}]";
                for (label, clause) in [
                    (
                        "result/groups-by-first-key",
                        "groups follow first key occurrence",
                    ),
                    (
                        "result/partition-exactly-once",
                        "every member occurrence appears in exactly one items LIST",
                    ),
                    (
                        "result/source-order-within-group",
                        "members retain LIST source order within each group",
                    ),
                ] {
                    runs.push(on_mock(
                        &initial(),
                        label,
                        clause,
                        &group(
                            "REF(data.words)",
                            &key_ref("key.initial"),
                            &format!("{words}{initial_decl}"),
                        ),
                        completed_with(&[("value", grouped)]),
                        MockHost::new(),
                        "pure implementation key.initial",
                    ));
                }
            }
        }
        ("core.sort", _) => {
            let key_ref = |id: &str| {
                format!("\n    PARAMETER:\n        NAME: key\n        TYPE: REFERENCE[REF({id})]\n        REQUIRED: FALSE\n        VALUE: REF({id})")
            };
            let sort = |target: &str, extra: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!("OPERATION: core.sort\n    TARGET: {target}{extra}"),
                    declarations,
                )
            };
            let initial_impl = || -> lcl_stdlib::PureOperation {
                Box::new(|m: &lcl_runtime::Value| match m {
                    lcl_runtime::Value::Text(t) => Ok(lcl_runtime::Value::Text(
                        t.chars().next().map(String::from).unwrap_or_default(),
                    )),
                    other => Err(format!("expected STRING, found {}", other.family())),
                })
            };
            let initial = || with_pure(runners, vec![("key.initial", initial_impl())]);
            let initial_decl = pure_operation(
                "key.initial",
                "STRING",
                "STRING",
                "Return the first character of the member.",
            );
            let words = dec(
                "data.words",
                "LIST[STRING]",
                "[\"beta\", \"alpha\", \"blue\"]",
            );
            if errors {
                runs.push(shipped.execute(
                    "path/comparator-unregistered",
                    "comparator is unregistered",
                    &sort(
                        "REF(data.list)",
                        &named("comparator", "STRING", "FALSE", "\"numeric\""),
                        "",
                    ),
                    Expectation::Rejects("error.operation.parameter".into()),
                ));
                runs.push(shipped.execute(
                    "path/stable-unregistered",
                    "stable is unregistered",
                    &sort(
                        "REF(data.list)",
                        &named("stable", "BOOLEAN", "FALSE", "TRUE"),
                        "",
                    ),
                    Expectation::Rejects("error.operation.parameter".into()),
                ));
                runs.push(shipped.execute(
                    "path/direction-outside-enum",
                    "direction outside ENUM[ascending|descending] is error.type.mismatch",
                    &sort(
                        "REF(data.list)",
                        &named("direction", "ENUM", "FALSE", "sideways"),
                        "",
                    ),
                    Expectation::Rejects("error.type.mismatch".into()),
                ));
                runs.push(failed_without_effects(
                    &initial(),
                    "path/equal-keys-for-distinct-set-members",
                    "distinct SET members producing equal keys",
                    &sort(
                        "REF(data.set)",
                        &key_ref("key.initial"),
                        &format!(
                            "{}{initial_decl}",
                            dec("data.set", "SET[STRING]", "[\"beta\", \"blue\"]")
                        ),
                    ),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(&initial(), "path/incompatible-key-results", "key values that are not mutually order-compatible", &sort("REF(data.mixed)", &named("key", "STRING", "FALSE", "\"value\""), "\nDEFINE:\n    ID: type.boxed\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: value\n        TYPE: STRING\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.one\n    TYPE: OBJECT[REF(type.boxed)]\n    VALUE:\n        value: \"a\"\n\nDEFINE:\n    ID: type.counted\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: value\n        TYPE: INTEGER\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.two\n    TYPE: OBJECT[REF(type.counted)]\n    VALUE:\n        value: 2\n\nDATA:\n    ID: data.mixed\n    TYPE: LIST[OBJECT]\n    VALUE: [REF(data.one), REF(data.two)]\n"), "error.operation.precondition"));
                runs.push(failed_without_effects(shipped, "path/missing-key", "a declared key value that is MISSING", &sort("REF(data.maybe)", &named("key", "STRING", "FALSE", "\"note\""), "\nDEFINE:\n    ID: type.noted\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: name\n        TYPE: STRING\n        REQUIRED: TRUE\n    FIELD:\n        NAME: note\n        TYPE: STRING\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.bare\n    TYPE: OBJECT[REF(type.noted)]\n    VALUE:\n        name: \"bare\"\n\nDATA:\n    ID: data.maybe\n    TYPE: LIST[OBJECT[REF(type.noted)]]\n    VALUE: [REF(data.bare)]\n"), "error.required.missing"));
                runs.push(failed_without_effects(shipped, "path/unknown-key", "a declared key value that is UNKNOWN", &sort("REF(data.maybe)", &named("key", "STRING", "FALSE", "\"note\""), "\nDEFINE:\n    ID: type.noted\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: note\n        TYPE: STRING\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.unsure\n    TYPE: OBJECT[REF(type.noted)]\n    VALUE:\n        note: UNKNOWN\n\nDATA:\n    ID: data.maybe\n    TYPE: LIST[OBJECT[REF(type.noted)]]\n    VALUE: [REF(data.unsure)]\n"), "error.value.unknown"));
                runs.push(failed_without_effects(shipped, "path/omitted-natural-order", "an omitted key over members without natural total order", &sort("REF(data.pair)", "", "\nDATA:\n    ID: data.zed\n    TYPE: OBJECT[REF(type.tagged)]\n    VALUE:\n        tag: \"z\"\n\nDATA:\n    ID: data.pair\n    TYPE: LIST[OBJECT[REF(type.tagged)]]\n    VALUE: [REF(data.zed), REF(data.tagged)]\n"), "error.operation.precondition"));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("key.failing", Box::new(|_: &lcl_runtime::Value| Err("the key implementation cannot evaluate this member".to_string())) as lcl_stdlib::PureOperation)]),
                    "path/referenced-key-operation-error-union",
                    "an applicable error of the referenced key operation is unioned with the row's own",
                    &sort("REF(data.list)", &key_ref("key.failing"), &pure_operation("key.failing", "INTEGER", "INTEGER", "A key operation whose implementation fails.")),
                    "error.operation.precondition",
                ));
                runs.push(shipped.execute(
                    "path/unresolved-key-reference",
                    "a key REFERENCE that does not resolve exactly once",
                    &sort("REF(data.list)", &key_ref("key.absent"), ""),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/wrong-kind-key-reference",
                    "a key REFERENCE resolving to a declaration other than kind.operation",
                    &sort("REF(data.list)", &key_ref("data.number"), ""),
                    "error.reference.kind",
                ));
                runs.push(failed_without_effects(
                    &initial(),
                    "precondition/incompatible-key-operation-signature",
                    "a key operation whose PARAMETER does not accept T",
                    &sort("REF(data.list)", &key_ref("key.initial"), &initial_decl),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    &initial(),
                    "precondition/invalid-key-operation-axes",
                    "a key operation declaring a dependency beyond declared_state_only",
                    &sort("REF(data.words)", &key_ref("key.remote"), &format!("{words}\nDEFINE:\n    ID: key.remote\n    KIND: kind.operation\n    MEANING: \"A key that reads the host.\"\n    SIDE_EFFECT: FALSE\n    DEPENDENCY: [host]\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: STRING\n")),
                    "error.operation.precondition",
                ));
                // The row's own error.operation.precondition trigger names "a
                // missing, ambiguous, incomplete, or out-of-bounds immutable
                // profile" for the key operation. A custom kind.operation
                // "selects no implementation profile", so those four words
                // apply to what stands in for one: the declared key contract
                // whose required properties the key constraint lists — "exactly
                // one PARAMETER accepting T, and exactly one RESULT of a
                // concrete registered ordered type" — and the installed
                // implementation that performs it.
                let key_declaration = |id: &str, body: &str| {
                    format!("{words}\nDEFINE:\n    ID: {id}\n    KIND: kind.operation\n    MEANING: \"A declared key.\"\n    SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n{body}")
                };
                runs.push(failed_without_effects(
                    shipped,
                    "precondition/key-operation-profile-missing",
                    "a declared key operation with no installed implementation",
                    &sort(
                        "REF(data.words)",
                        &key_ref("key.uninstalled"),
                        &key_declaration(
                            "key.uninstalled",
                            "    PARAMETER:\n        NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: STRING\n",
                        ),
                    ),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("key.two", initial_impl())]),
                    "precondition/key-operation-profile-ambiguous",
                    "a key operation declaring more than one PARAMETER, so which one accepts T is not determined",
                    &sort(
                        "REF(data.words)",
                        &key_ref("key.two"),
                        &key_declaration(
                            "key.two",
                            "    PARAMETER:\n        NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    PARAMETER:\n        NAME: fallback\n        TYPE: STRING\n        REQUIRED: FALSE\n    RESULT:\n        TYPE: STRING\n",
                        ),
                    ),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("key.resultless", initial_impl())]),
                    "precondition/key-operation-profile-incomplete",
                    "a key operation declaring no RESULT, so it states no ordered key type",
                    &sort(
                        "REF(data.words)",
                        &key_ref("key.resultless"),
                        &key_declaration(
                            "key.resultless",
                            "    PARAMETER:\n        NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n",
                        ),
                    ),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    &with_pure(runners, vec![("key.unordered", Box::new(|_: &lcl_runtime::Value| Ok(lcl_runtime::Value::Boolean(true))) as lcl_stdlib::PureOperation)]),
                    "precondition/key-operation-profile-out-of-bounds",
                    "a key operation whose RESULT is outside the registered ordered types",
                    &sort(
                        "REF(data.words)",
                        &key_ref("key.unordered"),
                        &key_declaration(
                            "key.unordered",
                            "    PARAMETER:\n        NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        TYPE: BOOLEAN\n",
                        ),
                    ),
                    "error.operation.precondition",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "precondition/malformed-property-path",
                    "a STRING key that is not one well-formed property_path",
                    &sort(
                        "REF(data.objects)",
                        &named("key", "STRING", "FALSE", "\"tag..name\""),
                        "",
                    ),
                    "error.operation.precondition",
                ));
            }
            if effects {
                runs.push(declared_state_only(
                    shipped,
                    "axes/declared-state-only",
                    "core.sort resolves declared_state_only and none",
                    &sort("REF(data.list)", "", ""),
                ));
                runs.push(on_mock(&initial(), "determinism/key-operation", "a validated deterministic key operation orders by its key, ties in source order", &sort("REF(data.words)", &key_ref("key.initial"), &format!("{words}{initial_decl}")), completed_with(&[("items", "[\"alpha\", \"beta\", \"blue\"]")]), MockHost::new(), "pure implementation key.initial"));
                runs.push(on_files(
                    shipped,
                    "determinism/ordered-type-rules",
                    "STRING natural order is by Unicode scalar value",
                    &sort(
                        "REF(data.names)",
                        "",
                        &dec("data.names", "LIST[STRING]", "[\"b\", \"B\", \"a\"]"),
                    ),
                    &[REPORT],
                    vec![completed_with(&[("items", "[\"B\", \"a\", \"b\"]")])],
                    &[REPORT],
                ));
                runs.push(on_files(shipped, "determinism/string-key-projection", "a STRING key projects the registered property path", &sort("REF(data.tags)", &named("key", "STRING", "FALSE", "\"tag\""), "\nDATA:\n    ID: data.zed\n    TYPE: OBJECT[REF(type.tagged)]\n    VALUE:\n        tag: \"z\"\n\nDATA:\n    ID: data.tags\n    TYPE: LIST[OBJECT[REF(type.tagged)]]\n    VALUE: [REF(data.zed), REF(data.tagged)]\n"), &[REPORT], vec![completed_with(&[("items", "[{tag: \"a\"}, {tag: \"z\"}]")])], &[REPORT]));
                runs.push(on_mock(
                    &initial(),
                    "result/distinct-keys-for-set-members",
                    "distinct SET members with distinct keys sort to a LIST",
                    &sort(
                        "REF(data.set)",
                        &key_ref("key.initial"),
                        &format!(
                            "{}{initial_decl}",
                            dec("data.set", "SET[STRING]", "[\"beta\", \"alpha\"]")
                        ),
                    ),
                    completed_with(&[("items", "[\"alpha\", \"beta\"]")]),
                    MockHost::new(),
                    "pure implementation key.initial",
                ));
                runs.push(on_mock(
                    &initial(),
                    "result/equal-keys-keep-source-position",
                    "equal-key LIST members retain source order, also descending",
                    &sort(
                        "REF(data.words)",
                        &format!(
                            "{}{}",
                            key_ref("key.initial"),
                            named("direction", "ENUM", "FALSE", "descending")
                        ),
                        &format!("{words}{initial_decl}"),
                    ),
                    completed_with(&[("items", "[\"beta\", \"blue\", \"alpha\"]")]),
                    MockHost::new(),
                    "pure implementation key.initial",
                ));
                runs.push(on_files(
                    shipped,
                    "result/list-output",
                    "result.collection items and the value are LIST[T]",
                    &sort(
                        "REF(data.set)",
                        "",
                        &dec("data.set", "SET[INTEGER]", "[3, 1, 2]"),
                    ),
                    &[REPORT],
                    vec![completed_with(&[("items", "[1, 2, 3]"), ("count", "3")])],
                    &[REPORT],
                ));
            }
        }
        ("core.return", _) => {
            let ret = |target: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!("OPERATION: core.return\n    TARGET: {target}"),
                    declarations,
                )
            };
            if errors {
                runs.push(failed_without_effects(shipped, "path/resolved-missing", "a REFERENCE resolving to MISSING", &ret("REF(output.pending)", "\nOUTPUT:\n    ID: output.pending\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n"), "error.required.missing"));
                runs.push(failed_without_effects(
                    shipped,
                    "path/resolved-unknown",
                    "a REFERENCE resolving to UNKNOWN",
                    &ret(
                        "REF(data.undetermined)",
                        &dec("data.undetermined", "INTEGER", "UNKNOWN"),
                    ),
                    "error.value.unknown",
                ));
            }
            if effects {
                runs.push(failed_without_effects(shipped, "precondition/0", "precondition: the target resolves to a material value other than MISSING or UNKNOWN", &ret("REF(output.pending)", "\nOUTPUT:\n    ID: output.pending\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n"), "error.required.missing"));
                runs.push(on_files(
                    shipped,
                    "postcondition/0",
                    "postcondition: the returned value equals the resolved target value",
                    &document(row),
                    &[REPORT],
                    vec![completed_with(&[("value", "3")])],
                    &[REPORT],
                ));
            }
        }
        _ => {}
    }
    let _ = spec;
    runs
}

// ---------------------------------------------------------------------------
// Operation-specific error paths and effects: host, lifecycle and store rows
// ---------------------------------------------------------------------------

/// How a scripted retry host answers the retry-safety query.
#[derive(Clone, Copy)]
enum SafetyEvidence {
    Missing,
    Unknown,
    Unsafe,
}

/// A deterministic host for retry-safety paths: every attempt of the wrapped
/// command fails with the given effect state, and the safety query answers
/// exactly as scripted.
struct RetrySafetyHost {
    effect: RecordState,
    evidence: SafetyEvidence,
    calls: usize,
}

impl lcl_runtime::Host for RetrySafetyHost {
    fn permits(&mut self, _request: &lcl_runtime::CapabilityRequest) -> lcl_runtime::Permission {
        lcl_runtime::Permission::Granted
    }
    fn invoke(&mut self, _request: &lcl_runtime::CapabilityRequest) -> CapabilityOutcome {
        self.calls += 1;
        let mut observation = Observation::none().with_effect(ObservedEffect {
            class: EffectClass::Process,
            state: self.effect,
            target: Some("/case/emit".into()),
            evidence: vec!["conformance: the command wrote to its stream before stopping".into()],
        });
        observation.host_limited = true;
        CapabilityOutcome::Failed {
            detail: "conformance: interrupted after a known effect".into(),
            observation,
        }
    }
    fn retry_evidence(
        &mut self,
        context: &lcl_runtime::capability::RetryContext,
    ) -> lcl_runtime::capability::RetryEvidence {
        use lcl_runtime::capability::RetryEvidence;
        match self.evidence {
            SafetyEvidence::Missing => RetryEvidence::Missing,
            SafetyEvidence::Unknown => RetryEvidence::Unknown,
            SafetyEvidence::Unsafe => RetryEvidence::Unsafe(Box::new(context.clone())),
        }
    }
}

/// The retry witness wrapping a command, whose attempts can leave effects.
fn retry_command_source() -> String {
    crate::witness_cases::retry_read(None)
        .replace("core.read", "core.execute")
        .replace(
            "TARGET: PATH(\"/case/retry.txt\")",
            "TARGET: PATH(\"/case/emit\")",
        )
        .replace("    OUTPUT: REF(output.payload)\n    RETRY:", "    RETRY:")
}

fn retry_safety_run(
    runner: &Runner,
    label: &str,
    clause: &str,
    effect: RecordState,
    evidence: SafetyEvidence,
    error: &str,
) -> ExecutedCase {
    let mut host = RetrySafetyHost {
        effect,
        evidence,
        calls: 0,
    };
    let source = retry_command_source();
    let mut case = runner.execute_on(
        label,
        clause,
        &source,
        Expectation::All(vec![
            Expectation::Diagnostic(error.into()),
            Expectation::Attempts {
                declaration: "action.read".into(),
                statuses: vec!["status.blocked".into()],
            },
            Expectation::NoDiagnostic("error.retry.exhausted".into()),
        ]),
        &mut host,
    );
    case.observed.input_evidence.push(format!(
        "host: scripted retry-safety host; every attempt fails after a {effect:?} process effect; safety evidence {}; attempts made {}",
        match evidence {
            SafetyEvidence::Missing => "missing",
            SafetyEvidence::Unknown => "UNKNOWN",
            SafetyEvidence::Unsafe => "proved unsafe for the exact context",
        },
        host.calls
    ));
    case
}

/// The retry witness over core.read, on a scripted host.
fn retry_scripted(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    outcomes: Vec<CapabilityOutcome>,
    deny: bool,
    expectation: Expectation,
) -> ExecutedCase {
    let mut host = MockHost::new().script("core.read", outcomes.clone());
    if deny {
        host = host.deny("core.read", "conformance: the host grants no access");
    }
    on_mock(
        runner,
        label,
        clause,
        source,
        expectation,
        host,
        &format!("core.read outcomes {outcomes:?}; deny={deny}"),
    )
}

fn unavailable() -> CapabilityOutcome {
    CapabilityOutcome::Unavailable("conformance: scripted read limitation".into())
}

fn completed_read() -> CapabilityOutcome {
    CapabilityOutcome::Completed(lcl_stdlib::schema::value(lcl_runtime::Value::Text(
        "complete".into(),
    )))
}

pub(super) fn lifecycle(runners: &Runners<'_>, row: &Row, family: &str) -> Vec<ExecutedCase> {
    let op = row.operation;
    let shipped = runners.shipped;
    let spec = runners.spec;
    let errors = family == "errors";
    let effects = family == "effects";
    let mut runs = Vec::new();
    let blocked = |error: &str| failed_with(spec, error, "pre_effect", "none");
    match op {
        "core.inspect" | "core.read" => {
            if errors {
                runs.push(on_mock(
                    shipped,
                    "path/host-constraint",
                    "a host limitation reading the target",
                    &document(row),
                    blocked("error.host.constraint"),
                    MockHost::new().unavailable(op, "conformance: no filesystem"),
                    "unavailable",
                ));
                runs.push(on_mock(
                    shipped,
                    "path/unauthorized-access",
                    "the host refuses access to the target",
                    &document(row),
                    blocked("error.permission.denied"),
                    MockHost::new().deny(op, "conformance: access refused"),
                    "deny",
                ));
                runs.push(on_files(
                    shipped,
                    "precondition/absent-target",
                    "precondition: the target exists",
                    &with_action(row, &retargeted(row, "PATH(\"/srv/data/absent.txt\")")),
                    &[REPORT],
                    refused_before_effects(spec, "error.operation.precondition"),
                    &[REPORT],
                ));
                if op == "core.read" {
                    runs.push(on_files(
                        shipped,
                        "precondition/unreadable-target",
                        "precondition: the target is readable content, not a directory",
                        &with_action(row, &retargeted(row, "PATH(\"/srv/data/tree\")")),
                        &[REPORT, ("/srv/data/tree/leaf.txt", "leaf")],
                        refused_before_effects(spec, "error.operation.precondition"),
                        &[REPORT, ("/srv/data/tree/leaf.txt", "leaf")],
                    ));
                    runs.push(on_files(
                        shipped,
                        "error/value.out_of_range",
                        "range bounds outside 0 <= start <= end <= length are error.value.out_of_range, never clipped",
                        &with_action(row, &action_with(row, None, "\n    PARAMETER:\n        NAME: range\n        TYPE: OBJECT\n        REQUIRED: FALSE\n        VALUE:\n            unit: \"scalar\"\n            start: 0\n            end: 99")),
                        &[REPORT],
                        refused_before_effects(spec, "error.value.out_of_range"),
                        &[REPORT],
                    ));
                }
            }
        }
        "core.ask" => {
            let ask = |question: &str, expected_type: &str, options: Option<&str>| {
                let options = options
                    .map(|o| named("options", "LIST[STRING]", "FALSE", o))
                    .unwrap_or_default();
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.ask\n    TARGET: REF(data.text){}{}{options}",
                        named("question", "STRING", "TRUE", question),
                        named("expected_type", "STRING", "TRUE", expected_type)
                    ),
                )
            };
            let missing_answer = |label: &str, clause: &str, source: String| {
                on_files(
                    shipped,
                    label,
                    clause,
                    &source,
                    &[REPORT],
                    vec![Expectation::Diagnostic("error.required.missing".into())],
                    &[REPORT],
                )
            };
            if errors {
                runs.push(on_mock(
                    shipped,
                    "path/host-constraint",
                    "no human responder is available",
                    &document(row),
                    blocked("error.host.constraint"),
                    MockHost::new().unavailable(op, "conformance: no responder"),
                    "unavailable",
                ));
                runs.push(on_mock(
                    shipped,
                    "path/unauthorized-request",
                    "the request is not authorized before the message",
                    &document(row),
                    blocked("error.permission.denied"),
                    MockHost::new().deny(op, "conformance: request refused"),
                    "deny",
                ));
                runs.push(missing_answer("path/missing-authoritative-answer", "no authoritative answer: the value stays MISSING and uses error.required.missing", ask("\"Which region?\"", "\"STRING\"", None)));
                runs.push(failed_without_effects(
                    shipped,
                    "path/option-incompatible-with-expected-type",
                    "an option incompatible with expected_type is error.type.mismatch before the question is put",
                    &with_action(row, &format!("OPERATION: core.ask\n    TARGET: REF(data.text){}{}\n    PARAMETER:\n        NAME: options\n        TYPE: LIST[INTEGER]\n        REQUIRED: FALSE\n        VALUE: [1, 2]", named("question", "STRING", "TRUE", "\"Which environment?\""), named("expected_type", "STRING", "TRUE", "\"STRING\""))),
                    "error.type.mismatch",
                ));
            }
            if effects {
                runs.push(on_files(
                    shipped,
                    "answer/compatible-with-expected-type",
                    "a non-MISSING answer compatible with expected_type is recorded",
                    &document(row),
                    &[REPORT],
                    vec![completed_with(&[("value", "\"staging\"")])],
                    &[REPORT],
                ));
                runs.push(missing_answer("answer/equals-listed-option", "an answer equal to no listed option is not a valid answer: MISSING and error.required.missing", ask("\"Which environment?\"", "\"STRING\"", Some("[\"production\"]"))));
                runs.push(missing_answer("answer/no-valid-answer-missing", "an answer incompatible with expected_type is not a valid answer: MISSING and error.required.missing", ask("\"Which environment?\"", "\"INTEGER\"", None)));
            }
        }
        "core.cancel" => {
            let cancel_other = with_action(
                row,
                &format!(
                    "OPERATION: core.cancel\n    TARGET: REF(action.other){}",
                    named("reason", "STRING", "TRUE", "\"the owner cancelled it\"")
                ),
            )
            .replacen(
                "ACTION: [REF(action.subject), REF(action.other)]",
                "ACTION: [REF(action.other), REF(action.subject)]",
                1,
            );
            let order = |label: &str, clause: &str| {
                shipped.execute(
                    label,
                    clause,
                    &cancel_other,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.execution.order".into()),
                        Expectation::Attempts {
                            declaration: "action.other".into(),
                            statuses: vec!["status.succeeded".into()],
                        },
                    ]),
                )
            };
            if errors {
                runs.push(shipped.execute(
                    "path/cancellation",
                    "the registered cancellation path: the invoking authority cancels, error.cancelled and status.cancelled",
                    &document(row),
                    Expectation::All(vec![Expectation::Diagnostic("error.cancelled".into()), Expectation::Terminal("status.cancelled".into())]),
                ));
                runs.push(shipped.execute(
                    "path/insufficient-authority",
                    "the invoking authority may not cancel the target: error.permission.denied",
                    &document(row).replacen("\nACTION:\n    ID: action.subject", "\nFORBID:\n    ID: forbid.cancel\n    OPERATION: core.cancel\n    TARGET: REF(task.subject)\n\nACTION:\n    ID: action.subject", 1),
                    Expectation::Rejects("error.permission.denied".into()),
                ));
                runs.push(order("path/status-disallows-cancel", "a current status whose allowed_next lacks status.cancelled uses error.execution.order"));
            }
            if effects {
                runs.push(shipped.execute("transition/allowed-cancel", "an active target whose allowed_next contains status.cancelled reaches status.cancelled", &document(row), Expectation::All(vec![succeeded(), Expectation::Terminal("status.cancelled".into())])));
                runs.push(order(
                    "transition/disallowed-cancel",
                    "a completed target does not permit status.cancelled",
                ));
                let observed = shipped.run(&document(row));
                let evidenced = observed
                    .invocations
                    .iter()
                    .filter(|i| i.declaration.as_deref() == Some("action.subject"))
                    .filter_map(|i| i.result.as_ref())
                    .flat_map(|r| r.observed_effects.iter().flat_map(|e| e.evidence.iter()))
                    .any(|evidence| evidence.contains("the owner cancelled it"));
                let mut case = shipped.execute(
                    "transition/reason-recorded",
                    "postcondition: the exact cancellation reason is evidenced",
                    &document(row),
                    Expectation::Accepts,
                );
                case.expectation = Expectation::All(vec![
                    succeeded(),
                    Expectation::Component(vec![("reason_evidenced".into(), "true".into())]),
                ]);
                case.observed.component = vec![("reason_evidenced".into(), evidenced.to_string())];
                case.verdict = crate::judge(&case.expectation, &case.observed);
                runs.push(case);
            }
        }
        "core.continue" => {
            let advance = || {
                crate::witness_cases::Probe::new(
                    crate::witness_cases::continue_read(true),
                    Expectation::All(vec![
                        Expectation::Accepts,
                        Expectation::Recovered("action.read".into()),
                        Expectation::Attempts {
                            declaration: "action.next".into(),
                            statuses: vec!["status.succeeded".into()],
                        },
                        Expectation::Output {
                            id: "output.payload".into(),
                            value: "9".into(),
                        },
                    ]),
                )
                .with_read_failures(1)
            };
            if errors {
                runs.push(on_mock(
                    shipped,
                    "error/operation.precondition",
                    "core.continue outside its selected handler context",
                    &document(row),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(
                    crate::witness_cases::Probe::new(
                        crate::witness_cases::continue_read(false),
                        Expectation::All(vec![
                            Expectation::Rejects("error.execution.order".into()),
                            Expectation::Attempts {
                                declaration: "action.next".into(),
                                statuses: vec![],
                            },
                        ]),
                    )
                    .with_read_failures(1)
                    .execute(
                        shipped,
                        "path/unhandled-event-or-absent-continuation",
                        "an absent continuation path uses error.execution.order",
                    ),
                );
            }
            if effects {
                runs.push(on_mock(
                    shipped,
                    "precondition/0",
                    "precondition: a selected handler is handling the current event",
                    &document(row),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(advance().execute(shipped, "postcondition/0", "on handler success the diagnostic is handled and execution resumes at the declared successor"));
                runs.push(advance().execute(shipped, "resolution/exact-invocation-axes", "core.continue resolves declared_state_only and state: only the read crosses the boundary"));
            }
        }
        "core.retry" => {
            let base = crate::witness_cases::retry_read(None);
            let attempts = |statuses: &[&str]| Expectation::Attempts {
                declaration: "action.read".into(),
                statuses: statuses.iter().map(|s| s.to_string()).collect(),
            };
            let limit_one_handler = base.replacen(
                "OPERATION: core.retry\n    LIMIT: 2",
                "OPERATION: core.retry\n    LIMIT: 1",
                1,
            );
            let no_block = base.replacen(
                "    RETRY:\n        LIMIT: 2\n        HANDLER: REF(handler.retry)\n",
                "",
                1,
            );
            let exhausted = |label: &str, clause: &str| {
                retry_scripted(
                    shipped,
                    label,
                    clause,
                    &base,
                    vec![unavailable(), unavailable(), unavailable()],
                    false,
                    Expectation::All(vec![
                        attempts(&["status.blocked", "status.blocked", "status.blocked"]),
                        Expectation::Diagnostic("error.retry.exhausted".into()),
                    ]),
                )
            };
            let in_order = |label: &str, clause: &str| {
                retry_scripted(
                    shipped,
                    label,
                    clause,
                    &base,
                    vec![unavailable(), unavailable(), completed_read()],
                    false,
                    Expectation::All(vec![
                        attempts(&["status.blocked", "status.blocked", "status.succeeded"]),
                        Expectation::NoDiagnostic("error.retry.exhausted".into()),
                    ]),
                )
            };
            let unequal = |label: &str| {
                retry_scripted(
                    shipped,
                    label,
                    "a resolved limit unequal to RETRY.LIMIT is error.operation.precondition",
                    &limit_one_handler,
                    vec![unavailable()],
                    false,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.operation.precondition".into()),
                        attempts(&["status.blocked"]),
                    ]),
                )
            };
            let unresolved = |label: &str| {
                shipped.execute(
                    label,
                    "the wrapped ACTION resolves exactly once before any retry decision",
                    &base.replacen(
                        "OPERATION: core.retry\n    LIMIT: 2",
                        "OPERATION: core.retry\n    TARGET: REF(action.absent)\n    LIMIT: 2",
                        1,
                    ),
                    Expectation::Rejects("error.reference.unresolved".into()),
                )
            };
            if errors {
                // "core.retry resolves the wrapped ACTION before retrying ... a
                // prohibited wrapped-ACTION reference cycle uses
                // error.reference.cycle." The wrapped ACTION here is the retrying
                // action itself.
                runs.push(shipped.execute(
                    "error/reference.cycle",
                    "a core.retry whose wrapped ACTION reference leads back to itself is a prohibited cycle",
                    &crate::fixtures::task_document(concat!(
                        "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n",
                        "\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n",
                        "\nACTION:\n    ID: action.looping\n    OPERATION: core.retry\n    TARGET: REF(action.looping)\n    ",
                        "PARAMETER:\n        NAME: limit\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 1\n",
                        "\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n",
                        "\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.looping)\n    SUCCESS: REF(success.case)\n",
                        "\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
                    )),
                    Expectation::All(vec![
                        Expectation::Rejects("error.reference.cycle".into()),
                        Expectation::Attempts {
                            declaration: "action.looping".into(),
                            statuses: Vec::new(),
                        },
                    ]),
                ));
                runs.push(retry_scripted(shipped, "error/execution.action", "a wrapped ACTION failing with error.execution.order-free action failure unions error.execution.action", &base, vec![MockHost::failed_before_effect("conformance: the read did not start")], false, Expectation::All(vec![Expectation::Diagnostic("error.execution.action".into()), attempts(&["status.failed"])])));
                runs.push(unresolved("path/action-resolution-before-inheritance"));
                runs.push(in_order(
                    "path/attempt-evidence-in-order",
                    "every attempt result is retained in attempt-index order",
                ));
                runs.push(exhausted(
                    "path/exhausted-after-all-attempts",
                    "error.retry.exhausted only after exactly 1 + LIMIT failed attempts",
                ));
                runs.push(retry_scripted(
                    shipped,
                    "path/false-when-preserves-failure",
                    "a FALSE WHEN preserves the prior failure without exhaustion",
                    &crate::witness_cases::retry_read(Some("FALSE")),
                    vec![unavailable()],
                    false,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.host.constraint".into()),
                        attempts(&["status.blocked"]),
                        Expectation::NoDiagnostic("error.retry.exhausted".into()),
                    ]),
                ));
                runs.push(retry_scripted(
                    shipped,
                    "path/missing-when-condition",
                    "a MISSING RETRY.WHEN uses error.required.missing without another attempt",
                    &crate::witness_cases::retry_read(Some("REF(output.flag)")).replacen("\nOUTPUT:\n    ID: output.payload", "\nOUTPUT:\n    ID: output.flag\n    TYPE: BOOLEAN\n    FORMAT: format.plain_text\n\nOUTPUT:\n    ID: output.payload", 1),
                    vec![unavailable()],
                    false,
                    Expectation::All(vec![Expectation::Diagnostic("error.required.missing".into()), attempts(&["status.blocked"])]),
                ));
                runs.push(retry_scripted(
                    shipped,
                    "path/unknown-when-condition",
                    "an UNKNOWN RETRY.WHEN uses error.value.unknown without another attempt",
                    &crate::witness_cases::retry_read(Some("UNKNOWN")),
                    vec![unavailable()],
                    false,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.value.unknown".into()),
                        attempts(&["status.blocked"]),
                    ]),
                ));
                runs.push(retry_safety_run(
                    shipped,
                    "path/missing-safety-proof",
                    "after known effects, missing safety proof uses error.required.missing",
                    RecordState::Partial,
                    SafetyEvidence::Missing,
                    "error.required.missing",
                ));
                runs.push(retry_safety_run(
                    shipped,
                    "path/unknown-safety-proof",
                    "after known effects, UNKNOWN safety proof uses error.value.unknown",
                    RecordState::Partial,
                    SafetyEvidence::Unknown,
                    "error.value.unknown",
                ));
                runs.push(retry_safety_run(
                    shipped,
                    "precondition/proved-unsafe-repetition",
                    "proved-unsafe repetition uses error.operation.precondition",
                    RecordState::Partial,
                    SafetyEvidence::Unsafe,
                    "error.operation.precondition",
                ));
                runs.push(retry_safety_run(
                    shipped,
                    "path/safety-blocked-attempt-is-not-exhaustion",
                    "a safety-blocked unmade attempt is not exhaustion",
                    RecordState::Partial,
                    SafetyEvidence::Missing,
                    "error.required.missing",
                ));
                runs.push(retry_scripted(shipped, "path/wrapped-action-error-union", "every applicable error of the wrapped ACTION is unioned: its permission refusal", &base, vec![], true, Expectation::All(vec![Expectation::Diagnostic("error.permission.denied".into()), attempts(&["status.failed"])])));
                runs.push(unequal("precondition/limit-unequal-retry-limit"));
                runs.push(retry_scripted(
                    shipped,
                    "precondition/no-retry-block",
                    "a wrapped ACTION with no RETRY block uses error.operation.precondition",
                    &no_block,
                    vec![unavailable()],
                    false,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.operation.precondition".into()),
                        attempts(&["status.blocked"]),
                    ]),
                ));
            }
            if effects {
                runs.push(on_mock(shipped, "precondition/0", "precondition: core.retry is valid only in the selected handler of the same ACTION invocation", &document(row), blocked("error.operation.precondition"), MockHost::new(), "default"));
                runs.push(unresolved("precondition/1"));
                runs.push(unequal("precondition/2"));
                runs.push(retry_safety_run(
                    shipped,
                    "precondition/3",
                    "precondition: after known effects exact evidence proves safe repetition",
                    RecordState::Partial,
                    SafetyEvidence::Missing,
                    "error.required.missing",
                ));
                runs.push(retry_safety_run(shipped, "precondition/4", "precondition: an indeterminate prior attempt is reconciled before another attempt", RecordState::Indeterminate, SafetyEvidence::Missing, "error.required.missing"));
                runs.push(exhausted(
                    "postcondition/0",
                    "postcondition: the attempt count never exceeds 1 + limit",
                ));
                runs.push(in_order(
                    "postcondition/1",
                    "postcondition: every attempt is evidenced in attempt-index order",
                ));
                runs.push(retry_scripted(
                    shipped,
                    "postcondition/3",
                    "postcondition: a successful attempt ends retrying",
                    &base,
                    vec![unavailable(), completed_read()],
                    false,
                    Expectation::All(vec![
                        attempts(&["status.blocked", "status.succeeded"]),
                        Expectation::NoDiagnostic("error.retry.exhausted".into()),
                        Expectation::Terminal("status.succeeded".into()),
                    ]),
                ));
                let mut host =
                    MockHost::new().script("core.read", vec![unavailable(), completed_read()]);
                let observed =
                    shipped.run_on(&base, &lcl_resolver::MemoryProvider::new(), &mut host);
                let axes: Vec<String> = host
                    .requests()
                    .iter()
                    .map(|r| {
                        format!(
                            "{} deps={} effects={}",
                            r.operation,
                            axis_set(&r.possible_dependencies, "declared_state_only"),
                            axis_set(&r.possible_effects, "none")
                        )
                    })
                    .collect();
                let mut case = shipped.execute(
                    "resolution/exact-invocation-axes",
                    "core.retry inherits exactly the wrapped ACTION's resolved axes and adds none",
                    &base,
                    Expectation::Accepts,
                );
                case.observed = observed;
                case.observed
                    .input_evidence
                    .push(format!("host: MockHost; requests {:?}", host.requests()));
                case.expectation = Expectation::All(vec![succeeded_read(), Expectation::Component(vec![("requests".into(), "[\"core.read deps=[host] effects=[none]\", \"core.read deps=[host] effects=[none]\"]".into())])]);
                case.observed.component = vec![("requests".into(), format!("{axes:?}"))];
                case.verdict = crate::judge(&case.expectation, &case.observed);
                runs.push(case);
                let mut case = retry_safety_run(shipped, "postcondition/2", "postcondition: aggregate failure phase and effects account for every attempt made", RecordState::Partial, SafetyEvidence::Missing, "error.required.missing");
                case.expectation = Expectation::All(vec![
                    Expectation::Attempts {
                        declaration: "action.read".into(),
                        statuses: vec!["status.blocked".into()],
                    },
                    Expectation::AttemptField {
                        declaration: "action.read".into(),
                        attempt: 0,
                        field: "failure_phase".into(),
                        value: "post_effect".into(),
                    },
                    Expectation::AttemptField {
                        declaration: "action.read".into(),
                        attempt: 0,
                        field: "effect_state".into(),
                        value: "partial".into(),
                    },
                ]);
                case.verdict = crate::judge(&case.expectation, &case.observed);
                runs.push(case);
            }
        }
        "core.stop" => {
            let process = || reaching_source(row);
            let stop_other =
                with_action(row, "OPERATION: core.stop\n    TARGET: REF(action.other)").replacen(
                    "ACTION: [REF(action.subject), REF(action.other)]",
                    "ACTION: [REF(action.other), REF(action.subject)]",
                    1,
                );
            let order = |label: &str, clause: &str| {
                shipped.execute(
                    label,
                    clause,
                    &stop_other,
                    Expectation::All(vec![
                        Expectation::Diagnostic("error.execution.order".into()),
                        Expectation::Attempts {
                            declaration: "action.other".into(),
                            statuses: vec!["status.succeeded".into()],
                        },
                    ]),
                )
            };
            if errors {
                runs.push(order("error/execution.order", "an internal target whose allowed_next lacks status.stopped uses error.execution.order"));
            }
            if effects {
                runs.push(on_mock(
                    shipped,
                    "precondition/0",
                    "precondition: the target is running or active; the host observes it is not",
                    &process(),
                    blocked("error.operation.precondition"),
                    MockHost::new().script(
                        op,
                        vec![CapabilityOutcome::Refused {
                            observation: lcl_runtime::capability::Observation::none(),
                            error: lcl_runtime::RuntimeError::OperationPrecondition,
                            cause: "target".into(),
                            detail: "conformance: the process is not running".into(),
                        }],
                    ),
                    "the host reports the target is not running",
                ));
                runs.push(order("precondition/1", "precondition: an internal target allows status.stopped in its current allowed_next"));
                runs.push(shipped.execute(
                    "postcondition/0",
                    "postcondition: an active internal target is stopped",
                    &document(row),
                    Expectation::All(vec![
                        succeeded(),
                        Expectation::Terminal("status.stopped".into()),
                    ]),
                ));
                runs.push(bound_parameters(
                    shipped,
                    "postcondition/1",
                    "postcondition: force is used only when TRUE",
                    &with_action(
                        row,
                        &format!(
                            "OPERATION: core.stop\n    TARGET: REF(data.command){}",
                            named("force", "BOOLEAN", "FALSE", "TRUE")
                        ),
                    ),
                    op,
                    &[("force", "TRUE")],
                ));
            }
        }
        "core.start" => {
            if effects {
                runs.push(on_mock(
                    shipped,
                    "precondition/0",
                    "precondition: the target is startable; the host observes it is not",
                    &document(row),
                    blocked("error.operation.precondition"),
                    MockHost::new().script(
                        op,
                        vec![CapabilityOutcome::Refused {
                            observation: lcl_runtime::capability::Observation::none(),
                            error: lcl_runtime::RuntimeError::OperationPrecondition,
                            cause: "target".into(),
                            detail: "conformance: not startable".into(),
                        }],
                    ),
                    "the host reports the target is not startable",
                ));
                runs.push(on_mock(
                    shipped,
                    "postcondition/0",
                    "postcondition: the target reaches running or ready state",
                    &document(row),
                    completed_with(&[("changed", "TRUE")]),
                    MockHost::new(),
                    "default completion",
                ));
            }
        }
        "core.send" => {
            if effects {
                runs.push(on_mock(shipped, "precondition/0", "precondition: recipient and content are authorized; a refusal precedes the message effect", &document(row), blocked("error.permission.denied"), MockHost::new().deny(op, "conformance: recipient not authorized"), "deny"));
                runs.push(on_files(
                    shipped,
                    "postcondition/0",
                    "postcondition: the delivery result and recipient are recorded",
                    &document(row),
                    &[REPORT],
                    vec![completed_with(&[
                        ("delivered", "TRUE"),
                        ("recipient", "URI(\"http://example.invalid/x\")"),
                    ])],
                    &[REPORT],
                ));
            }
        }
        "core.state_update" => {
            let update = |value: (&str, &str), extra: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.state_update\n    TARGET: REF(state.revision){}{extra}",
                        named("value", value.0, "TRUE", value.1)
                    ),
                    declarations,
                )
            };
            if errors {
                runs.push(on_mock(
                    shipped,
                    "error/operation.precondition",
                    "expected_before that does not match is error.operation.precondition",
                    &update(
                        ("INTEGER", "3"),
                        &named("expected_before", "INTEGER", "FALSE", "9"),
                        "",
                    ),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
            }
            if effects {
                runs.push(on_mock(
                    shipped,
                    "precondition/0",
                    "precondition: the STATE mode permits write",
                    &with_declarations(row, &format!("OPERATION: core.state_update\n    TARGET: REF(state.frozen){}", named("value", "INTEGER", "TRUE", "3")), "\nSTATE:\n    ID: state.frozen\n    TYPE: INTEGER\n    SCOPE: REF(scope.task)\n    MODE: mode.read_only\n    VALUE: 2\n"),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(on_mock(
                    shipped,
                    "precondition/1",
                    "precondition: the value type matches the declared STATE type",
                    &update(("STRING", "\"three\""), "", ""),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(on_mock(
                    shipped,
                    "precondition/2",
                    "precondition: expected_before matches when supplied",
                    &update(
                        ("INTEGER", "3"),
                        &named("expected_before", "INTEGER", "FALSE", "9"),
                        "",
                    ),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(store_read_back(shipped, row, "postcondition/0", "postcondition: the state equals the requested value", "OPERATION: core.state_update\n    TARGET: REF(state.revision)\n    PARAMETER:\n        NAME: value\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: 3", "", "REF(state.revision)", "3"));
            }
        }
        "core.memory_write" => {
            let pair = "\nDEFINE:\n    ID: type.pair\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: first\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    FIELD:\n        NAME: second\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    FIELD:\n        NAME: inner\n        TYPE: OBJECT[REF(type.inner)]\n        REQUIRED: FALSE\n\nDEFINE:\n    ID: type.inner\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: left\n        TYPE: INTEGER\n        REQUIRED: FALSE\n    FIELD:\n        NAME: right\n        TYPE: INTEGER\n        REQUIRED: FALSE\n\nMEMORY:\n    ID: memory.pair\n    TYPE: OBJECT[REF(type.pair)]\n    SCOPE: REF(scope.task)\n    MODE: mode.read_write\n    VALUE:\n        first: 1\n        second: 2\n        inner:\n            left: 1\n            right: 2\n";
            let write = |target: &str, value: &str, merge: bool| {
                format!(
                    "OPERATION: core.memory_write\n    TARGET: {target}\n    PARAMETER:\n        NAME: value\n        TYPE: OBJECT\n        REQUIRED: TRUE\n        VALUE:\n{value}{}",
                    if merge { named("merge", "BOOLEAN", "FALSE", "TRUE") } else { String::new() }
                )
            };
            if errors {
                runs.push(on_mock(
                    shipped,
                    "precondition/merge-current-not-object",
                    "merge TRUE requires the current MEMORY value to be OBJECT",
                    &with_declarations(
                        row,
                        &write("REF(memory.notes)", "            first: 5", true),
                        "",
                    ),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(on_mock(
                    shipped,
                    "precondition/merge-new-not-object",
                    "merge TRUE requires the new value to be OBJECT",
                    &with_declarations(
                        row,
                        &format!(
                            "OPERATION: core.memory_write\n    TARGET: REF(memory.pair){}{}",
                            named("value", "STRING", "TRUE", "\"flat\""),
                            named("merge", "BOOLEAN", "FALSE", "TRUE")
                        ),
                        pair,
                    ),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
                runs.push(on_mock(
                    shipped,
                    "precondition/merged-object-type-mismatch",
                    "merge TRUE requires the computed merged OBJECT to match the declared type",
                    &with_declarations(
                        row,
                        &write("REF(memory.pair)", "            second: \"two\"", true),
                        pair,
                    ),
                    blocked("error.operation.precondition"),
                    MockHost::new(),
                    "default",
                ));
            }
            if effects {
                let merged =
                    |label: &str, clause: &str, value: &str, merge: bool, expected: &str| {
                        store_read_back(
                            shipped,
                            row,
                            label,
                            clause,
                            &write("REF(memory.pair)", value, merge),
                            pair,
                            "REF(memory.pair)",
                            expected,
                        )
                    };
                runs.push(merged(
                    "merge/current-only-fields-preserved",
                    "merge TRUE preserves current-only top-level fields",
                    "            second: 3",
                    true,
                    "{first: 1, inner: {left: 1, right: 2}, second: 3}",
                ));
                runs.push(merged(
                    "merge/true-shallow-right-biased",
                    "merge TRUE lets new same-name fields win",
                    "            first: 7",
                    true,
                    "{first: 7, inner: {left: 1, right: 2}, second: 2}",
                ));
                runs.push(merged(
                    "merge/nested-objects-not-merged",
                    "merge TRUE never merges nested OBJECT values recursively",
                    "            inner:\n                left: 9",
                    true,
                    "{first: 1, inner: {left: 9}, second: 2}",
                ));
                runs.push(merged(
                    "merge/false-replaces",
                    "merge FALSE replaces the stored value exactly",
                    "            first: 5\n            second: 6",
                    false,
                    "{first: 5, second: 6}",
                ));
            }
        }
        "core.move" | "core.rename" if errors => {
            let precondition =
                |label: &str, clause: &str, action: String, seed: &[(&str, &str)]| {
                    on_files(
                        shipped,
                        label,
                        clause,
                        &with_action(row, &action),
                        seed,
                        refused_before_effects(spec, "error.operation.precondition"),
                        seed,
                    )
                };
            if op == "core.move" {
                runs.push(precondition(
                    "precondition/same-address",
                    "equal resolved source and destination addresses",
                    format!(
                        "OPERATION: core.move\n    TARGET: REF(data.path){}",
                        named("destination", "PATH", "TRUE", "REF(data.path)")
                    ),
                    &[REPORT],
                ));
            } else {
                let rename = |name: &str| {
                    format!(
                        "OPERATION: core.rename\n    TARGET: REF(data.path){}",
                        named("new_name", "STRING", "TRUE", name)
                    )
                };
                runs.push(precondition(
                    "precondition/disallowed-existing-destination",
                    "an existing renamed destination without overwrite",
                    rename("\"other.txt\""),
                    &[REPORT, OTHER],
                ));
                runs.push(precondition(
                    "precondition/illegal-new-name",
                    "a new_name carrying a path separator",
                    rename("\"a/b.txt\""),
                    &[REPORT],
                ));
                runs.push(precondition(
                    "precondition/same-name",
                    "a new_name equal to the current name",
                    rename("\"report.txt\""),
                    &[REPORT],
                ));
            }
        }
        _ => {}
    }
    runs
}

fn succeeded_read() -> Expectation {
    Expectation::Attempts {
        declaration: "action.read".into(),
        statuses: vec!["status.blocked".into(), "status.succeeded".into()],
    }
}

/// Write an engine-owned store, then read it back in a later sibling action.
#[allow(clippy::too_many_arguments)]
fn store_read_back(
    runner: &Runner,
    row: &Row,
    label: &str,
    clause: &str,
    action: &str,
    declarations: &str,
    read: &str,
    expected: &str,
) -> ExecutedCase {
    let source = with_declarations(row, action, declarations)
        .replacen("\nACTION:\n    ID: action.other\n    OPERATION: core.return\n    TARGET: REF(data.number)\n", &format!("\nACTION:\n    ID: action.other\n    OPERATION: core.return\n    TARGET: {read}\n"), 1);
    runner.execute(
        label,
        clause,
        &source,
        Expectation::All(vec![
            succeeded(),
            Expectation::AttemptField {
                declaration: "action.other".into(),
                attempt: 0,
                field: "value".into(),
                value: expected.into(),
            },
        ]),
    )
}

// ---------------------------------------------------------------------------
// Capability rows: fixture completions, address classes and derived category
// ---------------------------------------------------------------------------

/// One determinism resolution over an explicit profile set: the production
/// `ProfileCatalog::resolve_determinism`, with the exact profiles a case
/// installs.
fn determinism_run(
    runners: &Runners<'_>,
    label: &str,
    clause: &str,
    operation: &str,
    cases: Vec<(&str, Vec<Profile>, Option<Determinism>, &str)>,
) -> ExecutedCase {
    let catalog = lcl_capabilities::ProfileCatalog::load(runners.spec).expect("the catalog loads");
    let expected: Vec<(String, String)> = cases
        .iter()
        .map(|(name, _, _, expected)| (name.to_string(), expected.to_string()))
        .collect();
    let actual: Vec<(String, String)> = cases
        .iter()
        .map(|(name, profiles, graph, _)| {
            let selected: Vec<&Profile> = profiles.iter().collect();
            (
                name.to_string(),
                if catalog
                    .resolve_determinism(operation, &selected, *graph)
                    .is_deterministic()
                {
                    "deterministic".to_string()
                } else {
                    "nondeterministic".to_string()
                },
            )
        })
        .collect();
    let input = format!(
        "ProfileCatalog::resolve_determinism({operation}, ...) over {:?}",
        cases
            .iter()
            .map(|(name, profiles, graph, _)| format!(
                "{name}: profiles={profiles:?} graph={graph:?}"
            ))
            .collect::<Vec<_>>()
    );
    let expectation = Expectation::Component(expected);
    let observed = crate::Observed {
        component: actual,
        input_evidence: vec![input.clone()],
        ..crate::Observed::default()
    };
    let verdict = crate::judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: clause.into(),
        source: input,
        expectation,
        observed,
        verdict,
    }
}

/// One fixture profile for a row and role, with an explicit final category.
fn category_profile(
    operation: &str,
    role: &str,
    category: Determinism,
    deps: &[Dependency],
    effects: &[Effect],
) -> Profile {
    Profile::builder(operation, Role::new(role), "conformance.fixture.category", "1")
        .serving(TargetClass::Any)
        .determinism(
            category,
            "conformance fixture: the exact declared inputs and one immutable implementation version fix the result",
        )
        .axes(lcl_capabilities::profile::axes(deps, effects))
        .resolving("the row's own invocation rule")
}

/// The shipped profiles with every profile of one row and role replaced by
/// `profile`, so exactly one candidate serves the invocation.
fn replacing(operation: &str, role: &str, profile: Profile) -> Vec<Profile> {
    Runner::shipped_profiles()
        .into_iter()
        .filter(|p| !(p.operation_id == operation && p.profile_role.as_str() == role))
        .chain([profile])
        .collect()
}

/// A scripted host refusal naming the row's own registered identifier.
fn host_refuses(
    operation: &str,
    error: lcl_runtime::RuntimeError,
    cause: &str,
    detail: &str,
) -> MockHost {
    MockHost::new().script(
        operation,
        vec![CapabilityOutcome::Refused {
            observation: lcl_runtime::capability::Observation::none(),
            error,
            cause: cause.into(),
            detail: detail.into(),
        }],
    )
}

pub(super) fn capability(runners: &Runners<'_>, row: &Row, family: &str) -> Vec<ExecutedCase> {
    use lcl_runtime::Value;
    let op = row.operation;
    let spec = runners.spec;
    let shipped = runners.shipped;
    let fixture = &runners.fixture;
    let errors = family == "errors";
    let effects = family == "effects";
    let mut runs = Vec::new();
    let text = |s: &str| Value::Text(s.to_string());
    let path_value = |p: &str| Value::Constructed {
        constructor: "PATH".into(),
        text: p.into(),
    };
    let blocked = |error: &str| failed_with(spec, error, "pre_effect", "none");
    if !effects && !errors {
        return runs;
    }
    match op {
        "core.analyze" | "core.report" if effects => {
            let role = if op == "core.analyze" {
                "analysis"
            } else {
                "reporting"
            };
            let deps: &[Dependency] = if op == "core.analyze" {
                &[Dependency::Host, Dependency::Model]
            } else {
                &[Dependency::Model]
            };
            runs.push(resolved_axes(
                fixture,
                "axes/no-effects",
                "read_only requires possible effects exactly {none}: the invocation resolves none",
                &document(row),
                MockHost::new(),
                &[("[model]", "[none]")],
                "[none]",
            ));
            runs.push(determinism_run(
                runners,
                "determinism/base-nondeterministic",
                "the nondeterministic base row stays nondeterministic without an immutable profile that removes every variation",
                op,
                vec![("no profile selected", Vec::new(), None, "nondeterministic")],
            ));
            runs.push(determinism_run(
                runners,
                "determinism/verified-deterministic-profile",
                "a deterministic profile with an exact source narrows the nondeterministic base",
                op,
                vec![
                    (
                        "deterministic profile",
                        vec![category_profile(
                            op,
                            role,
                            Determinism::Deterministic,
                            deps,
                            &[],
                        )],
                        None,
                        "deterministic",
                    ),
                    (
                        "nondeterministic profile",
                        vec![category_profile(
                            op,
                            role,
                            Determinism::Nondeterministic,
                            deps,
                            &[],
                        )],
                        None,
                        "nondeterministic",
                    ),
                ],
            ));
        }
        "core.convert" if effects => {
            runs.push(on_mock(
                fixture,
                "precondition/0",
                "precondition: the source format or type is known",
                &document(row),
                blocked("error.operation.precondition"),
                host_refuses(
                    op,
                    lcl_runtime::RuntimeError::OperationPrecondition,
                    "target",
                    "conformance: the source format is not known",
                ),
                "the fixture capability reports an unknown source format",
            ));
            let converted = lcl_stdlib::schema::operation_with_value(
                path_value("/srv/data/report.txt"),
                Value::Boolean(true),
                text("{\"content\":\"content\"}"),
            );
            runs.push(on_mock(
                fixture,
                "postcondition/0",
                "postcondition: the output has the declared target_format and preserved properties",
                &with_action(
                    row,
                    &action_with(
                        row,
                        None,
                        &named("preserve", "LIST[STRING]", "FALSE", "[\"content\"]"),
                    ),
                ),
                Expectation::All(vec![succeeded(), attempt("changed", "TRUE".into())]),
                MockHost::new().script(op, vec![CapabilityOutcome::Completed(converted.clone())]),
                "D3 fixture conversion to format.json preserving content",
            ));
            runs.push(on_mock(
                fixture,
                "postcondition/1",
                "postcondition: result.operation.value equals the converted material value",
                &document(row),
                Expectation::All(vec![
                    succeeded(),
                    attempt("value", "\"{\\\"content\\\":\\\"content\\\"}\"".into()),
                ]),
                MockHost::new().script(op, vec![CapabilityOutcome::Completed(converted)]),
                "D3 fixture conversion",
            ));
            runs.push(resolved_axes(
                fixture,
                "resolution/exact-invocation-axes",
                "an omitted destination binds only the declared result or OUTPUT state",
                &document(row),
                MockHost::new(),
                &[("[host]", "[none]")],
                "[none]",
            ));
        }
        "core.generate" if effects => {
            runs.push(on_files(
                shipped,
                "precondition/0",
                "precondition: the target is not MEMORY or STATE",
                &with_action(row, &retargeted(row, "REF(memory.notes)")),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_mock(
                shipped,
                "precondition/1",
                "precondition: the required generation capability exists",
                &document(row),
                blocked("error.operation.precondition"),
                MockHost::new(),
                "the shipped engine installs no generation profile",
            ));
            let generated = lcl_stdlib::schema::operation_with_value(
                path_value("/srv/data/other.txt"),
                Value::Boolean(true),
                text("a short summary"),
            );
            for (label, clause) in [
                (
                    "postcondition/0",
                    "postcondition: the artifact satisfies the declared specification",
                ),
                (
                    "postcondition/1",
                    "postcondition: result.operation.value equals the produced material artifact",
                ),
            ] {
                runs.push(on_mock(
                    fixture,
                    label,
                    clause,
                    &document(row),
                    Expectation::All(vec![
                        succeeded(),
                        attempt("changed", "TRUE".into()),
                        attempt("value", "\"a short summary\"".into()),
                    ]),
                    MockHost::new()
                        .script(op, vec![CapabilityOutcome::Completed(generated.clone())]),
                    "D3 fixture generation of the declared specification",
                ));
            }
        }
        "core.install" if effects => {
            runs.push(on_mock(
                fixture,
                "precondition/0",
                "precondition: the package identity and source are unambiguous",
                &document(row),
                blocked("error.operation.precondition"),
                host_refuses(
                    op,
                    lcl_runtime::RuntimeError::OperationPrecondition,
                    "target",
                    "conformance: the package identity is ambiguous",
                ),
                "the fixture capability reports an ambiguous package identity",
            ));
            runs.push(on_mock(
                fixture,
                "postcondition/0",
                "postcondition: the declared package is installed at the requested version",
                &with_action(
                    row,
                    &action_with(row, None, &named("version", "STRING", "FALSE", "\"1.2.3\"")),
                ),
                Expectation::All(vec![succeeded(), attempt("changed", "TRUE".into())]),
                MockHost::new().script(
                    op,
                    vec![CapabilityOutcome::Completed(lcl_stdlib::schema::operation(
                        text("content"),
                        Value::Boolean(true),
                    ))],
                ),
                "D3 fixture installation at version 1.2.3",
            ));
        }
        "core.uninstall" if effects => {
            let removal = |purged: bool| {
                lcl_stdlib::schema::operation_with_value(
                    text("content"),
                    Value::Boolean(true),
                    text(if purged {
                        "installation absent; declared associated data absent"
                    } else {
                        "installation absent; declared associated data preserved"
                    }),
                )
            };
            runs.push(on_mock(
                fixture,
                "purge/false-preserves-data",
                "purge_data FALSE preserves declared associated data",
                &document(row),
                Expectation::All(vec![
                    succeeded(),
                    attempt(
                        "value",
                        "\"installation absent; declared associated data preserved\"".into(),
                    ),
                ]),
                MockHost::new().script(op, vec![CapabilityOutcome::Completed(removal(false))]),
                "D3 fixture removal with purge_data FALSE",
            ));
            runs.push(on_mock(
                fixture,
                "purge/true-removes-data",
                "purge_data TRUE makes every declared associated data item in authorized scope absent",
                &with_action(row, &action_with(row, None, &named("purge_data", "BOOLEAN", "FALSE", "TRUE"))),
                Expectation::All(vec![succeeded(), attempt("value", "\"installation absent; declared associated data absent\"".into())]),
                MockHost::new().script(op, vec![CapabilityOutcome::Completed(removal(true))]),
                "D3 fixture removal with purge_data TRUE",
            ));
            runs.push(on_mock(
                fixture,
                "purge/installation-absent",
                "postcondition: on success the target installation is absent",
                &document(row),
                Expectation::All(vec![succeeded(), attempt("changed", "TRUE".into())]),
                MockHost::new().script(op, vec![CapabilityOutcome::Completed(removal(false))]),
                "D3 fixture removal",
            ));
        }
        "core.copy" if effects => {
            let copy = |target: &str, destination: &str| {
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.copy\n    TARGET: {target}{}",
                        named(
                            "destination",
                            if destination.contains("uri") {
                                "URI"
                            } else {
                                "PATH"
                            },
                            "TRUE",
                            destination
                        )
                    ),
                )
            };
            runs.push(resolved_axes(
                shipped,
                "address/path-destination-filesystem",
                "a PATH destination adds the filesystem effect",
                &copy("REF(data.path)", "REF(data.other)"),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(
                shipped,
                "address/path-side-host",
                "any PATH side adds the host dependency",
                &copy("REF(data.path)", "REF(data.other)"),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            let any_copy = Runner::with_profiles(
                spec,
                replacing(
                    "core.copy",
                    "copy",
                    category_profile(
                        "core.copy",
                        "copy",
                        Determinism::Deterministic,
                        &[Dependency::Host, Dependency::Network],
                        &[Effect::Network, Effect::Filesystem],
                    ),
                ),
            )
            .expect("the engine assembles");
            runs.push(resolved_axes(
                &any_copy,
                "address/source-observation-dependency-only",
                "observing a MEMORY source adds its dependency but no source-side effect",
                &copy("REF(memory.notes)", "REF(data.other)"),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(
                &any_copy,
                "address/uri-side-network",
                "any URI side adds the network dependency and the network effect for the primary copy",
                &copy("REF(data.path)", "REF(data.uri)"),
                MockHost::new(),
                &[("[host, network]", "[network]")],
                "[network]",
            ));
        }
        "core.download" if effects => {
            let download = |target: &str| {
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.download\n    TARGET: {target}{}",
                        named("destination", "PATH", "TRUE", "PATH(\"/srv/data/new.txt\")")
                    ),
                )
            };
            runs.push(resolved_axes(
                shipped,
                "address/destination-path-host-filesystem",
                "the destination PATH adds host dependency and filesystem effect",
                &download("REF(data.uri)"),
                MockHost::new(),
                &[("[host, network]", "[filesystem, network]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(shipped, "address/remote-source-network", "a remote source adds network dependency and the network effect for the primary download", &download("REF(data.uri)"), MockHost::new(), &[("[host, network]", "[filesystem, network]")], "[filesystem]"));
            runs.push(resolved_axes(
                shipped,
                "address/local-source-host",
                "a local source adds host dependency but no network effect",
                &download("REF(data.path)"),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            let det = |role: &str, category: Determinism| {
                category_profile(
                    "core.download",
                    role,
                    category,
                    &[Dependency::Host, Dependency::Network],
                    &[Effect::Network, Effect::Filesystem],
                )
            };
            runs.push(determinism_run(
                runners,
                "determinism/source-identity-and-profiles",
                "deterministic exactly when the source profile fixes one immutable identity and both profiles are deterministic",
                op,
                vec![
                    ("both deterministic", vec![det("source", Determinism::Deterministic), det("transfer", Determinism::Deterministic)], None, "deterministic"),
                    ("mutable source", vec![det("source", Determinism::Nondeterministic), det("transfer", Determinism::Deterministic)], None, "nondeterministic"),
                ],
            ));
        }
        "core.move" if effects => {
            let mv = |target: &str, destination: (&str, &str)| {
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.move\n    TARGET: {target}{}",
                        named("destination", destination.0, "TRUE", destination.1)
                    ),
                )
            };
            let path_dest = ("PATH", "REF(data.other)");
            runs.push(resolved_axes(
                shipped,
                "address/path-host-filesystem",
                "PATH maps to host dependency and filesystem effect",
                &mv("REF(data.path)", path_dest),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(
                shipped,
                "address/independent-source-and-destination",
                "source and destination address classes resolve independently",
                &mv("REF(data.path)", path_dest),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(on_files(
                shipped,
                "address/memory-source-prohibited",
                "a MEMORY source is prohibited before effects",
                &mv("REF(memory.notes)", path_dest),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            runs.push(on_files(
                shipped,
                "address/state-source-prohibited",
                "a STATE source is prohibited before effects",
                &mv("REF(state.revision)", path_dest),
                &[REPORT],
                refused_before_effects(spec, "error.operation.precondition"),
                &[REPORT],
            ));
            let output = "\nOUTPUT:\n    ID: output.moved\n    TYPE: STRING\n    FORMAT: format.plain_text\n";
            let any_move = Runner::with_profiles(
                spec,
                replacing(
                    "core.move",
                    "move",
                    category_profile(
                        "core.move",
                        "move",
                        Determinism::Deterministic,
                        &[Dependency::Host, Dependency::Network],
                        &[Effect::Network, Effect::Filesystem, Effect::State],
                    ),
                ),
            )
            .expect("the engine assembles");
            runs.push(resolved_axes(
                &any_move,
                "address/output-source-state",
                "removing an OUTPUT source adds the state effect and its required dependency",
                &with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.move\n    TARGET: REF(output.moved){}",
                        named("destination", "PATH", "TRUE", "REF(data.other)")
                    ),
                    output,
                ),
                MockHost::new(),
                &[("[host]", "[filesystem, state]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(
                &any_move,
                "address/uri-network-network",
                "URI content relocation adds network dependency and network effect",
                &mv(
                    "REF(data.uri)",
                    ("URI", "URI(\"http://example.invalid/y\")"),
                ),
                MockHost::new(),
                &[("[network]", "[network]")],
                "[network]",
            ));
            runs.push(profile_role(
                runners,
                row,
                "address/profile-strategy",
                "move",
                AddressClass::Path,
                mv("REF(data.path)", path_dest),
            ));
        }
        "core.publish" if effects => {
            let publish = |target: &str, destination: (&str, &str)| {
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.publish\n    TARGET: {target}{}{}",
                        named("destination", destination.0, "TRUE", destination.1),
                        named("visibility", "STRING", "TRUE", "\"public\"")
                    ),
                )
            };
            runs.push(resolved_axes(
                shipped,
                "address/path-destination",
                "a PATH destination adds host dependency and filesystem effect",
                &publish("REF(data.path)", ("PATH", "REF(data.other)")),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(
                shipped,
                "address/uri-destination",
                "a URI destination adds network dependency and network effect",
                &publish("REF(data.path)", ("URI", "REF(data.uri)")),
                MockHost::new(),
                &[("[host, network]", "[network]")],
                "[network]",
            ));
            let any_publish = Runner::with_profiles(
                spec,
                replacing(
                    "core.publish",
                    "publication",
                    category_profile(
                        "core.publish",
                        "publication",
                        Determinism::Deterministic,
                        &[Dependency::Host, Dependency::Network],
                        &[Effect::Network, Effect::Filesystem],
                    ),
                ),
            )
            .expect("the engine assembles");
            runs.push(resolved_axes(
                &any_publish,
                "address/source-dependencies-only",
                "source observation contributes dependencies only",
                &publish("REF(data.text)", ("URI", "REF(data.uri)")),
                MockHost::new(),
                &[("[network]", "[network]")],
                "[network]",
            ));
            runs.push(determinism_run(
                runners,
                "determinism/profile-final-category",
                "the invocation copies the publication profile's final category once destination, visibility and replacement are fixed",
                op,
                vec![
                    ("deterministic publication", vec![category_profile(op, "publication", Determinism::Deterministic, &[Dependency::Host], &[Effect::Filesystem])], None, "deterministic"),
                    ("nondeterministic publication", vec![category_profile(op, "publication", Determinism::Nondeterministic, &[Dependency::Host], &[Effect::Filesystem])], None, "nondeterministic"),
                ],
            ));
        }
        "core.upload" if effects => {
            let upload = |destination: (&str, &str)| {
                with_action(
                    row,
                    &format!(
                        "OPERATION: core.upload\n    TARGET: REF(data.path){}",
                        named("destination", destination.0, "TRUE", destination.1)
                    ),
                )
            };
            let path_runner = Runner::with_profiles(
                spec,
                replacing(
                    "core.upload",
                    "transfer",
                    category_profile(
                        "core.upload",
                        "transfer",
                        Determinism::Deterministic,
                        &[Dependency::Host, Dependency::Network],
                        &[Effect::Network, Effect::Filesystem],
                    ),
                ),
            )
            .expect("the engine assembles");
            runs.push(resolved_axes(
                &path_runner,
                "address/path-destination-host-filesystem",
                "a PATH destination adds host dependency and filesystem effect",
                &upload(("PATH", "REF(data.other)")),
                MockHost::new(),
                &[("[host]", "[filesystem]")],
                "[filesystem]",
            ));
            runs.push(resolved_axes(shipped, "address/uri-destination-network", "a URI destination adds network dependency and the network effect for the primary upload", &upload(("URI", "REF(data.uri)")), MockHost::new(), &[("[host, network]", "[network]")], "[network]"));
            runs.push(resolved_axes(
                shipped,
                "address/path-source-host-without-filesystem-effect",
                "a PATH source adds host dependency but no filesystem effect",
                &upload(("URI", "REF(data.uri)")),
                MockHost::new(),
                &[("[host, network]", "[network]")],
                "[network]",
            ));
        }
        "core.execute" if effects => {
            runs.push(resolved_axes(
                shipped,
                "mode/non-graph-process-effect",
                "every non-graph invocation has the process effect because it runs the executable",
                &document(row),
                MockHost::new(),
                &[("[host]", "[process]")],
                "[process]",
            ));
            let wide = Runner::with_profiles(
                spec,
                Runner::shipped_profiles()
                    .into_iter()
                    .filter(|p| {
                        !(p.operation_id == "core.execute"
                            && p.profile_role.as_str() == "execution")
                    })
                    .chain([category_profile(
                        "core.execute",
                        "execution",
                        Determinism::Nondeterministic,
                        &[Dependency::Host, Dependency::Network],
                        &[Effect::Process, Effect::Filesystem],
                    )])
                    .collect(),
            )
            .expect("the engine assembles");
            runs.push(resolved_axes(
                &wide,
                "mode/non-graph-axis-union",
                "the rule unions process with any other dependencies and effects the profile selects, within the row maxima",
                &document(row),
                MockHost::new(),
                &[("[host, network]", "[filesystem, process]")],
                "[filesystem]",
            ));
            runs.push(determinism_run(
                runners,
                "mode/non-graph-profile-category",
                "non-graph mode copies the execution profile's final category",
                op,
                vec![
                    (
                        "deterministic execution profile",
                        vec![category_profile(
                            op,
                            "execution",
                            Determinism::Deterministic,
                            &[Dependency::Host],
                            &[Effect::Process],
                        )],
                        None,
                        "deterministic",
                    ),
                    (
                        "nondeterministic execution profile",
                        vec![category_profile(
                            op,
                            "execution",
                            Determinism::Nondeterministic,
                            &[Dependency::Host],
                            &[Effect::Process],
                        )],
                        None,
                        "nondeterministic",
                    ),
                ],
            ));
            let command = |started: bool, completed: bool, exit: Option<i64>, stdout: &str| {
                let mut observation = Observation::none()
                    .with("mode", Value::Identifier("non_graph".into()))
                    .with("started", Value::Boolean(started))
                    .with("completed", Value::Boolean(completed));
                if started {
                    observation = observation
                        .with("stdout", text(stdout))
                        .with("stderr", text(""));
                }
                if let Some(code) = exit {
                    observation = observation.with(
                        "exit_code",
                        Value::Integer(
                            lcl_checker::numeric::Decimal::parse_integer(&code.to_string())
                                .unwrap(),
                        ),
                    );
                }
                if started {
                    observation = observation.with_effect(ObservedEffect {
                        class: EffectClass::Process,
                        state: RecordState::Applied,
                        target: Some("report".into()),
                        evidence: Vec::new(),
                    });
                }
                CapabilityOutcome::Completed(observation)
            };
            runs.push(on_mock(
                shipped,
                "result/completed-exit-code",
                "a completed non_graph command records exit_code",
                &document(row),
                Expectation::All(vec![
                    succeeded(),
                    attempt("completed", "TRUE".into()),
                    attempt("exit_code", "0".into()),
                ]),
                MockHost::new().script(op, vec![command(true, true, Some(0), "all clear")]),
                "scripted completed command",
            ));
            runs.push(on_mock(
                shipped,
                "result/nonzero-exit-completed",
                "a completed command with a nonzero exit_code has producer status.succeeded",
                &document(row),
                Expectation::All(vec![succeeded(), attempt("exit_code", "7".into())]),
                MockHost::new().script(op, vec![command(true, true, Some(7), "")]),
                "scripted nonzero exit",
            ));
            runs.push(on_mock(
                shipped,
                "result/started-streams",
                "after start, stdout and stderr are present",
                &document(row),
                Expectation::All(vec![
                    succeeded(),
                    attempt("started", "TRUE".into()),
                    attempt("stdout", "\"all clear\"".into()),
                    attempt("stderr", "\"\"".into()),
                ]),
                MockHost::new().script(op, vec![command(true, true, Some(0), "all clear")]),
                "scripted started command",
            ));
            runs.push(on_mock(
                shipped,
                "result/failure-to-start",
                "a non_graph failure to start records started FALSE and completed FALSE without exit_code, stdout or stderr",
                &document(row),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.execution.action".into()),
                    attempt("started", "FALSE".into()),
                    attempt("completed", "FALSE".into()),
                    attempt("failure_phase", "pre_effect".into()),
                ]),
                MockHost::new().script(op, vec![CapabilityOutcome::Failed {
                    detail: "conformance: the program never started".into(),
                    observation: Observation::none()
                        .with("mode", Value::Identifier("non_graph".into()))
                        .with("started", Value::Boolean(false))
                        .with("completed", Value::Boolean(false)),
                }]),
                "scripted failure to start",
            ));
            runs.push(on_mock(
                shipped,
                "result/independent-phase-effect-output",
                "completion, failure phase, effects and OUTPUT binding are recorded independently",
                &document(row),
                Expectation::All(vec![
                    succeeded(),
                    attempt("failure_phase", "none".into()),
                    attempt("effect_state", "applied".into()),
                    attempt("exit_code", "7".into()),
                ]),
                MockHost::new().script(op, vec![command(true, true, Some(7), "")]),
                "scripted nonzero exit with an applied process effect",
            ));
        }
        "core.test" => {
            let test = |extra: &str, target: Option<&str>, declarations: &str| {
                let target = target
                    .map(|t| format!("\n    TARGET: {t}"))
                    .unwrap_or_default();
                with_declarations(
                    row,
                    &format!("OPERATION: core.test{target}{extra}"),
                    declarations,
                )
            };
            let comparison = |expected: (&str, &str), actual: (&str, &str)| {
                format!(
                    "{}{}",
                    named("expected", expected.0, "FALSE", expected.1),
                    named("actual", actual.0, "FALSE", actual.1)
                )
            };
            if errors {
                runs.push(shipped.execute(
                    "error/block.conditional_requirement",
                    "an invalid comparison shape is the row's registered conditional-requirement error",
                    &test("", Some("REF(data.number)"), ""),
                    Expectation::Rejects("error.block.conditional_requirement".into()),
                ));
                runs.push(shipped.execute(
                    "path/invalid-comparison-shape",
                    "assertion together with expected is not exactly one comparison form",
                    &test(
                        &format!(
                            "{}{}",
                            named("assertion", "BOOLEAN", "FALSE", "TRUE"),
                            named("expected", "INTEGER", "FALSE", "3")
                        ),
                        None,
                        "",
                    ),
                    Expectation::Rejects("error.block.conditional_requirement".into()),
                ));
                runs.push(on_files(
                    shipped,
                    "path/false-comparison-is-not-an-error",
                    "a completed FALSE comparison is a passed-domain outcome, not error.verification.failed",
                    &test(&comparison(("INTEGER", "3"), ("INTEGER", "4")), None, ""),
                    &[REPORT],
                    vec![succeeded(), attempt("passed", "FALSE".into()), Expectation::NoDiagnostic("error.verification.failed".into())],
                    &[REPORT],
                ));
                runs.push(shipped.execute(
                    "path/unresolved-typed-value-reference",
                    "an actual REFERENCE that does not resolve exactly once",
                    &test(
                        &comparison(("INTEGER", "3"), ("INTEGER", "REF(data.absent)")),
                        None,
                        "",
                    ),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
            }
            if effects {
                runs.push(determinism_run(
                    runners,
                    "mode/comparison-only-deterministic",
                    "comparison-only mode is deterministic",
                    op,
                    vec![("no graph", Vec::new(), None, "deterministic")],
                ));
                runs.push(on_files(
                    shipped,
                    "mode/strict-equality-comparison",
                    "expected-and-actual form always uses the registered == strict equality: different material types are unequal",
                    &test(&comparison(("INTEGER", "3"), ("STRING", "\"3\"")), None, ""),
                    &[REPORT],
                    vec![succeeded(), attempt("passed", "FALSE".into())],
                    &[REPORT],
                ));
            }
        }
        "core.validate" => {
            let validate = |extra: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!("OPERATION: core.validate\n    TARGET: REF(data.number){extra}"),
                    declarations,
                )
            };
            let rules = |value: &str, id: &str| {
                format!("\n    PARAMETER:\n        NAME: rules\n        TYPE: LIST[REFERENCE[REF({id})]]\n        REQUIRED: FALSE\n        VALUE: {value}")
            };
            let schema = |id: &str| {
                format!("\n    PARAMETER:\n        NAME: schema\n        TYPE: REFERENCE[REF({id})]\n        REQUIRED: FALSE\n        VALUE: REF({id})")
            };
            let failing_rule = "\nVALIDATE:\n    ID: validate.small\n    ASSERT: REF(data.number) < 2\n    REQUIRED: FALSE\n";
            if errors {
                runs.push(shipped.execute(
                    "path/unresolved-rule-reference",
                    "a rules REFERENCE that does not resolve exactly once",
                    &validate(
                        &rules("[REF(validate.absent)]", "validate.small"),
                        failing_rule,
                    ),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
                runs.push(shipped.execute(
                    "path/unresolved-schema-reference",
                    "a schema REFERENCE that does not resolve exactly once",
                    &validate(&schema("type.absent"), ""),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/wrong-kind-rule-reference",
                    "a rules REFERENCE resolving to a declaration other than VALIDATE",
                    &validate(&rules("[REF(data.number)]", "data.number"), ""),
                    "error.reference.kind",
                ));
                runs.push(failed_without_effects(
                    shipped,
                    "path/wrong-kind-schema-reference",
                    "a schema REFERENCE resolving to a declaration other than a kind.type OBJECT",
                    &validate(&schema("data.number"), ""),
                    "error.reference.kind",
                ));
                runs.push(shipped.execute(
                    "error/validation.failed",
                    "a required VALIDATE assertion that is FALSE is error.validation.failed before effects",
                    &validate("", "\nVALIDATE:\n    ID: validate.required\n    ASSERT: REF(data.number) < 2\n    REQUIRED: TRUE\n"),
                    Expectation::Rejects("error.validation.failed".into()),
                ));
            }
            if effects {
                runs.push(declared_state_only(
                    shipped,
                    "precondition/0",
                    "precondition: validation has no external side effect",
                    &document(row),
                ));
                runs.push(shipped.execute(
                    "postcondition/0",
                    "postcondition: every detected failure uses a registered error identifier",
                    &validate(
                        &rules("[REF(validate.small)]", "validate.small"),
                        failing_rule,
                    ),
                    Expectation::All(vec![
                        succeeded(),
                        attempt("valid", "FALSE".into()),
                        attempt("errors", "[error.validation.failed]".into()),
                    ]),
                ));
            }
        }
        "core.verify" => {
            let verify = |assertion: &str, extra: &str, declarations: &str| {
                with_declarations(
                    row,
                    &format!(
                        "OPERATION: core.verify\n    TARGET: REF(data.number){}{extra}",
                        named("assertion", "BOOLEAN", "TRUE", assertion)
                    ),
                    declarations,
                )
            };
            if errors {
                runs.push(shipped.execute(
                    "error/verification.failed",
                    "a FALSE assertion records the registered domain finding error.verification.failed",
                    &verify("REF(data.number) == 9", "", ""),
                    Expectation::All(vec![succeeded(), attempt("verified", "FALSE".into()), attempt("errors", "[error.verification.failed]".into())]),
                ));
                runs.push(shipped.execute(
                    "path/unresolved-assertion-reference",
                    "an assertion REFERENCE that does not resolve exactly once",
                    &with_action(row, "OPERATION: core.verify\n    TARGET: REF(data.number)\n    PARAMETER:\n        NAME: assertion\n        TYPE: REFERENCE[REF(data.absent)]\n        REQUIRED: TRUE\n        VALUE: REF(data.absent)"),
                    Expectation::Rejects("error.reference.unresolved".into()),
                ));
                runs.push(shipped.execute(
                    "error/evidence.missing",
                    "required EVIDENCE that is absent, unresolved or lacks provenance is error.evidence.missing",
                    &verify(
                        "REF(data.number) == 3",
                        "\n    PARAMETER:\n        NAME: evidence\n        TYPE: LIST[REFERENCE[REF(evidence.required)]]\n        REQUIRED: FALSE\n        VALUE: [REF(evidence.required)]",
                        "\nEVIDENCE:\n    ID: evidence.required\n    TYPE: INTEGER\n    VALUE: REF(data.list)[5]\n    REQUIRED: TRUE\n",
                    ),
                    Expectation::Diagnostic("error.evidence.missing".into()),
                ));
            }
            if effects {
                runs.push(shipped.execute(
                    "assertion/evaluated-against-snapshot",
                    "the assertion is evaluated deterministically against the observed target snapshot",
                    &verify("REF(data.number) == 3", "", ""),
                    Expectation::All(vec![succeeded(), attempt("verified", "TRUE".into()), attempt("observed", "{target: 3}".into())]),
                ));
                runs.push(declared_state_only(
                    shipped,
                    "axes/no-effects",
                    "verification itself has no effects",
                    &document(row),
                ));
                runs.push(declared_state_only(
                    shipped,
                    "axes/bounded-observation-dependencies",
                    "the dependency set resolves only to declared_state_only, host or network",
                    &document(row),
                ));
                runs.push(determinism_run(
                    runners,
                    "determinism/profile-final-category",
                    "the invocation copies the immutable verification profile's final category",
                    op,
                    vec![
                        (
                            "deterministic verification",
                            vec![category_profile(
                                op,
                                "verification",
                                Determinism::Deterministic,
                                &[],
                                &[],
                            )],
                            None,
                            "deterministic",
                        ),
                        (
                            "nondeterministic verification",
                            vec![category_profile(
                                op,
                                "verification",
                                Determinism::Nondeterministic,
                                &[],
                                &[],
                            )],
                            None,
                            "nondeterministic",
                        ),
                    ],
                ));
            }
        }
        _ => {}
    }
    runs
}
