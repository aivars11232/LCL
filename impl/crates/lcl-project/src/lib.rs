//! # lcl-project — projects, documents and reproducible source identity
//!
//! Milestone M9 component C.
//!
//! Supplies source units to the resolver, and nothing else. It decides no
//! language rule: it does not interpret a document, does not check a checksum
//! the language checks, and cannot make an invalid program valid or a valid one
//! invalid. What it decides is *which bytes exist under which identity*, which
//! is precisely what the source-provider contract leaves to an embedder.
//!
//! ## The three things a project fixes
//!
//! 1. **A root.** The directory holding [`manifest::MANIFEST_FILE`], or one a
//!    caller names outright. Never inferred from a working directory, because
//!    `05_SEMANTICS/02` says "Ambient current directory and implied nearby
//!    files do not exist in portable LCL".
//! 2. **An identity scheme.** A unit's identity is its root-relative path with
//!    `/` separators, so two checkouts at different absolute paths produce
//!    identical identities, spans and reports.
//! 3. **A containment boundary.** Every resolved path must be the root or a
//!    descendant of it, decided after canonicalisation — "textual prefix alone
//!    does not establish containment".
//!
//! ## What a project cannot do
//!
//! It cannot enumerate. [`lcl_resolver::SourceProvider`] has one method and no
//! listing, globbing, searching or defaulting, so a file that no document names
//! cannot enter the program through it. [`provider::FileProvider::loaded`]
//! exists to let a caller *prove* that after the fact, and the resolver cannot
//! call it.
//!
//! It does not fetch. A `URI` import is answered from the local
//! content-addressed [`cache::Cache`] or not at all.

pub mod cache;
pub mod lock;
pub mod manifest;
pub mod naming;
pub mod provider;

pub use cache::{Cache, CacheError};
pub use lock::{Drift, Lock, LockError};
pub use manifest::{Manifest, ManifestError, MANIFEST_FILE, MANIFEST_FORMAT};
pub use naming::{default_name, is_document, SUFFIX, SUFFIXES, TEXT_SUFFIX};
pub use provider::{FileProvider, ProjectPathError};

use std::fmt;
use std::path::{Path, PathBuf};

/// Why a project could not be opened.
#[derive(Debug)]
pub enum ProjectError {
    /// No manifest at the named root.
    NoManifest(PathBuf),
    Manifest(ManifestError),
    Path(ProjectPathError),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::NoManifest(dir) => {
                write!(f, "{} holds no {MANIFEST_FILE}", dir.display())
            }
            ProjectError::Manifest(inner) => write!(f, "{inner}"),
            ProjectError::Path(inner) => write!(f, "{inner}"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// One opened project: a root, its manifest, and the paths it declares.
///
/// Holds no source. Opening a project reads exactly one file, the manifest, and
/// touches nothing else until a document names something.
#[derive(Debug, Clone)]
pub struct Project {
    root: PathBuf,
    manifest: Manifest,
}

impl Project {
    /// Open the project rooted at `root`, which must hold a manifest.
    pub fn open(root: impl AsRef<Path>) -> Result<Project, ProjectError> {
        let root = root.as_ref().canonicalize().map_err(|e| {
            ProjectError::Path(ProjectPathError {
                path: root.as_ref().to_path_buf(),
                detail: format!("the project root is not readable: {e}"),
            })
        })?;
        let manifest_path = root.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            return Err(ProjectError::NoManifest(root));
        }
        let manifest = Manifest::read(&manifest_path).map_err(ProjectError::Manifest)?;
        Ok(Project { root, manifest })
    }

    /// A project rooted at `root` with no manifest.
    ///
    /// For a caller acting on one document outside any project: the root is
    /// still explicit, containment still applies, and identity is still
    /// root-relative. What is absent is the manifest's declared spec, entry,
    /// cache and lock, so a caller must supply whichever of those it needs.
    pub fn rootless(root: impl AsRef<Path>) -> Result<Project, ProjectError> {
        let root = root.as_ref().canonicalize().map_err(|e| {
            ProjectError::Path(ProjectPathError {
                path: root.as_ref().to_path_buf(),
                detail: format!("the project root is not readable: {e}"),
            })
        })?;
        Ok(Project {
            root,
            manifest: Manifest::default(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The declared specification package path, resolved against the root.
    pub fn spec_path(&self) -> Option<PathBuf> {
        self.manifest.spec.as_ref().map(|p| self.resolve(p))
    }

    /// The declared entry document, resolved against the root.
    pub fn entry_path(&self) -> Option<PathBuf> {
        self.manifest.entry.as_ref().map(|p| self.resolve(p))
    }

    /// The declared package cache directory, resolved against the root.
    pub fn cache_path(&self) -> Option<PathBuf> {
        self.manifest.cache.as_ref().map(|p| self.resolve(p))
    }

    /// The lock file, resolved against the root.
    ///
    /// Defaults to [`lock::LOCK_FILE`] in the root, so a project that wants a
    /// lock file does not have to say where it goes.
    pub fn lock_path(&self) -> PathBuf {
        match &self.manifest.lock {
            Some(path) => self.resolve(path),
            None => self.root.join(lock::LOCK_FILE),
        }
    }

    /// A provider over this project, with the declared cache when there is one.
    pub fn provider(&self) -> Result<FileProvider, ProjectError> {
        let provider = FileProvider::new(&self.root).map_err(ProjectError::Path)?;
        match self.cache_path() {
            Some(dir) => match Cache::open(&dir) {
                Ok(cache) => Ok(provider.with_cache(cache)),
                // A cache that will not open is a real fault, but it is the
                // caller's to report: the provider without it simply cannot
                // answer a URI, and says so when asked.
                Err(_) => Ok(provider),
            },
            None => Ok(provider),
        }
    }

    /// Resolve one manifest-declared path against the project root.
    ///
    /// An absolute path is taken as written. A relative one is joined to the
    /// root, never to a working directory.
    fn resolve(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }
}
