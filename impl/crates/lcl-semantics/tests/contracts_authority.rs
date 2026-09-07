//! The preflight vocabulary is the registry's, not this build's.
//!
//! Every one of these tests exists to make a disagreement between the canonical
//! registries and this crate a *failure*, never a silent divergence.

mod common;

use common::*;
use lcl_diagnostics::Stage;
use lcl_semantics::{Contracts, PreflightError};
use lcl_spec::SpecPackage;

#[test]
fn contracts_load_only_from_the_approved_package() {
    let unverified =
        SpecPackage::open_unverified(canonical_root()).expect("the package is readable");
    assert!(
        Contracts::load(&unverified).is_err(),
        "an unverified package must never supply the preflight vocabulary"
    );
    assert!(Contracts::load(spec()).is_ok());
}

#[test]
fn every_mirrored_identifier_is_registered() {
    let registry = contracts().diagnostics();
    for id in PreflightError::ALL {
        let def = registry
            .error(id.as_registry_str())
            .unwrap_or_else(|| panic!("{id} must be a registered error identifier"));
        assert_eq!(def.id, id.as_registry_str());
    }
}

#[test]
fn the_mirrored_set_covers_the_whole_registered_validation_stage() {
    // The layer spans three stages, but it owns `validation` entirely: if the
    // registry ever adds a validation-stage identifier, this build must grow to
    // match it rather than quietly ignore it.
    let registry = contracts().diagnostics();
    let registered: Vec<String> = registry
        .errors_by_stage(Stage::Validation)
        .into_iter()
        .map(|d| d.id.clone())
        .collect();
    assert_eq!(
        registered,
        vec![
            "error.determinism.mismatch".to_string(),
            "error.validation.failed".to_string()
        ]
    );
    for id in &registered {
        assert!(
            PreflightError::from_registry_str(id).is_some(),
            "{id} is a registered validation-stage error and must be mirrored"
        );
    }
}

#[test]
fn every_identifier_carries_its_registry_metadata_verbatim() {
    let registry = contracts().diagnostics();
    for id in PreflightError::ALL {
        let registered = contracts().error(id);
        let def = registry.error(id.as_registry_str()).expect("registered");
        assert_eq!(registered.stage, def.stage, "{id} stage");
        assert_eq!(registered.meaning, def.meaning, "{id} meaning");
        assert_eq!(
            registered.default_status, def.default_status,
            "{id} default_status"
        );
        assert_eq!(registered.event, def.event, "{id} event");
        assert_eq!(
            registered.recoverable, def.recoverable_with_declared_handler,
            "{id} recoverability"
        );
    }
}

#[test]
fn this_layer_spans_exactly_three_registered_stages() {
    // Stated as a test because it is the one structural fact about this layer
    // that is easy to get wrong: preflight is one step of the processing model
    // and three stages of the diagnostic model.
    let mut stages: Vec<Stage> = PreflightError::ALL
        .into_iter()
        .map(|id| contracts().error(id).stage)
        .collect();
    stages.sort_by_key(|s| s.index());
    stages.dedup();
    assert_eq!(
        stages,
        vec![
            Stage::Resolution,
            Stage::StaticOrExpression,
            Stage::Validation,
            Stage::Execution
        ]
    );
}

#[test]
fn required_missing_and_value_unknown_keep_status_blocked() {
    // `expression_demand_resolution` would give a *post*-preflight demand
    // `status.failed`, with these two overridden back to `status.blocked`. Its
    // context excludes this layer outright — "Preflight-required expression
    // demands retain registered source-validation classification" — so what
    // this layer reports is simply the registered default.
    assert_eq!(
        contracts()
            .error(PreflightError::RequiredMissing)
            .default_status,
        "status.blocked"
    );
    assert_eq!(
        contracts()
            .error(PreflightError::ValueUnknown)
            .default_status,
        "status.blocked"
    );
}

#[test]
fn deferred_identifiers_are_named_with_their_owner() {
    for (id, owner) in lcl_semantics::DEFERRED {
        assert!(
            !owner.is_empty(),
            "{id} is deferred and must name the milestone that owns it"
        );
    }
}

