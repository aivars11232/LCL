//! Step 9: finalize and check the ordering edges of the resolved candidate
//! graph, before effects.
//!
//! Authority: `block_schemas_v0.1.0.json#/execution_graph_contract` and
//! `05_SEMANTICS/01_DECLARATION_RESOLUTION_REACHABILITY_AND_EXECUTION_GRAPH.txt`.
//!
//! > ORDERING
//! > TASK and STEP child groups are sequential. PHASE and SEQUENCE use declared
//! > MODE, defaulting to mode.sequential. Sequential lexical order contributes
//! > required predecessor edges. BEFORE and AFTER add edges and never reverse a
//! > sequential edge. Endpoints must be distinct sibling execution units in the
//! > same container and iteration context; other endpoints and ordering cycles
//! > use error.execution.order.
//!
//! ## Membership is not this step's to change
//!
//! Step 9 says "Finalize and check ordering edges **of that resolved candidate
//! graph**", and adds the rule this module is built around:
//!
//! > no check reference or value read adds graph membership or edges.
//!
//! So [`finalize`] copies M3's candidate graph node-for-node and adds only
//! *edges*, and only from the three sources the contract names: sequential
//! lexical order, `BEFORE` and `AFTER`. Nothing a `VALIDATE` targets, nothing a
//! `SUCCESS` references, and nothing an `OUTPUT` read names can put a node in
//! this graph or an edge between two of them. `preflight_matrix` proves that by
//! planning the same document with and without those references and comparing
//! the node and edge sets.
//!
//! ## Why ordering failures are pre-effect
//!
//! `error.execution.order` is registered at `stage: execution`, and decision
//! witness `CLOSURE-063` settles what that means here: "Graph construction
//! yields error.execution.order while the invocation is ready. Transition to
//! status.failed is permitted before effects; failure_phase remains
//! producer-relative pre_effect." A registered stage is a classification, not a
//! schedule.

use crate::authority::RuleKind;
use crate::diagnostic::PreflightError;
use crate::engine::Engine;
use crate::plan::{Authorization, Edge, EdgeReason, Mode, PlanNode};
use crate::syntax;
use lcl_lexer::Span;
use lcl_resolver::{NodeKind, SourceId};
use std::collections::{BTreeMap, BTreeSet};

/// Build the plan's nodes, edges and order, and check them.
pub(crate) fn finalize(engine: &mut Engine) {
    build_nodes(engine);
    check_activation_identity(engine);
    check_output_ownership(engine);
    add_sequential_edges(engine);
    add_declared_edges(engine);
    check_parallel_independence(engine);
    graph_reference_cycles(engine);
    authorize_actions(engine);
    order_topologically(engine);
}

// ---------------------------------------------------------------------------
// Prohibited graph-target reference cycles
// ---------------------------------------------------------------------------

/// The rows whose TARGET names an execution unit they delegate to.
///
/// `core.execute` and `core.test` execute it as a graph;
/// `operations_v0.1.0.json#/contracts/core.retry` "resolves the wrapped ACTION
/// first" and `06_STANDARD_LIBRARY/10` states that "a prohibited wrapped-ACTION
/// reference cycle uses error.reference.cycle".
const GRAPH_ROWS: [&str; 3] = ["core.execute", "core.test", "core.retry"];

