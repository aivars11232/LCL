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

/// Android's versionCode for a product version, as the Android build and the
/// release builder both derive it.
fn version_code_of(version: &str) -> u64 {
    let v = lcl_update::version::Version::parse(version).unwrap();
    v.major * 1_000_000 + v.minor * 1_000 + v.patch
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
fn the_android_version_code_follows_from_the_version_and_is_past_every_published_one() {
    // Nobody types the code: the Android build derives it from versionName,
    // and the release builder from the product version, by one rule.
    assert_eq!(
        gradle_default("lclVersionCode"),
        "versionCodeOf(versionName!!)",
        "build.gradle.kts no longer derives versionCode from the version"
    );
    assert!(read("packaging/build_update_release.sh")
        .contains("$1 * 1000000 + $2 * 1000 + $3"));
    assert_eq!(version_code_of("0.5.1"), 5001);
    // v0.1.1 was published with versionCode 7 and v0.5.0 with 8; a build of
    // this version must never look older than a release a phone may have.
    let code = version_code_of(lcl_update::PRODUCT_VERSION);
    assert!(code > 8, "versionCode {code}");
    // And it grows with the version, so a later release always installs.
    assert!(version_code_of("0.5.2") > code && version_code_of("0.6.0") > code && version_code_of("1.0.0") > code);
}
