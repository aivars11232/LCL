//! Reading and writing one LCL document, byte for byte.
//!
//! Nothing here looks at what the document is called. A document is its bytes,
//! and `.lcl` and `.lcl.txt` are read and written identically; a save writes
//! the name it was given and never renames what it opened.
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

use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Why a document could not be read or written.
#[derive(Debug)]
pub enum DocumentError {
    /// Atomic create-only publication found an occupied destination.
    AlreadyExists(PathBuf),
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
            DocumentError::AlreadyExists(path) => write!(f, "{} already exists", path.display()),
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
    persist(root, relative, text, Publication::Replace)
}

/// Publish complete bytes only if the destination is absent. A same-directory
/// hard link is atomic and cannot replace an existing file or symbolic link.
/// Filesystems without that operation fail explicitly; there is no replacing
/// rename fallback. Ordinary saving continues to use `write`.
pub fn create(root: &Path, relative: &str, text: &str) -> Result<Document, DocumentError> {
    persist(root, relative, text, Publication::Create)
}

#[derive(Clone, Copy)]
enum Publication {
    Create,
    Replace,
}

fn persist(
    root: &Path,
    relative: &str,
    text: &str,
    publication: Publication,
) -> Result<Document, DocumentError> {
    let path = resolve(root, relative)?;
    let (text, _) = admissible(text)?;

    let directory = path.parent().unwrap_or(root);
    std::fs::create_dir_all(directory).map_err(|e| DocumentError::Io {
        path: directory.to_path_buf(),
        detail: format!("the directory could not be created: {e}"),
    })?;

    Temporary::prepare(directory, text.as_bytes())?.publish(&path, publication)?;

    let digest = lcl_spec::sha256::hex_digest(text.as_bytes());
    Ok(Document {
        id: relative.to_string(),
        text,
        digest,
    })
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Only a file successfully reserved by create_new belongs to this guard.
struct Temporary {
    path: PathBuf,
    owned: bool,
}

impl Temporary {
    fn prepare(directory: &Path, bytes: &[u8]) -> Result<Self, DocumentError> {
        for _ in 0..128 {
            let number = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
            let path = directory.join(format!(".lcl-write-{}-{number}.tmp", std::process::id()));
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(DocumentError::Io {
                        path,
                        detail: format!("the temporary file could not be reserved: {error}"),
                    })
                }
            };
            let mut temporary = Temporary { path, owned: true };
            let written = file.write_all(bytes).and_then(|()| file.flush());
            drop(file);
            if let Err(error) = written {
                let error = DocumentError::Io {
                    path: temporary.path.clone(),
                    detail: format!("the document could not be written: {error}"),
                };
                return Err(temporary.failure(error));
            }
            return Ok(temporary);
        }
        Err(DocumentError::Io {
            path: directory.to_path_buf(),
            detail: "no unused temporary name could be reserved after 128 attempts".into(),
        })
    }

    fn publish(
        mut self,
        destination: &Path,
        publication: Publication,
    ) -> Result<(), DocumentError> {
        let published = match publication {
            Publication::Create => std::fs::hard_link(&self.path, destination),
            Publication::Replace => std::fs::rename(&self.path, destination),
        };
        if let Err(error) = published {
            let error = if matches!(publication, Publication::Create)
                && error.kind() == std::io::ErrorKind::AlreadyExists
            {
                DocumentError::AlreadyExists(destination.to_path_buf())
            } else {
                DocumentError::Io {
                    path: destination.to_path_buf(),
                    detail: format!("the document could not be published: {error}"),
                }
            };
            return Err(self.failure(error));
        }
        if matches!(publication, Publication::Replace) {
            self.owned = false; // rename consumed this exact temporary name
        }
        self.remove().map_err(|error| DocumentError::Io {
            path: self.path.clone(),
            detail: format!("the document was published, but temporary cleanup failed: {error}"),
        })
    }

    fn remove(&mut self) -> std::io::Result<()> {
        if self.owned {
            match std::fs::remove_file(&self.path) {
                Ok(()) => self.owned = false,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.owned = false,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn failure(&mut self, original: DocumentError) -> DocumentError {
        match self.remove() {
            Ok(()) => original,
            Err(error) => DocumentError::Io {
                path: self.path.clone(),
                detail: format!("{original}; owned temporary cleanup also failed: {error}"),
            },
        }
    }
}

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "lcl-publication-{}-{}",
                std::process::id(),
                NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn two_fully_prepared_writers_cannot_both_create_the_destination() {
        let directory = Directory::new();
        let target = directory.0.join("new.lcl.txt");
        let first = vec![b'a'; 128 * 1024];
        let second = vec![b'b'; 128 * 1024];
        // Both complete files exist before either writer may publish. This
        // tests the actual filesystem publication point, not an existence lock.
        let left = Temporary::prepare(&directory.0, &first).unwrap();
        let right = Temporary::prepare(&directory.0, &second).unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (a, b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                left.publish(&target, Publication::Create)
            });
            let b = scope.spawn(|| {
                barrier.wait();
                right.publish(&target, Publication::Create)
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_ne!(a.is_ok(), b.is_ok());
        let (winner, loser) = if a.is_ok() { (&first, b) } else { (&second, a) };
        assert!(matches!(loser, Err(DocumentError::AlreadyExists(_))));
        assert_eq!(std::fs::read(target).unwrap(), *winner);
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    #[test]
    fn failed_publication_cleans_its_temporary_and_preserves_existing_data() {
        let directory = Directory::new();
        let target = directory.0.join("occupied");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("user.txt"), b"preserved").unwrap();
        let prepared = Temporary::prepare(&directory.0, b"replacement").unwrap();
        assert!(prepared.publish(&target, Publication::Replace).is_err());
        assert_eq!(
            std::fs::read(target.join("user.txt")).unwrap(),
            b"preserved"
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }
}
