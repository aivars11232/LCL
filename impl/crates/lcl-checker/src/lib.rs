//! # lcl-checker — deterministic, non-executing static and type checker
//!
//! Milestone M4.
//!
//! Turns a resolved program graph into a statically typed model: every
//! declaration's exact type, every expression's static contract, and the value
//! obligations the evaluating layer must still discharge — or a stable-ordered
//! list of registered `static_or_expression` diagnostics. It demands no runtime
//! value, reads no binding, and performs no external effect.
//!
//! This is step 5 of `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`:
//!
//! > Statically check value families, expression names/arity/types,
//! > constructors, parameters, and schemas without demanding deferred
//! > expression values.
//!
//! Step 6 — effective authority, priority, scope, condition contracts and
//! conflicts — is the next milestone's and is deliberately absent here.
//!
//! ## Authority
//!
//! The vocabulary is not written here. [`Contracts::load`] reads every operator,
//! function and constructor signature, all 39 operation contracts, the
//! registered units, formats and encodings, the ordered-type profile, the
//! numeric promotion table and the three-valued logic tables out of the
//! verified registries, and refuses any package that is not the approved
//! release ([`lcl_spec::Authority::Authoritative`]).
//!
//! ## Static, not evaluated
//!
//! `01_FOUNDATION/03`: "Static checking resolves a reference's declaration and
//! static type; it does not read an OUTPUT before its producer binds it." So
//! this crate answers exactly one question per expression — *what is its static
//! contract* — and never *what is its value*, with one bounded exception the
//! canonical model requires: an expression whose operands are **statically
//! known** (literals, constructors over literals, and `DEFINE kind.constant`
//! values built from them) has a statically knowable value, and the canonical
//! examples pin diagnostics that only that knowledge can produce —
//! `1 / 3` is `error.numeric.non_terminating` and `ROUND(1 / 0, 2)` is
//! `error.numeric.division_by_zero`, both at `status.invalid`, before any
//! effect. That evaluation is pure, total and reads no binding.
//!
//! Where an operand is *not* statically known, no value judgement is made. The
//! obligation is recorded in [`Checked::deferred`] for the layer that demands
//! it, which is what `expression_demand_resolution` describes.
//!
//! ## Stage monotonicity
//!
//! [`Checker::check`] takes a [`Resolved`] and returns [`StageSkipped`] when
//! that resolution did not succeed, so a program that failed an earlier stage
//! has no static verdict at all — not a passing one and not a failing one. The
//! signature makes that unskippable rather than merely documented.
//!
//! ## What a result means
//!
//! [`Outcome::Checked`] means **no static diagnostic**. It is not an acceptance
//! of the program: semantic preflight, validation and execution have not run
//! and nothing here claims they would pass.
//!
//! ## Guarantees
//!
//! * **Deterministic.** Output is a pure function of the resolved program and
//!   the loaded contracts. Every collection iterated is ordered; no `HashMap`
//!   appears in this crate.
//! * **Total.** [`Checker::check`] returns for every input and never panics.
//!   Every tree walk is iterative, so nesting depth costs heap, not stack.
//! * **Exact.** Every annotation and diagnostic carries a source identity and a
//!   zero-based byte span into that unit's own bytes.
//! * **Non-executing.** No evaluation of a binding, no I/O, no environment.

pub mod contracts;
mod declarations;
pub mod diagnostic;
mod expr;
mod numeric;
mod operation;
mod pattern;
mod schema;
pub mod ty;
mod types;

pub use contracts::{
    ConstructorRow, ContractType, Contracts, ContractsLoadError, Designator, FunctionRow, Operand,
    OperationContract, OperatorRow, Overload, ParameterSpec, RegisteredStaticError, ResultContract,
    ResultSpec, TargetSpec,
};
pub use diagnostic::{Cause, Diagnostic, StaticError, DEFERRED};
pub use ty::{EnumDomain, ObjectField, ObjectType, RefTarget, Type, UnitId};

use lcl_lexer::Span;
use lcl_resolver::{Resolved, SourceId};
use std::collections::BTreeMap;
use std::fmt;

