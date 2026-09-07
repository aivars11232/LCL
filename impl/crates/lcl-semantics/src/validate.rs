//! Step 8: run every selected and applicable pre-effect `VALIDATE`.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/check_selection_contract` and
//! `01_FOUNDATION/03` step 8.
//!
//! > Run every selected and applicable VALIDATE check before side effects,
//! > including optional checks and prerequisites. REQUIRED controls blocking
//! > on FALSE; check selection and ordering follow check_selection_contract.
//!
//! ## The three questions, in order
//!
//! 1. **Is it selected?** `selection`: "Select VALIDATE, VERIFY, and FAILURE
//!    declarations in the EXECUTE root source document. A targetless clause
//!    applies to that invocation." A targeted clause is selected only when its
//!    `TARGET` names a candidate graph member, a data or output declaration the
//!    graph explicitly references, or the exact material target a graph
//!    `ACTION` selected. "Unrelated imported checks never run merely because
//!    their document was imported."
//! 2. **Is it applicable?** `demand`: "Where WHEN exists, absence means TRUE
//!    and FALSE skips applicability. A skipped check has no result."
//! 3. **What does it assert?** Evaluated purely, before any effect.
//!
//! `REQUIRED` enters only at the end: "REQUIRED controls whether a FALSE result
//! blocks, not whether a selected check runs." An optional check still runs and
//! still records its Boolean outcome; it simply does not raise
//! `error.validation.failed` when that outcome is FALSE.
//!
//! ## No check may defer itself past effects
//!
//! `prerequisites`: "Every selected pre-effect VALIDATE, including an optional
//! check or prerequisite, must be evaluable before effects and cannot depend on
//! future OUTPUT or a post-execution check."
//!
//! This is the rule that makes preflight worth having, so it is enforced rather
//! than assumed. A selected check whose assertion this layer cannot evaluate is
//! not quietly postponed: it fails here, with the registered identifier for
//! what was missing — `error.required.missing` for an absent value and
//! `error.value.unknown` for an indeterminable one, both `status.blocked`.

use crate::diagnostic::PreflightError;
use crate::engine::Engine;
use crate::eval;
use crate::plan::{CheckResult, Selection};
use crate::syntax;
use crate::value::Value;
use lcl_lexer::Span;
use lcl_resolver::SourceId;
use std::collections::{BTreeMap, BTreeSet};

/// One selected check, before evaluation.
struct Selected {
    declaration: usize,
    id: String,
    source: SourceId,
    span: Span,
    required: bool,
    selection: Selection,
}

/// Select and run every applicable pre-effect `VALIDATE`.
pub(crate) fn run(engine: &mut Engine) {
    let selected = select(engine);
    let ordered = match order_by_prerequisites(engine, selected) {
        Some(ordered) => ordered,
        // A prerequisite cycle was reported; evaluating any of them would be
        // evaluating a chain that has no first element.
        None => return,
    };
    evaluate(engine, ordered);
}

/// Every `VALIDATE` this invocation selects.
fn select(engine: &Engine) -> Vec<Selected> {
    let root = engine.resolved.root().clone();
    let graph_members = graph_member_ids(engine);
    let graph_references = graph_referenced_ids(engine);
    let material_targets = graph_material_targets(engine);
    let prerequisites = prerequisite_ids(engine);

    let mut out = Vec::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "VALIDATE" {
            continue;
        }
        // "Select VALIDATE ... declarations in the EXECUTE root source
        // document. ... Unrelated imported checks never run merely because
        // their document was imported."
        let in_root = declaration.source == root;
        let id = declaration.id.qualified();
        let is_prerequisite = prerequisites.contains(&id);
        if !in_root && !is_prerequisite {
            continue;
        }

        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };

        let target = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body));
        let selection = match target {
            // "A targetless clause applies to that invocation."
            None => Selection::Targetless,
            Some(expr) => {
                let named = syntax::reference_target(expr).map(str::to_string);
                match named {
                    Some(named) => {
                        if let Some(node) = graph_members.get(&named) {
                            Selection::GraphMember(*node)
                        } else if let Some(declaration) = graph_references.get(&named) {
                            Selection::ReferencedDeclaration(*declaration)
                        } else if is_prerequisite {
                            Selection::Prerequisite
                        } else {
                            // "Match declaration identity or material identity
                            // ...; never expand ambient resources." A target
                            // this invocation's graph does not reach selects
                            // nothing.
                            continue;
                        }
                    }
                    None => {
                        // A material target: selected when a graph ACTION acts
                        // on that exact value. Witness CLOSURE-065.
                        let rendered = eval::render_static(expr);
                        if material_targets.contains(&rendered) {
                            Selection::MaterialTarget(rendered)
                        } else if is_prerequisite {
                            Selection::Prerequisite
                        } else {
                            continue;
                        }
                    }
                }
            }
        };

        // A `VALIDATE` selected only because something references it is a
        // prerequisite, whatever its own target says.
        let selection = if is_prerequisite && !in_root {
            Selection::Prerequisite
        } else {
            selection
        };

        let required = block
            .field("REQUIRED")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| eval::literal_value(engine, &declaration.source, expr))
            .and_then(|value| value.boolean())
            .unwrap_or_else(|| {
                engine
                    .contracts
                    .field_default_boolean("VALIDATE", "REQUIRED")
                    .unwrap_or(true)
            });

        out.push(Selected {
            declaration: index,
            id,
            source: declaration.source.clone(),
            span: declaration.id_span,
            required,
            selection,
        });
    }
    out
}

