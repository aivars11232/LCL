//! LCL-FEATURE-04 C0: the Core 0.2.0 package opens under its own external anchor.
//!
//! Core 0.1.0 stays the default. [`SpecPackage::open`] remains pinned to
//! [`APPROVED_PACKAGE`]; the 0.2.0 package is selected explicitly with
//! [`APPROVED_PACKAGE_0_2_0`], and neither anchor accepts the other package.
//! Both canonical packages are only read.

use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::{SpecError, SpecPackage, APPROVED_PACKAGE};
use std::path::{Path, PathBuf};

fn canonical(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("canonical package must be present")
}

const BASE_REGISTRIES: [&str; 12] = [
    "ambiguous_replacements",
    "block_schemas",
    "built_in_groups_and_results",
    "field_signatures",
    "formats_encodings_units",
    "keywords",
    "operations",
    "operators_and_functions",
    "semantic_meta_types",
    "statuses_and_errors",
    "symbols",
    "types",
];

#[test]
fn the_0_2_0_package_opens_under_its_own_anchor() {
    let package = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    assert!(package.is_authoritative());
    assert_eq!(package.formal_version(), "0.2.0");
    assert_eq!(
        package.identity_digest(),
        APPROVED_PACKAGE_0_2_0.identity_digest
    );
    for name in BASE_REGISTRIES
        .iter()
        .chain(["localization_surface", "locale_profile_schema"].iter())
    {
        assert!(
            package.registry(name).is_some(),
            "registry {name} is loaded"
        );
    }
    assert_eq!(package.registry_names().len(), 14);
    for name in ["core_conformance_cases", "language_decision_cases"] {
        assert!(package.catalog(name).is_some(), "catalog {name} is loaded");
    }
}

#[test]
fn the_default_open_stays_pinned_to_0_1_0() {
    let core = SpecPackage::open(canonical("0.1.0")).expect("the approved 0.1.0 package opens");
    assert_eq!(core.formal_version(), "0.1.0");
    assert_eq!(core.identity_digest(), APPROVED_PACKAGE.identity_digest);
    assert_eq!(core.registry_names().len(), 12);

    match SpecPackage::open(canonical("0.2.0")) {
        Err(SpecError::VersionMismatch { found, expected }) => {
            assert_eq!(found, "0.2.0");
            assert_eq!(expected, "0.1.0");
        }
        other => panic!("the default open must refuse 0.2.0 by version, got {other:?}"),
    }
}

#[test]
fn the_0_2_0_anchor_refuses_the_0_1_0_package() {
    match SpecPackage::open_with_anchor(canonical("0.1.0"), &APPROVED_PACKAGE_0_2_0) {
        Err(SpecError::VersionMismatch { found, expected }) => {
            assert_eq!(found, "0.1.0");
            assert_eq!(expected, "0.2.0");
        }
        other => panic!("the 0.2.0 anchor must refuse 0.1.0 by version, got {other:?}"),
    }
}

#[test]
fn the_0_2_0_registries_are_the_0_2_0_files() {
    let package = SpecPackage::open_with_anchor(canonical("0.2.0"), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    let errors = package
        .registry("statuses_and_errors")
        .and_then(|registry| registry.get("errors"))
        .and_then(|errors| errors.as_object())
        .expect("errors object");
    assert_eq!(errors.len(), 86);
    for name in BASE_REGISTRIES {
        let version = package
            .registry(name)
            .and_then(|registry| registry.get("version"))
            .and_then(|version| version.as_str());
        assert_eq!(version, Some("0.2.0"), "registry {name} version");
    }
}
