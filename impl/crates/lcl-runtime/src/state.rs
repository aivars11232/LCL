//! Runtime identity and the registered status lifecycle.
//!
//! ## Identity
//!
//! A plan node is a *source template*. What executes is an **invocation**, and
//! `05_SEMANTICS/01` fixes what distinguishes two of them:
//!
//! > Instance identity includes the full enclosing iteration-index path.
//!
//! and
//!
//! > Explicit bounded FOR EACH instances and RETRY attempts replicate their
//! > source template with distinct invocation identities and are not duplicate
//! > activation.
//!
//! So an [`InvocationId`] is a plan node plus its [`IterationPath`] plus, for a
//! retried `ACTION`, its attempt index. Two invocations of the same declaration
//! are the same invocation exactly when all three agree — which is also what
//! `duplicate_key` in the diagnostic-selection contract requires, since it keys
//! on `producer_path`, `iteration_index` and `retry_attempt_index`.
//!
//! ## Lifecycle
//!
//! [`Lifecycle`] does not hardcode a state machine. Every transition is checked
//! against the registry's own `allowed_next` for the current status, so a
//! change to `statuses_and_errors_v0.1.0.json` changes this behaviour without a
//! code change, and an illegal transition is detectable rather than
//! unrepresentable — the failure-lifecycle contract requires the illegal
//! request to be *reported* as `error.execution.order`, not silently prevented:
//!
//! > The selected canonical STATUS must be terminal, non-success, and present
//! > in the current invocation state's allowed_next; an illegal requested
//! > transition uses error.execution.order.

use lcl_diagnostics::DiagnosticRegistry;
use std::collections::BTreeMap;
use std::fmt;

use crate::value::Value;

/// The full enclosing iteration-index path of one invocation.
///
/// Empty for anything not inside a `FOR EACH`. Outermost loop first, so two
/// paths compare in the order the canonical `stable_order` key
/// "iteration index ascending" requires.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IterationPath(Vec<usize>);

impl IterationPath {
    /// The path of an invocation outside every loop.
    pub fn root() -> IterationPath {
        IterationPath(Vec::new())
    }

    /// This path extended by one more enclosing iteration index.
    pub fn child(&self, index: usize) -> IterationPath {
        let mut next = self.0.clone();
        next.push(index);
        IterationPath(next)
    }

    pub fn indexes(&self) -> &[usize] {
        &self.0
    }

    pub fn depth(&self) -> usize {
        self.0.len()
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// True when `self` is `other` or a loop instance nested inside it.
    ///
    /// Used for the "same container and iteration context" rule and for
    /// resolving a loop-local binding, which "is visible only within its body
    /// and resolves to that exact instance".
    pub fn starts_with(&self, other: &IterationPath) -> bool {
        self.0.len() >= other.0.len() && self.0[..other.0.len()] == other.0[..]
    }
}

impl fmt::Display for IterationPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("-");
        }
        for (i, index) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{index}")?;
        }
        Ok(())
    }
}

/// The identity of one executing invocation.
///
/// Ordered so that a `BTreeMap` keyed by it iterates in declared execution-path
/// order, then iteration index, then attempt index — exactly the `stable_order`
/// sequence, so no sort by discovery is ever needed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvocationId {
    /// Index into [`lcl_semantics::Plan::nodes`].
    pub node: usize,
    /// The full enclosing iteration-index path.
    pub iteration: IterationPath,
    /// Zero-based attempt index. `0` is the first attempt; later values exist
    /// only for an `ACTION` under a declared `RETRY`.
    pub attempt: usize,
}

impl InvocationId {
    pub fn new(node: usize, iteration: IterationPath, attempt: usize) -> InvocationId {
        InvocationId {
            node,
            iteration,
            attempt,
        }
    }

    /// The first attempt of this node in this iteration context.
    pub fn first(node: usize, iteration: IterationPath) -> InvocationId {
        InvocationId::new(node, iteration, 0)
    }

