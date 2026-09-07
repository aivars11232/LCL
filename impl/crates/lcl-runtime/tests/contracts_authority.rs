//! The runtime vocabulary agrees with the canonical registries.
//!
//! These are parity tests, not behaviour tests: they prove this build's
//! mirrored sets *are* the registry's sets, so a canonical change that this
//! build has not absorbed fails loudly instead of executing under a stale
//! vocabulary.

mod common;

use common::{contracts, spec};
use lcl_diagnostics::Stage;
use lcl_runtime::{Contracts, RuntimeError, ELSEWHERE};
use std::collections::BTreeSet;

#[test]
fn contracts_load_from_the_approved_package() {
    let contracts = Contracts::load(spec());
    assert!(
        contracts.is_ok(),
        "the approved package must load: {:?}",
        contracts.err()
    );
}

#[test]
fn every_mirrored_identifier_is_registered() {
    let registry = contracts().diagnostics();
    for id in RuntimeError::ALL {
        assert!(
            registry.error(id.as_registry_str()).is_some(),
            "{id} is mirrored by this build but absent from the registry"
        );
    }
}

#[test]
fn the_whole_registered_execution_stage_is_mirrored() {
    // This is the stage M6 owns, so nothing in it may be unnameable here.
    let registry = contracts().diagnostics();
    let registered: BTreeSet<String> = registry
        .errors_by_stage(Stage::Execution)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    let mirrored: BTreeSet<String> = RuntimeError::ALL
        .into_iter()
        .filter(|id| contracts().error(*id).stage == Stage::Execution)
        .map(|id| id.as_registry_str().to_string())
        .collect();
    assert_eq!(
        registered, mirrored,
        "the mirrored execution-stage set must equal the registered one"
    );
}

#[test]
fn mirrored_metadata_is_copied_from_the_registry_not_assumed() {
    let registry = contracts().diagnostics();
    for id in RuntimeError::ALL {
        let registered = registry
            .error(id.as_registry_str())
            .expect("mirrored identifier is registered");
        let mirrored = contracts().error(id);
        assert_eq!(mirrored.stage, registered.stage, "{id} stage");
        assert_eq!(
            mirrored.default_status, registered.default_status,
            "{id} default_status"
        );
        assert_eq!(mirrored.event, registered.event, "{id} event");
        assert_eq!(
            mirrored.recoverable, registered.recoverable_with_declared_handler,
            "{id} recoverability"
        );
        assert_eq!(mirrored.meaning, registered.meaning, "{id} meaning");
    }
}

#[test]
fn the_event_mapping_is_a_bijection_over_recoverable_errors() {
    // `mapping_invariant`: "event is non-null exactly when
    // recoverable_with_declared_handler is true, and every registered event is
    // the image of exactly one registered error."
    let registry = contracts().diagnostics();
    let mut events: Vec<String> = Vec::new();
    for error in registry.errors() {
        assert_eq!(
            error.event.is_some(),
            error.recoverable_with_declared_handler,
            "{}: event presence must equal recoverability",
            error.id
        );
        if let Some(event) = &error.event {
            events.push(event.clone());
        }
    }
    let unique: BTreeSet<&String> = events.iter().collect();
    assert_eq!(
        unique.len(),
        events.len(),
        "every registered event is the image of exactly one error"
    );
    assert_eq!(
        contracts().events().len(),
        events.len(),
        "the loaded event vocabulary is the whole image"
    );
}

#[test]
fn two_identifiers_are_mirrored_but_decided_by_preflight() {
    // Both are registered at the execution stage yet `05_SEMANTICS/09` states
    // they are pre_effect only. They are mirrored for stage parity and never
    // emitted here; `runtime_totality` proves the "never emitted" half.
    let named: BTreeSet<RuntimeError> = ELSEWHERE.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        named,
        BTreeSet::from([
            RuntimeError::DependencyUnsatisfied,
            RuntimeError::ScopeViolation
        ])
    );
    for (id, owner) in ELSEWHERE {
        assert!(id.is_elsewhere(), "{id} must be marked as decided elsewhere");
        assert!(owner.contains("M5"), "{id} must name its deciding milestone");
    }
    assert_eq!(
        RuntimeError::emitted().count(),
        RuntimeError::ALL.len() - ELSEWHERE.len()
    );
}

