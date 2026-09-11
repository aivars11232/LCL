//! The executable static gate over every canonical input.
//!
//! `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` requires that valid
//! examples pass every implemented stage and that invalid examples fail at
//! their own stage and no earlier. This is that check for the static stage, run
//! against the approved package rather than against a copied expectation: each
//! invalid example's pinned identifier and terminal status are read from its own
//! `.expected.txt` at test time.

mod common;

use lcl_checker::Outcome;
use lcl_resolver::MemoryProvider;
use std::fs;
use std::path::{Path, PathBuf};

fn examples(dir: &str) -> Vec<PathBuf> {
    let root = common::canonical_root().join("08_EXAMPLES").join(dir);
    let mut out: Vec<PathBuf> = fs::read_dir(root)
        .expect("examples directory is present")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "lcl"))
        .collect();
    out.sort();
    out
}

fn name(path: &Path) -> String {
    path.file_name()
        .expect("file name")
        .to_string_lossy()
        .to_string()
}

/// A provider holding every valid example, so `03` can import `02` by its own
/// declared source name and nothing else can enter.
fn valid_provider() -> MemoryProvider {
    let mut provider = MemoryProvider::new();
    for path in examples("VALID") {
        provider.insert(name(&path), fs::read(&path).expect("readable"));
    }
    provider
}

fn expectation(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path.with_extension("lcl.expected.txt")).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix(key).map(str::trim).map(str::to_string))
}

