//! The open workspace: one project root, one engine, and the documents in it.
//!
//! ## Listing files is not resolving them
//!
//! This module walks a project directory to fill a file tree. That is a file
//! browser, and it is worth being explicit that it is not source resolution,
//! because `05_SEMANTICS/02` is emphatic that "ambient current directory and
//! implied nearby files do not exist in portable LCL".
//!
//! Nothing found by this walk enters a program. When a document is checked or
//! run, the units that load are exactly the ones that document named, through
//! `lcl_project::FileProvider`, and the report says which those were. A file
//! sitting in the tree that nothing imports is a file the engine never reads.
//!
//! ## One engine, opened once
//!
//! Assembling an engine verifies the specification package against its external
//! trust anchor and loads seven layers' contracts. A request holds no state, so
//! one engine serves every request, and every request is judged against a
//! package whose identity digest the reply carries.
//!
//! A workspace that names a Core 0.2.0 package also holds the localized
//! engine, and each document is judged by exactly one of the two.

use crate::document::{self, Document, DocumentError};
use lcl_project::{Project, ProjectError, MANIFEST_FILE};
use lcl_protocol::{Engine, Engines};
use lcl_resolver::SourceUnit;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// How deep the file walk goes. A project is a source tree, not a filesystem.
pub const MAX_DEPTH: usize = 12;
/// How many entries the tree reports at most: documents, and the folders on
/// the way to them. Folders without a document never count.
pub const MAX_ENTRIES: usize = 4096;
/// How many filesystem entries one listing examines at most, shown or not,
/// so a project full of other files cannot make a listing unbounded.
pub const MAX_SCANNED: usize = 100_000;
/// How deep the walk looks, past [`MAX_DEPTH`], for a document it cannot show.
const PROBE_DEPTH: usize = 64;
/// The largest document whose `KIND` the tree reads.
const KIND_LIMIT: u64 = 1 << 20;
/// How old a file's timestamps must be before its cached `KIND` is trusted.
/// A filesystem stamps a write with a coarse clock (FAT to two seconds), so a
/// rewrite of the same length inside one tick can leave every timestamp as it
/// was. Past this margin a later write cannot share the old stamp.
const SETTLED: Duration = Duration::from_secs(3);

/// Why a workspace could not be opened.
#[derive(Debug)]
pub enum WorkspaceError {
    /// The specification package could not be found or would not verify.
    Spec(String),
    Project(ProjectError),
    Document(DocumentError),
    Io {
        path: PathBuf,
        detail: String,
    },
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::Spec(detail) => write!(f, "{detail}"),
            WorkspaceError::Project(inner) => write!(f, "{inner}"),
            WorkspaceError::Document(inner) => write!(f, "{inner}"),
            WorkspaceError::Io { path, detail } => write!(f, "{}: {detail}", path.display()),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl From<DocumentError> for WorkspaceError {
    fn from(inner: DocumentError) -> WorkspaceError {
        WorkspaceError::Document(inner)
    }
}

/// One entry in the project tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Root-relative path, with `/` separators on every platform.
    pub id: String,
    pub directory: bool,
    /// Byte length, for a file.
    pub bytes: Option<u64>,
}

/// The tree's entries and whether the walk left eligible ones out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// True only when at least one more document or folder existed than the
    /// limits let the tree report: more than [`MAX_ENTRIES`], or one below
    /// [`MAX_DEPTH`]. A listing of exactly [`MAX_ENTRIES`] can be complete.
    pub truncated: bool,
}

/// What a file's metadata says about its bytes without reading them: length,
/// modification time and, on Unix, the inode and its change time, which an
/// in-place rewrite or a replacing rename moves even when it keeps the length
/// and the modification time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
    #[cfg(unix)]
    node: (u64, u64, i64, i64),
}

