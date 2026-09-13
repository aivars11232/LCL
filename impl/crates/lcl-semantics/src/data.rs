//! Step 7: resolving input, state, memory, context, default, assumption and
//! dependencies.
//!
//! Authority: `05_SEMANTICS/06_MISSING_UNKNOWN_NULL_DEFAULT_ASSUME_AND_HANDLER_RESOLUTION.txt`.
//!
//! > MISSING: no value/source exists. UNKNOWN: value exists but cannot be
//! > determined. NULL: known explicit absence.
//! >
//! > Resolution order:
//! > 1. explicit VALUE;
//! > 2. resolved SOURCE/INPUT/STATE/MEMORY/CONTEXT;
//! > 3. DEFAULT only for MISSING;
//! > 4. applicable explicit ASSUME;
//! > 5. matching HANDLER selected by the event model below;
//! > 6. required item blocks/fails; optional item remains absent.
//! >
//! > DEFAULT never replaces NULL/UNKNOWN unless a rule explicitly maps them
//! > first. ASSUME is conditional, cannot override explicit data, and must be
//! > recorded as evidence. No silent guessing is permitted.
//!
//! ## Steps 1 to 4, and why not 5
//!
//! This layer implements steps 1 through 4 and step 6. Step 5 — handler
//! selection — is driven by the closed event model, and an event is raised only
//! by "a surviving diagnostic with a non-null event mapping", "at the producer
//! that emitted it". A preflight diagnostic here has no *producer invocation*
//! to attach a handler activation to, and `DEPENDENCY.HANDLER` is in scope
//! "only while resolving that DEPENDENCY". Handler activation is therefore
//! M6's, and this layer records the unresolved obligation exactly rather than
//! pretending a handler recovered it.
//!
//! ## Order is the rule, not an optimisation
//!
//! Each step is tried only when the previous one produced nothing, and step 3
//! is tried only for `MISSING` — never for `NULL` or `UNKNOWN`. That single
//! asymmetry is what "DEFAULT never replaces NULL/UNKNOWN" means, and it is why
//! the three sentinels have to be distinct values rather than one absence.

use crate::diagnostic::PreflightError;
use crate::engine::Engine;
use crate::eval;
use crate::plan::{EvidenceRecord, Origin, Resolution};
use crate::syntax;
use crate::value::Value;
use lcl_lexer::Span;
use lcl_resolver::SourceId;

/// The blocks whose values one invocation resolves.
///
/// `05_SEMANTICS/07`: "CONTEXT, MEMORY, and STATE are optional explicit typed
/// sources." `INPUT` "resolves VALUE or SOURCE under its TYPE/schema" and
/// `DATA` "is immutable material".
const SOURCE_BLOCKS: [&str; 5] = ["INPUT", "DATA", "CONTEXT", "MEMORY", "STATE"];

/// The one `DEFINE` whose declared value is readable in a value context.
///
/// `05_SEMANTICS/12`: "REF reads exactly one bound value of INPUT, DATA,
/// CONTEXT, MEMORY, STATE, OUTPUT, DEFINE kind.constant, or a loop-local
/// binding." Every other `DEFINE` -- a type, an operation, a format -- stays a
/// reference identity and has no value to read.
const CONSTANT_KIND: &str = "kind.constant";

/// True for the one declaration shape whose value a `REF` reads.
fn is_readable_constant(declaration: &lcl_resolver::Declaration) -> bool {
    declaration.block == "DEFINE" && declaration.definition_kind.as_deref() == Some(CONSTANT_KIND)
}

/// Resolve every declared datum and every declared dependency.
pub(crate) fn resolve(engine: &mut Engine) {
    resolve_sources(engine);
    record_unused_invocation_data(engine);
    resolve_dependencies(engine);
}

