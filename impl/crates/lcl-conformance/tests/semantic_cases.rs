//! Concrete additional semantic obligations, shared by acceptance and reporting.
//! A row may require multiple runs; each exact input and observation is retained.
//!
//! The population lives in `lcl_conformance::semantic_cases`; these tests
//! execute it and require every group to pass except the pinned groups whose
//! sub-runs expose engine defects.

mod common;

use lcl_conformance::semantic_cases::{execute, execute_operators};
use lcl_conformance::{judge, Expectation, Verdict};

/// Groups whose sub-runs expose engine defects while full semantic conformance
/// stays BLOCKED (LCL-CLOSE-02; residual report, "P4c, operators and functions"),
/// in sorted order. The production report test pins their exact failing
/// sub-runs. A repair, or any new failure, changes this list.
const KNOWN_FAILED_GROUPS: [&str; 8] = [
    "semantic/function_invalid/ROUND",
    "semantic/function_invalid/SUM",
    "semantic/operator_invalid//",
    "semantic/operator_invalid/MATCHES",
    "semantic/operator_valid/!=",
    "semantic/operator_valid/-",
    "semantic/operator_valid/==",
    "semantic/operator_valid/MATCHES",
];

#[test]
fn additional_semantic_cases_execute() {
    let runner = common::runner();
    let cases = execute(common::spec(), &runner);
    let mut failed = Vec::new();
    for case in &cases {
        if case.verdict != Verdict::Passed {
            eprintln!("{}", case.serialize());
            for (i, (expectation, observed)) in match &case.expectation {
                Expectation::Runs(runs) => runs,
                _ => unreachable!(),
            }
            .iter()
            .zip(&case.observed.runs)
            .enumerate()
            {
                if judge(expectation, observed) != Verdict::Passed {
                    eprintln!(
                        "FAIL {} subcase {i}: {} ; stage={} primary={:?}",
                        case.id,
                        expectation.serialize(),
                        observed.reached,
                        observed.primary
                    );
                }
            }
            failed.push(case.id.clone());
        }
    }
    println!(
        "additional semantic groups: {} ; sub-runs: {}",
        cases.len(),
        cases.iter().map(|c| c.observed.runs.len()).sum::<usize>()
    );
    failed.sort();
    assert_eq!(
        failed.iter().map(String::as_str).collect::<Vec<_>>(),
        KNOWN_FAILED_GROUPS,
        "failed semantic obligations: {failed:?}"
    );
}

#[test]
fn registered_measure_products_execute() {
    let runner = common::runner();
    let case = execute_operators(&runner)
        .into_iter()
        .find(|c| c.id == "semantic/operator_valid/*")
        .unwrap();
    assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
}
