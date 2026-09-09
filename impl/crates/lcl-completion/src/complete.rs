//! The entry point: one execution in, one finished invocation out.

use crate::check::Checks;
use crate::contracts::Contracts;
use crate::diagnostic::{deduplicate, stable_order, Diagnostic};
use crate::engine::Engine;
use crate::evidence::Evidence;
use crate::observe::Observation;
use crate::outputs::Outputs;
use crate::success::Verdict;
use crate::terminal::Terminal;
use lcl_checker::Checked;
use lcl_resolver::{Resolved, SourceId};
use lcl_runtime::Execution;
use lcl_semantics::Planned;
use std::fmt;

/// Completion refused to run because the execution had no plan behind it.
///
/// Structurally unreachable through [`Completion::of`], which takes an
/// `Execution` that only a successful plan produces. It exists for the
/// [`Completion::finish`] path, which accepts a `Planned` directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotExecuted {
    pub root: SourceId,
}

impl fmt::Display for NotExecuted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} was not completed: preflight produced no plan to execute",
            self.root
        )
    }
}

impl std::error::Error for NotExecuted {}

/// One finished invocation.
///
/// Every field is an observation of what happened, not an instruction about
/// what should have. `05_SEMANTICS/10` keeps producer completion and domain
/// outcome apart, and so does this: [`Completion::checks`] may hold a `VERIFY`
/// that recorded FALSE while [`Completion::terminal`] still reports whatever
/// the canonical order of precedence actually selected.
#[derive(Debug, Clone)]
pub struct Completion {
    root: SourceId,
    observation: Observation,
    checks: Checks,
    verdict: Verdict,
    evidence: Evidence,
    outputs: Outputs,
    terminal: Terminal,
    diagnostics: Vec<Diagnostic>,
}

impl Completion {
    /// Run canonical steps 11 through 13 over one finished execution.
    pub fn of(
        contracts: &Contracts,
        planned: &Planned,
        checked: &Checked,
        resolved: &Resolved,
        execution: &Execution,
    ) -> Result<Completion, NotExecuted> {
        let Some(plan) = planned.plan() else {
            return Err(NotExecuted {
                root: planned.root().clone(),
            });
        };
        let mut engine = Engine::new(contracts, plan, checked, resolved, execution);

        // 11. Post-execution VERIFY and TEST against observed results.
        let checks = crate::check::run(&mut engine);

        // 12. Declared SUCCESS and FAILURE, then the evidence they require.
        //     FAILURE is selected before evidence because its own required
        //     EVIDENCE is only obligated when the clause is selected.
        let verdict = crate::success::run(&mut engine, &checks);
        let evidence = crate::evidence::run(&mut engine, &checks, &verdict);

        // 13. The declared outputs and exactly one terminal status.
        let outputs = crate::outputs::run(&engine);
        let terminal =
            crate::terminal::resolve(&mut engine, &checks, &evidence, &outputs, &verdict);

        let mut diagnostics = deduplicate(engine.diagnostics);
        stable_order(&mut diagnostics);

        Ok(Completion {
            root: execution.root().clone(),
            observation: engine.observation,
            checks,
            verdict,
            evidence,
            outputs,
            terminal,
            diagnostics,
        })
    }

    pub fn root(&self) -> &SourceId {
        &self.root
    }

    /// What the execution actually activated and observed.
    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    /// Post-execution `VERIFY` and `TEST` results, and the selected checks that
    /// were skipped and therefore have none.
    pub fn checks(&self) -> &Checks {
        &self.checks
    }

    /// What `SUCCESS` and `FAILURE` decided. Deliberately status-free.
    pub fn verdict(&self) -> &Verdict {
        &self.verdict
    }

    /// The evidence this invocation required, and whether it resolved.
    pub fn evidence(&self) -> &Evidence {
        &self.evidence
    }

    /// The root's declared outputs and their publication state.
    pub fn outputs(&self) -> &Outputs {
        &self.outputs
    }

    /// Exactly one terminal status, and why it is that one.
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// The status this invocation ended with.
    pub fn terminal_status(&self) -> &str {
        &self.terminal.status
    }

    /// True exactly when the invocation ended `status.succeeded`.
    ///
    /// Named for what it is. It is not a claim that every declared check
    /// passed: an optional `VERIFY` may have recorded FALSE and success may
    /// still hold, because "Optional FALSE checks retain their Boolean domain
    /// outcome without emitting a required-check failure."
    pub fn succeeded(&self) -> bool {
        self.terminal.succeeded()
    }

    /// Diagnostics this completion pass emitted, in `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// A canonical, order-stable rendering, for reports and comparison.
    ///
    /// Contains no address, no timing and no iteration-order-dependent text, so
    /// two completions of the same execution produce identical bytes.
    pub fn serialize(&self) -> String {
        let mut out = String::from("COMPLETION\n");
        out.push_str(&self.checks.serialize());
        out.push_str(&self.verdict.serialize());
        out.push_str(&self.evidence.serialize());
        out.push_str(&self.outputs.serialize());
        for diagnostic in &self.diagnostics {
            out.push_str(&format!("  diagnostic {}\n", diagnostic.serialize()));
        }
        out.push_str(&self.terminal.serialize());
        out.push('\n');
        out
    }
}
