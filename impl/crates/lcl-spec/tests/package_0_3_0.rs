//! LCL Core 0.3 Task 01: the Core 0.3.0 package opens under its own anchor.
//!
//! [`SpecPackage::open`] stays pinned to [`APPROVED_PACKAGE`]; the 0.3.0
//! package is selected explicitly with [`APPROVED_PACKAGE_0_3_0`], and no
//! anchor accepts another version's package. The canonical packages are only
//! read.

use lcl_spec::anchor::{APPROVED_PACKAGE_0_2_0, APPROVED_PACKAGE_0_3_0};
use lcl_spec::{SpecError, SpecPackage};
use std::path::{Path, PathBuf};

fn canonical(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("canonical package must be present")
}

const REGISTRIES: [&str; 14] = [
    "ambiguous_replacements",
    "block_schemas",
    "built_in_groups_and_results",
    "field_signatures",
    "formats_encodings_units",
    "keywords",
    "locale_profile_schema",
    "localization_surface",
    "operations",
    "operators_and_functions",
    "semantic_meta_types",
    "statuses_and_errors",
    "symbols",
    "types",
];

#[test]
fn the_0_3_0_package_opens_under_its_own_anchor() {
    let package = SpecPackage::open_with_anchor(canonical("0.3.0"), &APPROVED_PACKAGE_0_3_0)
        .expect("the approved 0.3.0 package opens");
    assert!(package.is_authoritative());
    assert_eq!(package.formal_version(), "0.3.0");
    assert_eq!(
        package.identity_digest(),
        APPROVED_PACKAGE_0_3_0.identity_digest
    );
    assert_eq!(package.registry_names().len(), 14);
    for name in REGISTRIES {
        let version = package
            .registry(name)
            .and_then(|registry| registry.get("version"))
            .and_then(|version| version.as_str());
        assert_eq!(version, Some("0.3.0"), "registry {name} is the 0.3.0 file");
    }
    for name in ["core_conformance_cases", "language_decision_cases"] {
        assert!(package.catalog(name).is_some(), "catalog {name} is loaded");
    }
}

#[test]
fn no_anchor_accepts_another_versions_package() {
    for (root, anchor, found, expected) in [
        ("0.2.0", &APPROVED_PACKAGE_0_3_0, "0.2.0", "0.3.0"),
        ("0.3.0", &APPROVED_PACKAGE_0_2_0, "0.3.0", "0.2.0"),
    ] {
        match SpecPackage::open_with_anchor(canonical(root), anchor) {
            Err(SpecError::VersionMismatch {
                found: got,
                expected: want,
            }) => {
                assert_eq!(got, found);
                assert_eq!(want, expected);
            }
            other => panic!("{root} under the {expected} anchor must be refused, got {other:?}"),
        }
    }
    match SpecPackage::open(canonical("0.3.0")) {
        Err(SpecError::VersionMismatch { found, expected }) => {
            assert_eq!(found, "0.3.0");
            assert_eq!(expected, "0.1.0");
        }
        other => panic!("the default open must refuse 0.3.0 by version, got {other:?}"),
    }
}

/// The four project errors are registered at the resolution stage, beside the
/// 86 errors Core 0.2.0 registers.
#[test]
fn the_0_3_0_registry_adds_the_four_project_errors() {
    let package = SpecPackage::open_with_anchor(canonical("0.3.0"), &APPROVED_PACKAGE_0_3_0)
        .expect("the approved 0.3.0 package opens");
    let errors = package
        .registry("statuses_and_errors")
        .and_then(|registry| registry.get("errors"))
        .and_then(|errors| errors.as_object())
        .expect("errors object");
    assert_eq!(errors.len(), 90);
    for id in [
        "error.project.part_duplicate",
        "error.project.part_kind",
        "error.project.part_missing",
        "error.project.placement",
    ] {
        let stage = errors
            .iter()
            .find(|(key, _)| key == id)
            .and_then(|(_, error)| error.get("stage"))
            .and_then(|stage| stage.as_str());
        assert_eq!(stage, Some("resolution"), "{id}");
    }
}