/// The static outcome of one expression.
///
/// [`Static::Value`] claims a *type*, never a value. Whether the value is also
/// statically known is a separate question, answered by the absence of a
/// matching [`DemandObligation`].
///
/// Deliberately not a type: three of these outcomes are not types at all, and
/// collapsing them into one would let a later stage read a decision this stage
/// did not make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Static {
    /// A material value of exactly this type.
    Value(Type),
    /// A type designator, legal "only where the receiving field or operation
    /// parameter explicitly requires a type".
    TypeDesignator(Type),
    /// A retained reference identity, per
    /// `types_v0.1.0.json#/reference_context_contract/identity_contexts`.
    Identity(Type),
    /// The `MISSING` sentinel: "no binding or field value exists".
    Missing,
    /// The `UNKNOWN` sentinel: "a binding exists but its value cannot presently
    /// be determined".
    Unknown,
    /// A registered qualified identifier — a format, encoding, kind, mode,
    /// status, event, error or unit — received by a slot whose value kind names
    /// that domain. It is a registered name, not a material value.
    Identifier(String),
    /// Statically well formed, with no static type available at this stage
    /// because the value is a registered declaration field governed by its own
    /// field contract. Claiming a type here would be a guess and rejecting the
    /// expression would be a false diagnostic, so neither is done.
    Opaque,
    /// A diagnostic was emitted here. No type is claimed, so nothing downstream
    /// can cascade off an invented one.
    Rejected,
}

impl Static {
    /// The static type this outcome carries, when it carries one.
    pub fn ty(&self) -> Option<&Type> {
        match self {
            Static::Value(t) | Static::TypeDesignator(t) | Static::Identity(t) => Some(t),
            Static::Missing
            | Static::Unknown
            | Static::Identifier(_)
            | Static::Opaque
            | Static::Rejected => None,
        }
    }

    pub fn is_rejected(&self) -> bool {
        matches!(self, Static::Rejected)
    }
}

impl fmt::Display for Static {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Static::Value(t) => write!(f, "{t}"),
            Static::TypeDesignator(t) => write!(f, "type {t}"),
            Static::Identity(t) => write!(f, "identity {t}"),
            Static::Missing => f.write_str("MISSING"),
            Static::Unknown => f.write_str("UNKNOWN"),
            Static::Identifier(id) => write!(f, "identifier {id}"),
            Static::Opaque => f.write_str("declared field"),
            Static::Rejected => f.write_str("rejected"),
        }
    }
}

/// One statically checked expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    pub source: SourceId,
    pub span: Span,
    pub outcome: Static,
}

/// Why a value check could not be completed statically.
///
/// Each variant names an `expression_demand_resolution` trigger: the expression
/// is statically valid, and only its demanded value can decide the remaining
/// registered constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DemandKind {
    /// A division whose denominator is not statically known:
    /// `error.numeric.division_by_zero` or `error.numeric.non_terminating`.
    DivisionValue,
    /// A `MEASURE` whose exact unit is not statically known:
    /// `error.numeric.unit_mismatch`.
    MeasureUnit,
    /// A declared bound over a value that is not statically known:
    /// `error.value.out_of_range`.
    DeclaredBound,
    /// A declared `GLOB`/`REGEX` constraint over a value that is not statically
    /// known: `error.pattern.mismatch`.
    DeclaredPattern,
    /// A direct `FOR EACH` over a `SET` whose members are not statically known:
    /// `error.type.mismatch` on actual-member order compatibility.
    SetMemberOrder,
    /// A reduction over a collection whose emptiness is not statically known:
    /// `error.operator.operand`.
    NonemptyReduction,
    /// A required material site fed by a value that may be `MISSING` or
    /// `UNKNOWN` only at demand.
    RequiredValue,
    /// A registered constructor whose material value arrives only at demand:
    /// `error.literal.invalid` under `expression_demand_resolution`.
    ConstructorValue,
}

impl DemandKind {
    /// The registered identifier this obligation would raise at demand.
    pub fn identifier(self) -> &'static str {
        match self {
            DemandKind::DivisionValue => "error.numeric.division_by_zero",
            DemandKind::MeasureUnit => "error.numeric.unit_mismatch",
            DemandKind::DeclaredBound => "error.value.out_of_range",
            DemandKind::DeclaredPattern => "error.pattern.mismatch",
            DemandKind::SetMemberOrder => "error.type.mismatch",
            DemandKind::NonemptyReduction => "error.operator.operand",
            DemandKind::RequiredValue => "error.required.missing",
            DemandKind::ConstructorValue => "error.literal.invalid",
        }
    }
}

impl fmt::Display for DemandKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.identifier())
    }
}

/// One value obligation this stage proved it cannot decide, handed to the layer
/// that demands the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandObligation {
    pub source: SourceId,
    pub span: Span,
    pub kind: DemandKind,
    /// Non-normative human detail.
    pub detail: String,
}

