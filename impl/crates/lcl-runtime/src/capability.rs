//! The capability boundary: the only way an effect leaves the language.
//!
//! ## Two gates, neither implying the other
//!
//! The implementation contract states the rule this module exists to make
//! structural:
//!
//! > Host permission does not imply LCL authorization. LCL authorization does
//! > not force host permission. Both gates must pass for an effect.
//!
//! So an invocation crosses this boundary only when *both* [`Authorized`] —
//! which the plan decided, before effects, at step 6 — and [`Permission`] —
//! which the host decides, now — are affirmative. Neither is derived from the
//! other, and neither can be skipped: [`Host::invoke`] is reachable only
//! through [`Boundary::request`], which checks both.
//!
//! ## The host returns observations, not decisions
//!
//! > The capability layer returns a typed result/effect/error record. It may
//! > not smuggle hidden semantic decisions back into the runtime.
//!
//! [`CapabilityOutcome`] therefore has no LCL status, no LCL error identifier
//! and no failure phase in it. A host says *what happened* — it completed, it
//! refused, it could not — and the runtime chooses the canonical identifier,
//! the status and the phase. A host that wanted to force `status.succeeded`
//! has no field to write it into.
//!
//! ## No clock in the runtime core
//!
//! `RETRY.DELAY` is a declared quantity, and applying it is an effect on the
//! host's timeline, not a computation. [`Host::delay`] therefore hands the
//! declared duration to the host. The runtime never sleeps, never reads a
//! clock, and its observable result never depends on how long a host waited —
//! which is what makes the deterministic scheduler in `crate::schedule`
//! meaningful rather than approximate.

use crate::result::ObservedEffect;
use crate::state::InvocationId;
use crate::value::Value;
use lcl_lexer::Span;
use lcl_resolver::SourceId;
use lcl_semantics::Authorization;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The LCL-side gate: what step 6 authorized, copied from the plan.
///
/// Constructed only from a [`lcl_semantics::Authorization`] that a plan node
/// actually carries, so a node preflight did not authorize cannot produce one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorized {
    /// The registered or custom operation identifier this ACTION authorizes.
    pub operation: String,
    /// The resolved target, when the action declares one.
    pub target: Option<String>,
    /// The effective scope this action acts within.
    pub scope: Option<String>,
    /// Rule declarations that authorized it, in canonical order.
    pub permitted_by: Vec<String>,
    /// `FORBID` declarations defeated by an exact `OVERRIDE`.
    pub overridden: Vec<String>,
}

impl Authorized {
    /// Adopt one plan node's authorization decision.
    pub fn from_plan(authorization: &Authorization) -> Authorized {
        Authorized {
            operation: authorization.operation.clone(),
            target: authorization.target.clone(),
            scope: authorization.scope.clone(),
            permitted_by: authorization.permitted_by.clone(),
            overridden: authorization.overridden.clone(),
        }
    }
}

/// One request to cross the boundary.
///
/// Carries everything the implementation contract requires an effect request to
/// prove: operation identity, target, parameters, language authorization and
/// scope, the resolved axes, and the invocation and source identity that make
/// the request traceable as evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequest {
    pub operation: String,
    /// The resolved target value, when the operation takes one.
    pub target: Option<Value>,
    /// Named parameters. `operations_v0.1.0.json#/parameter_binding` is
    /// `named_only`, so there is no positional list to supply.
    pub parameters: BTreeMap<String, Value>,
    /// The language-side gate.
    pub authorization: Authorized,
    /// `category`: `read_only`, `mutating`, `memory_state` or `control`.
    pub category: String,
    /// The invocation's resolved possible-effect set. A host may not report an
    /// effect outside it.
    pub possible_effects: BTreeSet<String>,
    /// The invocation's resolved possible-dependency set.
    pub possible_dependencies: BTreeSet<String>,
    /// The registered result schema this invocation must produce.
    pub result_schema: String,
    /// Which invocation is asking, including iteration and attempt indexes.
    pub invocation: InvocationId,
    /// Source identity and locus, for evidence.
    pub source: SourceId,
    pub span: Span,
}

