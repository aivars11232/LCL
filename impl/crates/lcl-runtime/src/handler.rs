//! Events, handler selection, `FALLBACK`, `RETRY` and continuation.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/event_model`,
//! `#/failure_lifecycle/retry_safety`, `05_SEMANTICS/06`, `05_SEMANTICS/08`,
//! `05_SEMANTICS/09` and `06_STANDARD_LIBRARY/03_CONTROL_OPERATIONS.txt`.
//!
//! ## A RETRY block is a budget, not an authorization
//!
//! The single most misreadable rule in this area is stated twice, so this
//! module implements it once:
//!
//! > The ACTION RETRY block is the only source of attempt bounds. A handler
//! > whose OPERATION is core.retry selects and authorizes that declared RETRY;
//! > it never opens a second, independent attempt budget and never multiplies
//! > attempts.
//!
//! and
//!
//! > A RETRY block alone declares eligibility and a budget; the selected
//! > core.retry handler authorizes additional attempts.
//!
//! So a failing `ACTION` with a `RETRY` block and no selected `core.retry`
//! handler makes exactly one attempt. Retrying is something a *handler* does,
//! and [`Retry`] is only the bound it must respect.
//!
//! ## Termination
//!
//! > Handler selection is not re-entered for a diagnostic raised while
//! > evaluating a candidate WHEN, resolving a handler invocation contract, or
//! > executing a handler invocation or its FALLBACK for the same originating
//! > diagnostic. ... Together with bounded RETRY this makes handler activation
//! > terminating.
//!
//! Both halves are structural here: [`Engine::in_handler`] suppresses event
//! raising for the whole nested region, and the attempt budget is read from the
//! registry-bounded `RETRY.LIMIT`.

use crate::capability::{
    self, Authorized, CapabilityOutcome, CapabilityRequest, Refusal, RetryContext, RetryEvidence,
    RetryMethod, RetryProof,
};
use crate::diagnostic::RuntimeError;
use crate::eval::Fault;
use crate::event::Disposition;
use crate::execute::Engine;
use crate::result::{EffectState, FailurePhase, RecordState, ResultRecord};
use crate::state::{InvocationId, IterationPath};
use crate::syntax::{self, DeclBlock};
use crate::value::Value;
use lcl_semantics::PlanNode;
use std::collections::BTreeMap;

/// The declared `RETRY` contract of one `ACTION`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    /// "LIMIT is the maximum number of additional attempts after the first."
    pub limit: u32,
    /// The `WHEN` expression, when one is declared. Omitted means TRUE.
    pub when: Option<lcl_parser::syntax::Expr>,
    /// The declared `DELAY`, when one is declared.
    pub delay: Option<lcl_parser::syntax::Expr>,
    /// `RETRY.HANDLER` references, in source order.
    pub handlers: Vec<String>,
}

/// One attachment site along the declared producer path.
///
/// `selection_order` ranks by "attachment proximity along the declared producer
/// path, innermost to outermost, using the closed rank order local DEPENDENCY
/// or FAILURE scope, then RETRY, then STEP innermost to outermost, then TASK".
/// A smaller `rank` is nearer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    rank: usize,
    /// Position in that site's reference list: "then by the source declaration
    /// order of the HANDLER reference list at that attachment site".
    list_order: usize,
    /// "then by handler ID in Unicode scalar order".
    id: String,
    /// The handler declaration's index.
    declaration: usize,
}

/// What a selected handler did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handled {
    /// No candidate matched. "the diagnostic remains unhandled".
    NoMatch,
    /// A handler ran and recovered the originating diagnostic.
    Recovered { handler: String },
    /// A handler ran and did not recover it.
    NotRecovered { handler: String },
    /// A selected `core.retry` handler authorized another attempt.
    Retry { handler: String },
}

