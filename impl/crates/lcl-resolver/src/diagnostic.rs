//! The registered `stage: resolution` diagnostics, and the canonical selection
//! contract applied to them.
//!
//! `statuses_and_errors_v0.1.0.json#/errors` registers exactly fourteen
//! identifiers at `stage: resolution`. [`ResolutionError`] mirrors all
//! fourteen so the set can be checked against the registry rather than trusted,
//! exactly as `lcl_lexer::LexicalError` and `lcl_parser::GrammarError` are.
//! Core 0.3.0 registers four more, the `error.project` identifiers
//! ([`ResolutionError::PROJECT`]); a package registers all four or none, and
//! [`crate::Rules::load`] checks the registered set against exactly that.
//!
//! Two of the fourteen are **not emitted by this milestone**. They are listed in
//! [`DEFERRED`] with the layer that owns them, and no code path constructs them:
//!
//! * `error.conflict.hard` — "applicable hard clauses cannot all be satisfied
//!   after resolution". Deciding it requires effective authority, priority and
//!   applicable-condition contracts, which are step 6 of
//!   `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`, after this stage.
//! * `error.override.invalid` — "lets lower authority defeat higher authority"
//!   is the same step-6 question. This milestone still *binds* `OVERRIDE`
//!   `WINNER` and `LOSER` as `reference(rule_clause)` slots, so an override
//!   naming a nonexistent or wrong-kind clause is caught here as an ordinary
//!   `error.reference.unresolved` or `error.reference.kind`.
//!
//! A registered stage is a classification, not a schedule: `error.execution.order`
//! is registered at `stage: execution` and is likewise decided before effects,
//! by the ordering layer. Neither this crate nor the milestone report claims
//! either verdict.

use lcl_diagnostics::Stage;
use lcl_lexer::{Position, Span};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::source::SourceId;

/// One registered resolution-stage error identifier.
///
/// Ordered by registry identifier, so `stable_order`'s identifier tiebreak is
/// the enum's own ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolutionError {
    /// Applicable hard clauses cannot all be satisfied after resolution.
    /// **Deferred**: see [`DEFERRED`].
    ConflictHard,
    /// An extension violates the definition-only extension contract.
    ExtensionInvalid,
    /// Two declarations define the same ID in one namespace, two imports or
    /// extensions claim the same namespace prefix, or a local declaration ID
    /// occupies an import-owned first identifier segment.
    IdDuplicate,
    /// Imported content does not match CHECKSUM.
    ImportChecksum,
    /// Import graph contains a cycle.
    ImportCycle,
    /// IMPORT or EXTENSION source cannot be resolved.
    ImportNotFound,
    /// Namespace is absent, reserved, or malformed, or a declaration
    /// identifier begins with a reserved namespace.
    NamespaceInvalid,
    /// An ACTION names an operation absent from core and active extensions.
    OperationUndefined,
    /// An override is unresolved, circular, or lets lower authority defeat
    /// higher authority. **Deferred**: see [`DEFERRED`].
    OverrideInvalid,
    /// Core 0.3.0: two PART declarations, or a PART and an IMPORT or
    /// EXTENSION, of one project resolve to the same source unit.
    ProjectPartDuplicate,
    /// Core 0.3.0: a project part does not declare SPECIFICATION KIND equal to
    /// the project part kind its PART declares.
    ProjectPartKind,
    /// Core 0.3.0: the source of a required project part, or of an optional
    /// part whose source is not absent, cannot be obtained.
    ProjectPartMissing,
    /// Core 0.3.0: a kind.project document occurs other than as the evaluation
    /// root, or a project part other than as a part of the root's project.
    ProjectPlacement,
    /// A prohibited dependency cycle.
    ReferenceCycle,
    /// A reference resolves to a declaration kind illegal in that field.
    ReferenceKind,
    /// REF or an alias BASE does not resolve to exactly one declaration.
    ReferenceUnresolved,
    /// Imported/extended specification version differs from requested version.
    VersionMismatch,
    /// Declared exact LCL version is unsupported.
    VersionUnsupported,
}

