//! The filesystem source provider.
//!
//! ## The one rule this file exists to enforce
//!
//! `05_SEMANTICS/02_SCOPE_TARGET_WORKSPACE_AND_SOURCE.txt`:
//!
//! > Ambient current directory and implied nearby files do not exist in
//! > portable LCL.
//!
//! and
//!
//! > A workspace-relative PATH is legal only when its resolved target is the
//! > WORKSPACE root or a descendant; textual prefix alone does not establish
//! > containment.
//!
//! [`lcl_resolver::SourceProvider`] already makes half of this structural: it
//! has one method, and no way to list, glob, search or default a unit, so no
//! provider can volunteer a file the source text did not name. What is left is
//! the other half, containment, and it is enforced on the *resolved* path
//! rather than on the text — which is why a symbolic link pointing outside the
//! root is refused even though its spelling is entirely innocent.
//!
//! ## Identity is root-relative, and that is what makes a project reproducible
//!
//! A unit's [`lcl_resolver::SourceId`] is its path relative to the project
//! root, with `/` separators. Two checkouts of the same project at different
//! absolute paths therefore produce identical identities, identical spans and
//! identical reports. An identity carrying an absolute path would make every
//! diagnostic machine-specific.
//!
//! ## What this provider does not do
//!
//! It performs no network access. A `URI` import is answered from the local
//! content-addressed cache in [`crate::cache`] or not at all: the architecture
//! contract requires "no resolver-owned web browsing or hidden provider
//! discovery", and a provider that could fetch would make a document's meaning
//! depend on when it was resolved.

use crate::cache::Cache;
use lcl_resolver::{LoadError, SourceId, SourceProvider, SourceRef, SourceRequest, SourceUnit};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// A provider rooted at one explicit project directory.
///
/// Every unit it can answer with lives at or below `root`. It reads a file only
/// when a document names it, and it records what it read, so a caller can prove
/// afterwards that nothing else entered the program.
pub struct FileProvider {
    root: PathBuf,
    cache: Option<Cache>,
    /// Every unit actually handed to the resolver, in ascending identity order.
    loaded: RefCell<BTreeSet<String>>,
}

impl FileProvider {
    /// A provider over one project root.
    ///
    /// The root is canonicalised once. Containment is then decided against the
    /// canonical root, so a link, a `..` or a differently-spelled path cannot
    /// reach outside it.
    pub fn new(root: impl AsRef<Path>) -> Result<FileProvider, ProjectPathError> {
        let root = root.as_ref();
        let canonical = root.canonicalize().map_err(|source| ProjectPathError {
            path: root.to_path_buf(),
            detail: format!("the project root is not readable: {source}"),
        })?;
        if !canonical.is_dir() {
            return Err(ProjectPathError {
                path: canonical,
                detail: "the project root is not a directory".to_string(),
            });
        }
        Ok(FileProvider {
            root: canonical,
            cache: None,
            loaded: RefCell::new(BTreeSet::new()),
        })
    }

