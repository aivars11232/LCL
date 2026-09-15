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

/// LCL-REPAIR-03 B-11: every document is attributed to exactly one package,
/// and its report names that package. A defect after a readable `VERSION`
/// never moves a document to the other package; a document whose `VERSION`
/// is not readable declares nothing and keeps its Core 0.1.0 reading unless
/// D9 gives its localization failure the result.
#[test]
fn the_dispatch_matrix_attributes_each_document_to_one_package() {
    use lcl_diagnostics::Stage::{self, GrammarOrSchema, Lexical, Localization};

    let profiled = Engines::new(core(), Some(localized())).expect("engines");
    let unprofiled = Engines::new(
        core(),
        Some(Engine::open_localized(canonical("0.2.0"), &[]).expect("no profiles")),
    )
    .expect("engines");
    let identity = |version: &str| match version {
        "0.1.0" => core().spec_record().identity_digest.clone(),
        _ => localized().spec_record().identity_digest.clone(),
    };
    let core_fixture = |name: &str| {
        std::fs::read(
            canonical("0.1.0")
                .join("09_CONFORMANCE/SOURCE_FIXTURES")
                .join(name),
        )
        .expect("0.1.0 fixture")
    };
    let localized_fixture = |name: &str| fixture(name).bytes().to_vec();
    let english = String::from_utf8(localized_fixture("canonical_en.lcl")).expect("UTF-8");
    let edit = |from: &str, to: &str| {
        assert!(english.contains(from), "{from:?}");
        english.replacen(from, to, 1).into_bytes()
    };
    let quote = char::from_u32(0x201C).expect("a typographic quote");

    type Row<'a> = (
        &'a str,
        &'a Engines,
        Vec<u8>,
        &'a str,
        Option<(Stage, &'a str)>,
    );
    let rows: Vec<Row> = vec![
        (
            "valid 0.1.0",
            &profiled,
            core_fixture("valid_minimum.lcl"),
            "0.1.0",
            None,
        ),
        (
            "invalid 0.1.0",
            &profiled,
            core_fixture("invalid_tab.lcl"),
            "0.1.0",
            Some((Lexical, "error.source.tab")),
        ),
        (
            "valid 0.2.0 canonical English",
            &profiled,
            english.clone().into_bytes(),
            "0.2.0",
            None,
        ),
        (
            "valid 0.2.0 localized",
            &profiled,
            localized_fixture("auto_lv.lcl"),
            "0.2.0",
            None,
        ),
        (
            "0.2.0 explicit-locale localization failure",
            &profiled,
            localized_fixture("unavailable_explicit_locale.lcl"),
            "0.2.0",
            Some((Localization, "error.localization.profile_unavailable")),
        ),
        (
            "0.2.0 canonical English, a tab after VERSION",
            &profiled,
            edit("    TYPE: INTEGER\n", "\tTYPE: INTEGER\n"),
            "0.2.0",
            Some((Lexical, "error.source.tab")),
        ),
        (
            "0.2.0 canonical English, a typographic quote after VERSION",
            &profiled,
            edit(
                "\"Localized minimum\"",
                &format!("{quote}Localized minimum\""),
            ),
            "0.2.0",
            Some((Lexical, "error.source.non_ascii_outside_string")),
        ),
        (
            "0.2.0 canonical English, a grammar defect after VERSION",
            &profiled,
            edit("    VALUE: 3\n", "    VALUE: 3\n    VALUE: 3\n"),
            "0.2.0",
            Some((GrammarOrSchema, "error.field.duplicate")),
        ),
        (
            "canonical English whose VERSION is not readable",
            &profiled,
            edit("    VERSION: \"0.2.0\"\n", "    VERSION: \"0.2.0\n"),
            "0.1.0",
            Some((Lexical, "error.literal.unclosed")),
        ),
        (
            "0.2.0 canonical English, a tab before VERSION",
            &profiled,
            edit("    VERSION: \"0.2.0\"\n", "\tVERSION: \"0.2.0\"\n"),
            "0.1.0",
            Some((Lexical, "error.source.tab")),
        ),
        (
            "localized source with a directive whose profile is unavailable",
            &unprofiled,
            localized_fixture("explicit_lv.lcl"),
            "0.2.0",
            Some((Localization, "error.localization.profile_unavailable")),
        ),
        (
            "localized source without a directive and no profile (D9)",
            &unprofiled,
            localized_fixture("auto_lv.lcl"),
            "0.1.0",
            Some((Lexical, "error.keyword.unknown")),
        ),
    ];

    let provider = MemoryProvider::new();
    let mut wrong = Vec::new();
    for (case, engines, bytes, version, primary) in rows {
        let unit = SourceUnit::new(SourceId::new("matrix.lcl"), bytes);
        let report = engines.engine_for(&unit).check(&unit, &provider);
        let actual = (
            report.spec.formal_version.as_str(),
            report.spec.identity_digest == identity(version),
            report.primary().map(|d| (d.stage, d.id.as_str())),
        );
        if actual != (version, true, primary) {
            wrong.push(format!(
                "{case}: expected {version} {primary:?}, got {actual:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[test]
fn without_a_localized_engine_everything_is_core() {
    let engines = Engines::new(core(), None).expect("engines");
    assert_eq!(version_of(&engines, &fixture("auto_lv.lcl")), "0.1.0");
    assert!(Engines::new(localized(), None).is_err());
    assert!(Engines::new(core(), Some(core())).is_err());
}
