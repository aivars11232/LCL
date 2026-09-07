//! # lcl-runtime — the deterministic LCL evaluator and runtime
//!
//! Milestone M6.
//!
//! Canonical processing step 10, and only step 10:
//!
//! > 10. Evaluate dynamic reachability conditions when reached and execute only
//! >     reachable actions in declared order and authorization bounds.
//!
//! Steps 1 through 9 already happened. This crate consumes the accepted
//! [`lcl_semantics::Plan`] those steps produced and executes it. It never
//! reinterprets source to rediscover an authority, a scope, a resolved datum, a
//! pre-effect check outcome or an ordering edge: those are decisions, and the
//! plan carries them.
//!
//! What the plan deliberately does *not* carry is the expressions preflight was
//! forbidden to evaluate — `IF` conditions, `FOR EACH` collections,
//! `RETRY.WHEN`, `HANDLER.WHEN`. Those are step 10's to demand, so this crate
//! also receives the checked and resolved program and reads exactly those
//! expressions from it.
//!
//! Steps 11 through 13 — `VERIFY`, `TEST`, evidence, `SUCCESS`/`FAILURE` and
//! one terminal root status — are the next milestone's and are absent here.

pub mod capability;
pub mod contracts;
pub mod diagnostic;
pub mod event;
pub mod mock;
pub mod result;
pub mod state;
pub mod value;

pub use capability::{
    Authorized, CapabilityOutcome, CapabilityRequest, Host, Observation, Permission, Refusal,
};
pub use contracts::{
    Contracts, DemandResolution, RegisteredError, ResultSchema, RetryBounds, RuntimeContractsError,
};
pub use diagnostic::{Cause, DemandContext, Diagnostic, RuntimeError, ELSEWHERE};
pub use event::{Disposition, EventLog, EventRecord};
pub use mock::MockHost;
pub use result::{
    EffectClass, EffectState, FailurePhase, ObservedEffect, OutputBinding, RecordState,
    ResultRecord,
};
pub use state::{Bindings, InvocationId, IterationPath, Lifecycle, TransitionRefusal};
pub use value::Value;