#[test]
fn the_expression_demand_map_is_the_registrys() {
    let demand = contracts().demand();
    // Every eligible identifier the registry lists is mirrored, and the
    // resolved stage and statuses are the registry's own.
    assert_eq!(demand.resolved_stage, Stage::Execution);
    assert_eq!(demand.default_status, "status.failed");
    assert_eq!(
        demand.status_for(RuntimeError::RequiredMissing),
        "status.blocked",
        "required MISSING retains status.blocked"
    );
    assert_eq!(
        demand.status_for(RuntimeError::ValueUnknown),
        "status.blocked",
        "required UNKNOWN retains status.blocked"
    );
    assert_eq!(
        demand.status_for(RuntimeError::NumericDivisionByZero),
        "status.failed"
    );
    // The eligible set is exactly the eleven the registry names.
    let eligible: BTreeSet<&str> = demand.eligible().map(|e| e.as_registry_str()).collect();
    assert_eq!(
        eligible,
        BTreeSet::from([
            "error.literal.invalid",
            "error.numeric.division_by_zero",
            "error.numeric.non_terminating",
            "error.numeric.unit_mismatch",
            "error.operator.operand",
            "error.pattern.mismatch",
            "error.pattern.resource_limit",
            "error.required.missing",
            "error.type.mismatch",
            "error.value.out_of_range",
            "error.value.unknown",
        ])
    );
    // Every eligible identifier carries its exact registered trigger sentence.
    for id in demand.eligible() {
        let trigger = demand.trigger(id).unwrap_or_default();
        assert!(!trigger.is_empty(), "{id} must carry its trigger sentence");
    }
    assert!(
        demand.exclusion_rule.contains("never changes classification"),
        "the exclusion rule is kept verbatim"
    );
}

#[test]
fn an_ineligible_identifier_cannot_be_demand_resolved() {
    // `exclusion_rule`: "No source structure, token, name resolution,
    // type-family, signature arity, receiving-type, or required
    // static-validation defect qualifies."
    for id in [
        RuntimeError::ExecutionOrder,
        RuntimeError::PermissionDenied,
        RuntimeError::RetryExhausted,
        RuntimeError::HostConstraint,
        RuntimeError::Cancelled,
    ] {
        assert!(
            !contracts().demand().is_eligible(id),
            "{id} is not in the eligible map and must not be demand-resolvable"
        );
    }
}

#[test]
fn the_nine_result_schemas_load_with_their_registered_projections() {
    let schemas: Vec<&str> = contracts().schemas().map(|s| s.id.as_str()).collect();
    assert_eq!(schemas.len(), 9, "the registry closes nine result schemas");
    let command = contracts()
        .schema("result.command")
        .expect("result.command is registered");
    assert_eq!(command.default_property.as_deref(), Some("stdout"));
    assert!(command.partial_supported);
    assert_eq!(command.partial_fields, vec!["stdout", "stderr"]);
    // "Core 0.1.0 permits partial binding only for result.command stdout and
    // stderr after a non-graph command started."
    for schema in contracts().schemas() {
        if schema.id != "result.command" {
            assert!(
                !schema.partial_supported,
                "{} must not permit partial OUTPUT",
                schema.id
            );
        }
    }
    assert!(command.permits_partial(&["stdout".to_string()]));
    assert!(!command.permits_partial(&["exit_code".to_string()]));
    assert!(!command.permits_partial(&[]));
}

#[test]
fn every_core_operation_names_a_loaded_result_schema() {
    // Closure: the operation registry's `result_schema` must always resolve.
    let mut checked = 0;
    for operation in contracts().statics().operations() {
        assert!(
            contracts().schema(&operation.result_schema).is_some(),
            "{}: result schema {:?} is not registered",
            operation.id,
            operation.result_schema
        );
        checked += 1;
    }
    assert_eq!(checked, 39, "the registry closes 39 core operations");
}

#[test]
fn retry_bounds_come_from_the_field_signature_registry() {
    let bounds = contracts().retry_bounds();
    assert_eq!(bounds.minimum_limit, 0);
    assert_eq!(bounds.maximum_limit, 100);
    assert!(bounds.when_default, "omitted WHEN is TRUE");
    assert_eq!(
        bounds.delay_default, "DURATION(0, unit.second)",
        "omitted DELAY is DURATION(0, unit.second)"
    );
}

#[test]
fn an_unverified_package_is_refused() {
    // The runtime executes; executing against unverified canonical bytes would
    // leave every effect without authority.
    let unverified = SpecPackageProbe::open_unverified();
    if let Some(package) = unverified {
        assert!(
            Contracts::load(&package).is_err(),
            "an unverified package must be refused"
        );
    }
}

/// Opening the same root without verification, when the loader permits it.
struct SpecPackageProbe;

impl SpecPackageProbe {
    fn open_unverified() -> Option<lcl_spec::SpecPackage> {
        lcl_spec::SpecPackage::open_unverified(common::canonical_root()).ok()
    }
}
