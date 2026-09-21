//! The seam between the runtime and the executable operation surface.
//!
//! ## Why the standard library is not a `Host`
//!
//! `crate::capability` is deliberate about what a host may decide:
//!
//! > A host says *what happened* — it completed, it refused, it could not — and
//! > the runtime chooses the canonical identifier, the status and the phase. A
//! > host that wanted to force `status.succeeded` has no field to write it
//! > into.
//!
//! That is exactly right for a host, and exactly wrong for the standard
//! library. When `core.inspect` is given a depth of 101, the answer is
//! `error.value.out_of_range` — an identifier the *operation contract* selects,
//! before any host is asked anything. Expressing that through [`Host`] would
//! require giving hosts an error-identifier field, and the first adapter to
//! write into it would be deciding language meaning.
//!
//! So the operation surface gets its own seam. A dispatcher may answer in three
//! ways, and only the third reaches a host:
//!
//! 1. [`Resolution::Completed`] — the language computed the result itself. No
//!    host, no permission question, no effect.
//! 2. [`Resolution::Failed`] — the language selected a registered error from
//!    the operation's own contract, before effects.
//! 3. [`Resolution::Host`] — this operation genuinely needs the world. The
//!    request crosses [`crate::capability::request`], where both gates are
//!    checked, exactly as before.
//!
//! ## Why the dispatcher may rewrite the request
//!
//! `Resolution::Host` carries a `CapabilityRequest` rather than a bare
//! "yes, go ahead", because the request the runtime assembles is not yet the
//! request a host should see. Registry defaults are unapplied and the axis sets
//! are the row's *maxima*, which
//! `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt` is explicit are
//! "not claims that every listed capability is selected by every invocation".
//! The dispatcher resolves both and hands back the invocation's actual request.
//!
//! ## Why the default defers everything
//!
//! [`DeferAll`] reproduces the milestone-6 engine exactly: every operation goes
//! to the host, and no operation has executable semantics of its own. It is the
//! behavior `Runtime::execute` keeps, so an engine assembled without a standard
//! library still runs, still tells the truth about what it did, and still
//! cannot invent a result.

use crate::capability::{CapabilityRequest, Observation};
use crate::contracts::Contracts;
use crate::diagnostic::RuntimeError;
use crate::eval::Evaluator;
use crate::state::{Bindings, IterationPath};
use crate::value::Value;
use lcl_checker::Checked;
use lcl_lexer::Span;
use lcl_resolver::{Resolved, SourceId};
use lcl_semantics::Plan;

/// How one operation invocation resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// The language computed the outcome. No host was asked and no effect
    /// occurred.
    Completed(Observation),
    /// The operation's own contract selected a registered error before effects.
    Failed {
        error: RuntimeError,
        /// The `cause_identity` component, as `crate::diagnostic` uses it.
        cause: String,
        /// Non-normative human detail.
        detail: String,
    },
    /// The operation needs the world. This request crosses the boundary.
    ///
    /// Boxed because a resolved request is by far the largest of the three
    /// answers, and the other two are the common ones on any pure row.
    Host(Box<CapabilityRequest>),
    /// The operation delegates to a referenced execution unit.
    ///
    /// `core.execute` in graph mode and `core.test` with a TASK or ACTION
    /// TARGET both "execute [the] reachable graph" of a declaration this
    /// document already carries. Only the engine can run one, and it runs it in
    /// the same executor as everything else — there is no second execution
    /// engine — so the dispatcher names the unit and the runtime performs it,
    /// then invokes the row again with the [`GraphOutcome`] in hand.
    Graph(String),
}

/// What executing one referenced graph produced.
///
/// The runtime reports the facts; the dispatcher that asked for the graph
/// applies the registry contract to them, because
/// `operations_v0.1.0.json#/axis_contract/implementation_profile/graph_resolution`
/// is stated over "every reachable core row profile or custom kind.operation
/// declaration", which is the operation surface's own knowledge and not the
/// executor's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphOutcome {
    /// The unit that was executed.
    pub target: String,
    /// One record per invocation the graph performed, in execution order.
    pub invocations: Vec<GraphInvocation>,
    /// Whether every invocation of the graph succeeded.
    pub succeeded: bool,
    /// Each material primary result the completed graph exposed, in order.
    pub primaries: Vec<Value>,
}