fn resolve_sources(engine: &mut Engine) {
    let mut declarations: Vec<(usize, String, String, SourceId, Span)> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .filter(|(_, d)| SOURCE_BLOCKS.contains(&d.block.as_str()) || is_readable_constant(d))
        .map(|(index, d)| {
            (
                index,
                d.block.clone(),
                d.id.qualified(),
                d.source.clone(),
                d.id_span,
            )
        })
        .collect();

    let order = source_order(
        engine,
        &declarations.iter().map(|d| d.0).collect::<Vec<_>>(),
    );
    declarations.sort_by_key(|d| order[&d.0]);
    let mut evidence = Vec::new();

    for (index, block, id, source, span) in declarations {
        let Some(syntax_block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };

        // 1. explicit VALUE.
        let mut origin = Origin::DeclaredValue;
        let mut value = declared_value(engine, &source, syntax_block);
        // An expression this layer could not fold is undecided, not absent.
        // `eval` says so in its own contract: its `None` "is **not** MISSING",
        // and the caller "must leave the obligation to the layer that demands
        // it rather than inventing an outcome". Inventing MISSING here would
        // also hand the declaration to DEFAULT, which the canonical resolution
        // order admits for MISSING and for nothing else.
        let undecided = value.is_none() && writes_a_value(&syntax_block);

        // A `DEFINE kind.constant` is exactly its declared value and nothing
        // else. It is not a source an invocation supplies, it takes no DEFAULT
        // and no ASSUME applies to it, so the remaining steps are skipped
        // rather than given a constant to reinterpret.
        //
        // Repair, post-Task-20 finding F14. Every step of the read path already
        // honoured `05_SEMANTICS/12` except this one: M4 records the constant's
        // statically known value and judges a `REF` to it as a value context,
        // and the runtime's `declaration_value` reads the plan's resolutions.
        // Constants were never put in them, so the read fell through to
        // MISSING, an operation parameter quietly took its registered default,
        // and a declared bound had no INTEGER to check.
        if block == "DEFINE" {
            engine.plan.resolutions.push(Resolution {
                declaration: index,
                id,
                block,
                source,
                span,
                origin: match (value.is_some(), undecided) {
                    (true, _) => Origin::DeclaredValue,
                    (false, true) => Origin::DeclaredValue,
                    (false, false) => Origin::Absent,
                },
                value: value.unwrap_or(match undecided {
                    true => Value::Unknown,
                    false => Value::Missing,
                }),
            });
            continue;
        }

        // 2. resolved SOURCE/INPUT/STATE/MEMORY/CONTEXT — the explicit data
        //    this invocation supplied for this declaration.
        if value.is_none() {
            if let Some(supplied) = engine.invocation.get(&id) {
                origin = Origin::Supplied;
                value = Some(supplied.clone());
            }
        }

        // A declaration with neither is MISSING: "no value/source exists". One
        // whose written expression could not be decided here is UNKNOWN: "value
        // exists but cannot be established".
        let mut resolved = value.unwrap_or(match undecided {
            true => Value::Unknown,
            false => Value::Missing,
        });

        // 3. DEFAULT, and only for MISSING.
        if resolved == Value::Missing {
            if let Some(default) = syntax_block
                .field("DEFAULT")
                .and_then(|field| eval::field_value(engine, &source, &field.body))
            {
                origin = Origin::Default;
                resolved = default;
            }
        }

        // 4. applicable explicit ASSUME.
        if !resolved.is_material() {
            if let Some((assumption, assumed)) = applicable_assumption(engine, &id) {
                origin = Origin::Assumption;
                resolved = assumed;
                evidence.push(assumption);
            }
        }

        if resolved == Value::Missing && origin == Origin::DeclaredValue {
            origin = Origin::Absent;
        }

        engine.plan.resolutions.push(Resolution {
            declaration: index,
            id,
            block,
            source,
            span,
            origin,
            value: resolved,
        });
    }

    engine.plan.resolutions.sort_by_key(|r| r.declaration);
    engine.plan.evidence.extend(evidence);
}

