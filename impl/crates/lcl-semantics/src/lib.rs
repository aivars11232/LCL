//! # lcl-semantics — deterministic, no-effect semantic preflight
//!
//! Milestone M5.
//!
//! Turns a statically checked program plus explicit invocation data into a
//! fully authorized, dependency-resolved, prevalidated and ordered
//! [`Plan`] — or an exact pre-effect failure. It performs no external effect,
//! reads no ambient state, and starts nothing.
//!
//! This is steps 6 through 9 of
//! `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`:
//!
//! > 6. Establish effective authority, priority, scope, condition contracts,
//! >    and conflicts; evaluate each condition only when its values are
//! >    required.
//! > 7. Resolve input, state, memory, context, default, assumption, and
//! >    dependencies.
//! > 8. Run every selected and applicable VALIDATE check before side effects,
//! >    including optional checks and prerequisites. REQUIRED controls blocking
//! >    on FALSE; check selection and ordering follow check_selection_contract
//! >    in statuses_and_errors_v0.1.0.json.
//! > 9. Finalize and check ordering edges of that resolved candidate graph
//! >    before effects; no check reference or value read adds graph membership
//! >    or edges.
//!
//! Step 10 — evaluating dynamic reachability and executing reachable actions —
//! is the next milestone's and is deliberately absent here.
//!
//! ## The effect boundary
//!
//! This is the last layer before effects exist, so it is the layer where "no
//! effect has happened yet" stops being obvious and starts needing proof. Three
//! structural facts carry that proof:
//!
//! 1. the crate depends on no capability, host or I/O interface, and performs
//!    no filesystem, process, network or clock access of any kind;
//! 2. every external datum arrives through [`Invocation`], which the caller
//!    fills in explicitly and which has no method to enumerate, search or
//!    default a value — the same closure `lcl_resolver::SourceProvider` puts
//!    around source bytes;
//! 3. every diagnostic it emits carries
//!    [`FailurePhase::PreEffect`](diagnostic::FailurePhase::PreEffect), because
//!    no other phase is reachable from here.
//!
//! ## Stage monotonicity
//!
//! [`Preflight::plan`] takes a [`Checked`] and returns [`StageSkipped`] when
//! the static stage did not succeed, so a program that failed an earlier stage
//! has no preflight verdict at all — not a passing one and not a failing one.
//! The signature makes that unskippable rather than merely documented.
//!
//! ## What a result means
//!
//! [`Outcome::Planned`] means **no preflight diagnostic**. It is not a
//! prediction of success: steps 10 through 13 have not run, no action has
//! executed, and nothing here claims one would succeed.
//!
//! ## Guarantees
//!
//! * **Deterministic.** Output is a pure function of the checked program, the
//!   resolved program, the explicit invocation and the loaded contracts. Every
//!   collection iterated is ordered; no `HashMap` appears in this crate.
//! * **Total.** [`Preflight::plan`] returns for every input and never panics.
//!   Every graph walk is iterative, so nesting depth costs heap, not stack.
//! * **Exact.** Every record and diagnostic carries a source identity and a
//!   zero-based byte span into that unit's own bytes.
//! * **No-effect.** No evaluation of an operation, no I/O, no environment.

pub mod authority;
pub mod conflict;
pub mod contracts;
pub mod data;
pub mod diagnostic;
pub mod eval;
pub mod order;
pub mod plan;
pub mod scope;
pub(crate) mod syntax;
pub mod validate;
pub mod value;

pub use authority::{
    Applicability, AuthorityRecord, RuleKind, ScopeRecord, Selector, WorkspaceRecord,
};
pub use contracts::{Contracts, OperationAxes, PreflightContractsError};
pub use diagnostic::{Cause, Diagnostic, FailurePhase, PreflightError, DEFERRED};
pub use plan::{
    Authorization, CheckResult, Edge, EdgeReason, EvidenceRecord, Mode, Origin, Plan, PlanNode,
    Resolution, Selection,
};
pub use value::Value;

