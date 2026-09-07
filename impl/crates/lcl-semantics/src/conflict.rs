//! Conflict resolution and `OVERRIDE`.
//!
//! Authority: `05_SEMANTICS/04_AUTHORITY_PRIORITY_OVERRIDE_AND_CONFLICT_RESOLUTION.txt`.
//!
//! > For applicable clauses: higher authority wins only where clauses conflict;
//! > otherwise all remain. Equal-authority higher PRIORITY wins only when the
//! > lower clause is not an independently required hard rule. Equal hard
//! > contradictory clauses produce error.conflict.hard unless one exact
//! > OVERRIDE names WINNER and LOSER. A lower-authority winner is invalid.
//! > OVERRIDE never applies by similarity or broad category.
//!
//! ## What counts as a conflict
//!
//! Two clauses conflict when they cannot both be satisfied. This layer decides
//! that for the two shapes the language makes decidable before effects:
//!
//! 1. **Permission versus prohibition.** An `ALLOW` or a `REQUIRE`-with-`ACTION`
//!    naming the same operation and target as a `FORBID`. `05_SEMANTICS/03`:
//!    "FORBID blocks matching action even when an ACTION requires it unless a
//!    valid OVERRIDE resolves the exact conflict."
//! 2. **Contradictory hard assertions.** Two hard `ASSERT` clauses over the
//!    same subject that cannot both hold — the canonical example being
//!    `08_HARD_CONFLICT.invalid.lcl`, where `rule.one` asserts a flag is `TRUE`
//!    and `rule.two` asserts the same flag is `FALSE`.
//!
//! Anything less decidable than those is **not** reported. A conflict this
//! layer cannot prove is not a conflict it may invent: "otherwise all remain".
//!
//! ## Why matching is exact
//!
//! "OVERRIDE never applies by similarity or broad category" is a rule about
//! `OVERRIDE`, and the same discipline governs the conflict detection it
//! resolves. Two clauses are compared on their *written* operation and target,
//! so a `FORBID` is never widened onto an action it does not name, and never
//! narrowed off one it does.

use crate::authority::{AuthorityRecord, RuleKind};
use crate::diagnostic::PreflightError;
use crate::engine::Engine;
use crate::syntax;
use lcl_lexer::Span;
use lcl_parser::syntax::{BinaryOp, Expr};
use lcl_resolver::SourceId;
use std::collections::BTreeSet;

/// One declared `OVERRIDE`.
#[derive(Debug, Clone)]
struct Override {
    id: String,
    source: SourceId,
    span: Span,
    winner: Option<String>,
    loser: Option<String>,
}

/// Resolve conflicts among the applicable rule clauses.
pub(crate) fn resolve(engine: &mut Engine) {
    let overrides = collect_overrides(engine);
    check_override_validity(engine, &overrides);
    detect_permission_conflicts(engine, &overrides);
    detect_contradictory_assertions(engine, &overrides);
}

fn collect_overrides(engine: &Engine) -> Vec<Override> {
    let mut out = Vec::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "OVERRIDE" {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        out.push(Override {
            id: declaration.id.qualified(),
            source: declaration.source.clone(),
            span: declaration.id_span,
            winner: reference_field(block, "WINNER"),
            loser: reference_field(block, "LOSER"),
        });
    }
    out.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
    });
    out
}

fn reference_field(block: syntax::DeclBlock, name: &str) -> Option<String> {
    let field = block.field(name)?;
    let expr = syntax::inline_expr(&field.body)?;
    syntax::reference_target(expr).map(str::to_string)
}

