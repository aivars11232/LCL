//! The identifiers milestone M8 emits, mirrored from the registry.
//!
//! ## Two sets, kept apart on purpose
//!
//! `statuses_and_errors_v0.1.0.json` classifies exactly three errors at stage
//! `verification_or_completion`. Those three are **owned** by this milestone:
//! no earlier layer emits them, and no later one may.
//!
//! * `error.verification.failed`
//! * `error.evidence.missing`
//! * `error.success.unsatisfied`
//!
//! [`CompletionError::OWNED`] is that set, and [`crate::Contracts::load`]
//! refuses a package whose `verification_or_completion` stage is not exactly
//! it. That parity check is the whole reason this enum is not just a string:
//! a registry that gained a fourth completion error would silently go
//! unimplemented otherwise.
//!
//! The remaining variants are **reused**, not owned. Completion re-emits
//! identifiers whose registered stage belongs to an earlier layer, because the
//! canonical contracts name them for situations that only arise here:
//!
//! * `error.execution.order` — `failure_mapping_rule`: "The selected canonical
//!   STATUS must be terminal, non-success, and present in the current
//!   invocation state's allowed_next; an illegal requested transition uses
//!   error.execution.order. ... Execution roots cannot select status.skipped,
//!   including through an alias; that request also uses error.execution.order."
//! * `error.required.missing` and `error.value.unknown` — `demand`: "Required
//!   demanded MISSING and UNKNOWN use error.required.missing and
//!   error.value.unknown."
//! * `error.reference.cycle` — `prerequisites`: "A prerequisite cycle uses
//!   error.reference.cycle."
//!
//! Every one of them keeps its **registered** stage here. A reused identifier
//! is not relabelled `verification_or_completion` because completion happened
//! to emit it: `errors.<id>.stage` is verbatim registry data, and
//! `earliest_stage_rule` orders diagnostics by that stage, not by which layer
//! ran.

use lcl_diagnostics::Stage;
use lcl_lexer::{Position, Span};
use lcl_resolver::SourceId;
use lcl_runtime::FailurePhase;
use std::fmt;

/// One registered error identifier this completion layer may emit.
///
/// Ordered by registry identifier, so the `stable_order` tiebreak "error
/// identifier by Unicode scalar value ascending" is this enum's own ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompletionError {
    /// Declared required evidence did not resolve.
    EvidenceMissing,
    /// A requested terminal transition is not in the current state's
    /// `allowed_next`, or an execution root requested `status.skipped`.
    ExecutionOrder,
    /// A prerequisite cycle among selected checks.
    ReferenceCycle,
    /// A required demanded condition or check result yields MISSING.
    RequiredMissing,
    /// `SUCCESS` is unsatisfied and nothing else already fixed a non-success
    /// outcome.
    SuccessUnsatisfied,
    /// A required demanded condition yields UNKNOWN.
    ValueUnknown,
    /// A required post-execution FALSE `VERIFY` or `TEST` assertion.
    VerificationFailed,
}

impl CompletionError {
    /// Every identifier this layer mirrors, in registry order.
    pub const ALL: [CompletionError; 7] = [
        CompletionError::EvidenceMissing,
        CompletionError::ExecutionOrder,
        CompletionError::ReferenceCycle,
        CompletionError::RequiredMissing,
        CompletionError::SuccessUnsatisfied,
        CompletionError::ValueUnknown,
        CompletionError::VerificationFailed,
    ];

    /// The three identifiers the registry classifies
    /// `verification_or_completion`, which this milestone owns outright.
    pub const OWNED: [CompletionError; 3] = [
        CompletionError::EvidenceMissing,
        CompletionError::SuccessUnsatisfied,
        CompletionError::VerificationFailed,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            CompletionError::EvidenceMissing => "error.evidence.missing",
            CompletionError::ExecutionOrder => "error.execution.order",
            CompletionError::ReferenceCycle => "error.reference.cycle",
            CompletionError::RequiredMissing => "error.required.missing",
            CompletionError::SuccessUnsatisfied => "error.success.unsatisfied",
            CompletionError::ValueUnknown => "error.value.unknown",
            CompletionError::VerificationFailed => "error.verification.failed",
        }
    }

    pub fn from_registry_str(id: &str) -> Option<CompletionError> {
        CompletionError::ALL
            .into_iter()
            .find(|e| e.as_registry_str() == id)
    }

    /// True for the three identifiers no other milestone may emit.
    pub fn is_owned(self) -> bool {
        CompletionError::OWNED.contains(&self)
    }
}

