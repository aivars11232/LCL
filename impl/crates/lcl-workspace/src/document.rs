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
    /// The name is not an LCL document's: only the exact `.lcl` and
    /// `.lcl.txt` endings are, and nothing else is deleted as one.
    NotADocument(PathBuf),
    /// Nothing is there.
    NotFound(PathBuf),
    /// Something is there, but it is a directory, a symbolic link or another
    /// special file rather than a document file.
    NotAFile(PathBuf),
    /// The bytes on disk are not the ones the caller confirmed, so they were
    /// left alone.
    Changed(PathBuf),
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
            DocumentError::NotADocument(path) => write!(
                f,
                "{} is not an LCL document: only a name ending in .lcl or .lcl.txt is",
                path.display()
            ),
            DocumentError::NotFound(path) => write!(f, "{} does not exist", path.display()),
            DocumentError::NotAFile(path) => write!(
                f,
                "{} is a directory, a link or another special file, not a document file",
                path.display()
            ),
            DocumentError::Changed(path) => write!(
                f,
                "{} changed on disk after it was shown, so it was left alone",
                path.display()
            ),
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
    let outside = || DocumentError::Outside(root.join(relative));
    let normalised = normalise(root, relative)?;

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

/// Lexical normalisation of one root-relative path. An absolute path, a root,
/// a prefix or a `..` that climbs past the start is refused here, before
/// anything is opened.
fn normalise(root: &Path, relative: &str) -> Result<PathBuf, DocumentError> {
    let outside = || DocumentError::Outside(root.join(relative));
    let mut normalised = PathBuf::new();
    for component in Path::new(relative).components() {
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
    Ok(normalised)
}

/// Delete one document, and only the bytes the caller confirmed.
///
/// The request names a document and the SHA-256 of the content that was on
/// screen when deletion was confirmed. Whatever else is true, this removes
/// nothing else:
///
/// * the path is normalised lexically and its directory is canonicalised and
///   proven inside the project, exactly as for a save;
/// * the name must be an LCL document's, so an ordinary `.txt` file, a
///   manifest or anything else in the project is never deleted through this;
/// * the entry itself must be a regular file. It is not canonicalised: a
///   symbolic link named `a.lcl` would resolve to its target, and deleting the
///   target of a link the user pointed at is deleting a different file, so a
///   link is refused instead, as is a directory;
/// * the bytes are read and compared with the confirmed digest under the lock
///   saves publish under, and removed in the same critical section, so no save
///   from this process can land between the check and the removal.
///
/// A deletion also takes its place in the order writes were accepted in: a
/// save accepted before it and still in flight is refused as superseded when
/// it tries to publish, so a deleted document cannot come back from a write
/// the person had already moved past. A save accepted after it creates the
/// document again, which is what saving means.
///
/// Scope, as for saves: this orders what this process does. Another program
/// replacing the file in the instant between the comparison and the removal
/// is outside what a single `unlink` can rule out, and nothing here claims to.
pub fn delete(root: &Path, relative: &str, expected_digest: &str) -> Result<(), DocumentError> {
    let normalised = normalise(root, relative)?;
    let shown = root.join(&normalised);
    let name = normalised
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !lcl_project::is_document(&name) {
        return Err(DocumentError::NotADocument(shown));
    }
    let parent = shown.parent().unwrap_or(root);
    let directory = parent.canonicalize().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => DocumentError::NotFound(shown.clone()),
        _ => DocumentError::Io {
            path: parent.to_path_buf(),
            detail: format!("the directory is not readable: {error}"),
        },
    })?;
    if directory != root && !lcl_capabilities::contains(root, &directory) {
        return Err(DocumentError::Outside(directory));
    }
    let target = directory.join(&name);
    let entry = std::fs::symlink_metadata(&target).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => DocumentError::NotFound(shown.clone()),
        _ => DocumentError::Io {
            path: target.clone(),
            detail: format!("the document is not readable: {error}"),
        },
    })?;
    if !entry.file_type().is_file() {
        return Err(DocumentError::NotAFile(shown));
    }

    let sequence = NEXT_WRITE.fetch_add(1, Ordering::SeqCst) + 1;
    let mut order = PUBLISHED.lock().unwrap_or_else(|e| e.into_inner());
    if order.get(&target).is_some_and(|newest| *newest > sequence) {
        return Err(DocumentError::Superseded(target));
    }
    let bytes = std::fs::read(&target).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => DocumentError::NotFound(shown.clone()),
        _ => DocumentError::Io {
            path: target.clone(),
            detail: format!("the document is not readable: {error}"),
        },
    })?;
    if lcl_spec::sha256::hex_digest(&bytes) != expected_digest {
        return Err(DocumentError::Changed(shown));
    }
    std::fs::remove_file(&target).map_err(|error| DocumentError::Io {
        path: target.clone(),
        detail: format!("the document could not be deleted: {error}"),
    })?;
    order.insert(target, sequence);
    Ok(())
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
    persist(root, relative, text, Publication::Replace, None)
}

