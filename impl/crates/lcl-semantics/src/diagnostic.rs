//! The registered identifiers this preflight layer can emit, and the canonical
//! selection contract applied to them.
//!
//! ## A mixed-stage layer, on purpose
//!
//! M1 through M4 each own exactly one registered stage. This layer does not,
//! and pretending otherwise would be a false claim. Canonical processing steps
//! 6 through 9 are one no-effect preflight, but the diagnostics those steps
//! produce carry three different registered stages:
//!
//! * `resolution` — `error.conflict.hard`, `error.override.invalid` and
//!   `error.reference.cycle`. The resolver (M3) reports these two by name as
//!   "DEFERRED to M5", because effective authority is not known until the whole
//!   rule set is resolved;
//! * `validation` — `error.validation.failed` and
//!   `error.determinism.mismatch`, the entire registered validation stage;
//! * `execution` and `static_or_expression` — `error.execution.order`,
//!   `error.scope.violation`, `error.dependency.unsatisfied`,
//!   `error.permission.denied`, `error.required.missing`,
//!   `error.value.unknown` and `error.value.out_of_range`, all decided
//!   **before the first side effect**.
//!
//! `05_SEMANTICS/09` is explicit that the last group is pre-effect work:
//! "error.dependency.unsatisfied and error.scope.violation are pre_effect only:
//! dependency availability and effective scope resolve before the first
//! authorized effect." Decision witness `CLOSURE-063` pins the same thing for
//! ordering: "Graph construction yields error.execution.order while the
//! invocation is ready. Transition to status.failed is permitted before
//! effects; failure_phase remains producer-relative pre_effect."
//!
//! A registered stage is therefore a *classification*, never a schedule — the
//! distinction M4 already made with `lcl_checker::EarlierStageDefect`. Every
//! [`Diagnostic`] here carries the registry's own stage verbatim, so nothing
//! downstream has to guess which stage an identifier belongs to.
//!
//! ## Preflight demands keep their registered classification
//!
//! `#/diagnostic_selection/expression_demand_resolution` moves an eligible
//! identifier to the execution stage only when "a statically valid expression
//! is actually demanded **after** the document's preflight checks", and its
//! `context` closes the door on this layer in one sentence:
//!
//! > Preflight-required expression demands retain registered source-validation
//! > classification.
//!
//! So this layer never applies that map. A value demanded by a selected
//! pre-effect `VALIDATE`, by a rule condition or by dependency resolution keeps
//! its registered stage and its registered `default_status` — which is why
//! `error.required.missing` and `error.value.unknown` still carry
//! `status.blocked` here without any override being applied.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_lexer::{Position, Span};
use lcl_resolver::SourceId;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// One registered error identifier this layer can emit.
///
/// Mirrored so the set can be *checked against* the registry rather than
/// trusted, exactly as `lcl_lexer::LexicalError`, `lcl_parser::GrammarError`,
/// `lcl_resolver::ResolutionError` and `lcl_checker::StaticError` are. Ordered
/// by registry identifier, so `stable_order`'s identifier tiebreak is the
/// enum's own ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PreflightError {
    /// Applicable hard clauses cannot all be satisfied after resolution.
    ConflictHard,
    /// A required dependency is FALSE, MISSING, or UNKNOWN.
    DependencyUnsatisfied,
    /// A `kind.operation` definition declares `DETERMINISTIC TRUE`, but its
    /// fully resolved operation or selected profile set is nondeterministic.
    DeterminismMismatch,
    /// Ordering constraints form a cycle or are violated, a required
    /// continuation path is absent or ambiguous, or a requested lifecycle
    /// transition is not permitted.
    ExecutionOrder,
    /// An OVERRIDE does not name an exact conflicting pair, or its winner has
    /// lower authority than its loser.
    OverrideInvalid,
    /// Required access or an effect is unauthorized or prohibited.
    PermissionDenied,
    /// A reference chain, here a check-prerequisite chain, resolves to itself.
    ReferenceCycle,
    /// A required value, source, output, evidence item, or declaration is
    /// MISSING at a pre-effect demand point.
    RequiredMissing,
    /// An action targets an entity outside applicable SCOPE.
    ScopeViolation,
    /// A required VALIDATE assertion is FALSE.
    ValidationFailed,
    /// A value violates an exact bound; here, a resolved WORKSPACE escape.
    ValueOutOfRange,
    /// A required material value or condition is UNKNOWN at a pre-effect demand
    /// point.
    ValueUnknown,
}

