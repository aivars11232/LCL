//! Integration tests against the real canonical LCL Core 0.1.0 release.
//!
//! These tests are read-only with respect to `canonical/`. The tamper-detection
//! test operates on a throwaway copy under this crate's `target/` directory and
//! never touches the release.

use lcl_spec::{Defect, SpecError, SpecPackage};
use std::path::{Path, PathBuf};

/// Package-root-relative counts the release declares for itself.
const EXPECTED_FILES: usize = 176;
const EXPECTED_MANIFEST_RECORDS: usize = 173;
const EXPECTED_CHECKSUM_RECORDS: usize = 175;

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(name);
    if p.exists() {
        std::fs::remove_dir_all(&p).expect("clear scratch");
    }
    std::fs::create_dir_all(&p).expect("create scratch");
    p
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

#[test]
fn canonical_package_verifies() {
    let pkg = SpecPackage::open(canonical_root()).expect("canonical package must verify");
    let r = pkg.integrity();

    assert!(r.is_verified(), "unexpected defects: {:#?}", r.defects);
    assert_eq!(r.files_on_disk, EXPECTED_FILES);
    assert_eq!(r.manifest_records, EXPECTED_MANIFEST_RECORDS);
    assert_eq!(r.checksum_records, EXPECTED_CHECKSUM_RECORDS);
    assert_eq!(r.manifest_verified, EXPECTED_MANIFEST_RECORDS);
    assert_eq!(r.checksum_verified, EXPECTED_CHECKSUM_RECORDS);
    assert_eq!(r.formal_version, "0.1.0");
    assert_eq!(r.status, "bare_language_release");
    assert!(r.release_ready);
}

#[test]
fn all_twelve_registries_and_two_catalogs_load() {
    let pkg = SpecPackage::open(canonical_root()).unwrap();
    assert_eq!(pkg.registry_names().len(), 12);
    for (name, _) in lcl_spec::REGISTRY_FILES {
        assert!(pkg.registry(name).is_some(), "registry {name} missing");
    }
    for (name, _) in lcl_spec::CATALOG_FILES {
        assert!(pkg.catalog(name).is_some(), "catalog {name} missing");
    }
}

/// The central M0 claim: what the loader reads matches what the release says
/// it contains. Every checkable subject in `component_counts` must agree.
#[test]
fn declared_component_counts_match_observed() {
    let pkg = SpecPackage::open(canonical_root()).unwrap();
    let defects = pkg.component_count_defects();
    assert!(
        defects.is_empty(),
        "component count disagreement: {defects:#?}"
    );

    let declared = pkg.declared_component_counts();
    let observed = pkg.observed_component_counts();

    // Every subject the manifest declares must be independently checkable.
    let unchecked: Vec<&String> = declared
        .keys()
        .filter(|k| !observed.contains_key(*k))
        .collect();
    assert_eq!(
        unchecked,
        vec![&"concrete_source_fixtures".to_string()],
        "only the source-fixture count is not a registry cardinality"
    );

    // Spot-check the load-bearing ones against the pinned release.
    assert_eq!(observed["keywords"], 141);
    assert_eq!(observed["errors"], 77);
    assert_eq!(observed["statuses"], 12);
    assert_eq!(observed["operations"], 39);
    assert_eq!(observed["types"], 21);
    assert_eq!(observed["blocks"], 41);
    assert_eq!(observed["field_signatures"], 334);
    assert_eq!(observed["conformance_requirements"], 799);
}

#[test]
fn version_pin_is_enforced() {
    assert_eq!(lcl_spec::PINNED_FORMAL_VERSION, "0.1.0");
    let pkg = SpecPackage::open(canonical_root()).unwrap();
    assert_eq!(pkg.formal_version(), lcl_spec::PINNED_FORMAL_VERSION);
}

/// Fail-closed proof: a single flipped byte in a payload file must be detected
/// by both the manifest and the checksum record, and must prevent `open`.
#[test]
fn detects_single_byte_tamper() {
    let dir = scratch("tamper");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    // Sanity: the untouched copy verifies.
    SpecPackage::open(&pkg_dir).expect("pristine copy must verify");

    // Flip one byte in a normative registry.
    let victim = pkg_dir.join("10_REGISTRIES/keywords_v0.1.0.json");
    let mut bytes = std::fs::read(&victim).unwrap();
    let pos = bytes.len() / 2;
    bytes[pos] ^= 0x20;
    std::fs::write(&victim, &bytes).unwrap();

    match SpecPackage::open(&pkg_dir) {
        Err(SpecError::IntegrityFailed(report)) => {
            let hash_defects: Vec<_> = report
                .defects
                .iter()
                .filter(|d| matches!(d, Defect::HashMismatch { .. }))
                .collect();
            assert_eq!(
                hash_defects.len(),
                2,
                "expected manifest and checksum mismatch, got {:#?}",
                report.defects
            );
        }
        Err(SpecError::Json { .. }) => {
            // Also acceptable: the flip made the JSON unparsable, which is
            // itself fail-closed behaviour.
        }
        other => panic!("tamper not detected, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// An added file that no integrity record covers must be reported.
#[test]
fn detects_unrecorded_file() {
    let dir = scratch("extra");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    std::fs::write(pkg_dir.join("10_REGISTRIES/injected.json"), b"{}\n").unwrap();

    match SpecPackage::open(&pkg_dir) {
        Err(SpecError::IntegrityFailed(report)) => {
            assert!(
                report.defects.iter().any(|d| matches!(
                    d,
                    Defect::UnrecordedFile { path } if path == "10_REGISTRIES/injected.json"
                )),
                "unrecorded file not reported: {:#?}",
                report.defects
            );
        }
        other => panic!("extra file not detected, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// A removed payload file must be reported as missing by both records.
#[test]
fn detects_missing_file() {
    let dir = scratch("missing");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    std::fs::remove_file(pkg_dir.join("10_REGISTRIES/symbols_v0.1.0.json")).unwrap();

    match SpecPackage::open(&pkg_dir) {
        Err(SpecError::Io { .. }) => { /* refused at registry load: fail-closed */ }
        Err(SpecError::IntegrityFailed(report)) => {
            assert!(report
                .defects
                .iter()
                .any(|d| matches!(d, Defect::MissingFile { .. })));
        }
        other => panic!("missing file not detected, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// The loader must never mutate the package it reads.
#[test]
fn loading_does_not_modify_the_package() {
    let dir = scratch("readonly");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    fn digest_tree(root: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let e = e.unwrap();
                let p = e.path();
                if e.file_type().unwrap().is_dir() {
                    stack.push(p);
                } else {
                    let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                    let h = lcl_spec::sha256::hex_digest(&std::fs::read(&p).unwrap());
                    out.push((rel, h));
                }
            }
        }
        out.sort();
        out
    }

    let before = digest_tree(&pkg_dir);
    let pkg = SpecPackage::open(&pkg_dir).unwrap();
    let _ = pkg.observed_component_counts();
    let _ = pkg.registry("keywords");
    let after = digest_tree(&pkg_dir);

    assert_eq!(before, after, "loader modified the package");
    std::fs::remove_dir_all(&dir).ok();
}
