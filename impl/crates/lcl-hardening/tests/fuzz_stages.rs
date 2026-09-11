//! Phase B: every stage boundary, under every kind of hostile input.
//!
//! `06_TESTING`: "Malformed/untrusted source must not panic." This suite drives
//! the whole engine, not one layer, because the interesting failures are at the
//! seams: a document that survives the lexer and confuses the parser, or one
//! that resolves and then hands the checker a shape it has never seen.
//!
//! Every case runs under `catch_unwind` so a panic names the seed and the
//! bytes rather than aborting the run, and every returned report is checked
//! against `invariant::check_report`.

use lcl_hardening::corpus::{self, Corpus};
use lcl_hardening::{check_report, empty_provider, engine, invariant, unit, Rng};
use lcl_protocol::{Engine, Report};
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

/// The threshold `LCL_RELEASE_BASELINE.md` 5.1 sets for a generated corpus.
const GENERATED_CASES: usize = 2_000;

/// Carry one source through steps 1 to 5 without letting a panic escape.
fn checked(engine: &Engine, source: &str) -> Result<Report, String> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        engine.check(&unit(source), &empty_provider())
    }))
    .map_err(|payload| describe(&payload))
}

fn describe(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_string();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "a panic with no message".to_string()
}

/// Drive one corpus through the engine and assert every invariant.
fn drive(engine: &Engine, corpus: &Corpus, population: &str) {
    let registry = lcl_diagnostics::DiagnosticRegistry::load(engine.spec())
        .expect("the package is authoritative");
    for (label, source) in corpus.iter() {
        let report = match checked(engine, source) {
            Ok(report) => report,
            Err(message) => panic!("{population} case {label} panicked: {message}"),
        };
        let mut sources = BTreeMap::new();
        sources.insert("fuzz.lcl".to_string(), source.to_string());
        let violations = check_report(&report, &sources, &registry);
        assert!(
            violations.is_empty(),
            "{population} case {label} broke {} invariant(s): {}",
            violations.len(),
            violations
                .iter()
                .map(invariant::Violation::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
        // Serialization is the surface every consumer reads, and a report that
        // cannot be serialized is not a report. The CLI's human rendering is
        // driven separately, by running the binary.
        let json = panic::catch_unwind(AssertUnwindSafe(|| report.to_json().pretty()));
        assert!(
            json.is_ok(),
            "{population} case {label} produced a report that cannot be serialized"
        );
    }
}

#[test]
fn every_canonical_example_survives_every_stage() {
    let engine = engine();
    let corpus = corpus::canonical_examples(&lcl_hardening::canonical_root());
    assert_eq!(corpus.len(), 34, "13 valid and 21 invalid examples");
    drive(&engine, &corpus, "canonical");
}

#[test]
fn every_truncation_of_every_canonical_example_is_total() {
    let engine = engine();
    let examples = corpus::canonical_examples(&lcl_hardening::canonical_root());
    let mut corpus = Corpus::new();
    for (name, source) in examples.iter() {
        for (index, truncated) in corpus::truncations(source).into_iter().enumerate() {
            corpus.push(format!("{name}#{index}"), truncated);
        }
    }
    assert!(corpus.len() > 1_000, "{} truncations", corpus.len());
    drive(&engine, &corpus, "truncation");
}

#[test]
fn mutations_of_the_canonical_examples_are_total() {
    let engine = engine();
    let examples = corpus::canonical_examples(&lcl_hardening::canonical_root());
    let sources: Vec<String> = examples.iter().map(|(_, s)| s.to_string()).collect();
    let mut corpus = Corpus::new();
    for seed in 0..GENERATED_CASES as u64 {
        let mut rng = Rng::new(seed);
        let base = rng.below(sources.len());
        let mut mutated = sources[base].clone();
        // One to four mutations, so a case can be a near miss or a wreck.
        for _ in 0..=rng.below(4) {
            mutated = corpus::mutate(&mut rng, &mutated);
        }
        corpus.push(format!("seed {seed}"), mutated);
    }
    drive(&engine, &corpus, "mutation");
}

#[test]
fn documents_generated_from_the_real_vocabulary_are_total() {
    let engine = engine();
    let lexicon = engine.lexicon();
    let words: Vec<String> = lexicon.reserved_words().map(|w| w.to_string()).collect();
    assert!(!words.is_empty(), "the lexicon supplies the vocabulary");
    let mut corpus = Corpus::new();
    for seed in 0..GENERATED_CASES as u64 {
        let mut rng = Rng::new(seed ^ 0xA5A5_A5A5);
        corpus.push(
            format!("seed {seed}"),
            corpus::generated_document(&mut rng, &words, &words),
        );
    }
    drive(&engine, &corpus, "generated");
}

#[test]
fn arbitrary_bytes_are_total() {
    let engine = engine();
    let mut corpus = Corpus::new();
    for seed in 0..GENERATED_CASES as u64 {
        let mut rng = Rng::new(seed ^ 0x5A5A_5A5A);
        let length = rng.below(4_096);
        corpus.push(
            format!("seed {seed}"),
            corpus::arbitrary_bytes(&mut rng, length),
        );
    }
    drive(&engine, &corpus, "arbitrary");
}

#[test]
fn checking_the_same_source_twice_produces_the_same_report() {
    // `5.3 Determinism`: observable meaning may not depend on anything that is
    // not an explicit input. Two calls in one process is the weakest form of
    // that and the cheapest to run over a large corpus; `repeatability.rs`
    // takes it across processes.
    let engine = engine();
    let examples = corpus::canonical_examples(&lcl_hardening::canonical_root());
    let sources: Vec<String> = examples.iter().map(|(_, s)| s.to_string()).collect();
    for seed in 0..500u64 {
        let mut rng = Rng::new(seed ^ 0x1234_5678);
        let base = rng.below(sources.len());
        let mutated = corpus::mutate(&mut rng, &sources[base]);
        let first = engine.check(&unit(&mutated), &empty_provider());
        let second = engine.check(&unit(&mutated), &empty_provider());
        assert_eq!(
            first.to_json().pretty(),
            second.to_json().pretty(),
            "seed {seed} produced two different reports for one source"
        );
    }
}