/// Write one document only if the file still holds the bytes whose SHA-256 is
/// `expected`: the revision the writer started from.
///
/// For a writer that is not the only one — a paired device editing a copy
/// while the file can also change on the computer — so that a save made from
/// a stale revision is refused instead of silently replacing newer content.
/// The comparison is made inside the same critical section as the rename, so
/// no save from this process can land between the check and the replacement.
/// A file that changed reports [`DocumentError::Changed`], and one that is gone
/// [`DocumentError::NotFound`]; in both cases nothing is written.
pub fn write_expecting(
    root: &Path,
    relative: &str,
    text: &str,
    expected: &str,
) -> Result<Document, DocumentError> {
    persist(root, relative, text, Publication::Replace, Some(expected))
}

/// Publish complete bytes only if the destination is absent. A same-directory
/// hard link is atomic and cannot replace an existing file or symbolic link.
/// Filesystems without that operation fail explicitly; there is no replacing
/// rename fallback. Ordinary saving continues to use `write`.
pub fn create(root: &Path, relative: &str, text: &str) -> Result<Document, DocumentError> {
    persist(root, relative, text, Publication::Create, None)
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
    expected: Option<&str>,
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

    prepared.publish(&path, publication, sequence, expected)?;

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

/// One test at a time installs a hook: installing is a replacement, so two
/// running in parallel would each clear or overwrite the other's.
#[cfg(test)]
static ONE_HOOK_AT_A_TIME: Mutex<()> = Mutex::new(());

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
        expected: Option<&str>,
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
        // A writer that named the revision it started from replaces only that
        // revision, decided under the same lock as the rename below.
        if let Some(expected) = expected {
            let refusal = match std::fs::read(destination) {
                Ok(bytes) if lcl_spec::sha256::hex_digest(&bytes) == expected => None,
                Ok(_) => Some(DocumentError::Changed(destination.to_path_buf())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Some(DocumentError::NotFound(destination.to_path_buf()))
                }
                Err(error) => Some(DocumentError::Io {
                    path: destination.to_path_buf(),
                    detail: format!("the document is not readable: {error}"),
                }),
            };
            if let Some(error) = refusal {
                drop(order);
                return Err(self.failure(error));
            }
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
                left.publish(&target, Publication::Create, 1, None)
            });
            let b = scope.spawn(|| {
                barrier.wait();
                right.publish(&target, Publication::Create, 2, None)
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
        let _hook = ONE_HOOK_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
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
    fn a_document_is_deleted_only_with_the_digest_that_was_confirmed() {
        let directory = Directory::new();
        let root = directory.0.canonicalize().unwrap();
        let written = write(&root, "doc.lcl", "shown\n").unwrap();
        // Not the bytes that were on screen, so they are left alone.
        let other = lcl_spec::sha256::hex_digest(b"something else\n");
        assert!(matches!(
            delete(&root, "doc.lcl", &other),
            Err(DocumentError::Changed(_))
        ));
        assert_eq!(
            std::fs::read_to_string(root.join("doc.lcl")).unwrap(),
            "shown\n"
        );
        delete(&root, "doc.lcl", &written.digest).expect("the confirmed bytes are deleted");
        assert!(!root.join("doc.lcl").exists());
        assert!(matches!(
            delete(&root, "doc.lcl", &written.digest),
            Err(DocumentError::NotFound(_))
        ));
        // Both endings are documents, in a directory too.
        let text = write(&root, "sub/shared.lcl.txt", "text\n").unwrap();
        delete(&root, "sub/shared.lcl.txt", &text.digest).expect("a .lcl.txt document");
        assert!(!root.join("sub/shared.lcl.txt").exists());
        assert!(root.join("sub").is_dir(), "its directory stays");
    }

    #[test]
    fn nothing_but_a_document_file_inside_the_project_is_deleted() {
        let directory = Directory::new();
        let root = directory.0.canonicalize().unwrap();
        let digest = |bytes: &[u8]| lcl_spec::sha256::hex_digest(bytes);
        std::fs::write(root.join("notes.txt"), b"plain\n").unwrap();
        std::fs::write(root.join("lcl.project.json"), b"{}\n").unwrap();
        std::fs::create_dir(root.join("folder.lcl")).unwrap();
        std::fs::write(root.join("target.lcl"), b"kept\n").unwrap();
        std::os::unix::fs::symlink(root.join("target.lcl"), root.join("link.lcl")).unwrap();

        assert!(matches!(
            delete(&root, "notes.txt", &digest(b"plain\n")),
            Err(DocumentError::NotADocument(_))
        ));
        assert!(matches!(
            delete(&root, "lcl.project.json", &digest(b"{}\n")),
            Err(DocumentError::NotADocument(_))
        ));
        assert!(matches!(
            delete(&root, "folder.lcl", ""),
            Err(DocumentError::NotAFile(_))
        ));
        // A link named like a document is refused, and neither the link nor
        // the document it names is removed.
        assert!(matches!(
            delete(&root, "link.lcl", &digest(b"kept\n")),
            Err(DocumentError::NotAFile(_))
        ));
        for escape in ["../outside.lcl", "/etc/outside.lcl", "sub/../../x.lcl", ""] {
            assert!(
                matches!(delete(&root, escape, ""), Err(DocumentError::Outside(_))),
                "{escape:?}"
            );
        }
        assert!(root.join("notes.txt").is_file());
        assert!(root.join("lcl.project.json").is_file());
        assert!(root.join("folder.lcl").is_dir());
        assert!(std::fs::symlink_metadata(root.join("link.lcl"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_to_string(root.join("target.lcl")).unwrap(),
            "kept\n"
        );
    }

    /// A save accepted before a deletion, and still in flight when the
    /// deletion happens, cannot bring the document back.
    #[test]
    fn a_save_accepted_before_a_deletion_cannot_bring_the_document_back() {
        let _hook = ONE_HOOK_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let directory = Directory::new();
        let root = directory.0.canonicalize().unwrap();
        let original = write(&root, "gone.lcl.txt", "original\n").unwrap();

        let watched = root.join("gone.lcl.txt");
        let deleting = root.clone();
        let digest = original.digest.clone();
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = std::sync::Arc::clone(&fired);
        *BEFORE_PUBLISH.lock().unwrap() = Some(std::sync::Arc::new(move |destination: &Path| {
            if destination != watched || flag.swap(true, std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            delete(&deleting, "gone.lcl.txt", &digest)
                .expect("the deletion is accepted after the save was");
        }));
        let late = write(
            &root,
            "gone.lcl.txt",
            "a save that was already on its way\n",
        );
        *BEFORE_PUBLISH.lock().unwrap() = None;

        assert!(
            matches!(late, Err(DocumentError::Superseded(_))),
            "the stale save must be told it was superseded: {late:?}"
        );
        assert!(
            !root.join("gone.lcl.txt").exists(),
            "a save older than the deletion brought the document back"
        );
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            0,
            "with no temporary left behind"
        );
        // A save made after the deletion is a new decision, and creates it again.
        write(&root, "gone.lcl.txt", "saved again\n").expect("a later save");
        assert!(root.join("gone.lcl.txt").is_file());
    }

    #[test]
    fn a_save_from_a_stale_revision_is_refused_and_writes_nothing() {
        let directory = Directory::new();
        let root = directory.0.canonicalize().unwrap();
        let first = write(&root, "shared.lcl", "revision one\n").unwrap();
        // Someone else saves a newer revision.
        write(&root, "shared.lcl", "revision two\n").unwrap();
        let stale = write_expecting(&root, "shared.lcl", "from revision one\n", &first.digest);
        assert!(matches!(stale, Err(DocumentError::Changed(_))), "{stale:?}");
        assert_eq!(
            std::fs::read_to_string(root.join("shared.lcl")).unwrap(),
            "revision two\n",
            "a stale revision replaced newer content"
        );
        // Starting from the current revision, it saves.
        let current = lcl_spec::sha256::hex_digest(b"revision two\n");
        let saved = write_expecting(&root, "shared.lcl", "revision three\n", &current).unwrap();
        assert_eq!(
            saved.digest,
            lcl_spec::sha256::hex_digest(b"revision three\n")
        );
        // A file that is gone is not recreated by a save that expected it.
        std::fs::remove_file(root.join("shared.lcl")).unwrap();
        let gone = write_expecting(&root, "shared.lcl", "again\n", &saved.digest);
        assert!(matches!(gone, Err(DocumentError::NotFound(_))), "{gone:?}");
        assert!(!root.join("shared.lcl").exists());
        assert_eq!(
            std::fs::read_dir(&root).unwrap().count(),
            0,
            "no temporary left behind"
        );
    }

    #[test]
    fn failed_publication_cleans_its_temporary_and_preserves_existing_data() {
        let directory = Directory::new();
        let target = directory.0.join("occupied");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("user.txt"), b"preserved").unwrap();
        let prepared = Temporary::prepare(&directory.0, b"replacement").unwrap();
        assert!(prepared
            .publish(&target, Publication::Replace, 1, None)
            .is_err());
        assert_eq!(
            std::fs::read(target.join("user.txt")).unwrap(),
            b"preserved"
        );
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    }
}
