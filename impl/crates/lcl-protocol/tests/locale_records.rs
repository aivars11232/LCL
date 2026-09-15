//! LCL-FEATURE-04 D2: reports carry each unit's locale selection.
//!
//! The record is presentation and reproducibility metadata: the locale, how it
//! was selected, and the exact content identity of the profile that applied.

use lcl_localization::{content_identity, CoverageDetector, LocaleTag, MemoryResolver};
use lcl_protocol::{Engine, Report};
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

fn profile(locale: &str) -> Vec<u8> {
    std::fs::read(fixtures().join("profiles").join(format!("{locale}.json"))).expect("profile")
}

fn engine() -> Engine {
    let spec = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    let mut profiles = MemoryResolver::new("lcl.fixture.memory");
    for locale in ["lv-LV", "nl-NL", "ru-RU", "zh-CN"] {
        profiles.insert(LocaleTag::parse(locale).expect("tag"), profile(locale));
    }
    Engine::assemble(spec)
        .expect("assembles")
        .with_localization(Arc::new(profiles), Arc::new(CoverageDetector))
        .expect("localization contract")
}

fn check(engine: &Engine, name: &str) -> Report {
    let bytes = std::fs::read(fixtures().join("sources").join(name)).expect("source");
    engine.check(
        &SourceUnit::new(SourceId::new(name), bytes),
        &MemoryProvider::new(),
    )
}

#[test]
fn selected_locales_are_recorded_with_their_profile_identity() {
    let engine = engine();
    for (name, method, locale) in [
        ("auto_lv.lcl", "auto", "lv-LV"),
        ("auto_zh.lcl", "auto", "zh-CN"),
        ("explicit_ru.lcl", "explicit", "ru-RU"),
        ("explicit_nl.lcl", "explicit", "nl-NL"),
    ] {
        let report = check(&engine, name);
        let record = report.units[0].locale.as_ref().expect("a locale record");
        assert_eq!(record.method, method, "{name}");
        assert_eq!(record.locale.as_deref(), Some(locale), "{name}");
        let identity = content_identity(&profile(locale));
        assert_eq!(
            record.profile_identity.as_deref(),
            Some(identity.as_str()),
            "{name}"
        );
        assert_eq!(record.lcl_version, "0.2.0");
        if method == "auto" {
            assert_eq!(
                record.detector_identity.as_deref(),
                Some("lcl.detector.profile_coverage/1")
            );
            assert_eq!(record.candidate_locales.len(), 4);
        } else {
            assert!(record.detector_identity.is_none());
        }
        let json = report.to_json().compact();
        assert!(json.contains("\"locale\":{"), "{name}: {json}");
        assert!(json.contains(&identity), "{name}");
    }
}

#[test]
fn canonical_failed_and_unselected_units_are_recorded_honestly() {
    let engine = engine();
    let canonical_report = check(&engine, "canonical_en.lcl");
    let record = canonical_report.units[0]
        .locale
        .as_ref()
        .expect("canonical selection record");
    assert_eq!(record.method, "canonical");
    assert!(record.locale.is_none() && record.profile_identity.is_none());

    let mixed = check(&engine, "mixed_canonical_word_explicit.lcl");
    let record = mixed.units[0]
        .locale
        .as_ref()
        .expect("selection before the mixed word");
    assert_eq!(
        (record.method.as_str(), record.locale.as_deref()),
        ("explicit", Some("lv-LV"))
    );

    let ambiguous = check(&engine, "ambiguous_mixed_lv_nl.lcl");
    assert!(ambiguous.units[0].locale.is_none());

    let core = Engine::open(canonical("0.1.0")).expect("0.1.0 engine");
    let bytes =
        std::fs::read(canonical("0.1.0").join("09_CONFORMANCE/SOURCE_FIXTURES/valid_minimum.lcl"))
            .expect("fixture");
    let report = core.check(
        &SourceUnit::new(SourceId::new("valid_minimum.lcl"), bytes),
        &MemoryProvider::new(),
    );
    assert!(report.units.iter().all(|u| u.locale.is_none()));
    assert!(!report.to_json().compact().contains("\"locale\""));
}
