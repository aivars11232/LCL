//! M0.1: external trust anchor tests.
//!
//! The load-bearing test here is [`self_consistent_forgery_is_rejected`]. It
//! builds a package that is altered *and* internally perfect — manifest and
//! checksum records regenerated to match the alteration — and proves that
//! internal verification accepts it while the external anchor does not.
//!
//! All mutation happens on throwaway copies under `target/test-tmp/`. The
//! canonical release is never written to.

use lcl_spec::{Authority, SpecError, SpecPackage, TrustAnchor, APPROVED_PACKAGE};
use std::path::{Path, PathBuf};

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

fn hash_file(p: &Path) -> String {
    lcl_spec::sha256::hex_digest(&std::fs::read(p).unwrap())
}

/// Rewrite a package's own integrity records so they describe the package as it
/// now is. This is exactly what an attacker (or a careless "fix the checksums"
/// script) would do.
fn regenerate_internal_metadata(pkg: &Path, changed_rel: &str, old_hash: &str) {
    let new_hash = hash_file(&pkg.join(changed_rel));
    assert_ne!(new_hash, old_hash, "victim file must actually have changed");

    // 1. Manifest record for the altered file.
    let manifest_path = pkg.join("MANIFEST.json");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    assert_eq!(
        manifest.matches(old_hash).count(),
        1,
        "old hash should appear once in manifest"
    );
    let manifest = manifest.replace(old_hash, &new_hash);
    std::fs::write(&manifest_path, &manifest).unwrap();

    // 2. Checksum record for the altered file.
    let sums_path = pkg.join("SHA256SUMS.txt");
    let sums = std::fs::read_to_string(&sums_path).unwrap();
    let sums = sums.replace(old_hash, &new_hash);

    // 3. Checksum record for MANIFEST.json, which step 1 just changed.
    let old_manifest_line = sums
        .lines()
        .find(|l| l.ends_with("  MANIFEST.json"))
        .expect("manifest is covered by checksums")
        .to_string();
    let new_manifest_hash = lcl_spec::sha256::hex_digest(manifest.as_bytes());
    let sums = sums.replace(
        &old_manifest_line,
        &format!("{new_manifest_hash}  MANIFEST.json"),
    );
    std::fs::write(&sums_path, sums).unwrap();

    // A real forger would also rewrite VALIDATION_REPORT.txt, which records the
    // manifest hash. Deliberately skipped: it makes no difference. Any chain of
    // records living inside the package can be regenerated, which is the entire
    // reason an external anchor is required.
}

// ---------------------------------------------------------------------------

#[test]
fn approved_package_matches_the_anchor() {
    let pkg = SpecPackage::open(canonical_root()).expect("approved package must load");
    assert_eq!(pkg.identity_digest(), APPROVED_PACKAGE.identity_digest);
    assert_eq!(pkg.authority(), Authority::Authoritative);
    assert!(pkg.is_authoritative());
    assert_eq!(APPROVED_PACKAGE.package_file_count, 176);
    assert_eq!(APPROVED_PACKAGE.formal_version, "0.1.0");
}

/// Identity digest is stable across repeated computation and equals what the
/// loader reports.
#[test]
fn identity_digest_is_reproducible() {
    let (digest, count) = lcl_spec::compute_identity_digest(canonical_root()).unwrap();
    assert_eq!(count, 176);
    assert_eq!(digest, APPROVED_PACKAGE.identity_digest);
    let (again, _) = lcl_spec::compute_identity_digest(canonical_root()).unwrap();
    assert_eq!(again, digest);
}