impl Stamp {
    fn of(metadata: &std::fs::Metadata) -> Option<Stamp> {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Some(Stamp {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
            #[cfg(unix)]
            node: (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }

    /// Whether every timestamp is old enough that no later write can repeat
    /// this stamp. A stamp that is not is never cached, so the file is read
    /// again on the next listing.
    fn settled(&self) -> bool {
        let now = SystemTime::now();
        let old = |at: SystemTime| now.duration_since(at).is_ok_and(|age| age >= SETTLED);
        #[cfg(unix)]
        {
            let changed = SystemTime::UNIX_EPOCH
                .checked_add(Duration::new(
                    u64::try_from(self.node.2).unwrap_or(0),
                    u32::try_from(self.node.3).unwrap_or(0),
                ))
                .unwrap_or(now);
            if !old(changed) {
                return false;
            }
        }
        old(self.modified)
    }
}

/// One open project, with the engine that judges it.
pub struct Workspace {
    project: Project,
    engines: Engines,
    spec_root: PathBuf,
    /// The Core 0.2.0 package, when this workspace judges localized documents.
    localized_spec_root: Option<PathBuf>,
    /// The Core 0.3.0 package, when this workspace judges 0.3.0 documents and
    /// projects.
    project_spec_root: Option<PathBuf>,
    /// The document the frontend should open on load, when the workspace was
    /// launched for one. A file association supplies it; an ordinary launch
    /// does not.
    open_document: Option<String>,
    /// The `KIND` last read from each listed document, with the stamp it was
    /// read at. Tree labels only: checking, running and resolving always read
    /// the file.
    kinds: Mutex<HashMap<String, (Stamp, Option<String>)>>,
    /// How many times the tree has read a document for its `KIND`.
    kind_reads: AtomicU64,
}

impl Workspace {
    /// Open the project at `root`, judging it against `spec`.
    ///
    /// A directory holding a manifest opens as a project; one without opens
    /// rootless, which keeps containment and root-relative identity and gives
    /// up only the manifest's declared spec, entry, cache and lock. Both are
    /// real states, and an editor should not demand a manifest before it will
    /// open a folder of documents.
    pub fn open(
        root: impl AsRef<Path>,
        spec: impl AsRef<Path>,
    ) -> Result<Workspace, WorkspaceError> {
        Workspace::open_with(root, spec, None)
    }

    /// Open the project at `root`, judging each document by the engine
    /// [`Engines::engine_for`] chooses for it.
    ///
    /// `localized` names the Core 0.2.0 package. Without it the manifest's
    /// `localized_spec` applies, and without either every document is judged
    /// by Core 0.1.0 alone. The localized engine's locale profiles are the
    /// files in the manifest's profile directory.
    pub fn open_with(
        root: impl AsRef<Path>,
        spec: impl AsRef<Path>,
        localized: Option<PathBuf>,
    ) -> Result<Workspace, WorkspaceError> {
        Workspace::open_with_profiles(root, spec, localized, &[])
    }

    /// [`Workspace::open_with`], with locale profile files added after the
    /// manifest's profile directory, a later file for the same locale
    /// replacing an earlier one. This is the command line's `--profile` rule,
    /// so equal configuration judges a document the same way in both.
    pub fn open_with_profiles(
        root: impl AsRef<Path>,
        spec: impl AsRef<Path>,
        localized: Option<PathBuf>,
        profiles: &[PathBuf],
    ) -> Result<Workspace, WorkspaceError> {
        Workspace::open_with_specs(root, spec, localized, None, profiles)
    }

    /// [`Workspace::open_with_profiles`], with the Core 0.3.0 package too.
    ///
    /// `project` names the Core 0.3.0 package. Without it the manifest's
    /// `project_spec` applies, and without either a document declaring 0.3.0
    /// is judged by Core 0.1.0 and refused as an unsupported version — the
    /// command line's rule. The 0.3.0 engine's locale profiles are the same
    /// files the 0.2.0 engine's are.
    pub fn open_with_specs(
        root: impl AsRef<Path>,
        spec: impl AsRef<Path>,
        localized: Option<PathBuf>,
        project_spec: Option<PathBuf>,
        profiles: &[PathBuf],
    ) -> Result<Workspace, WorkspaceError> {
        let root = root.as_ref();
        let project = match Project::open(root) {
            Ok(project) => project,
            Err(ProjectError::NoManifest(_)) => {
                Project::rootless(root).map_err(WorkspaceError::Project)?
            }
            Err(other) => return Err(WorkspaceError::Project(other)),
        };
        let spec_root = spec.as_ref().to_path_buf();
        let core = Engine::open(&spec_root)
            .map_err(|e| WorkspaceError::Spec(format!("the specification package: {e}")))?;
        let localized_spec_root = localized.or_else(|| project.localized_spec_path());
        let localized = match &localized_spec_root {
            Some(localized_root) => {
                let mut files = project.profile_files().map_err(|e| {
                    WorkspaceError::Spec(format!("the locale profile directory: {e}"))
                })?;
                files.extend(profiles.iter().cloned());
                let engine = Engine::open_localized(localized_root, &files).map_err(|e| {
                    WorkspaceError::Spec(format!("the localized specification package: {e}"))
                })?;
                Some(engine)
            }
            // A profile file is used only by the localization stage; without
            // a localized package it would be silently ignored.
            None if !profiles.is_empty()
                && project_spec.is_none()
                && project.project_spec_path().is_none() =>
            {
                return Err(WorkspaceError::Spec(
                    "a locale profile file needs a localized specification package: pass \
                     --localized-spec <path>, set LCL_LOCALIZED_SPEC, or declare \
                     \"localized_spec\" in lcl.project.json"
                        .to_string(),
                ))
            }
            None => None,
        };
        let mut engines =
            Engines::new(core, localized).map_err(|e| WorkspaceError::Spec(e.to_string()))?;
        let project_spec_root = project_spec.or_else(|| project.project_spec_path());
        if let Some(project_root) = &project_spec_root {
            let mut files = project
                .profile_files()
                .map_err(|e| WorkspaceError::Spec(format!("the locale profile directory: {e}")))?;
            files.extend(profiles.iter().cloned());
            let engine = Engine::open_project(project_root, &files).map_err(|e| {
                WorkspaceError::Spec(format!("the Core 0.3.0 specification package: {e}"))
            })?;
            engines = engines
                .with_project(engine)
                .map_err(|e| WorkspaceError::Spec(e.to_string()))?;
        }
        Ok(Workspace {
            project,
            engines,
            spec_root,
            localized_spec_root,
            project_spec_root,
            open_document: None,
            kinds: Mutex::new(HashMap::new()),
            kind_reads: AtomicU64::new(0),
        })
    }

    /// Where the specification package this workspace judges against lives.
    ///
    /// Resolution order, which is the CLI's: the caller's explicit choice, then
    /// `LCL_SPEC`, then the project manifest. A workspace that found none stops
    /// and says so rather than searching the filesystem for a package, because
    /// a package discovered by guessing is a trust anchor nobody chose.
    pub fn locate_spec(root: &Path, explicit: Option<PathBuf>) -> Result<PathBuf, WorkspaceError> {
        if let Some(path) = explicit {
            return Ok(path);
        }
        if let Some(value) = std::env::var_os("LCL_SPEC") {
            return Ok(PathBuf::from(value));
        }
        if let Ok(project) = Project::open(root) {
            if let Some(path) = project.spec_path() {
                return Ok(path);
            }
        }
        Err(WorkspaceError::Spec(format!(
            "no specification package: pass one, set LCL_SPEC, or declare \"spec\" in {MANIFEST_FILE}"
        )))
    }

    /// The Core 0.2.0 package a launch names: the caller's explicit choice,
    /// then a non-empty `LCL_LOCALIZED_SPEC`. `None` leaves the choice to the
    /// manifest's `localized_spec`, in [`Workspace::open_with`].
    pub fn locate_localized_spec(explicit: Option<PathBuf>) -> Option<PathBuf> {
        explicit.or_else(|| {
            std::env::var_os("LCL_LOCALIZED_SPEC")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
    }

    /// The Core 0.3.0 package a launch names: the caller's explicit choice,
    /// then a non-empty `LCL_PROJECT_SPEC`. `None` leaves the choice to the
    /// manifest's `project_spec`, in [`Workspace::open_with_specs`].
    pub fn locate_project_spec(explicit: Option<PathBuf>) -> Option<PathBuf> {
        explicit.or_else(|| {
            std::env::var_os("LCL_PROJECT_SPEC")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        })
    }

    /// The project root and root-relative identity of one document.
    ///
    /// A desktop file association hands over a document, not a project, and
    /// the workspace opens projects. The rule is [`lcl_project::locate_document`],
    /// the same one the command line applies, so a document is judged under
    /// the same root and manifest whichever product opened it.
    pub fn locate_document(document: &Path) -> Result<(PathBuf, String), WorkspaceError> {
        lcl_project::locate_document(document).map_err(|e| WorkspaceError::Io {
            path: e.path,
            detail: e.detail,
        })
    }

    /// Record the document a launch asked for, so the frontend can open it.
    pub fn with_open_document(mut self, id: Option<String>) -> Workspace {
        self.open_document = id;
        self
    }

    /// The document this workspace was launched for, if it was launched for one.
    pub fn open_document(&self) -> Option<&str> {
        self.open_document.as_deref()
    }

    /// Create a project directory holding a manifest, and open it.
    ///
    /// Refuses to overwrite an existing manifest: a project already there is
    /// the user's, and the honest answer is to open it rather than replace it.
    pub fn create(
        root: impl AsRef<Path>,
        spec: impl AsRef<Path>,
    ) -> Result<Workspace, WorkspaceError> {
        let root = root.as_ref();
        std::fs::create_dir_all(root).map_err(|e| WorkspaceError::Io {
            path: root.to_path_buf(),
            detail: format!("the project directory could not be created: {e}"),
        })?;
        let manifest_path = root.join(MANIFEST_FILE);
        if manifest_path.exists() {
            return Err(WorkspaceError::Io {
                path: manifest_path,
                detail: "a project already exists here; open it instead".to_string(),
            });
        }
        let spec = spec.as_ref();
        let manifest = format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {}\n}}\n",
            json_string(&spec.display().to_string())
        );
        std::fs::write(&manifest_path, manifest).map_err(|e| WorkspaceError::Io {
            path: manifest_path,
            detail: format!("the project manifest could not be written: {e}"),
        })?;
        Workspace::open(root, spec)
    }

    pub fn root(&self) -> &Path {
        self.project.root()
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// The Core 0.1.0 engine.
    pub fn engine(&self) -> &Engine {
        self.engines.core()
    }

    pub fn engines(&self) -> &Engines {
        &self.engines
    }

    /// The engine that judges one document's bytes.
    pub fn engine_for(&self, unit: &SourceUnit) -> &Engine {
        self.engines.engine_for(unit)
    }

    pub fn spec_root(&self) -> &Path {
        &self.spec_root
    }

    /// Where the Core 0.2.0 package is, when this workspace uses one.
    pub fn project_spec_root(&self) -> Option<&Path> {
        self.project_spec_root.as_deref()
    }

    /// The Core 0.3.0 engine, which owns project roles and scaffolds, when
    /// this workspace has one.
    pub fn project_engine(&self) -> Option<&Engine> {
        self.engines.project()
    }

    pub fn localized_spec_root(&self) -> Option<&Path> {
        self.localized_spec_root.as_deref()
    }

    /// The document a bare command acts on, when the manifest declares one.
    pub fn entry(&self) -> Option<String> {
        let path = self.project.entry_path()?;
        let root = self.project.root();
        let relative = path.strip_prefix(root).ok()?;
        Some(to_identity(relative))
    }

    /// Every LCL document in the project, and every folder on the way to
    /// one, in ascending identity order.
    ///
    /// The tree navigates documents; it is not a file browser. A folder is
    /// listed only when a document is somewhere below it, so folders of
    /// outputs, logs or data, and empty ones, are left out whatever they are
    /// called, and a `docs/rules.lcl` keeps `docs/`. [`MAX_ENTRIES`] counts
    /// only what is listed; [`MAX_SCANNED`] bounds the filesystem entries
    /// examined, listed or not. Dot directories are tooling and never walked.
    ///
    /// Both recognised suffixes, `.lcl` and `.lcl.txt`. The test is on the file
    /// name rather than on `Path::extension`, because the extension of
    /// `notes.lcl.txt` is `txt` and a listing built on that would show neither
    /// the new default nor anything else useful. Ordinary `.txt` files are not
    /// listed: only the exact `.lcl.txt` ending is recognised.
    ///
    /// Deterministic: the walk sorts each directory's entries by name rather
    /// than taking the filesystem's enumeration order, which contract 5.3
    /// names as something observable meaning must never depend on. A file tree
    /// is not language meaning, but a list that reordered itself between two
    /// reads would still be a bug a user sees.
    pub fn documents(&self) -> Result<Vec<Entry>, WorkspaceError> {
        Ok(self.listing()?.entries)
    }

    /// The same listing, saying whether the limits left anything out.
    pub fn listing(&self) -> Result<Listing, WorkspaceError> {
        let mut listing = Listing {
            entries: Vec::new(),
            truncated: false,
        };
        let root = self.project.root();
        Walk {
            root,
            out: &mut listing,
            scanned: 0,
        }
        .walk(root, 0)?;
        listing.entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(listing)
    }

    /// The `SPECIFICATION` `KIND` the tree shows beside each entry, in order:
    /// `None` for a folder and for a document that cannot be read, does not
    /// parse, or is over 1 MiB.
    ///
    /// A document whose stamp is unchanged since it was last read is not read
    /// again. Afterwards the cache holds only the documents listed here, so a
    /// deleted one is forgotten.
    pub fn tree_kinds(&self, entries: &[Entry]) -> Vec<Option<String>> {
        let kinds = entries.iter().map(|e| self.tree_kind(e)).collect();
        let listed: HashSet<&str> = entries
            .iter()
            .filter(|e| !e.directory)
            .map(|e| e.id.as_str())
            .collect();
        self.cached_kinds()
            .retain(|id, _| listed.contains(id.as_str()));
        kinds
    }

    /// How many times [`Workspace::tree_kinds`] has read a document rather
    /// than reuse what it read before, and how many documents it holds now.
    #[doc(hidden)]
    pub fn tree_kind_stats(&self) -> (u64, usize) {
        (
            self.kind_reads.load(Ordering::Relaxed),
            self.cached_kinds().len(),
        )
    }

    fn tree_kind(&self, entry: &Entry) -> Option<String> {
        if entry.directory {
            return None;
        }
        // The stamp is taken before the read: a write in between leaves a
        // stamp older than the bytes, which only costs one more read later.
        let metadata = document::resolve(self.project.root(), &entry.id)
            .ok()
            .and_then(|path| std::fs::metadata(path).ok());
        let Some(metadata) = metadata.filter(|m| m.is_file() && m.len() <= KIND_LIMIT) else {
            self.cached_kinds().remove(&entry.id);
            return None;
        };
        let stamp = Stamp::of(&metadata);
        if let Some(stamp) = stamp {
            if let Some((cached, kind)) = self.cached_kinds().get(&entry.id) {
                if *cached == stamp {
                    return kind.clone();
                }
            }
        }
        self.kind_reads.fetch_add(1, Ordering::Relaxed);
        let kind = self
            .read(&entry.id)
            .ok()
            .and_then(|d| crate::authoring::declared_kind(self, &entry.id, &d.text));
        let mut cache = self.cached_kinds();
        match stamp.filter(Stamp::settled) {
            Some(stamp) => {
                cache.insert(entry.id.clone(), (stamp, kind.clone()));
            }
            None => {
                cache.remove(&entry.id);
            }
        }
        kind
    }

    fn cached_kinds(&self) -> std::sync::MutexGuard<'_, HashMap<String, (Stamp, Option<String>)>> {
        self.kinds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Read one document.
    pub fn read(&self, id: &str) -> Result<Document, WorkspaceError> {
        Ok(document::read(self.project.root(), id)?)
    }

    /// Save one document.
    ///
    /// Exactly the bytes and exactly the name given. Saving never renames what
    /// it opened, so a `.lcl` document stays a `.lcl` document forever.
    pub fn save(&self, id: &str, text: &str) -> Result<Document, WorkspaceError> {
        Ok(document::write(self.project.root(), id, text)?)
    }

    /// Save one document only if its file still holds the revision whose
    /// SHA-256 is `expected`. See [`document::write_expecting`].
    pub fn save_expecting(
        &self,
        id: &str,
        text: &str,
        expected: &str,
    ) -> Result<Document, WorkspaceError> {
        Ok(document::write_expecting(
            self.project.root(),
            id,
            text,
            expected,
        )?)
    }

    /// Atomically create a document without replacing an occupied destination.
    pub fn create_document(&self, id: &str, text: &str) -> Result<Document, WorkspaceError> {
        Ok(document::create(self.project.root(), id, text)?)
    }

    /// Delete one document, if its bytes are still the ones the caller
    /// confirmed. See [`document::delete`] for everything this refuses.
    pub fn delete(&self, id: &str, expected_digest: &str) -> Result<(), WorkspaceError> {
        Ok(document::delete(self.project.root(), id, expected_digest)?)
    }

    /// Whether one document exists at this instant. Creation must still use
    /// `create_document`: this observation cannot reserve a destination.
    pub fn exists(&self, id: &str) -> bool {
        document::resolve(self.project.root(), id)
            .map(|path| path.is_file())
            .unwrap_or(false)
    }
}

/// The walk for one listing: where it started, what it found, and how many
/// filesystem entries it has examined.
struct Walk<'a> {
    root: &'a Path,
    out: &'a mut Listing,
    scanned: usize,
}

impl Walk<'_> {
    /// The children of one directory, in name order; `None` once the scan
    /// budget is spent, which leaves the listing truncated.
    fn children(&mut self, directory: &Path) -> Result<Option<Vec<PathBuf>>, WorkspaceError> {
        let listing = std::fs::read_dir(directory).map_err(|e| WorkspaceError::Io {
            path: directory.to_path_buf(),
            detail: format!("the directory is not readable: {e}"),
        })?;
        let mut children = Vec::new();
        for entry in listing.filter_map(Result::ok) {
            self.scanned += 1;
            if self.scanned > MAX_SCANNED {
                self.out.truncated = true;
                return Ok(None);
            }
            children.push(entry.path());
        }
        children.sort();
        Ok(Some(children))
    }

    /// Append the documents below `directory` and the folders that lead to
    /// them; true when there was at least one. A folder is appended before
    /// its contents and taken back when nothing below it was, so a folder of
    /// other files, or an empty one, is never listed. Marks the listing
    /// truncated when a document is left out, at the entry limit or below
    /// the depth limit, and only then.
    fn walk(&mut self, directory: &Path, depth: usize) -> Result<bool, WorkspaceError> {
        if depth > MAX_DEPTH {
            if self.holds_document(directory, depth)? {
                self.out.truncated = true;
            }
            return Ok(false);
        }
        let Some(children) = self.children(directory)? else {
            return Ok(false);
        };
        let mut found = false;
        for path in children {
            if self.out.truncated {
                break;
            }
            let Some((relative, is_dir)) = eligible(self.root, &path) else {
                continue;
            };
            let full = self.out.entries.len() == MAX_ENTRIES;
            if is_dir {
                if full {
                    // No room even for the folder: whether the listing is
                    // incomplete depends on what is inside it.
                    if self.holds_document(&path, depth + 1)? {
                        self.out.truncated = true;
                    }
                    continue;
                }
                let mark = self.out.entries.len();
                self.out.entries.push(Entry {
                    id: to_identity(&relative),
                    directory: true,
                    bytes: None,
                });
                if self.walk(&path, depth + 1)? {
                    found = true;
                } else {
                    self.out.entries.truncate(mark);
                }
            } else if full {
                self.out.truncated = true;
            } else {
                found = true;
                self.out.entries.push(Entry {
                    id: to_identity(&relative),
                    directory: false,
                    bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
                });
            }
        }
        Ok(found)
    }

    /// Whether a document is anywhere below `directory`, which is `depth`
    /// below the root. A spent scan budget, or a depth past any real source
    /// tree (a link looping back up), counts as yes: it cannot rule one out.
    fn holds_document(&mut self, directory: &Path, depth: usize) -> Result<bool, WorkspaceError> {
        if depth > PROBE_DEPTH {
            return Ok(true);
        }
        let Some(children) = self.children(directory)? else {
            return Ok(true);
        };
        for path in children {
            match eligible(self.root, &path) {
                Some((_, false)) => return Ok(true),
                Some((_, true)) if self.holds_document(&path, depth + 1)? => return Ok(true),
                _ if self.out.truncated => return Ok(true),
                _ => {}
            }
        }
        Ok(false)
    }
}

/// Whether the tree lists this path, and as a folder or a document: its
/// root-relative path when it does.
fn eligible(root: &Path, path: &Path) -> Option<(PathBuf, bool)> {
    let relative = path.strip_prefix(root).ok()?.to_path_buf();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    // A dot directory is tooling, not source. The package cache is one.
    if name.starts_with('.') {
        return None;
    }
    // A link is listed only when what it names is inside the project, which
    // is also what opening it requires; the walk never enumerates through
    // one that leaves.
    let metadata = std::fs::symlink_metadata(path).ok()?;
    let is_dir = if metadata.file_type().is_symlink() {
        match path.canonicalize() {
            Ok(target) if lcl_capabilities::contains(root, &target) => target.is_dir(),
            _ => return None,
        }
    } else {
        metadata.is_dir()
    };
    (is_dir || lcl_project::is_document(&name)).then_some((relative, is_dir))
}

/// A root-relative path as one identity string, `/`-separated.
fn to_identity(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// One JSON string literal, for the manifest this module writes.
fn json_string(raw: &str) -> String {
    format!("\"{}\"", crate::http::escape_json(raw))
}
