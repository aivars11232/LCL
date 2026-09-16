//! The execution plan: the immutable artifact preflight hands the runtime.
//!
//! ## What a plan is for
//!
//! `01_FOUNDATION/03` puts a hard line between step 9 and step 10. Everything
//! before the line resolves meaning; everything after it acts. The plan is that
//! line made into a data structure: it carries every decision steps 6 through 9
//! reached, so the runtime never has to reinterpret source to discover an
//! authority, a scope, a resolved value, a check outcome or an ordering edge.
//!
//! A plan therefore contains **no expression to re-derive a decision from**. It
//! contains decisions.
//!
//! ## Determinism
//!
//! Every collection here is a `Vec` in a canonical order or a `BTreeMap`. Node
//! indexes are positions in [`Plan::nodes`], which is built by walking the
//! resolver's candidate graph in its own canonical child order, so two runs
//! over the same bytes produce identical indexes. Nothing is keyed by a hash,
//! and nothing is ordered by discovery.
//!
//! ## What a plan is not
//!
//! A plan is not a promise of success. It means preflight found no reason to
//! refuse. Steps 10 through 13 have not run.

use crate::value::Value;
use lcl_lexer::Span;
use lcl_resolver::{NodeKind, SourceId};
use std::collections::BTreeMap;
use std::fmt;

/// The declared execution mode of a container.
///
/// `block_schemas#/execution_graph_contract/ordering`: "PHASE and SEQUENCE use
/// declared MODE, defaulting to mode.sequential."
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    Sequential,
    Parallel,
}

impl Mode {
    pub fn as_registry_str(self) -> &'static str {
        match self {
            Mode::Sequential => "mode.sequential",
            Mode::Parallel => "mode.parallel",
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// Why an ordering edge exists.
///
/// Kept distinct because the contract treats them differently: "BEFORE and
/// AFTER add edges and never reverse a sequential edge."
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeReason {
    /// "Sequential lexical order contributes required predecessor edges."
    Sequential,
    /// An explicit `BEFORE` field on the successor's declaration.
    Before,
    /// An explicit `AFTER` field on the predecessor's declaration.
    After,
}

impl EdgeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeReason::Sequential => "sequential",
            EdgeReason::Before => "before",
            EdgeReason::After => "after",
        }
    }
}

impl fmt::Display for EdgeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One required predecessor edge: `from` must complete before `to` begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub reason: EdgeReason,
}

/// The authorization decision for one planned `ACTION`.
///
/// `05_SEMANTICS/03`: "A reachable required ACTION authorizes exactly its
/// declared OPERATION, TARGET, PARAMETER values, OUTPUT, and necessary direct
/// effects." The plan therefore carries the exact authorized shape, not a
/// permission to look one up later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorization {
    /// The registered or custom operation identifier.
    pub operation: String,
    /// The resolved target, when the action declares one.
    pub target: Option<String>,
    /// The effective scope this action acts within, by scope declaration id.
    pub scope: Option<String>,
    /// Rule declarations that authorized it, by declaration id, in canonical
    /// order. An empty list means the action's own requirement authorized it.
    pub permitted_by: Vec<String>,
    /// `FORBID` declarations that matched and were defeated by an exact
    /// `OVERRIDE`, by declaration id.
    pub overridden: Vec<String>,
}

/// One node of the finalized execution plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanNode {
    /// The corresponding index in the resolver's `CandidateGraph`. Preflight
    /// adds no member and removes none: "no check reference or value read adds
    /// graph membership or edges."
    pub candidate: usize,
    pub kind: NodeKind,
    pub source: SourceId,
    /// The declaring block, e.g. `ACTION`, or the control form `IF` /
    /// `FOR EACH`.
    pub block: String,
    pub span: Span,
    /// The activated declaration's index in the resolver's declaration index.
    pub declaration: Option<usize>,
    /// The activated declaration's qualified id.
    pub id: Option<String>,
    pub parent: Option<usize>,
    /// Child plan-node indexes, in canonical child order.
    pub children: Vec<usize>,
    /// The container's declared or defaulted execution mode.
    pub mode: Mode,
    /// The resolved `REQUIRED` contract of this unit.
    pub required: bool,
    /// The authorization decision, for nodes that act.
    pub authorization: Option<Authorization>,
}

/// One resolved invocation datum, and where its value came from.
///
/// `05_SEMANTICS/06` fixes the order and this record names which step produced
/// the value, because "No silent guessing is permitted" is only checkable if
/// the provenance is recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The declaration this resolves.
    pub declaration: usize,
    pub id: String,
    pub block: String,
    pub source: SourceId,
    pub span: Span,
    pub origin: Origin,
    pub value: Value,
    /// The written `VALUE` or `DEFAULT` that `origin` names is an expression
    /// this layer could not decide before effects, so `value` is the UNKNOWN
    /// placeholder "value exists but cannot be established" rather than a
    /// decided value. The layer that demands it evaluates that expression.
    pub undecided: bool,
}

