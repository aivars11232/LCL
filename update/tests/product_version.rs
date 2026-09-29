//! One product version for everything one release carries: the `lcl` and
//! `lcl-workspace` binaries, `lcl-remote`, this updater, and the Android app's
//! default `versionName`. The release builder refuses a mismatch; this test
//! finds one before a release is attempted.

use std::path::Path;

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The first `version = "..."` line of a Cargo manifest.
fn cargo_version(relative: &str) -> String {
    read(relative)
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .unwrap_or_else(|| panic!("{relative} declares no version"))
        .to_string()
}

/// The quoted default after `?:` on the Gradle line that reads `property`.
fn gradle_default(property: &str) -> String {
    let script = read("android/app/build.gradle.kts");
    let line = script
        .lines()
        .find(|l| l.contains(&format!("gradleProperty(\"{property}\")")))
        .unwrap_or_else(|| panic!("build.gradle.kts does not read {property}"));
    let default = line.rsplit("?:").next().unwrap().trim();
    default.trim_matches('"').to_string()
}

#[test]
fn every_part_of_a_release_carries_one_product_version() {
    let product = lcl_update::PRODUCT_VERSION;
    assert_eq!(
        cargo_version("impl/Cargo.toml"),
        product,
        "lcl, lcl-workspace"
    );
    assert_eq!(cargo_version("remote/Cargo.toml"), product, "lcl-remote");
    assert_eq!(cargo_version("update/Cargo.toml"), product, "lcl-update");
    assert_eq!(
        gradle_default("lclVersionName"),
        product,
        "Android versionName"
    );
    // Strict MAJOR.MINOR.PATCH, which every published updater parses.
    let parsed = lcl_update::version::Version::parse(product).expect("a product version");
    assert!(!parsed.is_prerelease());
    println!("product version {product}, shown as LCL {}", parsed.shown());
}

#[test]
fn the_android_default_version_code_is_past_every_published_one() {
    // v0.1.1 was published with versionCode 7; a default build must never
    // look older than a release a phone may already have.
    let code: u64 = gradle_default("lclVersionCode").parse().expect("a number");
    assert!(code > 7, "default versionCode {code}");
}