/// One invocation the executed graph performed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphInvocation {
    /// The operation identifier the invocation named.
    pub operation: String,
    /// The dependency classes that invocation actually resolved, when it
    /// crossed the boundary.
    pub dependencies: std::collections::BTreeSet<String>,
    /// The effect classes it actually resolved.
    pub effects: std::collections::BTreeSet<String>,
    /// The effects it recorded.
    pub observed: Vec<crate::result::ObservedEffect>,
    /// The execution errors it raised, in order.
    pub errors: Vec<String>,
}

impl Resolution {
    /// A registered pre-effect failure.
    pub fn failed(
        error: RuntimeError,
        cause: impl Into<String>,
        detail: impl Into<String>,
    ) -> Resolution {
        Resolution::Failed {
            error,
            cause: cause.into(),
            detail: detail.into(),
        }
    }

    /// A completed result carrying no schema fields and no effects.
    pub fn none() -> Resolution {
        Resolution::Completed(Observation::none())
    }

    /// Cross the boundary with this resolved request.
    pub fn host(request: CapabilityRequest) -> Resolution {
        Resolution::Host(Box::new(request))
    }
}

/// What a dispatcher may read and change while resolving one invocation.
///
/// It carries the four stage artifacts an expression fragment needs in order to
/// be evaluated by the *same* evaluator the rest of the runtime uses. There is
/// no second expression engine, because a second one would be a second
/// language.
pub struct Invocation<'a> {
    pub contracts: &'a Contracts,
    pub resolved: &'a Resolved,
    pub checked: &'a Checked,
    pub plan: &'a Plan,
    /// Loop locals, bound OUTPUTs, and the MEMORY and STATE stores.
    pub bindings: &'a mut Bindings,
    /// The unit whose bytes the spans belong to.
    pub source: SourceId,
    /// The iteration instance this invocation belongs to.
    pub iteration: IterationPath,
    /// The invocation site, for diagnostics.
    pub span: Span,
    /// The activated declaration's index, when the node activates one.
    ///
    /// A dispatcher needs it for the rows whose contract is stated over the
    /// *reference* a TARGET names rather than the value it reads — the two
    /// store rows take `REFERENCE[MEMORY]` and `REFERENCE[STATE]`, and which
    /// declaration is being written is not recoverable from the value.
    pub declaration: Option<usize>,
    /// The graph this invocation asked for, once the runtime has executed it.
    ///
    /// `None` on the first invocation of a row: a row that needs a graph
    /// answers [`Resolution::Graph`], and the runtime invokes it again with
    /// this set. A row that never asks for one never sees it.
    pub graph: Option<GraphOutcome>,
}

impl Invocation<'_> {
    /// An evaluator over this invocation's context.
    pub fn evaluator(&self) -> Evaluator<'_> {
        Evaluator {
            contracts: self.contracts,
            resolved: self.resolved,
            checked: self.checked,
            plan: self.plan,
            bindings: self.bindings,
            source: self.source.clone(),
            iteration: self.iteration.clone(),
        }
    }

    /// Read one declaration's current value, stores included.
    pub fn declaration_value(&self, id: &str) -> Value {
        self.evaluator().declaration_value(id)
    }

    /// Write one MEMORY or STATE declaration's current value.
    ///
    /// `05_SEMANTICS/07`: "Writes require reachable core.memory_write or
    /// core.state_update and applicable scope/authorization." This is the only
    /// way to perform one, and the two operations are the only callers.
    pub fn write_store(&mut self, id: &str, value: Value) {
        self.bindings.write_store(id, value);
    }
}

/// The executable operation surface.
///
/// One implementation lives in `lcl-stdlib`. The trait is here because the
/// runtime is what invokes operations, and a dispatcher that had to re-derive
/// the invocation loop in order to be called would be a second implementation
/// of the same semantics.
pub trait Operations {
    /// Resolve one invocation of one operation.
    fn invoke(&mut self, cx: &mut Invocation<'_>, request: &CapabilityRequest) -> Resolution;
}

/// The dispatcher that implements nothing and defers everything.
///
/// Not a placeholder for missing work: it is the honest behavior of an engine
/// assembled without a standard library, and `Runtime::execute` keeps it so
/// that milestone 6's observable behavior is unchanged.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeferAll;

impl Operations for DeferAll {
    fn invoke(&mut self, _cx: &mut Invocation<'_>, request: &CapabilityRequest) -> Resolution {
        Resolution::host(request.clone())
    }
}