impl PreflightError {
    /// Every identifier this layer can emit, in registry order.
    pub const ALL: [PreflightError; 12] = [
        PreflightError::ConflictHard,
        PreflightError::DependencyUnsatisfied,
        PreflightError::DeterminismMismatch,
        PreflightError::ExecutionOrder,
        PreflightError::OverrideInvalid,
        PreflightError::PermissionDenied,
        PreflightError::ReferenceCycle,
        PreflightError::RequiredMissing,
        PreflightError::ScopeViolation,
        PreflightError::ValidationFailed,
        PreflightError::ValueOutOfRange,
        PreflightError::ValueUnknown,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            PreflightError::ConflictHard => "error.conflict.hard",
            PreflightError::DependencyUnsatisfied => "error.dependency.unsatisfied",
            PreflightError::DeterminismMismatch => "error.determinism.mismatch",
            PreflightError::ExecutionOrder => "error.execution.order",
            PreflightError::OverrideInvalid => "error.override.invalid",
            PreflightError::PermissionDenied => "error.permission.denied",
            PreflightError::ReferenceCycle => "error.reference.cycle",
            PreflightError::RequiredMissing => "error.required.missing",
            PreflightError::ScopeViolation => "error.scope.violation",
            PreflightError::ValidationFailed => "error.validation.failed",
            PreflightError::ValueOutOfRange => "error.value.out_of_range",
            PreflightError::ValueUnknown => "error.value.unknown",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<PreflightError> {
        PreflightError::ALL
            .into_iter()
            .find(|e| e.as_registry_str() == id)
    }

    /// True for an identifier this milestone deliberately does not decide.
    pub fn is_deferred(self) -> bool {
        DEFERRED.iter().any(|(id, _)| *id == self)
    }

    /// The identifiers this milestone does emit.
    pub fn emitted() -> impl Iterator<Item = PreflightError> {
        PreflightError::ALL.into_iter().filter(|e| !e.is_deferred())
    }
}

/// Identifiers in this layer's mirrored set that a later milestone decides.
///
/// `error.determinism.mismatch` means "A kind.operation definition declares
/// DETERMINISTIC TRUE, but its **fully resolved operation or selected profile
/// set** is nondeterministic", and neither half is decidable before effects:
///
/// * a custom operation "declares its complete axis contract in its own DEFINE
///   block and selects no implementation profile", and `determinism_identity`
///   is stated over "identical declared inputs, **dependency snapshots**,
///   selected profile-role bindings, and implementation versions". Because the
///   dependency is snapshotted, even a `model` or `human` dependency can
///   satisfy it — so no `DEFINE` is self-contradictory on its own, and reading
///   one as such would invent a rule the language does not have;
/// * the selected profile set does not exist yet. `implementation_profile`
///   requires "the exact operation identifier, profile role, target or address
///   class, arguments, implementation identifier, and implementation version"
///   to "select exactly one immutable profile for each role before effects",
///   and implementations are supplied by the capability layer.
///
/// So it is deferred by name, exactly as M3 deferred `error.conflict.hard` to
/// this milestone, rather than being claimed and never triggered.
pub const DEFERRED: &[(PreflightError, &str)] = &[(
    PreflightError::DeterminismMismatch,
    "M7 capability kernel: implementation-profile selection supplies the resolved determinism category",
)];

impl fmt::Display for PreflightError {
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

/// The registry metadata of one mirrored identifier, read at load time.
#[derive(Debug, Clone)]
pub struct RegisteredError {
    pub id: PreflightError,
    /// `errors.<id>.stage`, verbatim. Not assumed by this crate.
    pub stage: Stage,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    /// `errors.<id>.event`, verbatim: the event a surviving diagnostic raises.
    pub event: Option<String>,
    pub recoverable: bool,
    pub supersedes: BTreeSet<PreflightError>,
}

/// One emitted preflight diagnostic.
///
/// `id`, `stage`, `source` and `span` are normative: a span is a byte offset
/// into the unit named by `source`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Registered error identifier.
    pub id: PreflightError,
    /// The identifier's **registered** stage, copied from the registry. This
    /// layer spans three of them and never overwrites one.
    pub stage: Stage,
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
    /// `errors.<id>.event`, verbatim. `None` means the diagnostic raises no
    /// event and cannot select a declared recovery handler.
    pub event: Option<String>,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: Cause,
    /// The declared execution-path order of the producer, when this diagnostic
    /// belongs to a graph node. `stable_order` sorts by "canonical source byte
    /// offset **or** declared execution-path order ascending".
    pub producer_path: Option<usize>,
    /// Non-normative human detail.
    pub detail: Option<String>,
    /// The failure phase, which is `pre_effect` for every diagnostic this layer
    /// emits: no effect can have begun before or during preflight.
    pub failure_phase: FailurePhase,
}

/// `05_SEMANTICS/09` failure phase. This layer only ever produces one value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FailurePhase {
    /// The producer failed before any concrete effect began.
    PreEffect,
}