impl ResolutionError {
    /// Every resolution identifier this build mirrors, in registry order.
    pub const ALL: [ResolutionError; 18] = [
        ResolutionError::ConflictHard,
        ResolutionError::ExtensionInvalid,
        ResolutionError::IdDuplicate,
        ResolutionError::ImportChecksum,
        ResolutionError::ImportCycle,
        ResolutionError::ImportNotFound,
        ResolutionError::NamespaceInvalid,
        ResolutionError::OperationUndefined,
        ResolutionError::OverrideInvalid,
        ResolutionError::ProjectPartDuplicate,
        ResolutionError::ProjectPartKind,
        ResolutionError::ProjectPartMissing,
        ResolutionError::ProjectPlacement,
        ResolutionError::ReferenceCycle,
        ResolutionError::ReferenceKind,
        ResolutionError::ReferenceUnresolved,
        ResolutionError::VersionMismatch,
        ResolutionError::VersionUnsupported,
    ];

    /// The four Core 0.3.0 project identifiers. A package registers all of
    /// them or none: Core 0.1.0 and 0.2.0 register none.
    pub const PROJECT: [ResolutionError; 4] = [
        ResolutionError::ProjectPartDuplicate,
        ResolutionError::ProjectPartKind,
        ResolutionError::ProjectPartMissing,
        ResolutionError::ProjectPlacement,
    ];

    /// True for a Core 0.3.0 project identifier.
    pub fn is_project(self) -> bool {
        ResolutionError::PROJECT.contains(&self)
    }

