//! The manifest, the cache and the lock file.
//!
//! Three pieces of product machinery, tested for the one property that makes
//! them worth having: a project that resolved once resolves to the same bytes
//! again, and any difference is reported rather than absorbed.

mod common;

use common::{example, scratch, write};
use lcl_project::{Cache, Drift, Lock, Manifest, Project};
use std::path::Path;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

#[test]
fn a_manifest_carries_the_paths_it_declares() {
    let manifest = Manifest::parse(
        r#"{
          "format": "lcl.project/1",
          "spec": "../canonical/LCL_Core_0.1.0",
          "entry": "src/main.lcl",
          "cache": ".lcl-cache",
          "lock": "lcl.lock"
        }"#,
        Path::new("lcl.project.json"),
    )
    .expect("a complete manifest parses");

    assert_eq!(
        manifest.spec.as_deref(),
        Some("../canonical/LCL_Core_0.1.0")
    );
    assert_eq!(manifest.entry.as_deref(), Some("src/main.lcl"));
    assert_eq!(manifest.cache.as_deref(), Some(".lcl-cache"));
    assert_eq!(manifest.lock.as_deref(), Some("lcl.lock"));
}

#[test]
fn a_manifest_may_declare_only_a_format() {
    let manifest = Manifest::parse(
        r#"{"format": "lcl.project/1"}"#,
        Path::new("lcl.project.json"),
    )
    .expect("a minimal manifest parses");
    assert_eq!(manifest, Manifest::default());
}

/// An unknown key refuses the manifest.
///
/// `07_VERSIONING_AND_EXTENSIONS/05` forbids an ignore-unknown mode for
/// normative content, and the same reasoning applies to the file that decides
/// which specification an engine loads: an old tool that silently ignored a new
/// key would read a new manifest wrongly and say nothing about it.
#[test]
fn an_unknown_key_is_refused() {
    let error = Manifest::parse(
        r#"{"format": "lcl.project/1", "plugins": ["anything"]}"#,
        Path::new("lcl.project.json"),
    )
    .expect_err("an unknown key refuses");
    assert!(format!("{error}").contains("unknown manifest key"));
}

#[test]
fn an_unknown_format_is_refused() {
    let error = Manifest::parse(
        r#"{"format": "lcl.project/99"}"#,
        Path::new("lcl.project.json"),
    )
    .expect_err("an unknown format refuses");
    assert!(format!("{error}").contains("unknown manifest format"));
}

