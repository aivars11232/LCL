//! The registered identifiers this runtime can emit, and the selection contract
//! applied to them.
//!
//! ## The first layer allowed to apply `expression_demand_resolution`
//!
//! Every earlier milestone was forbidden to. M5 says why in one sentence of its
//! own documentation: the map's `context` closes the door on preflight with
//! "Preflight-required expression demands retain registered source-validation
//! classification."
//!
//! Step 10 is the other side of that door. `01_FOUNDATION/03`:
//!
//! > an eligible value-domain failure of a statically valid expression demanded
//! > after preflight retains its exact identifier, resolves to execution stage,
//! > and uses status.failed except that required MISSING and UNKNOWN retain
//! > status.blocked.
//!
//! So this layer, and only this layer, may move an eligible identifier's
//! *stage* — never its identifier, event or recoverability, which
//! `resolution_rule` says are "inherited unchanged". [`Diagnostic`] keeps both:
//! `registered_stage` is what the registry declares and never changes;
//! `resolved_stage` is the demand-resolved stage when the map applied. Evidence
//! therefore shows the reclassification instead of hiding it, exactly as
//! `resolution_rule` requires: "Retain the canonical error identifier,
//! registered source-stage metadata, resolved demand stage, and demand locus in
//! evidence."
//!
//! The `exclusion_rule` is enforced structurally: [`DemandContext`] is the only
//! way to request the map, and [`RuntimeError::demand_eligible`] is a closed
//! list read from the registry at load time, so a structure, arity, type-family
//! or name defect has no path to it.
//!
//! ## One identifier this layer mirrors at a stage it cannot move
//!
//! `error.operation.parameter` is registered `static_or_expression`, and M4
//! decides almost all of it: a missing target, a duplicate or unregistered
//! named parameter, a malformed fragment, a declared family outside the row,
//! and a written object literal outside a closed parameter shape.
//!
//! One part of the same contract is not decidable there. `core.read`'s `range`
//! row says "An incompatible unit/representation or wrong key/type uses
//! error.operation.parameter", and whether a unit indexes the representation
//! depends on what the target actually held, which no stage before execution
//! knows. The identifier is mirrored here so that case can be reported under
//! the name the row gives it.
//!
//! What is emitted keeps its registered classification exactly. The stage stays
//! `static_or_expression` and the status stays `status.invalid`, both read from
//! the registry like every other mirrored identifier, because the
//! `exclusion_rule` is explicit that "Discovery time alone never changes
//! classification". This identifier is not in the eligible map, so
//! [`DemandContext`] cannot reach it and its stage is never resolved to
//! execution. Finding a source-stage defect late does not make it a late
//! defect, and the report says which stage owns it rather than relabelling it.
//!
//! ## Two identifiers this layer mirrors and never emits
//!
//! `error.dependency.unsatisfied` and `error.scope.violation` are registered at
//! the execution stage, but `05_SEMANTICS/09` states they are "pre_effect only:
//! dependency availability and effective scope resolve before the first
//! authorized effect". M5 decides both. They are mirrored here so the set can
//! be checked against the whole registered execution stage rather than a subset
//! chosen by this build, and a test asserts the runtime never emits them.

use crate::result::FailurePhase;
use crate::state::{InvocationId, IterationPath};
use lcl_diagnostics::Stage;
use lcl_lexer::{Position, Span};
use lcl_resolver::SourceId;
use std::collections::BTreeMap;
use std::fmt;

