//! Source-unit identity and the explicit source-provider contract.
//!
//! `05_SEMANTICS/02_SCOPE_TARGET_WORKSPACE_AND_SOURCE.txt` is categorical:
//! "Ambient current directory and implied nearby files do not exist in portable
//! LCL." A resolver that could enumerate a directory, consult a cache of
//! recently seen files, or fall back to a provider default would make document
//! meaning depend on its surroundings.
//!
//! So this module gives the resolver exactly one way to obtain bytes: ask for
//! one source, named by one `SOURCE` value written in one importing document.
//! [`SourceProvider`] has a single method and **no** enumeration, listing,
//! globbing, search-path or default-unit capability. That is not a convention
//! this crate follows; it is the whole interface, so no implementation of it
//! can offer the resolver anything that was not explicitly referenced.
//!
//! The provider owns the naming space, because only the embedder knows what a
//! `PATH` means in its deployment. `07_VERSIONING_AND_EXTENSIONS/02` fixes the
//! rule the provider must implement — "SOURCE PATH resolves relative only to
//! importing file or explicit WORKSPACE" — and [`SourceRequest::origin`]
//! carries the importing unit's identity so it can. The resolver enforces
//! everything the *language* owns: exact version, checksum, namespace, cycles
//! and identity.

use lcl_lexer::Span;
use std::collections::BTreeMap;
use std::fmt;

/// A stable identity for one source unit.
///
/// Assigned by the caller or the provider, never inferred by the resolver. Two
/// units are the same unit exactly when their identities are equal: that is
/// what makes import cycle detection and single-load memoization well defined
/// without a filesystem notion of sameness.
///
/// Ordering is lexicographic and total, so every collection keyed by a
/// `SourceId` iterates deterministically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(id: impl Into<String>) -> Self {
        SourceId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One source unit: an identity and the exact bytes under it.
///
/// The bytes are the caller's own buffer contents. Every [`Span`] this crate
/// reports is a zero-based byte offset into the bytes of the unit named by the
/// accompanying [`SourceId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUnit {
    id: SourceId,
    bytes: Vec<u8>,
}

impl SourceUnit {
    pub fn new(id: SourceId, bytes: impl Into<Vec<u8>>) -> Self {
        SourceUnit {
            id,
            bytes: bytes.into(),
        }
    }

    pub fn id(&self) -> &SourceId {
        &self.id
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Lowercase hex SHA-256 of the exact bytes.
    ///
    /// `07_VERSIONING_AND_EXTENSIONS/02`: "Checksum form is
    /// algorithm:lowercase_hex; Core 0.1.0 recognizes only sha256." Computed
    /// with the trust root's own implementation, so an import checksum and a
    /// package integrity digest are the same function.
    pub fn digest(&self) -> String {
        lcl_spec::sha256::hex_digest(&self.bytes)
    }
}

/// How one document names another.
///
/// `04_GRAMMAR/05`: "IMPORT/EXTENSION require ID, SOURCE, NAMESPACE, and exact
/// VERSION. URI sources also require CHECKSUM." The `source_expression` value
/// kind admits `PATH`, `URI` or a `REFERENCE`; only the two constructor forms
/// name another source unit, and the resolver passes their exact literal text
/// through without interpreting it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceRef {
    /// `PATH("...")` — resolved relative to the importing unit or an explicit
    /// WORKSPACE, by the provider.
    Path(String),
    /// `URI("...")` — an absolute URI. Its `CHECKSUM` is mandatory and is
    /// verified by the resolver, not by the provider.
    Uri(String),
}

impl SourceRef {
    /// The literal constructor argument, exactly as written in source.
    pub fn text(&self) -> &str {
        match self {
            SourceRef::Path(t) | SourceRef::Uri(t) => t,
        }
    }

    /// `"PATH"` or `"URI"`.
    pub fn constructor(&self) -> &'static str {
        match self {
            SourceRef::Path(_) => "PATH",
            SourceRef::Uri(_) => "URI",
        }
    }
}

impl fmt::Display for SourceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({:?})", self.constructor(), self.text())
    }
}

