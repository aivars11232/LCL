//! The filesystem capability: a primitive interface, and one real adapter.
//!
//! ## Primitives only
//!
//! Nothing in this module knows what an LCL value is. It takes paths, bytes and
//! booleans, and it answers with paths, bytes and booleans — because the
//! implementation contract says an adapter "may not smuggle hidden semantic
//! decisions back into the runtime", and the surest way to keep that true is to
//! give it no vocabulary in which to express one.
//!
//! ## Containment is checked twice, on purpose
//!
//! [`Grants`] refuses a path outside every granted scope lexically, before
//! anything is opened. That is necessary and not sufficient: a symlink inside a
//! granted root can still point outside it, and a lexical check cannot see one.
//! [`RealFileSystem`] therefore canonicalises the path — which resolves every
//! symlink — and checks containment again against a canonicalised root before
//! it opens anything.
//!
//! For a path that does not exist yet, which is most of `core.create`, the
//! nearest existing ancestor is canonicalised instead, so a new file inside a
//! symlinked directory is judged by where that directory really is. A final
//! component that is a *dangling* link is resolved through the link, because
//! that is where the bytes would actually go.
//!
//! ## And the decision has to survive until the syscall
//!
//! Deciding where a path leads is still a decision made a moment early. Two
//! things keep it true at the operation itself: every open of a resolved path
//! passes `O_NOFOLLOW`, so a component swapped for a link after the decision is
//! refused rather than followed; and every reservation that must not replace
//! something — `core.create`, a non-overwriting copy, a non-overwriting move —
//! is made in one atomic step rather than by asking whether the target exists
//! and then writing it. A prior existence check admits every writer that passes
//! it, which is not a reservation at all.
//!
//! ## A WORKSPACE is a second boundary, not a grant
//!
//! A WORKSPACE-form PATH arrives as a [`Location`] that names its declared
//! root. `03_TYPES_AND_VALUES/04`: "Containment is checked on the resolved
//! target, not by textual prefix, so parent traversal, links, or equivalent
//! indirection cannot escape the root." That is decided on the same resolution
//! the operation opens, and before the grant: a host that grants `/` has not
//! authorized a document to leave its WORKSPACE, so the grant is never asked to
//! stand in for that decision. An escape is [`FsError::Escape`].
//!
//! What this does not claim: nothing here defends against another process
//! replacing a *directory* along the path between resolution and the operation.
//! That needs directory-relative syscalls this module does not use. The limit
//! is stated where the mechanism is, in [`NO_FOLLOW`].

use crate::bounds::{Bounds, Cancelled};
use crate::grant::{self, Grant, Grants, Refusal};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

/// One path a request addresses, and the WORKSPACE root it may not leave.
///
/// Both are host paths. `within` is present exactly for a WORKSPACE-form PATH;
/// an absolute PATH is confined by grants alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location<'a> {
    pub path: &'a Path,
    pub within: Option<&'a Path>,
}

impl<'a, P: AsRef<Path> + ?Sized> From<&'a P> for Location<'a> {
    fn from(path: &'a P) -> Location<'a> {
        Location {
            path: path.as_ref(),
            within: None,
        }
    }
}

/// What one filesystem request could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The target does not exist.
    NotFound(PathBuf),
    /// The target exists and the request required that it not.
    AlreadyExists(PathBuf),
    /// The host refuses or cannot supply the access.
    Refused(Refusal),
    /// The resolved target leaves the WORKSPACE its location names.
    Escape { path: PathBuf, resolved: PathBuf },
    /// A declared bound stopped the work.
    Bounded(Cancelled),
    /// The operating system reported a failure, before the target was opened
    /// for modification. Nothing began, and that is established rather than
    /// assumed: the open itself is what would have begun it.
    Io(String),
    /// The two ends of a transfer are one underlying file.
    ///
    /// `core.move` states it as a precondition — "resolved source and
    /// destination addresses are distinct" — and `core.copy` reaches it
    /// through "source remains unchanged", which no self-copy can honour.
    SameFile {
        source: PathBuf,
        destination: PathBuf,
    },
    /// The operating system reported a failure *after* the target was opened
    /// for modification, so what began cannot be proven not to have.
    ///
    /// A replacing write has already truncated by this point, an append has
    /// already positioned, a copy has already created its destination. How much
    /// of the intended content landed is not knowable from the error alone —
    /// `write_all` does not say how far it got — so the extent is indeterminate
    /// rather than partial, and it is certainly not none.
    IoAfterChange { detail: String, target: PathBuf },
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound(path) => write!(f, "{} does not exist", path.display()),
            FsError::AlreadyExists(path) => write!(f, "{} already exists", path.display()),
            FsError::Refused(refusal) => write!(f, "{refusal}"),
            FsError::Escape { path, resolved } => write!(
                f,
                "{} resolves to {}, outside its WORKSPACE",
                path.display(),
                resolved.display()
            ),
            FsError::Bounded(cancelled) => write!(f, "{cancelled}"),
            FsError::SameFile {
                source,
                destination,
            } => write!(
                f,
                "{} and {} are the same file",
                source.display(),
                destination.display()
            ),
            FsError::Io(detail) => f.write_str(detail),
            FsError::IoAfterChange { detail, target } => {
                write!(
                    f,
                    "{detail}, after {} was opened for writing",
                    target.display()
                )
            }
        }
    }
}