use lcl_checker::Checked;
use lcl_lexer::Span;
use lcl_resolver::{Resolved, SourceId};
use std::collections::BTreeMap;
use std::fmt;

/// The explicit data one invocation supplies.
///
/// `05_SEMANTICS/07`: "CONTEXT, MEMORY, and STATE are optional explicit typed
/// sources." `05_SEMANTICS/02`: "Ambient current directory and implied nearby
/// files do not exist in portable LCL." So this type has exactly one way in —
/// the caller naming a declaration and supplying its value — and no way to
/// enumerate, search, glob or default one. A datum the document did not declare
/// cannot enter preflight, because there is no interface through which it
/// could.
///
/// An entry whose declaration the document never declares is never read. It is
/// retained only so a caller can be told it went unused; it can influence
/// nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Invocation {
    supplied: BTreeMap<String, Value>,
}

impl Invocation {
    /// An invocation that supplies nothing.
    ///
    /// This is the honest default: a document whose `INPUT` has no declared
    /// `VALUE` and no supplied value resolves to `MISSING`, and its required
    /// readers block. Nothing is invented to fill the gap.
    pub fn new() -> Invocation {
        Invocation {
            supplied: BTreeMap::new(),
        }
    }

    /// Supply the value of one declared `INPUT`, `STATE`, `MEMORY` or
    /// `CONTEXT`, by its qualified declaration id.
    pub fn with(mut self, id: impl Into<String>, value: Value) -> Invocation {
        self.supplied.insert(id.into(), value);
        self
    }

    /// The value supplied for one declaration id, if any.
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.supplied.get(id)
    }

    /// Every supplied datum, in id order.
    pub fn supplied(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.supplied.iter()
    }

    pub fn len(&self) -> usize {
        self.supplied.len()
    }

    pub fn is_empty(&self) -> bool {
        self.supplied.is_empty()
    }
}

/// The preflight verdict on one program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No preflight diagnostic.
    ///
    /// A statement about steps 6 through 9 only. Execution, verification and
    /// completion have not run.
    Planned,
    /// At least one preflight diagnostic. No plan is produced, and therefore no
    /// effect can be authorized.
    Rejected,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Planned => f.write_str("planned"),
            Outcome::Rejected => f.write_str("rejected"),
        }
    }
}

/// Preflight was not evaluated because an earlier stage failed.
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
            "preflight not evaluated: {} failed the {} stage with {} at byte {}",
            self.source,
            self.stage.as_registry_str(),
            self.primary,
            self.span.start
        )
    }
}

impl std::error::Error for StageSkipped {}

/// A preflight engine bound to loaded contracts.
///
/// Holds no mutable state: the same `Preflight` may plan any number of
/// programs, in any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Preflight<'a> {
    contracts: &'a Contracts,
}

impl<'a> Preflight<'a> {
    pub fn new(contracts: &'a Contracts) -> Self {
        Preflight { contracts }
    }

