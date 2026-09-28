//! The update manifest: the one signed statement of what a release holds.
//!
//! It is JSON, `update-manifest.json`, and its exact bytes are signed; the
//! detached signature is `update-manifest.sig`. It is read only after the
//! signature has been verified ([`crate::trust::verify`]), and then strictly:
//! every key is required, no other key is allowed, and every value is checked.
//! It names artifacts, never URLs and never commands: an artifact is fetched
//! from the same official release by its name, and trusted only if its size
//! and SHA-256 are the ones signed here.
//!
//! ```text
//! {
//!   "format": 1,
//!   "product": "lcl",
//!   "channel": "stable",
//!   "product_version": "0.2.0",
//!   "release_tag": "v0.2.0",
//!   "source_commit": "<40 hex digits>",
//!   "published_at": "2026-10-01T12:00:00Z",
//!   "release_notes": "shown to the person, never interpreted",
//!   "minimum_supported_version": "0.1.0",
//!   "signing_key_id": "lcl-update-1",
//!   "pc": {
//!     "artifact_name": "lcl-0.2.0-linux-x86_64.tar.gz",
//!     "size": 12345, "sha256": "<64 hex digits>",
//!     "architecture": "x86_64-linux", "required_updater_version": 1
//!   },
//!   "android": {
//!     "artifact_name": "lcl-android-0.2.0-3.apk",
//!     "size": 12345, "sha256": "<64 hex digits>",
//!     "application_id": "io.lcl.workspace",
//!     "version_name": "0.2.0", "version_code": 3, "minimum_sdk": 29,
//!     "signer_sha256": "<64 hex digits: the APK signing certificate>"
//!   }
//! }
//! ```

use crate::version::Version;
use lcl_spec::json::Json;

pub const FORMAT: u64 = 1;
pub const PRODUCT: &str = "lcl";
pub const CHANNEL: &str = "stable";
pub const APPLICATION_ID: &str = "io.lcl.workspace";
/// Upper bounds: a manifest, a PC payload, an APK.
pub const MAX_MANIFEST: u64 = 64 * 1024;
pub const MAX_PC_ARTIFACT: u64 = 512 * 1024 * 1024;
pub const MAX_APK: u64 = 256 * 1024 * 1024;
const MAX_NOTES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub product_version: Version,
    pub release_tag: String,
    pub source_commit: String,
    pub published_at: String,
    pub release_notes: String,
    pub minimum_supported_version: Version,
    pub signing_key_id: String,
    pub pc: PcArtifact,
    pub android: AndroidArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcArtifact {
    pub artifact_name: String,
    pub size: u64,
    pub sha256: String,
    pub architecture: String,
    pub required_updater_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidArtifact {
    pub artifact_name: String,
    pub size: u64,
    pub sha256: String,
    pub application_id: String,
    pub version_name: String,
    pub version_code: u64,
    pub minimum_sdk: u64,
    pub signer_sha256: String,
}

/// The PC artifact of a product version.
pub fn pc_artifact_name(version: &Version) -> String {
    format!("lcl-{version}-linux-x86_64.tar.gz")
}

/// The Android artifact of a version name and code.
pub fn android_artifact_name(version_name: &str, version_code: u64) -> String {
    format!("lcl-android-{version_name}-{version_code}.apk")
}

/// The members of an object, exactly `keys`, or why not.
fn members<'j>(value: &'j Json, what: &str, keys: &[&str]) -> Result<&'j [(String, Json)], String> {
    let members = value
        .as_object()
        .ok_or_else(|| format!("{what} is not a JSON object"))?;
    for (key, _) in members {
        if !keys.contains(&key.as_str()) {
            return Err(format!("{what} has an unknown key {key:?}"));
        }
    }
    for key in keys {
        if members.iter().filter(|(k, _)| k == key).count() != 1 {
            return Err(format!("{what} needs exactly one {key:?}"));
        }
    }
    Ok(members)
}

fn text<'j>(value: &'j Json, what: &str, key: &str) -> Result<&'j str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| format!("{what}.{key} must be a string"))
}

fn number(value: &Json, what: &str, key: &str) -> Result<u64, String> {
    value
        .get(key)
        .and_then(Json::as_u64)
        .ok_or_else(|| format!("{what}.{key} must be a whole number"))
}

fn digest(value: &Json, what: &str, key: &str) -> Result<String, String> {
    let text = text(value, what, key)?;
    let ok = text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if ok {
        Ok(text.to_string())
    } else {
        Err(format!(
            "{what}.{key} must be 64 lowercase hexadecimal digits"
        ))
    }
}

fn size(value: &Json, what: &str, limit: u64) -> Result<u64, String> {
    match number(value, what, "size")? {
        n if n > 0 && n <= limit => Ok(n),
        n => Err(format!("{what}.size {n} is outside 1..={limit}")),
    }
}

