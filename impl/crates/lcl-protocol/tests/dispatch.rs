//! LCL-FEATURE-04 D1: one rule chooses the engine that judges a document.
//!
//! Core 0.1.0 documents keep their exact Core 0.1.0 result. Documents that
//! declare 0.2.0, and documents whose localization failure decides their result
//! (owner decision D9), are judged by the 0.2.0 engine.

use lcl_localization::{CoverageDetector, LocaleTag, MemoryResolver};
use lcl_protocol::{Engine, Engines};
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

fn core() -> Engine {
    Engine::open(canonical("0.1.0")).expect("the approved 0.1.0 engine")
}

fn localized() -> Engine {
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
        .expect("localization contract")
}

fn fixture(name: &str) -> SourceUnit {
    let bytes = std::fs::read(fixtures().join("sources").join(name)).expect("source");
    SourceUnit::new(SourceId::new(name), bytes)
}

fn version_of(engines: &Engines, unit: &SourceUnit) -> String {
    engines
        .engine_for(unit)
        .spec_record()
        .formal_version
        .clone()
}

#[test]
fn a_core_0_1_0_document_keeps_its_exact_core_result() {
    let engines = Engines::new(core(), Some(localized())).expect("engines");
    let bytes =
        std::fs::read(canonical("0.1.0").join("09_CONFORMANCE/SOURCE_FIXTURES/valid_minimum.lcl"))
            .expect("0.1.0 fixture");
    let unit = SourceUnit::new(SourceId::new("valid_minimum.lcl"), bytes);
    assert_eq!(version_of(&engines, &unit), "0.1.0");
    let provider = MemoryProvider::new();
    let through_front = format!("{:?}", engines.engine_for(&unit).check(&unit, &provider));
    let direct = format!("{:?}", core().check(&unit, &provider));
    assert_eq!(through_front, direct);

    for invalid in [
        "invalid_tab.lcl",
        "invalid_typographic_quote.lcl",
        "invalid_hash.lcl",
    ] {
        let bytes = std::fs::read(
            canonical("0.1.0")
                .join("09_CONFORMANCE/SOURCE_FIXTURES")
                .join(invalid),
        )
        .expect("0.1.0 invalid fixture");
        let unit = SourceUnit::new(SourceId::new(invalid), bytes);
        assert_eq!(version_of(&engines, &unit), "0.1.0", "{invalid}");
    }
}

#[test]
fn documents_declaring_0_2_0_go_to_the_0_2_0_engine() {
    let engines = Engines::new(core(), Some(localized())).expect("engines");
    for name in [
        "canonical_en.lcl",
        "auto_lv.lcl",
        "auto_ru.lcl",
        "auto_zh.lcl",
        "explicit_nl.lcl",
        "strings_in_other_languages_auto.lcl",
        "canonical_with_foreign_string.lcl",
    ] {
        assert_eq!(version_of(&engines, &fixture(name)), "0.2.0", "{name}");
    }
}

#[test]
fn decision_d9_decides_failed_localizations() {
    let engines = Engines::new(core(), Some(localized())).expect("engines");
    for name in [
        "mixed_canonical_word_explicit.lcl",
        "mixed_canonical_word_auto.lcl",
        "ambiguous_mixed_lv_nl.lcl",
        "unavailable_explicit_locale.lcl",
        "directive_second_line.lcl",
        "locale_tag_malformed.lcl",
        "confusable_mixed_script_word.lcl",
    ] {
        assert_eq!(version_of(&engines, &fixture(name)), "0.2.0", "{name}");
    }
    // No directive and no profile recognised a word: the canonical reading's
    // result stands.
    assert_eq!(
        version_of(&engines, &fixture("detection_failed_unknown_word.lcl")),
        "0.1.0"
    );
}

#[test]
fn without_a_localized_engine_everything_is_core() {
    let engines = Engines::new(core(), None).expect("engines");
    assert_eq!(version_of(&engines, &fixture("auto_lv.lcl")), "0.1.0");
    assert!(Engines::new(localized(), None).is_err());
    assert!(Engines::new(core(), Some(core())).is_err());
}
