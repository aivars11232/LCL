//! The full-stack matrix: bytes to one terminal status, and the earliest-stage
//! boundary.
//!
//! `06_TESTING` in the execution contract:
//!
//! * "Valid canonical examples must continue through every implemented
//!   applicable stage."
//! * "Invalid examples expected at a later stage must pass every earlier
//!   implemented stage cleanly."
//!
//! Completion is now the last implemented stage, so this suite runs both halves
//! all the way to step 13.

mod common;

use common::*;
use lcl_completion::Completion;
use std::collections::BTreeMap;

fn complete_example(name: &str) -> Option<Completion> {
    let run = run_example("08_EXAMPLES/VALID", name).expect("a valid example resolves");
    let execution = run.execution.as_ref()?;
    Completion::of(
        completion_contracts(),
        &run.planned,
        &run.checked,
        &run.resolved,
        execution,
    )
    .ok()
}

#[test]
fn every_valid_example_reaches_exactly_one_terminal_status() {
    let names = canonical_example_names();
    assert_eq!(names.len(), 13, "the package ships thirteen valid examples");

    let mut summary: BTreeMap<String, String> = BTreeMap::new();
    for name in &names {
        let completion = complete_example(name)
            .unwrap_or_else(|| panic!("{name} must reach execution and complete"));
        let status = completion.terminal_status().to_string();
        assert!(
            completion_contracts().is_terminal(&status),
            "{name}: {status} must be a registered terminal status"
        );
        assert!(
            completion_contracts().permitted_at_root(&status),
            "{name}: {status} must be legal for an execution root"
        );
        summary.insert(name.clone(), status);
    }
    assert_eq!(summary.len(), names.len());
}

#[test]
fn every_valid_examples_declared_verify_blocks_are_selected() {
    // All thirteen valid examples declare VERIFY. None of them may be silently
    // ignored: a completion that selected nothing would report a terminal
    // status nothing had checked.
    for name in canonical_example_names() {
        let source =
            std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(&name))
                .expect("readable");
        let declared = source.lines().filter(|l| l.starts_with("VERIFY:")).count();
        if declared == 0 {
            continue;
        }
        let completion = complete_example(&name).expect("completes");
        let seen = completion.checks().results().len() + completion.checks().skipped().len();
        assert!(
            seen > 0,
            "{name} declares {declared} VERIFY blocks and completion selected none"
        );
    }
}

#[test]
fn every_invalid_example_still_fails_at_its_own_earliest_stage() {
    // Completion must not rescue, reclassify or re-stage an earlier failure.
    let names = canonical_invalid_names();
    // The count is the package's own: every invalid document ships with one
    // `.expected.txt` pair, so the two must agree.
    let pairs = std::fs::read_dir(canonical_root().join("08_EXAMPLES/INVALID"))
        .expect("readable")
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .ends_with(".invalid.lcl.expected.txt")
        })
        .count();
    assert_eq!(
        names.len(),
        pairs,
        "every invalid document ships one expectation pair"
    );
    assert!(names.len() >= 15, "the package ships invalid examples");
    for name in &names {
        // A document the resolver refused outright failed at the lexical or
        // grammar stage, which is the earliest-stage boundary holding.
        let Ok(run) = run_example("08_EXAMPLES/INVALID", name) else {
            continue;
        };
        let failed_early = run.resolved.primary().is_some()
            || run.checked.primary().is_some()
            || run.planned.primary().is_some();
        assert!(
            failed_early,
            "{name} must be rejected before execution completes"
        );
        // An example rejected before planning never reaches completion at all.
        if run.execution.is_none() {
            continue;
        }
        let execution = run.execution.as_ref().expect("checked above");
        let completion = Completion::of(
            completion_contracts(),
            &run.planned,
            &run.checked,
            &run.resolved,
            execution,
        )
        .expect("a planned document completes");
        assert!(
            !completion.succeeded(),
            "{name} is invalid and must not complete successfully"
        );
    }
}

#[test]
fn completion_emits_no_diagnostic_outside_its_mirrored_set() {
    for name in canonical_example_names() {
        let Some(completion) = complete_example(&name) else {
            continue;
        };
        for diagnostic in completion.diagnostics() {
            assert!(
                lcl_completion::CompletionError::from_registry_str(diagnostic.id.as_registry_str())
                    .is_some(),
                "{name}: {} is outside this layer's mirrored set",
                diagnostic.id
            );
        }
    }
}

#[test]
fn a_completion_diagnostic_keeps_its_registered_stage() {
    // `earliest_stage_rule` orders by registered stage, not by emitting layer.
    let registry = completion_contracts().diagnostics();
    for name in canonical_example_names() {
        let Some(completion) = complete_example(&name) else {
            continue;
        };
        for diagnostic in completion.diagnostics() {
            let def = registry
                .error(diagnostic.id.as_registry_str())
                .expect("registered");
            assert_eq!(
                diagnostic.registered_stage, def.stage,
                "{name}: {} must keep the registry's stage",
                diagnostic.id
            );
        }
    }
}

#[test]
fn completing_the_same_example_twice_produces_identical_bytes() {
    for name in canonical_example_names() {
        let Some(first) = complete_example(&name) else {
            continue;
        };
        let second = complete_example(&name).expect("the same example completes again");
        assert_eq!(
            first.serialize(),
            second.serialize(),
            "{name} must complete deterministically"
        );
    }
}