/// What one target is, structurally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub exists: bool,
    pub is_directory: bool,
    pub bytes: u64,
    /// Immediate entry names, in sorted order, for a directory.
    ///
    /// Sorted rather than enumerated, because `05_SEMANTICS/11` forbids an
    /// observable result that depends on "filesystem enumeration order".
    pub entries: Vec<String>,
}

/// Whether a write may replace existing content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    /// Fail if the target exists.
    Create,
    /// Replace the target's content, creating it when absent.
    Replace,
    /// Replace only an existing target.
    ReplaceExisting,
}

/// The primitive filesystem capability.
pub trait FileSystem {
    fn metadata(&mut self, path: Location<'_>) -> Result<Metadata, FsError>;
    fn read(&mut self, path: Location<'_>, bounds: &Bounds) -> Result<Vec<u8>, FsError>;
    fn write(&mut self, path: Location<'_>, content: &[u8], mode: WriteMode)
        -> Result<(), FsError>;
    fn append(&mut self, path: Location<'_>, content: &[u8]) -> Result<(), FsError>;
    /// Remove the target. Returns whether anything was removed.
    fn delete(&mut self, path: Location<'_>, recursive: bool) -> Result<bool, FsError>;
    fn rename(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<(), FsError>;
    /// Copy the target. Returns the number of bytes copied.
    fn copy(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<u64, FsError>;
}

/// The real filesystem, confined to granted scopes.
#[derive(Debug, Clone)]
pub struct RealFileSystem {
    grants: Grants,
}

impl RealFileSystem {
    pub fn new(grants: Grants) -> RealFileSystem {
        RealFileSystem { grants }
    }

    pub fn grants(&self) -> &Grants {
        &self.grants
    }

    /// Prove the WORKSPACE, then decide the host gate, both against real
    /// locations.
    fn admit(&self, location: Location<'_>, write: bool) -> Result<PathBuf, FsError> {
        let path = location.path;
        // The language gate first, on the resolution the operation will open.
        // A root spelled through a link is judged where it really is.
        let resolved = resolve(path);
        if let Some(root) = location.within {
            if !grant::contains(&resolve(root), &resolved) {
                return Err(FsError::Escape {
                    path: path.to_path_buf(),
                    resolved,
                });
            }
        }

        let grant = if write {
            Grant::WritePath(path.to_path_buf())
        } else {
            Grant::ReadPath(path.to_path_buf())
        };
        self.grants.decide(&grant).map_err(FsError::Refused)?;

        // Re-check the resolution. A lexical check cannot see a link that
        // leaves the scope, and this one can.
        let inside = self.grants.scopes().iter().any(|scope| {
            let root = std::fs::canonicalize(&scope.root).unwrap_or_else(|_| scope.root.clone());
            grant::contains(&root, &resolved) && (scope.writable || !write)
        });
        if !inside {
            return Err(FsError::Refused(Refusal::Denied(format!(
                "{} resolves to {}, outside every granted scope",
                path.display(),
                resolved.display()
            ))));
        }
        Ok(resolved)
    }
}

/// Whether two already-resolved locations name one underlying file.
///
/// The resolved paths answer it for a second spelling and for a symbolic link,
/// because [`resolve`] canonicalizes both before they get here. They cannot
/// answer it for a hard link: a second directory entry for one inode has a
/// canonical path of its own, and comparing path strings would call two names
/// for one file two files. So the file's own identity is asked for.
///
/// On a platform that exposes no such identity the path comparison is all
/// there is, and this says so rather than pretending otherwise: a hard link
/// would not be detected there, and a transfer between two of them would
/// behave as it did before this check existed.
#[cfg(unix)]
fn same_file(source: &Path, destination: &Path) -> bool {
    if source == destination {
        return true;
    }
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(source), std::fs::metadata(destination)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        // A destination that does not exist yet is not the source.
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_file(source: &Path, destination: &Path) -> bool {
    source == destination
}

/// The real location of a path, resolving symlinks as far as it exists.
///
/// A path that does not exist yet is judged by its nearest existing ancestor,
/// so a new file inside a symlinked directory is placed where that directory
/// really is rather than where it is spelled.
///
/// A final component that is itself a symbolic link to something that does not
/// exist is resolved through that link, not to the link's own path. The two
/// differ exactly where it matters: `canonicalize` fails on a dangling link, so
/// judging the link's own path would admit a write whose bytes the kernel then
/// delivers to wherever the link points — which is the one place the grant may
/// have been refusing.
pub fn resolve(path: &Path) -> PathBuf {
    resolve_within(path, 0)
}

/// The kernel's own symlink-chain limit, which a resolution here cannot exceed
/// and should not try to: a longer chain is `ELOOP` at the syscall too.
const MAX_LINK_DEPTH: usize = 40;

fn resolve_within(path: &Path, depth: usize) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
    }
    // A dangling final component. If it is a link, the destination it names is
    // what any operation on this path will actually reach.
    if depth < MAX_LINK_DEPTH {
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() {
                if let Ok(target) = std::fs::read_link(path) {
                    let absolute = match target.is_absolute() {
                        true => target,
                        // A relative link is relative to the directory the link
                        // lives in, not to the working directory.
                        false => match path.parent() {
                            Some(parent) => parent.join(target),
                            None => target,
                        },
                    };
                    return resolve_within(&absolute, depth + 1);
                }
            }
        }
    }
    let mut tail = Vec::new();
    let mut current = grant::normalize(path);
    loop {
        match current.file_name() {
            Some(name) => tail.push(name.to_os_string()),
            None => return grant::normalize(path),
        }
        let Some(parent) = current.parent().map(|p| p.to_path_buf()) else {
            return grant::normalize(path);
        };
        if let Ok(canonical) = std::fs::canonicalize(&parent) {
            let mut resolved = canonical;
            for segment in tail.iter().rev() {
                resolved.push(segment);
            }
            return resolved;
        }
        if parent.as_os_str().is_empty() {
            return grant::normalize(path);
        }
        current = parent;
    }
}

fn io(error: std::io::Error) -> FsError {
    FsError::Io(error.to_string())
}

/// The same failure, reported from after the target was opened for writing.
fn io_after_change(error: std::io::Error, target: &Path) -> FsError {
    FsError::IoAfterChange {
        detail: error.to_string(),
        target: target.to_path_buf(),
    }
}

/// `O_NOFOLLOW` for this target, or nothing where the value is not known.
///
/// [`resolve`] already decides *which* location a request is allowed to reach,
/// but it decides it a moment before the syscall. Between the two, another
/// process can replace the final component with a symbolic link, and an
/// ordinary open would then follow it out of the granted scope. Opening the
/// resolved path with `O_NOFOLLOW` removes that window: the resolved path's
/// final component is never a link — it was either canonicalized or it does not
/// exist — so refusing to follow one can only reject a component that changed
/// underneath the decision.
///
/// The value is part of each platform's ABI rather than of Rust's, so it is
/// stated per target. Where it is not known this is zero: the resolution and
/// containment checks still apply and every other property in this module
/// holds, but that last window is not closed, and saying so is better than a
/// guessed constant that silently opens the wrong file.
///
/// Parent components are a separate matter. Nothing here defends against
/// another process replacing a *directory* along the path between resolution
/// and the operation; that needs directory-relative syscalls this module does
/// not use. The threat model is stated plainly rather than overclaimed.
#[cfg(any(target_os = "linux", target_os = "android"))]
const NO_FOLLOW: i32 = 0o400_000;
#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
const NO_FOLLOW: i32 = 0x0100;
#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
const NO_FOLLOW: i32 = 0;

/// Apply `O_NOFOLLOW` where this platform has it.
#[cfg(unix)]
fn no_follow(options: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    match NO_FOLLOW {
        0 => options,
        flag => options.custom_flags(flag),
    }
}

#[cfg(not(unix))]
fn no_follow(options: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    options
}

impl FileSystem for RealFileSystem {
    fn metadata(&mut self, location: Location<'_>) -> Result<Metadata, FsError> {
        let resolved = self.admit(location, false)?;
        let Ok(meta) = std::fs::metadata(&resolved) else {
            return Ok(Metadata {
                exists: false,
                is_directory: false,
                bytes: 0,
                entries: Vec::new(),
            });
        };
        let mut entries = Vec::new();
        if meta.is_dir() {
            let read = std::fs::read_dir(&resolved).map_err(io)?;
            for entry in read {
                entries.push(entry.map_err(io)?.file_name().to_string_lossy().to_string());
            }
            // Enumeration order is not observable language meaning.
            entries.sort();
        }
        Ok(Metadata {
            exists: true,
            is_directory: meta.is_dir(),
            bytes: meta.len(),
            entries,
        })
    }

    /// Read the target, bounded by the bytes this process actually collects.
    ///
    /// The reported size is checked first, because refusing a file that is
    /// already known to be too large costs nothing. It is not the bound: a
    /// file's metadata and its content need not agree — most of `/proc`
    /// reports zero and yields content, and an ordinary file can grow between
    /// the two calls — so the cap is enforced again on the bytes read. A cap
    /// applied only to `stat` is a cap on the wrong number.
    fn read(&mut self, location: Location<'_>, bounds: &Bounds) -> Result<Vec<u8>, FsError> {
        let path = location.path;
        let resolved = self.admit(location, false)?;
        let meta =
            std::fs::metadata(&resolved).map_err(|_| FsError::NotFound(path.to_path_buf()))?;
        bounds
            .check(meta.len(), bounds.max_bytes, "the read")
            .map_err(FsError::Bounded)?;

        use std::io::Read;
        let file = no_follow(std::fs::OpenOptions::new().read(true))
            .open(&resolved)
            .map_err(io)?;
        // One byte past the cap is enough to know the cap was exceeded, and is
        // all that is ever retained beyond it.
        let mut collected = Vec::new();
        let limit = bounds.max_bytes.saturating_add(1);
        file.take(limit).read_to_end(&mut collected).map_err(io)?;
        bounds
            .check(collected.len() as u64, bounds.max_bytes, "the read")
            .map_err(FsError::Bounded)?;
        Ok(collected)
    }

    /// Write the target, reserving it as the operation requires.
    ///
    /// A `Create` is reserved with `create_new`, which is one atomic step: it
    /// cannot replace an existing file, cannot be raced by a second writer
    /// admitted by the same earlier check, and cannot be satisfied by an
    /// existing symbolic link — including a dangling one, which is exactly how
    /// a create was previously talked into writing outside its grant.
    fn write(
        &mut self,
        location: Location<'_>,
        content: &[u8],
        mode: WriteMode,
    ) -> Result<(), FsError> {
        let path = location.path;
        let resolved = self.admit(location, true)?;
        if matches!(mode, WriteMode::ReplaceExisting) && !resolved.exists() {
            return Err(FsError::NotFound(path.to_path_buf()));
        }
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }

        let mut options = std::fs::OpenOptions::new();
        options.write(true);
        match mode {
            WriteMode::Create => {
                options.create_new(true);
            }
            WriteMode::Replace => {
                options.create(true).truncate(true);
            }
            WriteMode::ReplaceExisting => {
                options.truncate(true);
            }
        }
        let mut file = match no_follow(&mut options).open(&resolved) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(FsError::AlreadyExists(path.to_path_buf()))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(FsError::NotFound(path.to_path_buf()))
            }
            Err(error) => return Err(io(error)),
        };
        file.write_all(content)
            .map_err(|error| io_after_change(error, &resolved))
    }

    fn append(&mut self, location: Location<'_>, content: &[u8]) -> Result<(), FsError> {
        let path = location.path;
        let resolved = self.admit(location, true)?;
        if !resolved.exists() {
            return Err(FsError::NotFound(path.to_path_buf()));
        }
        let mut file = no_follow(std::fs::OpenOptions::new().append(true))
            .open(&resolved)
            .map_err(io)?;
        file.write_all(content)
            .map_err(|error| io_after_change(error, &resolved))
    }

    fn delete(&mut self, location: Location<'_>, recursive: bool) -> Result<bool, FsError> {
        let resolved = self.admit(location, true)?;
        let Ok(meta) = std::fs::metadata(&resolved) else {
            return Ok(false);
        };
        if meta.is_dir() {
            if recursive {
                std::fs::remove_dir_all(&resolved).map_err(io)?;
            } else {
                std::fs::remove_dir(&resolved).map_err(io)?;
            }
        } else {
            std::fs::remove_file(&resolved).map_err(io)?;
        }
        Ok(true)
    }

    /// Move the target, reserving the destination as the operation requires.
    ///
    /// `rename` replaces its destination unconditionally, so a non-overwriting
    /// move cannot be built from a prior existence check: two movers that both
    /// passed the check both proceed, and the second silently destroys the
    /// first. A same-directory hard link is the reservation instead — one
    /// atomic step that fails when the destination exists — and the source is
    /// unlinked only once the destination is safely in place.
    ///
    /// A directory cannot be hard-linked. For that case the checked rename
    /// remains, and so does its window; a non-overwriting directory move is
    /// therefore reserved only as well as the check that precedes it.
    fn rename(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<(), FsError> {
        let source = self.admit(from, true)?;
        let destination = self.admit(to, true)?;
        let (from, to) = (from.path, to.path);
        if !source.exists() {
            return Err(FsError::NotFound(from.to_path_buf()));
        }
        // "resolved source and destination addresses are distinct" is
        // `core.move`'s own precondition, and it is checked before anything is
        // created or removed. POSIX `rename` on two entries for one file
        // "shall return successfully and perform no other action", so without
        // this the operation reports that it relocated something while both
        // names are still there — and `core.move` promises the opposite:
        // "source no longer exists at original address".
        if same_file(&source, &destination) {
            return Err(FsError::SameFile {
                source: from.to_path_buf(),
                destination: to.to_path_buf(),
            });
        }
        if destination.exists() && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        if overwrite || source.is_dir() {
            return std::fs::rename(&source, &destination).map_err(io);
        }
        match std::fs::hard_link(&source, &destination) {
            // The link exists from here on. If the unlink then fails, a new
            // name for this file is on disk and the old one is still there —
            // so the failure is reported from after a change, not from before
            // one. `FsError::Io` would say "Nothing began", which by now is
            // simply untrue, and a caller reading it would believe a retry
            // starts from the original state.
            Ok(()) => {
                std::fs::remove_file(&source).map_err(|error| io_after_change(error, &destination))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(FsError::AlreadyExists(to.to_path_buf()))
            }
            // A filesystem without hard links, or a cross-device move. The
            // checked rename is the remaining answer, and it is still confined.
            Err(_) => std::fs::rename(&source, &destination).map_err(io),
        }
    }

    /// Copy the target, reserving the destination as the operation requires.
    ///
    /// With overwrite off the destination is reserved with `create_new` before
    /// any byte is copied, for the same reason as [`FileSystem::write`]: a
    /// prior existence check admits every writer that passes it.
    fn copy(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<u64, FsError> {
        let source = self.admit(from, false)?;
        let destination = self.admit(to, true)?;
        let (from, to) = (from.path, to.path);
        if !source.exists() {
            return Err(FsError::NotFound(from.to_path_buf()));
        }
        if destination.exists() && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        // `core.copy` states no distinctness precondition — unlike
        // `core.move`, which does — so two names for one file are not refused
        // here. They are also not copied: `std::fs::copy` opens its
        // destination truncating, so it would empty the very file it is
        // reading and break "source remains unchanged". Nothing needs to
        // happen instead. The destination already holds the source's
        // pre-state, and the source is already unchanged, so both
        // postconditions hold with no bytes moved.
        if same_file(&source, &destination) {
            return Ok(0);
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        if overwrite {
            // Staged exactly like the reserving path below, so that each
            // failure is reported from the phase it actually happened in.
            // `std::fs::copy` does this internally and then returns one error
            // for all of it, which made a failure after the destination had
            // been truncated indistinguishable from one before it was opened.
            let mut reader = no_follow(std::fs::OpenOptions::new().read(true))
                .open(&source)
                .map_err(io)?;
            let mut writer = no_follow(
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true),
            )
            .open(&destination)
            .map_err(io)?;
            // The destination is truncated by the open above, so anything that
            // goes wrong from here has already changed it.
            return std::io::copy(&mut reader, &mut writer)
                .map_err(|error| io_after_change(error, &destination));
        }
        let mut reader = no_follow(std::fs::OpenOptions::new().read(true))
            .open(&source)
            .map_err(io)?;
        let mut writer = match no_follow(std::fs::OpenOptions::new().write(true).create_new(true))
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(FsError::AlreadyExists(to.to_path_buf()))
            }
            Err(error) => return Err(io(error)),
        };
        std::io::copy(&mut reader, &mut writer)
            .map_err(|error| io_after_change(error, &destination))
    }
}