/// A registered defect this stage detects whose registered stage is earlier.
///
/// A registered stage is a classification, not a schedule. Two defects are only
/// decidable once every static type is resolved, yet carry an identifier the
/// registry stages earlier:
///
/// * `error.reference.cycle` for a `kind.type` `BASE` chain that resolves to
///   itself — `03_TYPES_AND_VALUES/01` assigns exactly that identifier, and M3
///   checks only the alias domains that resolve to a core identifier;
/// * `error.literal.invalid` for a constructor value-domain constraint M1
///   deliberately left to a later layer, because it "depends on the receiving
///   field and on resolution".
///
/// Reporting them with their own identifier and stage keeps the canonical
/// classification exact. They are kept out of [`Checked::diagnostics`] so no
/// static-stage list ever contains a foreign identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlierStageDefect {
    /// The registered error identifier, verbatim.
    pub identifier: String,
    /// Its registered stage.
    pub stage: lcl_diagnostics::Stage,
    pub source: SourceId,
    pub span: Span,
    pub position: lcl_lexer::Position,
    /// `errors.<id>.default_status`, verbatim from the registry.
    pub default_status: String,
    pub detail: String,
}

impl fmt::Display for EarlierStageDefect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} stage) at {}:{}: {}",
            self.identifier,
            self.stage.as_registry_str(),
            self.source,
            self.position,
            self.detail
        )
    }
}

/// The static-stage verdict on one program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No static diagnostic.
    ///
    /// A statement about the static stage only. Semantic preflight, validation
    /// and execution have not run.
    Checked,
    /// At least one static diagnostic.
    Rejected,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Checked => f.write_str("checked"),
            Outcome::Rejected => f.write_str("rejected"),
        }
    }
}

/// The static stage was not evaluated because an earlier stage failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSkipped {
    /// The unit that failed.
    pub source: SourceId,
    /// Which earlier stage failed.
    pub stage: lcl_diagnostics::Stage,
    /// Registered identifier of that stage's primary diagnostic.
    pub primary: String,
    /// Its locus.
    pub span: Span,
}

impl fmt::Display for StageSkipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "static stage not evaluated: {} failed the {} stage with {} at byte {}",
            self.source,
            self.stage.as_registry_str(),
            self.primary,
            self.span.start
        )
    }
}

impl std::error::Error for StageSkipped {}

/// A checker bound to loaded contracts.
///
/// Holds no mutable state: the same `Checker` may check any number of programs,
/// in any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Checker<'a> {
    contracts: &'a Contracts,
}

impl<'a> Checker<'a> {
    pub fn new(contracts: &'a Contracts) -> Self {
        Checker { contracts }
    }

    pub fn contracts(&self) -> &'a Contracts {
        self.contracts
    }

    /// Statically check one resolved program.
    ///
    /// Total: never panics, for any resolved input.
    ///
    /// Returns [`StageSkipped`] when the resolution stage did not succeed,
    /// because the static stage is not evaluated for a program that failed an
    /// earlier stage.
    pub fn check(&self, resolved: &Resolved) -> Result<Checked, StageSkipped> {
        if let Some((source, failure)) = resolved.stage_failures().next() {
            return Err(StageSkipped {
                source: source.clone(),
                stage: failure.stage,
                primary: failure.primary.clone(),
                span: failure.span,
            });
        }
        if let Some(primary) = resolved.primary() {
            return Err(StageSkipped {
                source: primary.source.clone(),
                stage: lcl_diagnostics::Stage::Resolution,
                primary: primary.id.to_string(),
                span: primary.span,
            });
        }

        let catalog = declarations::catalog(resolved);
        let mut check = expr::Check {
            contracts: self.contracts,
            resolved,
            catalog: &catalog,
            emitter: Emitter::new(self.contracts, resolved),
            declaration_types: BTreeMap::new(),
            constants: BTreeMap::new(),
            schemas: BTreeMap::new(),
            locals: Vec::new(),
            relative_path_allowed: false,
            silent: false,
            raw: Vec::new(),
            annotations: BTreeMap::new(),
            values: BTreeMap::new(),
            field_types: BTreeMap::new(),
            resolving: std::collections::BTreeSet::new(),
            property_depth: 0,
            deferred: Vec::new(),
            earlier: Vec::new(),
        };
        declarations::check_program(&mut check);

        Ok(Checked {
            root: resolved.root().clone(),
            annotations: check.annotations,
            declaration_types: check.declaration_types,
            deferred: check.deferred,
            diagnostics: diagnostic::select(check.raw, self.contracts.supersedes()),
            earlier: check.earlier,
        })
    }
}