/// One registered error identifier this runtime mirrors.
///
/// Ordered by registry identifier, so `stable_order`'s final "error identifier
/// by Unicode scalar value ascending" tiebreak is this enum's own ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuntimeError {
    /// The invoking authority or host cancelled execution.
    Cancelled,
    /// A required dependency is FALSE, MISSING or UNKNOWN. Pre-effect only; M5.
    DependencyUnsatisfied,
    /// A reachable action failed to start, or failed after starting.
    ExecutionAction,
    /// Ordering constraints are violated, a continuation path is absent or
    /// ambiguous, or a requested lifecycle transition is not permitted.
    ExecutionOrder,
    /// A host limitation prevented the operation.
    HostConstraint,
    /// A well-typed registered constructor rejected a dynamically supplied
    /// value under its declared value-domain constraint.
    LiteralInvalid,
    /// A demanded, well-typed division or ROUND quotient has a zero
    /// denominator.
    NumericDivisionByZero,
    /// A demanded, well-typed exact division has no finite base-10 result.
    NumericNonTerminating,
    /// Dynamically supplied MEASURE values violate the exact unit rule.
    NumericUnitMismatch,
    /// A registered SUM, MIN or MAX reduction received an empty material
    /// collection.
    OperatorOperand,
    /// An invocation site got a closed parameter contract wrong.
    ///
    /// Registered at `static_or_expression`, and mirrored here for the part of
    /// that contract no earlier stage can decide. See the note below on why a
    /// source-stage identifier appears in a runtime enum at all.
    OperationParameter,
    /// An operation postcondition was not satisfied.
    OperationPostcondition,
    /// An operation precondition was not satisfied.
    OperationPrecondition,
    /// A dynamically supplied value failed a declared GLOB or REGEX constraint.
    PatternMismatch,
    /// Compiling or matching a demanded pattern exhausted its declared finite
    /// resource limit.
    PatternResourceLimit,
    /// Required access or an effect is unauthorized or prohibited.
    PermissionDenied,
    /// A handler-context target binding admits only REFERENCE[ACTION] and the
    /// owning aggregate is not an ACTION.
    ReferenceKind,
    /// An actually demanded required operand, source, condition or bound-value
    /// read yields MISSING.
    RequiredMissing,
    /// Exactly `1 + LIMIT` attempts were made and every one failed.
    RetryExhausted,
    /// An action targets an entity outside applicable SCOPE. Pre-effect only;
    /// M5.
    ScopeViolation,
    /// A direct FOR EACH over a statically valid SET whose actual members fail
    /// the registered pairwise order-compatibility check at demand.
    TypeMismatch,
    /// A dynamically supplied material value violates an exact registered
    /// bound.
    ValueOutOfRange,
    /// An actually demanded required material value or condition yields
    /// UNKNOWN.
    ValueUnknown,
}

