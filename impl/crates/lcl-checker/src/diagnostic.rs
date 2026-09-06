//! The registered `stage: static_or_expression` diagnostics, and the canonical
//! selection contract applied to them.
//!
//! `statuses_and_errors_v0.1.0.json#/errors` registers exactly twelve
//! identifiers at `stage: static_or_expression`. [`StaticError`] mirrors all
//! twelve so the set can be *checked against* the registry rather than trusted,
//! exactly as `lcl_lexer::LexicalError`, `lcl_parser::GrammarError` and
//! `lcl_resolver::ResolutionError` are.
//!
//! ## Static stage versus demanded value
//!
//! Ten of the twelve also appear in
//! `#/diagnostic_selection/expression_demand_resolution`. That map does **not**
//! move them out of this stage: it applies only to "a statically valid
//! expression … actually demanded after the document's preflight checks", and
//! its `exclusion_rule` is categorical — "No source structure, token, name
//! resolution, type-family, signature arity, receiving-type, or required
//! static-validation defect qualifies."
//!
//! So the split this crate implements is by *what is known*, not by identifier:
//!
//! * an operand whose value is statically known is judged here, at this stage;
//! * an operand whose value is only known when the expression is demanded is
//!   recorded as a [`crate::DemandObligation`] for the evaluating layer, and no
//!   diagnostic is invented for it.
//!
//! [`DEFERRED`] names the identifiers no canonical input can produce here
//! because nothing in a source document supplies their trigger statically, each
//! with the layer that owns it. A test asserts none is ever emitted.
//!
//! ## What is not emitted here
//!
//! `error.required.missing` is registered at `stage: execution`, and
//! `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt` is explicit: "An unbound
//! OUTPUT yields MISSING only at an actual bound-value read; its existence as a
//! reference during static checking does not emit error.required.missing." A
//! literal `MISSING` written where a material value is required is a
//! *type-family* defect, not a demanded read, and uses `error.type.mismatch`:
//! `03_TYPES_AND_VALUES/09` gives MISSING "no storable user type", and
//! `03_TYPES_AND_VALUES/10` closes material collection membership against it.

use lcl_diagnostics::Stage;
use lcl_lexer::{Position, Span};
use lcl_resolver::SourceId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One registered static-stage error identifier.
///
/// Ordered by registry identifier, so `stable_order`'s identifier tiebreak is
/// the enum's own ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StaticError {
    /// A LIST or SET contains a member incompatible with its declared item
    /// type.
    CollectionHeterogeneous,
    /// A division denominator is mathematical zero, including inside ROUND.
    NumericDivisionByZero,
    /// An exact quotient has no finite base-10 representation outside the
    /// direct first-argument context of ROUND.
    NumericNonTerminating,
    /// A unit outside its required category, or an exact-unit overload given
    /// different unit identifiers.
    NumericUnitMismatch,
    /// An OBJECT violates required, forbidden, duplicate or typed schema
    /// fields.
    ObjectSchema,
    /// An invocation site violates the operation's target/named-parameter
    /// contract.
    OperationParameter,
    /// No registered operator, function or typed-constructor overload accepts
    /// the operand types or arity.
    OperatorOperand,
    /// A value fails its declared GLOB or REGEX pattern.
    PatternMismatch,
    /// Compiling or matching a pattern exhausts the declared finite limit.
    PatternResourceLimit,
    /// A value is incompatible with its declared type, or with the type or
    /// registered order domain required by its use context.
    TypeMismatch,
    /// A value violates an exact bound.
    ValueOutOfRange,
    /// A required material value or condition is UNKNOWN.
    ValueUnknown,
}

impl StaticError {
    /// Every registered static-stage identifier, in registry order.
    pub const ALL: [StaticError; 12] = [
        StaticError::CollectionHeterogeneous,
        StaticError::NumericDivisionByZero,
        StaticError::NumericNonTerminating,
        StaticError::NumericUnitMismatch,
        StaticError::ObjectSchema,
        StaticError::OperationParameter,
        StaticError::OperatorOperand,
        StaticError::PatternMismatch,
        StaticError::PatternResourceLimit,
        StaticError::TypeMismatch,
        StaticError::ValueOutOfRange,
        StaticError::ValueUnknown,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            StaticError::CollectionHeterogeneous => "error.collection.heterogeneous",
            StaticError::NumericDivisionByZero => "error.numeric.division_by_zero",
            StaticError::NumericNonTerminating => "error.numeric.non_terminating",
            StaticError::NumericUnitMismatch => "error.numeric.unit_mismatch",
            StaticError::ObjectSchema => "error.object.schema",
            StaticError::OperationParameter => "error.operation.parameter",
            StaticError::OperatorOperand => "error.operator.operand",
            StaticError::PatternMismatch => "error.pattern.mismatch",
            StaticError::PatternResourceLimit => "error.pattern.resource_limit",
            StaticError::TypeMismatch => "error.type.mismatch",
            StaticError::ValueOutOfRange => "error.value.out_of_range",
            StaticError::ValueUnknown => "error.value.unknown",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<StaticError> {
        StaticError::ALL
            .into_iter()
            .find(|e| e.as_registry_str() == id)
    }

    /// True for an identifier this milestone deliberately does not decide.
    pub fn is_deferred(self) -> bool {
        DEFERRED.iter().any(|(id, _)| *id == self)
    }

    /// The identifiers this milestone does emit.
    pub fn emitted() -> impl Iterator<Item = StaticError> {
        StaticError::ALL.into_iter().filter(|e| !e.is_deferred())
    }
}

/// Registered static-stage identifiers this milestone does not decide, each
/// with the layer that owns it.
///
/// `error.pattern.resource_limit` is an implementation-capacity outcome of
/// *matching*: `03_TYPES_AND_VALUES/07` raises it when a run "exhausts a
/// declared finite resource limit while compiling or matching". This crate
/// compiles a pattern only to judge a statically known value against a declared
/// constraint, under a fixed bound it never exceeds, so the identifier has no
/// static trigger; the evaluating layer owns it.
pub const DEFERRED: &[(StaticError, &str)] = &[(
    StaticError::PatternResourceLimit,
    "M6 evaluation: pattern compilation and matching under host resource limits",
)];

impl fmt::Display for StaticError {
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

/// One emitted static-stage diagnostic.
///
/// `id`, `source` and `span` are normative: a span is a byte offset into the
/// unit named by `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Registered error identifier.
    pub id: StaticError,
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
    /// The normative stage of every diagnostic this crate emits.
    ///
    /// One classification, not a schedule: an identifier this crate emits is a
    /// static-validation defect, which `expression_demand_resolution`'s
    /// `exclusion_rule` keeps at this stage.
    pub fn stage(&self) -> Stage {
        Stage::StaticOrExpression
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
/// transitively before duplicate suppression and ordering") and
/// `duplicate_rule` ("After supersession, emit one diagnostic for each
/// duplicate_key"), exactly as the earlier stages apply them.
pub(crate) fn select(
    mut raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<StaticError, BTreeSet<StaticError>>,
) -> Vec<Diagnostic> {
    type Key = (StaticError, SourceId, Span, Cause);

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
    raw.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
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
    id: StaticError,
    supersedes: &BTreeMap<StaticError, BTreeSet<StaticError>>,
) -> BTreeSet<StaticError> {
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
