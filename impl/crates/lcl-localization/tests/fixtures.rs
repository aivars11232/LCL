//! LCL-FEATURE-04 C1: the localization stage against the Core 0.2.0 fixtures.
//!
//! The expectations are the package's own
//! `09_CONFORMANCE/LOCALIZATION_FIXTURES/expected_results.json`, which
//! `TOOLS/validate_localization.py` also satisfies. The fixture profile
//! identities are the SHA-256 values that tool computed independently.

use lcl_localization::{
    content_identity, ids, localize, validate_profile, Contract, CoverageDetector, LocaleTag,
    Localization, MemoryResolver, Method, Pin, UnavailableResolver,
};
use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::json::{self, Json};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.2.0")
        .canonicalize()
        .expect("the 0.2.0 package is present")
}

fn fixtures() -> PathBuf {
    package_root().join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
}

fn contract() -> Contract {
    let spec = SpecPackage::open_with_anchor(package_root(), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    Contract::load(&spec).expect("the localization contract loads")
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).expect("a valid tag")
}

fn profile_bytes(locale: &str) -> Vec<u8> {
    std::fs::read(fixtures().join("profiles").join(format!("{locale}.json"))).expect("profile")
}

fn expected() -> Json {
    let text = std::fs::read_to_string(fixtures().join("expected_results.json")).expect("read");
    json::parse(&text).expect("expected_results.json parses")
}

const FIXTURE_IDENTITIES: [(&str, &str); 4] = [
    (
        "lv-LV",
        "sha256:d61622cb8783dcbfe8aaa762161e0f8980983e42eafaf5dbab9eb242f5f60bf4",
    ),
    (
        "nl-NL",
        "sha256:4a0efe837370fd9e365e10a9d6f558a921ac8c28402a309f43fd98b25f8cabe0",
    ),
    (
        "ru-RU",
        "sha256:4841684746349649f23bef19e95b3fab1ff2b3567c4d43474fbaa5dcde4807b3",
    ),
    (
        "zh-CN",
        "sha256:c2ef20d06c5836b61207cc15a339992adc8dd6405f6146a773c45ff54dae8c43",
    ),
];

#[test]
fn every_fixture_profile_validates_under_its_recorded_identity() {
    let contract = contract();
    let listed = expected();
    let profiles = listed
        .get("profiles")
        .and_then(Json::as_object)
        .expect("profiles");
    assert_eq!(profiles.len(), FIXTURE_IDENTITIES.len());
    for (locale, identity) in FIXTURE_IDENTITIES {
        let relative = format!("profiles/{locale}.json");
        let wanted = profiles
            .iter()
            .find(|(path, _)| *path == relative)
            .and_then(|(_, outcome)| outcome.as_str());
        assert_eq!(wanted, Some("valid"), "{relative} is listed as valid");
        let bytes = profile_bytes(locale);
        assert_eq!(content_identity(&bytes), identity);
        let profile = validate_profile(&contract, &bytes, Some(&tag(locale)))
            .unwrap_or_else(|e| panic!("{locale} must validate: {e}"));
        assert_eq!(profile.locale().as_str(), locale);
        assert_eq!(profile.identity(), identity);
        assert_eq!(profile.mapped_reserved_words(), 60);
        assert!(!profile.is_complete());
        assert_eq!(profile.provider_class(), "fixture");
    }
}

fn outcome_view(localization: &Localization) -> Vec<(&'static str, String)> {
    let mut view = Vec::new();
    if let Some(first) = localization.diagnostics.first() {
        view.push(("error", first.id.to_string()));
        view.push(("offset", first.offset.to_string()));
    } else if let Some(word) = localization.unknown_words.iter().min_by_key(|w| w.offset) {
        view.push(("error", ids::KEYWORD_UNKNOWN.to_string()));
        view.push(("offset", word.offset.to_string()));
    }
    if let Some(record) = &localization.record {
        view.push(("method", record.method.as_str().to_string()));
        if let Some(locale) = &record.locale {
            view.push(("locale", locale.as_str().to_string()));
        }
    }
    view
}