/// Refuse a reachable `core.execute`, `core.test` or `core.retry` whose target
/// leads back to itself.
///
/// `05_SEMANTICS/11`: "For a referenced TASK, PHASE, SEQUENCE, ACTION, or TEST,
/// a prohibited reference cycle emits error.reference.cycle and fails **before
/// axis resolution**", which `operations_v0.1.0.json#/axis_contract/
/// implementation_profile/graph_resolution` repeats for the graph mode of both
/// rows. Deciding it here, in step 9, is what "before axis resolution" means:
/// the invocation never reaches the point where a graph's transitive
/// dependency and effect unions would be formed.
///
/// The resolver already refuses a *structural* cycle while it expands the
/// candidate graph. A graph target is not a structural child — "no check
/// reference or value read adds graph membership or edges" — so the reference
/// edge is followed here instead, over the declarations the two rows name.
fn graph_reference_cycles(engine: &mut Engine) {
    let mut targets: BTreeMap<usize, usize> = BTreeMap::new();
    let mut sites: Vec<(usize, SourceId, Span, String)> = Vec::new();
    for node in &engine.plan.nodes {
        let (Some(declaration), Some(block)) = (
            node.declaration,
            node.declaration
                .and_then(|d| syntax::declaration_block(engine.resolved, d)),
        ) else {
            continue;
        };
        let Some(operation) = syntax::field_text(block, "OPERATION") else {
            continue;
        };
        if !GRAPH_ROWS.contains(&operation.as_str()) {
            continue;
        }
        let Some(target) = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(syntax::reference_target)
        else {
            continue;
        };
        let Some(referenced) = engine
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == target)
        else {
            continue;
        };
        targets.insert(declaration, referenced);
        sites.push((
            declaration,
            node.source.clone(),
            node.span,
            node.id.clone().unwrap_or_default(),
        ));
    }

    let mut cycles = Vec::new();
    for (declaration, source, span, id) in sites {
        let mut seen = BTreeSet::new();
        let mut at = declaration;
        while let Some(next) = targets.get(&at).copied() {
            if !seen.insert(at) {
                break;
            }
            if next == declaration {
                cycles.push((
                    source.clone(),
                    span,
                    format!("graph-cycle:{id}"),
                    format!(
                        "`{id}` delegates to a unit that leads back to `{id}`, which is a prohibited reference cycle"
                    ),
                ));
                break;
            }
            at = next;
        }
    }
    for (source, span, cause, detail) in cycles {
        engine.emit(PreflightError::ReferenceCycle, &source, span, cause, detail);
    }
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

/// Copy M3's candidate graph, adding each node's resolved mode and required
/// contract. One plan node per candidate node, in the same order.
fn build_nodes(engine: &mut Engine) {
    let mut nodes = Vec::new();
    for (index, member) in engine.resolved.graph().nodes().iter().enumerate() {
        let declaration = member.declaration;
        let id = declaration
            .and_then(|d| engine.resolved.declarations().get(d))
            .map(|d| d.id.qualified());

        let block = declaration.and_then(|d| syntax::declaration_block(engine.resolved, d));

        // "TASK and STEP child groups are sequential. PHASE and SEQUENCE use
        // declared MODE, defaulting to mode.sequential."
        let mode = match member.block.as_str() {
            "PHASE" | "SEQUENCE" => block
                .and_then(|b| syntax::field_text(b, "MODE"))
                .and_then(|mode| match mode.as_str() {
                    "mode.parallel" => Some(Mode::Parallel),
                    "mode.sequential" => Some(Mode::Sequential),
                    _ => None,
                })
                .unwrap_or(Mode::Sequential),
            _ => Mode::Sequential,
        };

        let required = block
            .and_then(|b| b.field("REQUIRED"))
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| {
                crate::eval::literal_value(engine, &member.source, expr).and_then(|v| v.boolean())
            })
            .unwrap_or(true);

        nodes.push(PlanNode {
            candidate: index,
            kind: member.kind,
            source: member.source.clone(),
            block: member.block.clone(),
            span: member.span,
            declaration,
            id,
            parent: member.parent,
            children: member.children.clone(),
            mode,
            required,
            authorization: None,
        });
    }
    engine.plan.nodes = nodes;
}

// ---------------------------------------------------------------------------
// Activation identity
// ---------------------------------------------------------------------------