    /// The same invocation at the next attempt index.
    pub fn next_attempt(&self) -> InvocationId {
        InvocationId {
            node: self.node,
            iteration: self.iteration.clone(),
            attempt: self.attempt.saturating_add(1),
        }
    }

    /// This invocation ignoring which attempt it is.
    ///
    /// `05_SEMANTICS/09`: one `ACTION` *invocation* makes at most `1 + LIMIT`
    /// attempts, so the budget is owned by the aggregate, not the attempt.
    pub fn aggregate(&self) -> InvocationId {
        InvocationId {
            node: self.node,
            iteration: self.iteration.clone(),
            attempt: 0,
        }
    }
}

impl fmt::Display for InvocationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.node)?;
        if !self.iteration.is_root() {
            write!(f, " iteration {}", self.iteration)?;
        }
        if self.attempt > 0 {
            write!(f, " attempt {}", self.attempt)?;
        }
        Ok(())
    }
}

/// One invocation's position in the registered status lifecycle.
///
/// Holds the *canonical identifier*, never a mirrored enum, because the status
/// vocabulary and its transitions are closed registry data and this layer must
/// not carry a second copy of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lifecycle {
    current: String,
    /// Every status this invocation has held, in order, for evidence.
    history: Vec<String>,
    /// True for an execution root, which "cannot be skipped, including through
    /// a FAILURE mapping or status alias".
    root: bool,
}

/// Why a requested lifecycle transition was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionRefusal {
    /// The requested status is not in the current status's `allowed_next`.
    NotAllowed { from: String, to: String },
    /// `status.skipped` was requested for an execution root.
    RootCannotSkip,
    /// The requested identifier is not a registered status.
    Unregistered(String),
}

impl fmt::Display for TransitionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransitionRefusal::NotAllowed { from, to } => {
                write!(f, "{from} does not permit a transition to {to}")
            }
            TransitionRefusal::RootCannotSkip => {
                f.write_str("an execution root cannot take status.skipped")
            }
            TransitionRefusal::Unregistered(id) => write!(f, "{id} is not a registered status"),
        }
    }
}

impl Lifecycle {
    /// A new invocation, before validation.
    pub fn new(root: bool) -> Lifecycle {
        Lifecycle {
            current: "status.not_started".to_string(),
            history: vec!["status.not_started".to_string()],
            root,
        }
    }

    /// An invocation entering the runtime from an accepted plan.
    ///
    /// Steps 1 through 9 already completed for it, so it starts at
    /// `status.ready`: "Validation passed and required inputs/dependencies are
    /// available." Its `not_started` and `validating` history is retained,
    /// because those states really did occur in the earlier milestones.
    pub fn planned(root: bool) -> Lifecycle {
        Lifecycle {
            current: "status.ready".to_string(),
            history: vec![
                "status.not_started".to_string(),
                "status.validating".to_string(),
                "status.ready".to_string(),
            ],
            root,
        }
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    pub fn is_root(&self) -> bool {
        self.root
    }

    /// True when the current status is registered terminal.
    pub fn is_terminal(&self, registry: &DiagnosticRegistry) -> bool {
        registry
            .status(&self.current)
            .map(|s| s.terminal)
            .unwrap_or(false)
    }

    /// True when this invocation may legally move to `to`.
    pub fn permits(&self, registry: &DiagnosticRegistry, to: &str) -> bool {
        self.refuse(registry, to).is_none()
    }

    /// The reason `to` is refused, or `None` when it is permitted.
    pub fn refuse(&self, registry: &DiagnosticRegistry, to: &str) -> Option<TransitionRefusal> {
        if registry.status(to).is_none() {
            return Some(TransitionRefusal::Unregistered(to.to_string()));
        }
        if to == "status.skipped" && self.root {
            return Some(TransitionRefusal::RootCannotSkip);
        }
        let current = registry.status(&self.current)?;
        if current.allowed_next.iter().any(|next| next == to) {
            return None;
        }
        Some(TransitionRefusal::NotAllowed {
            from: self.current.clone(),
            to: to.to_string(),
        })
    }

    /// Move to `to`, or report exactly why the transition is illegal.
    ///
    /// The caller turns a refusal into `error.execution.order`; this type never
    /// emits a diagnostic of its own.
    pub fn transition(
        &mut self,
        registry: &DiagnosticRegistry,
        to: &str,
    ) -> Result<(), TransitionRefusal> {
        if let Some(refusal) = self.refuse(registry, to) {
            return Err(refusal);
        }
        self.current = to.to_string();
        self.history.push(to.to_string());
        Ok(())
    }
}

/// The values one execution knows: loop-local bindings and bound `OUTPUT`s.
///
/// Both are keyed by iteration path, because both are per-instance:
///
/// > The local binding is visible only within its body and resolves to that
/// > exact instance.
///
/// > An ACTION replicated by FOR EACH has a separate OUTPUT binding per full
/// > enclosing iteration-index path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bindings {
    /// `(binding name, iteration path)` -> the loop-local value.
    locals: BTreeMap<(String, IterationPath), Value>,
    /// `(OUTPUT declaration id, iteration path)` -> the bound projection.
    outputs: BTreeMap<(String, IterationPath), Value>,
}