impl<'a> Engine<'a> {
    /// Select and run the handler for one raised event, if any.
    ///
    /// `occurrence` is the event this failure raised; `None` means the
    /// diagnostic's registered mapping is null and no handler can be selected.
    pub(crate) fn handle(
        &mut self,
        occurrence: Option<usize>,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Handled {
        let Some(occurrence) = occurrence else {
            return Handled::NoMatch;
        };
        let Some(event) = self
            .events
            .records()
            .get(occurrence)
            .map(|record| record.event.clone())
        else {
            return Handled::NoMatch;
        };

        let candidates = self.candidates(planned);
        // "Exactly the first matching candidate is selected, so one diagnostic
        // causes at most one event-selected handler activation."
        let selected = self.first_match(&candidates, &event, planned, iteration);
        let Some(candidate) = selected else {
            self.events.dispose(occurrence, Disposition::NoMatch);
            return Handled::NoMatch;
        };

        let outcome = self.activate(&candidate, planned, id, iteration);
        let disposition = match &outcome {
            Handled::Recovered { handler } => Disposition::Selected {
                handler: handler.clone(),
                recovered: true,
            },
            Handled::NotRecovered { handler } | Handled::Retry { handler } => {
                Disposition::Selected {
                    handler: handler.clone(),
                    recovered: false,
                }
            }
            Handled::NoMatch => Disposition::NoMatch,
        };
        self.events.dispose(occurrence, disposition);
        outcome
    }

    /// Every handler attached along the declared producer path, in
    /// `selection_order`.
    fn candidates(&self, planned: &PlanNode) -> Vec<Candidate> {
        let mut out: Vec<Candidate> = Vec::new();
        let mut rank = 0usize;

        // Rank 0: `RETRY.HANDLER`, "in scope only for diagnostics raised by an
        // attempt of the ACTION declaring that RETRY, including the first
        // attempt".
        if let Some(retry) = self.retry_of(planned) {
            self.push_handlers(&mut out, rank, &retry.handlers);
        }
        rank += 1;

        // Then STEP innermost to outermost, then TASK. Walking the parent chain
        // yields exactly that order.
        let mut current = Some(planned.clone());
        while let Some(node) = current {
            if matches!(node.block.as_str(), "STEP" | "TASK") {
                if let Some(declaration) = node.declaration {
                    if let Some(block) = syntax::declaration_block(self.resolved, declaration) {
                        let references = references_of(&block, "HANDLER");
                        self.push_handlers(&mut out, rank, &references);
                    }
                }
                rank += 1;
            }
            current = node.parent.and_then(|p| self.plan.node(p).cloned());
        }

        // "That order is total and has no tie."
        out.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then(a.list_order.cmp(&b.list_order))
                .then(a.id.cmp(&b.id))
        });
        out
    }

    fn push_handlers(&self, out: &mut Vec<Candidate>, rank: usize, references: &[String]) {
        for (list_order, id) in references.iter().enumerate() {
            // "A declared but unattached HANDLER is never selected by an event
            // attachment" — so only a referenced id becomes a candidate.
            let Some(declaration) = self
                .resolved
                .declarations()
                .all()
                .iter()
                .position(|d| d.id.qualified() == *id && d.block == "HANDLER")
            else {
                continue;
            };
            out.push(Candidate {
                rank,
                list_order,
                id: id.clone(),
                declaration,
            });
        }
    }

    /// The first candidate whose `EVENT` matches and whose `WHEN` is TRUE.
    ///
    /// "A candidate matches when its EVENT equals the raised event and its WHEN
    /// is absent or evaluates TRUE; a WHEN evaluating MISSING or UNKNOWN does
    /// not match and raises nothing further."
    fn first_match(
        &mut self,
        candidates: &[Candidate],
        event: &str,
        planned: &PlanNode,
        iteration: &IterationPath,
    ) -> Option<Candidate> {
        for candidate in candidates {
            let Some(block) = syntax::declaration_block(self.resolved, candidate.declaration)
            else {
                continue;
            };
            let declared = syntax::field_text(&block, "EVENT").unwrap_or_default();
            if declared != event {
                continue;
            }
            let Some(when) = syntax::field_expr(&block, "WHEN") else {
                // "absent ... or evaluates TRUE"
                return Some(candidate.clone());
            };
            let when = when.clone();
            // "Handler selection is not re-entered for a diagnostic raised
            // while evaluating a candidate WHEN"; those diagnostics raise no
            // event.
            self.in_handler += 1;
            let verdict = self
                .evaluator(&planned.source, iteration)
                .demand(&when)
                .ok();
            self.in_handler -= 1;
            if verdict == Some(Value::Boolean(true)) {
                return Some(candidate.clone());
            }
        }
        None
    }

    /// Run one selected handler.
    fn activate(
        &mut self,
        candidate: &Candidate,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Handled {
        let Some(block) = syntax::declaration_block(self.resolved, candidate.declaration) else {
            return Handled::NoMatch;
        };
        let Some(operation) = syntax::field_text(&block, "OPERATION") else {
            return Handled::NoMatch;
        };

        // `core.retry` does not invoke anything: it authorizes the declared
        // RETRY of the owning ACTION.
        //
        // It runs inside the non-reentrant region for the same reason an
        // ordinary handler invocation does: every diagnostic it raises —
        // `error.retry.exhausted` included — is raised "while executing a
        // handler invocation ... for the same originating diagnostic", and
        // `non_reentrancy_rule` says "Those diagnostics raise no event."
        // Together with the bounded `RETRY.LIMIT` this is what makes handler
        // activation terminating: an exhaustion diagnostic cannot select
        // another `core.retry` and consume a budget that is already spent.
        if operation == "core.retry" {
            self.in_handler += 1;
            let outcome = self.authorize_retry(candidate, &block, planned, id, iteration);
            self.in_handler -= 1;
            return outcome;
        }

        // Everything else is an ordinary invocation site. Its own diagnostics
        // raise no event, per `non_reentrancy_rule`.
        self.in_handler += 1;
        // Where the primary invocation's diagnostics begin, so a successful
        // FALLBACK can mark exactly them as substituted evidence.
        let primary_mark = self.raw.len();
        let result = self.invoke_handler(&operation, &block, planned, id, iteration);
        let outcome = match &result {
            Some(record) if record.succeeded() => Handled::Recovered {
                handler: candidate.id.clone(),
            },
            // "FALLBACK is eligible only after the primary handler invocation
            // ... either cannot proceed because a dynamic operation
            // precondition is unsatisfied or ends with status.failed or
            // status.blocked. ... never runs after success, cancellation, a
            // declared stop, or a skipped invocation."
            Some(record) if fallback_eligible(record) => {
                match self.fallback(&block, planned, id, iteration) {
                    // "A successful fallback substitutes its success for the
                    // failed primary in the selected handler's result and
                    // recovers the original diagnostic." The failed primary's
                    // diagnostics stay as local evidence but stop controlling
                    // the aggregate result.
                    Some(true) => {
                        for sequence in primary_mark..self.raw.len() {
                            self.substituted.insert(sequence);
                        }
                        Handled::Recovered {
                            handler: candidate.id.clone(),
                        }
                    }
                    _ => Handled::NotRecovered {
                        handler: candidate.id.clone(),
                    },
                }
            }
            _ => Handled::NotRecovered {
                handler: candidate.id.clone(),
            },
        };
        self.in_handler -= 1;
        outcome
    }

    /// A selected `core.retry` handler authorizing the declared `RETRY`.
    fn authorize_retry(
        &mut self,
        candidate: &Candidate,
        handler: &DeclBlock<'_>,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Handled {
        let Some(retry) = self.retry_of(planned) else {
            // "an ACTION declaring no RETRY block uses
            // error.operation.precondition before another attempt"
            self.precondition(
                planned,
                id,
                "core.retry selected an ACTION that declares no RETRY block",
            );
            return Handled::NotRecovered {
                handler: candidate.id.clone(),
            };
        };

        // "Its resolved limit must equal the wrapped ACTION's RETRY LIMIT. An
        // unequal limit ... uses error.operation.precondition."
        if let Some(declared) = handler_limit(handler) {
            if declared != retry.limit {
                self.precondition(
                    planned,
                    id,
                    format!(
                        "core.retry resolved limit {declared} does not equal the declared \
                         RETRY LIMIT {}",
                        retry.limit
                    ),
                );
                return Handled::NotRecovered {
                    handler: candidate.id.clone(),
                };
            }
        }

        // "After an unsuccessful attempt, first stop if the attempt budget is
        // consumed."
        //
        // `id.attempt` is the zero-based index of the attempt that just failed,
        // so attempts made so far is `attempt + 1` and the budget is
        // `1 + LIMIT`.
        let made = id.attempt.saturating_add(1);
        // "One ACTION invocation makes at most 1 + LIMIT attempts."
        let budget = (retry.limit as usize).saturating_add(1);
        if made >= budget {
            // "error.retry.exhausted is emitted only when exactly 1 + LIMIT
            // attempts were actually made and every one failed."
            self.emit_at(
                RuntimeError::RetryExhausted,
                planned,
                id,
                "retry exhausted",
                format!("{made} attempts were made and all failed"),
                FailurePhase::PostEffect.min_with(self.phase_of_attempt(id)),
            );
            return Handled::NotRecovered {
                handler: candidate.id.clone(),
            };
        }

        // "Otherwise evaluate WHEN before each additional attempt."
        if let Some(when) = &retry.when {
            let when = when.clone();
            self.in_handler += 1;
            let verdict = self.evaluator(&planned.source, iteration).demand(&when);
            self.in_handler -= 1;
            match verdict {
                Ok(Value::Boolean(true)) => {}
                // "FALSE ends retrying and leaves the previous failure
                // unhandled without error.retry.exhausted."
                Ok(Value::Boolean(false)) => {
                    return Handled::NotRecovered {
                        handler: candidate.id.clone(),
                    }
                }
                // "MISSING uses error.required.missing and UNKNOWN uses
                // error.value.unknown; neither permits an attempt."
                Ok(Value::Missing) => {
                    self.emit_at(
                        RuntimeError::RequiredMissing,
                        planned,
                        id,
                        "retry condition",
                        "RETRY.WHEN yielded MISSING, which permits no attempt",
                        FailurePhase::PreEffect,
                    );
                    return Handled::NotRecovered {
                        handler: candidate.id.clone(),
                    };
                }
                Ok(Value::Unknown) => {
                    self.emit_at(
                        RuntimeError::ValueUnknown,
                        planned,
                        id,
                        "retry condition",
                        "RETRY.WHEN yielded UNKNOWN, which permits no attempt",
                        FailurePhase::PreEffect,
                    );
                    return Handled::NotRecovered {
                        handler: candidate.id.clone(),
                    };
                }
                _ => {
                    return Handled::NotRecovered {
                        handler: candidate.id.clone(),
                    }
                }
            }
        }

        // "For TRUE, establish the retry-safety proof required by the failure
        // lifecycle, then apply DELAY before beginning the next attempt. A
        // refused or unproven safety check consumes no attempt and is not
        // exhaustion."
        if !self.retry_is_safe(planned, id) {
            return Handled::NotRecovered {
                handler: candidate.id.clone(),
            };
        }

        // The delay is the host's to apply; the runtime core has no clock.
        if let Some(delay) = &retry.delay {
            let delay = delay.clone();
            self.in_handler += 1;
            if let Ok(duration) = self.evaluator(&planned.source, iteration).demand(&delay) {
                self.host.delay(&duration);
            }
            self.in_handler -= 1;
        }

        Handled::Retry {
            handler: candidate.id.clone(),
        }
    }

    /// The retry-safety proof for one further attempt.
    ///
    /// `retry_safety`: a `pre_effect` attempt with `effect_state none` "may be
    /// attempted again"; after known effects another attempt "is permitted only
    /// when exact evidence proves" safety; an `indeterminate` attempt
    /// "prohibits another attempt until exact reconciliation evidence resolves
    /// the relevant state."
    fn retry_is_safe(&mut self, planned: &PlanNode, id: &InvocationId) -> bool {
        let Some(record) = self.records.get(id).and_then(|r| r.result.clone()) else {
            return true;
        };
        if matches!(
            record.failure_phase,
            FailurePhase::None | FailurePhase::PreEffect
        ) && record.effect_state == EffectState::None
        {
            return true;
        }
        let context = self.host_requests.get(id).map(|request| RetryContext {
            request: request.clone(),
            previous: record.clone(),
        });
        let evidence = context
            .as_ref()
            .map(|context| self.host.retry_evidence(context));
        let (error, detail) = match (context, evidence) {
            (Some(context), Some(RetryEvidence::Established(proof)))
                if retry_proof_matches(&context, &proof) =>
            {
                if let Some(action) = self.action_requests.get(id) {
                    // Both the original language request and the resolved host
                    // request must remain exact at the next attempt boundary.
                    self.retry_guards
                        .insert(id.next_attempt(), (action.clone(), context.request));
                    self.retry_proofs.push(*proof);
                    return true;
                }
                (
                    RuntimeError::RequiredMissing,
                    "the original action request is unavailable",
                )
            }
            (Some(_), Some(RetryEvidence::Unknown)) => (
                RuntimeError::ValueUnknown,
                "the host could not establish retry safety",
            ),
            (Some(context), Some(RetryEvidence::Unsafe(subject))) if *subject == context => (
                RuntimeError::OperationPrecondition,
                "the host proved this exact retry unsafe",
            ),
            _ => (
                RuntimeError::RequiredMissing,
                "exact state and retry evidence for the previous request were not established",
            ),
        };
        self.emit_at(
            error,
            planned,
            id,
            "retry safety",
            detail,
            record.failure_phase,
        );
        false
    }

    /// Invoke one handler's `OPERATION` under the invocation-site contract.
    fn invoke_handler(
        &mut self,
        operation: &str,
        handler: &DeclBlock<'_>,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Option<ResultRecord> {
        // The three control operations that act on execution state rather than
        // through a host.
        match operation {
            "core.continue" => return Some(self.continue_at(planned, id)),
            "core.stop" => return Some(self.stop_at(planned, id, "status.stopped")),
            "core.cancel" => return Some(self.stop_at(planned, id, "status.cancelled")),
            _ => {}
        }

        let target = match syntax::field_expr(handler, "TARGET") {
            Some(expr) => {
                let expr = expr.clone();
                self.evaluator(&planned.source, iteration)
                    .demand(&expr)
                    .ok()
            }
            // The handler-context binding: an omitted target binds "the
            // still-active invocation aggregate that owns the originating
            // failed attempt".
            None => Some(Value::Reference(
                planned.id.clone().unwrap_or_else(|| planned.block.clone()),
            )),
        };

        let axes = self.contracts.preflight().operation_axes(operation);
        let schema = self
            .contracts
            .operation_schema(operation)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "result.operation".to_string());

        let request = CapabilityRequest {
            operation: operation.to_string(),
            target,
            parameters: handler_parameters(self, handler, planned, iteration),
            // A handler adds no authority: it acts within the authorization the
            // plan already decided for the owning aggregate.
            authorization: planned
                .authorization
                .as_ref()
                .map(Authorized::from_plan)
                .unwrap_or(Authorized {
                    operation: operation.to_string(),
                    target: None,
                    scope: None,
                    permitted_by: Vec::new(),
                    overridden: Vec::new(),
                }),
            category: axes.map(|a| a.category.clone()).unwrap_or_default(),
            possible_effects: axes.map(|a| a.possible_effects.clone()).unwrap_or_default(),
            possible_dependencies: axes
                .map(|a| a.possible_dependencies.clone())
                .unwrap_or_default(),
            result_schema: schema.clone(),
            invocation: id.clone(),
            source: planned.source.clone(),
            span: planned.span,
        };

        let authorized = planned.authorization.is_some();
        match capability::request(self.host, authorized, &request) {
            Ok(CapabilityOutcome::Completed(observation)) => {
                let mut record = ResultRecord::new(&schema, "status.succeeded");
                record.fields = observation.fields;
                record.observed_effects = observation.effects.clone();
                record.effect_state = if observation.effects.is_empty() {
                    EffectState::None
                } else {
                    EffectState::Applied
                };
                // A handler's result is a result. The ordinary dispatch path
                // checks the closed registered field set before anything is
                // bound, and a field set that is closed on only one path is not
                // closed — least of all here, where the record decides whether
                // a registered diagnostic is kept or discarded. A malformed
                // claim of success would recover the failure it was called for
                // and take the diagnostic with it.
                let violations = record.schema_violations(self.contracts);
                if !violations.is_empty() {
                    let phase = crate::execute::phase_of(
                        &record.observed_effects,
                        observation.proven_effect_free,
                    );
                    self.emit_at(
                        RuntimeError::HostConstraint,
                        planned,
                        id,
                        "result contract",
                        format!("{schema}: {}", violations.join("; ")),
                        phase,
                    );
                    record.status = self
                        .contracts
                        .error(RuntimeError::HostConstraint)
                        .default_status
                        .clone();
                    record
                        .execution_errors
                        .push(RuntimeError::HostConstraint.as_registry_str().into());
                    record.failure_phase = phase;
                    record.effect_state =
                        crate::execute::effect_state_of(&record.observed_effects, phase);
                }
                Some(record)
            }
            Ok(CapabilityOutcome::Failed {
                detail,
                observation,
            }) if observation.host_limited => Some(
                self.record_of(
                    &schema,
                    Ok(CapabilityOutcome::Failed {
                        detail,
                        observation,
                    }),
                    planned,
                    id,
                )
                .0,
            ),
            Ok(CapabilityOutcome::Failed { .. })
            | Ok(CapabilityOutcome::Refused { .. })
            | Ok(CapabilityOutcome::Denied(_)) => Some(ResultRecord::new(&schema, "status.failed")),
            Ok(CapabilityOutcome::Unavailable(_)) | Err(Refusal::Unavailable(_)) => {
                Some(ResultRecord::new(&schema, "status.blocked"))
            }
            Err(_) => Some(ResultRecord::new(&schema, "status.failed")),
        }
    }

    /// `core.continue`: "requests the exact declared successor of the failing
    /// execution unit."
    ///
    /// "Commit recovery and advancement only when the selected handler,
    /// including permitted FALLBACK substitution, succeeds." The successor is
    /// therefore recorded and queued by the caller on success, and
    /// "continuation creates no successor": an absent or ambiguous one is
    /// `error.execution.order`.
    fn continue_at(&mut self, planned: &PlanNode, id: &InvocationId) -> ResultRecord {
        match self.declared_successor(planned) {
            Some(successor) => {
                self.continuation = Some((successor, id.iteration.clone()));
                ResultRecord::new("result.operation", "status.succeeded")
                    .with_field("changed", Value::Boolean(true))
            }
            None => {
                self.emit_at(
                    RuntimeError::ExecutionOrder,
                    planned,
                    id,
                    "continuation",
                    "core.continue found no unambiguous declared successor",
                    FailurePhase::PreEffect,
                );
                ResultRecord::new("result.operation", "status.failed")
            }
        }
    }

    /// "A declared successor is the unique next reachable sibling in the
    /// containing sequential group, ascending finished enclosing sequential
    /// containers when necessary."
    fn declared_successor(&self, planned: &PlanNode) -> Option<usize> {
        let mut child = self.plan.nodes().iter().position(|n| {
            n.candidate == planned.candidate && n.span == planned.span && n.id == planned.id
        })?;
        loop {
            let node = self.plan.node(child)?;
            let parent_index = node.parent?;
            let parent = self.plan.node(parent_index)?;
            // "ambiguous parallel continuation uses error.execution.order"
            if parent.mode == lcl_semantics::Mode::Parallel {
                return None;
            }
            let position = parent.children.iter().position(|c| *c == child)?;
            if let Some(next) = parent.children.get(position + 1) {
                return Some(*next);
            }
            // Ascend a finished enclosing sequential container.
            child = parent_index;
        }
    }

    /// `core.stop` and `core.cancel`, which act on internal execution state.
    ///
    /// "require its registered allowed_next set to contain status.cancelled"
    /// (and status.stopped respectively); "a current status that does not allow
    /// [it] uses error.execution.order".
    fn stop_at(&mut self, planned: &PlanNode, id: &InvocationId, status: &str) -> ResultRecord {
        let permitted = self
            .records
            .get(id)
            .map(|record| {
                record
                    .lifecycle
                    .permits(self.contracts.diagnostics(), status)
            })
            .unwrap_or(false);
        if !permitted {
            self.emit_at(
                RuntimeError::ExecutionOrder,
                planned,
                id,
                "lifecycle transition",
                format!("the current invocation state does not permit {status}"),
                FailurePhase::PreEffect,
            );
            return ResultRecord::new("result.operation", "status.failed");
        }
        self.requested_status = Some(status.to_string());
        ResultRecord::new("result.operation", "status.succeeded")
            .with_field("changed", Value::Boolean(true))
    }

    /// One `FALLBACK` invocation after an eligible primary refusal or failure.
    ///
    /// "At most one fallback invocation follows each eligible primary refusal
    /// or failure." Returns `Some(true)` when the fallback succeeded and
    /// therefore substitutes for the primary.
    fn fallback(
        &mut self,
        handler: &DeclBlock<'_>,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Option<bool> {
        let declared = syntax::field_text(handler, "FALLBACK")?;
        // "It admits exactly two forms. One REF resolving to a HANDLER ... One
        // operation identifier".
        if let Some(referenced) = declared
            .strip_prefix("REF(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let declaration = self
                .resolved
                .declarations()
                .all()
                .iter()
                .position(|d| d.id.qualified() == referenced && d.block == "HANDLER")?;
            let block = syntax::declaration_block(self.resolved, declaration)?;
            let operation = syntax::field_text(&block, "OPERATION")?;
            // "its EVENT and WHEN do not select another event or gate this
            // direct invocation."
            let result = self.invoke_handler(&operation, &block, planned, id, iteration);
            return Some(result.map(|r| r.succeeded()).unwrap_or(false));
        }
        // An operation identifier, whose required target the handler-context
        // binding supplies.
        let result = self.invoke_handler(&declared, handler, planned, id, iteration);
        Some(result.map(|r| r.succeeded()).unwrap_or(false))
    }

    /// The declared `RETRY` block of one `ACTION`, if it declares one.
    pub(crate) fn retry_of(&self, planned: &PlanNode) -> Option<Retry> {
        let declaration = planned.declaration?;
        let block = syntax::declaration_block(self.resolved, declaration)?;
        let retry = block.nested("RETRY")?;
        let limit_text = syntax::field_text(&retry, "LIMIT")?;
        let limit: u32 = limit_text.trim().parse().ok()?;
        let bounds = self.contracts.retry_bounds();
        // "LIMIT is the maximum number of additional attempts, 0 through 100."
        if (limit as i64) < bounds.minimum_limit || (limit as i64) > bounds.maximum_limit {
            return None;
        }
        Some(Retry {
            limit,
            when: syntax::field_expr(&retry, "WHEN").cloned(),
            delay: syntax::field_expr(&retry, "DELAY").cloned(),
            handlers: references_of(&retry, "HANDLER"),
        })
    }

    fn phase_of_attempt(&self, id: &InvocationId) -> FailurePhase {
        self.records
            .get(id)
            .and_then(|record| record.result.as_ref())
            .map(|result| result.failure_phase)
            .unwrap_or(FailurePhase::PreEffect)
    }

    fn precondition(&mut self, planned: &PlanNode, id: &InvocationId, detail: impl Into<String>) {
        self.emit_at(
            RuntimeError::OperationPrecondition,
            planned,
            id,
            "operation precondition",
            detail,
            FailurePhase::PreEffect,
        );
    }

    /// Emit one diagnostic at a producer, without going through a [`Fault`].
    pub(crate) fn emit_at(
        &mut self,
        error: RuntimeError,
        planned: &PlanNode,
        id: &InvocationId,
        cause: impl Into<String>,
        detail: impl Into<String>,
        phase: FailurePhase,
    ) {
        let fault = Fault {
            id: error,
            span: planned.span,
            cause: cause.into(),
            detail: detail.into(),
            demand_resolved: self.contracts.demand().is_eligible(error),
        };
        self.fault(&fault, planned, id, phase);
    }
}