impl fmt::Display for CapabilityRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operation)?;
        if let Some(target) = &self.target {
            write!(f, " on {target}")?;
        }
        write!(f, " [{}]", self.invocation)
    }
}

/// The host-side gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Permission {
    /// The host permits this request. It says nothing about whether LCL
    /// authorizes it.
    Granted,
    /// The host refuses. The runtime raises `error.permission.denied`:
    /// "Required access or an effect is unauthorized or prohibited."
    Denied(String),
    /// The host cannot supply the capability at all. The runtime raises
    /// `error.host.constraint`: "Host limitations produce
    /// error.host.constraint and never change LCL meaning."
    Unavailable(String),
}

/// What a host observed. No status, no error identifier, no phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// Schema-local result fields, under their registered names.
    pub fields: BTreeMap<String, Value>,
    /// Every concrete effect that began, in the order they began.
    pub effects: Vec<ObservedEffect>,
    /// True when the host can prove no concrete effect began.
    ///
    /// `05_SEMANTICS/09`: a pre-effect failure "requires effect_state none, an
    /// empty observed_effects list, and no bound or partial OUTPUT". Absence of
    /// recorded effects is not by itself that proof — "Absence of evidence
    /// never proves absence of effects" — so a host states it explicitly or the
    /// runtime records the phase as indeterminate.
    pub proven_effect_free: bool,
    /// A host resource or supervision limitation prevented completion. This
    /// fact does not assert when effects began; the runtime derives that from
    /// the observations, independently of its error.host.constraint mapping.
    pub host_limited: bool,
}

impl Observation {
    /// An observation with no fields and no effects, proven effect-free.
    pub fn none() -> Observation {
        Observation {
            fields: BTreeMap::new(),
            effects: Vec::new(),
            proven_effect_free: true,
            host_limited: false,
        }
    }

    pub fn with(mut self, field: impl Into<String>, value: Value) -> Observation {
        self.fields.insert(field.into(), value);
        self
    }

    pub fn with_effect(mut self, effect: ObservedEffect) -> Observation {
        self.effects.push(effect);
        self.proven_effect_free = false;
        self
    }
}

/// The outcome of one boundary crossing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityOutcome {
    /// The operation completed its contract. A completed operation may still
    /// carry a FALSE domain outcome; that is a field, not a failure.
    Completed(Observation),
    /// The operation began but did not complete. The runtime resolves the phase
    /// and effect state from the observation.
    Failed {
        /// Non-normative detail for evidence.
        detail: String,
        observation: Observation,
    },
    /// The operation's own registered contract refused the request, naming the
    /// identifier its row lists.
    ///
    /// Not a host decision. `Denied` and `Unavailable` are the host's two ways
    /// of saying no, and the runtime already maps each to one registered
    /// identifier; this arm is the same shape for the *standard library's*
    /// implementation of a row, which decides a contract this side of the
    /// boundary and can only discover part of it once the representation is in
    /// hand. `core.read`'s range is the case that needs it:
    /// `operations_v0.1.0.json#/contracts/core.read/parameters/range` requires
    /// "0 <= start <= end <= sequence length; otherwise
    /// error.value.out_of_range", and a sequence length is not knowable before
    /// the target has been read.
    ///
    /// The identifier must be one the row's own `errors` list admits. Nothing
    /// here invents a classification: it carries one the registry already
    /// assigned to this operation.
    Refused {
        error: crate::RuntimeError,
        /// The `cause_identity` component, as `crate::diagnostic` uses it.
        cause: String,
        /// Non-normative human detail.
        detail: String,
    },
    /// The host refused. `error.permission.denied`.
    Denied(String),
    /// A host limitation. `error.host.constraint`.
    Unavailable(String),
}