/// Resolve material sources before the declarations that read them. These are
/// pure value dependencies, never producer activation or execution-order edges.
/// The finite worklist also avoids recursive evaluation of declaration chains.
fn source_order(engine: &Engine, indexes: &[usize]) -> std::collections::BTreeMap<usize, usize> {
    use std::collections::{BTreeMap, BTreeSet};
    let sources: BTreeSet<_> = indexes.iter().copied().collect();
    let mut pending = BTreeMap::new();
    for &index in indexes {
        let declaration = &engine.resolved.declarations().all()[index];
        let mut fields = Vec::new();
        if let Some(block) = syntax::declaration_block(engine.resolved, index) {
            for name in ["VALUE", "DEFAULT"] {
                if let Some(field) = block.field(name) {
                    fields.push((&declaration.source, field.body.span()));
                }
            }
        }
        // An applicable assumption is part of source resolution too. Inspect
        // its condition/value dependencies without activating any effect.
        for (assumption_index, assumption) in
            engine.resolved.declarations().all().iter().enumerate()
        {
            if assumption.block != "ASSUME" {
                continue;
            }
            let Some(block) = syntax::declaration_block(engine.resolved, assumption_index) else {
                continue;
            };
            let Some(target) = block.field("TARGET") else {
                continue;
            };
            let target_span = target.body.span();
            let applies_to = engine.resolved.bindings().iter().any(|b| {
                b.source == assumption.source
                    && target_span.start <= b.span.start
                    && b.span.end <= target_span.end
                    && b.target == lcl_resolver::BindingTarget::Declaration(index)
            });
            if applies_to {
                for name in ["WHEN", "VALUE"] {
                    if let Some(field) = block.field(name) {
                        fields.push((&assumption.source, field.body.span()));
                    }
                }
            }
        }
        let dependencies: BTreeSet<_> = engine
            .resolved
            .bindings()
            .iter()
            .filter_map(|binding| {
                let lcl_resolver::BindingTarget::Declaration(target) = binding.target else {
                    return None;
                };
                (sources.contains(&target)
                    && fields.iter().any(|(source, span)| {
                        *source == &binding.source
                            && span.start <= binding.span.start
                            && binding.span.end <= span.end
                    }))
                .then_some(target)
            })
            .collect();
        pending.insert(index, dependencies);
    }
    let mut order = BTreeMap::new();
    while let Some(next) = pending
        .iter()
        .find(|(_, dependencies)| dependencies.iter().all(|d| !pending.contains_key(d)))
        .map(|(index, _)| *index)
    {
        pending.remove(&next);
        order.insert(next, order.len());
    }
    // A retained identity or an undemanded conditional reference can form a
    // structural cycle without a value cycle. Leave such residual declarations
    // to the ordinary bounded evaluator, rather than inventing a cycle error.
    for index in pending.keys() {
        order.insert(*index, order.len());
    }
    order
}

/// The value a declaration writes inline, when it writes one this layer can
/// read without demanding anything.
fn declared_value(engine: &Engine, source: &SourceId, block: syntax::DeclBlock) -> Option<Value> {
    let field = block.field("VALUE")?;
    eval::field_value(engine, source, &field.body)
}

/// Whether this declaration writes an explicit `VALUE` at all.
///
/// The distinction the resolution order turns on. A declaration with no `VALUE`
/// field and nothing supplied has "no value/source", which is MISSING and which
/// `DEFAULT` exists for. A declaration that *does* write one has a value and a
/// source whatever this layer manages to work out about the expression, so the
/// absence of a computed result here is "cannot be established", not "is not
/// there" — and `05_SEMANTICS/06` gives those two states different names,
/// different fallback rules and different meanings.
fn writes_a_value(block: &syntax::DeclBlock) -> bool {
    block.field("VALUE").is_some()
}

/// The first applicable `ASSUME` for one target, with its evidence record.
///
/// "ASSUME is conditional, cannot override explicit data, and must be recorded
/// as evidence." The caller has already established that no explicit data
/// exists, so the first rule is satisfied by construction; this function
/// supplies the evidence the third rule requires.
fn applicable_assumption(engine: &Engine, target_id: &str) -> Option<(EvidenceRecord, Value)> {
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "ASSUME" {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let names_target = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(syntax::reference_target)
            .map(|id| id == target_id)
            .unwrap_or(false);
        if !names_target {
            continue;
        }
        // `ASSUME.WHEN` is a required field. An assumption whose condition is
        // not TRUE does not apply, and one whose condition cannot be decided
        // before effects does not apply either: "No silent guessing is
        // permitted."
        let applies = block
            .field("WHEN")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| eval::literal_value(engine, &declaration.source, expr))
            .and_then(|value| value.boolean())
            .unwrap_or(false);
        if !applies {
            continue;
        }
        let value = block
            .field("VALUE")
            .and_then(|field| eval::field_value(engine, &declaration.source, &field.body))?;
        return Some((
            EvidenceRecord {
                declaration: index,
                id: declaration.id.qualified(),
                source: declaration.source.clone(),
                span: declaration.id_span,
                detail: format!("assumed {target_id} = {value}"),
            },
            value,
        ));
    }
    None
}

