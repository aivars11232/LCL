//! Concrete operation binding, effect and error evidence, shared with reporting.
//!
//! The population lives in `lcl_conformance::operation_cases`; this test
//! executes it and requires every group to pass.

mod common;

use lcl_conformance::operation_cases::execute;
use lcl_conformance::{judge, Expectation, Verdict};

#[test]
fn operation_obligations_execute() {
    let runner = common::runner();
    let cases = execute(common::spec(), &runner);
    let mut failed = Vec::new();
    for case in &cases {
        if case.verdict == Verdict::Failed {
            eprintln!("{}", case.serialize());
            if let Expectation::Runs(expected) = &case.expectation {
                for (i, (e, o)) in expected.iter().zip(&case.observed.runs).enumerate() {
                    if judge(e, o) == Verdict::Failed {
                        eprintln!(
                            "FAIL {} subcase {i}: {} ; primary={:?} component={:?}",
                            case.id,
                            e.serialize(),
                            o.primary,
                            o.component
                        );
                    }
                }
            }
            failed.push(case.id.clone());
        }
    }
    println!(
        "operation groups={} sub-runs={}",
        cases.len(),
        cases.iter().map(|c| c.observed.runs.len()).sum::<usize>()
    );
    assert!(
        failed.is_empty(),
        "failed operation obligations: {failed:?}"
    );
}
