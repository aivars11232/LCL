//! Host permission: what the machine allows, decided independently of LCL.
//!
//! ## The gate this module is, and the gate it is not
//!
//! > Host permission does not imply LCL authorization. LCL authorization does
//! > not force host permission. Both gates must pass for an effect.
//!
//! Everything here is the *host* gate. Nothing in this module reads a `REQUIRE`
//! or an `ALLOW`, and nothing in it can be satisfied by one: a document that
//! authorizes writing a path still writes nothing unless the operator granted
//! that path, and a granted path is still not written unless the document
//! authorized it. The two decisions meet in `lcl-stdlib`, and neither is
//! derived from the other.
//!
//! ## Deny by default
//!
//! [`Grants::none`] permits nothing, and every constructor builds up from
//! there. "Unknown/unregistered … fail closed" is the canonical posture for
//! language surface, and the same posture is the only safe one for effects: a
//! capability that has to be named to exist cannot be acquired by an
//! unanticipated code path.
//!
//! ## Containment is lexical here, and verified again at the adapter
//!
//! [`Grants::decide`] normalises `.` and `..` textually before comparing a path
//! against a granted root, so `root/../../etc/passwd` is refused without
//! touching the filesystem. That is necessary but not sufficient — a symlink
//! inside the root can still point outside it — so the real filesystem adapter
//! canonicalises and re-checks before it opens anything. Two checks, because
//! the first one is the only one available when the path does not exist yet.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};

/// One primitive permission question, in host vocabulary only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Grant {
    /// Read the bytes or metadata of one path.
    ReadPath(PathBuf),
    /// Create, change, move or remove one path.
    WritePath(PathBuf),
    /// Run one program.
    RunProgram(String),
    /// Reach one network host. `secure` marks a scheme that requires TLS.
    Network { host: String, secure: bool },
    /// Change installed package state.
    Package,
    /// Obtain inference or generation from a model capability.
    Model,
    /// Obtain an authoritative answer from a person.
    Human,
    /// Read or write the engine's own MEMORY and STATE stores.
    InternalStore,
}

impl fmt::Display for Grant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Grant::ReadPath(path) => write!(f, "read {}", path.display()),
            Grant::WritePath(path) => write!(f, "write {}", path.display()),
            Grant::RunProgram(program) => write!(f, "run {program}"),
            Grant::Network { host, secure } => {
                write!(
                    f,
                    "reach {host} over {}",
                    if *secure { "TLS" } else { "TCP" }
                )
            }
            Grant::Package => f.write_str("change package state"),
            Grant::Model => f.write_str("use a model capability"),
            Grant::Human => f.write_str("ask a person"),
            Grant::InternalStore => f.write_str("use the engine's MEMORY and STATE stores"),
        }
    }
}

/// Why the host will not do something.
///
/// The two variants are canonically different outcomes, not two spellings of
/// one: a refusal is `error.permission.denied`, "Required access or an effect
/// is unauthorized or prohibited", while a limitation is
/// `error.host.constraint`, which "never changes LCL meaning".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The host has the capability and withholds it.
    Denied(String),
    /// The host cannot supply the capability at all.
    Unavailable(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Denied(detail) => f.write_str(detail),
            Refusal::Unavailable(detail) => f.write_str(detail),
        }
    }
}

/// One granted filesystem subtree.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scope {
    pub root: PathBuf,
    pub writable: bool,
}

/// The host's complete permission policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    scopes: Vec<Scope>,
    programs: BTreeSet<String>,
    any_program: bool,
    hosts: BTreeSet<String>,
    any_host: bool,
    /// Whether this host can terminate a TLS connection at all.
    tls: bool,
    packages: bool,
    model: bool,
    human: bool,
    internal_store: bool,
}

impl Grants {
    /// Permit nothing.
    pub fn none() -> Grants {
        Grants::default()
    }

    /// Permit the engine's own MEMORY and STATE stores, and nothing external.
    ///
    /// This is the ordinary starting point: `core.memory_write` and
    /// `core.state_update` change declared LCL state that the engine already
    /// owns, so refusing them by default would make a document that touches no
    /// external resource unrunnable.
    pub fn internal() -> Grants {
        Grants {
            internal_store: true,
            ..Grants::default()
        }
    }

    /// Permit reading one subtree.
    pub fn permit_read(mut self, root: impl Into<PathBuf>) -> Grants {
        self.scopes.push(Scope {
            root: normalize(&root.into()),
            writable: false,
        });
        self
    }

    /// Permit reading and writing one subtree.
    pub fn permit_write(mut self, root: impl Into<PathBuf>) -> Grants {
        self.scopes.push(Scope {
            root: normalize(&root.into()),
            writable: true,
        });
        self
    }

    /// Permit running one program.
    pub fn permit_program(mut self, program: impl Into<String>) -> Grants {
        self.programs.insert(program.into());
        self
    }