fn timestamp(text: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SSZ
    let b = text.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, &c)| match i {
            4 | 7 => c == b'-',
            10 => c == b'T',
            13 | 16 => c == b':',
            19 => c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

/// Read a manifest whose signature has already been verified.
pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
    if bytes.len() as u64 > MAX_MANIFEST {
        return Err("the update manifest is too large".to_string());
    }
    let utf8 = std::str::from_utf8(bytes).map_err(|_| "the update manifest is not UTF-8")?;
    let json = lcl_spec::json::parse(utf8).map_err(|e| format!("the update manifest: {e}"))?;
    let top = "manifest";
    members(
        &json,
        top,
        &[
            "format",
            "product",
            "channel",
            "product_version",
            "release_tag",
            "source_commit",
            "published_at",
            "release_notes",
            "minimum_supported_version",
            "signing_key_id",
            "pc",
            "android",
        ],
    )?;
    if number(&json, top, "format")? != FORMAT {
        return Err(format!(
            "this updater reads update manifest format {FORMAT} only"
        ));
    }
    if !text_is(&json, "product", PRODUCT)? {
        return Err(format!("the manifest is not for the product {PRODUCT:?}"));
    }
    if !text_is(&json, "channel", CHANNEL)? {
        return Err(format!("the manifest is not for the {CHANNEL} channel"));
    }
    let version_text = text(&json, top, "product_version")?;
    let product_version = Version::parse(version_text)?;
    if product_version.is_prerelease() {
        return Err(format!(
            "{version_text} is a pre-release, which the stable channel never offers"
        ));
    }
    let release_tag = text(&json, top, "release_tag")?.to_string();
    if release_tag != format!("v{product_version}") {
        return Err(format!(
            "release tag {release_tag} does not name version {product_version}"
        ));
    }
    let source_commit = text(&json, top, "source_commit")?.to_string();
    if source_commit.len() != 40
        || !source_commit
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("manifest.source_commit must be a 40-digit lowercase commit id".to_string());
    }
    let published_at = text(&json, top, "published_at")?.to_string();
    if !timestamp(&published_at) {
        return Err("manifest.published_at must be YYYY-MM-DDTHH:MM:SSZ".to_string());
    }
    let release_notes = text(&json, top, "release_notes")?.to_string();
    if release_notes.chars().count() > MAX_NOTES {
        return Err("the release notes are too long".to_string());
    }
    let minimum_supported_version = Version::parse(text(&json, top, "minimum_supported_version")?)?;
    if minimum_supported_version > product_version {
        return Err("manifest.minimum_supported_version is newer than the release".to_string());
    }
    let signing_key_id = text(&json, top, "signing_key_id")?.to_string();

    let pc_json = json.get("pc").ok_or("manifest.pc is missing")?;
    members(
        pc_json,
        "pc",
        &[
            "artifact_name",
            "size",
            "sha256",
            "architecture",
            "required_updater_version",
        ],
    )?;
    let pc = PcArtifact {
        artifact_name: text(pc_json, "pc", "artifact_name")?.to_string(),
        size: size(pc_json, "pc", MAX_PC_ARTIFACT)?,
        sha256: digest(pc_json, "pc", "sha256")?,
        architecture: text(pc_json, "pc", "architecture")?.to_string(),
        required_updater_version: number(pc_json, "pc", "required_updater_version")?,
    };
    if pc.artifact_name != pc_artifact_name(&product_version) {
        return Err(format!(
            "pc.artifact_name must be {}, not {}",
            pc_artifact_name(&product_version),
            pc.artifact_name
        ));
    }

    let android_json = json.get("android").ok_or("manifest.android is missing")?;
    members(
        android_json,
        "android",
        &[
            "artifact_name",
            "size",
            "sha256",
            "application_id",
            "version_name",
            "version_code",
            "minimum_sdk",
            "signer_sha256",
        ],
    )?;
    let android = AndroidArtifact {
        artifact_name: text(android_json, "android", "artifact_name")?.to_string(),
        size: size(android_json, "android", MAX_APK)?,
        sha256: digest(android_json, "android", "sha256")?,
        application_id: text(android_json, "android", "application_id")?.to_string(),
        version_name: text(android_json, "android", "version_name")?.to_string(),
        version_code: number(android_json, "android", "version_code")?,
        minimum_sdk: number(android_json, "android", "minimum_sdk")?,
        signer_sha256: digest(android_json, "android", "signer_sha256")?,
    };
    if android.application_id != APPLICATION_ID {
        return Err(format!("android.application_id must be {APPLICATION_ID}"));
    }
    if android.version_name != version_text {
        return Err("android.version_name must be the product version".to_string());
    }
    if android.version_code == 0 || android.version_code > i32::MAX as u64 {
        return Err("android.version_code must be 1 to 2147483647".to_string());
    }
    let wanted = android_artifact_name(&android.version_name, android.version_code);
    if android.artifact_name != wanted {
        return Err(format!(
            "android.artifact_name must be {wanted}, not {}",
            android.artifact_name
        ));
    }
    Ok(Manifest {
        product_version,
        release_tag,
        source_commit,
        published_at,
        release_notes,
        minimum_supported_version,
        signing_key_id,
        pc,
        android,
    })
}