impl FailurePhase {
    /// The more conservative of two phases, for an aggregate that folds an
    /// attempt's observations into its own.
    ///
    /// `phase_scope`: "the aggregate producer derives its phase from all
    /// effects begun within its invocation."
    fn min_with(self, other: FailurePhase) -> FailurePhase {
        match (self, other) {
            (FailurePhase::Indeterminate, _) | (_, FailurePhase::Indeterminate) => {
                FailurePhase::Indeterminate
            }
            (FailurePhase::PostEffect, _) | (_, FailurePhase::PostEffect) => {
                FailurePhase::PostEffect
            }
            (FailurePhase::PreEffect, _) | (_, FailurePhase::PreEffect) => FailurePhase::PreEffect,
            _ => FailurePhase::None,
        }
    }
}

/// The `REF(...)` ids one reference-or-list field names, in source order.
fn references_of(block: &DeclBlock<'_>, field: &str) -> Vec<String> {
    let Some(expr) = syntax::field_expr(block, field) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_references(expr, &mut out);
    out
}

fn collect_references(expr: &lcl_parser::syntax::Expr, out: &mut Vec<String>) {
    match expr {
        lcl_parser::syntax::Expr::Call(call) if call.is_reference() => {
            if let Some(target) = call.reference_target() {
                out.push(target.text.clone());
            }
        }
        lcl_parser::syntax::Expr::Collection(collection) => {
            for member in &collection.members {
                collect_references(member, out);
            }
        }
        lcl_parser::syntax::Expr::Group(group) => collect_references(&group.inner, out),
        _ => {}
    }
}

