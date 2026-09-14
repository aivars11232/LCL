//! The production report path itself: `lcl_conformance::production::report`,
//! exactly as the report command runs it, over every executed population.
//!
//! Synthetic accounting controls live beside the report instrument. These tests
//! use real execution records only.

mod common;

use lcl_conformance::obligations::MAPPING_DIGEST;
use lcl_conformance::production;
use lcl_conformance::report::{ClaimLevel, ProbeState};
use lcl_spec::json::{self, Json};
use std::collections::BTreeSet;

/// Semantic families this build has no executed population for. Every other
/// required probe must be carried by exactly one real record. Connecting a
/// population, or losing one, fails these tests until this list is reviewed.
const UNPOPULATED: [&str; 2] = ["semantic/diagnostic_policy/", "semantic/failure_lifecycle/"];

/// Engine defects that faithful sub-runs expose while full semantic conformance
/// stays BLOCKED (LCL-CLOSE-02; residual report, "P4c, operators and functions").
/// Each entry is a failed probe and exactly the sub-runs that fail. A repair, or
/// any new failure, changes this set and fails the test until it is reviewed.
const KNOWN_FAILED: [(&str, &[&str]); 8] = [
    ("semantic/function_invalid/ROUND", &["round/host-capacity"]),
    ("semantic/function_invalid/SUM", &["sum/unit-mismatch"]),
    (
        "semantic/operator_invalid//",
        &["division/declared-bound", "division/host-capacity"],
    ),
    (
        "semantic/operator_invalid/MATCHES",
        &["pattern/resource-limit"],
    ),
    (
        "semantic/operator_valid/!=",
        &["equality/path-address-form-identity"],
    ),
    (
        "semantic/operator_valid/-",
        &["constraint/negative-duration-result"],
    ),
    (
        "semantic/operator_valid/==",
        &["equality/path-address-form-identity"],
    ),
    (
        "semantic/operator_valid/MATCHES",
        &["match/glob-workspace-path"],
    ),
];

#[test]
fn the_production_report_executes_every_population_and_claims_exactly_what_it_supports() {
    let report = production::report(common::spec()).expect("the production report assembles");
    let inventory = report
        .obligations()
        .expect("a claim-capable verified inventory");
    assert_eq!(inventory.digest(), MAPPING_DIGEST);
    assert_eq!(
        report.implementation().package_identity,
        lcl_spec::APPROVED_PACKAGE.identity_digest
    );
    let failed = report.failed_ids();
    assert_eq!(
        failed.iter().map(String::as_str).collect::<BTreeSet<_>>(),
        KNOWN_FAILED
            .iter()
            .map(|(id, _)| *id)
            .collect::<BTreeSet<_>>(),
        "{failed:?}"
    );
    assert!(
        report.unrequired_records().is_empty(),
        "{:?}",
        report.unrequired_records()
    );
    assert!(report.excluded_records().is_empty());
    assert!(
        report.unsupported_witnesses().is_empty(),
        "{:?}",
        report.unsupported_witnesses()
    );
    assert!(
        report.missing_probes(ClaimLevel::Source).is_empty(),
        "{:?}",
        report.missing_probes(ClaimLevel::Source)
    );

    let unpopulated: Vec<&str> = inventory
        .probes()
        .filter(|(id, level)| {
            *level == ClaimLevel::Semantics
                && UNPOPULATED.iter().any(|family| id.starts_with(family))
        })
        .map(|(id, _)| id)
        .collect();
    assert_eq!(unpopulated.len(), UNPOPULATED.len());
    // Each semantic probe is exactly one of: satisfied; missing because its
    // family has no population; invalid only because pinned sub-runs have no run
    // yet; or failed as one of the pinned engine defects, with exactly its known
    // failing sub-runs. Any other failed, duplicated or unexpected record is
    // never tolerated.
    for account in report.probe_accounts(ClaimLevel::Semantics) {
        let unpopulated_row = unpopulated.contains(&account.id.as_str());
        let awaiting_pinned_runs = account.records == 1
            && !account.missing_subruns.is_empty()
            && account.unexpected_subruns.is_empty()
            && account.duplicated_subruns.is_empty()
            && account.failed_subruns.is_empty();
        let accepted = match account.state {
            ProbeState::Satisfied => !unpopulated_row,
            ProbeState::Missing => unpopulated_row,
            ProbeState::Invalid => !unpopulated_row && awaiting_pinned_runs,
            ProbeState::Failed => KNOWN_FAILED.iter().any(|(id, labels)| {
                *id == account.id
                    && account.records == 1
                    && account.missing_subruns.is_empty()
                    && account.unexpected_subruns.is_empty()
                    && account.duplicated_subruns.is_empty()
                    && account
                        .failed_subruns
                        .iter()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>()
                        == labels.iter().copied().collect::<BTreeSet<_>>()
            }),
        };
        assert!(accepted, "{}", account.describe());
    }
    assert_eq!(report.claim(), ClaimLevel::Source);
}