    pub fn as_registry_str(self) -> &'static str {
        match self {
            ResolutionError::ConflictHard => "error.conflict.hard",
            ResolutionError::ExtensionInvalid => "error.extension.invalid",
            ResolutionError::IdDuplicate => "error.id.duplicate",
            ResolutionError::ImportChecksum => "error.import.checksum",
            ResolutionError::ImportCycle => "error.import.cycle",
            ResolutionError::ImportNotFound => "error.import.not_found",
            ResolutionError::NamespaceInvalid => "error.namespace.invalid",
            ResolutionError::OperationUndefined => "error.operation.undefined",
            ResolutionError::OverrideInvalid => "error.override.invalid",
            ResolutionError::ProjectPartDuplicate => "error.project.part_duplicate",
            ResolutionError::ProjectPartKind => "error.project.part_kind",
            ResolutionError::ProjectPartMissing => "error.project.part_missing",
            ResolutionError::ProjectPlacement => "error.project.placement",
            ResolutionError::ReferenceCycle => "error.reference.cycle",
            ResolutionError::ReferenceKind => "error.reference.kind",
            ResolutionError::ReferenceUnresolved => "error.reference.unresolved",
            ResolutionError::VersionMismatch => "error.version.mismatch",
            ResolutionError::VersionUnsupported => "error.version.unsupported",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<ResolutionError> {
        ResolutionError::ALL
            .into_iter()
            .find(|e| e.as_registry_str() == id)
    }

    /// True for an identifier this milestone deliberately does not decide.
    pub fn is_deferred(self) -> bool {
        DEFERRED.iter().any(|(id, _)| *id == self)
    }

    /// The identifiers this milestone does emit.
    pub fn emitted() -> impl Iterator<Item = ResolutionError> {
        ResolutionError::ALL
            .into_iter()
            .filter(|e| !e.is_deferred())
    }
}

/// Registered resolution identifiers this milestone does not decide, each with
/// the layer that owns it.
///
/// Both are step-6 determinations in
/// `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`: "Establish effective
/// authority, priority, scope, condition contracts, and conflicts."
pub const DEFERRED: &[(ResolutionError, &str)] = &[
    (
        ResolutionError::ConflictHard,
        "M5 semantic preflight: effective authority, priority and conflict resolution",
    ),
    (
        ResolutionError::OverrideInvalid,
        "M5 semantic preflight: override chains and authority defeat",
    ),
];

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// Why a diagnostic was raised, at the granularity `duplicate_key` calls
/// `cause_identity`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cause(pub(crate) String);

impl Cause {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One emitted resolution diagnostic.
///
/// `id`, `source` and `span` are normative: a span is a byte offset into the
/// unit named by `source`, never into "the" document. Multi-source resolution
/// is the first stage where that distinction has teeth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Registered error identifier.
    pub id: ResolutionError,
    /// The source unit this locus belongs to.
    pub source: SourceId,
    /// Exact locus inside `source`.
    pub span: Span,
    /// Derived line/column for the span start. Presentation only.
    pub position: Position,
    /// `errors.<id>.meaning`, verbatim from the registry.
    pub meaning: String,
    /// `errors.<id>.default_status`, verbatim from the registry.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank` for this identifier.
    pub specificity_rank: u64,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: Cause,
    /// Non-normative human detail.
    pub detail: Option<String>,
}

impl Diagnostic {
    /// The normative stage of every diagnostic this crate can emit.
    pub fn stage(&self) -> Stage {
        Stage::Resolution
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.id, self.source, self.position)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

/// Apply the canonical selection contract to raw emissions.
///
/// The order is fixed by `supersession_rule` ("Supersession is applied
/// transitively before duplicate suppression and ordering") and `duplicate_rule`
/// ("After supersession, emit one diagnostic for each duplicate_key"), exactly
/// as the lexical and grammar stages apply them.
///
/// `stable_order` sorts by "source unit, then byte offset ascending, then
/// specificity descending, then identifier". A single-unit stage could ignore
/// the first key; this one cannot, so units sort by identity first and the
/// result is independent of the order in which units were loaded.
///
/// A Core 0.3.0 project evaluation supplies `project_rank`: "a project
/// evaluation compares source units in project source order before byte
/// offsets" (`block_schemas#/project_contract/order`), so a file's name never
/// decides which diagnostic is primary. Every other evaluation keeps the
/// identity order.
pub(crate) fn select(
    mut raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<ResolutionError, BTreeSet<ResolutionError>>,
    project_rank: Option<&BTreeMap<SourceId, usize>>,
) -> Vec<Diagnostic> {
    type Key = (ResolutionError, SourceId, Span, Cause);

    // 1. Supersession, transitively, over identical (locus, cause) pairs.
    let present: BTreeSet<Key> = raw
        .iter()
        .map(|d| (d.id, d.source.clone(), d.span, d.cause.clone()))
        .collect();
    let suppressed: BTreeSet<Key> = present
        .iter()
        .flat_map(|(id, source, span, cause)| {
            transitive_targets(*id, supersedes)
                .into_iter()
                .map(move |target| (target, source.clone(), *span, cause.clone()))
        })
        .collect();
    raw.retain(|d| !suppressed.contains(&(d.id, d.source.clone(), d.span, d.cause.clone())));

    // 2. Duplicate suppression on the applicable components of `duplicate_key`.
    let mut seen: BTreeSet<Key> = BTreeSet::new();
    raw.retain(|d| seen.insert((d.id, d.source.clone(), d.span, d.cause.clone())));

    // 3. `stable_order`.
    let unit_order = |a: &SourceId, b: &SourceId| match project_rank {
        Some(rank) => {
            let of = |s: &SourceId| rank.get(s).copied().unwrap_or(usize::MAX);
            of(a).cmp(&of(b)).then(a.cmp(b))
        }
        None => a.cmp(b),
    };
    raw.sort_by(|a, b| {
        unit_order(&a.source, &b.source)
            .then(a.span.start.cmp(&b.span.start))
            .then(b.specificity_rank.cmp(&a.specificity_rank))
            .then(a.id.cmp(&b.id))
            .then(a.span.end.cmp(&b.span.end))
            .then(a.cause.cmp(&b.cause))
    });
    raw
}

/// All errors reachable from `id` along `supersedes` edges.
fn transitive_targets(
    id: ResolutionError,
    supersedes: &BTreeMap<ResolutionError, BTreeSet<ResolutionError>>,
) -> BTreeSet<ResolutionError> {
    let mut out = BTreeSet::new();
    let mut stack = vec![id];
    while let Some(current) = stack.pop() {
        let Some(targets) = supersedes.get(&current) else {
            continue;
        };
        for target in targets {
            if out.insert(*target) {
                stack.push(*target);
            }
        }
    }
    out
}

/// Derive a human-facing position for a byte offset in `text`.
pub(crate) fn position(text: &str, offset: usize) -> Position {
    let mut line = 1u32;
    let mut column = 1u32;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    Position {
        offset,
        line,
        column,
    }
}
