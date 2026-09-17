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

/// The largest file this product reads whole: a project document, the manifest,
/// a lock file, the cache index, a cached blob, or a file being vendored.
///
/// A host limit, not a language rule. It equals the workspace's request-body
/// ceiling, so the editor can open whatever it can save.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Read one whole file, holding at most [`MAX_FILE_BYTES`] + 1 bytes.
///
/// One byte past the limit is enough to know the limit was exceeded, so a
/// larger or endless file is refused without being allocated.
pub fn read_file(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("the file exceeds the product limit of {MAX_FILE_BYTES} bytes"),
        ));
    }
    Ok(bytes)
}

/// [`read_file`], as UTF-8 text.
pub(crate) fn read_text(path: &Path) -> std::io::Result<String> {
    String::from_utf8(read_file(path)?)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// The project root a document belongs to, and its root-relative identity.
///
/// Every product asks this one question the same way, so a document never
/// receives a different root, identity or declared specification package
/// depending on which product opened it. A relative `document` is taken from
/// the working directory where the caller runs, once.
///
/// `07_VERSIONING_AND_EXTENSIONS/02` resolves a `SOURCE PATH` "relative only to
/// importing file or explicit WORKSPACE", so the root is the nearest ancestor
/// that declares itself one with a manifest. When no ancestor does, the
/// document's own directory is the root: a rootless project, which keeps
/// containment without inventing a manifest.
///
/// The search stops at the first manifest and never leaves the path it was
/// given. Nothing is discovered by scanning, and no file outside the returned
/// root becomes reachable.
pub fn locate_document(document: &Path) -> Result<(PathBuf, String), ProjectPathError> {
    let document = document.canonicalize().map_err(|e| ProjectPathError {
        path: document.to_path_buf(),
        detail: format!("the document is not readable: {e}"),
    })?;
    if !document.is_file() {
        return Err(ProjectPathError {
            path: document,
            detail: "a document is a file; pass a directory as the project instead".to_string(),
        });
    }
    let directory = document
        .parent()
        .ok_or_else(|| ProjectPathError {
            path: document.clone(),
            detail: "the document has no containing directory".to_string(),
        })?
        .to_path_buf();
    let root = directory
        .ancestors()
        .find(|ancestor| ancestor.join(MANIFEST_FILE).is_file())
        .unwrap_or(directory.as_path())
        .to_path_buf();
    let identity = document
        .strip_prefix(&root)
        .map_err(|_| ProjectPathError {
            path: document.clone(),
            detail: "the document is outside its project root".to_string(),
        })?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    Ok((root, identity))
}

/// Why a project could not be opened.
#[derive(Debug)]
pub enum ProjectError {
    /// No manifest at the named root.
    NoManifest(PathBuf),
    Manifest(ManifestError),
    Path(ProjectPathError),
    /// The manifest declares a package cache that will not open.
    Cache(CacheError),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::NoManifest(dir) => {
                write!(f, "{} holds no {MANIFEST_FILE}", dir.display())
            }
            ProjectError::Manifest(inner) => write!(f, "{inner}"),
            ProjectError::Path(inner) => write!(f, "{inner}"),
            ProjectError::Cache(inner) => write!(f, "the declared package cache: {inner}"),
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

    /// The declared Core 0.2.0 package path, resolved against the root.
    pub fn localized_spec_path(&self) -> Option<PathBuf> {
        self.manifest
            .localized_spec
            .as_ref()
            .map(|p| self.resolve(p))
    }

    /// The declared locale profile directory, resolved against the root.
    pub fn profiles_path(&self) -> Option<PathBuf> {
        self.manifest.profiles.as_ref().map(|p| self.resolve(p))
    }

    /// Every locale profile file in the declared profile directory: each
    /// `*.json` file directly inside it, in ascending path order. Empty when
    /// the manifest declares no profile directory.
    pub fn profile_files(&self) -> std::io::Result<Vec<PathBuf>> {
        let Some(directory) = self.profiles_path() else {
            return Ok(Vec::new());
        };
        let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        Ok(files)
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

    /// Where `package lock` may write the lock file.
    ///
    /// A manifest may name a lock outside the project for reading; a shared,
    /// read-only lock is a legitimate reference. Writing is a product effect,
    /// and a manifest must not direct it at an arbitrary path, so the
    /// destination must really lie inside the project root, judged where it
    /// leads, links included.
    pub fn lock_destination(&self) -> Result<PathBuf, ProjectPathError> {
        let path = self.lock_path();
        let resolved = lcl_capabilities::fs::resolve(&path);
        if lcl_capabilities::contains(&self.root, &resolved) {
            return Ok(path);
        }
        Err(ProjectPathError {
            detail: format!(
                "the lock file resolves to {}, outside the project root {}; \
                 `package lock` writes only inside the project",
                resolved.display(),
                self.root.display()
            ),
            path,
        })
    }

    /// A provider over this project, with the declared cache when there is one.
    ///
    /// A declared cache that will not open is a fault in the project's
    /// configuration and is returned as one. Dropping it would turn a damaged
    /// store into a project without a cache, and every `URI` import into a
    /// missing one.
    pub fn provider(&self) -> Result<FileProvider, ProjectError> {
        let provider = FileProvider::new(&self.root).map_err(ProjectError::Path)?;
        match self.cache_path() {
            Some(dir) => Ok(provider.with_cache(Cache::open(&dir).map_err(ProjectError::Cache)?)),
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
