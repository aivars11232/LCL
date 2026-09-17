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
//!
//! ## Where the operation surface lives
//!
//! Executing a plan means invoking operations, and the closed Core operation
//! surface is `lcl-stdlib`'s. This crate therefore carries the *seam* rather
//! than the surface: [`operations::Operations`] is the trait a dispatcher
//! implements, [`Runtime::execute_with`] is where one is supplied, and
//! [`operations::DeferAll`] is the behavior of an engine assembled without one
//! — every operation goes to the host, which is exactly what milestone 6 did.

pub mod capability;
pub mod contracts;
pub mod diagnostic;
pub mod eval;
pub mod event;
pub mod execute;
pub mod functions;
pub mod handler;
pub mod mock;
pub mod operations;
pub mod order_profile;
pub mod pattern;
pub mod result;
pub mod schedule;
pub mod state;
pub mod syntax;
pub mod value;

pub use capability::{
    Authorized, CapabilityOutcome, CapabilityRequest, Host, Observation, Permission, Refusal,
};
pub use contracts::{
    Contracts, DemandResolution, RegisteredError, ResultSchema, RetryBounds, RuntimeContractsError,
};
pub use diagnostic::{Cause, DemandContext, Diagnostic, RuntimeError, ELSEWHERE};
pub use eval::{strict_equal, Demand, Evaluator, Fault};
pub use event::{Disposition, EventLog, EventRecord};
pub use execute::{position_of, Execution, InvocationRecord, NotPlanned, Runtime};
pub use mock::MockHost;
pub use operations::{DeferAll, Invocation, Operations, Resolution};
pub use order_profile::{DurationProfile, OrderKey, DURATION_UNIT};
pub use pattern::{Flags, Glob, MatchFault, PatternFault, Regex};
pub use result::{
    EffectClass, EffectState, FailurePhase, ObservedEffect, OutputBinding, RecordState,
    ResultRecord,
};
pub use schedule::{Interleaving, Queue, Step};
pub use state::{Bindings, InvocationId, IterationPath, Lifecycle, TransitionRefusal};
pub use value::Value;
