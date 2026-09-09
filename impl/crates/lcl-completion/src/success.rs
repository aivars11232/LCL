//! Step 12b: evaluate declared `SUCCESS`, then declared `FAILURE`.
//!
//! Authority: `05_SEMANTICS/10`, `check_selection_contract#/root_success` and
//! `failure_lifecycle#/{status_rule,failure_mapping_rule}`.
//!
//! ## The rule that outranks everything else here
//!
//! > An existing primary unhandled diagnostic always fixes status by its
//! > resolved default_status ... A FAILURE mapping cannot override that
//! > diagnostic.
//!
//! and, from `status_rule`:
//!
//! > secondary diagnostics and FAILURE mappings never override it.
//!
//! So [`Verdict`] records what `SUCCESS` and `FAILURE` *decided*, and says
//! nothing about status. Turning a verdict into a terminal status is
//! [`crate::terminal`]'s job, and it consults the primary diagnostic first.
//! Keeping the two apart is what makes "a FAILURE mapping cannot override that
//! diagnostic" a structural property rather than a rule to remember.

use crate::check::Checks;
use crate::diagnostic::CompletionError;
use crate::engine::{Emission, Engine};
use crate::syntax;
use lcl_lexer::Span;
use lcl_resolver::SourceId;
use lcl_runtime::Value;

/// The `SUCCESS` declaration the execution root references.
///
/// Reached through the root declaration's own `SUCCESS` field, never by
/// scanning the document: an unreferenced `SUCCESS` declaration governs
/// nothing, and `TASK`'s schema makes the field required precisely so the root
/// names its own.
pub(crate) fn root_success_declaration(engine: &Engine) -> Option<usize> {
    let root = engine.root_declaration()?;
    let block = lcl_runtime::syntax::declaration_block(engine.resolved, root)?;
    let field = block.field("SUCCESS")?;
    let (id, _) = syntax::reference_list(&field.body).into_iter().next()?;
    engine
        .resolved
        .declarations()
        .all()
        .iter()
        .position(|d| d.id.qualified() == id && d.block == "SUCCESS")
}

/// Which quantifier a `SUCCESS` declaration uses.
///
/// > SUCCESS contains exactly ALL, ANY, or NONE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Quantifier {
    All,
    Any,
    None,
}

impl Quantifier {
    pub fn field(self) -> &'static str {
        match self {
            Quantifier::All => "ALL",
            Quantifier::Any => "ANY",
            Quantifier::None => "NONE",
        }
    }
}

impl std::fmt::Display for Quantifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.field())
    }
}

/// What `SUCCESS` evaluated to, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuccessOutcome {
    pub id: String,
    pub quantifier: Quantifier,
    /// Each member reference and the value it contributed, in source order.
    pub members: Vec<(String, Value)>,
    /// TRUE, FALSE or UNKNOWN.
    pub value: Value,
}

impl SuccessOutcome {
    pub fn satisfied(&self) -> bool {
        matches!(self.value, Value::Boolean(true))
    }
}

/// One selected declared `FAILURE` clause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedFailure {
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    /// The canonical status the clause requests, after alias resolution.
    pub requested_status: String,
    /// The status exactly as written, before alias resolution.
    pub written_status: String,
    /// `ERROR` is "exact classification metadata", not an emitted diagnostic.
    pub classification: Option<String>,
    /// Declared `EVIDENCE` references.
    pub evidence: Vec<String>,
}

/// What steps 12's success and failure evaluation decided.
///
/// Deliberately status-free. See the module note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Verdict {
    /// The root `SUCCESS`, when the root references one.
    pub success: Option<SuccessOutcome>,
    /// The first `FAILURE` clause whose `WHEN` was TRUE, when one was selected.
    pub failure: Option<SelectedFailure>,
}

impl Verdict {
    /// True exactly when nothing this step decided prevents success.
    ///
    /// A selected `FAILURE` "prevents success even when other SUCCESS
    /// conditions are TRUE", so it is checked first.
    pub fn permits_success(&self) -> bool {
        if self.failure.is_some() {
            return false;
        }
        match &self.success {
            Some(success) => success.satisfied(),
            // "PHASE, SEQUENCE and ACTION roots succeed when all applicable
            // required graph, check, rule, evidence and OUTPUT obligations
            // complete, without requiring an unavailable SUCCESS field."
            None => true,
        }
    }