/// **The M0.1 proof.**
///
/// An altered package whose manifest and checksums were regenerated to match is
/// internally flawless. Internal verification accepts it. The anchor does not.
#[test]
fn self_consistent_forgery_is_rejected() {
    let dir = scratch("forgery");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    let victim_rel = "04_GRAMMAR/10_COMPLETE_EBNF.ebnf";
    let victim = pkg_dir.join(victim_rel);
    let old_hash = hash_file(&victim);

    // A real semantic alteration to the normative grammar, same byte length so
    // that only the hash changes.
    let text = std::fs::read_to_string(&victim).unwrap();
    assert!(
        text.contains("\"ELSE\""),
        "fixture assumption: grammar mentions ELSE"
    );
    let altered = text.replacen("\"ELSE\"", "\"ELIF\"", 1);
    assert_eq!(
        altered.len(),
        text.len(),
        "replacement must preserve length"
    );
    std::fs::write(&victim, &altered).unwrap();

    regenerate_internal_metadata(&pkg_dir, victim_rel, &old_hash);

    // Step 1: the forgery is internally perfect.
    let unverified = SpecPackage::open_unverified(&pkg_dir).expect("forgery loads");
    assert!(
        unverified.integrity().is_verified(),
        "forgery should be internally consistent, defects: {:#?}",
        unverified.integrity().defects
    );
    assert_eq!(
        unverified.integrity().manifest_verified,
        unverified.integrity().manifest_records
    );
    assert_eq!(
        unverified.integrity().checksum_verified,
        unverified.integrity().checksum_records
    );

    // Step 2: and it is still not authoritative.
    assert_eq!(unverified.authority(), Authority::Unverified);
    assert!(!unverified.is_authoritative());

    // Step 3: the anchor rejects it, and says why.
    match SpecPackage::open(&pkg_dir) {
        Err(SpecError::TrustAnchorMismatch {
            expected,
            actual,
            internally_consistent,
            ..
        }) => {
            assert_eq!(expected, APPROVED_PACKAGE.identity_digest);
            assert_ne!(actual, APPROVED_PACKAGE.identity_digest);
            assert!(internally_consistent, "the point: it WAS self-consistent");
            let msg = SpecError::TrustAnchorMismatch {
                label: APPROVED_PACKAGE.label,
                expected,
                actual,
                internally_consistent,
            }
            .to_string();
            assert!(msg.contains("TRUST ANCHOR MISMATCH"));
            assert!(msg.contains("internal verification alone would have accepted"));
        }
        other => panic!("forgery was not rejected by the anchor: {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// The same forgery, applied to a machine-readable registry rather than prose.
#[test]
fn self_consistent_registry_forgery_is_rejected() {
    let dir = scratch("forgery-registry");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    let victim_rel = "10_REGISTRIES/statuses_and_errors_v0.1.0.json";
    let victim = pkg_dir.join(victim_rel);
    let old_hash = hash_file(&victim);

    // Flip a normative classification: make a nonrecoverable error recoverable.
    let text = std::fs::read_to_string(&victim).unwrap();
    let altered = text.replacen(
        "\"recoverable_with_declared_handler\": false",
        "\"recoverable_with_declared_handler\": true ",
        1,
    );
    assert_ne!(altered, text, "fixture assumption: a false flag exists");
    assert_eq!(altered.len(), text.len());
    std::fs::write(&victim, &altered).unwrap();

    regenerate_internal_metadata(&pkg_dir, victim_rel, &old_hash);

    let unverified = SpecPackage::open_unverified(&pkg_dir).unwrap();
    assert!(
        unverified.integrity().is_verified(),
        "forgery is internally consistent"
    );

    assert!(
        matches!(
            SpecPackage::open(&pkg_dir),
            Err(SpecError::TrustAnchorMismatch { .. })
        ),
        "registry forgery must be rejected"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// The integrity summary must not use the word "VERIFIED", which would imply
/// an authority the internal check cannot establish on its own.
#[test]
fn integrity_summary_does_not_overclaim() {
    let pkg = SpecPackage::open_unverified(canonical_root()).unwrap();
    let summary = pkg.integrity().summary();
    assert!(
        summary.starts_with("INTERNALLY CONSISTENT"),
        "got: {summary}"
    );
    assert!(
        !summary.contains("VERIFIED"),
        "internal check must not claim verification"
    );
}

/// Unverified loading stays usable for diagnosis but is never authoritative.
#[test]
fn unverified_loading_is_clearly_non_authoritative() {
    // Even on the genuine package.
    let pkg = SpecPackage::open_unverified(canonical_root()).unwrap();
    assert_eq!(pkg.authority(), Authority::Unverified);
    assert!(!pkg.is_authoritative());
    assert!(
        pkg.integrity().is_verified(),
        "genuine package is internally consistent"
    );
    assert_eq!(pkg.identity_digest(), APPROVED_PACKAGE.identity_digest);

    assert_eq!(
        Authority::Unverified.to_string(),
        "UNVERIFIED (non-authoritative)"
    );
    assert_eq!(Authority::Authoritative.to_string(), "AUTHORITATIVE");
    assert!(!Authority::Unverified.is_authoritative());
}

/// Internal verification is preserved, not replaced: a package whose bytes do
/// not match its own records still fails before the anchor is consulted.
#[test]
fn internal_verification_still_runs_first() {
    let dir = scratch("internal-first");
    let pkg_dir = dir.join("LCL_Core_0.1.0");
    copy_tree(&canonical_root(), &pkg_dir);

    // Alter a file WITHOUT regenerating metadata.
    let victim = pkg_dir.join("04_GRAMMAR/10_COMPLETE_EBNF.ebnf");
    let text = std::fs::read_to_string(&victim).unwrap();
    std::fs::write(&victim, text.replacen("\"ELSE\"", "\"ELIF\"", 1)).unwrap();

    match SpecPackage::open(&pkg_dir) {
        Err(SpecError::IntegrityFailed(report)) => {
            assert!(!report.defects.is_empty());
        }
        other => panic!("expected internal integrity failure, got {other:?}"),
    }

    std::fs::remove_dir_all(&dir).ok();
}

static WRONG_DIGEST_ANCHOR: TrustAnchor = TrustAnchor {
    label: "test anchor with wrong digest",
    formal_version: "0.1.0",
    package_file_count: 176,
    identity_digest: "1111111111111111111111111111111111111111111111111111111111111111",
};

static WRONG_COUNT_ANCHOR: TrustAnchor = TrustAnchor {
    label: "test anchor with wrong file count",
    formal_version: "0.1.0",
    package_file_count: 175,
    identity_digest: APPROVED_PACKAGE.identity_digest,
};

static WRONG_VERSION_ANCHOR: TrustAnchor = TrustAnchor {
    label: "test anchor for a future version",
    formal_version: "0.2.0",
    package_file_count: 176,
    identity_digest: APPROVED_PACKAGE.identity_digest,
};

/// The genuine package is rejected by an anchor that does not describe it, so
/// the anchor is genuinely load-bearing rather than decorative.
#[test]
fn anchor_is_load_bearing_in_all_three_fields() {
    let root = canonical_root();

    assert!(matches!(
        SpecPackage::open_with_anchor(&root, &WRONG_DIGEST_ANCHOR),
        Err(SpecError::TrustAnchorMismatch { .. })
    ));
    assert!(matches!(
        SpecPackage::open_with_anchor(&root, &WRONG_COUNT_ANCHOR),
        Err(SpecError::TrustAnchorFileCount {
            expected: 175,
            actual: 176,
            ..
        })
    ));
    assert!(matches!(
        SpecPackage::open_with_anchor(&root, &WRONG_VERSION_ANCHOR),
        Err(SpecError::VersionMismatch { .. })
    ));

    // And the real anchor still accepts it.
    assert!(SpecPackage::open_with_anchor(&root, &APPROVED_PACKAGE).is_ok());
}