fn text_is(json: &Json, key: &str, wanted: &str) -> Result<bool, String> {
    Ok(text(json, "manifest", key)? == wanted)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A valid manifest for `version`, as JSON text, with its PC artifact's
    /// size and digest.
    pub fn sample(version: &str, pc_size: u64, pc_sha: &str) -> String {
        format!(
            r#"{{"format": 1, "product": "lcl", "channel": "stable", "product_version": "{version}",
"release_tag": "v{version}", "source_commit": "{commit}", "published_at": "2026-10-01T12:00:00Z",
"release_notes": "Faster checks.\nFixed a crash.", "minimum_supported_version": "0.1.0",
"signing_key_id": "test-key",
"pc": {{"artifact_name": "lcl-{version}-linux-x86_64.tar.gz", "size": {pc_size}, "sha256": "{pc_sha}",
"architecture": "x86_64-linux", "required_updater_version": 1}},
"android": {{"artifact_name": "lcl-android-{version}-7.apk", "size": 1000, "sha256": "{sha}",
"application_id": "io.lcl.workspace", "version_name": "{version}", "version_code": 7,
"minimum_sdk": 29, "signer_sha256": "{sha}"}}}}"#,
            commit = "a".repeat(40),
            sha = "b".repeat(64),
        )
    }

    #[test]
    fn a_valid_manifest_reads_exactly() {
        let text = sample("0.2.0", 10, &"c".repeat(64));
        let m = parse(text.as_bytes()).unwrap();
        assert_eq!(m.product_version.to_string(), "0.2.0");
        assert_eq!(m.pc.artifact_name, "lcl-0.2.0-linux-x86_64.tar.gz");
        assert_eq!(m.pc.size, 10);
        assert_eq!(m.android.version_code, 7);
        assert_eq!(m.release_notes, "Faster checks.\nFixed a crash.");
    }

    #[test]
    fn anything_malformed_or_unexpected_is_refused() {
        let good = sample("0.2.0", 10, &"c".repeat(64));
        let cases: Vec<(String, &str)> = vec![
            (good.replace("\"format\": 1", "\"format\": 2"), "format"),
            (good.replace("\"stable\"", "\"beta\""), "channel"),
            (
                good.replace("\"product\": \"lcl\"", "\"product\": \"other\""),
                "product",
            ),
            (
                good.replace(
                    "\"product_version\": \"0.2.0\"",
                    "\"product_version\": \"0.2.0-rc.1\"",
                ),
                "pre-release",
            ),
            (
                good.replace("\"release_tag\": \"v0.2.0\"", "\"release_tag\": \"v0.3.0\""),
                "release tag",
            ),
            (
                good.replace("lcl-0.2.0-linux", "lcl-0.3.0-linux"),
                "pc.artifact_name",
            ),
            (
                good.replace("lcl-android-0.2.0-7", "../evil"),
                "android.artifact_name",
            ),
            (good.replace("\"size\": 10", "\"size\": 0"), "size"),
            (good.replace("\"size\": 10", "\"size\": \"10\""), "size"),
            (good.replace(&"c".repeat(64), &"C".repeat(64)), "sha256"),
            (
                good.replace("io.lcl.workspace", "io.other.app"),
                "application_id",
            ),
            (
                good.replace("\"version_code\": 7", "\"version_code\": 0"),
                "version_code",
            ),
            (
                good.replace("\"format\": 1,", "\"format\": 1, \"run\": \"rm -rf ~\","),
                "unknown key",
            ),
            (
                good.replace("\"signing_key_id\": \"test-key\",", ""),
                "signing_key_id",
            ),
            (
                good.replace("2026-10-01T12:00:00Z", "yesterday"),
                "published_at",
            ),
            (
                good.replace(
                    "\"minimum_supported_version\": \"0.1.0\"",
                    "\"minimum_supported_version\": \"0.9.0\"",
                ),
                "minimum",
            ),
            ("[]".to_string(), "object"),
            ("{not json".to_string(), "json"),
        ];
        for (text, why) in cases {
            assert!(parse(text.as_bytes()).is_err(), "accepted with {why}");
        }
    }
}
