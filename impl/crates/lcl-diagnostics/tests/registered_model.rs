//! Integration tests for the diagnostics skeleton against the canonical release.
//!
//! Numeric expectations here are a *pinned oracle*: the loaded package is
//! version-pinned to 0.1.0, so any change in these numbers is drift, not an
//! upgrade, and should fail loudly.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn registry() -> DiagnosticRegistry {
    let pkg = SpecPackage::open(canonical_root()).expect("package verifies");
    DiagnosticRegistry::load(&pkg).expect("diagnostic registry loads")
}

#[test]
fn loads_all_registered_errors_and_statuses() {
    let r = registry();
    assert_eq!(r.error_count(), 77);
    assert_eq!(r.status_count(), 12);
}

/// The registry's declared stage order must match this build's `Stage` enum,
/// and every error must classify into it.
#[test]
fn stage_order_agrees_with_registry() {
    let r = registry();
    assert_eq!(r.stage_order(), Stage::ORDER.as_slice());
    let names: Vec<&str> = r
        .stage_order()
        .iter()
        .map(|s| s.as_registry_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "lexical",
            "grammar_or_schema",
            "resolution",
            "static_or_expression",
            "validation",
            "execution",
            "verification_or_completion",
        ]
    );
}

/// Pinned distribution of the 77 registered errors across the 7 stages.
#[test]
fn stage_histogram_is_exact() {
    let r = registry();
    let hist: Vec<(&str, usize)> = r
        .stage_histogram()
        .into_iter()
        .map(|(s, n)| (s.as_registry_str(), n))
        .collect();
    assert_eq!(
        hist,
        vec![
            ("lexical", 23),
            ("grammar_or_schema", 12),
            ("resolution", 14),
            ("static_or_expression", 12),
            ("validation", 2),
            ("execution", 11),
            ("verification_or_completion", 3),
        ]
    );
    let total: usize = hist.iter().map(|(_, n)| n).sum();
    assert_eq!(
        total,
        r.error_count(),
        "histogram must partition all errors"
    );
}

/// Closure: nothing references an identifier outside the closed registry.
/// `DiagnosticRegistry::load` enforces this, so reaching here proves it held.
#[test]
fn registry_is_closed() {
    let r = registry();
    for e in r.errors() {
        assert!(
            r.status(&e.default_status).is_some(),
            "{}: default_status {} unregistered",
            e.id,
            e.default_status
        );
    }
    for s in r.statuses() {
        for n in &s.allowed_next {
            assert!(
                r.status(n).is_some(),
                "{}: allowed_next {n} unregistered",
                s.id
            );
        }
    }
}

#[test]
fn error_identifiers_are_namespaced() {
    let r = registry();
    for e in r.errors() {
        assert!(e.id.starts_with("error."), "unexpected error id {}", e.id);
        assert!(!e.meaning.is_empty());
    }
    for s in r.statuses() {
        assert!(s.id.starts_with("status."), "unexpected status id {}", s.id);
    }
}

/// Spot-check specific registered contracts against the release text.
#[test]
fn spot_checks_match_release() {
    let r = registry();

    let e = r.error("error.determinism.mismatch").expect("registered");
    assert_eq!(e.stage, Stage::Validation);
    assert!(
        !e.recoverable_with_declared_handler,
        "documented nonrecoverable"
    );
    assert_eq!(e.default_status, "status.invalid");

    let e = r
        .error("error.block.conditional_requirement")
        .expect("registered");
    assert_eq!(e.stage, Stage::GrammarOrSchema);
    assert_eq!(e.default_status, "status.invalid");

    let s = r.status("status.not_started").expect("registered");
    assert!(!s.terminal);
    assert_eq!(
        s.allowed_next,
        vec!["status.validating", "status.cancelled"]
    );
}

#[test]
fn terminal_statuses_are_identified() {
    let r = registry();
    let terminal: Vec<&str> = {
        let mut v: Vec<&str> = r
            .terminal_statuses()
            .iter()
            .map(|s| s.id.as_str())
            .collect();
        v.sort_unstable();
        v
    };
    assert!(terminal.contains(&"status.succeeded"));
    assert!(terminal.contains(&"status.failed"));
    assert!(terminal.contains(&"status.blocked"));
    assert!(!terminal.contains(&"status.running"));
}

/// M0 boundary: the selection contract is exposed as data, not implemented.
#[test]
fn selection_contract_is_available_as_data() {
    let r = registry();
    let sc = r.selection_contract();
    for key in [
        "stage_order",
        "expression_demand_resolution",
        "supersedes",
        "duplicate_key",
        "stable_order",
        "primary_rule",
        "secondary_rule",
    ] {
        assert!(sc.get(key).is_some(), "selection contract missing {key}");
    }
    assert!(r.event_model().get("selection_order").is_some());
    assert!(r.failure_lifecycle().get("retry_safety").is_some());
    assert!(r.check_selection_contract().get("selection").is_some());
}

/// Recoverability is a real distinction in the registry, not all-or-nothing.
#[test]
fn recoverable_errors_are_a_strict_subset() {
    let r = registry();
    let n = r.recoverable_errors().len();
    assert!(n > 0, "some errors are handler-recoverable");
    assert!(n < r.error_count(), "not all errors are recoverable");
}