impl Bindings {
    pub fn new() -> Bindings {
        Bindings::default()
    }

    /// Bind one `FOR EACH` local for exactly one iteration instance.
    pub fn bind_local(&mut self, name: &str, iteration: &IterationPath, value: Value) {
        self.locals
            .insert((name.to_string(), iteration.clone()), value);
    }

    /// Drop every local bound at or inside `iteration`.
    ///
    /// A loop body's binding does not outlive its instance, and nothing outside
    /// the body may read it.
    pub fn release_locals(&mut self, iteration: &IterationPath) {
        self.locals
            .retain(|(_, path), _| !path.starts_with(iteration));
    }

    /// Read a loop-local visible from `iteration`.
    ///
    /// Resolves to the innermost enclosing instance that bound the name, so a
    /// nested loop sees its own binding and an outer loop's alike, and neither
    /// sees a sibling instance's.
    pub fn local(&self, name: &str, iteration: &IterationPath) -> Option<&Value> {
        self.locals
            .iter()
            .filter(|((bound, path), _)| bound == name && iteration.starts_with(path))
            .max_by_key(|((_, path), _)| path.depth())
            .map(|(_, value)| value)
    }

    /// Bind one `OUTPUT` for one producer instance.
    pub fn bind_output(&mut self, id: &str, iteration: &IterationPath, value: Value) {
        self.outputs.insert((id.to_string(), iteration.clone()), value);
    }

    /// Clear one `OUTPUT` binding for one producer instance.
    ///
    /// `05_SEMANTICS/05`: "Each retry attempt starts with its own selected
    /// OUTPUT binding unbound."
    pub fn unbind_output(&mut self, id: &str, iteration: &IterationPath) {
        self.outputs.remove(&(id.to_string(), iteration.clone()));
    }

    /// Read one `OUTPUT` binding visible from `iteration`.
    ///
    /// `None` means unbound; the caller yields `MISSING`, because "Within a
    /// valid instance an output not yet bound yields MISSING" — and never
    /// starts the producer.
    pub fn output(&self, id: &str, iteration: &IterationPath) -> Option<&Value> {
        self.outputs
            .iter()
            .filter(|((bound, path), _)| bound == id && iteration.starts_with(path))
            .max_by_key(|((_, path), _)| path.depth())
            .map(|(_, value)| value)
    }

    /// Every bound output, in id then iteration order.
    pub fn outputs(&self) -> impl Iterator<Item = (&(String, IterationPath), &Value)> {
        self.outputs.iter()
    }
}
