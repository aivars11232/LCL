//! Reading and writing one `.lcl` document, byte for byte.
//!
//! ## Why saving is fussy
//!
//! `02_LEXICAL/01_CHARACTER_ENCODING_AND_SOURCE_TEXT.txt` requires UTF-8
//! without a byte order mark, U+000A line terminators only, and a final line
//! terminator; and it says an implementation "must not silently repair,
//! normalize, re-indent, re-quote, or otherwise rewrite source before
//! validation."
//!
//! An editor is the exact place that rule is usually broken. A text widget that
//! helpfully converts line endings, strips a trailing newline or adds a BOM
//! changes the bytes the engine will judge, and the user is then debugging a
//! document they did not write. So this module refuses instead: a save that
//! would write bytes the encoding rule forbids is rejected and reported, and
//! the file on disk is left exactly as it was.
//!
//! The one thing it does add is a missing final line feed, because that is the
//! difference between "the user pressed save" and "the document is malformed",
//! and every editor in existence treats the final newline as its own business.
//! It is stated here rather than done quietly, and the reply says it happened.

use std::path::{Component, Path, PathBuf};

/// Why a document could not be read or written.
#[derive(Debug)]
pub enum DocumentError {
    /// The path resolved outside the project root.
    Outside(PathBuf),
    /// The bytes are not valid UTF-8.
    NotUtf8(String),
    /// The bytes carry a byte order mark, which `02_LEXICAL/01` forbids.
    ByteOrderMark,
    /// The bytes carry a carriage return, which `02_LEXICAL/01` forbids.
    CarriageReturn {
        line: usize,
    },
    Io {
        path: PathBuf,
        detail: String,
    },
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentError::Outside(path) => {
                write!(f, "{} is outside the project root", path.display())
            }
            DocumentError::NotUtf8(detail) => write!(
                f,
                "an LCL document must be UTF-8, and this is not: {detail}"
            ),
            DocumentError::ByteOrderMark => f.write_str(
                "an LCL document must not begin with a byte order mark; \
                 02_LEXICAL/01 forbids one and forbids removing it silently",
            ),
            DocumentError::CarriageReturn { line } => write!(
                f,
                "line {line} ends with a carriage return; 02_LEXICAL/01 permits \
                 U+000A only, and this editor will not rewrite your source to fix it"
            ),
            DocumentError::Io { path, detail } => {
                write!(f, "{}: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for DocumentError {}

/// One document as the editor holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Root-relative identity, which is what every import of it also produces.
    pub id: String,
    /// The exact bytes on disk.
    pub text: String,
    /// SHA-256 of those bytes, for reload and conflict detection.
    pub digest: String,
}

/// Check one document's bytes against the canonical encoding rule.
///
/// Returns the text to write, which differs from the input only by a final
/// line feed, and only when one was missing.
pub fn admissible(text: &str) -> Result<(String, bool), DocumentError> {
    if text.starts_with('\u{feff}') {
        return Err(DocumentError::ByteOrderMark);
    }
    if let Some(offset) = text.find('\r') {
        let line = text[..offset].matches('\n').count() + 1;
        return Err(DocumentError::CarriageReturn { line });
    }
    if text.is_empty() || text.ends_with('\n') {
        return Ok((text.to_string(), false));
    }
    Ok((format!("{text}\n"), true))
}

/// Resolve one root-relative path inside a project root.
///
/// Two things have to hold at once, and they pull in different directions.
///
/// A path that exists must be checked *after* the filesystem has resolved it,
/// because a symbolic link pointing out of the project is exactly the escape a
/// textual check misses. `lcl-project` says it plainly: "textual prefix alone
/// does not establish containment."
///
/// A path that does *not* exist still has to resolve, because creating a new
/// document in a new folder is an ordinary thing to do in an editor, and a
/// path with no parent on disk cannot be canonicalised at all.
///
/// So: normalise the relative path lexically first, which is what rejects `..`
/// before the filesystem is consulted; then canonicalise the deepest ancestor
/// that does exist and check containment on that; then re-append the part that
/// does not exist yet. Every existing component is checked by the filesystem,
/// and nothing that does not exist can introduce a link.
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, DocumentError> {
    let requested = PathBuf::from(relative);
    let outside = || DocumentError::Outside(root.join(relative));

    // Lexical normalisation. An absolute path, a root, a prefix or a `..` that
    // climbs past the start is refused here, before anything is opened.
    let mut normalised = PathBuf::new();
    for component in requested.components() {
        match component {
            Component::Normal(part) => normalised.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalised.pop() {
                    return Err(outside());
                }
            }
            Component::RootDir | Component::Prefix(_) => return Err(outside()),
        }
    }
    if normalised.as_os_str().is_empty() {
        return Err(outside());
    }

    let candidate = root.join(&normalised);
    if let Ok(canonical) = candidate.canonicalize() {
        if !lcl_capabilities::contains(root, &canonical) {
            return Err(DocumentError::Outside(canonical));
        }
        return Ok(canonical);
    }

    // It does not exist yet. Find the deepest ancestor that does, prove *that*
    // is inside the project, and rebuild the rest onto it.
    let mut existing = candidate.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|n| n.to_os_string()) else {
            return Err(outside());
        };
        tail.push(name);
        if !existing.pop() {
            return Err(outside());
        }
    }
    let anchor = existing.canonicalize().map_err(|_| outside())?;
    if !lcl_capabilities::contains(root, &anchor) && anchor != root {
        return Err(DocumentError::Outside(anchor));
    }
    let mut resolved = anchor;
    for name in tail.into_iter().rev() {
        resolved.push(name);
    }
    Ok(resolved)
}

/// Read one document.
pub fn read(root: &Path, relative: &str) -> Result<Document, DocumentError> {
    let path = resolve(root, relative)?;
    let bytes = std::fs::read(&path).map_err(|e| DocumentError::Io {
        path: path.clone(),
        detail: format!("the document is not readable: {e}"),
    })?;
    let text = String::from_utf8(bytes).map_err(|e| DocumentError::NotUtf8(e.to_string()))?;
    let digest = lcl_spec::sha256::hex_digest(text.as_bytes());
    Ok(Document {
        id: relative.to_string(),
        text,
        digest,
    })
}

/// Write one document, atomically.
///
/// The bytes go to a temporary file in the same directory and are then renamed
/// over the target, so a crash or a full disk leaves the previous version
/// intact rather than a half-written one. Same directory, because a rename
/// across filesystems is a copy and would lose that property.
pub fn write(root: &Path, relative: &str, text: &str) -> Result<Document, DocumentError> {
    let path = resolve(root, relative)?;
    let (text, _) = admissible(text)?;

    let directory = path.parent().unwrap_or(root);
    std::fs::create_dir_all(directory).map_err(|e| DocumentError::Io {
        path: directory.to_path_buf(),
        detail: format!("the directory could not be created: {e}"),
    })?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());
    let temporary = directory.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&temporary, text.as_bytes()).map_err(|e| DocumentError::Io {
        path: temporary.clone(),
        detail: format!("the document could not be written: {e}"),
    })?;
    std::fs::rename(&temporary, &path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        DocumentError::Io {
            path: path.clone(),
            detail: format!("the document could not be replaced: {e}"),
        }
    })?;

    let digest = lcl_spec::sha256::hex_digest(text.as_bytes());
    Ok(Document {
        id: relative.to_string(),
        text,
        digest,
    })
}