/// Supplied data whose declaration this document does not declare.
///
/// Recorded, never read. A caller that misspells an id learns that its datum
/// went nowhere instead of silently getting `MISSING` behaviour.
fn record_unused_invocation_data(engine: &mut Engine) {
    let declared: Vec<String> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .filter(|d| SOURCE_BLOCKS.contains(&d.block.as_str()))
        .map(|d| d.id.qualified())
        .collect();
    engine.unused = engine
        .invocation
        .supplied()
        .map(|(id, _)| id.clone())
        .filter(|id| !declared.contains(id))
        .collect();
}

/// Resolve every declared `DEPENDENCY`.
///
/// `05_SEMANTICS/09`: "error.dependency.unsatisfied and error.scope.violation
/// are pre_effect only: dependency availability and effective scope resolve
/// before the first authorized effect."
///
/// The registry gives `error.dependency.unsatisfied` the meaning "A required
/// dependency is FALSE, MISSING, or UNKNOWN" and the default status
/// `status.blocked`, which is why an unresolvable dependency blocks rather than
/// invalidates: the document is not wrong, the world is not ready.
fn resolve_dependencies(engine: &mut Engine) {
    let declarations: Vec<(usize, String, SourceId, Span)> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .filter(|(_, d)| d.block == "DEPENDENCY")
        .map(|(index, d)| (index, d.id.qualified(), d.source.clone(), d.id_span))
        .collect();

    let required_default = engine
        .contracts
        .field_default_boolean("DEPENDENCY", "REQUIRED")
        .unwrap_or(true);

    let mut failures = Vec::new();
    for (index, id, source, span) in declarations {
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let required = block
            .field("REQUIRED")
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| eval::literal_value(engine, &source, expr))
            .and_then(|value| value.boolean())
            .unwrap_or(required_default);
        if !required {
            continue;
        }

        // `WHEN` gates applicability. An absent `WHEN` means TRUE.
        let applicable = match block
            .field("WHEN")
            .and_then(|f| syntax::inline_expr(&f.body))
        {
            None => true,
            Some(expr) => eval::literal_value(engine, &source, expr)
                .and_then(|value| value.boolean())
                .unwrap_or(true),
        };
        if !applicable {
            continue;
        }

        // A `DEPENDENCY` without `ASSERT` states availability of its
        // `REFERENCE`, which resolution already proved. Only a declared
        // `ASSERT` carries a truth this layer can find FALSE, MISSING or
        // UNKNOWN.
        let Some(assert) = block
            .field("ASSERT")
            .and_then(|f| syntax::inline_expr(&f.body))
        else {
            continue;
        };
        match eval::literal_value(engine, &source, assert) {
            Some(Value::Boolean(true)) => {}
            Some(Value::Boolean(false)) => failures.push((
                PreflightError::DependencyUnsatisfied,
                source.clone(),
                span,
                id.clone(),
                format!("required dependency `{id}` asserts a condition that is FALSE"),
            )),
            Some(Value::Unknown) => failures.push((
                PreflightError::DependencyUnsatisfied,
                source.clone(),
                span,
                id.clone(),
                format!("required dependency `{id}` asserts a condition that is UNKNOWN"),
            )),
            Some(Value::Missing) => failures.push((
                PreflightError::DependencyUnsatisfied,
                source.clone(),
                span,
                id.clone(),
                format!("required dependency `{id}` asserts a condition that is MISSING"),
            )),
            // A condition whose value this layer cannot establish without
            // demanding an effect is not reported as unsatisfied: that would
            // invent a failure. It is left to the demanding layer.
            _ => {}
        }
    }

    for (id, source, span, cause, detail) in failures {
        engine.emit(id, &source, span, cause, detail);
    }
}
