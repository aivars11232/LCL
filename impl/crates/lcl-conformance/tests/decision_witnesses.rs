//! `semantic_case_execution`: the executed implementation gate.
//!
//! The canonical release validator classifies `semantic_case_execution` as
//! `OUT_OF_SCOPE` because the bare-language package ships no engine
//! (`validate_release.py`, the `catalog` scope). That classification is
//! correct and unchanged; this file is the executed implementation gate it
//! defers to, following the precedent `complete_example_parse_matrix` set in
//! `lcl-parser`'s `parse_matrix.rs`.
//!
//! Every one of the 66 decision witnesses is accounted for here, in exactly one
//! of three populations, and the suite asserts the accounting is total and
//! exact. A witness cannot be dropped, cannot pass without a concrete source
//! and an observed result, and cannot fail without the gap being named in
//! `witness_cases`.

mod common;
mod witness_cases;

use common::*;
use lcl_conformance::report::ClaimLevel;
use lcl_conformance::{ConformanceReport, ExecutedCase, Runner, Verdict};
use std::collections::BTreeSet;
use witness_cases::{Plan, Probe};

fn execute_probe(runner: &Runner, id: &str, contract: &str, probe: &Probe) -> ExecutedCase {
    probe.execute(runner, id, contract)
}

/// Run every witness and build the report.
fn run_all() -> (ConformanceReport, Vec<(String, ExecutedCase)>) {
    let runner = runner();
    let index = index();
    let mut report = ConformanceReport::for_spec(spec()).expect("complete verified inventory");
    let mut all: Vec<(String, ExecutedCase)> = Vec::new();

    for case in witness_cases::cases() {
        let contract = index
            .witnesses()
            .iter()
            .find(|w| w.id == case.id)
            .map(|w| w.contract.clone())
            .unwrap_or_else(|| panic!("{} is not in the canonical catalog", case.id));

        match &case.plan {
            Plan::Executable(probes) | Plan::NotImplemented { probes, .. } => {
                for probe in probes {
                    let executed = execute_probe(&runner, case.id, &contract, probe);
                    report.record(executed.clone(), case.coverage);
                    all.push((case.id.to_string(), executed));
                }
            }
            Plan::Descriptive { reason } => {
                report.record_descriptive(case.id, *reason);
            }
        }
        if let Plan::NotImplemented { missing, owner, .. } = &case.plan {
            report.record_descriptive(
                format!("{} (not implemented)", case.id),
                format!("{missing}. Owner: {owner}."),
            );
        }
    }
    (report, all)
}

/// Witness ids this build records as not yet implemented.
fn known_gaps() -> BTreeSet<String> {
    witness_cases::cases()
        .into_iter()
        .filter(|c| matches!(c.plan, Plan::NotImplemented { .. }))
        .map(|c| c.id.to_string())
        .collect()
}

#[test]
fn every_canonical_witness_is_accounted_for_exactly_once() {
    let index = index();
    let planned: Vec<&str> = witness_cases::cases().iter().map(|c| c.id).collect();
    let mut unique: BTreeSet<&str> = BTreeSet::new();
    for id in &planned {
        assert!(unique.insert(id), "{id} is planned twice");
    }
    let catalogued: BTreeSet<&str> = index.witnesses().iter().map(|w| w.id.as_str()).collect();

    let missing: Vec<&&str> = catalogued
        .iter()
        .filter(|id| !unique.contains(*id))
        .collect();
    assert!(
        missing.is_empty(),
        "these catalogued witnesses have no plan: {missing:?}"
    );
    let extra: Vec<&&str> = unique
        .iter()
        .filter(|id| !catalogued.contains(*id))
        .collect();
    assert!(
        extra.is_empty(),
        "these planned ids are not in the catalog: {extra:?}"
    );
    assert_eq!(unique.len(), index.witness_count());
}

#[test]
fn semantic_case_execution() {
    let (report, all) = run_all();
    let gaps = known_gaps();

    let mut unexpected_failures: Vec<String> = Vec::new();
    let mut unexpected_passes: Vec<String> = Vec::new();
    for (witness, case) in &all {
        let expected_to_fail = gaps.contains(witness);
        match (case.verdict, expected_to_fail) {
            (Verdict::Failed, false) => unexpected_failures.push(case.serialize()),
            (Verdict::Passed, true) => unexpected_passes.push(case.id.clone()),
            _ => {}
        }
    }

    assert!(
        unexpected_failures.is_empty(),
        "witnesses failed that this build claims to support:\n{}",
        unexpected_failures.join("\n")
    );
    assert!(
        unexpected_passes.is_empty(),
        "these witnesses are listed as not implemented but now pass; remove them from \
         witness_cases::cases so the gate stops under-claiming: {unexpected_passes:?}"
    );

    // The gate is only a gate if it actually ran something.
    assert!(
        report.executed_count() >= all.len(),
        "every planned probe must appear in the report"
    );
    assert!(
        report.executed_count() > 0,
        "semantic_case_execution executed nothing"
    );
}

#[test]
fn descriptive_requirements_are_never_counted_as_executed() {
    let (report, _) = run_all();
    assert_eq!(
        report.descriptive_count(),
        799,
        "the requirements index stays descriptive"
    );
    assert!(
        report.executed_count() < report.descriptive_count(),
        "a sanity check that the two counters are independent"
    );
    let rendered = report.render();
    assert!(rendered.contains("799 entries, all not_executed"));
    assert!(rendered.contains("EXECUTED CASES (concrete input, observed result)"));
}

#[test]
fn all_decision_witnesses_pass_but_do_not_substitute_for_source_and_other_contracts() {
    let (report, all) = run_all();
    assert_eq!(all.len(), 83);
    assert!(report.unsupported_witnesses().is_empty(), "{:?}", report.unsupported_witnesses());
    assert_eq!(report.failed_count(), 0);
    assert_eq!(report.claim(), ClaimLevel::None);
    assert!(!report.missing_probes(ClaimLevel::Source).is_empty());
    assert!(!report.missing_probes(ClaimLevel::Semantics).is_empty());
}

#[test]
fn the_report_states_everything_a_claim_requires() {
    // "A claimed result must state implementation version, host capabilities,
    // exact failed case IDs, and evidence."
    let (report, _) = run_all();
    let rendered = report.render();
    assert!(rendered.contains("version           :"));
    assert!(rendered.contains("host capabilities :"));
    assert!(rendered.contains("failed case IDs"));
    assert!(rendered.contains("evidence:"));
    assert!(rendered.contains("CLAIM:"));

    // And the claim itself is never stronger than the evidence.
    if report.failed_count() > 0 {
        assert_eq!(
            report.claim(),
            ClaimLevel::None,
            "a failed case withdraws the claim: {:?}",
            report.failed_ids()
        );
    }
}

#[test]
fn every_executed_case_carries_its_source_and_observation() {
    let (_, all) = run_all();
    for (_, case) in &all {
        assert!(!case.source.is_empty(), "{} has no concrete input", case.id);
        assert!(
            !case.observed.serialize().is_empty(),
            "{} has no implementation result",
            case.id
        );
    }
}

#[test]
fn the_gate_is_deterministic() {
    let (first, _) = run_all();
    let (second, _) = run_all();
    assert_eq!(first.render(), second.render());
}