/// Which step of the canonical resolution order supplied a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// 1. explicit VALUE.
    DeclaredValue,
    /// 2. resolved SOURCE/INPUT/STATE/MEMORY/CONTEXT.
    Supplied,
    /// 3. DEFAULT, only for MISSING.
    Default,
    /// 4. applicable explicit ASSUME.
    Assumption,
    /// 6. optional item remains absent.
    Absent,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::DeclaredValue => "declared value",
            Origin::Supplied => "supplied source",
            Origin::Default => "default",
            Origin::Assumption => "assumption",
            Origin::Absent => "absent",
        }
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The outcome of one selected pre-effect `VALIDATE` check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckResult {
    pub declaration: usize,
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    /// `REQUIRED` controls blocking, never whether a selected check runs.
    pub required: bool,
    /// Why this check was selected.
    pub selection: Selection,
    /// The check's Boolean domain outcome, or `None` when `WHEN` was FALSE and
    /// the check is inapplicable.
    ///
    /// `check_selection_contract/demand`: "A skipped check has no result; an
    /// explicit required read of that absent result uses ordinary MISSING
    /// behavior, never implicit TRUE."
    pub outcome: Option<Value>,
}

/// Why a check is in the selected set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Selection {
    /// "A targetless clause applies to that invocation."
    Targetless,
    /// The resolved TARGET names this candidate graph member.
    GraphMember(usize),
    /// The resolved TARGET names a data or output declaration the graph
    /// explicitly references.
    ReferencedDeclaration(usize),
    /// The resolved TARGET is the exact material value a graph ACTION targets.
    MaterialTarget(String),
    /// Selected only because a selected check or the root SUCCESS references
    /// it: "Include check prerequisites explicitly referenced by selected
    /// checks or the root SUCCESS."
    Prerequisite,
}

/// One recorded evidence item.
///
/// `05_SEMANTICS/06`: an applied `ASSUME` "must be recorded as evidence".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub declaration: usize,
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    pub detail: String,
}

/// The finalized, immutable execution plan.
///
/// Construction is crate-private: a `Plan` can only come out of a successful
/// preflight, so a caller cannot assemble one that skipped a stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub(crate) nodes: Vec<PlanNode>,
    pub(crate) edges: Vec<Edge>,
    pub(crate) order: Vec<usize>,
    pub(crate) resolutions: Vec<Resolution>,
    pub(crate) checks: Vec<CheckResult>,
    pub(crate) evidence: Vec<EvidenceRecord>,
    pub(crate) authorities: Vec<crate::authority::AuthorityRecord>,
}

impl Plan {
    /// Every planned node, in canonical graph order.
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    pub fn node(&self, index: usize) -> Option<&PlanNode> {
        self.nodes.get(index)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Every required predecessor edge, in canonical order.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// One stable topological order over the plan's nodes.
    ///
    /// Deterministic by construction: ties are broken by canonical child order,
    /// never by discovery. Under `mode.parallel` the runtime may execute
    /// independent eligible children in any order — "Result collection and
    /// diagnostics use declared child order, never finish order" — so this
    /// order is the canonical *reporting* order in every mode.
    pub fn order(&self) -> &[usize] {
        &self.order
    }

    /// Every resolved invocation datum, in declaration order.
    pub fn resolutions(&self) -> &[Resolution] {
        &self.resolutions
    }

    pub fn resolution(&self, declaration: usize) -> Option<&Resolution> {
        self.resolutions
            .iter()
            .find(|r| r.declaration == declaration)
    }

    /// Every selected pre-effect check and its outcome, in evaluation order.
    pub fn checks(&self) -> &[CheckResult] {
        &self.checks
    }

    /// Every evidence record preflight produced.
    pub fn evidence(&self) -> &[EvidenceRecord] {
        &self.evidence
    }

    /// Every applicable rule clause with its effective authority and priority.
    pub fn authorities(&self) -> &[crate::authority::AuthorityRecord] {
        &self.authorities
    }

    /// The plan's canonical serialization, for comparison and evidence.
    ///
    /// Stable across runs and across processes: it contains no address, no
    /// timing and no iteration-order-dependent text.
    pub fn serialize(&self) -> String {
        let mut out = String::new();
        out.push_str("PLAN\n");
        for (index, node) in self.nodes.iter().enumerate() {
            out.push_str(&format!(
                "  node {index} {:?} {} {} [{}] mode={} required={}",
                node.kind,
                node.block,
                node.id.as_deref().unwrap_or("-"),
                node.span.start,
                node.mode,
                node.required
            ));
            if let Some(auth) = &node.authorization {
                out.push_str(&format!(
                    " authorized={} target={} scope={} by=[{}] overridden=[{}]",
                    auth.operation,
                    auth.target.as_deref().unwrap_or("-"),
                    auth.scope.as_deref().unwrap_or("-"),
                    auth.permitted_by.join(","),
                    auth.overridden.join(",")
                ));
            }
            out.push('\n');
        }
        for edge in &self.edges {
            out.push_str(&format!(
                "  edge {} -> {} ({})\n",
                edge.from, edge.to, edge.reason
            ));
        }
        out.push_str("  order [");
        for (i, index) in self.order.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&index.to_string());
        }
        out.push_str("]\n");
        for resolution in &self.resolutions {
            out.push_str(&format!(
                "  resolved {} {} = {} ({})\n",
                resolution.block, resolution.id, resolution.value, resolution.origin
            ));
        }
        for check in &self.checks {
            out.push_str(&format!(
                "  check {} required={} selection={:?} outcome={}\n",
                check.id,
                check.required,
                check.selection,
                check
                    .outcome
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "no result".to_string())
            ));
        }
        for record in &self.evidence {
            out.push_str(&format!("  evidence {} {}\n", record.id, record.detail));
        }
        out
    }

    /// The plan's nodes keyed by activated declaration, for callers that ask
    /// "was this declaration planned, and where".
    pub fn by_declaration(&self) -> BTreeMap<usize, usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.declaration.map(|d| (d, index)))
            .collect()
    }
}
