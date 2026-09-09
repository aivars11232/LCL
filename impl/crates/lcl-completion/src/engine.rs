//! The mutable state of one completion pass, and how it emits.
//!
//! One `Engine` finishes exactly one invocation. It never re-executes and
//! never reaches a host: everything it needs about what happened is already in
//! the [`Execution`] it was handed.
//!
//! ## The evaluator is borrowed, not rebuilt
//!
//! Post-execution assertions are ordinary LCL expressions, and M6 already has
//! an evaluator that demands them correctly. This layer therefore builds an
//! [`lcl_runtime::Evaluator`] over the execution's own bindings rather than
//! carrying a second expression implementation, which `5.8 UI/CLI truthfulness`
//! forbids in spirit for exactly the reason it applies here: two evaluators
//! would eventually disagree, and the disagreement would be invisible.
//!
//! ## How a completed check becomes readable
//!
//! `12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS`: "VALIDATE and VERIFY
//! expose the Boolean result of their declared check in value context." The
//! evaluator already resolves a *preflight* check that way, from
//! `Plan::checks`. A completion check has no place in the plan, so this engine
//! binds each completed check's outcome into its own copy of the bindings as
//! the check's declaration id.
//!
//! That copy is the whole mechanism, and it is why a skipped check needs no
//! special case anywhere else. A skipped check is simply never bound, so
//! `declaration_value` falls through to `MISSING` — which is exactly what
//! `demand` requires: "A skipped check has no result; an explicit required read
//! of that absent result uses ordinary MISSING behavior, never implicit TRUE."
//!
//! The binding is made at the root iteration path, the shallowest there is, so
//! a genuine loop-local of the same name still wins the innermost-scope
//! lookup rather than being shadowed by a check result.

use crate::contracts::Contracts;
use crate::diagnostic::{CompletionError, Diagnostic};
use crate::observe::Observation;
use lcl_checker::Checked;
use lcl_lexer::Span;
use lcl_resolver::{Resolved, SourceId};
use lcl_runtime::{Bindings, Evaluator, Execution, FailurePhase, IterationPath, Value};
use lcl_semantics::Plan;

/// One diagnostic to emit.
///
/// A record rather than a long parameter list, so a call site names each part
/// and no two same-typed arguments can silently swap places.
pub(crate) struct Emission<'a> {
    pub(crate) id: CompletionError,
    pub(crate) source: &'a SourceId,
    pub(crate) span: Span,
    /// The declaration this diagnostic is about, when it is about one.
    pub(crate) declaration: Option<String>,
    /// The `cause_identity` component of `duplicate_key`.
    pub(crate) cause: String,
    /// Non-normative human detail.
    pub(crate) detail: String,
    pub(crate) phase: FailurePhase,
}

pub(crate) struct Engine<'a> {
    pub(crate) contracts: &'a Contracts,
    pub(crate) resolved: &'a Resolved,
    pub(crate) checked: &'a Checked,
    pub(crate) plan: &'a Plan,
    pub(crate) execution: &'a Execution,
    pub(crate) observation: Observation,
    /// The execution's bindings plus this pass's completed check results.
    pub(crate) bindings: Bindings,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        contracts: &'a Contracts,
        plan: &'a Plan,
        checked: &'a Checked,
        resolved: &'a Resolved,
        execution: &'a Execution,
    ) -> Engine<'a> {
        Engine {
            contracts,
            resolved,
            checked,
            plan,
            execution,
            observation: Observation::of(execution),
            bindings: execution.bindings().clone(),
            diagnostics: Vec::new(),
        }
    }

    /// The `EXECUTE` root source unit. Only its declarations select checks.
    pub(crate) fn root_source(&self) -> SourceId {
        self.resolved.root().clone()
    }

    /// The declaration the `EXECUTE` root activates, when it activates one.
    pub(crate) fn root_declaration(&self) -> Option<usize> {
        let graph = self.resolved.graph();
        if let Some(declaration) = graph.root().and_then(|node| node.declaration) {
            return Some(declaration);
        }
        // An `EXECUTE` node that carries no declaration of its own activates
        // its first child, which is the referenced root.
        graph
            .root()
            .and_then(|node| node.children.first().copied())
            .and_then(|child| graph.get(child))
            .and_then(|node| node.declaration)
    }

    /// An evaluator over this pass's bindings at the root iteration context.
    pub(crate) fn evaluator(&self) -> Evaluator<'_> {
        Evaluator {
            contracts: self.contracts.runtime(),
            resolved: self.resolved,
            checked: self.checked,
            plan: self.plan,
            bindings: &self.bindings,
            source: self.root_source(),
            iteration: IterationPath::root(),
        }
    }

    /// Publish one completed check's Boolean result under its declaration id.
    pub(crate) fn bind_check_result(&mut self, id: &str, value: Value) {
        self.bindings.bind_local(id, &IterationPath::root(), value);
    }

    /// Emit one registered diagnostic.
    pub(crate) fn emit(&mut self, emission: Emission<'_>) {
        let registered = self.contracts.error(emission.id);
        let text = self
            .resolved
            .unit(emission.source)
            .map(|unit| unit.source())
            .unwrap_or("");
        let position = lcl_runtime::position_of(text, emission.span.start);
        self.diagnostics.push(Diagnostic {
            sequence: self.diagnostics.len(),
            id: emission.id,
            registered_stage: registered.stage,
            source: emission.source.clone(),
            span: emission.span,
            position,
            meaning: registered.meaning.clone(),
            default_status: registered.default_status.clone(),
            specificity_rank: registered.specificity_rank,
            event: registered.event.clone(),
            cause: emission.cause,
            declaration: emission.declaration,
            failure_phase: emission.phase,
            detail: emission.detail,
        });
    }

    /// The failure phase every completion diagnostic carries.
    ///
    /// `failure_lifecycle.phase_scope`: "the aggregate producer derives its
    /// phase from all effects begun within its invocation." Completion runs
    /// after execution, so the phase is read from what the execution actually
    /// recorded, never assumed.
    pub(crate) fn observed_phase(&self) -> FailurePhase {
        let mut any_effect = false;
        let mut any_indeterminate = false;
        for record in self.execution.invocations() {
            let Some(result) = &record.result else {
                continue;
            };
            if !result.observed_effects.is_empty() {
                any_effect = true;
            }
            if matches!(result.effect_state, lcl_runtime::EffectState::Indeterminate) {
                any_indeterminate = true;
            }
        }
        // "indeterminate failure_phase requires effect_state indeterminate
        // unless independent evidence proves the exact effect_state."
        if any_indeterminate {
            return FailurePhase::Indeterminate;
        }
        if any_effect {
            FailurePhase::PostEffect
        } else {
            FailurePhase::PreEffect
        }
    }
}