/// "An execution-unit source declaration has one structural activation path per
/// candidate invocation. Multiple structural paths to the same declaration use
/// error.execution.order before effects, including mutually exclusive
/// branches."
///
/// Decision witness `CLOSURE-054`: "Two distinct STEP activation paths select
/// the same ACTION source declaration → error.execution.order before effects;
/// bounded loop/retry template replication remains allowed."
///
/// What identifies one structural activation: the declaration it activates, the
/// enclosing loop templates, and the enclosing delegating invocations. Two
/// activations are duplicates only when all three agree.
type ActivationKey = (usize, Vec<usize>, Vec<usize>);

/// Where an activation was first seen: its source, its span and its plan node.
type ActivationSite = (SourceId, Span, usize);

/// Loop templates are the explicit exception: "Explicit bounded FOR EACH
/// instances and RETRY attempts replicate their source template with distinct
/// invocation identities and are not duplicate activation." A node inside a
/// loop template is one source template, not two activations, so only nodes
/// whose enclosing loop context is identical are compared.
fn check_activation_identity(engine: &mut Engine) {
    let mut seen: BTreeMap<ActivationKey, ActivationSite> = BTreeMap::new();
    let mut duplicates = Vec::new();

    for (index, node) in engine.plan.nodes.iter().enumerate() {
        let Some(declaration) = node.declaration else {
            continue;
        };
        if node.kind != NodeKind::ExecutionUnit {
            continue;
        }
        // "one structural activation path **per candidate invocation**": an
        // explicit graph-valued invocation "create[s] [its] own child
        // invocation", so a unit reached through one is in a different
        // candidate invocation from the same unit activated structurally, and
        // the two are not duplicate activation of one path.
        let key = (
            declaration,
            loop_context(engine, index),
            delegation_context(engine, index),
        );
        match seen.get(&key) {
            Some((source, span, first)) => duplicates.push((
                source.clone(),
                *span,
                node.id.clone().unwrap_or_default(),
                *first,
                index,
            )),
            None => {
                seen.insert(key, (node.source.clone(), node.span, index));
            }
        }
    }

    for (source, span, id, first, second) in duplicates {
        engine.emit_at_node(
            PreflightError::ExecutionOrder,
            second,
            &source,
            span,
            format!("activation|{id}"),
            format!(
                "`{id}` is activated by two distinct structural paths (plan nodes {first} and {second}) in one candidate invocation"
            ),
        );
    }
}

/// The chain of enclosing delegating invocations, outermost first.
///
/// An `ACTION` acquires a child only by being an explicit graph-valued
/// operation invocation — `05_SEMANTICS/01`: "Explicit graph-valued operation
/// invocations create their own child invocation under the same rules" — so its
/// ancestors of that block are exactly the delegations this node was reached
/// through, and they name the candidate invocation it belongs to.
fn delegation_context(engine: &Engine, node: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut current = engine.plan.nodes.get(node).and_then(|n| n.parent);
    while let Some(index) = current {
        let Some(parent) = engine.plan.nodes.get(index) else {
            break;
        };
        if parent.block == "ACTION" {
            out.push(index);
        }
        current = parent.parent;
    }
    out.reverse();
    out
}

/// The chain of enclosing loop-template nodes, outermost first.
///
/// "Instance identity includes the full enclosing iteration-index path." Two
/// activations under the same loop template are one template, replicated per
/// iteration; two under different templates are different contexts.
fn loop_context(engine: &Engine, node: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut current = engine.plan.nodes.get(node).and_then(|n| n.parent);
    while let Some(index) = current {
        let Some(parent) = engine.plan.nodes.get(index) else {
            break;
        };
        if parent.kind == NodeKind::LoopTemplate || parent.kind == NodeKind::BranchTemplate {
            out.push(index);
        }
        current = parent.parent;
    }
    out.reverse();
    out
}

// ---------------------------------------------------------------------------
// Output ownership
// ---------------------------------------------------------------------------

