//! The execution engine: canonical processing step 10.
//!
//! > 10. Evaluate dynamic reachability conditions when reached and execute only
//! >     reachable actions in declared order and authorization bounds.
//!
//! ## What "only reachable" is made to mean
//!
//! The engine's node universe is [`lcl_semantics::Plan::nodes`] and nothing
//! else. There is no path from a source declaration to execution that does not
//! go through a plan node, and [`lcl_semantics::Planned::plan`] hands out
//! `None` for a rejected preflight — so a program preflight refused has no plan
//! to execute, and a declaration outside the accepted candidate graph has no
//! node to execute. "Runtime never executes a node absent from the accepted
//! candidate graph" is therefore a property of the types, not a rule the engine
//! remembers to follow.
//!
//! ## Order
//!
//! Work is a deterministic explicit queue of [`Step`]s, never OS threads. Under
//! `mode.sequential` a container's children are queued in canonical child
//! order. Under `mode.parallel` the contract says "Independent eligible
//! children may execute in any order" *and* "Result collection and diagnostics
//! use declared child order, never finish order" — so the engine may interleave
//! admissible orders while every observable output stays in declared order.
//! `crate::schedule` owns that choice; this module owns what each step does.
//!
//! ## Reachability is evaluated, not assumed
//!
//! An `IF` "evaluates once upon reachability" and only its selected arm becomes
//! reachable. A `FOR EACH` "evaluates its finite snapshot once at reachability"
//! and replicates its body per instance. A `WHEN` that is FALSE moves a
//! non-root declaration from `status.ready` to `status.skipped` "without
//! beginning its operation".

use crate::capability::{self, Authorized, CapabilityOutcome, CapabilityRequest, Host, Refusal};
use crate::contracts::Contracts;
use crate::diagnostic::{Cause, Diagnostic, RuntimeError};
use crate::eval::{Evaluator, Fault};
use crate::event::EventLog;
use crate::operations::{Invocation, Operations, Resolution};
use crate::result::{EffectState, FailurePhase, ObservedEffect, OutputBinding, ResultRecord};
use crate::schedule::{Queue, Step};
use crate::state::{Bindings, InvocationId, IterationPath, Lifecycle};
use crate::syntax::{self, DeclBlock};
use crate::value::Value;
use lcl_checker::Checked;
use lcl_lexer::Span;
use lcl_resolver::{NodeKind, Resolved, SourceId};
use lcl_semantics::{Plan, PlanNode, Planned};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// A bound on total executed steps, so a pathological plan costs time rather
/// than termination. Loop snapshots are finite and retries are bounded, so a
/// conforming program never approaches it.
const MAX_STEPS: usize = 1_000_000;

/// The runtime refused to execute because preflight produced no plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotPlanned {
    pub root: SourceId,
    /// The registered identifier of preflight's primary diagnostic.
    pub primary: Option<String>,
}

impl fmt::Display for NotPlanned {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.primary {
            Some(primary) => write!(
                f,
                "{} was not executed: preflight rejected it with {primary}",
                self.root
            ),
            None => write!(
                f,
                "{} was not executed: preflight produced no plan",
                self.root
            ),
        }
    }
}

impl std::error::Error for NotPlanned {}

/// One invocation the runtime actually entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRecord {
    pub id: InvocationId,
    /// The plan node this invocation activates.
    pub node: usize,
    /// The activated declaration's qualified id, when it activates one.
    pub declaration: Option<String>,
    /// The declaring block, e.g. `ACTION`, or the control form.
    pub block: String,
    pub lifecycle: Lifecycle,
    /// The producer result, for an invocation that produced one.
    pub result: Option<ResultRecord>,
}

impl InvocationRecord {
    pub fn status(&self) -> &str {
        self.lifecycle.current()
    }
}

/// The outcome of one execution.
#[derive(Debug, Clone)]
pub struct Execution {
    pub(crate) root: SourceId,
    pub(crate) invocations: Vec<InvocationRecord>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) events: EventLog,
    pub(crate) bindings: Bindings,
    pub(crate) steps: usize,
    pub(crate) substituted: BTreeSet<usize>,
}

impl Execution {
    pub fn root(&self) -> &SourceId {
        &self.root
    }

    /// Every invocation the runtime entered, in declared execution-path order.
    pub fn invocations(&self) -> &[InvocationRecord] {
        &self.invocations
    }

    /// The invocation record for one plan node in one iteration context.
    pub fn invocation(&self, id: &InvocationId) -> Option<&InvocationRecord> {
        self.invocations.iter().find(|r| &r.id == id)
    }

    /// Every diagnostic, after supersession, duplicate suppression and
    /// `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// The first unhandled diagnostic in stable order, per `primary_rule`.
    ///
    /// Two kinds of diagnostic are excluded, and both are still retained as
    /// evidence. A diagnostic a selected handler recovered — "The originating
    /// diagnostic is recovered exactly when that handler invocation's own
    /// result ... records status.succeeded." And a diagnostic of an invocation
    /// a successful `FALLBACK` substituted for, which "remains ordered local
    /// evidence and never controls that successful aggregate result".
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|diagnostic| {
            !self.events.recovered(diagnostic.sequence)
                && !self.substituted.contains(&diagnostic.sequence)
        })
    }

    /// Every raised event occurrence, in the order raised.
    pub fn events(&self) -> &[crate::event::EventRecord] {
        self.events.records()
    }

    /// Every `OUTPUT` binding this execution produced.
    pub fn bindings(&self) -> &Bindings {
        &self.bindings
    }

    /// The resolved `default_status` of the primary unhandled diagnostic.
    ///
    /// `failure_lifecycle.status_rule`: "The primary unhandled diagnostic
    /// contributes its default_status after canonical alias resolution and any
    /// exact expression-demand override."
    ///
    /// `None` means no diagnostic fixes a status — which is **not** a claim of
    /// success. `SUCCESS`/`FAILURE` selection and the one terminal root status
    /// are canonical steps 12 and 13, and belong to the next milestone.
    pub fn terminal_status(&self) -> Option<&str> {
        self.primary().map(|d| d.default_status.as_str())
    }

    /// How many steps the queue executed. Evidence that the run was bounded.
    pub fn steps(&self) -> usize {
        self.steps
    }

    /// A canonical, order-stable rendering, for reports and comparison.
    ///
    /// Contains no address, no timing and no iteration-order-dependent text, so
    /// two runs of the same program over the same mock host produce identical
    /// bytes.
    pub fn serialize(&self) -> String {
        let mut out = String::from("EXECUTION\n");
        for record in &self.invocations {
            out.push_str(&format!(
                "  {} {} {} status={}",
                record.id,
                record.block,
                record.declaration.as_deref().unwrap_or("-"),
                record.status()
            ));
            if let Some(result) = &record.result {
                out.push_str(&format!(" -> {}", result.serialize()));
            }
            out.push('\n');
        }
        for diagnostic in &self.diagnostics {
            out.push_str(&format!(
                "  diagnostic {} stage={} status={}\n",
                diagnostic.id,
                diagnostic.stage().as_registry_str(),
                diagnostic.default_status
            ));
        }
        for event in self.events.records() {
            out.push_str(&format!("  event {event}\n"));
        }
        for ((id, path), value) in self.bindings.outputs() {
            out.push_str(&format!("  bound {id} [{path}] = {value}\n"));
        }
        out
    }
}

/// A runtime bound to loaded contracts.
///
/// Holds no mutable state: the same `Runtime` may execute any number of plans,
/// in any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Runtime<'a> {
    contracts: &'a Contracts,
    interleaving: crate::schedule::Interleaving,
}

