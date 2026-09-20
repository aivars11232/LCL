//! Step 13b: exactly one terminal status.
//!
//! Authority: `05_SEMANTICS/10` and
//! `failure_lifecycle#/{status_rule,failure_mapping_rule,terminal_invocation_rule}`.
//!
//! ## One decision, in one order, and the order is the whole rule
//!
//! 1. **A primary unhandled diagnostic fixes the status.** "The primary
//!    unhandled diagnostic contributes its default_status after canonical alias
//!    resolution and any exact expression-demand override; secondary
//!    diagnostics and FAILURE mappings never override it." Nothing later in
//!    this list gets a vote.
//! 2. **Otherwise a declared `FAILURE` clause may map one.** Only the first
//!    whose `WHEN` was TRUE, and only if the status it requests is legal.
//! 3. **Otherwise success, if everything success requires actually holds.**
//!    "At a TASK execution root, status.succeeded is legal only when its
//!    SUCCESS is TRUE, all required outputs are fully bound and valid, all hard
//!    rules hold, and all required evidence exists."
//! 4. **Otherwise `error.success.unsatisfied`.** "FALSE root completion uses
//!    error.success.unsatisfied unless a primary diagnostic or selected FAILURE
//!    already fixes the outcome."
//!
//! ## Why the primary is computed across both layers
//!
//! `earliest_stage_rule` orders diagnostics by *registered stage*, not by which
//! layer emitted them. An execution-stage diagnostic therefore outranks a
//! completion-stage one, and a completion-emitted `error.reference.cycle` —
//! registered at `resolution` — outranks both. So this module merges the
//! execution's selected diagnostics with this pass's and applies `stable_order`
//! across the union, rather than assuming later-emitted means later-ordered.
//!
//! ## The two gates on a requested status, and the one that does not apply
//!
//! A declared `FAILURE` request must be terminal, non-success, and "present in
//! the current invocation state's allowed_next", and "an execution root cannot
//! select status.skipped, including through an alias". Each failure uses
//! `error.execution.order`.
//!
//! A status fixed by a primary diagnostic is *not* subject to `allowed_next`.
//! `status_rule` says the diagnostic contributes its `default_status` outright,
//! and `lifecycle` separately notes that registered diagnostics move a root's
//! state themselves. Only a declared mapping is a *request*, and only a request
//! can be refused.

use crate::check::Checks;
use crate::diagnostic::CompletionError;
use crate::engine::{Emission, Engine};
use crate::evidence::Evidence;
use crate::outputs::Outputs;
use crate::success::Verdict;

/// Which layer the primary unhandled diagnostic came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimarySource {
    /// A diagnostic step 10 selected.
    Execution,
    /// A diagnostic this completion pass emitted.
    Completion,
}

/// The primary unhandled diagnostic, whichever layer emitted it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Primary {
    pub layer: PrimarySource,
    /// The registered error identifier.
    pub id: String,
    /// The resolved `default_status` this diagnostic fixes.
    pub default_status: String,
}

/// Why the invocation ended with the status it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// A primary unhandled diagnostic fixed it.
    PrimaryDiagnostic(Primary),
    /// A declared `FAILURE` clause mapped it.
    DeclaredFailure(String),
    /// A committed `core.stop` halted the root on a declared stop path.
    DeclaredStop,
    /// An illegal `FAILURE` request produced `error.execution.order`.
    IllegalRequest {
        failure: String,
        requested: String,
        why: String,
    },
    /// Everything root success requires held.
    SuccessSatisfied,
    /// `SUCCESS` was unsatisfied and nothing else had fixed the outcome.
    SuccessUnsatisfied,
}

/// The one terminal status of one invocation, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminal {
    pub status: String,
    pub reason: Reason,
}

impl Terminal {
    pub fn succeeded(&self) -> bool {
        self.status == "status.succeeded"
    }

    pub fn serialize(&self) -> String {
        format!("TERMINAL {} ({:?})", self.status, self.reason)
    }
}

