//! A deterministic in-memory filesystem.
//!
//! Same contents in, same answers out, on every machine and in every order. It
//! has no clock, no real paths and no enumeration order of its own: entries
//! live in a `BTreeMap`, so a listing is sorted by construction rather than by
//! whatever the operating system happened to return.
//!
//! It enforces the same scope rule the real adapter does, because a fixture
//! that permitted what the real one refuses would let a conformance test pass
//! against behavior no real run could reproduce.

use lcl_capabilities::fs::{FileSystem, FsError, Location, Metadata, WriteMode};
use lcl_capabilities::{Bounds, Grant, Grants};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// An in-memory filesystem.
#[derive(Debug, Clone, Default)]
pub struct MemoryFileSystem {
    files: BTreeMap<PathBuf, Vec<u8>>,
    directories: Vec<PathBuf>,
    grants: Grants,
}

impl MemoryFileSystem {
    /// A filesystem that permits nothing until a scope is granted.
    pub fn new() -> MemoryFileSystem {
        MemoryFileSystem::default()
    }

    /// Grant one writable scope, as an operator would.
    pub fn with_scope(mut self, root: impl Into<PathBuf>) -> MemoryFileSystem {
        self.grants = self.grants.permit_write(root);
        self
    }

    /// Grant one read-only scope.
    pub fn with_read_scope(mut self, root: impl Into<PathBuf>) -> MemoryFileSystem {
        self.grants = self.grants.permit_read(root);
        self
    }

    /// Seed one file.
    pub fn with_file(
        mut self,
        path: impl Into<PathBuf>,
        content: impl AsRef<[u8]>,
    ) -> MemoryFileSystem {
        let path = lcl_capabilities::normalize(&path.into());
        if let Some(parent) = path.parent() {
            self.directories.push(parent.to_path_buf());
        }
        self.files.insert(path, content.as_ref().to_vec());
        self
    }

    /// Seed one directory.
    pub fn with_directory(mut self, path: impl Into<PathBuf>) -> MemoryFileSystem {
        self.directories
            .push(lcl_capabilities::normalize(&path.into()));
        self
    }

    /// The exact bytes one path holds, for a test to assert against.
    pub fn file(&self, path: impl AsRef<Path>) -> Option<&[u8]> {
        self.files
            .get(&lcl_capabilities::normalize(path.as_ref()))
            .map(|bytes| bytes.as_slice())
    }

    /// Every path this filesystem holds, in sorted order.
    pub fn paths(&self) -> Vec<&Path> {
        self.files.keys().map(|p| p.as_path()).collect()
    }

    pub fn grants(&self) -> &Grants {
        &self.grants
    }

    /// The same two gates as the real adapter, in the same order. This
    /// filesystem has no links, so its resolution is lexical.
    fn admit(&self, location: Location<'_>, write: bool) -> Result<PathBuf, FsError> {
        let path = location.path;
        let resolved = lcl_capabilities::normalize(path);
        if let Some(root) = location.within {
            if !lcl_capabilities::contains(&lcl_capabilities::normalize(root), &resolved) {
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
        Ok(resolved)
    }

    fn is_directory(&self, path: &Path) -> bool {
        self.directories.iter().any(|d| d == path)
            || self
                .files
                .keys()
                .any(|f| f.parent().is_some_and(|parent| parent == path))
    }
}

impl FileSystem for MemoryFileSystem {
    fn metadata(&mut self, location: Location<'_>) -> Result<Metadata, FsError> {
        let resolved = self.admit(location, false)?;
        if let Some(content) = self.files.get(&resolved) {
            return Ok(Metadata {
                exists: true,
                is_directory: false,
                bytes: content.len() as u64,
                entries: Vec::new(),
            });
        }
        if self.is_directory(&resolved) {
            let mut entries: Vec<String> = self
                .files
                .keys()
                .filter(|f| f.parent().is_some_and(|parent| parent == resolved))
                .filter_map(|f| f.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .collect();
            entries.sort();
            entries.dedup();
            return Ok(Metadata {
                exists: true,
                is_directory: true,
                bytes: 0,
                entries,
            });
        }
        Ok(Metadata {
            exists: false,
            is_directory: false,
            bytes: 0,
            entries: Vec::new(),
        })
    }

    fn read(&mut self, location: Location<'_>, bounds: &Bounds) -> Result<Vec<u8>, FsError> {
        let path = location.path;
        let resolved = self.admit(location, false)?;
        let content = self
            .files
            .get(&resolved)
            .ok_or_else(|| FsError::NotFound(path.to_path_buf()))?;
        bounds
            .check(content.len() as u64, bounds.max_bytes, "the read")
            .map_err(FsError::Bounded)?;
        Ok(content.clone())
    }

    fn write(
        &mut self,
        location: Location<'_>,
        content: &[u8],
        mode: WriteMode,
    ) -> Result<(), FsError> {
        let path = location.path;
        let resolved = self.admit(location, true)?;
        let exists = self.files.contains_key(&resolved);
        match mode {
            WriteMode::Create if exists => return Err(FsError::AlreadyExists(path.to_path_buf())),
            WriteMode::ReplaceExisting if !exists => {
                return Err(FsError::NotFound(path.to_path_buf()))
            }
            _ => {}
        }
        self.files.insert(resolved, content.to_vec());
        Ok(())
    }

    fn append(&mut self, location: Location<'_>, content: &[u8]) -> Result<(), FsError> {
        let path = location.path;
        let resolved = self.admit(location, true)?;
        let existing = self
            .files
            .get_mut(&resolved)
            .ok_or_else(|| FsError::NotFound(path.to_path_buf()))?;
        existing.extend_from_slice(content);
        Ok(())
    }

    fn delete(&mut self, location: Location<'_>, recursive: bool) -> Result<bool, FsError> {
        let path = location.path;
        let resolved = self.admit(location, true)?;
        if self.files.remove(&resolved).is_some() {
            return Ok(true);
        }
        if self.is_directory(&resolved) {
            if !recursive {
                return Err(FsError::Io(format!(
                    "{} is a directory and the request is not recursive",
                    path.display()
                )));
            }
            let inside: Vec<PathBuf> = self
                .files
                .keys()
                .filter(|f| f.starts_with(&resolved))
                .cloned()
                .collect();
            for file in inside {
                self.files.remove(&file);
            }
            self.directories.retain(|d| !d.starts_with(&resolved));
            return Ok(true);
        }
        Ok(false)
    }

    fn rename(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<(), FsError> {
        let source = self.admit(from, true)?;
        let destination = self.admit(to, true)?;
        let (from, to) = (from.path, to.path);
        if !self.files.contains_key(&source) {
            return Err(FsError::NotFound(from.to_path_buf()));
        }
        if self.files.contains_key(&destination) && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        let content = self.files.remove(&source).unwrap_or_default();
        self.files.insert(destination, content);
        Ok(())
    }

    fn copy(
        &mut self,
        from: Location<'_>,
        to: Location<'_>,
        overwrite: bool,
    ) -> Result<u64, FsError> {
        let source = self.admit(from, false)?;
        let destination = self.admit(to, true)?;
        let (from, to) = (from.path, to.path);
        let content = self
            .files
            .get(&source)
            .cloned()
            .ok_or_else(|| FsError::NotFound(from.to_path_buf()))?;
        if self.files.contains_key(&destination) && !overwrite {
            return Err(FsError::AlreadyExists(to.to_path_buf()));
        }
        let bytes = content.len() as u64;
        self.files.insert(destination, content);
        Ok(bytes)
    }
}