    /// Answer `URI` imports from this content-addressed cache.
    ///
    /// Without one, a `URI` import is unresolvable, which is the honest state
    /// for a tool that does not fetch.
    pub fn with_cache(mut self, cache: Cache) -> FileProvider {
        self.cache = Some(cache);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every unit this provider actually supplied, in identity order.
    ///
    /// The resolver never calls this — [`SourceProvider`] does not expose it —
    /// so it cannot influence resolution. It exists so a caller can assert that
    /// the set of files that entered the program is exactly the set the
    /// documents named.
    pub fn loaded(&self) -> Vec<String> {
        self.loaded.borrow().iter().cloned().collect()
    }

    /// Read the document at `path` as the project's root unit.
    ///
    /// The path must be inside the project root. Its identity is its
    /// root-relative form, which is the identity every import of it will also
    /// produce.
    pub fn root_unit(&self, path: impl AsRef<Path>) -> Result<SourceUnit, ProjectPathError> {
        let path = path.as_ref();
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        let canonical = absolute.canonicalize().map_err(|source| ProjectPathError {
            path: absolute.clone(),
            detail: format!("the document is not readable: {source}"),
        })?;
        let id = self.identity(&canonical)?;
        let bytes = std::fs::read(&canonical).map_err(|source| ProjectPathError {
            path: canonical.clone(),
            detail: format!("the document is not readable: {source}"),
        })?;
        self.loaded.borrow_mut().insert(id.clone());
        Ok(SourceUnit::new(SourceId::new(id), bytes))
    }

    /// The root-relative identity of one canonical path inside the project.
    fn identity(&self, canonical: &Path) -> Result<String, ProjectPathError> {
        if !lcl_capabilities::contains(&self.root, canonical) {
            return Err(ProjectPathError {
                path: canonical.to_path_buf(),
                detail: format!(
                    "the resolved path is outside the project root {}",
                    self.root.display()
                ),
            });
        }
        let relative = canonical
            .strip_prefix(&self.root)
            .map_err(|_| ProjectPathError {
                path: canonical.to_path_buf(),
                detail: "the resolved path is outside the project root".to_string(),
            })?;
        Ok(relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/"))
    }

    /// Resolve one `PATH` reference against the unit that wrote it.
    ///
    /// `07_VERSIONING_AND_EXTENSIONS/02`: "SOURCE PATH resolves relative only
    /// to importing file or explicit WORKSPACE." A leading `/` selects the
    /// project root, which is the "explicit WORKSPACE" form for a project; any
    /// other spelling is relative to the importing unit's own directory. There
    /// is no upward search and no fallback, because either would let a document
    /// import a file it did not name.
    fn resolve_path(&self, origin: &SourceId, path: &str) -> Result<PathBuf, LoadError> {
        if path.is_empty() {
            return Err(LoadError::new("the SOURCE PATH is empty"));
        }
        let joined = if let Some(rooted) = path.strip_prefix('/') {
            self.root.join(rooted)
        } else {
            let origin_dir = match origin.as_str().rfind('/') {
                Some(cut) => self.root.join(&origin.as_str()[..cut]),
                None => self.root.clone(),
            };
            origin_dir.join(path)
        };
        // Fold `.` and `..` before touching the filesystem, so a path that
        // escapes on paper is refused without a syscall, and then canonicalise,
        // so one that escapes through a link is refused as well.
        let folded = fold(&joined).ok_or_else(|| {
            LoadError::new(format!(
                "{path:?} resolves above the project root {}",
                self.root.display()
            ))
        })?;
        let canonical = folded
            .canonicalize()
            .map_err(|source| LoadError::new(format!("{path:?} is not readable: {source}")))?;
        if !lcl_capabilities::contains(&self.root, &canonical) {
            return Err(LoadError::new(format!(
                "{path:?} resolves outside the project root {}",
                self.root.display()
            )));
        }
        Ok(canonical)
    }
}

impl SourceProvider for FileProvider {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError> {
        match &request.reference {
            SourceRef::Path(path) => {
                let canonical = self.resolve_path(&request.origin, path)?;
                let id = self
                    .identity(&canonical)
                    .map_err(|e| LoadError::new(e.detail))?;
                let bytes = std::fs::read(&canonical).map_err(|source| {
                    LoadError::new(format!("{path:?} is not readable: {source}"))
                })?;
                self.loaded.borrow_mut().insert(id.clone());
                Ok(SourceUnit::new(SourceId::new(id), bytes))
            }
            SourceRef::Uri(uri) => {
                // A URI names itself, and nothing here fetches. The cache can
                // answer only for a URI something already placed in it, under
                // the exact checksum the importing document declares.
                let Some(cache) = &self.cache else {
                    return Err(LoadError::new(format!(
                        "{uri:?} is not available offline: no package cache is configured, and \
                         this tool does not fetch"
                    )));
                };
                let bytes = cache.get(uri)?;
                self.loaded.borrow_mut().insert(uri.clone());
                Ok(SourceUnit::new(SourceId::new(uri.clone()), bytes))
            }
        }
    }
}

/// Fold `.` and `..` lexically. `None` when `..` escapes above the path's root.
fn fold(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    Some(out)
}

/// A path a project could not use, and exactly why.
///
/// Distinct from [`LoadError`]: that one is an answer to a request a document
/// made, and every variant of it becomes `error.import.not_found`. This one is
/// an answer to the *caller*, about a path the caller supplied, and it is never
/// a language diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPathError {
    pub path: PathBuf,
    pub detail: String,
}

impl std::fmt::Display for ProjectPathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.detail)
    }
}

impl std::error::Error for ProjectPathError {}
