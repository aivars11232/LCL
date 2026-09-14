//! Real runtime result-boundary probes; supplied observations are explicit input.
//!
//! The population lives in `lcl_conformance::result_cases`; this test
//! executes it and requires every schema group to pass.

mod common;

use lcl_conformance::result_cases::execute;
use lcl_conformance::{judge, Expectation, Verdict};

#[test]
fn result_schema_obligations_execute() {
    let runner = common::runner();
    let cases = execute(common::spec(), &runner);
    let mut failed = Vec::new();
    for case in &cases {
        if let Expectation::Runs(expected) = &case.expectation {
            for (i, (expected, actual)) in expected.iter().zip(&case.observed.runs).enumerate() {
                if judge(expected, actual) == Verdict::Failed {
                    eprintln!(
                        "FAIL {} subcase {i}: {} ; primary={:?}",
                        case.id,
                        expected.serialize(),
                        actual.primary
                    );
                }
            }
        }
        if case.verdict != Verdict::Passed {
            failed.push(case.id.clone());
        }
    }
    println!(
        "result schema groups: {}; sub-runs: {}",
        cases.len(),
        cases.iter().map(|c| c.observed.runs.len()).sum::<usize>()
    );
    assert!(failed.is_empty(), "failed schemas: {failed:?}");
}