impl RuntimeError {
    /// Every identifier this layer mirrors, in registry order.
    pub const ALL: [RuntimeError; 23] = [
        RuntimeError::Cancelled,
        RuntimeError::DependencyUnsatisfied,
        RuntimeError::ExecutionAction,
        RuntimeError::ExecutionOrder,
        RuntimeError::HostConstraint,
        RuntimeError::LiteralInvalid,
        RuntimeError::NumericDivisionByZero,
        RuntimeError::NumericNonTerminating,
        RuntimeError::NumericUnitMismatch,
        RuntimeError::OperatorOperand,
        RuntimeError::OperationParameter,
        RuntimeError::OperationPostcondition,
        RuntimeError::OperationPrecondition,
        RuntimeError::PatternMismatch,
        RuntimeError::PatternResourceLimit,
        RuntimeError::PermissionDenied,
        RuntimeError::ReferenceKind,
        RuntimeError::RequiredMissing,
        RuntimeError::RetryExhausted,
        RuntimeError::ScopeViolation,
        RuntimeError::TypeMismatch,
        RuntimeError::ValueOutOfRange,
        RuntimeError::ValueUnknown,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            RuntimeError::Cancelled => "error.cancelled",
            RuntimeError::DependencyUnsatisfied => "error.dependency.unsatisfied",
            RuntimeError::ExecutionAction => "error.execution.action",
            RuntimeError::ExecutionOrder => "error.execution.order",
            RuntimeError::HostConstraint => "error.host.constraint",
            RuntimeError::LiteralInvalid => "error.literal.invalid",
            RuntimeError::NumericDivisionByZero => "error.numeric.division_by_zero",
            RuntimeError::NumericNonTerminating => "error.numeric.non_terminating",
            RuntimeError::NumericUnitMismatch => "error.numeric.unit_mismatch",
            RuntimeError::OperatorOperand => "error.operator.operand",
            RuntimeError::OperationParameter => "error.operation.parameter",
            RuntimeError::OperationPostcondition => "error.operation.postcondition",
            RuntimeError::OperationPrecondition => "error.operation.precondition",
            RuntimeError::PatternMismatch => "error.pattern.mismatch",
            RuntimeError::PatternResourceLimit => "error.pattern.resource_limit",
            RuntimeError::PermissionDenied => "error.permission.denied",
            RuntimeError::ReferenceKind => "error.reference.kind",
            RuntimeError::RequiredMissing => "error.required.missing",
            RuntimeError::RetryExhausted => "error.retry.exhausted",
            RuntimeError::ScopeViolation => "error.scope.violation",
            RuntimeError::TypeMismatch => "error.type.mismatch",
            RuntimeError::ValueOutOfRange => "error.value.out_of_range",
            RuntimeError::ValueUnknown => "error.value.unknown",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<RuntimeError> {
        RuntimeError::ALL
            .into_iter()
            .find(|e| e.as_registry_str() == id)
    }

    /// True for an identifier another milestone decides.
    pub fn is_elsewhere(self) -> bool {
        ELSEWHERE.iter().any(|(id, _)| *id == self)
    }

    /// The identifiers this milestone does emit.
    pub fn emitted() -> impl Iterator<Item = RuntimeError> {
        RuntimeError::ALL.into_iter().filter(|e| !e.is_elsewhere())
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// Mirrored identifiers whose decision belongs to another milestone, with the
/// milestone named and the canonical sentence that assigns it.
pub const ELSEWHERE: [(RuntimeError, &str); 2] = [
    (
        RuntimeError::DependencyUnsatisfied,
        "M5 semantic preflight: 05_SEMANTICS/09 states error.dependency.unsatisfied is \
         pre_effect only, because dependency availability resolves before the first \
         authorized effect",
    ),
    (
        RuntimeError::ScopeViolation,
        "M5 semantic preflight: 05_SEMANTICS/09 states error.scope.violation is pre_effect \
         only, because effective scope resolves before the first authorized effect",
    ),
];

/// Why a value was demanded, when the demand is one the eligible map covers.
///
/// Constructing one is the *only* way to ask for
/// `expression_demand_resolution`, so an ineligible defect cannot reach the map
/// by accident. Each variant names an exact trigger sentence from
/// `#/diagnostic_selection/expression_demand_resolution/eligible_errors`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandContext {
    /// "A statically valid expression is actually demanded after the document's
    /// preflight checks, during a reachable invocation, condition,
    /// verification, or completion step."
    ReachableDemand,
}

/// The `cause_identity` component of `duplicate_key`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cause(pub String);

impl Cause {
    pub fn new(text: impl Into<String>) -> Cause {
        Cause(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One emitted runtime diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// A stable emission identity.
    ///
    /// Selection reorders and merges diagnostics, so a position in the final
    /// list is not an identity. An event records the diagnostic it was raised
    /// by, and recovery has to find that diagnostic again *after* selection —
    /// which is only sound if the reference survives reordering.
    pub sequence: usize,
    pub id: RuntimeError,
    /// `errors.<id>.stage`, verbatim from the registry. Never overwritten.
    pub registered_stage: Stage,
    /// The stage `expression_demand_resolution` resolved, when it applied.
    /// `None` means the registered stage governs.
    pub resolved_stage: Option<Stage>,
    /// The source unit this locus belongs to.
    pub source: SourceId,
    /// Exact byte locus inside `source`.
    pub span: Span,
    /// Derived line/column. Presentation only.
    pub position: Position,
    /// `errors.<id>.meaning`, verbatim.
    pub meaning: String,
    /// The resolved default status: the registry's, or the demand-resolution
    /// override where the map applied.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank` for this identifier.
    pub specificity_rank: u64,
    /// `errors.<id>.event`, verbatim. `None` raises no event.
    pub event: Option<String>,
    pub cause: Cause,
    /// The producer that emitted it.
    pub producer: Option<InvocationId>,
    /// Declared execution-path order of the producer: its position in the
    /// plan's canonical order.
    pub producer_path: Option<usize>,
    pub failure_phase: FailurePhase,
    /// Non-normative human detail.
    pub detail: Option<String>,
}

impl Diagnostic {
    /// The stage that governs ordering and classification.
    pub fn stage(&self) -> Stage {
        self.resolved_stage.unwrap_or(self.registered_stage)
    }

    /// The iteration path of the producer, or the root path.
    pub fn iteration(&self) -> IterationPath {
        self.producer
            .as_ref()
            .map(|p| p.iteration.clone())
            .unwrap_or_default()
    }

    /// The retry attempt index of the producer.
    pub fn attempt(&self) -> usize {
        self.producer.as_ref().map(|p| p.attempt).unwrap_or(0)
    }

    /// `duplicate_key`: error identifier, stage, cause identity, canonical
    /// locus, producer path, iteration index and retry-attempt index.
    fn duplicate_key(&self) -> DuplicateKey {
        (
            self.id,
            self.stage(),
            self.cause.clone(),
            self.source.as_str().to_string(),
            self.span.start,
            self.producer_path,
            self.iteration(),
            self.attempt(),
        )
    }

    /// `stable_order`, in the registry's exact key sequence.
    ///
    /// Severity is omitted because its `closed_values` list is `["error"]`: a
    /// one-valued key cannot order anything, and inventing a second severity to
    /// sort by would be inventing language.
    fn order_key(&self) -> OrderKey {
        (
            self.stage().index(),
            // "canonical source byte offset or declared execution-path order
            // ascending". A diagnostic with a producer is ordered by its
            // declared path; a source diagnostic by its byte offset.
            self.producer_path.unwrap_or(usize::MAX),
            self.span.start,
            self.iteration(),
            self.attempt(),
            // "specificity_rank descending": negate so ascending sort matches.
            std::cmp::Reverse(self.specificity_rank),
            self.id,
        )
    }
}

type DuplicateKey = (
    RuntimeError,
    Stage,
    Cause,
    String,
    usize,
    Option<usize>,
    IterationPath,
    usize,
);

type OrderKey = (
    usize,
    usize,
    usize,
    IterationPath,
    usize,
    std::cmp::Reverse<u64>,
    RuntimeError,
);

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}:{}", self.id, self.source, self.position)?;
        if let Some(producer) = &self.producer {
            write!(f, " [{producer}]")?;
        }
        if self.resolved_stage.is_some() {
            write!(
                f,
                " (demand-resolved from {} to {})",
                self.registered_stage.as_registry_str(),
                self.stage().as_registry_str()
            )?;
        }
        Ok(())
    }
}

/// Apply `supersession_rule`, then `duplicate_rule`, then `stable_order`.
///
/// The order of the three is the registry's: "Supersession is applied
/// transitively before duplicate suppression and ordering."
pub fn select(
    raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<RuntimeError, Vec<RuntimeError>>,
) -> Vec<Diagnostic> {
    let superseded = suppress_superseded(raw, supersedes);
    let deduplicated = merge_duplicates(superseded);
    order(deduplicated)
}

/// `supersession_rule`: "A supersedes edge suppresses its target only when both
/// diagnostics describe the same cause at the same canonical locus, stage,
/// producer path, iteration index, and retry-attempt index. It never suppresses
/// an independent occurrence."
fn suppress_superseded(
    raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<RuntimeError, Vec<RuntimeError>>,
) -> Vec<Diagnostic> {
    if supersedes.is_empty() {
        return raw;
    }
    let mut keep = vec![true; raw.len()];
    // Transitive: repeat until no further suppression occurs. The set only
    // shrinks, so this terminates in at most `raw.len()` rounds.
    for _ in 0..=raw.len() {
        let mut changed = false;
        for (i, winner) in raw.iter().enumerate() {
            if !keep[i] {
                continue;
            }
            let Some(targets) = supersedes.get(&winner.id) else {
                continue;
            };
            for (j, loser) in raw.iter().enumerate() {
                if i == j || !keep[j] || !targets.contains(&loser.id) {
                    continue;
                }
                let same_occurrence = winner.cause == loser.cause
                    && winner.source == loser.source
                    && winner.span.start == loser.span.start
                    && winner.stage() == loser.stage()
                    && winner.producer_path == loser.producer_path
                    && winner.iteration() == loser.iteration()
                    && winner.attempt() == loser.attempt();
                if same_occurrence {
                    keep[j] = false;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    raw.into_iter()
        .zip(keep)
        .filter_map(|(d, keep)| keep.then_some(d))
        .collect()
}

/// `duplicate_rule`: "After supersession, emit one diagnostic for each
/// duplicate_key. ... Discovery count and discovery time do not affect output."
fn merge_duplicates(raw: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut seen: Vec<DuplicateKey> = Vec::new();
    let mut out = Vec::new();
    for diagnostic in raw {
        let key = diagnostic.duplicate_key();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(diagnostic);
    }
    out
}

/// `stable_order`, applied by a stable sort so equal keys keep emission order —
/// which for equal keys is also declared order, since the engine walks the plan
/// in its canonical order.
fn order(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by_key(|d| d.order_key());
    diagnostics
}
