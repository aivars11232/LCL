//! The executable resolution matrix over the canonical example set.
//!
//! The test form of `examples/m3_report.rs`: every valid example must resolve,
//! and every invalid example must be consistent with the stage that owns its
//! pinned identifier.

mod common;

use common::{canonical_root, resolver, spec};
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_resolver::{MemoryProvider, Outcome, ResolutionError, SourceId, SourceUnit};
use std::path::{Path, PathBuf};

fn sorted_lcl(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lcl"))
        .collect();
    out.sort();
    out
}

fn name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

/// A provider holding every valid example under its own filename, so the one
/// example that imports a sibling resolves exactly as written.
fn valid_provider() -> (MemoryProvider, Vec<PathBuf>) {
    let valid = sorted_lcl(&canonical_root().join("08_EXAMPLES/VALID"));
    let mut provider = MemoryProvider::new();
    for path in &valid {
        provider.insert(name(path), std::fs::read(path).expect("read"));
    }
    (provider, valid)
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(": ")))
        .map(|v| v.trim().to_string())
}

#[test]
fn every_valid_example_resolves() {
    let (provider, valid) = valid_provider();
    assert_eq!(valid.len(), 13);
    for path in &valid {
        let unit = SourceUnit::new(
            SourceId::new(name(path)),
            std::fs::read(path).expect("read"),
        );
        let resolved = resolver()
            .resolve(&unit, &provider)
            .unwrap_or_else(|e| panic!("{}: {e}", name(path)));
        assert_eq!(
            resolved
                .diagnostics()
                .iter()
                .map(|d| d.id.to_string())
                .collect::<Vec<_>>(),
            Vec::<String>::new(),
            "{} must resolve cleanly",
            name(path)
        );
        assert_eq!(resolved.outcome(), Outcome::Resolved, "{}", name(path));
    }
}

#[test]
fn every_invalid_example_is_consistent_with_its_owning_stage() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics load");
    let (provider, _) = valid_provider();
    let invalid = sorted_lcl(&canonical_root().join("08_EXAMPLES/INVALID"));
    assert_eq!(invalid.len(), 21);

    let mut counted = (0usize, 0usize, 0usize, 0usize);
    for path in &invalid {
        let expectation =
            std::fs::read_to_string(format!("{}.expected.txt", path.display())).expect("expected");
        let want_error = field(&expectation, "EXPECTED_ERROR").expect("EXPECTED_ERROR");
        let want_status =
            field(&expectation, "EXPECTED_TERMINAL_STATUS").expect("EXPECTED_TERMINAL_STATUS");
        let stage = registry.error(&want_error).expect("registered").stage;
        let unit = SourceUnit::new(
            SourceId::new(name(path)),
            std::fs::read(path).expect("read"),
        );
        let deferred = ResolutionError::from_registry_str(&want_error)
            .is_some_and(ResolutionError::is_deferred);

        match resolver().resolve(&unit, &provider) {
            Err(skipped) => {
                counted.0 += 1;
                assert!(
                    stage == Stage::Lexical || stage == Stage::GrammarOrSchema,
                    "{}: resolution was skipped for a {stage} expectation ({})",
                    name(path),
                    skipped.primary
                );
            }
            Ok(resolved) => {
                let raised: Vec<String> = resolved
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect();
                if deferred {
                    counted.1 += 1;
                    // A deferred identifier is not decided here, and this
                    // milestone must not invent a verdict for it either way.
                    assert!(
                        raised.is_empty(),
                        "{}: a deferred expectation must raise nothing here, got {raised:?}",
                        name(path)
                    );
                } else if stage == Stage::Resolution {
                    counted.2 += 1;
                    assert!(
                        raised.contains(&want_error),
                        "{}: expected {want_error}, got {raised:?}",
                        name(path)
                    );
                    assert_eq!(
                        resolved.terminal_status(),
                        Some(want_status.as_str()),
                        "{}",
                        name(path)
                    );
                } else {
                    counted.3 += 1;
                    // earliest_stage_rule: a later-stage expectation must pass
                    // every earlier implemented stage cleanly.
                    assert!(
                        raised.is_empty(),
                        "{}: a {stage} expectation must resolve cleanly, got {raised:?}",
                        name(path)
                    );
                }
            }
        }
    }
    assert_eq!(
        counted,
        (12, 1, 3, 5),
        "earlier / deferred / resolution / later"
    );
}

#[test]
fn every_source_fixture_is_total() {
    // 09_CONFORMANCE/SOURCE_FIXTURES are mostly lexically invalid; none may
    // panic, and each must produce either a skipped stage or a verdict.
    let (provider, _) = valid_provider();
    let fixtures = sorted_lcl(&canonical_root().join("09_CONFORMANCE/SOURCE_FIXTURES"));
    assert_eq!(fixtures.len(), 15);
    let mut reached = 0usize;
    for path in &fixtures {
        let unit = SourceUnit::new(
            SourceId::new(name(path)),
            std::fs::read(path).expect("read"),
        );
        if resolver().resolve(&unit, &provider).is_ok() {
            reached += 1;
        }
    }
    assert_eq!(
        reached, 2,
        "two fixtures are valid through the grammar stage"
    );
}

#[test]
fn no_deferred_identifier_is_ever_emitted() {
    // The two deferred identifiers must not appear from any canonical input.
    let (provider, valid) = valid_provider();
    let invalid = sorted_lcl(&canonical_root().join("08_EXAMPLES/INVALID"));
    for path in valid.iter().chain(invalid.iter()) {
        let unit = SourceUnit::new(
            SourceId::new(name(path)),
            std::fs::read(path).expect("read"),
        );
        if let Ok(resolved) = resolver().resolve(&unit, &provider) {
            for diagnostic in resolved.diagnostics() {
                assert!(
                    !diagnostic.id.is_deferred(),
                    "{} emitted the deferred {}",
                    name(path),
                    diagnostic.id
                );
            }
        }
    }
}