    pub fn serialize(&self) -> String {
        let mut out = String::from("VERDICT\n");
        if let Some(success) = &self.success {
            out.push_str(&format!(
                "  success {} {} = {}\n",
                success.id, success.quantifier, success.value
            ));
            for (id, value) in &success.members {
                out.push_str(&format!("    member {id} = {value}\n"));
            }
        }
        if let Some(failure) = &self.failure {
            out.push_str(&format!(
                "  failure {} requests {}\n",
                failure.id, failure.requested_status
            ));
        }
        out
    }
}

/// Evaluate the root `SUCCESS` and then the declared `FAILURE` clauses.
pub(crate) fn run(engine: &mut Engine, checks: &Checks) -> Verdict {
    let success = evaluate_success(engine);
    let failure = select_failure(engine, checks, success.as_ref());
    Verdict { success, failure }
}

/// Evaluate the root `SUCCESS` declaration.
fn evaluate_success(engine: &mut Engine) -> Option<SuccessOutcome> {
    let declaration = root_success_declaration(engine)?;
    let id = engine
        .resolved
        .declarations()
        .get(declaration)
        .map(|d| d.id.qualified())?;
    let block = lcl_runtime::syntax::declaration_block(engine.resolved, declaration)?;

    // "Exactly one ALL, ANY, or NONE." The block schema enforces exactly-one at
    // M2, so the first present field is the declared quantifier.
    let (quantifier, field) = [Quantifier::All, Quantifier::Any, Quantifier::None]
        .into_iter()
        .find_map(|q| block.field(q.field()).map(|f| (q, f)))?;

    // Members may be written as a reference LIST or as a single expression.
    let references = syntax::reference_list(&field.body);
    let mut members: Vec<(String, Value)> = Vec::new();
    if references.is_empty() {
        if let Some(expr) = syntax::inline_expr(&field.body) {
            let expr = expr.clone();
            let value = engine.evaluator().demand(&expr).unwrap_or(Value::Unknown);
            members.push((lcl_runtime::syntax::render(&expr), value));
        }
    } else {
        for (member, _) in references {
            // A member reference reads one scheduled result rather than
            // rerunning anything: "Check references observe one scheduled
            // result rather than rerunning the check."
            let value = engine.evaluator().declaration_value(&member);
            members.push((member, value));
        }
    }

    let value = quantify(quantifier, &members);
    Some(SuccessOutcome {
        id,
        quantifier,
        members,
        value,
    })
}

/// Apply one quantifier over its members under three-valued logic.
///
/// A member that is MISSING or UNKNOWN cannot establish truth. It is not read
/// as FALSE either — "A skipped check has no result ... never implicit TRUE"
/// cuts both ways, and inventing FALSE would be as much a fabrication as
/// inventing TRUE.
fn quantify(quantifier: Quantifier, members: &[(String, Value)]) -> Value {
    let mut unknown = false;
    let mut truths = 0usize;
    let mut falsehoods = 0usize;
    for (_, value) in members {
        match value {
            Value::Boolean(true) => truths += 1,
            Value::Boolean(false) => falsehoods += 1,
            _ => unknown = true,
        }
    }
    match quantifier {
        // Empty ALL is vacuously TRUE; one FALSE decides it whatever else is
        // unknown.
        Quantifier::All => {
            if falsehoods > 0 {
                Value::Boolean(false)
            } else if unknown {
                Value::Unknown
            } else {
                Value::Boolean(true)
            }
        }
        // One TRUE decides ANY whatever else is unknown.
        Quantifier::Any => {
            if truths > 0 {
                Value::Boolean(true)
            } else if unknown {
                Value::Unknown
            } else {
                Value::Boolean(false)
            }
        }
        // NONE is the negation of ANY: one TRUE member decides it FALSE.
        Quantifier::None => {
            if truths > 0 {
                Value::Boolean(false)
            } else if unknown {
                Value::Unknown
            } else {
                Value::Boolean(true)
            }
        }
    }
}