impl<'a> Runtime<'a> {
    pub fn new(contracts: &'a Contracts) -> Runtime<'a> {
        Runtime {
            contracts,
            interleaving: crate::schedule::Interleaving::Declared,
        }
    }

    /// The same runtime, interleaving provably independent parallel children
    /// differently.
    ///
    /// `execution_graph_contract/parallel`: "Independent eligible children may
    /// execute in any order." Every interleaving is admissible, and none may
    /// change the observable result — which is a property a test can only
    /// check by running more than one.
    pub fn with_interleaving(self, interleaving: crate::schedule::Interleaving) -> Runtime<'a> {
        Runtime {
            interleaving,
            ..self
        }
    }

    pub fn contracts(&self) -> &'a Contracts {
        self.contracts
    }

    /// Execute one accepted plan.
    ///
    /// Returns [`NotPlanned`] when preflight rejected the program, because a
    /// rejected preflight authorizes nothing and therefore hands the runtime
    /// nothing to run.
    pub fn execute(
        &self,
        planned: &Planned,
        checked: &Checked,
        resolved: &Resolved,
        host: &mut dyn Host,
    ) -> Result<Execution, NotPlanned> {
        let mut operations = crate::operations::DeferAll;
        self.execute_with(planned, checked, resolved, &mut operations, host)
    }

    /// Execute one accepted plan against a supplied operation surface.
    ///
    /// The dispatcher answers first. It may compute a result itself, select a
    /// registered error from the operation's own contract, or hand back the
    /// resolved request to cross the capability boundary — and only that third
    /// answer reaches the host. `Operations` and `Host` stay separate because
    /// the standard library decides language meaning and a host never may.
    pub fn execute_with(
        &self,
        planned: &Planned,
        checked: &Checked,
        resolved: &Resolved,
        operations: &mut dyn Operations,
        host: &mut dyn Host,
    ) -> Result<Execution, NotPlanned> {
        let Some(plan) = planned.plan() else {
            return Err(NotPlanned {
                root: planned.root().clone(),
                primary: planned
                    .primary()
                    .map(|d| d.id.as_registry_str().to_string()),
            });
        };
        let mut engine = Engine::new(self.contracts, plan, checked, resolved, operations, host);
        engine.queue = crate::schedule::Queue::with_interleaving(self.interleaving);
        engine.run();
        Ok(engine.finish(planned.root().clone()))
    }
}

/// The engine's mutable state for one run.
pub(crate) struct Engine<'a> {
    pub(crate) contracts: &'a Contracts,
    pub(crate) plan: &'a Plan,
    pub(crate) checked: &'a Checked,
    pub(crate) resolved: &'a Resolved,
    pub(crate) host: &'a mut dyn Host,
    /// The executable operation surface, when one was supplied.
    pub(crate) operations: &'a mut dyn Operations,
    pub(crate) queue: Queue,
    pub(crate) bindings: Bindings,
    pub(crate) records: BTreeMap<InvocationId, InvocationRecord>,
    pub(crate) raw: Vec<Diagnostic>,
    pub(crate) events: EventLog,
    pub(crate) steps: usize,
    /// Invocations whose subtree must not be entered, because an ancestor was
    /// skipped or failed unrecoverably.
    pub(crate) abandoned: BTreeSet<usize>,
    /// Depth of nested handler activity.
    ///
    /// `non_reentrancy_rule`: a diagnostic raised "while evaluating a candidate
    /// WHEN, while resolving a handler's invocation contract, or while
    /// executing a handler invocation or its FALLBACK for the same originating
    /// diagnostic" raises no event. A depth rather than a flag, because a
    /// permitted FALLBACK nests inside its handler.
    pub(crate) in_handler: usize,
    /// Set when an unrecovered required failure stopped the run.
    ///
    /// `05_SEMANTICS/09`: "During execution, failure of a required action
    /// invokes applicable handler/retry; otherwise status.failed." Nothing
    /// after it is reachable until a selected handler recovers or a
    /// `core.continue` commits advancement.
    pub(crate) halted: bool,
    /// A successor `core.continue` requested, committed only on handler
    /// success.
    pub(crate) continuation: Option<(usize, IterationPath)>,
    /// A terminal status `core.stop` or `core.cancel` requested.
    pub(crate) requested_status: Option<String>,
    /// The successor a committed `core.continue` advanced to, as evidence.
    pub(crate) continued: Option<usize>,
    /// Emission identities of diagnostics belonging to an invocation a
    /// successful `FALLBACK` substituted for.
    ///
    /// `secondary_rule`: such a diagnostic "remains ordered local evidence and
    /// never controls that successful aggregate result" — so it is retained,
    /// and it is not primary.
    pub(crate) substituted: BTreeSet<usize>,
}