/// `HANDLER.LIMIT`, which "is exactly the limit named parameter of the selected
/// operation".
fn handler_limit(handler: &DeclBlock<'_>) -> Option<u32> {
    syntax::field_text(handler, "LIMIT")?.trim().parse().ok()
}

/// The handler's declared named parameters.
fn handler_parameters(
    engine: &mut Engine,
    handler: &DeclBlock<'_>,
    planned: &PlanNode,
    iteration: &IterationPath,
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    // `HANDLER.LIMIT` is exactly the `limit` named parameter.
    if let Some(limit) = handler_limit(handler) {
        out.insert(
            "limit".to_string(),
            Value::Integer(lcl_checker::numeric::Decimal::from_integer(
                lcl_checker::numeric::Integer::from_u64(limit as u64),
            )),
        );
    }
    let nested: Vec<_> = handler
        .fields("PARAMETER")
        .into_iter()
        .filter_map(|field| field.body.as_nested())
        .map(|nested| nested.statements.clone())
        .collect();
    for statements in nested {
        let mut name = None;
        let mut value_expr = None;
        for statement in &statements {
            if let lcl_parser::syntax::Statement::Field(field) = statement {
                match field.key.text.as_str() {
                    "NAME" => {
                        name = field
                            .body
                            .as_inline()
                            .and_then(|v| v.as_expression())
                            .map(syntax::render)
                            .map(|t| t.trim_matches('"').to_string())
                    }
                    "VALUE" => {
                        value_expr = field
                            .body
                            .as_inline()
                            .and_then(|v| v.as_expression())
                            .cloned()
                    }
                    _ => {}
                }
            }
        }
        if let (Some(name), Some(expr)) = (name, value_expr) {
            if let Ok(value) = engine.evaluator(&planned.source, iteration).demand(&expr) {
                out.insert(name, value);
            }
        }
    }
    out
}

