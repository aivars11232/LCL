//! LCL Core 0.3 Task 01: Core 0.3.0 keeps Core 0.2.0's localization.
//!
//! The Core 0.3.0 package carries the same localization contract, derived
//! over its own reserved words, and its fixture profiles name `lcl_version`
//! 0.3.0. Each contract accepts only its own version's profiles, and every
//! selection records the language version of the contract it was made under.

use lcl_localization::{
    ids, localize, validate_profile, Contract, CoverageDetector, LocaleTag, MemoryResolver, Method,
};
use lcl_spec::anchor::{APPROVED_PACKAGE_0_2_0, APPROVED_PACKAGE_0_3_0};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

const LOCALES: [&str; 4] = ["lv-LV", "nl-NL", "ru-RU", "zh-CN"];

fn package_root(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("the package is present")
}

fn fixtures(version: &str) -> PathBuf {
    package_root(version).join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
}

fn contract(version: &str) -> Contract {
    let anchor = match version {
        "0.2.0" => &APPROVED_PACKAGE_0_2_0,
        "0.3.0" => &APPROVED_PACKAGE_0_3_0,
        other => panic!("no anchor for {other}"),
    };
    let spec = SpecPackage::open_with_anchor(package_root(version), anchor)
        .expect("the approved package opens");
    Contract::load(&spec).expect("the localization contract loads")
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).expect("a valid tag")
}

fn profile_bytes(version: &str, locale: &str) -> Vec<u8> {
    std::fs::read(fixtures(version).join(format!("profiles/{locale}.json"))).expect("profile")
}

fn source(version: &str, name: &str) -> Vec<u8> {
    std::fs::read(fixtures(version).join("sources").join(name)).expect("source")
}

#[test]
fn each_package_loads_the_contract_of_its_own_version() {
    for version in ["0.2.0", "0.3.0"] {
        assert_eq!(contract(version).language_version(), version);
    }
}

#[test]
fn each_contract_accepts_exactly_its_own_versions_profiles() {
    for locale in LOCALES {
        for (contract_version, profile_version) in [
            ("0.2.0", "0.2.0"),
            ("0.3.0", "0.3.0"),
            ("0.2.0", "0.3.0"),
            ("0.3.0", "0.2.0"),
        ] {
            let result = validate_profile(
                &contract(contract_version),
                &profile_bytes(profile_version, locale),
                Some(&tag(locale)),
            );
            let context = format!("{profile_version} {locale} under {contract_version}");
            if contract_version == profile_version {
                assert!(result.is_ok(), "{context}: {result:?}");
            } else {
                let error = result.expect_err(&context);
                assert_eq!(error.id, ids::PROFILE_INVALID, "{context}");
            }
        }
    }
}

/// A canonical, an explicit and an automatically selected unit each record
/// the language version of the contract that selected it.
#[test]
fn every_selection_records_its_contracts_language_version() {
    for version in ["0.2.0", "0.3.0"] {
        let contract = contract(version);
        let mut resolver = MemoryResolver::new("lcl.fixture.memory");
        for locale in LOCALES {
            resolver.insert(tag(locale), profile_bytes(version, locale));
        }
        for (name, method) in [
            ("canonical_en.lcl", Method::Canonical),
            ("explicit_lv.lcl", Method::Explicit),
            ("auto_lv.lcl", Method::Auto),
        ] {
            let localization = localize(
                &contract,
                &source(version, name),
                &resolver,
                &CoverageDetector,
                None,
            );
            assert!(
                localization.is_accepted(),
                "{version} {name}: {:?}",
                localization.diagnostics
            );
            let record = localization.record.expect("a selection record");
            assert_eq!(record.method, method, "{version} {name}");
            assert_eq!(record.lcl_version, version, "{version} {name}");
        }
    }
}