/// One explicit request for one source unit.
///
/// Every field is derived from source the resolver actually read. There is no
/// "current unit", no implicit base and no request the resolver can synthesize
/// on its own behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRequest {
    /// The unit whose `IMPORT` or `EXTENSION` block asked for this source.
    pub origin: SourceId,
    /// The `SOURCE` value of that block.
    pub reference: SourceRef,
    /// Byte span of the `SOURCE` value inside `origin`, for diagnostics.
    pub span: Span,
}

/// Why a provider could not supply a requested source.
///
/// Every variant maps to `error.import.not_found` — "IMPORT or EXTENSION source
/// cannot be resolved" — with the message retained as non-normative detail. A
/// provider reports failure; it never decides a language outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    message: String,
}

impl LoadError {
    pub fn new(message: impl Into<String>) -> Self {
        LoadError {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LoadError {}

/// The complete interface between the resolver and the outside world.
///
/// One method, one explicit request, one answer. There is deliberately no way
/// to list, search, glob, watch or default a source: an implementation cannot
/// volunteer a unit the source text did not name.
///
/// An implementation must be deterministic for the duration of one resolution:
/// the same request must yield the same identity and the same bytes.
pub trait SourceProvider {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError>;
}

impl<P: SourceProvider + ?Sized> SourceProvider for &P {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError> {
        (**self).load(request)
    }
}

/// A deterministic in-memory provider.
///
/// Holds a fixed set of units and answers a request by matching the reference
/// text against a key. It performs no I/O, which is why it is the provider the
/// conformance tests use: a resolution result obtained through it cannot depend
/// on a filesystem, a clock or an environment.
///
/// Keys are matched exactly, then — for a [`SourceRef::Path`] — as a sibling of
/// the requesting unit. That is the whole of "relative only to importing file";
/// there is no upward search and no fallback.
#[derive(Debug, Clone, Default)]
pub struct MemoryProvider {
    units: BTreeMap<String, Vec<u8>>,
}

impl MemoryProvider {
    pub fn new() -> Self {
        MemoryProvider::default()
    }

    /// Add one unit under an exact key. Replaces any unit already at that key.
    pub fn insert(&mut self, key: impl Into<String>, bytes: impl Into<Vec<u8>>) -> &mut Self {
        self.units.insert(key.into(), bytes.into());
        self
    }

    pub fn with(mut self, key: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        self.insert(key, bytes);
        self
    }

    /// Every key held, in ascending order. For test assertions only: the
    /// resolver never calls this, because [`SourceProvider`] does not expose it.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.units.keys().map(String::as_str)
    }

    /// Resolve a `PATH` against the requesting unit's key.
    ///
    /// Purely lexical: the segment before the last `/` of the origin key is the
    /// origin's container, and the reference is joined to it. `.` and `..`
    /// segments are folded; a `..` that would escape above the root leaves the
    /// join unresolvable rather than reaching for anything outside.
    fn sibling_key(origin: &SourceId, path: &str) -> Option<String> {
        if path.starts_with('/') {
            return normalize(path.trim_start_matches('/'));
        }
        let base = match origin.as_str().rfind('/') {
            Some(i) => &origin.as_str()[..i],
            None => "",
        };
        let joined = if base.is_empty() {
            path.to_string()
        } else {
            format!("{base}/{path}")
        };
        normalize(&joined)
    }
}

/// Fold `.` and `..` segments. Returns `None` when `..` escapes the root.
fn normalize(path: &str) -> Option<String> {
    let mut out: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            other => out.push(other),
        }
    }
    Some(out.join("/"))
}

impl SourceProvider for MemoryProvider {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError> {
        let text = request.reference.text();
        let key = match &request.reference {
            SourceRef::Uri(_) => {
                // A URI names itself. There is no relative form.
                text.to_string()
            }
            SourceRef::Path(path) => match MemoryProvider::sibling_key(&request.origin, path) {
                Some(key) if self.units.contains_key(&key) => key,
                // An exact key is honoured too, so a caller may key its units
                // however it likes as long as the source text names them.
                _ if self.units.contains_key(text) => text.to_string(),
                Some(key) => key,
                None => text.to_string(),
            },
        };
        match self.units.get(&key) {
            Some(bytes) => Ok(SourceUnit::new(SourceId::new(key), bytes.clone())),
            None => Err(LoadError::new(format!(
                "no source unit is registered under {key:?}"
            ))),
        }
    }
}
