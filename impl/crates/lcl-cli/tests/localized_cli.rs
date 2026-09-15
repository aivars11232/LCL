//! LCL-FEATURE-04 D3: the command line judges localized documents.
//!
//! The Core 0.2.0 package is named explicitly (`--localized-spec`), locale
//! profiles are supplied as files, and a project lock pins each localized
//! unit's locale profile identity.

mod common;

use common::{canonical_root, lcl, scratch, write};
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