#[test]
fn authority_and_priority_bounds_match_the_prose() {
    // `05_SEMANTICS/04`: "Effective AUTHORITY is 0..1000; local default 500.
    // PRIORITY is -1000..1000 as a strict INTEGER ... When PRIORITY is optional
    // and MISSING, its value is 0."
    let authority = contracts().authority_bounds();
    assert_eq!((authority.minimum, authority.maximum), (0, 1000));
    assert_eq!(authority.local_default, 500);
    let priority = contracts().priority_bounds();
    assert_eq!((priority.minimum, priority.maximum), (-1000, 1000));
    assert_eq!(priority.optional_default, 0);
}

#[test]
fn the_graph_and_check_contracts_are_read_from_the_registries() {
    let graph = contracts().graph_contract();
    assert!(graph
        .candidate_graph
        .contains("Build the complete finite structural activation graph from EXECUTE"));
    assert!(graph
        .ordering
        .contains("BEFORE and AFTER add edges and never reverse a sequential edge"));
    assert!(graph.parallel.contains("proven independence"));

    let checks = contracts().check_selection();
    assert!(checks
        .demand
        .contains("Every selected and applicable VALIDATE is evaluated before any effect"));
    assert!(checks.prerequisites.contains("stable topological order"));
    assert!(checks.failure.contains("error.validation.failed"));
}

#[test]
fn the_none_effect_sentinel_is_an_absence_not_an_effect_class() {
    // `05_SEMANTICS/05`: "No record is created for the none sentinel."
    // `operations_v0.1.0.json` writes a read-only operation's effects as
    // `["none"]`, not `[]`, so a layer that tested the list for emptiness would
    // classify every read-only operation as mutating. The registry's own
    // `category` is the classifier for that question.
    let read_only = contracts()
        .operation_axes("core.inspect")
        .expect("core.inspect is registered");
    assert_eq!(read_only.category, "read_only");
    assert!(!read_only.is_mutating());
    assert!(
        read_only.possible_effects.is_empty(),
        "the none sentinel is stripped, so an absence reads as an absence"
    );

    let mutating = contracts()
        .operation_axes("core.copy")
        .expect("core.copy is registered");
    assert_eq!(mutating.category, "mutating");
    assert!(mutating.is_mutating());
    assert!(mutating.possible_effects.contains("filesystem"));
}

#[test]
fn every_registered_operation_has_loaded_axes() {
    // 39 registered core operations; a registry that grows must not leave this
    // layer silently unaware of a new one.
    assert_eq!(contracts().operation_axes_count(), 39);
    for operation in contracts().statics().operations() {
        assert!(
            contracts().operation_axes(&operation.id).is_some(),
            "{} must have loaded effect and determinism axes",
            operation.id
        );
    }
}

#[test]
fn a_deferred_identifier_is_never_emitted_by_this_layer() {
    // A deferred identifier must be deferred in fact, not only in a comment: if
    // any canonical example or fixture made this layer emit one, the deferral
    // would be a false statement about what this milestone decides.
    let deferred: Vec<String> = lcl_semantics::DEFERRED
        .iter()
        .map(|(id, _)| id.to_string())
        .collect();
    assert!(!deferred.is_empty(), "this test is about the deferred set");

    let dir = canonical_root().join("08_EXAMPLES");
    for sub in ["VALID", "INVALID"] {
        let mut paths: Vec<_> = std::fs::read_dir(dir.join(sub))
            .expect("readable")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "lcl"))
            .collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let source = std::fs::read_to_string(&path).expect("readable");
            let Ok(resolved) =
                resolver().resolve(&unit(&name, &source), &canonical_example_provider())
            else {
                continue;
            };
            let Ok(checked) = checker().check(&resolved) else {
                continue;
            };
            let Ok(planned) =
                preflight().plan(&checked, &resolved, &lcl_semantics::Invocation::new())
            else {
                continue;
            };
            for diagnostic in planned.diagnostics() {
                assert!(
                    !deferred.contains(&diagnostic.id.to_string()),
                    "{name} emitted {}, which this milestone declares deferred",
                    diagnostic.id
                );
            }
        }
    }
}
