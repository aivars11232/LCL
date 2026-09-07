//! The preflight engine: one sequential pass over canonical steps 6 through 9.
//!
//! The order of [`Engine::run`] is the canonical order, not a convenience:
//! authority before conflicts, conflicts before data, data before checks,
//! checks before ordering. Each step consumes what the previous one decided,
//! and none of them reaches back.
//!
//! ## Stopping
//!
//! `earliest_stage_rule`: "If any applicable diagnostic remains unhandled, do
//! not evaluate later stages for that failed source unit or invocation path."
//! Within one preflight the steps span three registered stages, so the engine
//! stops before the first step whose registered stage is later than a stage
//! that already failed. That keeps a validation-stage check from running over a
//! rule set that never resolved, which would report a second, derived failure
//! as though it were independent.

use crate::authority::{AuthorityRecord, ScopeRecord, WorkspaceRecord};
use crate::contracts::Contracts;
use crate::diagnostic::{self, Cause, Diagnostic, FailurePhase, PreflightError};
use crate::plan::Plan;
use crate::{Invocation, Planned};
use lcl_checker::Checked;
use lcl_diagnostics::Stage;
use lcl_lexer::Span;
use lcl_resolver::{Resolved, SourceId};

pub(crate) struct Engine<'a> {
    pub(crate) contracts: &'a Contracts,
    pub(crate) checked: &'a Checked,
    pub(crate) resolved: &'a Resolved,
    pub(crate) invocation: &'a Invocation,
    /// Diagnostics as emitted, before the selection contract is applied.
    pub(crate) raw: Vec<Diagnostic>,
    /// The plan under construction.
    pub(crate) plan: Plan,
    /// Effective authority and priority for every rule clause.
    pub(crate) authorities: Vec<AuthorityRecord>,
    /// Every resolved `SCOPE` declaration.
    pub(crate) scopes: Vec<ScopeRecord>,
    /// Every resolved `WORKSPACE` declaration.
    pub(crate) workspaces: Vec<WorkspaceRecord>,
    /// Supplied invocation data whose id this document does not declare.
    pub(crate) unused: Vec<String>,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(
        contracts: &'a Contracts,
        checked: &'a Checked,
        resolved: &'a Resolved,
        invocation: &'a Invocation,
    ) -> Engine<'a> {
        Engine {
            contracts,
            checked,
            resolved,
            invocation,
            raw: Vec::new(),
            plan: Plan::default(),
            authorities: Vec::new(),
            scopes: Vec::new(),
            workspaces: Vec::new(),
            unused: Vec::new(),
        }
    }

    /// Run steps 6 through 9 in canonical order.
    pub(crate) fn run(&mut self) {
        // Step 6: effective authority, priority, scope and conflicts.
        crate::authority::establish(self);
        crate::scope::resolve(self);
        crate::conflict::resolve(self);
        if self.failed_at_or_before(Stage::Resolution) {
            return;
        }

        // Step 7: input, state, memory, context, default, assumption,
        // dependencies.
        crate::data::resolve(self);
        if self.failed_at_or_before(Stage::Resolution) {
            return;
        }

        // Step 8: every selected and applicable pre-effect VALIDATE.
        crate::validate::run(self);
        if self.failed_at_or_before(Stage::Validation) {
            return;
        }

        // Step 9: finalize and check ordering edges.
        crate::order::finalize(self);
    }

    /// True when a diagnostic of `stage` or earlier has already been emitted.
    pub(crate) fn failed_at_or_before(&self, stage: Stage) -> bool {
        self.raw.iter().any(|d| d.stage.index() <= stage.index())
    }

    /// Emit one registered diagnostic.
    ///
    /// The identifier's stage, meaning, default status, specificity rank and
    /// event come from the registry, never from this call site, so a diagnostic
    /// cannot drift from its canonical metadata.
    pub(crate) fn emit(
        &mut self,
        id: PreflightError,
        source: &SourceId,
        span: Span,
        cause: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let registered = self.contracts.error(id);
        let text = self
            .resolved
            .unit(source)
            .map(|unit| unit.source())
            .unwrap_or("");
        self.raw.push(Diagnostic {
            id,
            stage: registered.stage,
            source: source.clone(),
            span,
            position: diagnostic::position(text, span.start),
            meaning: registered.meaning.clone(),
            default_status: registered.default_status.clone(),
            specificity_rank: registered.specificity_rank,
            event: registered.event.clone(),
            cause: Cause(cause.into()),
            producer_path: None,
            detail: Some(detail.into()),
            // No effect can have begun: this whole layer runs before the first
            // one. `05_SEMANTICS/09`: "pre_effect means the producer failed
            // before any concrete effect began."
            failure_phase: FailurePhase::PreEffect,
        });
    }

    /// Emit one diagnostic against a graph node, recording its declared
    /// execution-path order for `stable_order`.
    pub(crate) fn emit_at_node(
        &mut self,
        id: PreflightError,
        node: usize,
        source: &SourceId,
        span: Span,
        cause: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.emit(id, source, span, cause, detail);
        if let Some(last) = self.raw.last_mut() {
            last.producer_path = Some(node);
        }
    }

    pub(crate) fn finish(self) -> Planned {
        let Engine {
            contracts,
            checked,
            raw,
            mut plan,
            authorities,
            unused,
            ..
        } = self;
        plan.authorities = authorities;
        Planned {
            root: checked.root().clone(),
            plan,
            diagnostics: diagnostic::select(raw, contracts.supersedes()),
            unused_invocation_data: unused,
        }
    }
}