    /// Permit running any program. Deliberately explicit.
    pub fn permit_any_program(mut self) -> Grants {
        self.any_program = true;
        self
    }

    /// Permit reaching one network host.
    pub fn permit_network_host(mut self, host: impl Into<String>) -> Grants {
        self.hosts.insert(host.into().to_ascii_lowercase());
        self
    }

    /// Permit reaching any network host. Deliberately explicit.
    pub fn permit_any_network_host(mut self) -> Grants {
        self.any_host = true;
        self
    }

    /// Declare that this host can terminate TLS.
    ///
    /// Left false by a transport that speaks only cleartext, so an `https`
    /// target reports a limitation instead of silently downgrading.
    pub fn permit_tls(mut self) -> Grants {
        self.tls = true;
        self
    }

    pub fn permit_packages(mut self) -> Grants {
        self.packages = true;
        self
    }

    pub fn permit_model(mut self) -> Grants {
        self.model = true;
        self
    }

    pub fn permit_human(mut self) -> Grants {
        self.human = true;
        self
    }

    pub fn permit_internal_store(mut self) -> Grants {
        self.internal_store = true;
        self
    }

    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Decide one primitive permission question.
    pub fn decide(&self, grant: &Grant) -> Result<(), Refusal> {
        match grant {
            Grant::ReadPath(path) => self.path(path, false),
            Grant::WritePath(path) => self.path(path, true),
            Grant::RunProgram(program) => {
                if self.any_program || self.programs.contains(program) {
                    return Ok(());
                }
                Err(Refusal::Denied(format!(
                    "no granted program matches {program}"
                )))
            }
            Grant::Network { host, secure } => {
                let lower = host.to_ascii_lowercase();
                if !self.any_host && !self.hosts.contains(&lower) {
                    return Err(Refusal::Denied(format!(
                        "no granted network host matches {host}"
                    )));
                }
                if *secure && !self.tls {
                    return Err(Refusal::Unavailable(format!(
                        "this host has no TLS transport, so {host} cannot be reached securely"
                    )));
                }
                Ok(())
            }
            Grant::Package => self.flag(self.packages, "package state is not granted"),
            Grant::Model => self.flag(self.model, "no model capability is granted"),
            Grant::Human => self.flag(self.human, "no human responder is granted"),
            Grant::InternalStore => self.flag(
                self.internal_store,
                "the engine's MEMORY and STATE stores are not granted",
            ),
        }
    }

    fn flag(&self, granted: bool, denial: &str) -> Result<(), Refusal> {
        if granted {
            Ok(())
        } else {
            Err(Refusal::Denied(denial.to_string()))
        }
    }

    /// Whether one path is inside a granted scope of sufficient strength.
    fn path(&self, path: &Path, write: bool) -> Result<(), Refusal> {
        let normalized = normalize(path);
        let mut inside_readable = false;
        for scope in &self.scopes {
            if !contains(&scope.root, &normalized) {
                continue;
            }
            if scope.writable || !write {
                return Ok(());
            }
            inside_readable = true;
        }
        if inside_readable {
            return Err(Refusal::Denied(format!(
                "{} is inside a read-only granted scope",
                normalized.display()
            )));
        }
        Err(Refusal::Denied(format!(
            "{} is outside every granted scope",
            normalized.display()
        )))
    }
}

/// Lexically normalise a path: resolve `.` and `..` without touching the disk.
///
/// A leading `..` that would climb above the root is kept, so a relative path
/// that escapes stays visibly escaped rather than silently collapsing to
/// something inside a scope.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out: Vec<OsString> = Vec::new();
    let mut prefix = PathBuf::new();
    let mut absolute = false;
    for component in path.components() {
        match component {
            Component::Prefix(p) => prefix.push(p.as_os_str()),
            Component::RootDir => absolute = true,
            Component::CurDir => {}
            Component::ParentDir => {
                match out.last() {
                    Some(last) if last != ".." => {
                        out.pop();
                    }
                    // Above an absolute root there is nothing to pop, and the
                    // filesystem itself treats `/..` as `/`.
                    _ if absolute => {}
                    _ => out.push(OsString::from("..")),
                }
            }
            Component::Normal(segment) => out.push(segment.to_os_string()),
        }
    }
    let mut result = prefix;
    if absolute {
        result.push(std::path::MAIN_SEPARATOR_STR);
    }
    for segment in out {
        result.push(segment);
    }
    result
}

/// Whether `candidate` is `root` or lies beneath it.
///
/// Both sides must already be normalised. Comparison is by whole components,
/// so `/srv/data-private` is not inside `/srv/data`.
pub fn contains(root: &Path, candidate: &Path) -> bool {
    let mut root_parts = root.components();
    let mut candidate_parts = candidate.components();
    loop {
        match (root_parts.next(), candidate_parts.next()) {
            (None, _) => return true,
            (Some(_), None) => return false,
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => return false,
        }
    }
}