impl fmt::Display for CompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// One emitted completion diagnostic.
///
/// Shaped like [`lcl_runtime::Diagnostic`] so a consumer that already renders
/// runtime diagnostics renders these without a second code path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// A stable emission identity within this completion pass.
    pub sequence: usize,
    pub id: CompletionError,
    /// `errors.<id>.stage`, verbatim. Never rewritten to this layer's stage.
    pub registered_stage: Stage,
    pub source: SourceId,
    /// Exact byte locus. Authoritative.
    pub span: Span,
    /// Derived line and column. Presentation only.
    pub position: Position,
    /// `errors.<id>.meaning`, verbatim.
    pub meaning: String,
    /// `errors.<id>.default_status`, verbatim.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank` for this identifier.
    pub specificity_rank: u64,
    /// `errors.<id>.event`, verbatim.
    pub event: Option<String>,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: String,
    /// The declaration this diagnostic is about, when it is about one.
    pub declaration: Option<String>,
    /// `failure_lifecycle.phase_scope`, measured against the exposed producer.
    pub failure_phase: FailurePhase,
    /// Non-normative human detail.
    pub detail: String,
}

impl Diagnostic {
    /// The stage that governs ordering and classification.
    ///
    /// Always the registered stage: this layer never demand-resolves an
    /// identifier into a different stage.
    pub fn stage(&self) -> Stage {
        self.registered_stage
    }

    /// `duplicate_key`: identifier, locus and cause identity together.
    pub fn duplicate_key(&self) -> (CompletionError, SourceId, usize, String) {
        (
            self.id,
            self.source.clone(),
            self.span.start,
            self.cause.clone(),
        )
    }

    /// A canonical, order-stable rendering with no address and no timing.
    pub fn serialize(&self) -> String {
        format!(
            "{} stage={} status={} at {}:{} cause={}",
            self.id,
            self.registered_stage.as_registry_str(),
            self.default_status,
            self.source,
            self.span.start,
            self.cause,
        )
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// Order diagnostics by `diagnostic_selection.stable_order`.
///
/// > earliest stage, then source order, then specificity rank descending, then
/// > error identifier by Unicode scalar value ascending.
///
/// The identifier tiebreak is [`CompletionError`]'s own ordering, which is why
/// that enum is declared in registry-identifier order.
pub fn stable_order(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        a.stage()
            .index()
            .cmp(&b.stage().index())
            .then_with(|| a.source.to_string().cmp(&b.source.to_string()))
            .then_with(|| a.span.start.cmp(&b.span.start))
            .then_with(|| b.specificity_rank.cmp(&a.specificity_rank))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// Drop exact duplicates under `duplicate_rule`, keeping the first.
///
/// > Diagnostics equal under duplicate_key are one diagnostic.
pub fn deduplicate(diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        if seen.insert(diagnostic.duplicate_key()) {
            out.push(diagnostic);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_sorted_by_registry_identifier() {
        let mut sorted = CompletionError::ALL;
        sorted.sort_by_key(|e| e.as_registry_str());
        assert_eq!(sorted, CompletionError::ALL);
    }

    #[test]
    fn every_identifier_round_trips_through_its_registry_spelling() {
        for id in CompletionError::ALL {
            assert_eq!(
                CompletionError::from_registry_str(id.as_registry_str()),
                Some(id)
            );
        }
    }

    #[test]
    fn owned_is_a_subset_of_all() {
        for id in CompletionError::OWNED {
            assert!(CompletionError::ALL.contains(&id));
            assert!(id.is_owned());
        }
        assert!(!CompletionError::ExecutionOrder.is_owned());
    }
}
