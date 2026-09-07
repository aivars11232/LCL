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

/// Resolve every declared datum and every declared dependency.
pub(crate) fn resolve(engine: &mut Engine) {
    resolve_sources(engine);
    record_unused_invocation_data(engine);
    resolve_dependencies(engine);
}

fn resolve_sources(engine: &mut Engine) {
    let declarations: Vec<(usize, String, String, SourceId, Span)> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .filter(|(_, d)| SOURCE_BLOCKS.contains(&d.block.as_str()))
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

    let mut resolutions = Vec::new();
    let mut evidence = Vec::new();

    for (index, block, id, source, span) in declarations {
        let Some(syntax_block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };

        // 1. explicit VALUE.
        let mut origin = Origin::DeclaredValue;
        let mut value = declared_value(engine, syntax_block);

        // 2. resolved SOURCE/INPUT/STATE/MEMORY/CONTEXT — the explicit data
        //    this invocation supplied for this declaration.
        if value.is_none() {
            if let Some(supplied) = engine.invocation.get(&id) {
                origin = Origin::Supplied;
                value = Some(supplied.clone());
            }
        }

        // A declaration with neither is MISSING: "no value/source exists".
        let mut resolved = value.unwrap_or(Value::Missing);

        // 3. DEFAULT, and only for MISSING.
        if resolved == Value::Missing {
            if let Some(default) = syntax_block
                .field("DEFAULT")
                .and_then(|f| syntax::inline_expr(&f.body))
                .and_then(|expr| eval::literal_value(engine, &source, expr))
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

        resolutions.push(Resolution {
            declaration: index,
            id,
            block,
            source,
            span,
            origin,
            value: resolved,
        });
    }

    engine.plan.resolutions = resolutions;
    engine.plan.evidence.extend(evidence);
}

/// The value a declaration writes inline, when it writes one this layer can
/// read without demanding anything.
fn declared_value(engine: &Engine, block: syntax::DeclBlock) -> Option<Value> {
    let field = block.field("VALUE")?;
    let source = engine.resolved.root().clone();
    if let Some(collection) = syntax::inline_collection(&field.body) {
        let mut members = Vec::new();
        for member in &collection.members {
            members.push(eval::literal_value(engine, &source, member)?);
        }
        return Some(Value::List(members));
    }
    let expr = syntax::inline_expr(&field.body)?;
    eval::literal_value(engine, &source, expr)
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
            .and_then(|f| syntax::inline_expr(&f.body))
            .and_then(|expr| eval::literal_value(engine, &declaration.source, expr))?;
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