/// "A lower-authority winner is invalid."
fn check_override_validity(engine: &mut Engine, overrides: &[Override]) {
    let mut invalid = Vec::new();
    for over in overrides {
        let (Some(winner_id), Some(loser_id)) = (&over.winner, &over.loser) else {
            // A missing endpoint is a schema defect the grammar stage already
            // owns; this layer does not report it a second time.
            continue;
        };
        let winner = engine.authorities.iter().find(|r| &r.id == winner_id);
        let loser = engine.authorities.iter().find(|r| &r.id == loser_id);
        let (Some(winner), Some(loser)) = (winner, loser) else {
            continue;
        };
        if winner.authority < loser.authority {
            invalid.push((
                over.source.clone(),
                over.span,
                format!(
                    "`{}` names `{}` at authority {} as winner over `{}` at authority {}; a lower-authority winner is invalid",
                    over.id, winner.id, winner.authority, loser.id, loser.authority
                ),
                over.id.clone(),
            ));
        }
    }
    for (source, span, detail, cause) in invalid {
        engine.emit(
            PreflightError::OverrideInvalid,
            &source,
            span,
            cause,
            detail,
        );
    }
}

/// Does an exact `OVERRIDE` resolve this exact pair?
///
/// Exact means exact: the override must name one of the two clauses as winner
/// and the *other* as loser. An override naming one endpoint and something else
/// resolves a different conflict, not this one.
fn resolved_by_override(overrides: &[Override], a: &str, b: &str) -> Option<String> {
    overrides
        .iter()
        .find(|over| {
            let (Some(winner), Some(loser)) = (&over.winner, &over.loser) else {
                return false;
            };
            (winner == a && loser == b) || (winner == b && loser == a)
        })
        .map(|over| over.id.clone())
}

/// Permission versus prohibition over the same operation and target.
fn detect_permission_conflicts(engine: &mut Engine, overrides: &[Override]) {
    let clauses: Vec<AuthorityRecord> = engine
        .authorities
        .iter()
        .filter(|r| r.is_applicable())
        .cloned()
        .collect();

    let mut emissions = Vec::new();
    for forbid in clauses.iter().filter(|r| r.kind == RuleKind::Forbid) {
        for permit in clauses.iter().filter(|r| r.kind.authorizes()) {
            if !same_subject(forbid, permit) {
                continue;
            }
            if resolved_by_override(overrides, &forbid.id, &permit.id).is_some() {
                continue;
            }
            // "higher authority wins only where clauses conflict" — a strictly
            // higher authority on either side resolves the conflict without a
            // diagnostic. The prohibition simply wins or simply loses.
            if forbid.authority != permit.authority {
                continue;
            }
            // Equal authority: `PRIORITY` decides only when the lower clause is
            // "not an independently required hard rule". `FORBID` is always a
            // hard rule, so priority can never defeat it, and `ALLOW` is not
            // hard, so a higher-priority `ALLOW` still cannot defeat a
            // `FORBID`. Only `REQUIRE` versus `FORBID` is a hard contradiction.
            if permit.kind == RuleKind::Allow {
                // "ALLOW ... Never defeats FORBID by itself." Not a hard
                // contradiction: the permission is simply not exercisable, and
                // the action that needs it is reported where it is authorized.
                continue;
            }
            emissions.push((
                forbid.source.clone(),
                forbid.span,
                format!("{}|{}", forbid.id, permit.id),
                format!(
                    "`{}` forbids {} on {} at authority {}, and `{}` requires it at the same authority; no exact OVERRIDE names a winner and a loser",
                    forbid.id,
                    forbid.operation.as_deref().unwrap_or("its operation"),
                    forbid.target.as_deref().unwrap_or("its target"),
                    forbid.authority,
                    permit.id
                ),
            ));
        }
    }
    for (source, span, cause, detail) in emissions {
        engine.emit(PreflightError::ConflictHard, &source, span, cause, detail);
    }
}

/// Two clauses name the same operation and the same target.
///
/// A clause that names no operation constrains no operation identity and is not
/// matched here; widening it to "every operation" would invent authority the
/// document did not write.
fn same_subject(left: &AuthorityRecord, right: &AuthorityRecord) -> bool {
    match (&left.operation, &right.operation) {
        (Some(a), Some(b)) if a == b => {}
        _ => return false,
    }
    match (&left.target, &right.target) {
        (Some(a), Some(b)) => a == b,
        // Two clauses over the same operation with no target on either side
        // name the same subject: the operation itself.
        (None, None) => true,
        _ => false,
    }
}

