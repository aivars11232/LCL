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

use crate::document::{self, Document, DocumentError};
use lcl_project::{Project, ProjectError, MANIFEST_FILE};
use lcl_protocol::Engine;
use std::path::{Path, PathBuf};

/// How deep the file walk goes. A project is a source tree, not a filesystem.
const MAX_DEPTH: usize = 12;
/// How many documents the tree reports at most.
const MAX_ENTRIES: usize = 4096;

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

/// One open project, with the engine that judges it.
pub struct Workspace {
    project: Project,
    engine: Engine,
    spec_root: PathBuf,
    /// The document the frontend should open on load, when the workspace was
    /// launched for one. A file association supplies it; an ordinary launch
    /// does not.
    open_document: Option<String>,
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
        let root = root.as_ref();
        let project = match Project::open(root) {
            Ok(project) => project,
            Err(ProjectError::NoManifest(_)) => {
                Project::rootless(root).map_err(WorkspaceError::Project)?
            }
            Err(other) => return Err(WorkspaceError::Project(other)),
        };
        let spec_root = spec.as_ref().to_path_buf();
        let engine = Engine::open(&spec_root)
            .map_err(|e| WorkspaceError::Spec(format!("the specification package: {e}")))?;
        Ok(Workspace {
            project,
            engine,
            spec_root,
            open_document: None,
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

    /// The project root and root-relative identity of one document.
    ///
    /// A desktop file association hands over a document, not a project, and
    /// the workspace opens projects. `07_VERSIONING_AND_EXTENSIONS/02` resolves
    /// a `SOURCE PATH` "relative only to importing file or explicit WORKSPACE",
    /// so the root a document belongs to is the nearest ancestor that declares
    /// itself one. When no ancestor does, the document's own directory is the
    /// root: that is a rootless project, which `Workspace::open` already
    /// supports, and it keeps containment without inventing a manifest in
    /// someone's home directory.
    ///
    /// The search stops at the first manifest and never leaves the filesystem
    /// path it was given. Nothing is discovered by scanning, and no file
    /// outside the returned root becomes reachable.
    pub fn locate_document(document: &Path) -> Result<(PathBuf, String), WorkspaceError> {
        let document = document.canonicalize().map_err(|e| WorkspaceError::Io {
            path: document.to_path_buf(),
            detail: format!("the document could not be opened: {e}"),
        })?;
        if !document.is_file() {
            return Err(WorkspaceError::Io {
                path: document,
                detail: "a document is a file; pass a directory as the project instead"
                    .to_string(),
            });
        }
        let directory = document
            .parent()
            .ok_or_else(|| WorkspaceError::Io {
                path: document.clone(),
                detail: "the document has no containing directory".to_string(),
            })?
            .to_path_buf();

        let mut root = directory.clone();
        loop {
            if root.join(MANIFEST_FILE).is_file() {
                break;
            }
            match root.parent() {
                Some(parent) => root = parent.to_path_buf(),
                // No ancestor declares a project, so the document's own
                // directory is the root.
                None => {
                    root = directory;
                    break;
                }
            }
        }

        let relative = document
            .strip_prefix(&root)
            .map_err(|_| WorkspaceError::Document(DocumentError::Outside(document.clone())))?;
        Ok((root, to_identity(relative)))
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

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn spec_root(&self) -> &Path {
        &self.spec_root
    }

    /// The document a bare command acts on, when the manifest declares one.
    pub fn entry(&self) -> Option<String> {
        let path = self.project.entry_path()?;
        let root = self.project.root();
        let relative = path.strip_prefix(root).ok()?;
        Some(to_identity(relative))
    }

    /// Every `.lcl` document in the project, in ascending identity order.
    ///
    /// Deterministic: the walk sorts each directory's entries by name rather
    /// than taking the filesystem's enumeration order, which contract 5.3
    /// names as something observable meaning must never depend on. A file tree
    /// is not language meaning, but a list that reordered itself between two
    /// reads would still be a bug a user sees.
    pub fn documents(&self) -> Result<Vec<Entry>, WorkspaceError> {
        let mut entries = Vec::new();
        walk(self.project.root(), self.project.root(), 0, &mut entries)?;
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        entries.truncate(MAX_ENTRIES);
        Ok(entries)
    }

    /// Read one document.
    pub fn read(&self, id: &str) -> Result<Document, WorkspaceError> {
        Ok(document::read(self.project.root(), id)?)
    }

    /// Save one document.
    pub fn save(&self, id: &str, text: &str) -> Result<Document, WorkspaceError> {
        Ok(document::write(self.project.root(), id, text)?)
    }
}

/// Walk one directory, appending every `.lcl` document and every directory.
fn walk(
    root: &Path,
    directory: &Path,
    depth: usize,
    out: &mut Vec<Entry>,
) -> Result<(), WorkspaceError> {
    if depth > MAX_DEPTH || out.len() > MAX_ENTRIES {
        return Ok(());
    }
    let listing = std::fs::read_dir(directory).map_err(|e| WorkspaceError::Io {
        path: directory.to_path_buf(),
        detail: format!("the directory is not readable: {e}"),
    })?;
    let mut children: Vec<PathBuf> = listing.filter_map(Result::ok).map(|e| e.path()).collect();
    children.sort();

    for path in children {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // A dot directory is tooling, not source. The package cache is one.
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            out.push(Entry {
                id: to_identity(relative),
                directory: true,
                bytes: None,
            });
            walk(root, &path, depth + 1, out)?;
        } else if path.extension().is_some_and(|e| e == "lcl") {
            out.push(Entry {
                id: to_identity(relative),
                directory: false,
                bytes: std::fs::metadata(&path).ok().map(|m| m.len()),
            });
        }
    }
    Ok(())
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
