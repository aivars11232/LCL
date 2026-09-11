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
///
/// The *index* still executes nothing, at M8 exactly as at M0. What changed is
/// only the stated reason: the original text said the claim was blocked because
/// no lexer, parser, evaluator or executor existed "at milestone M0", and that
/// sentence became false when M8 supplied all four. The blocked claim itself is
/// unchanged and is asserted here more strictly than before, against the
/// canonical requirement rather than against a milestone name:
///
/// > catalog entries without concrete input and an implementation result are
/// > not executed conformance cases
///
/// Executed evidence lives in `runner::ExecutedCase`, a different type that
/// cannot be constructed without both, and `executed_cases_are_a_separate_type`
/// below pins that separation.
#[test]
fn nothing_is_executed_and_no_claim_is_available_from_the_index() {
    let i = index();
    assert!(i.all_unexecuted(), "the index executes nothing");
    for r in i.requirements() {
        assert_eq!(r.state, CaseState::NotExecuted);
    }
    for w in i.witnesses() {
        assert_eq!(w.state, CaseState::NotExecuted);
    }
    let reason = i.claim_blocked_reason();
    assert!(reason.contains("No conformance level may be claimed from this index"));
    assert!(
        reason.contains("without concrete input and an implementation result"),
        "the reason must cite the canonical threshold, not a milestone number"
    );
    assert!(
        reason.contains("runner::Runner"),
        "and must name where executed evidence does come from"
    );
}

/// An indexed requirement and an executed case cannot be confused or combined.
#[test]
fn executed_cases_are_a_separate_type_from_indexed_requirements() {
    let i = index();
    // `CaseState` still has exactly one variant, so no indexed entry can carry
    // a verdict. The exhaustive match is the guard: adding a `Passed` variant
    // stops this compiling.
    for r in i.requirements() {
        match r.state {
            CaseState::NotExecuted => {}
        }
    }
    // And the report keeps the two populations in separate columns with no
    // total, so 799 indexed rows can never be read as 799 executed ones.
    let report = lcl_conformance::ConformanceReport::new(
        lcl_conformance::report::Implementation::under_test("test", "0.1.0"),
        i.requirement_count(),
        i.witnesses().iter().map(|w| w.id.clone()),
    );
    assert_eq!(report.descriptive_count(), 799);
    assert_eq!(report.executed_count(), 0);
    assert_eq!(report.claim(), lcl_conformance::report::ClaimLevel::None);
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