/// Builds registered diagnostics against the units they belong to.
///
/// Every diagnostic carries the registry's own `meaning`, `default_status` and
/// `specificity_rank`, so a diagnostic is self-describing without a second
/// lookup and cannot drift from the canonical metadata.
pub(crate) struct Emitter<'a> {
    contracts: &'a Contracts,
    resolved: &'a Resolved,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(contracts: &'a Contracts, resolved: &'a Resolved) -> Self {
        Emitter {
            contracts,
            resolved,
        }
    }

    /// Build a defect for a registered identifier whose stage is earlier than
    /// this one, reading its stage and status from the diagnostic registry.
    ///
    /// Returns `None` when the identifier is not registered, so an unregistered
    /// spelling can never be reported.
    pub(crate) fn earlier_stage(
        &self,
        source: &SourceId,
        span: Span,
        identifier: &str,
        detail: String,
    ) -> Option<EarlierStageDefect> {
        let registered = self.contracts.diagnostics().error(identifier)?;
        let text = self
            .resolved
            .unit(source)
            .map(|unit| unit.source())
            .unwrap_or("");
        Some(EarlierStageDefect {
            identifier: registered.id.clone(),
            stage: registered.stage,
            source: source.clone(),
            span,
            position: diagnostic::position(text, span.start),
            default_status: registered.default_status.clone(),
            detail,
        })
    }

    pub(crate) fn emit(
        &self,
        raw: &mut Vec<Diagnostic>,
        id: StaticError,
        source: &SourceId,
        span: Span,
        cause: &str,
        detail: String,
    ) {
        debug_assert!(
            !id.is_deferred(),
            "{id} is deferred to a later milestone and must not be emitted here"
        );
        let registered = self.contracts.error(id);
        let text = self
            .resolved
            .unit(source)
            .map(|unit| unit.source())
            .unwrap_or("");
        raw.push(Diagnostic {
            id,
            source: source.clone(),
            span,
            position: diagnostic::position(text, span.start),
            meaning: registered.meaning.clone(),
            default_status: registered.default_status.clone(),
            specificity_rank: registered.specificity_rank,
            cause: Cause(cause.to_string()),
            detail: Some(detail),
        });
    }
}

/// The complete result of statically checking one program.
#[derive(Debug)]
pub struct Checked {
    pub(crate) root: SourceId,
    /// Every checked expression, keyed by its exact locus.
    pub(crate) annotations: BTreeMap<(SourceId, Span), Annotation>,
    /// Every declaration's static type, by index into the resolver's
    /// declaration index.
    pub(crate) declaration_types: BTreeMap<usize, Type>,
    pub(crate) deferred: Vec<DemandObligation>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) earlier: Vec<EarlierStageDefect>,
}

impl Checked {
    /// The root unit's identity.
    pub fn root(&self) -> &SourceId {
        &self.root
    }

    /// Every checked expression, in unit and byte order.
    pub fn annotations(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations.values()
    }

    pub fn annotation_count(&self) -> usize {
        self.annotations.len()
    }

    /// The static outcome recorded at one exact locus.
    pub fn annotation(&self, source: &SourceId, span: Span) -> Option<&Annotation> {
        self.annotations.get(&(source.clone(), span))
    }

    /// One declaration's static type, when this stage determined one.
    pub fn declaration_type(&self, declaration: usize) -> Option<&Type> {
        self.declaration_types.get(&declaration)
    }

    pub fn declaration_types(&self) -> impl Iterator<Item = (&usize, &Type)> {
        self.declaration_types.iter()
    }

    /// Every value obligation handed to the demanding layer, in source order.
    pub fn deferred(&self) -> &[DemandObligation] {
        &self.deferred
    }

    /// Every emitted static diagnostic, in `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// `primary_rule`: the first unhandled diagnostic is primary. Nothing is
    /// handled at the static stage, so this is the first in `stable_order`.
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.first()
    }

    /// Registered defects this stage detected whose registered stage is
    /// earlier. See [`EarlierStageDefect`].
    pub fn earlier_stage_defects(&self) -> &[EarlierStageDefect] {
        &self.earlier
    }

    /// The static verdict.
    ///
    /// A cross-stage defect rejects the program too: it is a registered
    /// diagnostic of an earlier stage, and `earliest_stage_rule` does not let a
    /// later stage pass a source that failed an earlier one.
    pub fn outcome(&self) -> Outcome {
        if self.diagnostics.is_empty() && self.earlier.is_empty() {
            Outcome::Checked
        } else {
            Outcome::Rejected
        }
    }

    /// Registered `default_status` of the primary diagnostic, if any.
    ///
    /// An earlier-stage defect takes precedence, because its stage is earlier.
    pub fn terminal_status(&self) -> Option<&str> {
        self.earlier
            .first()
            .map(|d| d.default_status.as_str())
            .or_else(|| self.primary().map(|d| d.default_status.as_str()))
    }
}
