//! Checking for an update: ask the pinned source for its latest stable
//! release, fetch the release's manifest and signature, verify the signature
//! before reading anything in the manifest, and only then compare versions.

use crate::http::Failure;
use crate::manifest::{self, Manifest};
use crate::source::{Source, MANIFEST_ASSET, SIGNATURE_ASSET};
use crate::state::{self, Available, Paths, State};
use crate::trust::{self, TrustedKey};
use crate::version::Version;

/// Everything a check, a download or an installation works from.
pub struct Context {
    pub paths: Paths,
    pub source: Source,
    pub keys: Vec<TrustedKey>,
    pub installed: Version,
}

impl Context {
    pub fn from_env() -> Result<Context, String> {
        Ok(Context {
            paths: Paths::from_env()?,
            source: Source::configured()?,
            keys: trust::trusted_keys()?,
            installed: Version::parse(crate::PRODUCT_VERSION)?,
        })
    }
}

/// A manifest read only after its signature verified, and only if it names
/// the key that actually signed it.
pub fn verify(bytes: &[u8], signature: &[u8], keys: &[TrustedKey]) -> Result<Manifest, String> {
    let signer = trust::verify(bytes, signature, keys)?;
    let manifest = manifest::parse(bytes)?;
    if manifest.signing_key_id != signer {
        return Err(format!(
            "the manifest names key {}, but key {signer} signed it",
            manifest.signing_key_id
        ));
    }
    Ok(manifest)
}

/// Whether `manifest` is newer than `installed` and can replace it here: an
/// older or equal version is never offered, and one this updater cannot
/// install says why.
pub fn applicable(
    manifest: &Manifest,
    installed: &Version,
) -> Result<bool, (&'static str, String)> {
    if manifest.product_version <= *installed {
        return Ok(false);
    }
    if manifest.pc.architecture != crate::ARCHITECTURE {
        return Err((
            "unsupported",
            format!(
                "LCL {} is for {}, and this computer is {}",
                manifest.product_version,
                manifest.pc.architecture,
                crate::ARCHITECTURE
            ),
        ));
    }
    if manifest.pc.required_updater_version > crate::UPDATER_PROTOCOL {
        return Err((
            "unsupported",
            format!(
                "LCL {} needs a newer updater than this one; install it manually",
                manifest.product_version
            ),
        ));
    }
    if *installed < manifest.minimum_supported_version {
        return Err((
            "unsupported",
            format!(
                "LCL {} replaces {} or newer only, and {installed} is installed; install it manually",
                manifest.product_version, manifest.minimum_supported_version
            ),
        ));
    }
    Ok(true)
}

/// The kind a request failure is reported as.
pub fn kind(failure: &Failure) -> &'static str {
    match failure {
        Failure::Offline(_) => "offline",
        Failure::Status(_) | Failure::Invalid(_) => "invalid",
    }
}

/// Check now, and return the state to record.
pub fn check(ctx: &Context) -> State {
    let mut state = state::load(&ctx.paths);
    let now = state::now();
    let previous = state.clone();
    state.checked_at = Some(now);
    state.progress = None;
    state.error = None;
    if ctx.keys.is_empty() {
        state.state = "not_configured".to_string();
        state.available = None;
        state.error = Some((
            "not_configured".to_string(),
            trust::NOT_CONFIGURED.to_string(),
        ));
        return state;
    }
    let release = match ctx.source.latest() {
        Ok(Some(release)) => release,
        Ok(None) => {
            state.last_success_at = Some(now);
            state.state = "up_to_date".to_string();
            state.available = None;
            return state;
        }
        Err(failure) if kind(&failure) == "offline" => {
            state.state = "offline".to_string();
            state.error = Some(("offline".to_string(), failure.to_string()));
            return state;
        }
        Err(failure) => return state.failed("invalid", failure.to_string()),
    };
    state.last_success_at = Some(now);
    let fetch = |name: &str, limit: u64| {
        if release.asset(name).is_none() {
            return Err(("invalid", format!("release {} has no {name}", release.tag)));
        }
        ctx.source
            .fetch(&release, name, limit, &mut |_, _| {})
            .map_err(|f| (kind(&f), f.to_string()))
    };
    let fetched = fetch(MANIFEST_ASSET, manifest::MAX_MANIFEST)
        .and_then(|bytes| Ok((bytes, fetch(SIGNATURE_ASSET, 1024)?)));
    let (bytes, signature) = match fetched {
        Ok(pair) => pair,
        Err((kind, message)) => return state.failed(kind, message),
    };
    let manifest = match verify(&bytes, &signature, &ctx.keys) {
        Ok(manifest) => manifest,
        Err(why) => return state.failed("verification", why),
    };
    if manifest.release_tag != release.tag {
        return state.failed(
            "verification",
            format!(
                "release {} carries the manifest of {}",
                release.tag, manifest.release_tag
            ),
        );
    }
    match release.asset(&manifest.pc.artifact_name) {
        Some(asset) if asset.size == manifest.pc.size => {}
        _ => {
            return state.failed(
                "invalid",
                format!(
                    "release {} does not hold the PC artifact its manifest names",
                    release.tag
                ),
            )
        }
    }
    match applicable(&manifest, &ctx.installed) {
        Ok(false) => {
            state.state = "up_to_date".to_string();
            state.available = None;
        }
        Ok(true) => {
            let saved = state::write_atomically(&ctx.paths.manifest(), &bytes)
                .and_then(|_| state::write_atomically(&ctx.paths.signature(), &signature));
            if let Err(why) = saved {
                return state.failed("install", why);
            }
            let version = manifest.product_version.to_string();
            let staged = previous.state == "ready_to_install"
                && previous
                    .available
                    .as_ref()
                    .is_some_and(|a| a.version == version);
            state.state = if staged {
                "ready_to_install"
            } else {
                "update_available"
            }
            .to_string();
            state.available = Some(Available {
                version,
                tag: manifest.release_tag.clone(),
                published_at: manifest.published_at.clone(),
                release_notes: manifest.release_notes.clone(),
                size: manifest.pc.size,
            });
        }
        Err((kind, message)) => {
            state.available = None;
            return state.failed(kind, message);
        }
    }
    state
}

/// The manifest a check saved, verified again: nothing on disk is trusted
/// because it was trusted before.
pub fn saved_manifest(ctx: &Context) -> Result<Manifest, String> {
    let read = |path: std::path::PathBuf| {
        std::fs::read(&path).map_err(|_| "no update has been found yet; check first".to_string())
    };
    verify(
        &read(ctx.paths.manifest())?,
        &read(ctx.paths.signature())?,
        &ctx.keys,
    )
}