#[test]
fn every_fixture_source_selects_and_classifies_as_expected() {
    let contract = contract();
    let listed = expected();
    let sources = listed
        .get("sources")
        .and_then(Json::as_object)
        .expect("sources");
    assert_eq!(sources.len(), 31);
    let mut failures = Vec::new();
    for (relative, case) in sources {
        let source = std::fs::read(fixtures().join(relative)).expect("source");
        let mut resolver = MemoryResolver::new("lcl.fixture.memory");
        for locale in case
            .get("available")
            .and_then(Json::as_array)
            .expect("available")
        {
            let locale = locale.as_str().expect("locale string");
            resolver.insert(tag(locale), profile_bytes(locale));
        }
        let pin = case.get("pin").and_then(|pin| {
            let locale = pin.get("locale")?.as_str()?;
            let identity = pin.get("identity")?.as_str()?;
            let identity = if identity == "@profile" {
                content_identity(&profile_bytes(locale))
            } else {
                identity.to_string()
            };
            Some(Pin {
                locale: tag(locale),
                identity,
            })
        });
        let resolver_available = case
            .get("resolver_available")
            .and_then(Json::as_bool)
            .unwrap_or(true);
        let localization = if resolver_available {
            localize(
                &contract,
                &source,
                &resolver,
                &CoverageDetector,
                pin.as_ref(),
            )
        } else {
            localize(
                &contract,
                &source,
                &UnavailableResolver,
                &CoverageDetector,
                pin.as_ref(),
            )
        };
        let view = outcome_view(&localization);
        let wanted = case
            .get("expected")
            .and_then(Json::as_object)
            .expect("expected");
        for (key, value) in wanted {
            let want = value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_u64().map(|n| n.to_string()))
                .expect("expected value");
            let got = view.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            if got.as_deref() != Some(want.as_str()) {
                failures.push(format!("{relative}: {key} expected {want}, got {got:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "fixture mismatches:\n{}",
        failures.join("\n")
    );
}

#[test]
fn equivalent_localized_documents_share_one_canonical_word_sequence() {
    let contract = contract();
    let mut resolver = MemoryResolver::new("lcl.fixture.memory");
    for (locale, _) in FIXTURE_IDENTITIES {
        resolver.insert(tag(locale), profile_bytes(locale));
    }
    let run = |name: &str| {
        let source = std::fs::read(fixtures().join("sources").join(name)).expect("source");
        localize(&contract, &source, &resolver, &CoverageDetector, None)
    };
    let canonical = run("canonical_en.lcl");
    assert!(canonical.is_accepted());
    assert_eq!(
        canonical.record.as_ref().map(|r| r.method),
        Some(Method::Canonical)
    );
    assert!(!canonical.canonical_words.is_empty());
    for (name, locale) in [
        ("auto_lv.lcl", "lv-LV"),
        ("auto_nl.lcl", "nl-NL"),
        ("auto_ru.lcl", "ru-RU"),
        ("auto_zh.lcl", "zh-CN"),
        ("explicit_lv.lcl", "lv-LV"),
        ("explicit_zh.lcl", "zh-CN"),
    ] {
        let localized = run(name);
        assert!(
            localized.is_accepted(),
            "{name}: {:?}",
            localized.diagnostics
        );
        assert!(localized.unknown_words.is_empty(), "{name}");
        let record = localized.record.as_ref().expect("record");
        assert_eq!(record.locale.as_ref().map(LocaleTag::as_str), Some(locale));
        assert_eq!(
            localized.canonical_words, canonical.canonical_words,
            "{name} normalizes to the canonical word sequence"
        );
        let identity = FIXTURE_IDENTITIES
            .iter()
            .find(|(l, _)| *l == locale)
            .map(|(_, i)| *i);
        assert_eq!(record.profile_identity.as_deref(), identity);
    }
}

/// A mutation name, the profile bytes, the expected rejection identifier (or
/// `None` for a valid profile), and the locale the caller selected.
type MutationCase = (
    &'static str,
    Vec<u8>,
    Option<&'static str>,
    Option<&'static str>,
);

fn mutate(base: &str, old: &str, new: &str) -> Vec<u8> {
    assert_eq!(base.matches(old).count(), 1, "mutation anchor {old:?}");
    base.replacen(old, new, 1).into_bytes()
}

#[test]
fn invalid_profiles_are_rejected_whole_with_registered_identifiers() {
    let contract = contract();
    let lv = String::from_utf8(profile_bytes("lv-LV")).expect("utf-8");
    let ru = String::from_utf8(profile_bytes("ru-RU")).expect("utf-8");
    let ch = |v: u32| char::from_u32(v).expect("scalar");
    let mixed_script: String = [0x0417, 0x0410, 0x0414, 0x0410, 0x0427]
        .into_iter()
        .map(ch)
        .chain(['A'])
        .collect();
    let confusable_set: String = [0x0405, 0x0415, 0x0422].into_iter().map(ch).collect();
    let combining: String = format!("UZDEVUMSA{}", ch(0x0304));
    let spell = |base: &str, spelling: &str, word: &str| {
        mutate(
            base,
            "\"spellings\": {",
            &format!("\"spellings\": {{\n    \"{spelling}\": \"{word}\","),
        )
    };
    let lv_step = {
        let doc = json::parse(&lv).expect("lv");
        doc.get("preferred")
            .and_then(|p| p.get("STEP"))
            .and_then(Json::as_str)
            .expect("STEP")
            .to_string()
    };
    let lv_task = {
        let doc = json::parse(&lv).expect("lv");
        doc.get("preferred")
            .and_then(|p| p.get("TASK"))
            .and_then(Json::as_str)
            .expect("TASK")
            .to_string()
    };
    let invalid = ids::PROFILE_INVALID;
    let cases: Vec<MutationCase> = vec![
        ("valid_lv_roundtrip", lv.clone().into_bytes(), None, None),
        (
            "incompatible_version",
            mutate(&lv, "\"lcl_version\": \"0.2.0\"", "\"lcl_version\": \"0.1.0\""),
            Some(invalid),
            None,
        ),
        (
            "malformed_tag",
            mutate(&lv, "\"locale\": \"lv-LV\"", "\"locale\": \"lv_LV\""),
            Some(ids::LOCALE_INVALID),
            None,
        ),
        (
            "unnormalized_tag",
            mutate(&lv, "\"locale\": \"lv-LV\"", "\"locale\": \"LV-lv\""),
            Some(invalid),
            None,
        ),
        (
            "duplicate_key",
            mutate(&lv, "\"format\":", "\"format\": \"x\",\n  \"format\":"),
            Some(invalid),
            None,
        ),
        ("unknown_reserved_word", spell(&lv, "DARBS", "JOB"), Some(invalid), None),
        (
            "spelling_is_other_canonical_word",
            spell(&lv, "TEST", "TASK"),
            Some(invalid),
            None,
        ),
        ("lowercase_spelling", spell(&lv, "uzdevums", "TASK"), Some(invalid), None),
        ("combining_mark", spell(&lv, &combining, "TASK"), Some(invalid), None),
        (
            "mixed_repertoire_spelling",
            spell(&ru, &mixed_script, "TASK"),
            Some(invalid),
            None,
        ),
        (
            "confusable_with_profile_spelling",
            spell(&ru, "BCE", "ALL"),
            Some(invalid),
            None,
        ),
        (
            "confusable_with_other_canonical_word",
            spell(&ru, &confusable_set, "STEP"),
            Some(invalid),
            None,
        ),
        (
            "unknown_repertoire",
            mutate(&lv, "\"repertoires\": [", "\"repertoires\": [\n    \"greek\","),
            Some(invalid),
            None,
        ),
        (
            "preferred_mismatch",
            mutate(
                &lv,
                &format!("\"TASK\": \"{lv_task}\""),
                &format!("\"TASK\": \"{lv_step}\""),
            ),
            Some(invalid),
            None,
        ),
        (
            "coverage_mismatch",
            mutate(
                &lv,
                "\"mapped_reserved_words\": 60",
                "\"mapped_reserved_words\": 1",
            ),
            Some(invalid),
            None,
        ),
        (
            "unknown_provider_class",
            mutate(&lv, "\"provider_class\": \"fixture\"", "\"provider_class\": \"vendor\""),
            Some(invalid),
            None,
        ),
        (
            "extra_field",
            mutate(&lv, "\"format\":", "\"extra\": true,\n  \"format\":"),
            Some(invalid),
            None,
        ),
        (
            "byte_order_mark",
            [b"\xEF\xBB\xBF".as_slice(), lv.as_bytes()].concat(),
            Some(invalid),
            None,
        ),
        (
            "empty_spellings",
            br#"{"coverage": {"complete": false, "mapped_reserved_words": 0}, "format": "lcl-locale-profile/1", "lcl_version": "0.2.0", "locale": "lv-LV", "preferred": {}, "provenance": {"provider_class": "fixture", "provider_identity": "x"}, "repertoires": ["latin"], "spellings": {}}"#.to_vec(),
            Some(invalid),
            None,
        ),
        (
            "display_label_unregistered",
            mutate(
                &lv,
                "\"format\":",
                "\"display_labels\": {\"status.unheard\": \"X\"},\n  \"format\":",
            ),
            Some(invalid),
            None,
        ),
        ("wrong_selected_locale", lv.clone().into_bytes(), Some(invalid), Some("nl-NL")),
    ];
    assert_eq!(cases.len(), 21);
    let mut failures = Vec::new();
    for (name, bytes, want, selected) in cases {
        let expected_locale = selected.map(tag);
        let got = validate_profile(&contract, &bytes, expected_locale.as_ref())
            .err()
            .map(|e| e.id);
        if got != want {
            failures.push(format!("{name}: expected {want:?}, got {got:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "mutation mismatches:\n{}",
        failures.join("\n")
    );
}