/// Two hard `ASSERT` clauses that cannot both hold.
///
/// The decidable case, and the one the canonical invalid example pins, is a
/// pair of equality assertions over the same subject expression demanding
/// different literal values. `08_HARD_CONFLICT.invalid.lcl` is exactly that:
///
/// ```text
/// REQUIRE: ASSERT: REF(data.flag) == TRUE
/// REQUIRE: ASSERT: REF(data.flag) == FALSE
/// ```
///
/// Nothing broader is attempted. Proving that two arbitrary predicates are
/// jointly unsatisfiable is not a decision the language asks this layer to
/// make, and a wrong guess would reject a valid document.
fn detect_contradictory_assertions(engine: &mut Engine, overrides: &[Override]) {
    #[derive(Clone)]
    struct Assertion {
        record: AuthorityRecord,
        subject: String,
        value: String,
        span: Span,
    }

    let mut assertions: Vec<Assertion> = Vec::new();
    for record in engine.authorities.iter().filter(|r| r.is_applicable()) {
        if !record.kind.is_hard() {
            continue;
        }
        let Some(block) = syntax::declaration_block(engine.resolved, record.declaration) else {
            continue;
        };
        let Some(field) = block.field("ASSERT") else {
            continue;
        };
        let Some(expr) = syntax::inline_expr(&field.body) else {
            continue;
        };
        if let Some((subject, value)) = equality_claim(expr) {
            assertions.push(Assertion {
                record: record.clone(),
                subject,
                value,
                span: field.body.span(),
            });
        }
    }

    let mut emissions = Vec::new();
    let mut reported: BTreeSet<(String, String)> = BTreeSet::new();
    for (i, left) in assertions.iter().enumerate() {
        for right in assertions.iter().skip(i + 1) {
            if left.subject != right.subject || left.value == right.value {
                continue;
            }
            if left.record.authority != right.record.authority {
                continue;
            }
            if resolved_by_override(overrides, &left.record.id, &right.record.id).is_some() {
                continue;
            }
            let key = if left.record.id <= right.record.id {
                (left.record.id.clone(), right.record.id.clone())
            } else {
                (right.record.id.clone(), left.record.id.clone())
            };
            if !reported.insert(key.clone()) {
                continue;
            }
            emissions.push((
                left.record.source.clone(),
                left.span,
                format!("{}|{}", key.0, key.1),
                format!(
                    "`{}` asserts {} == {} and `{}` asserts {} == {} at the same authority {}; no exact OVERRIDE names a winner and a loser",
                    left.record.id,
                    left.subject,
                    left.value,
                    right.record.id,
                    right.subject,
                    right.value,
                    left.record.authority
                ),
            ));
        }
    }
    for (source, span, cause, detail) in emissions {
        engine.emit(PreflightError::ConflictHard, &source, span, cause, detail);
    }
}

/// One `subject == literal` claim, rendered for comparison.
///
/// `!=` is deliberately not treated as a claim: two clauses demanding a value
/// differ from two different literals are jointly satisfiable.
fn equality_claim(expr: &Expr) -> Option<(String, String)> {
    match expr {
        Expr::Binary(binary) if binary.operator == BinaryOp::Equal => {
            let left = crate::eval::render_static(&binary.left);
            let right = crate::eval::render_static(&binary.right);
            // Order the pair so `a == TRUE` and `TRUE == a` are one claim.
            if is_literal(&binary.left) && !is_literal(&binary.right) {
                Some((right, left))
            } else if is_literal(&binary.right) && !is_literal(&binary.left) {
                Some((left, right))
            } else {
                None
            }
        }
        Expr::Group(group) => equality_claim(&group.inner),
        _ => None,
    }
}

fn is_literal(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) => true,
        Expr::Group(group) => is_literal(&group.inner),
        _ => false,
    }
}