impl<'a> Engine<'a> {
    fn new(
        contracts: &'a Contracts,
        plan: &'a Plan,
        checked: &'a Checked,
        resolved: &'a Resolved,
        operations: &'a mut dyn Operations,
        host: &'a mut dyn Host,
    ) -> Engine<'a> {
        Engine {
            contracts,
            plan,
            checked,
            resolved,
            host,
            operations,
            queue: Queue::new(),
            bindings: Bindings::new(),
            records: BTreeMap::new(),
            raw: Vec::new(),
            events: EventLog::new(),
            steps: 0,
            abandoned: BTreeSet::new(),
            in_handler: 0,
            halted: false,
            continuation: None,
            requested_status: None,
            continued: None,
            substituted: BTreeSet::new(),
        }
    }

    /// Walk the plan from its root.
    fn run(&mut self) {
        if self.plan.is_empty() {
            return;
        }
        self.queue.push(Step::Enter {
            node: 0,
            iteration: IterationPath::root(),
        });
        while let Some(step) = self.queue.take() {
            self.steps += 1;
            if self.steps > MAX_STEPS {
                let node = self.plan.node(0);
                if let Some(node) = node {
                    // The budget is a runtime bound, not a producer failure:
                    // it selects no handler, so its occurrence is discarded.
                    let _ = self.emit(
                        RuntimeError::ExecutionOrder,
                        &node.source,
                        node.span,
                        "step budget",
                        "the execution exceeded this runtime's bounded step budget",
                        None,
                        FailurePhase::Indeterminate,
                    );
                }
                return;
            }
            match step {
                Step::Enter { node, iteration } => self.enter(node, iteration),
                Step::Leave { node, iteration } => self.leave(node, iteration),
                Step::Iterate {
                    node,
                    iteration,
                    snapshot,
                    index,
                } => self.iterate(node, iteration, snapshot, index),
            }
        }
    }

    fn finish(self, root: SourceId) -> Execution {
        let diagnostics = crate::diagnostic::select(self.raw, self.contracts.supersedes());
        Execution {
            root,
            invocations: self.records.into_values().collect(),
            diagnostics,
            events: self.events,
            bindings: self.bindings,
            steps: self.steps,
            substituted: self.substituted,
        }
    }

    // -----------------------------------------------------------------------
    // Steps
    // -----------------------------------------------------------------------

    fn enter(&mut self, node: usize, iteration: IterationPath) {
        let Some(planned) = self.plan.node(node).cloned() else {
            return;
        };
        if self.abandoned.contains(&node) || self.halted {
            return;
        }
        let id = InvocationId::first(node, iteration.clone());
        let is_root = planned.parent.is_none();
        let mut lifecycle = Lifecycle::planned(is_root);

        // An `IF` or `ELSE` arm is entered only when its condition selected it,
        // which `select_branch` decided at the parent. A `FOR EACH` body is
        // entered per instance by `iterate`.
        match planned.kind {
            NodeKind::BranchTemplate | NodeKind::LoopTemplate => {
                // Reached here only through an explicit selection or iteration
                // step, so the arm is reachable by construction.
            }
            _ => {}
        }

        // "A non-root declaration with FALSE applicability or optional omission
        // transitions from status.ready to status.skipped without beginning its
        // operation. An execution root cannot take this transition."
        match self.applicable(&planned, &iteration) {
            Applicability::Applicable => {}
            Applicability::Skipped => {
                if !is_root {
                    let _ = lifecycle.transition(self.contracts.diagnostics(), "status.skipped");
                    self.record(id, &planned, lifecycle, None);
                    self.abandoned.insert(node);
                    return;
                }
            }
            Applicability::Faulted(fault) => {
                self.fault(&fault, &planned, &id, FailurePhase::PreEffect);
                let _ = lifecycle.transition(self.contracts.diagnostics(), "status.failed");
                self.record(id, &planned, lifecycle, None);
                self.abandoned.insert(node);
                return;
            }
        }

        // "A root enters status.running before evaluating dynamic execution or
        // control or post-execution completion."
        let _ = lifecycle.transition(self.contracts.diagnostics(), "status.running");

        match planned.kind {
            NodeKind::LoopTemplate => {
                self.record(id.clone(), &planned, lifecycle, None);
                self.begin_loop(node, &planned, iteration);
            }
            _ if planned.block == "ACTION" => {
                // Every attempt records itself, so the aggregate's attempt
                // history is evidence rather than a summary.
                self.perform_action(&planned, &id, &iteration);
            }
            _ => {
                self.record(id.clone(), &planned, lifecycle, None);
                self.enter_children(&planned, &iteration);
            }
        }
    }

    /// Queue a container's children in canonical order.
    ///
    /// `child_order`: children "contribute in source field order" and "in
    /// lexical order", which is the order the resolver recorded, so this queues
    /// `planned.children` as-is. Both `IF` arms are children of the same
    /// parent, so exactly one is selected here rather than both queued.
    fn enter_children(&mut self, planned: &PlanNode, iteration: &IterationPath) {
        let children = self.selected_children(planned, iteration);
        // A container finishes after its children, so `Leave` is queued first
        // and the queue pops it last.
        self.queue.push(Step::Leave {
            node: self.index_of(planned),
            iteration: iteration.clone(),
        });
        self.queue.extend(
            children.into_iter().map(|child| Step::Enter {
                node: child,
                iteration: iteration.clone(),
            }),
            planned.mode == lcl_semantics::Mode::Parallel,
        );
    }

    /// The children that are actually reachable from this container.
    ///
    /// For a container holding `IF` arms, "IF evaluates once upon reachability"
    /// and only the selected arm is reachable. Every other child is reachable
    /// as declared.
    fn selected_children(&mut self, planned: &PlanNode, iteration: &IterationPath) -> Vec<usize> {
        let mut out = Vec::new();
        let mut handled: BTreeSet<usize> = BTreeSet::new();
        for &child in &planned.children {
            if handled.contains(&child) {
                continue;
            }
            let Some(node) = self.plan.node(child) else {
                continue;
            };
            if node.kind != NodeKind::BranchTemplate {
                out.push(child);
                continue;
            }
            // The `IF` arm and its optional `ELSE` sibling form one decision.
            let arms = self.branch_arms(planned, child);
            for arm in &arms {
                handled.insert(*arm);
            }
            match self.select_branch(planned, &arms, iteration) {
                Ok(Some(selected)) => out.push(selected),
                // A FALSE condition with no `ELSE` selects nothing.
                Ok(None) => {}
                Err(fault) => {
                    let id = InvocationId::first(child, iteration.clone());
                    self.fault(&fault, node, &id, FailurePhase::PreEffect);
                }
            }
        }
        out
    }

    /// The `IF` arm and its `ELSE` sibling, if the source declares one.
    fn branch_arms(&self, parent: &PlanNode, first: usize) -> Vec<usize> {
        let mut arms = vec![first];
        let mut seen_first = false;
        for &child in &parent.children {
            if child == first {
                seen_first = true;
                continue;
            }
            if !seen_first {
                continue;
            }
            match self.plan.node(child) {
                Some(node) if node.kind == NodeKind::BranchTemplate && node.block == "ELSE" => {
                    arms.push(child);
                    break;
                }
                _ => break,
            }
        }
        arms
    }

    /// Evaluate one `IF` condition and select its arm.
    ///
    /// "IF evaluates once upon reachability." The condition is demanded exactly
    /// once here, and the arm not selected is never entered, so nothing inside
    /// it is demanded either.
    fn select_branch(
        &mut self,
        parent: &PlanNode,
        arms: &[usize],
        iteration: &IterationPath,
    ) -> Result<Option<usize>, Fault> {
        let Some(&then_arm) = arms.first() else {
            return Ok(None);
        };
        let Some(arm) = self.plan.node(then_arm) else {
            return Ok(None);
        };
        let Some(document) = self
            .resolved
            .unit(&arm.source)
            .and_then(|unit| unit.document())
        else {
            return Ok(None);
        };
        let Some(conditional) = syntax::conditional_at(document, arm.span.start) else {
            return Ok(None);
        };
        let condition = conditional.condition.clone();
        let evaluator = self.evaluator(&arm.source, iteration);
        let value = evaluator.demand(&condition)?;
        let _ = parent;
        match value {
            Value::Boolean(true) => Ok(Some(then_arm)),
            Value::Boolean(false) => Ok(arms.get(1).copied()),
            // A required condition that is MISSING or UNKNOWN cannot select a
            // branch; the registered identifiers are exact.
            Value::Missing => Err(Fault::new(
                self.contracts,
                RuntimeError::RequiredMissing,
                conditional.condition.span(),
                "branch condition",
                "a required branch condition yielded MISSING at its demand point",
            )),
            Value::Unknown => Err(Fault::new(
                self.contracts,
                RuntimeError::ValueUnknown,
                conditional.condition.span(),
                "branch condition",
                "a required branch condition yielded UNKNOWN at its demand point",
            )),
            other => Err(Fault::new(
                self.contracts,
                RuntimeError::OperatorOperand,
                conditional.condition.span(),
                "branch condition",
                format!(
                    "a branch condition must be BOOLEAN, found {}",
                    other.family()
                ),
            )),
        }
    }

    /// Take a `FOR EACH` snapshot and queue its first instance.
    ///
    /// "FOR EACH evaluates its finite snapshot once at reachability. Iterations
    /// run sequentially in registered LIST or SET order; each body finishes
    /// before the next starts."
    fn begin_loop(&mut self, node: usize, planned: &PlanNode, iteration: IterationPath) {
        let Some(document) = self
            .resolved
            .unit(&planned.source)
            .and_then(|unit| unit.document())
        else {
            return;
        };
        let Some(for_each) = syntax::for_each_at(document, planned.span.start) else {
            return;
        };
        let collection = for_each.collection.clone();
        let span = for_each.collection.span();
        let evaluator = self.evaluator(&planned.source, &iteration);
        let snapshot = match evaluator.demand(&collection) {
            Ok(value) => value,
            Err(fault) => {
                let id = InvocationId::first(node, iteration.clone());
                self.fault(&fault, planned, &id, FailurePhase::PreEffect);
                self.abandoned.insert(node);
                return;
            }
        };

        let members: Vec<Value> = match &snapshot {
            Value::List(items) => items.clone(),
            // "Direct SET iteration is legal only when every pair of actual
            // members is mutually order-compatible under the registered
            // total-order profile and then uses canonical ascending order.
            // Otherwise direct iteration produces error.type.mismatch before
            // that iteration or any of its effects."
            Value::Set(items) => match crate::eval::sorted(items) {
                Some(ordered) => ordered,
                None => {
                    let id = InvocationId::first(node, iteration.clone());
                    let fault = Fault::new(
                        self.contracts,
                        RuntimeError::TypeMismatch,
                        span,
                        "set iteration order",
                        "a directly iterated SET has actual members that are not mutually \
                         order-compatible under the registered total-order profile",
                    );
                    self.fault(&fault, planned, &id, FailurePhase::PreEffect);
                    self.abandoned.insert(node);
                    return;
                }
            },
            Value::Missing => {
                let id = InvocationId::first(node, iteration.clone());
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::RequiredMissing,
                    span,
                    "loop collection",
                    "a required loop collection yielded MISSING at its demand point",
                );
                self.fault(&fault, planned, &id, FailurePhase::PreEffect);
                self.abandoned.insert(node);
                return;
            }
            Value::Unknown => {
                let id = InvocationId::first(node, iteration.clone());
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::ValueUnknown,
                    span,
                    "loop collection",
                    "a required loop collection yielded UNKNOWN at its demand point",
                );
                self.fault(&fault, planned, &id, FailurePhase::PreEffect);
                self.abandoned.insert(node);
                return;
            }
            other => {
                let id = InvocationId::first(node, iteration.clone());
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::OperatorOperand,
                    span,
                    "loop collection",
                    format!("FOR EACH requires a collection, found {}", other.family()),
                );
                self.fault(&fault, planned, &id, FailurePhase::PreEffect);
                self.abandoned.insert(node);
                return;
            }
        };

        // The loop node finishes after every instance.
        self.queue.push(Step::Leave {
            node,
            iteration: iteration.clone(),
        });
        self.queue.push(Step::Iterate {
            node,
            iteration,
            snapshot: members,
            index: 0,
        });
    }

    /// Run one loop instance, then queue the next.
    ///
    /// Sequential by construction: the next `Iterate` is queued only after this
    /// instance's body has been queued, and the queue is LIFO, so "each body
    /// finishes before the next starts".
    fn iterate(
        &mut self,
        node: usize,
        iteration: IterationPath,
        snapshot: Vec<Value>,
        index: usize,
    ) {
        let Some(planned) = self.plan.node(node).cloned() else {
            return;
        };
        let Some(member) = snapshot.get(index).cloned() else {
            // Every instance is finished; release the loop-local bindings.
            self.bindings.release_locals(&iteration);
            return;
        };
        let Some(document) = self
            .resolved
            .unit(&planned.source)
            .and_then(|unit| unit.document())
        else {
            return;
        };
        let Some(for_each) = syntax::for_each_at(document, planned.span.start) else {
            return;
        };
        let binding = for_each.binding.text.clone();

        // "Instance identity includes the full enclosing iteration-index path."
        let instance = iteration.child(index);
        self.bindings.bind_local(&binding, &instance, member);

        // Queue the next instance first so it pops last: the whole body runs
        // before the next iteration begins.
        self.queue.push(Step::Iterate {
            node,
            iteration: iteration.clone(),
            snapshot,
            index: index + 1,
        });
        let children = self.selected_children(&planned, &instance);
        self.queue.extend(
            children.into_iter().map(|child| Step::Enter {
                node: child,
                iteration: instance.clone(),
            }),
            false,
        );
    }

    fn leave(&mut self, node: usize, iteration: IterationPath) {
        let Some(planned) = self.plan.node(node).cloned() else {
            return;
        };
        let id = InvocationId::first(node, iteration);
        // A container completes once its children have, and this milestone
        // records that it finished running without its own failure.
        //
        // An execution *root* is the exception, and stays in `status.running`.
        // `check_selection_contract/lifecycle`: "A root enters status.running
        // before evaluating dynamic execution/control or post-execution
        // completion." Post-execution completion is step 11 onward, so the root
        // is still running when it begins — and `05_SEMANTICS/10` is explicit
        // that "At a TASK execution root, status.succeeded is legal only when
        // its SUCCESS is TRUE, all required outputs are fully bound and valid,
        // all hard rules hold, and all required evidence exists." None of those
        // has been evaluated here.
        //
        // Asserting `status.succeeded` for a root would also make every
        // declared FAILURE mapping illegal, because `failure_mapping_rule`
        // requires the requested status to be "present in the current
        // invocation state's allowed_next" and a terminal status has none.
        let is_root = planned.parent.is_none();
        if let Some(record) = self.records.get_mut(&id) {
            if !is_root && record.lifecycle.current() == "status.running" {
                let _ = record
                    .lifecycle
                    .transition(self.contracts.diagnostics(), "status.succeeded");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Applicability
    // -----------------------------------------------------------------------

    fn applicable(&mut self, planned: &PlanNode, iteration: &IterationPath) -> Applicability {
        let Some(declaration) = planned.declaration else {
            return Applicability::Applicable;
        };
        let Some(block) = syntax::declaration_block(self.resolved, declaration) else {
            return Applicability::Applicable;
        };
        let Some(when) = syntax::field_expr(&block, "WHEN") else {
            // "Where WHEN exists, absence means TRUE".
            return Applicability::Applicable;
        };
        let when = when.clone();
        let evaluator = self.evaluator(&planned.source, iteration);
        match evaluator.demand(&when) {
            Ok(Value::Boolean(true)) => Applicability::Applicable,
            Ok(Value::Boolean(false)) => Applicability::Skipped,
            Ok(Value::Missing) => Applicability::Faulted(Fault::new(
                self.contracts,
                RuntimeError::RequiredMissing,
                when.span(),
                "applicability condition",
                "a required applicability condition yielded MISSING at its demand point",
            )),
            Ok(Value::Unknown) => Applicability::Faulted(Fault::new(
                self.contracts,
                RuntimeError::ValueUnknown,
                when.span(),
                "applicability condition",
                "a required applicability condition yielded UNKNOWN at its demand point",
            )),
            Ok(other) => Applicability::Faulted(Fault::new(
                self.contracts,
                RuntimeError::OperatorOperand,
                when.span(),
                "applicability condition",
                format!("WHEN must be BOOLEAN, found {}", other.family()),
            )),
            Err(fault) => Applicability::Faulted(fault),
        }
    }

    // -----------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------

    /// Execute one reachable `ACTION` "in declared order and authorization
    /// bounds", including every attempt its declared `RETRY` authorizes.
    ///
    /// Each attempt records itself under its own [`InvocationId`], so "Every
    /// attempt result and diagnostic remains in attempt-index order" is the
    /// shape of the evidence rather than a claim about it.
    fn perform_action(&mut self, planned: &PlanNode, id: &InvocationId, iteration: &IterationPath) {
        let selected_output = planned
            .declaration
            .and_then(|d| syntax::declaration_block(self.resolved, d))
            .and_then(|block| syntax::field_expr(&block, "OUTPUT").cloned());

        let mut attempt = 0usize;
        // Every event this aggregate's attempts raised. A later successful
        // attempt recovers them: "A successful retry recovers the originating
        // failure under the handler contract."
        let mut raised: Vec<usize> = Vec::new();
        loop {
            let attempt_id = InvocationId::new(id.node, iteration.clone(), attempt);

            // "Each retry attempt starts with its own selected OUTPUT binding
            // unbound. Prior attempt bindings remain ordered local evidence and
            // are not reused as the next attempt output."
            if attempt > 0 {
                if let Some(target) = selected_output.as_ref().and_then(output_reference) {
                    self.bindings.unbind_output(target, iteration);
                }
            }

            let mut lifecycle = Lifecycle::planned(false);
            let _ = lifecycle.transition(self.contracts.diagnostics(), "status.running");

            let (record, occurrence) = match self.attempt(planned, &attempt_id, iteration) {
                Some(outcome) => outcome,
                None => return,
            };
            if let Some(occurrence) = occurrence {
                raised.push(occurrence);
            }

            // The aggregate is recorded while still *active*: "The owning
            // aggregate stays active during event selection, the selected
            // handler, and its fallback chain; it finalizes only after that
            // resolution finishes." A control operation such as `core.stop`
            // acts on that active aggregate, so finalizing here would make its
            // registered transition illegal.
            self.record(
                attempt_id.clone(),
                planned,
                lifecycle.clone(),
                Some(record.clone()),
            );

            if record.succeeded() {
                self.finalize(&attempt_id, &record.status);
                // A successful attempt after an earlier failure recovers it.
                for occurrence in &raised {
                    self.events.dispose(
                        *occurrence,
                        crate::event::Disposition::Selected {
                            handler: "core.retry".to_string(),
                            recovered: true,
                        },
                    );
                }
                return;
            }

            // A failed attempt raised its event at this producer; selection
            // decides what happens next.
            match self.handle(occurrence, planned, &attempt_id, iteration) {
                // "the selected core.retry handler authorizes additional
                // attempts"
                crate::handler::Handled::Retry { .. } => {
                    self.finalize(&attempt_id, &record.status);
                    attempt += 1;
                    continue;
                }
                crate::handler::Handled::Recovered { .. } => {
                    // "Commit recovery and advancement only when the selected
                    // handler ... succeeds."
                    //
                    // Advancement is *permitted*, not manufactured: the
                    // declared successor is already queued by its enclosing
                    // sequential container, and `continue_rule` forbids
                    // creating one — "continuation creates no successor,
                    // repeated execution, reachability, or authority." What
                    // recovery changes is that the run is not halted, so the
                    // successor that was already reachable is reached.
                    self.continued = self.continuation.take().map(|(node, _)| node);
                    match self.requested_status.take() {
                        // A declared stop or cancel finalizes the aggregate to
                        // the status its control operation requested.
                        Some(status) => {
                            self.finalize(&attempt_id, &status);
                            self.halted = true;
                        }
                        None => self.finalize(&attempt_id, &record.status),
                    }
                    return;
                }
                _ => {
                    // A failed handler does not advance, and a requested stop
                    // or cancel whose handler did not complete commits nothing.
                    self.continuation = None;
                    self.requested_status = None;
                    self.finalize(&attempt_id, &record.status);
                    // "failure of a required action ... otherwise
                    // status.failed": nothing *after* an unrecovered required
                    // failure is reachable.
                    //
                    // "After" means along a sequential edge. `mode.parallel`
                    // "omits implicit sequential edges", so an independent
                    // sibling is not after this failure and is not cancelled by
                    // it — cancelling it would make the observable result
                    // depend on which child the queue happened to run first,
                    // which "Completion semantics remain deterministic"
                    // forbids.
                    if planned.required {
                        self.halt_successors_of(&attempt_id);
                    }
                    return;
                }
            }
        }
    }

    /// One attempt of one `ACTION`.
    ///
    /// Returns the producer result and the event occurrence its failure raised,
    /// if any.
    fn attempt(
        &mut self,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) -> Option<(ResultRecord, Option<usize>)> {
        let declaration = planned.declaration?;
        let block = syntax::declaration_block(self.resolved, declaration)?;
        let operation = syntax::field_text(&block, "OPERATION")?;

        // The authorization is the plan's, decided before effects at step 6.
        let authorization = planned.authorization.as_ref().map(Authorized::from_plan);
        let authorized = authorization.is_some();
        let authorization = authorization.unwrap_or(Authorized {
            operation: operation.clone(),
            target: None,
            scope: None,
            permitted_by: Vec::new(),
            overridden: Vec::new(),
        });

        // The target and named parameters are demanded now.
        let target = match syntax::field_expr(&block, "TARGET") {
            Some(expr) => {
                let expr = expr.clone();
                match self.evaluator(&planned.source, iteration).demand(&expr) {
                    Ok(value) => Some(value),
                    Err(fault) => {
                        let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                        return Some((self.pre_effect_failure(&operation, fault.id), occurrence));
                    }
                }
            }
            None => None,
        };
        let parameters = match self.parameters(&block, planned, iteration) {
            Ok(parameters) => parameters,
            Err(fault) => {
                let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                return Some((self.pre_effect_failure(&operation, fault.id), occurrence));
            }
        };

        // `core.cancel` and `core.stop` over an internal execution unit change
        // the runtime's own lifecycle state, which no host owns:
        //
        // > Resolve only the referenced internal execution-unit state, require
        // > its registered allowed_next set to contain status.cancelled, and
        // > record the exact reason.
        //
        // > Explicit cancellation outside a handler remains permitted under the
        // > registered authority and transition checks.
        //
        // A `core.stop` whose target is a path, a command or a service is not
        // an internal unit and continues to the boundary below.
        if let Some(status) = internal_terminal_status(&operation) {
            if let Some(unit) = self.internal_unit_target(&block) {
                let record = self.request_terminal_status(planned, id, &unit, status);
                return Some((record, None));
            }
        }

        let axes = self.contracts.preflight().operation_axes(&operation);
        let schema = self
            .contracts
            .operation_schema(&operation)
            .map(|s| s.id.clone())
            .unwrap_or_else(|| "result.operation".to_string());

        let request = CapabilityRequest {
            operation: operation.clone(),
            target,
            parameters,
            authorization,
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

        // The operation surface answers first. An unauthorized invocation is
        // not offered to it at all: step 6 decided that before effects, and a
        // dispatcher that could see an unauthorized request could act on one.
        let (mut record, occurrence) = if authorized {
            match self.dispatch(&request, planned, iteration) {
                Resolution::Completed(observation) => self.record_of(
                    &schema,
                    Ok(CapabilityOutcome::Completed(observation)),
                    planned,
                    id,
                ),
                // The operation's own contract selected this identifier, before
                // any effect and without asking a host.
                Resolution::Failed {
                    error,
                    cause,
                    detail,
                } => {
                    let fault = Fault::new(self.contracts, error, planned.span, cause, detail);
                    let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                    (self.pre_effect_failure(&schema, error), occurrence)
                }
                Resolution::Host(resolved_request) => {
                    let outcome = capability::request(self.host, true, &resolved_request);
                    // "A host may not report an effect outside it." The
                    // invocation's resolved effect set is a language decision;
                    // a host that reported a class outside it would be adding
                    // meaning the document never authorized, so the claim is
                    // refused rather than recorded.
                    if let Some(class) = reported_outside(&outcome, &resolved_request) {
                        let fault = Fault::new(
                            self.contracts,
                            RuntimeError::OperationPostcondition,
                            planned.span,
                            "observed_effect",
                            format!(
                                "the host reported a {class} effect, which {} did not resolve",
                                resolved_request.operation
                            ),
                        );
                        let occurrence =
                            self.fault(&fault, planned, id, FailurePhase::Indeterminate);
                        let mut record =
                            self.pre_effect_failure(&schema, RuntimeError::OperationPostcondition);
                        // The host says something happened and cannot say what,
                        // so neither the phase nor the effect state is knowable:
                        // "Absence of evidence never proves absence of effects."
                        record.failure_phase = FailurePhase::Indeterminate;
                        record.effect_state = EffectState::Indeterminate;
                        (record, occurrence)
                    } else {
                        self.record_of(&schema, outcome, planned, id)
                    }
                }
            }
        } else {
            let outcome = capability::request(self.host, false, &request);
            self.record_of(&schema, outcome, planned, id)
        };

        // Bind the selected OUTPUT from the producer result.
        if let Some(output) = syntax::field_expr(&block, "OUTPUT") {
            let output = output.clone();
            self.bind_output(&output, &mut record, planned, id, iteration);
        }
        Some((record, occurrence))
    }

    /// The declaration one `TARGET` names, when it names an execution unit.
    ///
    /// `meta.execution_unit` is "TASK, PHASE, SEQUENCE, STEP, ACTION, or TEST
    /// reference", read from the invocation site's own `REF(...)` rather than
    /// from the value reading it produced.
    fn internal_unit_target(&self, block: &DeclBlock<'_>) -> Option<String> {
        let expr = syntax::field_expr(block, "TARGET")?;
        let lcl_parser::syntax::Expr::Call(call) = expr else {
            return None;
        };
        let id = call.reference_target()?.text.clone();
        let declaration = self
            .resolved
            .declarations()
            .all()
            .iter()
            .find(|d| d.id.qualified() == id)?;
        matches!(
            declaration.block.as_str(),
            "TASK" | "PHASE" | "SEQUENCE" | "STEP" | "ACTION" | "TEST"
        )
        .then_some(id)
    }

    /// Request one terminal status for a named internal execution unit.
    ///
    /// The registered `allowed_next` set decides whether the transition is
    /// permitted: "a current status that does not allow status.cancelled uses
    /// error.execution.order".
    fn request_terminal_status(
        &mut self,
        planned: &PlanNode,
        id: &InvocationId,
        unit: &str,
        status: &str,
    ) -> ResultRecord {
        // The invocation record of the named unit, when it has one. A unit that
        // has not run yet has no lifecycle to transition.
        let target = self
            .records
            .iter()
            .find(|(_, record)| record.declaration.as_deref() == Some(unit))
            .map(|(key, record)| {
                (
                    key.clone(),
                    record
                        .lifecycle
                        .permits(self.contracts.diagnostics(), status),
                )
            });
        let Some((target_id, permitted)) = target else {
            let fault = Fault::new(
                self.contracts,
                RuntimeError::ExecutionOrder,
                planned.span,
                "lifecycle transition",
                format!("{unit} has no active invocation to move to {status}"),
            );
            self.fault(&fault, planned, id, FailurePhase::PreEffect);
            return self.pre_effect_failure("result.operation", RuntimeError::ExecutionOrder);
        };
        if !permitted {
            let fault = Fault::new(
                self.contracts,
                RuntimeError::ExecutionOrder,
                planned.span,
                "lifecycle transition",
                format!("{unit} is in a state that does not permit {status}"),
            );
            self.fault(&fault, planned, id, FailurePhase::PreEffect);
            return self.pre_effect_failure("result.operation", RuntimeError::ExecutionOrder);
        }
        self.set_status(&target_id, status);
        let mut record = ResultRecord::new("result.operation", "status.succeeded")
            .with_field("changed", Value::Boolean(true));
        // `result.operation` requires its target exactly once.
        record
            .fields
            .insert("target".to_string(), Value::Reference(unit.to_string()));
        // The transition is a change to internal execution-unit state, which is
        // exactly the `state` effect class.
        record.observed_effects.push(ObservedEffect {
            class: crate::result::EffectClass::State,
            state: crate::result::RecordState::Applied,
            target: Some(unit.to_string()),
            evidence: Vec::new(),
        });
        record.effect_state = EffectState::Applied;
        record
    }

    /// Ask the operation surface to resolve one request.
    ///
    /// Borrowing is why this is its own method: the dispatcher needs mutable
    /// access to the bindings while the engine holds the rest of its state, and
    /// splitting the borrow here keeps that disjointness local and obvious.
    fn dispatch(
        &mut self,
        request: &CapabilityRequest,
        planned: &PlanNode,
        iteration: &IterationPath,
    ) -> Resolution {
        let mut cx = Invocation {
            contracts: self.contracts,
            resolved: self.resolved,
            checked: self.checked,
            plan: self.plan,
            bindings: &mut self.bindings,
            source: planned.source.clone(),
            iteration: iteration.clone(),
            span: planned.span,
            declaration: planned.declaration,
        };
        self.operations.invoke(&mut cx, request)
    }

    /// Abandon everything reachable only *after* one failed invocation.
    ///
    /// Walks the enclosing containers and abandons each later sibling in every
    /// **sequential** group. A `mode.parallel` group contributes nothing,
    /// because it declares no order between its children to be "after".
    fn halt_successors_of(&mut self, id: &InvocationId) {
        let mut child = id.node;
        loop {
            let Some(parent_index) = self.plan.node(child).and_then(|n| n.parent) else {
                return;
            };
            let Some(parent) = self.plan.node(parent_index) else {
                return;
            };
            if parent.mode == lcl_semantics::Mode::Sequential {
                if let Some(position) = parent.children.iter().position(|c| *c == child) {
                    for later in &parent.children[position + 1..] {
                        self.abandoned.insert(*later);
                    }
                }
            }
            child = parent_index;
        }
    }

    /// Finalize one aggregate to its terminal status, after resolution.
    fn finalize(&mut self, id: &InvocationId, status: &str) {
        self.set_status(id, status)
    }

    /// Move one recorded invocation to a status, recording it on its result.
    fn set_status(&mut self, id: &InvocationId, status: &str) {
        if let Some(record) = self.records.get_mut(id) {
            let _ = record
                .lifecycle
                .transition(self.contracts.diagnostics(), status);
            if let Some(result) = record.result.as_mut() {
                result.status = status.to_string();
            }
        }
    }

    /// The declared named parameters, demanded in declaration order.
    ///
    /// `operations_v0.1.0.json#/parameter_binding` is `named_only`, so each
    /// `PARAMETER` block supplies exactly one `NAME` and one `VALUE`.
    fn parameters(
        &mut self,
        block: &DeclBlock<'_>,
        planned: &PlanNode,
        iteration: &IterationPath,
    ) -> Result<BTreeMap<String, Value>, Fault> {
        let mut out = BTreeMap::new();
        let fields: Vec<_> = block
            .fields("PARAMETER")
            .into_iter()
            .filter_map(|field| field.body.as_nested())
            .map(|nested| nested.statements.clone())
            .collect();
        for statements in fields {
            let mut name = None;
            let mut value_expr = None;
            let mut value_object = None;
            for statement in &statements {
                if let lcl_parser::syntax::Statement::Field(field) = statement {
                    match field.key.text.as_str() {
                        "NAME" => {
                            name = field
                                .body
                                .as_inline()
                                .and_then(|v| v.as_expression())
                                .map(syntax::render)
                                .map(|t| t.trim_matches('"').to_string());
                        }
                        "VALUE" => {
                            value_expr = field
                                .body
                                .as_inline()
                                .and_then(|v| v.as_expression())
                                .cloned();
                            // `03_TYPES_AND_VALUES/10`, OBJECT: "An object uses
                            // an indented VALUE block containing unique
                            // lowercase property names." A parameter whose
                            // registered type is OBJECT is written that way, so
                            // a reader that saw only the inline form skipped
                            // the parameter entirely and the operation received
                            // nothing.
                            value_object = field.body.as_nested().cloned();
                        }
                        _ => {}
                    }
                }
            }
            let value = match (&value_expr, &value_object) {
                (Some(expr), _) => Some(self.evaluator(&planned.source, iteration).demand(expr)?),
                (None, Some(nested)) => {
                    Some(self.object_value(nested, &planned.source, iteration)?)
                }
                (None, None) => None,
            };
            if let (Some(name), Some(value)) = (name, value) {
                out.insert(name, value);
            }
        }
        Ok(out)
    }

    /// The object one indented `VALUE` body declares, demanded property by
    /// property.
    ///
    /// "Property order has no semantic effect", so the result is keyed rather
    /// than ordered. A statement that is not a property is not object data:
    /// the grammar stage has already emitted `error.field.duplicate` for a
    /// repeated property and `error.block.field` for an uppercase key inside
    /// object data, so nothing here judges the shape a second time; it reports
    /// no object rather than guessing what a surviving oddity meant.
    fn object_value(
        &mut self,
        nested: &lcl_parser::syntax::Nested,
        source: &SourceId,
        iteration: &IterationPath,
    ) -> Result<Value, Fault> {
        use lcl_parser::syntax::Statement;
        let mut fields = BTreeMap::new();
        for statement in &nested.statements {
            let Statement::Property(property) = statement else {
                continue;
            };
            let value = match (&property.body.as_inline(), property.body.as_nested()) {
                (Some(inline), _) => match inline.as_expression() {
                    Some(expr) => self.evaluator(source, iteration).demand(expr)?,
                    None => continue,
                },
                (None, Some(inner)) => self.object_value(inner, source, iteration)?,
                (None, None) => continue,
            };
            fields.insert(property.key.text.clone(), value);
        }
        Ok(Value::Object(fields))
    }

    /// Turn a boundary outcome into a producer result record.
    pub(crate) fn record_of(
        &mut self,
        schema: &str,
        outcome: Result<CapabilityOutcome, Refusal>,
        planned: &PlanNode,
        id: &InvocationId,
    ) -> (ResultRecord, Option<usize>) {
        match outcome {
            Ok(CapabilityOutcome::Completed(observation)) => {
                let mut record = ResultRecord::new(schema, "status.succeeded");
                record.fields = observation.fields;
                record.observed_effects = observation.effects.clone();
                record.effect_state = if observation.effects.is_empty() {
                    EffectState::None
                } else {
                    EffectState::Applied
                };
                (record, None)
            }
            // The operation's own registered contract refused, naming an
            // identifier its row lists. No effect occurred: a contract this
            // side of the boundary is decided before the world changes, or
            // over a representation already read without effect.
            Ok(CapabilityOutcome::Refused {
                error,
                cause,
                detail,
            }) => {
                let fault = Fault::new(self.contracts, error, planned.span, cause, detail);
                let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                let mut record = ResultRecord::new(schema, "status.failed");
                record.execution_errors = vec![error.as_registry_str().to_string()];
                record.effect_state = EffectState::None;
                (record, occurrence)
            }
            Ok(CapabilityOutcome::Failed {
                detail,
                observation,
            }) => {
                let phase = phase_of(&observation.effects, observation.proven_effect_free);
                let (error, cause) = if observation.host_limited {
                    (RuntimeError::HostConstraint, "host")
                } else {
                    (RuntimeError::ExecutionAction, "action")
                };
                let fault = Fault::new(self.contracts, error, planned.span, cause, detail);
                let occurrence = self.fault(&fault, planned, id, phase);
                let mut record =
                    ResultRecord::new(schema, &self.contracts.error(error).default_status);
                record.execution_errors = vec![error.as_registry_str().to_string()];
                record.failure_phase = phase;
                record.effect_state = effect_state_of(&observation.effects, phase);
                record.observed_effects = observation.effects;
                record.fields = observation.fields;
                record.output_binding = OutputBinding::Unbound;
                (record, occurrence)
            }
            // "Required access or an effect is unauthorized or prohibited."
            // A host may refuse at either gate: when it is asked for
            // permission, or when it discovers the refusal while invoking.
            Err(Refusal::Unauthorized(detail))
            | Err(Refusal::Denied(detail))
            | Ok(CapabilityOutcome::Denied(detail)) => {
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::PermissionDenied,
                    planned.span,
                    "permission",
                    detail,
                );
                let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                (
                    self.pre_effect_failure(schema, RuntimeError::PermissionDenied),
                    occurrence,
                )
            }
            // "Host limitations produce error.host.constraint and never change
            // LCL meaning."
            Err(Refusal::Unavailable(detail)) | Ok(CapabilityOutcome::Unavailable(detail)) => {
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::HostConstraint,
                    planned.span,
                    "host",
                    detail,
                );
                let occurrence = self.fault(&fault, planned, id, FailurePhase::PreEffect);
                (
                    self.pre_effect_failure(schema, RuntimeError::HostConstraint),
                    occurrence,
                )
            }
        }
    }

    /// A result for a producer that failed before any effect began.
    ///
    /// The status is the *resolved* one: an identifier the closed demand map
    /// lists takes its demand-resolved status here, because every failure this
    /// runtime raises is by construction a post-preflight demand.
    fn pre_effect_failure(&self, schema: &str, error: RuntimeError) -> ResultRecord {
        let status = if self.contracts.demand().is_eligible(error) {
            self.contracts.demand().status_for(error).to_string()
        } else {
            self.contracts.error(error).default_status.clone()
        };
        let mut record = ResultRecord::new(schema, status);
        record.execution_errors = vec![error.as_registry_str().to_string()];
        record.failure_phase = FailurePhase::PreEffect;
        record.effect_state = EffectState::None;
        record.output_binding = OutputBinding::Unbound;
        record
    }

    /// Project the producer result onto the selected `OUTPUT` and bind it.
    fn bind_output(
        &mut self,
        output: &lcl_parser::syntax::Expr,
        record: &mut ResultRecord,
        planned: &PlanNode,
        id: &InvocationId,
        iteration: &IterationPath,
    ) {
        let Some(target) = output_reference(output) else {
            return;
        };
        let Some(declaration) = self
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == target)
        else {
            return;
        };
        let Some(block) = syntax::declaration_block(self.resolved, declaration) else {
            return;
        };
        let Some(schema) = self.contracts.schema(&record.schema) else {
            return;
        };

        // "zero PROPERTY occurrences select the result schema's registered
        // default_property. One PROPERTY selects the named schema-local field
        // as a scalar. Two or more PROPERTY occurrences select a closed OBJECT
        // containing exactly those unique top-level fields in declaration
        // order."
        let selected: Vec<String> = block
            .fields("PROPERTY")
            .into_iter()
            .filter_map(|field| field.body.as_inline())
            .filter_map(|value| value.as_expression())
            .map(syntax::render)
            .map(|text| text.trim_matches('"').to_string())
            .collect();

        let names: Vec<String> = if selected.is_empty() {
            schema.default_property.clone().into_iter().collect()
        } else {
            selected
        };
        if names.is_empty() {
            record.output_binding = OutputBinding::Unbound;
            return;
        }
        // "Every selected name must occur in the schema's projectable_fields
        // list; common bookkeeping fields cannot be projected."
        for name in &names {
            if !schema.projectable_fields.contains(name) {
                let fault = Fault::new(
                    self.contracts,
                    RuntimeError::OperationPostcondition,
                    planned.span,
                    "output projection",
                    format!("{name} is not a projectable field of {}", schema.id),
                );
                self.fault(&fault, planned, id, record.failure_phase);
                record.output_binding = OutputBinding::Unbound;
                return;
            }
        }

        let projected = if names.len() == 1 {
            record.field(&names[0]).cloned()
        } else {
            let mut fields = BTreeMap::new();
            for name in &names {
                match record.field(name) {
                    Some(value) => {
                        fields.insert(name.clone(), value.clone());
                    }
                    None => return,
                }
            }
            Some(Value::Object(fields))
        };

        match projected {
            // "MISSING and UNKNOWN are non-material and never bind or partially
            // bind OUTPUT."
            Some(value) if value.is_material() => {
                self.bindings.bind_output(target, iteration, value);
                record.output_binding = OutputBinding::Bound;
            }
            // "An absent conditional field makes that projection unavailable."
            _ => record.output_binding = OutputBinding::Unbound,
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    pub(crate) fn evaluator(&self, source: &SourceId, iteration: &IterationPath) -> Evaluator<'_> {
        Evaluator {
            contracts: self.contracts,
            resolved: self.resolved,
            checked: self.checked,
            plan: self.plan,
            bindings: &self.bindings,
            source: source.clone(),
            iteration: iteration.clone(),
        }
    }

    fn index_of(&self, planned: &PlanNode) -> usize {
        self.plan
            .nodes()
            .iter()
            .position(|node| std::ptr::eq(node, planned))
            .unwrap_or_else(|| {
                self.plan
                    .nodes()
                    .iter()
                    .position(|node| node.candidate == planned.candidate)
                    .unwrap_or(0)
            })
    }

    fn record(
        &mut self,
        id: InvocationId,
        planned: &PlanNode,
        lifecycle: Lifecycle,
        result: Option<ResultRecord>,
    ) {
        let node = self.index_of(planned);
        self.records.insert(
            id.clone(),
            InvocationRecord {
                id,
                node,
                declaration: planned.id.clone(),
                block: planned.block.clone(),
                lifecycle,
                result,
            },
        );
    }

    /// Emit one fault as a registered diagnostic at its producer.
    pub(crate) fn fault(
        &mut self,
        fault: &Fault,
        planned: &PlanNode,
        id: &InvocationId,
        phase: FailurePhase,
    ) -> Option<usize> {
        let path = self.plan.order().iter().position(|n| *n == id.node);
        let occurrence = self.emit(
            fault.id,
            &planned.source,
            fault.span,
            fault.cause.clone(),
            fault.detail.clone(),
            Some((id.clone(), path)),
            phase,
        );
        // The demand map resolves the stage for an eligible identifier.
        if fault.demand_resolved {
            if let Some(last) = self.raw.last_mut() {
                last.resolved_stage = Some(self.contracts.demand().resolved_stage);
                last.default_status = self.contracts.demand().status_for(fault.id).to_string();
            }
        }
        occurrence
    }

    /// Emit one registered diagnostic.
    ///
    /// Metadata comes from the registry, never from the call site, so a
    /// diagnostic cannot drift from its canonical classification.
    #[allow(clippy::too_many_arguments)]
    #[must_use = "the raised event occurrence selects a handler"]
    pub(crate) fn emit(
        &mut self,
        id: RuntimeError,
        source: &SourceId,
        span: Span,
        cause: impl Into<String>,
        detail: impl Into<String>,
        producer: Option<(InvocationId, Option<usize>)>,
        phase: FailurePhase,
    ) -> Option<usize> {
        let registered = self.contracts.error(id);
        let text = self
            .resolved
            .unit(source)
            .map(|unit| unit.source())
            .unwrap_or("");
        let position = position_of(text, span.start);
        let (producer, producer_path) = match producer {
            Some((id, path)) => (Some(id), path),
            None => (None, None),
        };
        let diagnostic = Diagnostic {
            sequence: self.raw.len(),
            id,
            registered_stage: registered.stage,
            resolved_stage: None,
            source: source.clone(),
            span,
            position,
            meaning: registered.meaning.clone(),
            default_status: registered.default_status.clone(),
            specificity_rank: registered.specificity_rank,
            event: registered.event.clone(),
            cause: Cause::new(cause),
            producer,
            producer_path,
            failure_phase: phase,
            detail: Some(detail.into()),
        };
        // The emission identity, which survives selection.
        let sequence = diagnostic.sequence;
        let event = diagnostic.event.clone();
        let owner = diagnostic
            .producer
            .clone()
            .unwrap_or_else(|| InvocationId::first(0, IterationPath::root()));
        self.raw.push(diagnostic);
        // "Emission of a diagnostic is the only producer of an event", unless
        // `non_reentrancy_rule` suppresses it: "Those diagnostics raise no
        // event."
        if self.in_handler > 0 {
            return None;
        }
        self.events.raise(event.as_deref(), sequence, owner)
    }
}

/// Whether a declaration is applicable at its demand point.
enum Applicability {
    Applicable,
    Skipped,
    Faulted(Fault),
}

/// The `OUTPUT` declaration one `ACTION.OUTPUT` field selects.
fn output_reference(expr: &lcl_parser::syntax::Expr) -> Option<&str> {
    match expr {
        lcl_parser::syntax::Expr::Call(call) if call.is_reference() => {
            call.reference_target().map(|ident| ident.text.as_str())
        }
        _ => None,
    }
}

/// The failure phase implied by a host's observations.
///
/// "Absence of evidence never proves absence of effects": a host that cannot
/// prove it caused no effect yields `indeterminate`, not `pre_effect`.
fn phase_of(effects: &[ObservedEffect], proven_effect_free: bool) -> FailurePhase {
    if !effects.is_empty() {
        return FailurePhase::PostEffect;
    }
    if proven_effect_free {
        FailurePhase::PreEffect
    } else {
        FailurePhase::Indeterminate
    }
}

fn effect_state_of(effects: &[ObservedEffect], phase: FailurePhase) -> EffectState {
    if effects.is_empty() {
        return match phase {
            FailurePhase::Indeterminate => EffectState::Indeterminate,
            _ => EffectState::None,
        };
    }
    if effects
        .iter()
        .any(|e| e.state == crate::result::RecordState::Indeterminate)
    {
        return EffectState::Indeterminate;
    }
    if effects
        .iter()
        .any(|e| e.state == crate::result::RecordState::Partial)
    {
        return EffectState::Partial;
    }
    EffectState::Applied
}

/// A derived line/column for one byte offset. Presentation only; the byte
/// offset stays authoritative.
/// Derive a line and column from an exact byte offset.
///
/// Presentation only. `5.2 Source identity and spans`: "Source byte offsets
/// remain authoritative. Derived line/column locations must not replace exact
/// byte spans." Exported so the completion layer above derives the *same*
/// position for the same offset instead of carrying a second implementation.
pub fn position_of(text: &str, offset: usize) -> lcl_lexer::Position {
    let mut line = 1u32;
    let mut column = 1u32;
    for (index, ch) in text.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    lcl_lexer::Position {
        offset,
        line,
        column,
    }
}

/// The first effect class a host reported outside the invocation's resolved set.
///
/// `CapabilityRequest::possible_effects` is documented as "The invocation's
/// resolved possible-effect set. A host may not report an effect outside it."
/// This is where that stops being documentation.
fn reported_outside(
    outcome: &Result<CapabilityOutcome, Refusal>,
    request: &CapabilityRequest,
) -> Option<String> {
    let observation = match outcome {
        Ok(CapabilityOutcome::Completed(observation)) => observation,
        Ok(CapabilityOutcome::Failed { observation, .. }) => observation,
        _ => return None,
    };
    observation
        .effects
        .iter()
        .map(|effect| effect.class.as_registry_str().to_string())
        .find(|class| !request.possible_effects.contains(class))
}

/// The terminal status one control operation requests, when it requests one.
fn internal_terminal_status(operation: &str) -> Option<&'static str> {
    match operation {
        "core.cancel" => Some("status.cancelled"),
        "core.stop" => Some("status.stopped"),
        _ => None,
    }
}
