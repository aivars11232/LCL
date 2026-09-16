//! A `TEST` root's comparison.
//!
//! `check_selection_contract/demand`:
//!
//! > A TEST root executes only its explicit TASK or ACTION, if present, then
//! > evaluates ASSERT or EXPECTED/ACTUAL.
//!
//! and `root_success`:
//!
//! > A TEST root evaluates ASSERT, or strict EXPECTED/ACTUAL equality when
//! > ASSERT is absent; if both are present they must both hold.
//!
//! ## The execution half already happened
//!
//! "executes only its explicit TASK or ACTION" is not this milestone's work to
//! do again. `block_schemas#/execution_graph_contract` makes `TEST`'s `TASK`
//! and `ACTION` fields execution-bearing, so M3 puts them in the candidate
//! graph as the root's children and M6 runs them in step 10. By the time
//! completion sees the `TEST`, its referenced graph has already executed and
//! bound whatever outputs it binds.
//!
//! So this module does exactly the second half: it demands the comparison over
//! the values that execution produced. That is also why this crate needs no
//! operation dispatcher and reaches no host — running the graph would have been
//! the only reason to, and step 10 owns it.
//!
//! ## Both forms, when both are written
//!
//! The schema requires "At least ASSERT or EXPECTED+ACTUAL", not exactly one.
//! When a document writes both, both must hold — so this returns their
//! conjunction rather than preferring one, and an UNKNOWN on either side makes
//! the conjunction UNKNOWN unless the other side is already FALSE.

use crate::check::{demand_assertion, report_demand_fault, Selected};
use crate::engine::Engine;
use lcl_runtime::{strict_equal, Value};

/// Evaluate one `TEST` root's declared comparison.
///
/// Returns the Boolean domain outcome and whether a diagnostic was already
/// emitted for its cause, exactly as a `VERIFY` assertion does.
pub(crate) fn outcome(engine: &mut Engine, check: &Selected) -> (Value, bool) {
    let id = check.id.as_str();
    let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, check.declaration)
    else {
        return (Value::Unknown, false);
    };
    let assertion = lcl_runtime::syntax::field_expr(&block, "ASSERT").cloned();
    let expected = lcl_runtime::syntax::field_expr(&block, "EXPECTED").cloned();
    let actual = lcl_runtime::syntax::field_expr(&block, "ACTUAL").cloned();

    // A MISSING or non-Boolean assertion cannot establish a pass. It is not
    // fabricated into FALSE either: the domain outcome is simply not
    // establishable. MISSING and an evaluator fault are reported at their own
    // registered identifiers.
    let mut reported = false;
    let asserted = assertion.map(|expr| {
        let (value, emitted) = demand_assertion(engine, &expr, check, "ASSERT");
        reported |= emitted;
        match value {
            Value::Missing => Value::Unknown,
            other => other,
        }
    });

    let compared = match (expected, actual) {
        (Some(expected), Some(actual)) => {
            let mut demand = |expr: &lcl_parser::syntax::Expr, field: &str| {
                let demanded = engine.evaluator().demand(expr);
                if let Err(fault) = &demanded {
                    let kind = check.kind.to_string();
                    reported |= report_demand_fault(engine, fault, &check.source, &kind, id, field);
                }
                demanded
            };
            let expected = demand(&expected, "EXPECTED");
            let actual = demand(&actual, "ACTUAL");
            Some(match (expected, actual) {
                (Ok(expected), Ok(actual)) => {
                    // "Expected-and-actual form always uses the registered ==
                    // strict-equality operator."
                    if matches!(expected, Value::Unknown) || matches!(actual, Value::Unknown) {
                        Value::Unknown
                    } else {
                        Value::Boolean(strict_equal(&expected, &actual))
                    }
                }
                _ => Value::Unknown,
            })
        }
        // One half of the comparison form without the other is a schema defect
        // M2 already rejects, so it cannot reach here with a document that
        // parsed. Treat it as unestablishable rather than inventing a verdict.
        (Some(_), None) | (None, Some(_)) => Some(Value::Unknown),
        (None, None) => None,
    };

    let value = match (asserted, compared) {
        (Some(asserted), None) => asserted,
        (None, Some(compared)) => compared,
        // "if both are present they must both hold."
        (Some(asserted), Some(compared)) => conjunction(asserted, compared),
        (None, None) => {
            debug_assert!(
                false,
                "{id} parsed without ASSERT or EXPECTED/ACTUAL, which the block schema forbids"
            );
            Value::Unknown
        }
    };
    (value, reported)
}

/// Three-valued AND: FALSE dominates, then UNKNOWN.
///
/// `12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS` fixes this table, and it
/// matters here because a FALSE half is a decided failure even when the other
/// half cannot be established.
fn conjunction(left: Value, right: Value) -> Value {
    match (&left, &right) {
        (Value::Boolean(false), _) | (_, Value::Boolean(false)) => Value::Boolean(false),
        (Value::Boolean(true), Value::Boolean(true)) => Value::Boolean(true),
        _ => Value::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn false_dominates_unknown_in_the_conjunction() {
        assert_eq!(
            conjunction(Value::Boolean(false), Value::Unknown),
            Value::Boolean(false)
        );
        assert_eq!(
            conjunction(Value::Unknown, Value::Boolean(false)),
            Value::Boolean(false)
        );
    }

    #[test]
    fn unknown_blocks_a_true_conjunction() {
        assert_eq!(
            conjunction(Value::Boolean(true), Value::Unknown),
            Value::Unknown
        );
        assert_eq!(
            conjunction(Value::Boolean(true), Value::Boolean(true)),
            Value::Boolean(true)
        );
    }
}
