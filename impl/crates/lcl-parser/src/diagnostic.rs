//! Grammar and schema diagnostics, and the canonical selection algorithm over
//! them.
//!
//! ## Identifiers are not invented here
//!
//! [`GrammarError`] is a closed enum of the **12** error identifiers that
//! `statuses_and_errors_v0.1.0.json` registers with
//! `"stage": "grammar_or_schema"`. It follows the exact precedent M0 set for
//! `Stage` and M1 set for `LexicalError`: a named enum that is *validated
//! against* the registry rather than trusted. [`crate::Grammar`] refuses to
//! load unless the enum's identifier set equals the registry's
//! `grammar_or_schema` set exactly, so adding, renaming or restaging an error
//! in a future release fails the build's tests rather than silently drifting.
//!
//! Every other field of a diagnostic — meaning, default status, specificity
//! rank, supersession edges — is copied from the registry at load time and is
//! never written into this source.
//!
//! ## Selection
//!
//! [`select`] implements `diagnostic_selection` for one source-validation run,
//! the same one of the two `selection_scope` values M1 implements. Supersession
//! runs first, then duplicate suppression, then `stable_order`; the first
//! surviving diagnostic is primary. The grammar stage adds no producer path,
//! iteration index or retry-attempt index, so those `duplicate_key` components
//! are constant here exactly as they are at the lexical stage.

use lcl_lexer::{Position, Span};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The closed set of grammar-and-schema-stage error identifiers.
///
/// Ordering of the enum is alphabetical by registry identifier, which is also
/// the `stable_order` tiebreak ("error identifier by Unicode scalar value
/// ascending"), so deriving `Ord` gives that tiebreak for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GrammarError {
    BlockConditionalRequirement,
    BlockContext,
    BlockDuplicate,
    BlockField,
    BlockOccurrence,
    BlockRequired,
    FieldCardinality,
    FieldDuplicate,
    FieldForbidden,
    FieldRequired,
    FieldType,
    GrammarInvalid,
}

impl GrammarError {
    /// Every grammar-or-schema error identifier, in registry-identifier order.
    pub const ALL: [GrammarError; 12] = [
        GrammarError::BlockConditionalRequirement,
        GrammarError::BlockContext,
        GrammarError::BlockDuplicate,
        GrammarError::BlockField,
        GrammarError::BlockOccurrence,
        GrammarError::BlockRequired,
        GrammarError::FieldCardinality,
        GrammarError::FieldDuplicate,
        GrammarError::FieldForbidden,
        GrammarError::FieldRequired,
        GrammarError::FieldType,
        GrammarError::GrammarInvalid,
    ];

    /// The registered identifier. Checked against the registry at load time.
    pub fn as_registry_str(self) -> &'static str {
        match self {
            GrammarError::BlockConditionalRequirement => "error.block.conditional_requirement",
            GrammarError::BlockContext => "error.block.context",
            GrammarError::BlockDuplicate => "error.block.duplicate",
            GrammarError::BlockField => "error.block.field",
            GrammarError::BlockOccurrence => "error.block.occurrence",
            GrammarError::BlockRequired => "error.block.required",
            GrammarError::FieldCardinality => "error.field.cardinality",
            GrammarError::FieldDuplicate => "error.field.duplicate",
            GrammarError::FieldForbidden => "error.field.forbidden",
            GrammarError::FieldRequired => "error.field.required",
            GrammarError::FieldType => "error.field.type",
            GrammarError::GrammarInvalid => "error.grammar.invalid",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<GrammarError> {
        GrammarError::ALL
            .iter()
            .copied()
            .find(|e| e.as_registry_str() == id)
    }
}

impl fmt::Display for GrammarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// Why a diagnostic was raised, at the granularity `duplicate_key` calls
/// `cause_identity`.
///
/// Two diagnostics with the same identifier and locus but different causes are
/// independent and are both emitted; the registry's `duplicate_rule` merges
/// only same-cause records.
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

/// One emitted grammar-or-schema diagnostic.
///
/// `id`, `span` and `position` are normative. `meaning`, `default_status` and
/// `specificity_rank` are copied from the canonical registry so a diagnostic is
/// self-describing without a second lookup. `detail` is implementation prose
/// and carries no normative weight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Registered error identifier.
    pub id: GrammarError,
    /// Exact source locus.
    ///
    /// Zero-width where the normative locus is a position rather than a
    /// lexeme. `location_rule` requires exactly that for an omitted field or
    /// child block: "the zero-width byte position at the first following
    /// nonblank line whose indentation is not greater than the parent block
    /// header; when no such line exists, it uses the end-of-file offset equal
    /// to the source byte length."
    pub span: Span,
    /// Derived line/column for the span start.
    pub position: Position,
    /// `errors.<id>.meaning`, verbatim from the registry.
    pub meaning: String,
    /// `errors.<id>.default_status`, verbatim from the registry.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank` for this identifier.
    pub specificity_rank: u64,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: Cause,
    /// Non-normative human detail, e.g. the offending field key.
    pub detail: Option<String>,
}

