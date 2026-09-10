//! The lock file: exactly which bytes a project resolved to.
//!
//! ## What locking can and cannot promise
//!
//! It records the specification package's identity and every source unit's
//! SHA-256, so a later run over the same project can be shown to have read the
//! same bytes. That is a reproducibility statement about *inputs*, and it is
//! the only one available honestly: an execution's outputs depend on the host
//! it was given, and no file can pin a host.
//!
//! It changes no language rule. `07_VERSIONING_AND_EXTENSIONS/02` already makes
//! `VERSION` exact and a `URI` import's `CHECKSUM` mandatory, and the resolver
//! already enforces both. A lock file that disagrees with the documents does
//! not overrule them; it reports the disagreement and the caller decides.
//!
//! ## The format
//!
//! One header line, then keyed lines, then one `unit` line per source in
//! ascending identity order. Digest and name are separated by two spaces, which
//! is what `SHA256SUMS.txt` in the canonical package does, and what
//! `sha256sum -c` reads. Sorted, so a lock file is stable across machines and
//! diffs cleanly.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// The first line of a lock file, which fixes its format.
const HEADER: &str = "lcl-lock/1";

/// The default lock file name inside a project root.
pub const LOCK_FILE: &str = "lcl.lock";

/// Why a lock file could not be used.
#[derive(Debug)]
pub enum LockError {
    Io { path: PathBuf, detail: String },
    Malformed { path: PathBuf, detail: String },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Io { path, detail } | LockError::Malformed { path, detail } => {
                write!(f, "{}: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for LockError {}

/// One way a project's current state differs from its lock file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drift {
    /// The specification package is not the one that was locked.
    Spec { locked: String, actual: String },
    /// The root document is not the one that was locked.
    Root { locked: String, actual: String },
    /// A locked unit's bytes changed.
    Changed {
        unit: String,
        locked: String,
        actual: String,
    },
    /// A locked unit was not loaded this time.
    Missing { unit: String },
    /// A unit was loaded that the lock file does not list.
    Added { unit: String, digest: String },
}

impl fmt::Display for Drift {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Drift::Spec { locked, actual } => write!(
                f,
                "the specification package changed: locked {locked}, now {actual}"
            ),
            Drift::Root { locked, actual } => {
                write!(
                    f,
                    "the root document changed: locked {locked}, now {actual}"
                )
            }
            Drift::Changed {
                unit,
                locked,
                actual,
            } => write!(f, "{unit} changed: locked {locked}, now {actual}"),
            Drift::Missing { unit } => write!(f, "{unit} is locked but was not loaded"),
            Drift::Added { unit, digest } => {
                write!(f, "{unit} was loaded but is not locked ({digest})")
            }
        }
    }
}

/// The recorded state of one project's inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    /// The canonical package's identity digest.
    pub spec_identity: String,
    /// The canonical package's formal version.
    pub spec_version: String,
    /// The root document's identity.
    pub root: String,
    /// Unit identity -> lowercase hex SHA-256, in ascending identity order.
    pub units: BTreeMap<String, String>,
}

impl Lock {
    /// Build a lock from what a resolution actually loaded.
    pub fn new(
        spec_identity: impl Into<String>,
        spec_version: impl Into<String>,
        root: impl Into<String>,
        units: impl IntoIterator<Item = (String, String)>,
    ) -> Lock {
        Lock {
            spec_identity: spec_identity.into(),
            spec_version: spec_version.into(),
            root: root.into(),
            units: units.into_iter().collect(),
        }
    }

    /// Read a lock file.
    pub fn read(path: impl AsRef<Path>) -> Result<Lock, LockError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|e| LockError::Io {
            path: path.to_path_buf(),
            detail: format!("the lock file is not readable: {e}"),
        })?;
        Lock::parse(&text).map_err(|detail| LockError::Malformed {
            path: path.to_path_buf(),
            detail,
        })
    }

    /// Write this lock to `path`.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), LockError> {
        let path = path.as_ref();
        std::fs::write(path, self.render()).map_err(|e| LockError::Io {
            path: path.to_path_buf(),
            detail: format!("the lock file could not be written: {e}"),
        })
    }

    /// The exact bytes of this lock file.
    pub fn render(&self) -> String {
        let mut out = String::from(HEADER);
        out.push('\n');
        out.push_str(&format!("spec-version {}\n", self.spec_version));
        out.push_str(&format!("spec-identity {}\n", self.spec_identity));
        out.push_str(&format!("root {}\n", self.root));
        for (unit, digest) in &self.units {
            out.push_str(&format!("unit {digest}  {unit}\n"));
        }
        out
    }

    /// Parse a lock file's text.
    pub fn parse(text: &str) -> Result<Lock, String> {
        let mut lines = text.lines();
        match lines.next() {
            Some(HEADER) => {}
            Some(other) => return Err(format!("unknown lock format {other:?}")),
            None => return Err("the lock file is empty".to_string()),
        }
        let mut spec_identity = None;
        let mut spec_version = None;
        let mut root = None;
        let mut units = BTreeMap::new();
        for (number, line) in lines.enumerate() {
            let line_number = number + 2;
            if line.is_empty() {
                continue;
            }
            let Some((key, rest)) = line.split_once(' ') else {
                return Err(format!("line {line_number} has no key"));
            };
            match key {
                "spec-version" => spec_version = Some(rest.to_string()),
                "spec-identity" => spec_identity = Some(rest.to_string()),
                "root" => root = Some(rest.to_string()),
                "unit" => {
                    let Some((digest, unit)) = rest.split_once("  ") else {
                        return Err(format!(
                            "line {line_number} is not `unit <digest>  <identity>`"
                        ));
                    };
                    if digest.len() != 64
                        || !digest
                            .chars()
                            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
                    {
                        return Err(format!(
                            "line {line_number} does not hold a lowercase hex sha256 digest"
                        ));
                    }
                    units.insert(unit.to_string(), digest.to_string());
                }
                other => return Err(format!("line {line_number} has unknown key {other:?}")),
            }
        }
        Ok(Lock {
            spec_identity: spec_identity.ok_or("no spec-identity line")?,
            spec_version: spec_version.ok_or("no spec-version line")?,
            root: root.ok_or("no root line")?,
            units,
        })
    }

    /// Every way `actual` differs from this lock, in a stable order.
    ///
    /// An empty result means the inputs are exactly the locked ones.
    pub fn drift(&self, actual: &Lock) -> Vec<Drift> {
        let mut drift = Vec::new();
        if self.spec_identity != actual.spec_identity {
            drift.push(Drift::Spec {
                locked: self.spec_identity.clone(),
                actual: actual.spec_identity.clone(),
            });
        }
        if self.root != actual.root {
            drift.push(Drift::Root {
                locked: self.root.clone(),
                actual: actual.root.clone(),
            });
        }
        for (unit, locked) in &self.units {
            match actual.units.get(unit) {
                None => drift.push(Drift::Missing { unit: unit.clone() }),
                Some(now) if now != locked => drift.push(Drift::Changed {
                    unit: unit.clone(),
                    locked: locked.clone(),
                    actual: now.clone(),
                }),
                Some(_) => {}
            }
        }
        for (unit, digest) in &actual.units {
            if !self.units.contains_key(unit) {
                drift.push(Drift::Added {
                    unit: unit.clone(),
                    digest: digest.clone(),
                });
            }
        }
        drift
    }
}
