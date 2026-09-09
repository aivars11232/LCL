//! The completion vocabulary agrees with the registry, or it does not load.
//!
//! Every assertion here reads the canonical package. Nothing is compared
//! against a number written into this file, except the counts the registry
//! itself declares.

mod common;

use common::*;
use lcl_completion::{CompletionError, Contracts};
use lcl_diagnostics::Stage;

#[test]
fn the_owned_set_is_exactly_the_registrys_verification_or_completion_stage() {
    let registry = completion_contracts().diagnostics();
    let mut registered: Vec<String> = registry
        .errors_by_stage(Stage::VerificationOrCompletion)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    registered.sort();

    let mut owned: Vec<String> = CompletionError::OWNED
        .iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    owned.sort();

    assert_eq!(
        registered, owned,
        "this milestone owns exactly the registry's verification_or_completion errors"
    );
}

#[test]
fn every_mirrored_identifier_keeps_its_registered_stage() {
    let contracts = completion_contracts();
    let registry = contracts.diagnostics();
    for id in CompletionError::ALL {
        let def = registry
            .error(id.as_registry_str())
            .unwrap_or_else(|| panic!("{id} is registered"));
        assert_eq!(
            contracts.error(id).stage,
            def.stage,
            "{id} must carry the registry's stage, not this layer's"
        );
        assert_eq!(contracts.error(id).meaning, def.meaning);
        assert_eq!(contracts.error(id).default_status, def.default_status);
        assert_eq!(contracts.error(id).event, def.event);
    }
}

#[test]
fn a_reused_identifier_is_not_relabelled_as_a_completion_error() {
    let contracts = completion_contracts();
    for id in CompletionError::ALL {
        if id.is_owned() {
            assert_eq!(contracts.error(id).stage, Stage::VerificationOrCompletion);
        } else {
            assert_ne!(
                contracts.error(id).stage,
                Stage::VerificationOrCompletion,
                "{id} belongs to an earlier stage and keeps it"
            );
        }
    }
}

#[test]
fn terminal_statuses_come_from_the_registry_and_have_no_outgoing_transition() {
    let contracts = completion_contracts();
    let mut terminal = 0;
    for status in contracts.statuses() {
        if status.terminal {
            terminal += 1;
            assert!(
                status.allowed_next.is_empty(),
                "{} is terminal, so it has no allowed_next",
                status.id
            );
            assert!(contracts.is_terminal(&status.id));
        } else {
            assert!(
                !status.allowed_next.is_empty(),
                "{} is not terminal, so it must permit a next state",
                status.id
            );
        }
    }
    assert!(terminal > 0, "the registry defines terminal statuses");
}

#[test]
fn succeeded_is_the_only_terminal_status_that_is_not_a_failure_mapping_candidate() {
    let contracts = completion_contracts();
    assert!(contracts.is_terminal("status.succeeded"));
    assert!(
        !contracts.is_terminal_non_success("status.succeeded"),
        "SUCCESS is not a FAILURE mapping target"
    );
    // "status.partial is explicitly non-success."
    assert!(contracts.is_terminal_non_success("status.partial"));
    assert!(contracts.is_terminal_non_success("status.failed"));
    assert!(contracts.is_terminal_non_success("status.blocked"));
}

#[test]
fn skipped_is_the_one_status_scoped_away_from_an_execution_root() {
    let contracts = completion_contracts();
    // "Only non-root declarations may move from status.ready to
    // status.skipped." The registry expresses that as a narrower scope, and
    // this layer reads the scope rather than special-casing the identifier.
    assert_eq!(
        contracts.scope("status.skipped"),
        Some("declaration_or_result")
    );
    assert!(!contracts.permitted_at_root("status.skipped"));
    for status in contracts.statuses() {
        if status.id != "status.skipped" {
            assert!(
                contracts.permitted_at_root(&status.id),
                "{} must be legal at an execution root",
                status.id
            );
        }
    }
}

#[test]
fn allowed_next_is_read_from_the_registry() {
    let contracts = completion_contracts();
    // Spot-check against the registry itself, not against a transcribed table.
    for status in contracts.statuses() {
        for next in &status.allowed_next {
            assert!(
                contracts.permits(&status.id, next),
                "{} permits {next}",
                status.id
            );
        }
        assert!(
            !contracts.permits(&status.id, "status.not_started"),
            "no status returns to not_started"
        );
    }
}

#[test]
fn the_governing_sentences_are_loaded_verbatim() {
    let contracts = completion_contracts();
    let selection = contracts.check_selection();
    assert!(selection
        .selection
        .contains("Targeted post-execution VERIFY applies only to an actually activated producer"));
    assert!(selection.demand.contains("A skipped check has no result"));
    assert!(selection.failure.contains("error.verification.failed"));
    assert!(selection.root_success.contains("error.success.unsatisfied"));

    let lifecycle = contracts.failure_lifecycle();
    assert!(lifecycle
        .status_rule
        .contains("secondary diagnostics and FAILURE mappings never override it"));
    assert!(lifecycle
        .failure_mapping_rule
        .contains("Execution roots cannot select status.skipped"));
}

#[test]
fn an_unverified_package_is_refused() {
    let unverified = lcl_spec::SpecPackage::open_unverified(canonical_root())
        .expect("the package is readable without verification");
    assert!(
        Contracts::load(&unverified).is_err(),
        "completion contracts load only from the approved release"
    );
}