/// "Each selected OUTPUT has exactly one producing ACTION source declaration in
/// the candidate invocation. Two distinct ACTION owners use
/// error.execution.order before effects, including mutually exclusive
/// branches."
///
/// Decision witness `CLOSURE-056`.
fn check_output_ownership(engine: &mut Engine) {
    let mut owners: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut conflicts = Vec::new();

    for (index, node) in engine.plan.nodes.iter().enumerate() {
        if node.block != "ACTION" {
            continue;
        }
        let Some(declaration) = node.declaration else {
            continue;
        };
        let Some(block) = syntax::declaration_block(engine.resolved, declaration) else {
            continue;
        };
        let Some(field) = block.field("OUTPUT") else {
            continue;
        };
        for (output, _) in syntax::reference_list(&field.body) {
            let action = node.id.clone().unwrap_or_default();
            match owners.get(&output) {
                Some((first, first_action)) if first_action != &action => {
                    conflicts.push((
                        node.source.clone(),
                        node.span,
                        output.clone(),
                        first_action.clone(),
                        action.clone(),
                        *first,
                        index,
                    ));
                }
                Some(_) => {}
                None => {
                    owners.insert(output, (index, action));
                }
            }
        }
    }

    for (source, span, output, first, second, _first_node, node) in conflicts {
        engine.emit_at_node(
            PreflightError::ExecutionOrder,
            node,
            &source,
            span,
            format!("output_owner|{output}"),
            format!(
                "OUTPUT `{output}` is produced by two ACTION declarations, `{first}` and `{second}`; each selected OUTPUT has exactly one producing ACTION per candidate invocation"
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Sequential edges
// ---------------------------------------------------------------------------

/// "Sequential lexical order contributes required predecessor edges."
///
/// "mode.parallel omits implicit sequential edges but retains BEFORE and AFTER
/// edges", so a parallel container contributes none.
fn add_sequential_edges(engine: &mut Engine) {
    let mut edges = Vec::new();
    for node in &engine.plan.nodes {
        if node.mode == Mode::Parallel {
            continue;
        }
        let children = orderable_children(engine, node);
        for pair in children.windows(2) {
            edges.push(Edge {
                from: pair[0],
                to: pair[1],
                reason: EdgeReason::Sequential,
            });
        }
    }
    engine.plan.edges = edges;
}

/// A container's children that take part in ordering, in canonical child order.
///
/// Branch and loop templates are structural containers rather than sibling
/// execution units, so their own children are the orderable ones.
fn orderable_children(engine: &Engine, node: &PlanNode) -> Vec<usize> {
    node.children
        .iter()
        .filter(|child| {
            engine
                .plan
                .nodes
                .get(**child)
                .map(|c| c.kind == NodeKind::ExecutionUnit)
                .unwrap_or(false)
        })
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------
// Declared BEFORE and AFTER edges
// ---------------------------------------------------------------------------

/// "BEFORE and AFTER add edges and never reverse a sequential edge. Endpoints
/// must be distinct sibling execution units in the same container and iteration
/// context; other endpoints and ordering cycles use error.execution.order."
///
/// Decision witness `CLOSURE-055`: "A sequential second sibling requests BEFORE
/// the first sibling → error.execution.order; explicit ordering may not reverse
/// the required sequential edge."
fn add_declared_edges(engine: &mut Engine) {
    struct Declared {
        node: usize,
        target: String,
        field: &'static str,
        source: SourceId,
        span: Span,
    }

    let mut declared = Vec::new();
    for (index, node) in engine.plan.nodes.iter().enumerate() {
        let Some(declaration) = node.declaration else {
            continue;
        };
        let Some(block) = syntax::declaration_block(engine.resolved, declaration) else {
            continue;
        };
        for field in ["BEFORE", "AFTER"] {
            let Some(body) = block.field(field) else {
                continue;
            };
            for (target, span) in syntax::reference_list(&body.body) {
                declared.push(Declared {
                    node: index,
                    target,
                    field: if field == "BEFORE" { "BEFORE" } else { "AFTER" },
                    source: node.source.clone(),
                    span,
                });
            }
        }
    }

    let sequential: BTreeSet<(usize, usize)> = engine
        .plan
        .edges
        .iter()
        .filter(|e| e.reason == EdgeReason::Sequential)
        .map(|e| (e.from, e.to))
        .collect();

    let mut additions = Vec::new();
    let mut failures = Vec::new();

    for entry in declared {
        let Some(target) = engine
            .plan
            .nodes
            .iter()
            .position(|n| n.id.as_deref() == Some(entry.target.as_str()))
        else {
            failures.push((
                entry.source.clone(),
                entry.span,
                format!("endpoint|{}|{}", entry.field, entry.target),
                entry.node,
                format!(
                    "{} names `{}`, which is not an execution unit of this candidate invocation",
                    entry.field, entry.target
                ),
            ));
            continue;
        };

        // "Endpoints must be distinct sibling execution units in the same
        // container and iteration context."
        let self_node = &engine.plan.nodes[entry.node];
        let target_node = &engine.plan.nodes[target];
        if target == entry.node {
            failures.push((
                entry.source.clone(),
                entry.span,
                format!("endpoint|{}|{}", entry.field, entry.target),
                entry.node,
                format!(
                    "{} names its own declaration; endpoints must be distinct",
                    entry.field
                ),
            ));
            continue;
        }
        if self_node.parent != target_node.parent {
            failures.push((
                entry.source.clone(),
                entry.span,
                format!("endpoint|{}|{}", entry.field, entry.target),
                entry.node,
                format!(
                    "{} names `{}`, which is not a sibling in the same container and iteration context",
                    entry.field, entry.target
                ),
            ));
            continue;
        }

        // `BEFORE` on X naming Y means X precedes Y; `AFTER` on X naming Y
        // means Y precedes X.
        let (from, to) = match entry.field {
            "BEFORE" => (entry.node, target),
            _ => (target, entry.node),
        };

        // "never reverse a sequential edge": the required sequential edge runs
        // the other way.
        if sequential.contains(&(to, from)) {
            failures.push((
                entry.source.clone(),
                entry.span,
                format!("reversal|{}|{}", entry.field, entry.target),
                entry.node,
                format!(
                    "{} names `{}`, which would reverse the required sequential edge between siblings",
                    entry.field, entry.target
                ),
            ));
            continue;
        }

        additions.push(Edge {
            from,
            to,
            reason: if entry.field == "BEFORE" {
                EdgeReason::Before
            } else {
                EdgeReason::After
            },
        });
    }

    engine.plan.edges.extend(additions);
    engine.plan.edges.sort();
    engine.plan.edges.dedup();

    for (source, span, cause, node, detail) in failures {
        engine.emit_at_node(
            PreflightError::ExecutionOrder,
            node,
            &source,
            span,
            cause,
            detail,
        );
    }
}

// ---------------------------------------------------------------------------
// Parallel independence
// ---------------------------------------------------------------------------

/// "Every included pair must have proven independence: no conflicting writes,
/// read/write dependency, shared mutable binding, or conflicting output
/// destination. Disjoint output paths alone are insufficient."
///
/// The word is *proven*. The decidable proof obligation before effects is the
/// one the contract names last: two parallel siblings must not write the same
/// destination or bind the same `OUTPUT`. Anything this layer cannot prove
/// independent it does not silently declare independent — but neither does it
/// invent a conflict, so only demonstrated overlaps are reported.
fn check_parallel_independence(engine: &mut Engine) {
    let mut conflicts = Vec::new();

    for node in &engine.plan.nodes {
        if node.mode != Mode::Parallel {
            continue;
        }
        let children = orderable_children(engine, node);
        let ordered: BTreeSet<(usize, usize)> =
            engine.plan.edges.iter().map(|e| (e.from, e.to)).collect();

        for (i, &left) in children.iter().enumerate() {
            for &right in children.iter().skip(i + 1) {
                // An explicitly ordered pair is not concurrent.
                if ordered.contains(&(left, right)) || ordered.contains(&(right, left)) {
                    continue;
                }
                let left_writes = written_destinations(engine, left);
                let right_writes = written_destinations(engine, right);
                let shared: Vec<&String> = left_writes.intersection(&right_writes).collect();
                if shared.is_empty() {
                    continue;
                }
                let names: Vec<String> = shared.into_iter().cloned().collect();
                conflicts.push((
                    engine.plan.nodes[right].source.clone(),
                    engine.plan.nodes[right].span,
                    format!("parallel|{left}|{right}"),
                    right,
                    format!(
                        "`{}` and `{}` are unordered children of a mode.parallel container and both write {}; every included pair must have proven independence",
                        engine.plan.nodes[left].id.clone().unwrap_or_default(),
                        engine.plan.nodes[right].id.clone().unwrap_or_default(),
                        names.join(", ")
                    ),
                ));
            }
        }
    }

    for (source, span, cause, node, detail) in conflicts {
        engine.emit_at_node(
            PreflightError::ExecutionOrder,
            node,
            &source,
            span,
            cause,
            detail,
        );
    }
}

/// Everything one execution unit writes: its bound `OUTPUT`s and its target
/// when its operation is a mutating one.
fn written_destinations(engine: &Engine, node: usize) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(plan_node) = engine.plan.nodes.get(node) else {
        return out;
    };
    // A container's writes include its descendants'.
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        let Some(current_node) = engine.plan.nodes.get(current) else {
            continue;
        };
        stack.extend(current_node.children.iter().copied());
        let Some(declaration) = current_node.declaration else {
            continue;
        };
        let Some(block) = syntax::declaration_block(engine.resolved, declaration) else {
            continue;
        };
        if let Some(field) = block.field("OUTPUT") {
            for (output, _) in syntax::reference_list(&field.body) {
                out.insert(output);
            }
        }
        if let Some(operation) = syntax::field_text(block, "OPERATION") {
            let mutating = engine
                .contracts
                .operation_axes(&operation)
                .map(|axes| axes.is_mutating())
                .unwrap_or(false);
            if mutating {
                if let Some(expr) = block
                    .field("TARGET")
                    .and_then(|f| syntax::inline_expr(&f.body))
                {
                    out.insert(match syntax::reference_target(expr) {
                        Some(id) => id.to_string(),
                        None => crate::eval::render_static(expr),
                    });
                }
            }
        }
    }
    let _ = plan_node;
    out
}

// ---------------------------------------------------------------------------
// Authorization
// ---------------------------------------------------------------------------

/// Record the authorization decision for every planned `ACTION`.
///
/// `05_SEMANTICS/03`: "A reachable required ACTION authorizes exactly its
/// declared OPERATION, TARGET, PARAMETER values, OUTPUT, and necessary direct
/// effects. ... FORBID blocks matching action even when an ACTION requires it
/// unless a valid OVERRIDE resolves the exact conflict."
///
/// A prohibition that wins outright leaves the action unauthorized, which is
/// `error.permission.denied`: "Required access or an effect is unauthorized or
/// prohibited." An equal-authority contradiction is `error.conflict.hard` and
/// was already reported in step 6, so it is not reported a second time here.
fn authorize_actions(engine: &mut Engine) {
    let mut decisions: Vec<(usize, Authorization)> = Vec::new();
    let mut denials = Vec::new();
    let mut violations = Vec::new();

    for (index, node) in engine.plan.nodes.iter().enumerate() {
        if node.block != "ACTION" {
            continue;
        }
        let Some(declaration) = node.declaration else {
            continue;
        };
        let Some(block) = syntax::declaration_block(engine.resolved, declaration) else {
            continue;
        };
        let Some(operation) = syntax::field_text(block, "OPERATION") else {
            continue;
        };
        let target_expr = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body));
        let target = target_expr.map(|expr| match syntax::reference_target(expr) {
            Some(id) => id.to_string(),
            None => crate::eval::render_static(expr),
        });
        // `SCOPE` on an ACTION is written as a reference, so the id comes from
        // the reference target; `field_text` reads only identifiers and
        // literals and would leave every referenced scope unrecorded.
        let scope = block
            .field("SCOPE")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| match syntax::reference_target(expr) {
                Some(id) => Some(id.to_string()),
                None => syntax::literal_text(expr),
            });

        // Step 6 resolved every SCOPE; this is where the action's own target is
        // compared with the one it names. `statuses_and_errors_v0.1.0.json`
        // registers error.scope.violation as "An action targets an entity
        // outside applicable SCOPE", and 05_SEMANTICS/09 makes it "pre_effect
        // only: ... effective scope resolves at processing step 6 before the
        // first authorized effect". An unauthorized action performs no effect,
        // so refusing here is that phase.
        //
        // The applicable scope is the one this ACTION names. An enclosing TASK
        // SCOPE is not applied to every action under it: the canonical valid
        // example 04_AUTOMATED_CODING_TASK declares TASK SCOPE scope.source,
        // which selects one source file, while `action.test` under it targets
        // PATH("/usr/bin/python3"). Widening the rule that way would reject a
        // canonical valid example.
        if let Some(named) = scope.as_deref() {
            if let Some(record) = crate::scope::applicable(&engine.scopes, named, &operation) {
                // A restriction this layer cannot decide is never treated as
                // absent: an action does not gain permission because its scope
                // is written as a pattern, or because its TARGET names no one
                // entity before effects.
                let verdict = match target_expr.and_then(crate::scope::selector_of) {
                    Some(selector) => match crate::scope::admits(record, &selector) {
                        crate::scope::Admission::Admitted => None,
                        crate::scope::Admission::Refused => Some(format!(
                            "`{}` targets {selector}, which `{named}` does not admit",
                            node.id.clone().unwrap_or_default()
                        )),
                        crate::scope::Admission::Undecided(reason) => Some(format!(
                            "`{}` targets {selector}, and `{named}` could not be resolved against \
                             it before effects: {reason}",
                            node.id.clone().unwrap_or_default()
                        )),
                    },
                    None => Some(format!(
                        "`{}` names `{named}` but its TARGET does not identify one entity that \
                         can be resolved against that scope before effects",
                        node.id.clone().unwrap_or_default()
                    )),
                };
                if let Some(detail) = verdict {
                    violations.push((
                        node.source.clone(),
                        node.span,
                        format!("scope|{}", node.id.clone().unwrap_or_default()),
                        index,
                        format!(
                            "{detail}: INCLUDE [{}] minus EXCLUDE [{}]",
                            record
                                .include
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                            record
                                .exclude
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                    ));
                    continue;
                }
            }
        }

        let matching = |kind: RuleKind| -> Vec<&crate::authority::AuthorityRecord> {
            engine
                .authorities
                .iter()
                .filter(|r| r.kind == kind && r.is_applicable())
                .filter(|r| r.operation.as_deref() == Some(operation.as_str()))
                .filter(|r| match (&r.target, &target) {
                    (Some(a), Some(b)) => a == b,
                    (None, _) => true,
                    _ => false,
                })
                .collect()
        };

        let forbids = matching(RuleKind::Forbid);
        let permits: Vec<&crate::authority::AuthorityRecord> = matching(RuleKind::Allow)
            .into_iter()
            .chain(matching(RuleKind::Require))
            .collect();

        let strongest_permit = permits.iter().map(|r| r.authority).max();
        let strongest_forbid = forbids.iter().map(|r| r.authority).max();

        if let Some(forbid_authority) = strongest_forbid {
            let defeated = forbids.iter().all(|f| {
                // A FORBID is defeated when an exact OVERRIDE named it loser,
                // which step 6 already validated, or when a permission of
                // strictly higher authority wins.
                overridden(engine, &f.id) || strongest_permit.is_some_and(|p| p > f.authority)
            });
            if !defeated {
                let blocking = forbids
                    .iter()
                    .find(|f| !overridden(engine, &f.id))
                    .map(|f| f.id.clone())
                    .unwrap_or_default();
                denials.push((
                    node.source.clone(),
                    node.span,
                    format!("permission|{}", node.id.clone().unwrap_or_default()),
                    index,
                    format!(
                        "`{}` invokes {operation}, which `{blocking}` forbids at authority {forbid_authority} with no exact OVERRIDE naming a winner and a loser",
                        node.id.clone().unwrap_or_default()
                    ),
                ));
                continue;
            }
        }

        decisions.push((
            index,
            Authorization {
                operation,
                target,
                scope,
                permitted_by: permits.iter().map(|r| r.id.clone()).collect(),
                overridden: forbids
                    .iter()
                    .filter(|f| overridden(engine, &f.id))
                    .map(|f| f.id.clone())
                    .collect(),
            },
        ));
    }

    for (index, authorization) in decisions {
        engine.plan.nodes[index].authorization = Some(authorization);
    }
    for (source, span, cause, node, detail) in violations {
        engine.emit_at_node(
            PreflightError::ScopeViolation,
            node,
            &source,
            span,
            cause,
            detail,
        );
    }
    for (source, span, cause, node, detail) in denials {
        engine.emit_at_node(
            PreflightError::PermissionDenied,
            node,
            &source,
            span,
            cause,
            detail,
        );
    }
}

/// Was this clause named the loser of an exact `OVERRIDE`?
fn overridden(engine: &Engine, id: &str) -> bool {
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "OVERRIDE" {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let loser = block
            .field("LOSER")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(syntax::reference_target);
        if loser == Some(id) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Stable order
// ---------------------------------------------------------------------------

/// One stable topological order over the plan's execution units.
///
/// Ties are broken by candidate-graph index, which is M3's canonical child
/// order. "Discovery time and parallel scheduling are never classification or
/// ordering inputs", and neither is anything else here: the same bytes always
/// produce the same sequence.
///
/// An ordering cycle uses `error.execution.order`.
fn order_topologically(engine: &mut Engine) {
    let count = engine.plan.nodes.len();
    let mut incoming: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for index in 0..count {
        incoming.insert(index, BTreeSet::new());
    }
    for edge in &engine.plan.edges {
        incoming.entry(edge.to).or_default().insert(edge.from);
    }

    let mut order = Vec::new();
    let mut placed: BTreeSet<usize> = BTreeSet::new();
    loop {
        // Lowest candidate index first: the canonical child order.
        let next = (0..count).find(|index| {
            !placed.contains(index)
                && incoming
                    .get(index)
                    .map(|from| from.iter().all(|f| placed.contains(f)))
                    .unwrap_or(true)
        });
        match next {
            Some(index) => {
                placed.insert(index);
                order.push(index);
            }
            None => break,
        }
    }

    if placed.len() != count {
        let mut stuck: Vec<usize> = (0..count).filter(|i| !placed.contains(i)).collect();
        stuck.sort();
        let cycle: Vec<String> = stuck
            .iter()
            .map(|i| engine.plan.nodes[*i].id.clone().unwrap_or_default())
            .collect();
        for index in stuck {
            let node = &engine.plan.nodes[index];
            let (source, span, id) = (node.source.clone(), node.span, node.id.clone());
            engine.emit_at_node(
                PreflightError::ExecutionOrder,
                index,
                &source,
                span,
                format!("cycle|{}", id.clone().unwrap_or_default()),
                format!(
                    "`{}` is in an ordering cycle with {}; ordering cycles use error.execution.order before effects",
                    id.unwrap_or_default(),
                    cycle.join(", ")
                ),
            );
        }
        return;
    }

    engine.plan.order = order;
}
