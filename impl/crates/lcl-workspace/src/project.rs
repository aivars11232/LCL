//! The open workspace: one project root, one engine, and the documents in it.
//!
//! ## Listing files is not resolving them
//!
//! This module lists one folder of a project at a time to fill the project
//! explorer. That is a file browser, and it is worth being explicit that it is
//! not source resolution, because `05_SEMANTICS/02` is emphatic that "ambient
//! current directory and implied nearby files do not exist in portable LCL".
//!
//! Nothing a listing finds enters a program. When a document is checked or
//! run, the units that load are exactly the ones that document named, through
//! `lcl_project::FileProvider`, and the report says which those were. A file
//! sitting in the tree that nothing imports is a file the engine never reads.
//!
//! ## One folder at a time
//!
//! The explorer is lazy: [`Workspace::children`] reads the direct children of
//! the one folder asked for, and nothing below them. A folder that is never
//! unfolded is never read, so the size of a project, or of anything else that
//! happens to be inside it, does not decide how long the sidebar takes. Every
//! folder inside the project is listed, empty or not: the explorer shows the
//! project as it is on disk, and a person may make a folder before its files.
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

/// How many entries one folder's listing reports at most. The bound is per
/// folder, because a listing is: one folder holding more says so, and every
/// other folder of the project is listed in full.
pub const MAX_CHILDREN: usize = 4096;
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
    /// The last part of the path: what the explorer shows.
    pub name: String,
    pub directory: bool,
    /// Byte length, for a file.
    pub bytes: Option<u64>,
}