impl FailurePhase {
    pub fn as_registry_str(self) -> &'static str {
        match self {
            FailurePhase::PreEffect => "pre_effect",
        }
    }
}

impl fmt::Display for FailurePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} stage) at {}:{}",
            self.id,
            self.stage.as_registry_str(),
            self.source,
            self.position
        )?;
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
///
/// `stable_order` begins with "stage_order ascending", which matters here and
/// did not in the single-stage layers: a resolution-stage conflict sorts before
/// a validation-stage failure regardless of byte offset.
pub(crate) fn select(
    mut raw: Vec<Diagnostic>,
    supersedes: &BTreeMap<PreflightError, BTreeSet<PreflightError>>,
) -> Vec<Diagnostic> {
    type Key = (PreflightError, SourceId, Span, Cause);

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
        a.stage
            .index()
            .cmp(&b.stage.index())
            .then(a.source.cmp(&b.source))
            .then(a.producer_path.cmp(&b.producer_path))
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
    id: PreflightError,
    supersedes: &BTreeMap<PreflightError, BTreeSet<PreflightError>>,
) -> BTreeSet<PreflightError> {
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

/// Read one mirrored identifier's metadata out of the registry.
///
/// Returns `None` when the identifier is not registered, so an unregistered
/// spelling can never be reported.
pub(crate) fn registered(
    registry: &DiagnosticRegistry,
    id: PreflightError,
    default_rank: u64,
    rank_overrides: &BTreeMap<String, u64>,
    supersede_overrides: &BTreeMap<String, BTreeSet<PreflightError>>,
) -> Option<RegisteredError> {
    let def = registry.error(id.as_registry_str())?;
    Some(RegisteredError {
        id,
        stage: def.stage,
        meaning: def.meaning.clone(),
        default_status: def.default_status.clone(),
        specificity_rank: *rank_overrides
            .get(id.as_registry_str())
            .unwrap_or(&default_rank),
        event: def.event.clone(),
        recoverable: def.recoverable_with_declared_handler,
        supersedes: supersede_overrides
            .get(id.as_registry_str())
            .cloned()
            .unwrap_or_default(),
    })
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