#[test]
fn the_production_verdict_json_reconciles_with_the_pinned_inventory() {
    let report = production::report(common::spec()).expect("the production report assembles");
    let inventory = report
        .obligations()
        .expect("a claim-capable verified inventory");
    let parsed = json::parse(&report.render_verdict_json()).expect("the verdict is JSON");
    assert_eq!(
        parsed.get("claim").and_then(Json::as_str),
        Some("source_conforming")
    );
    assert_eq!(
        parsed.get("mapping_digest").and_then(Json::as_str),
        Some(MAPPING_DIGEST)
    );
    let levels = parsed
        .get("levels")
        .and_then(Json::as_array)
        .expect("levels");
    assert_eq!(levels.len(), 2);
    for (level, entry) in [ClaimLevel::Source, ClaimLevel::Semantics]
        .into_iter()
        .zip(levels)
    {
        let required: BTreeSet<&str> = inventory
            .probes()
            .filter(|(_, required)| *required == level)
            .map(|(id, _)| id)
            .collect();
        let mut union = BTreeSet::new();
        let mut total = 0;
        for state in ProbeState::ALL {
            for id in entry.get(state.as_str()).and_then(Json::as_array).unwrap() {
                total += 1;
                let id = id.as_str().unwrap();
                assert!(union.insert(id), "{id} is in two states");
            }
        }
        assert_eq!(
            entry.get("required").and_then(Json::as_u64),
            Some(required.len() as u64)
        );
        assert_eq!(total, required.len());
        assert_eq!(union, required);
    }
}

#[test]
fn the_production_report_is_deterministic() {
    let first = production::report(common::spec()).expect("the production report assembles");
    let second = production::report(common::spec()).expect("the production report assembles");
    assert_eq!(first.render_verdict_json(), second.render_verdict_json());
    assert_eq!(first.render(), second.render());
}

#[test]
fn every_grouped_record_labels_each_run_exactly_once() {
    let report = production::report(common::spec()).expect("the production report assembles");
    let inventory = report
        .obligations()
        .expect("a claim-capable verified inventory");
    // Every populated semantic contract row is one grouped record.
    let expected_groups = inventory
        .probes()
        .filter(|(id, level)| {
            *level == ClaimLevel::Semantics
                && id.starts_with("semantic/")
                && !UNPOPULATED.iter().any(|family| id.starts_with(family))
        })
        .count();
    let mut groups = 0;
    let mut violations = Vec::new();
    for covered in report.executed() {
        let observed = &covered.case.observed;
        if observed.runs.is_empty() {
            continue;
        }
        groups += 1;
        let mut seen = BTreeSet::new();
        let repeated: BTreeSet<&str> = observed
            .run_labels
            .iter()
            .map(String::as_str)
            .filter(|label| !seen.insert(*label))
            .collect();
        if observed.run_labels.len() != observed.runs.len() || !repeated.is_empty() {
            violations.push(format!(
                "{}: {} runs, {} labels, repeated {repeated:?}",
                covered.case.id,
                observed.runs.len(),
                observed.run_labels.len()
            ));
        }
    }
    assert!(violations.is_empty(), "{violations:#?}");
    assert_eq!(groups, expected_groups);
}
