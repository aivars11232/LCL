//! The project manifest: `lcl.project.json`.
//!
//! ## Why a project needs a file at all
//!
//! Because the alternative is ambience. `05_SEMANTICS/02` says "Ambient
//! current directory and implied nearby files do not exist in portable LCL",
//! and a tool that inferred a project root by walking upward from wherever it
//! was invoked would be reintroducing exactly that. A manifest makes the root
//! an explicit, named thing: the directory the manifest is in, and nothing
//! else.
//!
//! ## Why it is JSON, and not LCL
//!
//! A manifest that was itself an LCL document would need the engine to read it,
//! and the engine needs the specification package the manifest names. That is a
//! bootstrap loop with no honest resolution. JSON avoids it, and the trust root
//! already carries a strict reader for JSON — no trailing commas, no comments,
//! no duplicate keys — so the manifest is held to the same standard as the
//! canonical registries.
//!
//! ## Unknown keys are refused
//!
//! `07_VERSIONING_AND_EXTENSIONS/05` forbids an ignore-unknown mode for
//! normative content, and while a manifest is not normative content, the same
//! reasoning applies to a file that decides which specification an engine
//! loads: silently ignoring a key a future version gives meaning to would make
//! an old tool read a new manifest wrongly and say nothing.

use lcl_spec::json::{self, Json};
use std::fmt;
use std::path::{Path, PathBuf};

/// The manifest's file name. A project is the directory that contains one.
pub const MANIFEST_FILE: &str = "lcl.project.json";

/// The only manifest format this build understands.
pub const MANIFEST_FORMAT: &str = "lcl.project/1";

/// Every key a manifest may carry.
const KNOWN_KEYS: &[&str] = &[
    "format",
    "spec",
    "entry",
    "cache",
    "lock",
    "localized_spec",
    "profiles",
];

/// Why a manifest could not be used.
#[derive(Debug)]
pub enum ManifestError {
    Io { path: PathBuf, detail: String },
    Json { path: PathBuf, detail: String },
    Invalid { path: PathBuf, detail: String },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io { path, detail }
            | ManifestError::Json { path, detail }
            | ManifestError::Invalid { path, detail } => write!(f, "{}: {detail}", path.display()),
        }
    }
}

impl std::error::Error for ManifestError {}

/// One project's declared configuration.
///
/// Every path is stored exactly as written and resolved against the project
/// root by [`crate::Project`], never against a working directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Where the canonical specification package is, as written.
    pub spec: Option<String>,
    /// The document a bare command acts on, as written.
    pub entry: Option<String>,
    /// The package cache directory, as written.
    pub cache: Option<String>,
    /// The lock file, as written.
    pub lock: Option<String>,
    /// Where the canonical LCL Core 0.2.0 package, whose localization stage
    /// judges localized documents, is, as written.
    pub localized_spec: Option<String>,
    /// The directory of `<locale>.json` locale profiles, as written.
    pub profiles: Option<String>,
}

impl Manifest {
    /// Read and validate the manifest at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
            path: path.to_path_buf(),
            detail: format!("the project manifest is not readable: {e}"),
        })?;
        Manifest::parse(&text, path)
    }

    /// Parse one manifest's text, reporting against `path`.
    pub fn parse(text: &str, path: &Path) -> Result<Manifest, ManifestError> {
        let invalid = |detail: String| ManifestError::Invalid {
            path: path.to_path_buf(),
            detail,
        };
        let value = json::parse(text).map_err(|e| ManifestError::Json {
            path: path.to_path_buf(),
            detail: format!("the project manifest is not valid JSON: {e}"),
        })?;
        let Some(members) = value.as_object() else {
            return Err(invalid(format!(
                "the project manifest must be a JSON object, not a {}",
                value.type_name()
            )));
        };

        for (key, _) in members {
            if !KNOWN_KEYS.contains(&key.as_str()) {
                return Err(invalid(format!(
                    "unknown manifest key {key:?}; this build understands {}",
                    KNOWN_KEYS.join(", ")
                )));
            }
        }

        match value.get("format").and_then(Json::as_str) {
            Some(MANIFEST_FORMAT) => {}
            Some(other) => {
                return Err(invalid(format!(
                    "unknown manifest format {other:?}; this build understands \
                     {MANIFEST_FORMAT:?}"
                )))
            }
            None => {
                return Err(invalid(format!(
                    "the manifest must declare \"format\": {MANIFEST_FORMAT:?}"
                )))
            }
        }

        let text_field = |key: &str| -> Result<Option<String>, ManifestError> {
            match value.get(key) {
                None => Ok(None),
                Some(Json::String(text)) if !text.is_empty() => Ok(Some(text.clone())),
                Some(Json::String(_)) => Err(invalid(format!("{key:?} is empty"))),
                Some(other) => Err(invalid(format!(
                    "{key:?} must be a string, not a {}",
                    other.type_name()
                ))),
            }
        };

        Ok(Manifest {
            spec: text_field("spec")?,
            entry: text_field("entry")?,
            cache: text_field("cache")?,
            lock: text_field("lock")?,
            localized_spec: text_field("localized_spec")?,
            profiles: text_field("profiles")?,
        })
    }
}