/// Resolve one status alias chain to its canonical core identifier.
///
/// `event_model.alias_rule`: "Resolve BASE acyclically to one core identifier
/// in the matching domain before diagnostics, event matching, status
/// transitions, and equality."
///
/// A name the registry already knows is returned unchanged. A `DEFINE
/// kind.status` alias is followed through `BASE`. The walk is bounded by the
/// declaration count, so a cycle M3 failed to reject still terminates here
/// rather than looping.
pub(crate) fn resolve_status_alias(engine: &Engine, written: &str) -> String {
    let mut current = written.to_string();
    let limit = engine.resolved.declarations().len() + 1;
    for _ in 0..limit {
        if engine.contracts.status(&current).is_some() {
            return current;
        }
        let next = engine
            .resolved
            .declarations()
            .all()
            .iter()
            .find(|d| {
                d.id.qualified() == current && d.definition_kind.as_deref() == Some("kind.status")
            })
            .and_then(|d| d.base_identifier.as_ref())
            .map(|(base, _)| base.clone());
        match next {
            Some(base) => current = base,
            None => return current,
        }
    }
    current
}

/// Select the primary unhandled diagnostic across execution and completion.
fn primary(engine: &Engine) -> Option<Primary> {
    let from_execution = engine.execution.primary().map(|d| {
        (
            d.stage().index(),
            d.source.to_string(),
            d.span.start,
            d.specificity_rank,
            d.id.as_registry_str().to_string(),
            PrimarySource::Execution,
            d.default_status.clone(),
        )
    });

    let mut mine: Vec<_> = engine.diagnostics.clone();
    crate::diagnostic::stable_order(&mut mine);
    let from_completion = mine.first().map(|d| {
        (
            d.stage().index(),
            d.source.to_string(),
            d.span.start,
            d.specificity_rank,
            d.id.as_registry_str().to_string(),
            PrimarySource::Completion,
            d.default_status.clone(),
        )
    });

    // `stable_order`: earliest stage, then source order, then specificity rank
    // descending, then error identifier ascending.
    let winner = match (from_execution, from_completion) {
        (Some(a), Some(b)) => {
            let a_key = (a.0, a.1.clone(), a.2, std::cmp::Reverse(a.3), a.4.clone());
            let b_key = (b.0, b.1.clone(), b.2, std::cmp::Reverse(b.3), b.4.clone());
            if a_key <= b_key {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }?;

    Some(Primary {
        layer: winner.5,
        id: winner.4,
        default_status: winner.6,
    })
}

/// Decide the one terminal status of this invocation.
pub(crate) fn resolve(
    engine: &mut Engine,
    checks: &Checks,
    evidence: &Evidence,
    outputs: &Outputs,
    verdict: &Verdict,
) -> Terminal {
    // 1. A primary unhandled diagnostic fixes the status outright.
    if let Some(primary) = primary(engine) {
        return Terminal {
            status: primary.default_status.clone(),
            reason: Reason::PrimaryDiagnostic(primary),
        };
    }

    // 2. A declared FAILURE clause may map one, if its request is legal.
    //
    // A committed `core.stop` is considered after it, because "core.stop yields
    // status.stopped unless another declared failure status applies".
    if verdict.failure.is_none() && root_state(engine) == "status.stopped" {
        // `status.stopped` means exactly "Execution halted on a declared stop
        // path without satisfying SUCCESS", so a root the stop already moved
        // does not then evaluate success as if it had run to the end.
        return Terminal {
            status: "status.stopped".to_string(),
            reason: Reason::DeclaredStop,
        };
    }
    if let Some(failure) = &verdict.failure {
        let requested = failure.requested_status.clone();
        // The root is still in `status.running` when completion runs:
        // "A root enters status.running before evaluating dynamic
        // execution/control or post-execution completion."
        let current = root_state(engine);
        let illegal = if !engine.contracts.is_terminal(&requested) {
            Some(format!("{requested} is not a terminal status"))
        } else if !engine.contracts.is_terminal_non_success(&requested) {
            Some(format!("{requested} is not a terminal non-success status"))
        } else if !engine.contracts.permitted_at_root(&requested) {
            // "an execution root cannot select status.skipped, including
            // through an alias."
            Some(format!(
                "{requested} has non-root scope, so an execution root cannot select it"
            ))
        } else if !engine.contracts.permits(&current, &requested) {
            Some(format!(
                "{requested} is not in {current}'s allowed_next set"
            ))
        } else {
            None
        };

        match illegal {
            None => {
                return Terminal {
                    status: requested,
                    reason: Reason::DeclaredFailure(failure.id.clone()),
                };
            }
            Some(why) => {
                let phase = engine.observed_phase();
                let error = engine.contracts.error(CompletionError::ExecutionOrder);
                let status = error.default_status.clone();
                engine.emit(Emission {
                    id: CompletionError::ExecutionOrder,
                    source: &failure.source,
                    span: failure.span,
                    declaration: Some(failure.id.clone()),
                    cause: failure.id.clone(),
                    detail: format!(
                        "FAILURE `{}` requested {} ({}), which is an illegal transition: {why}",
                        failure.id, failure.written_status, requested
                    ),
                    phase,
                    demand_resolved: false,
                });
                return Terminal {
                    status,
                    reason: Reason::IllegalRequest {
                        failure: failure.id.clone(),
                        requested: failure.requested_status.clone(),
                        why,
                    },
                };
            }
        }
    }

    // 3. Success, but only when every condition root success names actually
    //    holds.
    let blocking_check = checks.blocking().next().is_some();
    if verdict.permits_success() && evidence.complete() && outputs.complete() && !blocking_check {
        return Terminal {
            status: "status.succeeded".to_string(),
            reason: Reason::SuccessSatisfied,
        };
    }

    // 4. "FALSE root completion uses error.success.unsatisfied unless a primary
    //    diagnostic or selected FAILURE already fixes the outcome."
    let phase = engine.observed_phase();
    let (source, span) = root_locus(engine);
    let detail = unsatisfied_detail(checks, evidence, outputs, verdict);
    engine.emit(Emission {
        id: CompletionError::SuccessUnsatisfied,
        source: &source,
        span,
        declaration: verdict.success.as_ref().map(|s| s.id.clone()),
        cause: verdict
            .success
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "root".to_string()),
        detail,
        phase,
        demand_resolved: false,
    });
    let status = engine
        .contracts
        .error(CompletionError::SuccessUnsatisfied)
        .default_status
        .clone();
    Terminal {
        status,
        reason: Reason::SuccessUnsatisfied,
    }
}

/// Exactly why success was unsatisfied, for the diagnostic's detail.
fn unsatisfied_detail(
    checks: &Checks,
    evidence: &Evidence,
    outputs: &Outputs,
    verdict: &Verdict,
) -> String {
    let mut reasons: Vec<String> = Vec::new();
    if let Some(success) = &verdict.success {
        if !success.satisfied() {
            reasons.push(format!(
                "SUCCESS `{}` evaluated {}",
                success.id, success.value
            ));
        }
    }
    for check in checks.blocking() {
        reasons.push(format!(
            "required {} `{}` asserted {}",
            check.kind, check.id, check.outcome
        ));
    }
    for record in evidence.missing() {
        reasons.push(format!("required evidence `{}` is absent", record.id));
    }
    for record in outputs.unsatisfied() {
        reasons.push(format!("required output `{}` is not bound", record.id));
    }
    if reasons.is_empty() {
        return "root completion is unsatisfied".to_string();
    }
    reasons.join("; ")
}

/// The root invocation's lifecycle state at the start of completion.
fn root_state(engine: &Engine) -> String {
    engine
        .execution
        .invocations()
        .iter()
        .find(|record| {
            engine
                .plan
                .node(record.node)
                .is_some_and(|node| node.parent.is_none())
        })
        .map(|record| record.status().to_string())
        // A plan with no entered root ran nothing; "A root enters
        // status.running before evaluating ... post-execution completion."
        .unwrap_or_else(|| "status.running".to_string())
}

/// The locus a root-level completion diagnostic points at.
fn root_locus(engine: &Engine) -> (lcl_resolver::SourceId, lcl_lexer::Span) {
    if let Some(root) = engine.root_declaration() {
        if let Some(decl) = engine.resolved.declarations().get(root) {
            return (decl.source.clone(), decl.id_span);
        }
    }
    (engine.root_source(), lcl_lexer::Span { start: 0, end: 0 })
}