#[test]
fn every_valid_example_passes_the_static_stage() {
    let provider = valid_provider();
    let mut checked_count = 0usize;
    for path in examples("VALID") {
        let file = name(&path);
        let source = fs::read_to_string(&path).expect("readable");
        let resolved = common::resolver()
            .resolve(&common::unit(&file, &source), &provider)
            .expect("a valid example passes every earlier stage");
        assert_eq!(
            resolved.diagnostics().len(),
            0,
            "{file} must resolve cleanly"
        );

        let checked = common::checker()
            .check(&resolved)
            .expect("resolution succeeded");
        assert_eq!(
            checked.outcome(),
            Outcome::Checked,
            "{file}: {:?} {:?}",
            checked
                .diagnostics()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            checked
                .earlier_stage_defects()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert!(
            checked.annotation_count() > 0,
            "{file} must annotate its expressions"
        );
        checked_count += 1;
    }
    assert_eq!(checked_count, 13, "every valid example was checked");
}

#[test]
fn every_invalid_example_fails_at_its_own_stage_and_no_earlier() {
    let mut owned_here = 0usize;
    let mut owned_earlier = 0usize;
    let mut owned_later = 0usize;

    for path in examples("INVALID") {
        let file = name(&path);
        let source = fs::read_to_string(&path).expect("readable");
        let want = expectation(&path, "EXPECTED_ERROR:").expect("every invalid example pins one");
        let want_status =
            expectation(&path, "EXPECTED_TERMINAL_STATUS:").expect("and one terminal status");

        // An earlier stage owning it means the static stage is never evaluated.
        let Ok(resolved) =
            common::resolver().resolve(&common::unit(&file, &source), &MemoryProvider::new())
        else {
            owned_earlier += 1;
            continue;
        };
        if !resolved.diagnostics().is_empty() {
            owned_earlier += 1;
            continue;
        }

        let checked = common::checker()
            .check(&resolved)
            .expect("resolution succeeded");
        let raised: Vec<String> = checked
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect();

        if raised.contains(&want) {
            // This stage owns it: the pinned identifier is primary, at its own
            // locus, with its registered terminal status.
            let primary = checked.primary().expect("a raised diagnostic is primary");
            assert_eq!(primary.id.to_string(), want, "{file} primary identifier");
            assert_eq!(
                checked.terminal_status(),
                Some(want_status.as_str()),
                "{file} terminal status"
            );
            assert!(
                primary.span.start < source.len(),
                "{file} locus is inside the source"
            );
            owned_here += 1;
        } else {
            // A later stage owns it, so this stage must raise nothing at all.
            assert_eq!(
                checked.outcome(),
                Outcome::Checked,
                "{file} expects {want} at a later stage, so the static stage must raise nothing: {raised:?}"
            );
            owned_later += 1;
        }
    }

    // Twenty-one invalid examples: twelve fail lexically or grammatically,
    // three at resolution, five here, and one is M5's.
    assert_eq!(owned_earlier, 15, "earlier stages own fifteen");
    assert_eq!(owned_here, 5, "the static stage owns five");
    assert_eq!(owned_later, 1, "M5 owns error.conflict.hard");
    assert_eq!(owned_earlier + owned_here + owned_later, 21);
}

#[test]
fn the_five_static_examples_raise_exactly_their_pinned_identifier() {
    // Named explicitly so a regression names the example rather than a count.
    for (file, identifier) in [
        ("07_TYPE_MISMATCH.invalid.lcl", "error.type.mismatch"),
        (
            "18_NON_TERMINATING_DIVISION.invalid.lcl",
            "error.numeric.non_terminating",
        ),
        (
            "19_DIVISION_BY_ZERO.invalid.lcl",
            "error.numeric.division_by_zero",
        ),
        (
            "20_UNORDERED_SET_DIRECT_ITERATION.invalid.lcl",
            "error.type.mismatch",
        ),
        (
            "21_SORT_STABLE_PARAMETER.invalid.lcl",
            "error.operation.parameter",
        ),
    ] {
        let path = common::canonical_root()
            .join("08_EXAMPLES/INVALID")
            .join(file);
        let source = fs::read_to_string(&path).expect("readable");
        let resolved = common::resolver()
            .resolve(&common::unit(file, &source), &MemoryProvider::new())
            .expect("earlier stages pass");
        let checked = common::checker()
            .check(&resolved)
            .expect("resolution succeeded");
        assert_eq!(
            checked.primary().map(|d| d.id.to_string()),
            Some(identifier.to_string()),
            "{file}"
        );
        assert_eq!(checked.terminal_status(), Some("status.invalid"), "{file}");
    }
}

#[test]
fn a_program_that_failed_an_earlier_stage_has_no_static_verdict() {
    // Stage monotonicity is in the signature: there is no way to ask for a
    // static verdict on a program the resolver rejected.
    let source = format!(
        "{}\nDATA:\n    ID: data.x\n    TYPE: INTEGER\n    VALUE: REF(data.missing)\n",
        common::HEADER
    );
    let resolved = common::resolve(&source);
    assert!(!resolved.diagnostics().is_empty(), "resolution must fail");
    let skipped = common::checker()
        .check(&resolved)
        .expect_err("the static stage is not evaluated");
    assert_eq!(skipped.stage, lcl_diagnostics::Stage::Resolution);
    assert_eq!(skipped.primary, "error.reference.unresolved");
}

#[test]
fn every_source_fixture_is_total() {
    let root = common::canonical_root().join("09_CONFORMANCE/SOURCE_FIXTURES");
    let mut reached = 0usize;
    let mut total = 0usize;
    for entry in fs::read_dir(root).expect("fixtures") {
        let path = entry.expect("entry").path();
        if !path.extension().is_some_and(|ext| ext == "lcl") {
            continue;
        }
        total += 1;
        let file = name(&path);
        let bytes = fs::read(&path).expect("readable");
        let unit = lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new(&file), bytes);
        let Ok(resolved) = common::resolver().resolve(&unit, &MemoryProvider::new()) else {
            continue;
        };
        if !resolved.diagnostics().is_empty() {
            continue;
        }
        // Never panics, whatever the bytes were.
        let _ = common::checker().check(&resolved);
        reached += 1;
    }
    assert_eq!(total, 15, "fifteen source fixtures");
    assert!(reached > 0, "at least one fixture reaches the static stage");
}

#[test]
fn no_deferred_identifier_is_ever_emitted() {
    let provider = valid_provider();
    for dir in ["VALID", "INVALID"] {
        for path in examples(dir) {
            let file = name(&path);
            let source = fs::read_to_string(&path).expect("readable");
            let Ok(resolved) = common::resolver().resolve(&common::unit(&file, &source), &provider)
            else {
                continue;
            };
            let Ok(checked) = common::checker().check(&resolved) else {
                continue;
            };
            for diagnostic in checked.diagnostics() {
                assert!(
                    !diagnostic.id.is_deferred(),
                    "{file} emitted the deferred identifier {}",
                    diagnostic.id
                );
            }
        }
    }
}
