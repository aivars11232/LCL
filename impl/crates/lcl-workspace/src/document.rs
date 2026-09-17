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

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

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
    /// A newer write to this exact path has already been published, so this
    /// one is stale and was not applied.
    Superseded(PathBuf),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentError::AlreadyExists(path) => write!(f, "{} already exists", path.display()),
            DocumentError::Superseded(path) => write!(
                f,
                "{} was saved again while this write was still in flight; the newer content is kept",
                path.display()
            ),
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
    let bytes = lcl_project::read_file(&path).map_err(|e| DocumentError::Io {
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
    // Taken now, when this write is accepted, and not at publication: what
    // makes one write newer than another is when it was asked for.
    let sequence = NEXT_WRITE.fetch_add(1, Ordering::SeqCst) + 1;

    let directory = path.parent().unwrap_or(root);
    std::fs::create_dir_all(directory).map_err(|e| DocumentError::Io {
        path: directory.to_path_buf(),
        detail: format!("the directory could not be created: {e}"),
    })?;

    let prepared = Temporary::prepare(directory, text.as_bytes())?;

    #[cfg(test)]
    before_publish(&path);

    prepared.publish(&path, publication, sequence)?;

    let digest = lcl_spec::sha256::hex_digest(text.as_bytes());
    Ok(Document {
        id: relative.to_string(),
        text,
        digest,
    })
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// The order writes were accepted in, and the newest already published for each
/// path.
///
/// ## Why ordering has to live here
///
/// A save is accepted, prepared and only then published, and between those
/// moments a newer save of the same file can be accepted, prepared, published
/// and acknowledged. The older write then renames its stale bytes over the
/// newer ones. Nothing above this layer can prevent it: the editor's own
/// queue is per open document, so closing a tab and reopening the same path
/// starts a second queue that knows nothing about the first, and a queue is in
/// any case not shared with another writer.
///
/// So the order is decided where the bytes actually land. Each write takes a
/// number when it is accepted, and publication happens under one lock that
/// refuses a number older than the one already on that path. The check and the
/// publication are the same critical section, because a check that releases its
/// lock before publishing is the defect again with more steps.
///
/// Scope, stated rather than implied: this orders the writes *this process*
/// accepted. Two workspace processes sharing a root do not order against each
/// other through it; atomic create-only publication is what protects them from
/// destroying each other's files, and an explicit precondition is available to
/// a caller that wants one.
static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);
static PUBLISHED: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

/// A test seam, compiled out of the product entirely. It runs after a write has
/// taken its number and prepared its bytes, and before it publishes them —
/// which is exactly the window a newer save can pass through.
///
/// Held as an `Arc` so the hook is cloned out and the lock released before it
/// runs: the hook's whole purpose is to perform another write, and calling it
/// under this lock would deadlock against that write.
///
/// It is handed the destination, because unit tests run in parallel threads
/// over one static and a hook that fired for every writer in the process would
/// be answering other tests' writes.
#[cfg(test)]
#[allow(clippy::type_complexity)]
static BEFORE_PUBLISH: Mutex<Option<std::sync::Arc<dyn Fn(&Path) + Send + Sync>>> =
    Mutex::new(None);

#[cfg(test)]
fn before_publish(destination: &Path) {
    let hook = BEFORE_PUBLISH
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook(destination);
    }
}

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
        sequence: u64,
    ) -> Result<(), DocumentError> {
        // One critical section: decide whether this write is still the newest
        // for this path, and publish it, without letting go in between. A check
        // that released its lock before publishing would be the defect again
        // with more steps.
        //
        // Ordering governs replacing saves only. A create is a reservation
        // rather than newer content, and its refusal must keep saying that the
        // destination is occupied — which is what the atomic create-only
        // publication above already establishes.
        let mut order = PUBLISHED.lock().unwrap_or_else(|e| e.into_inner());
        let ordered = matches!(publication, Publication::Replace);
        if ordered
            && order
                .get(destination)
                .is_some_and(|newest| *newest > sequence)
        {
            let error = DocumentError::Superseded(destination.to_path_buf());
            drop(order);
            return Err(self.failure(error));
        }
        let published = match publication {
            Publication::Create => std::fs::hard_link(&self.path, destination),
            Publication::Replace => std::fs::rename(&self.path, destination),
        };
        if ordered && published.is_ok() {
            order.insert(destination.to_path_buf(), sequence);
        }
        drop(order);
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
                left.publish(&target, Publication::Create, 1)
            });
            let b = scope.spawn(|| {
                barrier.wait();
                right.publish(&target, Publication::Create, 2)
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_ne!(a.is_ok(), b.is_ok());
        let (winner, loser) = if a.is_ok() { (&first, b) } else { (&second, a) };
        assert!(matches!(loser, Err(DocumentError::AlreadyExists(_))));
        assert_eq!(std::fs::read(target).unwrap(), *winner);
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }

    /// UI-03: an older save must not replace a newer one that was already
    /// acknowledged.
    ///
    /// The exact interleaving the finding names: an older write is held
    /// **before** its bytes are published — not merely before its response is
    /// delivered — while the document is closed, reopened and saved again with
    /// newer text, successfully. The older write is then released.
    ///
    /// A per-document queue in the editor cannot prevent this. Closing a tab
    /// and reopening the same path creates a second document object with its
    /// own queue, which knows nothing about the write the first one left in
    /// flight. Delaying the response, abandoning the request in the browser or
    /// checking the destination before renaming would all leave the same
    /// window open, because by then the rename has already happened or is about
    /// to. The ordering has to be decided where the bytes land.
    #[test]
    fn an_older_write_held_before_publication_cannot_replace_a_newer_one() {
        let directory = Directory::new();
        let root = directory.0.clone();
        std::fs::write(root.join("doc.lcl.txt"), b"original\n").unwrap();

        // While the older save sits between taking its number and publishing,
        // a newer save of the same path is accepted, published and finished.
        let newer_root = root.clone();
        let watched = root.join("doc.lcl.txt");
        let newer_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&newer_done);
        *BEFORE_PUBLISH.lock().unwrap() = Some(std::sync::Arc::new(move |destination: &Path| {
            // Only this case's own document, because unit tests run in
            // parallel over one static, and only the first write through the
            // seam — the second is the newer save this hook performs.
            if destination != watched {
                return;
            }
            if flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            write(&newer_root, "doc.lcl.txt", "newer text\n")
                .expect("the newer save is accepted and published");
        }));

        let older = write(&root, "doc.lcl.txt", "older text\n");
        *BEFORE_PUBLISH.lock().unwrap() = None;

        assert_eq!(
            std::fs::read_to_string(root.join("doc.lcl.txt")).unwrap(),
            "newer text\n",
            "the acknowledged newer content must still be on disk"
        );
        assert!(
            matches!(older, Err(DocumentError::Superseded(_))),
            "and the stale write must be told it was superseded rather than \
             reporting a success it did not have: {older:?}"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            1,
            "with no temporary left behind"
        );
    }

    /// The control: ordinary sequential saves still replace, in order.
    #[test]
    fn sequential_saves_still_replace_in_order() {
        let directory = Directory::new();
        let root = directory.0.clone();
        write(&root, "doc.lcl.txt", "first\n").expect("first save");
        write(&root, "doc.lcl.txt", "second\n").expect("second save");
        write(&root, "doc.lcl.txt", "third\n").expect("third save");
        assert_eq!(
            std::fs::read_to_string(root.join("doc.lcl.txt")).unwrap(),
            "third\n"
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    /// And a create is still refused for being occupied, not for being late.
    #[test]
    fn a_create_over_an_existing_document_still_reports_it_exists() {
        let directory = Directory::new();
        let root = directory.0.clone();
        write(&root, "doc.lcl.txt", "content\n").expect("save");
        let created = create(&root, "doc.lcl.txt", "other\n");
        assert!(
            matches!(created, Err(DocumentError::AlreadyExists(_))),
            "{created:?}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("doc.lcl.txt")).unwrap(),
            "content\n"
        );
    }

    #[test]
    fn failed_publication_cleans_its_temporary_and_preserves_existing_data() {
        let directory = Directory::new();
        let target = directory.0.join("occupied");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("user.txt"), b"preserved").unwrap();
        let prepared = Temporary::prepare(&directory.0, b"replacement").unwrap();
        assert!(prepared.publish(&target, Publication::Replace, 1).is_err());
        assert_eq!(
            std::fs::read(target.join("user.txt")).unwrap(),
            b"preserved"
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }
}