    pub fn contracts(&self) -> &'a Contracts {
        self.contracts
    }

    /// Run steps 6 through 9 over one statically checked program.
    ///
    /// Total: never panics, for any checked input and any invocation.
    ///
    /// Returns [`StageSkipped`] when the static stage did not succeed, because
    /// preflight is not evaluated for a program that failed an earlier stage.
    pub fn plan(
        &self,
        checked: &Checked,
        resolved: &Resolved,
        invocation: &Invocation,
    ) -> Result<Planned, StageSkipped> {
        // `earliest_stage_rule`: "If any applicable diagnostic remains
        // unhandled, do not evaluate later stages for that failed source unit
        // or invocation path."
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
        if let Some(primary) = checked.primary() {
            return Err(StageSkipped {
                source: primary.source.clone(),
                stage: lcl_diagnostics::Stage::StaticOrExpression,
                primary: primary.id.to_string(),
                span: primary.span,
            });
        }
        // M4 reports two registered identifiers whose stage is earlier than its
        // own. They are real diagnostics of earlier stages, so preflight is not
        // evaluated over a program that has one.
        if let Some(defect) = checked.earlier_stage_defects().first() {
            return Err(StageSkipped {
                source: defect.source.clone(),
                stage: defect.stage,
                primary: defect.identifier.clone(),
                span: defect.span,
            });
        }

        let mut engine = engine::Engine::new(self.contracts, checked, resolved, invocation);
        engine.run();
        Ok(engine.finish())
    }

    /// Evaluate one already-parsed expression to a material value, if it has
    /// one, without planning anything.
    ///
    /// This is the same evaluation `DATA` and `INPUT` resolution performs at
    /// step 7 — literally the same function — exposed so that a caller
    /// supplying an invocation datum can obtain a [`Value`] the way the
    /// language obtains one, rather than by writing a second literal reader
    /// somewhere in a tool.
    ///
    /// It is pure. `05_SEMANTICS/05` and the evaluation path's own contract
    /// hold: it reads only literals in the expression, the resolved values of
    /// declarations, and the registered operator and function tables. It calls
    /// no capability, starts no producer, and reads no `OUTPUT` binding.
    ///
    /// `None` means the expression resolves to no material value here — an
    /// unresolvable reference, a property or index access whose receiver is not
    /// bound before effects, or an operand family the fold does not accept. A
    /// caller decides what to do about that; this function invents nothing.
    ///
    /// `source` is the identity the expression's spans index. An expression
    /// that did not come from a document unit should carry its own identity, so
    /// that no static annotation of a real unit can match it by accident.
    pub fn value_of(
        &self,
        checked: &Checked,
        resolved: &Resolved,
        source: &SourceId,
        expression: &lcl_parser::syntax::Expr,
    ) -> Option<Value> {
        let invocation = Invocation::new();
        let engine = engine::Engine::new(self.contracts, checked, resolved, &invocation);
        eval::literal_value(&engine, source, expression)
    }
}

/// The result of one preflight run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub(crate) root: SourceId,
    pub(crate) plan: Plan,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) unused_invocation_data: Vec<String>,
}

impl Planned {
    /// The root unit's identity.
    pub fn root(&self) -> &SourceId {
        &self.root
    }

    /// The accepted execution plan.
    ///
    /// Present only when [`Planned::outcome`] is [`Outcome::Planned`]: a
    /// rejected preflight authorizes nothing, so it hands the runtime nothing.
    pub fn plan(&self) -> Option<&Plan> {
        match self.outcome() {
            Outcome::Planned => Some(&self.plan),
            Outcome::Rejected => None,
        }
    }

    /// The plan as far as preflight built it, whether or not it was accepted.
    ///
    /// For reports and tests. A runtime must use [`Planned::plan`], which
    /// refuses to hand out a rejected plan.
    pub fn partial_plan(&self) -> &Plan {
        &self.plan
    }

    /// Every emitted diagnostic, in the registry's stable order.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The first diagnostic in stable order, per `primary_rule`.
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.first()
    }

    pub fn outcome(&self) -> Outcome {
        if self.diagnostics.is_empty() {
            Outcome::Planned
        } else {
            Outcome::Rejected
        }
    }

    /// The registered `default_status` of the primary diagnostic.
    ///
    /// `05_SEMANTICS/09`: "The first unhandled diagnostic in stable order
    /// supplies its resolved default_status." `None` means no diagnostic, which
    /// is not a terminal status: preflight succeeding is not an invocation
    /// completing.
    pub fn terminal_status(&self) -> Option<&str> {
        self.primary().map(|d| d.default_status.as_str())
    }

    /// Supplied invocation data whose declaration this document does not
    /// declare, in id order.
    ///
    /// Never read by preflight. Reported so a caller can see that a datum it
    /// supplied went nowhere, rather than having it silently absorbed.
    pub fn unused_invocation_data(&self) -> &[String] {
        &self.unused_invocation_data
    }
}

pub(crate) mod engine;