/// Declaration ids the candidate graph activates, with their node indexes.
fn graph_member_ids(engine: &Engine) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (node, member) in engine.resolved.graph().nodes().iter().enumerate() {
        if let Some(declaration) = member.declaration {
            if let Some(d) = engine.resolved.declarations().get(declaration) {
                out.insert(d.id.qualified(), node);
            }
        }
    }
    out
}

/// Data and output declarations the graph explicitly references.
///
/// "a data/output declaration explicitly referenced by that graph". Nothing
/// ambient: a declaration is here only because a graph member's own source
/// names it.
fn graph_referenced_ids(engine: &Engine) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    let member_declarations: BTreeSet<usize> = engine
        .resolved
        .graph()
        .nodes()
        .iter()
        .filter_map(|n| n.declaration)
        .collect();

    for &member in &member_declarations {
        let Some(block) = syntax::declaration_block(engine.resolved, member) else {
            continue;
        };
        for statement in block.statements() {
            let lcl_parser::syntax::Statement::Field(field) = statement else {
                continue;
            };
            for (id, _) in syntax::reference_list(&field.body) {
                if let Some(index) = engine
                    .resolved
                    .declarations()
                    .all()
                    .iter()
                    .position(|d| d.id.qualified() == id)
                {
                    let block_name = &engine.resolved.declarations().all()[index].block;
                    if matches!(block_name.as_str(), "DATA" | "OUTPUT" | "INPUT") {
                        out.insert(id, index);
                    }
                }
            }
        }
    }
    out
}

/// The exact material targets graph actions select.
fn graph_material_targets(engine: &Engine) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for node in engine.resolved.graph().nodes() {
        let Some(declaration) = node.declaration else {
            continue;
        };
        let Some(block) = syntax::declaration_block(engine.resolved, declaration) else {
            continue;
        };
        if block.key_text() != "ACTION" {
            continue;
        }
        if let Some(expr) = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body))
        {
            if syntax::reference_target(expr).is_none() {
                out.insert(eval::render_static(expr));
            }
        }
    }
    out
}

/// Check ids explicitly referenced by a selected check or the root `SUCCESS`.
///
/// "Include check prerequisites explicitly referenced by selected checks or the
/// root SUCCESS."
fn prerequisite_ids(engine: &Engine) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let root = engine.resolved.root().clone();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        let interesting = declaration.source == root
            && matches!(declaration.block.as_str(), "VALIDATE" | "SUCCESS");
        if !interesting {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        for statement in block.statements() {
            let lcl_parser::syntax::Statement::Field(field) = statement else {
                continue;
            };
            for (id, _) in syntax::reference_list(&field.body) {
                out.insert(id);
            }
        }
    }
    out
}

/// Order selected checks so a prerequisite is evaluated before its dependent.
///
/// "Evaluate selected check-result prerequisites in stable topological order
/// with source-unit namespace then source declaration order as tie-breakers. A
/// prerequisite cycle uses error.reference.cycle."
///
/// Returns `None` when a cycle was found and reported.
fn order_by_prerequisites(engine: &mut Engine, selected: Vec<Selected>) -> Option<Vec<Selected>> {
    let ids: BTreeSet<String> = selected.iter().map(|s| s.id.clone()).collect();

    // Edges: dependent -> the prerequisites it reads.
    let mut needs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for check in &selected {
        let mut required = BTreeSet::new();
        if let Some(block) = syntax::declaration_block(engine.resolved, check.declaration) {
            if let Some(field) = block.field("ASSERT") {
                for (id, _) in syntax::reference_list(&field.body) {
                    if ids.contains(&id) && id != check.id {
                        required.insert(id);
                    }
                }
                // A reference nested inside an expression, not just a bare
                // reference list.
                if let Some(expr) = syntax::inline_expr(&field.body) {
                    for id in referenced_ids(expr) {
                        if ids.contains(&id) && id != check.id {
                            required.insert(id);
                        }
                    }
                }
            }
        }
        needs.insert(check.id.clone(), required);
    }

    // The stable tie-break order: source unit, then source declaration order.
    let mut pending: Vec<&Selected> = selected.iter().collect();
    pending.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
    });

    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = Vec::new();
    loop {
        let mut progressed = false;
        for check in &pending {
            if done.contains(&check.id) {
                continue;
            }
            let ready = needs
                .get(&check.id)
                .map(|r| r.iter().all(|id| done.contains(id)))
                .unwrap_or(true);
            if ready {
                done.insert(check.id.clone());
                order.push(check.id.clone());
                progressed = true;
            }
        }
        if done.len() == pending.len() {
            break;
        }
        if !progressed {
            // Everything left is in, or behind, a cycle.
            let mut stuck: Vec<(SourceId, Span, String)> = pending
                .iter()
                .filter(|c| !done.contains(&c.id))
                .map(|c| (c.source.clone(), c.span, c.id.clone()))
                .collect();
            stuck.sort();
            for (source, span, id) in stuck {
                engine.emit(
                    PreflightError::ReferenceCycle,
                    &source,
                    span,
                    id.clone(),
                    format!("check `{id}` is in a prerequisite cycle, so no first check exists"),
                );
            }
            return None;
        }
    }

    let mut by_id: BTreeMap<String, Selected> =
        selected.into_iter().map(|s| (s.id.clone(), s)).collect();
    Some(
        order
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect(),
    )
}

