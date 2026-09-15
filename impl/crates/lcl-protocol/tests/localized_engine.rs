//! LCL-FEATURE-04 D1: the engine applies the Core 0.2.0 localization stage.
//!
//! A 0.2.0 engine with the package's four fixture profiles checks equivalent
//! localized documents exactly as it checks canonical English. A document the
//! localization stage rejects stops at that stage, with registered
//! diagnostics on the author's own bytes. A Core 0.1.0 engine neither accepts a
//! 0.2.0 document nor takes the localization stage.

use lcl_diagnostics::Stage;
use lcl_localization::{CoverageDetector, LocaleTag, MemoryResolver};
use lcl_protocol::{Engine, Outcome, Reached, Report};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn canonical(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("canonical package present")
}

fn fixtures() -> PathBuf {
    canonical("0.2.0").join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
}

fn localized_engine() -> Engine {
    let spec = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    let mut profiles = MemoryResolver::new("lcl.fixture.memory");
    for locale in ["lv-LV", "nl-NL", "ru-RU", "zh-CN"] {
        let bytes = std::fs::read(fixtures().join("profiles").join(format!("{locale}.json")))
            .expect("profile");
        profiles.insert(LocaleTag::parse(locale).expect("tag"), bytes);
    }
    Engine::assemble(spec)
        .expect("the 0.2.0 engine assembles")
        .with_localization(Arc::new(profiles), Arc::new(CoverageDetector))
        .expect("the 0.2.0 package carries the localization contract")
}

fn check(engine: &Engine, name: &str) -> (Vec<u8>, Report) {
    let bytes = std::fs::read(fixtures().join("sources").join(name)).expect("source");
    let unit = SourceUnit::new(SourceId::new(name), bytes.clone());
    let report = engine.check(&unit, &MemoryProvider::new());
    (bytes, report)
}

#[test]
fn localized_documents_check_exactly_like_canonical_english() {
    let engine = localized_engine();
    let (_, canonical) = check(&engine, "canonical_en.lcl");
    assert!(
        canonical.diagnostics.is_empty(),
        "{:?}",
        canonical.diagnostics
    );
    assert_eq!(canonical.outcome, Outcome::Accepted);
    for name in [
        "auto_lv.lcl",
        "auto_nl.lcl",
        "auto_ru.lcl",
        "auto_zh.lcl",
        "explicit_lv.lcl",
        "explicit_nl.lcl",
        "explicit_ru.lcl",
        "explicit_zh.lcl",
    ] {
        let (_, report) = check(&engine, name);
        assert!(
            report.diagnostics.is_empty(),
            "{name}: {:?}",
            report.diagnostics
        );
        assert_eq!(report.outcome, canonical.outcome, "{name}: outcome");
        assert_eq!(report.reached, canonical.reached, "{name}: reached");
    }
}

#[test]
fn a_rejected_localization_stops_at_the_localization_stage() {
    let engine = localized_engine();
    let (bytes, report) = check(&engine, "mixed_canonical_word_explicit.lcl");
    assert_eq!(report.outcome, Outcome::Rejected);
    assert_eq!(report.reached, Reached::Lexical);
    let primary = report
        .diagnostics
        .iter()
        .find(|d| d.primary)
        .expect("a primary diagnostic");
    assert_eq!(primary.id, "error.localization.mixed");
    assert_eq!(primary.stage, Stage::Localization);
    assert_eq!((primary.span.start, primary.span.end), (169, 173));
    assert_eq!(
        std::str::from_utf8(&bytes[primary.span.start..primary.span.end]).expect("utf-8"),
        "DATA"
    );
    assert_eq!((primary.position.line, primary.position.column), (11, 1));
    assert_eq!(primary.default_status, "status.invalid");
    assert!(!primary.meaning.is_empty());

    let (_, ambiguous) = check(&engine, "ambiguous_mixed_lv_nl.lcl");
    let primary = ambiguous
        .diagnostics
        .iter()
        .find(|d| d.primary)
        .expect("a primary diagnostic");
    assert_eq!(primary.id, "error.localization.detection_ambiguous");
    assert_eq!(primary.stage, Stage::Localization);
    assert_eq!(primary.span.start, 9);
}

#[test]
fn an_unmapped_word_after_selection_is_a_lexical_diagnostic() {
    let engine = localized_engine();
    let (_, report) = check(&engine, "multibyte_offset_unknown_word.lcl");
    let primary = report
        .diagnostics
        .iter()
        .find(|d| d.primary)
        .expect("a primary diagnostic");
    assert_eq!(primary.id, "error.keyword.unknown");
    assert_eq!(primary.stage, Stage::Lexical);
    assert_eq!(primary.span.start, 220);
}

#[test]
fn a_core_0_1_0_engine_takes_no_localization_and_rejects_0_2_0_source() {
    let core = Engine::open(canonical("0.1.0")).expect("the approved 0.1.0 engine");
    let (_, report) = check(&core, "canonical_en.lcl");
    assert_eq!(report.outcome, Outcome::Rejected);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.id == "error.version.unsupported"),
        "{:?}",
        report.diagnostics
    );
    let refused = core.with_localization(
        Arc::new(MemoryResolver::new("lcl.fixture.memory")),
        Arc::new(CoverageDetector),
    );
    assert!(refused.is_err());
}