/// Whether one result is eligible for `FALLBACK`.
///
/// "FALLBACK is eligible only after the primary handler invocation passes
/// lexical, grammar/schema, resolution, type, and required static validation
/// and then either cannot proceed because a dynamic operation precondition is
/// unsatisfied or ends with status.failed or status.blocked."
pub(crate) fn fallback_eligible(record: &ResultRecord) -> bool {
    matches!(record.status.as_str(), "status.failed" | "status.blocked")
}

/// Correspondence is checked by the core; capability-specific safety is the
/// host's proof obligation. A proof cannot rewrite the previous attempt.
fn retry_proof_matches(context: &RetryContext, proof: &RetryProof) -> bool {
    if proof.context != *context
        || proof.post_state.trim().is_empty()
        || proof.evidence.is_empty()
        || proof.evidence.iter().any(|item| item.trim().is_empty())
        || proof.observed_effects.iter().any(|effect| {
            effect.state == RecordState::Indeterminate
                || !context
                    .request
                    .possible_effects
                    .contains(effect.class.as_registry_str())
        })
    {
        return false;
    }
    let consistent = match proof.effect_state {
        EffectState::None => proof.observed_effects.is_empty(),
        EffectState::Applied => {
            !proof.observed_effects.is_empty()
                && proof
                    .observed_effects
                    .iter()
                    .all(|effect| effect.state == RecordState::Applied)
        }
        EffectState::Partial => proof
            .observed_effects
            .iter()
            .any(|effect| effect.state == RecordState::Partial),
        EffectState::Indeterminate => false,
    };
    let uncertain = context.previous.failure_phase == FailurePhase::Indeterminate
        || context.previous.effect_state == EffectState::Indeterminate;
    consistent
        && if uncertain {
            proof.method == RetryMethod::Reconcile
        } else {
            proof.effect_state == context.previous.effect_state
                && proof.observed_effects == context.previous.observed_effects
        }
}