/// Select the first applicable `FAILURE` clause whose `WHEN` is TRUE.
///
/// > evaluate applicable FAILURE clauses in source declaration order and select
/// > the first whose WHEN is TRUE. FALSE does not select a clause. MISSING or
/// > UNKNOWN is a required-condition failure using error.required.missing or
/// > error.value.unknown and follows ordinary diagnostic handling before a
/// > later clause is considered.
fn select_failure(
    engine: &mut Engine,
    _checks: &Checks,
    _success: Option<&SuccessOutcome>,
) -> Option<SelectedFailure> {
    let root = engine.root_source();
    let candidates: Vec<usize> = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .enumerate()
        .filter(|(_, d)| d.block == "FAILURE" && d.source == root)
        .map(|(index, _)| index)
        .collect();

    for declaration in candidates {
        let (id, source, span) = {
            let decl = engine.resolved.declarations().get(declaration)?;
            (decl.id.qualified(), decl.source.clone(), decl.id_span)
        };
        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration)
        else {
            continue;
        };
        let when = lcl_runtime::syntax::field_expr(&block, "WHEN").cloned();
        let written_status = lcl_runtime::syntax::field_text(&block, "STATUS");
        let classification = lcl_runtime::syntax::field_text(&block, "ERROR");
        let evidence: Vec<String> = block
            .field("EVIDENCE")
            .map(|f| {
                syntax::reference_list(&f.body)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default();

        let Some(when) = when else {
            // `FAILURE` requires `WHEN`; M2 rejects a document without one.
            continue;
        };
        let phase = engine.observed_phase();
        match engine.evaluator().demand(&when) {
            Ok(Value::Boolean(true)) => {
                let Some(written_status) = written_status else {
                    continue;
                };
                let requested = crate::terminal::resolve_status_alias(engine, &written_status);
                return Some(SelectedFailure {
                    id,
                    source,
                    span,
                    requested_status: requested,
                    written_status,
                    classification,
                    evidence,
                });
            }
            // "FALSE does not select a clause."
            Ok(Value::Boolean(false)) => continue,
            Ok(Value::Missing) => {
                engine.emit(Emission {
                    id: CompletionError::RequiredMissing,
                    source: &source,
                    span: when.span(),
                    declaration: Some(id.clone()),
                    cause: "WHEN".to_string(),
                    detail: format!("FAILURE `{id}` demanded a MISSING condition"),
                    phase,
                });
                continue;
            }
            Ok(Value::Unknown) => {
                engine.emit(Emission {
                    id: CompletionError::ValueUnknown,
                    source: &source,
                    span: when.span(),
                    declaration: Some(id.clone()),
                    cause: "WHEN".to_string(),
                    detail: format!("FAILURE `{id}` demanded an UNKNOWN condition"),
                    phase,
                });
                continue;
            }
            Ok(_) | Err(_) => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn members(values: &[Value]) -> Vec<(String, Value)> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("m{i}"), v.clone()))
            .collect()
    }

    #[test]
    fn all_is_false_when_any_member_is_false_even_beside_unknown() {
        let m = members(&[Value::Boolean(false), Value::Unknown]);
        assert_eq!(quantify(Quantifier::All, &m), Value::Boolean(false));
    }

    #[test]
    fn all_is_unknown_when_no_member_is_false_and_one_is_unknown() {
        let m = members(&[Value::Boolean(true), Value::Unknown]);
        assert_eq!(quantify(Quantifier::All, &m), Value::Unknown);
    }

    #[test]
    fn a_missing_member_cannot_satisfy_all() {
        let m = members(&[Value::Boolean(true), Value::Missing]);
        assert_eq!(quantify(Quantifier::All, &m), Value::Unknown);
    }

    #[test]
    fn any_is_true_on_one_true_member() {
        let m = members(&[Value::Boolean(false), Value::Boolean(true)]);
        assert_eq!(quantify(Quantifier::Any, &m), Value::Boolean(true));
    }

    #[test]
    fn none_is_the_negation_of_any() {
        let m = members(&[Value::Boolean(false), Value::Boolean(false)]);
        assert_eq!(quantify(Quantifier::None, &m), Value::Boolean(true));
        let m = members(&[Value::Boolean(true)]);
        assert_eq!(quantify(Quantifier::None, &m), Value::Boolean(false));
    }
}