/// Every declaration id an expression references.
fn referenced_ids(expr: &lcl_parser::syntax::Expr) -> Vec<String> {
    use lcl_parser::syntax::Expr;
    let mut out = Vec::new();
    let mut stack = vec![expr];
    while let Some(current) = stack.pop() {
        match current {
            Expr::Call(call) => {
                if call.is_reference() {
                    if let Some(Expr::Identifier(ident)) = call.arguments.first() {
                        out.push(ident.text.clone());
                        continue;
                    }
                }
                stack.extend(call.arguments.iter());
            }
            Expr::Group(group) => stack.push(&group.inner),
            Expr::Unary(unary) => stack.push(&unary.operand),
            Expr::Binary(binary) => {
                stack.push(&binary.left);
                stack.push(&binary.right);
            }
            Expr::Collection(collection) => stack.extend(collection.members.iter()),
            Expr::Property(property) => stack.push(&property.base),
            Expr::Index(index) => {
                stack.push(&index.base);
                stack.push(&index.index);
            }
            Expr::Literal(_) | Expr::Identifier(_) | Expr::Type(_) => {}
        }
    }
    out
}

/// Evaluate each selected check in order and record its outcome.
fn evaluate(engine: &mut Engine, selected: Vec<Selected>) {
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for check in selected {
        let Some(block) = syntax::declaration_block(engine.resolved, check.declaration) else {
            continue;
        };

        // "Where WHEN exists, absence means TRUE and FALSE skips
        // applicability. A skipped check has no result."
        let applicable = match block
            .field("WHEN")
            .and_then(|f| syntax::inline_expr(&f.body))
        {
            None => true,
            Some(expr) => match eval::literal_value(engine, &check.source, expr) {
                Some(Value::Boolean(value)) => value,
                // "a WHEN evaluating MISSING or UNKNOWN does not match". An
                // applicability condition that cannot be decided does not make
                // the check applicable.
                _ => false,
            },
        };
        if !applicable {
            results.push(CheckResult {
                declaration: check.declaration,
                id: check.id,
                source: check.source,
                span: check.span,
                required: check.required,
                selection: check.selection,
                outcome: None,
            });
            continue;
        }

        let assertion = block
            .field("ASSERT")
            .and_then(|f| syntax::inline_expr(&f.body));
        let (outcome, failure) = match assertion {
            None => (None, None),
            Some(expr) => match eval::literal_value(engine, &check.source, expr) {
                Some(Value::Boolean(true)) => (Some(Value::Boolean(true)), None),
                Some(Value::Boolean(false)) => {
                    let failure = check.required.then(|| {
                        (
                            PreflightError::ValidationFailed,
                            format!(
                                "required check `{}` asserts a condition that is FALSE",
                                check.id
                            ),
                        )
                    });
                    (Some(Value::Boolean(false)), failure)
                }
                // "Required demanded MISSING and UNKNOWN use
                // error.required.missing and error.value.unknown."
                Some(Value::Missing) => (
                    Some(Value::Missing),
                    check.required.then(|| {
                        (
                            PreflightError::RequiredMissing,
                            format!(
                                "required check `{}` demands a value that is MISSING before effects",
                                check.id
                            ),
                        )
                    }),
                ),
                Some(Value::Unknown) => (
                    Some(Value::Unknown),
                    check.required.then(|| {
                        (
                            PreflightError::ValueUnknown,
                            format!(
                                "required check `{}` demands a value that is UNKNOWN before effects",
                                check.id
                            ),
                        )
                    }),
                ),
                // A selected pre-effect check that this layer cannot evaluate
                // "cannot depend on future OUTPUT ... and silently defer itself
                // past effects". It is reported here rather than postponed.
                Some(_) | None => (
                    None,
                    check.required.then(|| {
                        (
                            PreflightError::RequiredMissing,
                            format!(
                                "required check `{}` cannot be evaluated before effects; a selected pre-effect VALIDATE may not defer itself past them",
                                check.id
                            ),
                        )
                    }),
                ),
            },
        };

        if let Some((id, detail)) = failure {
            failures.push((
                id,
                check.source.clone(),
                check.span,
                check.id.clone(),
                detail,
            ));
        }

        results.push(CheckResult {
            declaration: check.declaration,
            id: check.id,
            source: check.source,
            span: check.span,
            required: check.required,
            selection: check.selection,
            outcome,
        });
    }

    engine.plan.checks = results;
    for (id, source, span, cause, detail) in failures {
        engine.emit(id, &source, span, cause, detail);
    }
}
