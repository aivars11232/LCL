//! The canonical example and fixture matrix, at the preflight stage.
//!
//! Three obligations from the execution contract's testing rules:
//!
//! * "Valid canonical examples must continue through every implemented
//!   applicable stage" — every valid example must plan;
//! * "Invalid examples expected at a later stage must pass every earlier
//!   implemented stage cleanly" — an invalid example whose expectation belongs
//!   to a later stage must not be rejected here, and one whose expectation is
//!   this layer's must be rejected here with exactly that identifier;
//! * malformed source must not panic.

mod common;

use common::*;
use lcl_semantics::Outcome;
use std::fs;

/// The identifier an invalid example's `.expected.txt` pins.
fn expected_identifier(name: &str) -> Option<String> {
    let path = canonical_root()
        .join("08_EXAMPLES/INVALID")
        .join(format!("{name}.expected.txt"));
    let text = fs::read_to_string(path).ok()?;
    text.split_whitespace()
        .find(|word| word.starts_with("error."))
        .map(|word| word.trim_end_matches(['.', ',']).to_string())
}

fn sorted_files(dir: &str, suffix: &str) -> Vec<(String, String)> {
    let dir = canonical_root().join(dir);
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .expect("the canonical package is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(suffix))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                fs::read_to_string(&p).expect("readable"),
            )
        })
        .collect()
}

#[test]
fn every_valid_canonical_example_plans() {
    let examples = sorted_files("08_EXAMPLES/VALID", ".lcl");
    assert_eq!(examples.len(), 13, "the package declares 13 valid examples");
    for (name, _) in &examples {
        let planned = plan_example(name);
        assert_eq!(
            planned.outcome(),
            Outcome::Planned,
            "{name} must plan: {:?}",
            planned
                .diagnostics()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert!(
            planned.plan().is_some(),
            "{name} planned, so a plan must be available to the runtime"
        );
    }
}

#[test]
fn every_invalid_canonical_example_is_consistent_with_its_expectation() {
    // The identifiers this layer decides, whatever stage the registry
    // classifies them under.
    const DECIDED_HERE: [&str; 12] = [
        "error.conflict.hard",
        "error.dependency.unsatisfied",
        "error.determinism.mismatch",
        "error.execution.order",
        "error.override.invalid",
        "error.permission.denied",
        "error.reference.cycle",
        "error.required.missing",
        "error.scope.violation",
        "error.validation.failed",
        "error.value.out_of_range",
        "error.value.unknown",
    ];

    let examples = sorted_files("08_EXAMPLES/INVALID", ".invalid.lcl");
    assert_eq!(
        examples.len(),
        21,
        "the package declares 21 invalid examples"
    );

    let mut decided_here = 0usize;
    let mut earlier = 0usize;

    for (name, source) in &examples {
        let expected = expected_identifier(name)
            .unwrap_or_else(|| panic!("{name} must pin an expected identifier"));

        // An example that fails an earlier stage never reaches preflight, which
        // is itself the earlier-stage rule holding.
        let resolved = match resolver().resolve(&unit(name, source), &canonical_example_provider())
        {
            Err(_) => {
                earlier += 1;
                continue;
            }
            Ok(resolved) => resolved,
        };
        if !resolved.diagnostics().is_empty() || resolved.stage_failures().next().is_some() {
            earlier += 1;
            continue;
        }
        let checked = match checker().check(&resolved) {
            Err(_) => {
                earlier += 1;
                continue;
            }
            Ok(checked) => checked,
        };
        if !checked.diagnostics().is_empty() || !checked.earlier_stage_defects().is_empty() {
            earlier += 1;
            continue;
        }

        let planned = preflight()
            .plan(&checked, &resolved, &lcl_semantics::Invocation::new())
            .expect("the static stage succeeded");
        let raised: Vec<String> = planned
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect();

        if DECIDED_HERE.contains(&expected.as_str()) {
            decided_here += 1;
            assert_eq!(
                planned.primary().map(|d| d.id.to_string()),
                Some(expected.clone()),
                "{name} pins {expected}, which this layer owns, so it must be primary here; raised {raised:?}"
            );
        } else {
            assert!(
                raised.is_empty(),
                "{name} pins {expected}, which a later stage owns, so preflight must raise nothing; raised {raised:?}"
            );
        }
    }

    assert!(
        decided_here >= 1,
        "at least the hard-conflict example must be decided here"
    );
    assert!(
        earlier >= 15,
        "most invalid examples fail earlier: {earlier}"
    );
}

#[test]
fn the_hard_conflict_example_is_the_one_this_layer_owns() {
    // M3's report names `error.conflict.hard` as "DEFERRED to M5". This is the
    // test that closes that hand-off against the canonical example itself.
    let source = fs::read_to_string(
        canonical_root().join("08_EXAMPLES/INVALID/08_HARD_CONFLICT.invalid.lcl"),
    )
    .expect("readable");
    let planned = plan(&source);
    assert_eq!(
        planned.primary().map(|d| d.id.to_string()),
        Some("error.conflict.hard".to_string())
    );
}

#[test]
fn every_source_fixture_is_total() {
    // Every fixture, valid or not, must either fail an earlier stage or plan
    // without panicking.
    for (name, source) in sorted_files("09_CONFORMANCE/SOURCE_FIXTURES", ".lcl") {
        let Ok(resolved) = resolver().resolve(&unit(&name, &source), &canonical_example_provider())
        else {
            continue;
        };
        let Ok(checked) = checker().check(&resolved) else {
            continue;
        };
        let _ = preflight().plan(&checked, &resolved, &lcl_semantics::Invocation::new());
    }
}