/// The direct children of one folder, and whether the bound left any out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Children {
    /// The folder listed: a root-relative identity, or `""` for the root.
    pub parent: String,
    /// Folders first, then documents, each in name order.
    pub entries: Vec<Entry>,
    /// True only when this folder held more listable entries than
    /// [`MAX_CHILDREN`]; the first [`MAX_CHILDREN`] are in `entries`. A
    /// folder of exactly [`MAX_CHILDREN`] is complete.
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

    /// The folder `parent` names, proven inside the project: the root for
    /// `""`. A folder that does not exist, is not a folder, or is a dot
    /// directory (tooling, never listed) is refused.
    fn folder(&self, parent: &str) -> Result<PathBuf, WorkspaceError> {
        let root = self.project.root();
        if parent.is_empty() {
            return Ok(root.to_path_buf());
        }
        let path = document::resolve(root, parent)?;
        let hidden = path
            .strip_prefix(root)
            .ok()
            .into_iter()
            .flat_map(Path::components)
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'));
        if hidden || !path.is_dir() {
            return Err(WorkspaceError::Io {
                path,
                detail: "there is no such folder in the project".to_string(),
            });
        }
        Ok(path)
    }

    /// The direct children of one folder of the project: every folder in it,
    /// empty or not, and every LCL document, and nothing below them.
    ///
    /// Both recognised suffixes, `.lcl` and `.lcl.txt`. The test is on the file
    /// name rather than on `Path::extension`, because the extension of
    /// `notes.lcl.txt` is `txt` and a listing built on that would show neither
    /// the new default nor anything else useful. Ordinary `.txt` files are not
    /// listed: only the exact `.lcl.txt` ending is recognised. A dot directory
    /// is tooling, not source, and is not listed; a link is listed only when
    /// what it names is inside the project.
    ///
    /// Deterministic: folders first, then documents, each sorted by name
    /// rather than taken in the filesystem's enumeration order, which
    /// contract 5.3 names as something observable meaning must never depend
    /// on. A file tree is not language meaning, but a list that reordered
    /// itself between two reads would still be a bug a person sees. At most
    /// [`MAX_CHILDREN`] entries, the first in that order; `truncated` says
    /// when there were more.
    pub fn children(&self, parent: &str) -> Result<Children, WorkspaceError> {
        let root = self.project.root();
        let directory = self.folder(parent)?;
        let listing = std::fs::read_dir(&directory).map_err(|e| WorkspaceError::Io {
            path: directory.clone(),
            detail: format!("the folder is not readable: {e}"),
        })?;
        let mut folders = Vec::new();
        let mut documents = Vec::new();
        for entry in listing.filter_map(Result::ok) {
            let path = entry.path();
            let Some((name, is_dir)) = eligible(root, &path) else {
                continue;
            };
            let id = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            if is_dir {
                folders.push(Entry {
                    id,
                    name,
                    directory: true,
                    bytes: None,
                });
            } else {
                documents.push(Entry {
                    id,
                    name,
                    directory: false,
                    bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
                });
            }
        }
        folders.sort_by(|a, b| a.name.cmp(&b.name));
        documents.sort_by(|a, b| a.name.cmp(&b.name));
        let mut entries = folders;
        entries.append(&mut documents);
        let truncated = entries.len() > MAX_CHILDREN;
        entries.truncate(MAX_CHILDREN);
        Ok(Children {
            parent: parent.to_string(),
            entries,
            truncated,
        })
    }

    /// Make one folder in the project, empty, where the explorer can show it
    /// at once. `id` is root-relative; its parent must exist, so a folder is
    /// made where a person chose, never along a path nobody looked at. A dot
    /// name is refused (the explorer never lists one), and so is a name that
    /// is taken.
    pub fn create_folder(&self, id: &str) -> Result<String, WorkspaceError> {
        let root = self.project.root();
        let path = document::resolve(root, id)?;
        let relative = path.strip_prefix(root).map_err(|_| WorkspaceError::Io {
            path: path.clone(),
            detail: "the folder would be outside the project".to_string(),
        })?;
        if relative
            .components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
        {
            return Err(WorkspaceError::Io {
                path: path.clone(),
                detail: "a folder name cannot start with a dot".to_string(),
            });
        }
        if std::fs::symlink_metadata(&path).is_ok() {
            return Err(WorkspaceError::Document(DocumentError::AlreadyExists(path)));
        }
        std::fs::create_dir(&path).map_err(|e| WorkspaceError::Io {
            path: path.clone(),
            detail: format!("the folder could not be created: {e}"),
        })?;
        Ok(to_identity(relative))
    }

    /// The `SPECIFICATION` `KIND` the tree shows beside each entry of one
    /// folder's listing, in order: `None` for a folder and for a document
    /// that cannot be read, does not parse, or is over 1 MiB. Only the
    /// documents listed are read; nothing below the folder is.
    ///
    /// A document whose stamp is unchanged since it was last read is not read
    /// again. Afterwards the cache holds, for this folder, only the documents
    /// listed here, so a deleted one is forgotten; other folders' entries are
    /// kept until they are listed again.
    pub fn tree_kinds(&self, children: &Children) -> Vec<Option<String>> {
        let kinds = children.entries.iter().map(|e| self.tree_kind(e)).collect();
        let listed: HashSet<&str> = children
            .entries
            .iter()
            .filter(|e| !e.directory)
            .map(|e| e.id.as_str())
            .collect();
        let parent = children.parent.as_str();
        self.cached_kinds().retain(|id, _| {
            let in_folder = match id.rsplit_once('/') {
                Some((folder, _)) => folder == parent,
                None => parent.is_empty(),
            };
            !in_folder || listed.contains(id.as_str())
        });
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

/// Whether the explorer lists this path, and as a folder or a document: its
/// name when it does.
fn eligible(root: &Path, path: &Path) -> Option<(String, bool)> {
    let name = path.file_name()?.to_string_lossy().to_string();
    // A dot directory is tooling, not source. The package cache is one.
    if name.starts_with('.') {
        return None;
    }
    // A link is listed only when what it names is inside the project, which
    // is also what opening it requires; the explorer never looks through one
    // that leaves.
    let metadata = std::fs::symlink_metadata(path).ok()?;
    let is_dir = if metadata.file_type().is_symlink() {
        match path.canonicalize() {
            Ok(target) if lcl_capabilities::contains(root, &target) => target.is_dir(),
            _ => return None,
        }
    } else {
        metadata.is_dir()
    };
    (is_dir || lcl_project::is_document(&name)).then_some((name, is_dir))
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
