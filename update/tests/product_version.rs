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

/// The default after `?:` in the Gradle statement that reads `property`,
/// however the formatter wrapped that statement.
fn gradle_default(property: &str) -> String {
    let script = read("android/app/build.gradle.kts");
    let flat = script.split_whitespace().collect::<Vec<_>>().join(" ");
    let statement = flat
        .split(&format!("gradleProperty(\"{property}\")"))
        .nth(1)
        .unwrap_or_else(|| panic!("build.gradle.kts does not read {property}"));
    let default = statement
        .split("?:")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("{property} has no default"));
    default.trim_matches('"').to_string()
}

/// Android's versionCode for a product version, as the Android build and the
/// release builder both derive it: MAJOR·1 000 000 + MINOR·1 000 + PATCH, for
/// exactly MAJOR.MINOR.PATCH with MINOR and PATCH at most 999, within
/// Android's range; `None` for any other version.
fn version_code_of(version: &str) -> Option<u64> {
    let v = lcl_update::version::Version::parse(version).ok()?;
    if v.is_prerelease() || v.minor > 999 || v.patch > 999 {
        return None;
    }
    let code = v.major * 1_000_000 + v.minor * 1_000 + v.patch;
    (1..=2_100_000_000).contains(&code).then_some(code)
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
    let builder = read("packaging/build_update_release.sh");
    assert!(builder.contains("$1 * 1000000 + $2 * 1000 + $3"));
    // Both anchor the version and bound its parts, so the two never differ.
    assert!(builder.contains("/^[0-9]+\\.[0-9]+\\.[0-9]+$/ && $2 <= 999 && $3 <= 999"));
    assert!(builder.contains("code <= 2100000000"));
    let gradle = read("android/app/build.gradle.kts");
    assert!(
        gradle.contains("matchEntire(version)") && gradle.contains("minor <= 999 && patch <= 999")
    );
    assert!(gradle.contains("code in 1..2_100_000_000"));
    assert_eq!(version_code_of("0.9.0"), Some(9000));
    // v0.1.1 was published with versionCode 7, v0.5.0 with 8, v0.5.2 with
    // 5002, v0.9.0 with 9000, v0.9.1 with 9001; a build of this version must
    // never look older than a release a phone may have.
    let code = version_code_of(lcl_update::PRODUCT_VERSION).expect("a code follows");
    assert!(code > 9001, "versionCode {code}");
    // And it grows with the version, so a later release always installs.
    assert!(
        version_code_of("1.0.1") > Some(code)
            && version_code_of("1.1.0") > Some(code)
            && version_code_of("2.0.0") > Some(code)
    );
    // The boundary of a part: 0.5.999 and 0.6.0 are neighbours, never equal.
    assert_eq!(version_code_of("0.5.999"), Some(5999));
    assert_eq!(version_code_of("0.6.0"), Some(6000));
    assert_eq!(version_code_of("2100.0.0"), Some(2_100_000_000));
    // Beyond it, or not exactly MAJOR.MINOR.PATCH: no code, so no build.
    for bad in [
        "0.5.1000",
        "0.1000.0",
        "2101.0.0",
        "0.0.0",
        "1.2",
        "1.2.3.4",
        "v1.2.3",
        "1.2.3-rc1",
        " 1.2.3",
        "1.2.3 ",
        "01.2.3",
    ] {
        assert_eq!(version_code_of(bad), None, "{bad}");
    }
}
