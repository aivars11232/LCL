//! # lcl-completion — the end of an LCL invocation
//!
//! Milestone M8.
//!
//! Canonical processing steps 11 through 13, and only those:
//!
//! > 11. Run post-execution VERIFY and TEST against actually observed results.
//! > 12. Collect required evidence and evaluate declared SUCCESS or FAILURE.
//! > 13. Return exactly one terminal status and the declared outputs.
//!
//! Steps 1 through 10 already happened. This crate consumes the
//! [`lcl_runtime::Execution`] step 10 produced, together with the checked and
//! resolved program the post-execution declarations live in, and finishes the
//! invocation.
//!
//! ## Why this is a crate and not a runtime module
//!
//! `lcl-runtime` states its own boundary at step 10, and
//! [`lcl_runtime::Execution::terminal_status`] documents that success selection
//! and the one terminal status belong here. Keeping M8 above that boundary buys
//! three things:
//!
//! 1. **Stage monotonicity as a type property.** A [`Completion`] can only be
//!    built from an `Execution`, which only `Runtime::execute*` returns. There
//!    is no path to a terminal status that skipped execution, and none to
//!    execution that skipped preflight.
//! 2. **An honest error mirror.** This milestone owns exactly the three
//!    `verification_or_completion` identifiers. They live in their own enum
//!    with their own parity check, instead of enlarging the runtime's mirror
//!    with identifiers the runtime must never emit.
//! 3. **A `TEST` root that can run a real graph.** A `TEST` may name a `TASK`
//!    or `ACTION` to execute first. Doing that against the real Core operation
//!    surface means reaching `lcl-stdlib`, which already depends on
//!    `lcl-runtime`. Sitting above the runtime and staying generic over its
//!    [`lcl_runtime::Operations`] trait keeps that possible without a cycle,
//!    and without this crate depending on the standard library at all.
//!
//! ## What this crate does not do
//!
//! No external effect of its own. `VERIFY` observes, it does not act:
//! `core.verify`'s registered `possible_effects` is exactly `none`. Where a
//! `TEST` root executes a referenced graph, it does so by calling back into the
//! runtime with the caller's own operation surface and host, so both the
//! language authorization gate and the host permission gate still apply
//! unchanged.
//!
//! It also does not re-decide anything an earlier stage decided. The primary
//! unhandled diagnostic keeps fixing the status by its resolved
//! `default_status`; a declared `FAILURE` mapping never overrides it. That one
//! rule is the difference between finishing an invocation and quietly
//! overwriting its verdict.

pub mod check;
pub mod complete;
pub mod contracts;
pub mod diagnostic;
mod engine;
pub mod evidence;
pub mod observe;
pub mod outputs;
pub mod success;
pub mod syntax;
pub mod terminal;
pub mod test_root;

pub use check::{CheckKind, CheckOutcome, Checks, Selection, SkipReason, SkippedCheck};
pub use complete::{Completion, NotExecuted};
pub use contracts::{
    CheckSelection, CompletionContractsError, Contracts, FailureLifecycle, RegisteredError,
};
pub use diagnostic::{deduplicate, stable_order, CompletionError, Diagnostic};
pub use evidence::{Evidence, EvidenceRecord, Provision};
pub use observe::{Activation, Observation};
pub use outputs::{OutputRecord, Outputs, Publication};
pub use success::{Quantifier, SelectedFailure, SuccessOutcome, Verdict};
pub use terminal::{Primary, PrimarySource, Reason, Terminal};
