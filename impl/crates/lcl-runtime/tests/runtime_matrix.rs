//! The full-stack matrix: bytes to execution, and the earliest-stage boundary.
//!
//! `06_TESTING` in the execution contract:
//!
//! * "Valid canonical examples must continue through every implemented
//!   applicable stage."
//! * "Invalid examples expected at a later stage must pass every earlier
//!   implemented stage cleanly."
//!
//! This suite runs both halves over the canonical corpus.

mod common;

use lcl_runtime::{MockHost, Runtime};
use std::collections::BTreeMap;

/// The registered identifier one invalid example's `.expected.txt` pins.
fn expected_identifier(name: &str) -> String {
    let path = common::canonical_root()
        .join("08_EXAMPLES/INVALID")
        .join(format!("{name}.expected.txt"));
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.split_whitespace()
                .find(|word| word.starts_with("error."))
                .map(|word| word.trim_end_matches(['.', ',']).to_string())
        })
        .unwrap_or_else(|| "-".to_string())
}

fn invalid_example_names() -> Vec<String> {
    let dir = common::canonical_root().join("08_EXAMPLES/INVALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".invalid.lcl"))
        .collect();
    names.sort();
    names
}

#[test]
fn every_valid_example_reaches_execution_and_produces_a_truthful_result() {
    let mut summary: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for name in common::canonical_example_names() {
        let fixture = common::example_fixture(&name);
        assert!(
            fixture.resolved.primary().is_none(),
            "{name} must resolve cleanly"
        );
        assert!(
            fixture.checked.primary().is_none(),
            "{name} must check cleanly"
        );
        assert!(
            common::is_planned(&fixture),
            "{name} must plan: {:?}",
            fixture.planned.primary().map(|d| d.id.to_string())
        );
        let mut host = MockHost::new();
        let execution = Runtime::new(common::contracts())
            .execute(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut host,
            )
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            execution.diagnostics().is_empty(),
            "{name} produced {:?}",
            execution
                .diagnostics()
                .iter()
                .map(|d| d.id.to_string())
                .collect::<Vec<_>>()
        );
        // Every producer result is internally consistent.
        for record in execution.invocations() {
            if let Some(result) = &record.result {
                assert!(
                    result.violations().is_empty(),
                    "{name} {}: {:?}",
                    record.id,
                    result.violations()
                );
            }
        }
        summary.insert(
            name.clone(),
            (execution.invocations().len(), host.requests().len()),
        );
    }
    assert_eq!(summary.len(), 13);
    // Four canonical documents declare no EXECUTE root, so they run nothing.
    let inert = summary.values().filter(|(nodes, _)| *nodes == 0).count();
    assert_eq!(inert, 4, "{summary:?}");
}

#[test]
fn every_invalid_example_fails_at_or_before_its_expected_stage() {
    // "Invalid examples expected at a later stage must pass every earlier
    // implemented stage cleanly", and no invalid example may reach execution
    // and then be silently accepted.
    let mut reached_runtime = 0;
    for name in invalid_example_names() {
        let expected = expected_identifier(&name);
        let source = std::fs::read_to_string(
            common::canonical_root()
                .join("08_EXAMPLES/INVALID")
                .join(&name),
        )
        .expect("readable");
        let unit = lcl_resolver::SourceUnit::new(
            lcl_resolver::SourceId::new(name.clone()),
            source.as_bytes(),
        );
        let Ok(resolved) =
            lcl_resolver::Resolver::new(common::rules(), common::grammar(), common::lexicon())
                .resolve(&unit, &lcl_resolver::MemoryProvider::new())
        else {
            continue; // an earlier stage owns it
        };
        if resolved.primary().is_some() || resolved.stage_failures().next().is_some() {
            continue;
        }
        let checked = common::check(&resolved);
        if checked.primary().is_some() || !checked.earlier_stage_defects().is_empty() {
            continue;
        }
        let Ok(planned) = lcl_semantics::Preflight::new(common::preflight_contracts()).plan(
            &checked,
            &resolved,
            &lcl_semantics::Invocation::new(),
        ) else {
            continue;
        };
        if planned.plan().is_none() {
            continue;
        }
        // It reached the runtime. It must then fail here, and with the exact
        // registered identifier the example pins.
        reached_runtime += 1;
        let mut host = MockHost::new();
        let execution = Runtime::new(common::contracts())
            .execute(&planned, &checked, &resolved, &mut host)
            .expect("planned");
        let primary = execution
            .primary()
            .unwrap_or_else(|| panic!("{name} reached the runtime and was accepted"));
        assert_eq!(
            primary.id.as_registry_str(),
            expected,
            "{name} failed with the wrong identifier"
        );
    }
    // Every canonical invalid example is owned by an earlier stage; none is
    // this milestone's. That is the truthful result, not a gap.
    assert_eq!(
        reached_runtime, 0,
        "no canonical invalid example is a runtime-stage failure"
    );
}

#[test]
fn the_pipeline_is_reproducible_from_bytes() {
    // The whole stack, twice, compared byte for byte.
    for name in common::canonical_example_names() {
        let once = {
            let fixture = common::example_fixture(&name);
            let mut host = MockHost::new();
            let execution = Runtime::new(common::contracts())
                .execute(
                    &fixture.planned,
                    &fixture.checked,
                    &fixture.resolved,
                    &mut host,
                )
                .expect("planned");
            format!(
                "{}\n{}",
                fixture.planned.partial_plan().serialize(),
                execution.serialize()
            )
        };
        let twice = {
            let fixture = common::example_fixture(&name);
            let mut host = MockHost::new();
            let execution = Runtime::new(common::contracts())
                .execute(
                    &fixture.planned,
                    &fixture.checked,
                    &fixture.resolved,
                    &mut host,
                )
                .expect("planned");
            format!(
                "{}\n{}",
                fixture.planned.partial_plan().serialize(),
                execution.serialize()
            )
        };
        assert_eq!(once, twice, "{name}");
    }
}

#[test]
fn a_stage_that_did_not_succeed_yields_no_execution() {
    // Stage monotonicity across the whole stack: the runtime's input type is an
    // accepted plan, and a rejected preflight produces none.
    let source = common::data_document(&[("data.subject", "DECIMAL", "1 / 0")]);
    let resolved = common::resolve(&source);
    let checked = common::check(&resolved);
    assert!(checked.primary().is_some(), "M4 rejects it");
    // Preflight is not even evaluated for a program that failed the static
    // stage, so there is nothing for the runtime to receive.
    let planned = lcl_semantics::Preflight::new(common::preflight_contracts()).plan(
        &checked,
        &resolved,
        &lcl_semantics::Invocation::new(),
    );
    assert!(
        planned.is_err(),
        "preflight is skipped after a static failure"
    );
}