impl Diagnostic {
    /// The normative stage of every diagnostic this crate can emit.
    pub fn stage(&self) -> lcl_diagnostics::Stage {
        lcl_diagnostics::Stage::GrammarOrSchema
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.id, self.position)?;
        if let Some(detail) = &self.detail {
            write!(f, ": {detail}")?;
        }
        Ok(())
    }
}

/// Apply the canonical selection contract to raw emissions.
///
/// Order is fixed by `supersession_rule` ("Supersession is applied transitively
/// before duplicate suppression and ordering") and `duplicate_rule` ("After
/// supersession, emit one diagnostic for each duplicate_key").
///
/// `supersedes` is the registry's map, already restricted to grammar errors.
pub(crate) fn select(
    mut raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<GrammarError, BTreeSet<GrammarError>>,
) -> Vec<Diagnostic> {
    // 1. Supersession. An edge suppresses its target "only when both
    //    diagnostics describe the same cause at the same canonical locus,
    //    stage, producer path, iteration index, and retry-attempt index". In a
    //    source-validation run there is no producer path, iteration index or
    //    retry index, and the stage is always grammar_or_schema, so the
    //    condition reduces to same cause and same locus.
    let present: BTreeSet<(GrammarError, Span, Cause)> = raw
        .iter()
        .map(|d| (d.id, d.span, d.cause.clone()))
        .collect();
    // Transitive closure of the edges actually present.
    let suppressed: BTreeSet<(GrammarError, Span, Cause)> = present
        .iter()
        .flat_map(|(id, span, cause)| {
            transitive_targets(*id, supersedes)
                .into_iter()
                .map(move |target| (target, *span, cause.clone()))
        })
        .collect();
    raw.retain(|d| !suppressed.contains(&(d.id, d.span, d.cause.clone())));

    // 2. Duplicate suppression on the applicable components of `duplicate_key`.
    let mut seen: BTreeSet<(GrammarError, Span, Cause)> = BTreeSet::new();
    raw.retain(|d| seen.insert((d.id, d.span, d.cause.clone())));

    // 3. `stable_order`. Stage is constant, severity is a closed single value,
    //    and there is no iteration or retry index in a source-validation run,
    //    so the live keys are byte offset ascending, specificity rank
    //    descending, then identifier ascending.
    raw.sort_by(|a, b| {
        a.span
            .start
            .cmp(&b.span.start)
            .then(b.specificity_rank.cmp(&a.specificity_rank))
            .then(a.id.cmp(&b.id))
            .then(a.span.end.cmp(&b.span.end))
            .then(a.cause.cmp(&b.cause))
    });
    raw
}

/// All errors reachable from `id` along `supersedes` edges.
fn transitive_targets(
    id: GrammarError,
    supersedes: &BTreeMap<GrammarError, BTreeSet<GrammarError>>,
) -> BTreeSet<GrammarError> {
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