#[test]
fn a_manifest_without_a_format_is_refused() {
    assert!(Manifest::parse(r#"{"entry": "main.lcl"}"#, Path::new("m.json")).is_err());
}

#[test]
fn a_wrongly_typed_field_is_refused() {
    let error = Manifest::parse(
        r#"{"format": "lcl.project/1", "entry": 7}"#,
        Path::new("m.json"),
    )
    .expect_err("a number is not a path");
    assert!(format!("{error}").contains("must be a string"));
}

/// The strict reader's rules are the manifest's rules.
#[test]
fn the_manifest_is_read_by_the_strict_reader() {
    for text in [
        r#"{"format": "lcl.project/1",}"#,            // trailing comma
        "{\"format\": \"lcl.project/1\"} // comment", // comment
        r#"{"format": "lcl.project/1", "format": "lcl.project/1"}"#, // duplicate key
    ] {
        assert!(
            Manifest::parse(text, Path::new("m.json")).is_err(),
            "refused: {text}"
        );
    }
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

#[test]
fn a_cached_source_is_returned_by_uri() {
    let dir = scratch("cache_roundtrip");
    let mut cache = Cache::open(&dir).expect("an empty cache opens");
    assert!(cache.is_empty());

    let bytes = example("02_IMPORT_LIBRARY.lcl");
    let digest = cache
        .put("https://example.invalid/lib.lcl", bytes.as_bytes())
        .expect("the source is cached");
    assert_eq!(digest, lcl_spec::sha256::hex_digest(bytes.as_bytes()));

    let reopened = Cache::open(&dir).expect("the cache reopens");
    let got = reopened
        .get("https://example.invalid/lib.lcl")
        .expect("the cached source is returned");
    assert_eq!(got, bytes.as_bytes());
}

#[test]
fn an_uncached_uri_is_not_available() {
    let dir = scratch("cache_absent");
    let cache = Cache::open(&dir).expect("opens");
    let error = cache
        .get("https://example.invalid/missing.lcl")
        .expect_err("an uncached URI has no bytes");
    assert!(error.message().contains("not in the package cache"));
}

/// A blob whose bytes changed is refused, not returned.
///
/// The cache is content-addressed, so its own integrity is checkable without
/// consulting anything else. The importing document's `CHECKSUM` is a separate
/// gate that still runs afterwards; neither stands in for the other.
#[test]
fn a_corrupted_blob_is_refused() {
    let dir = scratch("cache_corrupt");
    let mut cache = Cache::open(&dir).expect("opens");
    let digest = cache
        .put("https://example.invalid/lib.lcl", b"original")
        .expect("cached");

    write(dir.join("sha256").join(&digest), b"tampered");

    let reopened = Cache::open(&dir).expect("reopens");
    let error = reopened
        .get("https://example.invalid/lib.lcl")
        .expect_err("a corrupt blob is refused");
    assert!(error.message().contains("corrupt"), "{}", error.message());
    assert_eq!(reopened.verify().len(), 1, "verify reports it too");
}

#[test]
fn a_malformed_index_is_refused_rather_than_treated_as_empty() {
    let dir = scratch("cache_malformed");
    write(dir.join("index"), "lcl-cache/1\nnot-a-digest  uri\n");
    assert!(Cache::open(&dir).is_err());

    let other = scratch("cache_unknown_format");
    write(other.join("index"), "lcl-cache/99\n");
    assert!(Cache::open(&other).is_err());
}

#[test]
fn the_cache_index_is_sorted_and_stable() {
    let dir = scratch("cache_sorted");
    let mut cache = Cache::open(&dir).expect("opens");
    cache.put("https://example.invalid/z.lcl", b"z").unwrap();
    cache.put("https://example.invalid/a.lcl", b"a").unwrap();
    cache.put("https://example.invalid/m.lcl", b"m").unwrap();

    let index = std::fs::read_to_string(dir.join("index")).expect("readable");
    let uris: Vec<&str> = index
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once("  "))
        .map(|(_, uri)| uri)
        .collect();
    let mut sorted = uris.clone();
    sorted.sort();
    assert_eq!(uris, sorted);
}

/// Storing identical bytes twice changes nothing.
#[test]
fn caching_the_same_bytes_twice_is_idempotent() {
    let dir = scratch("cache_idempotent");
    let mut cache = Cache::open(&dir).expect("opens");
    cache
        .put("https://example.invalid/lib.lcl", b"same")
        .unwrap();
    let first = std::fs::read_to_string(dir.join("index")).unwrap();
    cache
        .put("https://example.invalid/lib.lcl", b"same")
        .unwrap();
    let second = std::fs::read_to_string(dir.join("index")).unwrap();
    assert_eq!(first, second);
}

// ---------------------------------------------------------------------------
// Lock
// ---------------------------------------------------------------------------

fn lock_of(units: &[(&str, &str)]) -> Lock {
    Lock::new(
        "a".repeat(64),
        "0.1.0",
        "main.lcl",
        units
            .iter()
            .map(|(id, digest)| (id.to_string(), digest.to_string())),
    )
}

#[test]
fn a_lock_file_round_trips() {
    let lock = lock_of(&[("main.lcl", &"1".repeat(64)), ("lib.lcl", &"2".repeat(64))]);
    let rendered = lock.render();
    let parsed = Lock::parse(&rendered).expect("the lock file parses");
    assert_eq!(parsed, lock);
    assert_eq!(parsed.render(), rendered, "rendering is stable");
}

#[test]
fn a_lock_file_lists_units_in_identity_order() {
    let lock = lock_of(&[
        ("z.lcl", &"1".repeat(64)),
        ("a.lcl", &"2".repeat(64)),
        ("m.lcl", &"3".repeat(64)),
    ]);
    let rendered = lock.render();
    let names: Vec<&str> = rendered
        .lines()
        .filter_map(|l| l.strip_prefix("unit "))
        .filter_map(|l| l.split_once("  "))
        .map(|(_, name)| name)
        .collect();
    assert_eq!(names, ["a.lcl", "m.lcl", "z.lcl"]);
}

#[test]
fn an_unchanged_project_has_no_drift() {
    let lock = lock_of(&[("main.lcl", &"1".repeat(64))]);
    assert!(lock.drift(&lock.clone()).is_empty());
}

#[test]
fn every_kind_of_drift_is_reported() {
    let locked = lock_of(&[("main.lcl", &"1".repeat(64)), ("gone.lcl", &"2".repeat(64))]);

    let mut actual = lock_of(&[("main.lcl", &"9".repeat(64)), ("new.lcl", &"3".repeat(64))]);
    actual.spec_identity = "b".repeat(64);
    actual.root = "other.lcl".to_string();

    let drift = locked.drift(&actual);
    assert!(drift.iter().any(|d| matches!(d, Drift::Spec { .. })));
    assert!(drift.iter().any(|d| matches!(d, Drift::Root { .. })));
    assert!(drift
        .iter()
        .any(|d| matches!(d, Drift::Changed { unit, .. } if unit == "main.lcl")));
    assert!(drift
        .iter()
        .any(|d| matches!(d, Drift::Missing { unit } if unit == "gone.lcl")));
    assert!(drift
        .iter()
        .any(|d| matches!(d, Drift::Added { unit, .. } if unit == "new.lcl")));
}

#[test]
fn a_malformed_lock_file_is_refused() {
    for text in [
        "lcl-lock/99\n",
        "lcl-lock/1\nspec-version 0.1.0\n",
        "lcl-lock/1\nspec-version 0.1.0\nspec-identity x\nroot m\nunit short  m\n",
        "lcl-lock/1\nspec-version 0.1.0\nspec-identity x\nroot m\nnonsense y\n",
        "",
    ] {
        assert!(Lock::parse(text).is_err(), "refused: {text:?}");
    }
}

#[test]
fn a_lock_file_written_to_disk_reads_back() {
    let dir = scratch("lock_disk");
    write(dir.join("lcl.project.json"), common::manifest_with(""));
    let project = Project::open(&dir).expect("opens");

    let lock = lock_of(&[("main.lcl", &"1".repeat(64))]);
    lock.write(project.lock_path()).expect("written");
    let back = Lock::read(project.lock_path()).expect("read back");
    assert_eq!(back, lock);
}
