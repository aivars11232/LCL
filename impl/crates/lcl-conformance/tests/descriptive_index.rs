//! Integration tests for the conformance skeleton against the canonical release.
//!
//! The central assertion is negative: this crate indexes requirements and
//! cannot report a pass.

use lcl_conformance::{CaseState, ConformanceIndex};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn index() -> ConformanceIndex {
    let pkg = SpecPackage::open(canonical_root()).expect("package verifies");
    ConformanceIndex::load(&pkg).expect("conformance index loads")
}

#[test]
fn loads_declared_catalog_sizes() {
    let i = index();
    assert_eq!(i.requirement_count(), 799);
    assert_eq!(i.witness_count(), 66);
}

#[test]
fn category_counts_agree_with_catalog() {
    let i = index();
    let defects = i.category_count_defects();
    assert!(
        defects.is_empty(),
        "category count disagreement: {defects:#?}"
    );
    assert_eq!(i.declared_category_counts().len(), 25);

    let observed = i.observed_category_counts();
    let total: u64 = observed.values().sum();
    assert_eq!(total, 799, "categories must partition the index");
}

#[test]
fn requirement_ids_are_unique_and_addressable() {
    let i = index();
    let r = i
        .requirement("KEYWORD-VALID-0001")
        .expect("known requirement id");
    assert_eq!(r.category, "keyword_valid");
    assert_eq!(r.subject, "ABS");
    assert_eq!(r.expected, "accept");
    assert_eq!(r.source, "keywords_v0.1.0.json");
    // Uniqueness is enforced at load; every entry must be retrievable by id.
    for req in i.requirements() {
        assert!(
            i.requirement(&req.id).is_some(),
            "unaddressable id {}",
            req.id
        );
    }
}

#[test]
fn every_requirement_is_fully_populated() {
    let i = index();
    for r in i.requirements() {
        assert!(!r.id.is_empty());
        assert!(!r.category.is_empty());
        assert!(!r.requirement.is_empty());
        assert!(!r.expected.is_empty());
        assert!(!r.source.is_empty());
    }
    for w in i.witnesses() {
        assert!(!w.id.is_empty());
        assert!(!w.contract.is_empty());
        assert!(!w.witness.is_empty());
        assert!(!w.expected.is_empty());
    }
}

#[test]
fn requirements_trace_to_registry_sources() {
    let i = index();
    assert!(!i.by_source("keywords_v0.1.0.json").is_empty());
    assert!(!i.by_source("statuses_and_errors_v0.1.0.json").is_empty());
    assert!(!i.by_source("operations_v0.1.0.json").is_empty());
    assert!(!i.by_category("keyword_valid").is_empty());
}

#[test]
fn expectation_vocabulary_is_descriptive() {
    let i = index();
    let vocab = i.expectation_vocabulary();
    assert!(
        vocab.contains_key("accept"),
        "vocabulary: {:?}",
        vocab.keys().collect::<Vec<_>>()
    );
    let total: u64 = vocab.values().sum();
    assert_eq!(total, 799);
}

/// The load-bearing negative assertion of this crate.
#[test]
fn nothing_is_executed_and_no_claim_is_available() {
    let i = index();
    assert!(i.all_unexecuted(), "M0 executes nothing");
    for r in i.requirements() {
        assert_eq!(r.state, CaseState::NotExecuted);
    }
    for w in i.witnesses() {
        assert_eq!(w.state, CaseState::NotExecuted);
    }
    let reason = i.claim_blocked_reason();
    assert!(reason.contains("No conformance level may be claimed"));
    assert!(reason.contains("milestone M0"));
}

/// The witness catalog must keep declaring itself unexecuted; if the release
/// ever said otherwise, loading must refuse rather than infer a result.
#[test]
fn witness_catalog_declares_itself_unexecuted() {
    let pkg = SpecPackage::open(canonical_root()).unwrap();
    let cat = pkg.catalog("language_decision_cases").unwrap();
    assert_eq!(cat.get("executed").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
        cat.get("evidence_kind").and_then(|v| v.as_str()),
        Some("descriptive_language_decision_witnesses")
    );
}