/// Exact subject of a retry-safety query. The runtime captures the request
/// actually sent to the host and the resulting attempt record; programs cannot
/// construct this context or replace its authority, scope or invocation ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryContext {
    pub request: CapabilityRequest,
    pub previous: crate::result::ResultRecord,
}

/// The three methods admitted by the canonical known-effect retry rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryMethod {
    Repeat,
    Resume,
    Reconcile,
}

/// Host-established evidence for one exact previous attempt. This is a host
/// attestation about a concrete capability implementation, not a language value
/// or a generic promise that running an arbitrary process twice is safe.
///
/// The host must prove that this method cannot duplicate a non-idempotent
/// effect or exceed the request's original authority, scope and post-state.
/// The runtime verifies correspondence and concrete state before accepting it;
/// the portable core cannot invent a host's implementation-specific proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryProof {
    pub context: RetryContext,
    pub method: RetryMethod,
    pub effect_state: crate::result::EffectState,
    pub observed_effects: Vec<ObservedEffect>,
    /// Exact established post-state, retained separately from prior history.
    pub post_state: String,
    /// Concrete observation/reconciliation evidence supporting the method.
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryEvidence {
    Missing,
    Unknown,
    /// A proof that this exact retry subject is unsafe.
    Unsafe(Box<RetryContext>),
    Established(Box<RetryProof>),
}

/// The host effect boundary.
///
/// Implementations live outside the language: `crate::mock::MockHost` is the
/// deterministic one this milestone ships, and M7 adds the real adapters. No
/// implementation may add or relax an LCL rule; it can only observe, refuse, or
/// report a limitation.
pub trait Host {
    /// Whether the host permits this request, decided independently of LCL
    /// authorization.
    fn permits(&mut self, request: &CapabilityRequest) -> Permission;

    /// Perform one authorized, permitted request.
    ///
    /// Called only after both gates pass.
    fn invoke(&mut self, request: &CapabilityRequest) -> CapabilityOutcome;

    /// Query evidence after a failed attempt with known or indeterminate
    /// effects. Absence never proves safety; existing hosts deny by default.
    /// This query must not repeat the operation or expand its permissions.
    fn retry_evidence(&mut self, _context: &RetryContext) -> RetryEvidence {
        RetryEvidence::Missing
    }

    /// Apply one declared `RETRY.DELAY` before the next attempt.
    ///
    /// The runtime hands over the declared duration verbatim and does not wait
    /// itself. A host that ignores it changes no observable LCL result.
    fn delay(&mut self, _duration: &Value) {}
}

/// Why a request never reached the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The plan carried no authorization for this node, so LCL never
    /// authorized the effect. `error.permission.denied`.
    Unauthorized(String),
    /// The host refused. `error.permission.denied`.
    Denied(String),
    /// A host limitation. `error.host.constraint`.
    Unavailable(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Unauthorized(detail) => write!(f, "LCL did not authorize it: {detail}"),
            Refusal::Denied(detail) => write!(f, "the host refused it: {detail}"),
            Refusal::Unavailable(detail) => write!(f, "the host cannot supply it: {detail}"),
        }
    }
}

/// The gate every effect passes through.
///
/// A free function rather than a method so there is exactly one code path from
/// the runtime to a [`Host`], and it is the one that checks both gates.
pub fn request(
    host: &mut dyn Host,
    authorized: bool,
    req: &CapabilityRequest,
) -> Result<CapabilityOutcome, Refusal> {
    // Gate one: LCL authorization, decided before effects by step 6.
    if !authorized {
        return Err(Refusal::Unauthorized(format!(
            "no reachable required declaration authorizes {} on this target",
            req.operation
        )));
    }
    // Gate two: host permission, decided now and independently.
    match host.permits(req) {
        Permission::Granted => {}
        Permission::Denied(detail) => return Err(Refusal::Denied(detail)),
        Permission::Unavailable(detail) => return Err(Refusal::Unavailable(detail)),
    }
    Ok(host.invoke(req))
}
