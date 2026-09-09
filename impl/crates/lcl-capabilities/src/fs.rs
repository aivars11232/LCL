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
//! symlinked directory is judged by where that directory really is.

use crate::bounds::{Bounds, Cancelled};
use crate::grant::{self, Grant, Grants, Refusal};
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

/// What one filesystem request could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The target does not exist.
    NotFound(PathBuf),
    /// The target exists and the request required that it not.
    AlreadyExists(PathBuf),
    /// The host refuses or cannot supply the access.
    Refused(Refusal),
    /// A declared bound stopped the work.
    Bounded(Cancelled),
    /// The operating system reported a failure.
    Io(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotFound(path) => write!(f, "{} does not exist", path.display()),
            FsError::AlreadyExists(path) => write!(f, "{} already exists", path.display()),
            FsError::Refused(refusal) => write!(f, "{refusal}"),
            FsError::Bounded(cancelled) => write!(f, "{cancelled}"),
            FsError::Io(detail) => f.write_str(detail),
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
    fn metadata(&mut self, path: &Path) -> Result<Metadata, FsError>;
    fn read(&mut self, path: &Path, bounds: &Bounds) -> Result<Vec<u8>, FsError>;
    fn write(&mut self, path: &Path, content: &[u8], mode: WriteMode) -> Result<(), FsError>;
    fn append(&mut self, path: &Path, content: &[u8]) -> Result<(), FsError>;
    /// Remove the target. Returns whether anything was removed.
    fn delete(&mut self, path: &Path, recursive: bool) -> Result<bool, FsError>;
    fn rename(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<(), FsError>;
    /// Copy the target. Returns the number of bytes copied.
    fn copy(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<u64, FsError>;
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

    /// Decide the host gate, then verify containment against real locations.
    fn admit(&self, path: &Path, write: bool) -> Result<PathBuf, FsError> {
        let grant = if write {
            Grant::WritePath(path.to_path_buf())
        } else {
            Grant::ReadPath(path.to_path_buf())
        };
        self.grants.decide(&grant).map_err(FsError::Refused)?;

        // Resolve symlinks and re-check. A lexical check cannot see a link that
        // leaves the scope, and this one can.
        let resolved = resolve(path);
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

/// The real location of a path, resolving symlinks as far as it exists.
///
/// A path that does not exist yet is judged by its nearest existing ancestor,
/// so a new file inside a symlinked directory is placed where that directory
/// really is rather than where it is spelled.
fn resolve(path: &Path) -> PathBuf {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return canonical;
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

impl FileSystem for RealFileSystem {
    fn metadata(&mut self, path: &Path) -> Result<Metadata, FsError> {
        let resolved = self.admit(path, false)?;
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

    fn read(&mut self, path: &Path, bounds: &Bounds) -> Result<Vec<u8>, FsError> {
        let resolved = self.admit(path, false)?;
        let meta =
            std::fs::metadata(&resolved).map_err(|_| FsError::NotFound(path.to_path_buf()))?;
        bounds
            .check(meta.len(), bounds.max_bytes, "the read")
            .map_err(FsError::Bounded)?;
        std::fs::read(&resolved).map_err(io)
    }

    fn write(&mut self, path: &Path, content: &[u8], mode: WriteMode) -> Result<(), FsError> {
        let resolved = self.admit(path, true)?;
        let exists = resolved.exists();
        match mode {
            WriteMode::Create if exists => return Err(FsError::AlreadyExists(path.to_path_buf())),
            WriteMode::ReplaceExisting if !exists => {
                return Err(FsError::NotFound(path.to_path_buf()))
            }
            _ => {}
        }
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::write(&resolved, content).map_err(io)
    }

    fn append(&mut self, path: &Path, content: &[u8]) -> Result<(), FsError> {
        let resolved = self.admit(path, true)?;
        if !resolved.exists() {
            return Err(FsError::NotFound(path.to_path_buf()));
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&resolved)
            .map_err(io)?;
        file.write_all(content).map_err(io)
    }

    fn delete(&mut self, path: &Path, recursive: bool) -> Result<bool, FsError> {
        let resolved = self.admit(path, true)?;
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

    fn rename(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<(), FsError> {
        let source = self.admit(from, true)?;
        let destination = self.admit(to, true)?;
        if !source.exists() {
            return Err(FsError::NotFound(from.to_path_buf()));
        }
        if destination.exists() && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::rename(&source, &destination).map_err(io)
    }

    fn copy(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<u64, FsError> {
        let source = self.admit(from, false)?;
        let destination = self.admit(to, true)?;
        if !source.exists() {
            return Err(FsError::NotFound(from.to_path_buf()));
        }
        if destination.exists() && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::copy(&source, &destination).map_err(io)
    }
}
