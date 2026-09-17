//! LCL-FEATURE-04 D3: the command line judges localized documents.
//!
//! The Core 0.2.0 package is named explicitly (`--localized-spec`), locale
//! profiles are supplied as files, and a project lock pins each localized
//! unit's locale profile identity.

mod common;

use common::{canonical_root, lcl, lcl_in, scratch, write};
use std::path::{Path, PathBuf};

fn localized_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.2.0")
        .canonicalize()
        .expect("the 0.2.0 package must be present")
}

fn fixture(relative: &str) -> Vec<u8> {
    std::fs::read(
        localized_root()
            .join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
            .join(relative),
    )
    .expect("fixture")
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

/// `version` claims Core 0.2.0 only for a package named to it, by option or by
/// environment, that opens as the approved Core 0.2.0 package.
#[test]
fn version_names_core_0_2_0_only_when_its_package_is_named() {
    let languages = |stdout: &str| -> Vec<String> {
        stdout
            .lines()
            .filter_map(|line| line.strip_prefix("language "))
            .map(str::to_string)
            .collect()
    };
    let root = text(&localized_root());

    let named = lcl(&["version", "--localized-spec", &root]);
    assert_eq!(named.code, 0, "{}\n{}", named.stdout, named.stderr);
    assert_eq!(languages(&named.stdout), ["0.1.0", "0.2.0"]);

    let from_environment = lcl_in(
        &std::env::temp_dir(),
        &["version"],
        &[("LCL_LOCALIZED_SPEC", &root)],
    );
    assert_eq!(from_environment.code, 0, "{}", from_environment.stderr);
    assert_eq!(languages(&from_environment.stdout), ["0.1.0", "0.2.0"]);

    // Any other package is refused with the environment exit code, as every
    // command refuses it, before anything is printed.
    let other = lcl(&["version", "--localized-spec", &text(&canonical_root())]);
    assert_eq!(other.code, 4, "{}", other.stdout);
    assert!(other.stdout.is_empty(), "{}", other.stdout);
}

#[test]
fn a_localized_document_is_checked_by_the_0_2_0_engine() {
    let dir = scratch("localized-cli-check");
    write(dir.join("main.lcl"), fixture("sources/auto_lv.lcl"));
    write(dir.join("lv-LV.json"), fixture("profiles/lv-LV.json"));
    let run = lcl(&[
        "check",
        "--machine",
        "--spec",
        &text(&canonical_root()),
        "--localized-spec",
        &text(&localized_root()),
        "--profile",
        &text(&dir.join("lv-LV.json")),
        &text(&dir.join("main.lcl")),
    ]);
    assert_eq!(run.code, 0, "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("0.2.0"), "{}", run.stdout);
    assert!(run.stdout.contains("\"lv-LV\""), "{}", run.stdout);
    assert!(run.stdout.contains("auto"), "{}", run.stdout);

    let without = lcl(&[
        "check",
        "--machine",
        "--spec",
        &text(&canonical_root()),
        &text(&dir.join("main.lcl")),
    ]);
    assert_ne!(without.code, 0, "{}", without.stdout);
    assert!(!without.stdout.contains("\"lv-LV\""));
}

#[test]
fn a_core_0_1_0_document_stays_core_with_both_packages() {
    let dir = scratch("localized-cli-core");
    let minimum =
        std::fs::read(canonical_root().join("09_CONFORMANCE/SOURCE_FIXTURES/valid_minimum.lcl"))
            .expect("0.1.0 fixture");
    write(dir.join("minimum.lcl"), minimum);
    let both = lcl(&[
        "check",
        "--machine",
        "--spec",
        &text(&canonical_root()),
        "--localized-spec",
        &text(&localized_root()),
        &text(&dir.join("minimum.lcl")),
    ]);
    let core = lcl(&[
        "check",
        "--machine",
        "--spec",
        &text(&canonical_root()),
        &text(&dir.join("minimum.lcl")),
    ]);
    assert_eq!(both.code, 0, "{}\n{}", both.stdout, both.stderr);
    assert_eq!(both.stdout, core.stdout);
}

#[test]
fn a_lock_pins_the_locale_profile_and_detects_its_drift() {
    let root = scratch("localized-cli-lock");
    write(root.join("main.lcl"), fixture("sources/auto_lv.lcl"));
    write(
        root.join("profiles/lv-LV.json"),
        fixture("profiles/lv-LV.json"),
    );
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"localized_spec\": {:?},\n  \"profiles\": \"profiles\",\n  \"entry\": \"main.lcl\"\n}}\n",
            text(&canonical_root()),
            text(&localized_root()),
        ),
    );
    let project = text(&root);
    let locked = lcl(&["package", "lock", "--project", &project]);
    assert_eq!(locked.code, 0, "{}\n{}", locked.stdout, locked.stderr);
    let lock = std::fs::read_to_string(root.join("lcl.lock")).expect("lock written");
    assert!(lock.starts_with("lcl-lock/2\n"), "{lock}");
    assert!(lock.contains("\nlocale lv-LV auto sha256:"), "{lock}");

    let verified = lcl(&["package", "verify", "--project", &project]);
    assert_eq!(verified.code, 0, "{}\n{}", verified.stdout, verified.stderr);
    let pinned = lcl(&["check", "--locked", "--project", &project]);
    assert_eq!(pinned.code, 0, "{}\n{}", pinned.stdout, pinned.stderr);

    // A later profile with different bytes is a different profile.
    let mut changed = fixture("profiles/lv-LV.json");
    changed.extend_from_slice(b"\n");
    write(root.join("profiles/lv-LV.json"), changed);
    let drifted = lcl(&["check", "--locked", "--project", &project]);
    assert_ne!(drifted.code, 0, "{}\n{}", drifted.stdout, drifted.stderr);
    let output = format!("{}{}", drifted.stdout, drifted.stderr);
    assert!(
        output.contains("locale profile changed") || output.contains("profile_drift"),
        "{output}"
    );
}

/// PRETEST-03 F13: an explicit `--profile` is never silently ignored. Without
/// a localized specification package it is a usage error, before anything is
/// printed; `version` opens the profiles it is given.
#[test]
fn a_profile_without_a_localized_package_is_a_usage_error() {
    let dir = scratch("localized-cli-profile-inactive");
    write(dir.join("main.lcl"), fixture("sources/auto_lv.lcl"));
    write(dir.join("lv-LV.json"), fixture("profiles/lv-LV.json"));
    let profile = text(&dir.join("lv-LV.json"));
    let spec = text(&canonical_root());
    let document = text(&dir.join("main.lcl"));
    for args in [
        vec!["check", "--spec", &spec, "--profile", &profile, &document],
        vec!["run", "--spec", &spec, "--profile", &profile, &document],
        vec!["version", "--profile", &profile],
    ] {
        let refused = lcl(&args);
        assert_eq!(refused.code, 3, "{args:?}\n{}", refused.stdout);
        assert!(refused.stdout.is_empty(), "{args:?}\n{}", refused.stdout);
        assert!(
            refused.stderr.contains("--profile"),
            "{args:?}\n{}",
            refused.stderr
        );
    }

    let missing = text(&dir.join("nl-NL.json"));
    let unreadable = lcl(&[
        "version",
        "--localized-spec",
        &text(&localized_root()),
        "--profile",
        &missing,
    ]);
    assert_eq!(unreadable.code, 4, "{}", unreadable.stdout);
    assert!(unreadable.stdout.is_empty(), "{}", unreadable.stdout);
}
