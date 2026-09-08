//! The registered built-in pure functions.
//!
//! Authority: `06_STANDARD_LIBRARY/04_BUILT_IN_FUNCTIONS.txt`,
//! `operators_and_functions_v0.1.0.json#/functions` and the function rules in
//! `05_SEMANTICS/12`.
//!
//! Every function here is pure: it reads its arguments and nothing else. None
//! touches a binding, a capability or the host. `05_SEMANTICS/12` puts them all
//! under one evaluation rule — "All other pure functions and operators are
//! eager" — so with the single exception of the quantifiers' immediate
//! sequence, which `crate::eval` handles before arguments are evaluated, an
//! argument list arrives here already evaluated.

use crate::diagnostic::RuntimeError;
use crate::eval::{count_of, reduce_quantifier, Demand, Evaluator, Fault};
use crate::order_profile;
use crate::value::Value;
use lcl_checker::numeric::Decimal;
use lcl_lexer::Span;
use std::cmp::Ordering;

/// Apply one registered function to its evaluated arguments.
pub(crate) fn apply(evaluator: &Evaluator, name: &str, arguments: &[Value], span: Span) -> Demand {
    // `EXISTS` is one of the three forms that consume MISSING without error;
    // every other function rejects it.
    if name != "EXISTS" {
        if let Some(position) = arguments.iter().position(|a| a == &Value::Missing) {
            let _ = position;
            return Err(evaluator.missing(span, "function argument"));
        }
    }

    match name {
        "EXISTS" => exists(arguments, span, evaluator),
        "COUNT" => count(arguments, span, evaluator),
        "EMPTY" => empty(arguments, span, evaluator),
        "SUM" => reduce(evaluator, "SUM", arguments, span),
        "MIN" => reduce(evaluator, "MIN", arguments, span),
        "MAX" => reduce(evaluator, "MAX", arguments, span),
        "ALL" | "ANY" | "NONE" => quantifier(evaluator, name, arguments, span),
        other => Err(evaluator.operand(span, format!("{other} is not an implemented function"))),
    }
}

/// `EXISTS` asks whether a value is material.
///
/// It is one of the three forms exempt from the MISSING rule, so it answers
/// `FALSE` for MISSING rather than raising.
fn exists(arguments: &[Value], span: Span, evaluator: &Evaluator) -> Demand {
    let [value] = arguments else {
        return Err(evaluator.operand(span, "EXISTS takes exactly one argument"));
    };
    Ok(match value {
        Value::Missing => Value::Boolean(false),
        // An UNKNOWN value exists; only its content is undetermined.
        Value::Unknown => Value::Boolean(true),
        other => Value::Boolean(other.is_material()),
    })
}

fn count(arguments: &[Value], span: Span, evaluator: &Evaluator) -> Demand {
    let [value] = arguments else {
        return Err(evaluator.operand(span, "COUNT takes exactly one argument"));
    };
    if value == &Value::Unknown {
        return Ok(Value::Unknown);
    }
    match count_of(value) {
        Some(count) => Ok(Value::Integer(count)),
        None => Err(evaluator.operand(
            span,
            format!("COUNT is not registered for {}", value.family()),
        )),
    }
}

/// `EMPTY` is "TRUE exactly when COUNT is zero; NULL is not an accepted
/// family".
fn empty(arguments: &[Value], span: Span, evaluator: &Evaluator) -> Demand {
    let [value] = arguments else {
        return Err(evaluator.operand(span, "EMPTY takes exactly one argument"));
    };
    if value == &Value::Unknown {
        return Ok(Value::Unknown);
    }
    if value == &Value::Null {
        return Err(evaluator.operand(span, "NULL is not an accepted EMPTY family"));
    }
    match count_of(value) {
        Some(count) => Ok(Value::Boolean(count.is_zero())),
        None => Err(evaluator.operand(
            span,
            format!("EMPTY is not registered for {}", value.family()),
        )),
    }
}

/// `SUM`, `MIN` and `MAX`.
///
/// "SUM, MIN, and MAX require nonempty collections and reject a typed empty
/// collection with error.operator.operand."
fn reduce(evaluator: &Evaluator, name: &str, arguments: &[Value], span: Span) -> Demand {
    let [collection] = arguments else {
        return Err(evaluator.operand(span, format!("{name} takes exactly one argument")));
    };
    if collection == &Value::Unknown {
        return Ok(Value::Unknown);
    }
    let Some(members) = collection.members() else {
        return Err(evaluator.operand(
            span,
            format!(
                "{name} requires a collection, found {}",
                collection.family()
            ),
        ));
    };
    if members.is_empty() {
        return Err(Fault::new(
            evaluator.contracts,
            RuntimeError::OperatorOperand,
            span,
            "empty reduction",
            format!("{name} requires a nonempty collection"),
        ));
    }
    // An UNKNOWN member makes the whole reduction UNKNOWN, per the common
    // propagation rule for ordinary pure functions.
    if members.iter().any(|m| m == &Value::Unknown) {
        return Ok(Value::Unknown);
    }
    if let Some(position) = members.iter().position(|m| m == &Value::Missing) {
        let _ = position;
        return Err(evaluator.missing(span, "reduction member"));
    }

    match name {
        "SUM" => sum(evaluator, members, span),
        // "MIN/MAX use the registered total order. SET source order never
        // changes a reduction."
        "MIN" => extremum(evaluator, members, span, Ordering::Less),
        "MAX" => extremum(evaluator, members, span, Ordering::Greater),
        other => Err(evaluator.operand(span, format!("{other} is not a reduction"))),
    }
}

/// "SUM returns the exact mathematical sum in the registered member family,
/// with one exact unit for MEASURE; it never invents an empty-sum unit."
fn sum(evaluator: &Evaluator, members: &[Value], span: Span) -> Demand {
    let mut total: Option<Decimal> = None;
    let mut family = members[0].clone();
    for member in members {
        let Some(number) = member.number() else {
            return Err(evaluator.operand(
                span,
                format!("SUM requires numbers, found {}", member.family()),
            ));
        };
        // One exact unit for MEASURE.
        if let (Value::Quantity(_, a), Value::Quantity(_, b)) = (&family, member) {
            if a != b {
                return Err(Fault::new(
                    evaluator.contracts,
                    RuntimeError::NumericUnitMismatch,
                    span,
                    "unit",
                    format!("SUM requires one exact unit; found {} and {}", a.0, b.0),
                ));
            }
        }
        total = Some(match total {
            None => number.clone(),
            Some(running) => running.add(number),
        });
        // A DECIMAL anywhere promotes the family; otherwise the first member's
        // family is the result family.
        if matches!(member, Value::Decimal(_)) && matches!(family, Value::Integer(_)) {
            family = member.clone();
        }
    }
    let total =
        total.unwrap_or_else(|| Decimal::from_integer(lcl_checker::numeric::Integer::zero()));
    Ok(match family {
        Value::Integer(_) => Value::Integer(total),
        Value::Decimal(_) => Value::Decimal(total),
        Value::Percentage(_) => Value::Percentage(total),
        Value::Bytes(_) => Value::Bytes(total),
        Value::Quantity(_, unit) => Value::Quantity(total, unit),
        other => {
            return Err(evaluator.operand(
                span,
                format!("SUM is not registered for {}", other.family()),
            ))
        }
    })
}

fn extremum(evaluator: &Evaluator, members: &[Value], span: Span, wanted: Ordering) -> Demand {
    let mut best = members[0].clone();
    for member in &members[1..] {
        // "MEASURE values are order-compatible only with the same exact UNIT"
        if let (Value::Quantity(_, a), Value::Quantity(_, b)) = (&best, member) {
            if a != b {
                return Err(Fault::new(
                    evaluator.contracts,
                    RuntimeError::NumericUnitMismatch,
                    span,
                    "unit",
                    format!(
                        "a reduction requires one exact unit; found {} and {}",
                        a.0, b.0
                    ),
                ));
            }
        }
        let Some(ordering) = order_profile::compare(member, &best) else {
            return Err(evaluator.operand(
                span,
                format!(
                    "{} and {} are not mutually order-compatible",
                    best.family(),
                    member.family()
                ),
            ));
        };
        if ordering == wanted {
            best = member.clone();
        }
    }
    Ok(best)
}

/// `ALL`, `ANY` and `NONE` over "one material LIST[BOOLEAN]".
///
/// The immediate bracket sequence form — the only one that admits UNKNOWN
/// members — is handled in `crate::eval` before arguments are evaluated.
fn quantifier(evaluator: &Evaluator, name: &str, arguments: &[Value], span: Span) -> Demand {
    let [argument] = arguments else {
        return Err(evaluator.operand(span, format!("{name} takes exactly one argument")));
    };
    // "Whole-argument UNKNOWN propagates; whole-argument MISSING errors."
    if argument == &Value::Unknown {
        return Ok(Value::Unknown);
    }
    let Some(members) = argument.members() else {
        return Err(evaluator.operand(
            span,
            format!(
                "{name} requires a material LIST[BOOLEAN], found {}",
                argument.family()
            ),
        ));
    };
    let mut observations = Vec::new();
    for member in members {
        match member {
            Value::Boolean(b) => observations.push(Some(*b)),
            // A material LIST cannot hold UNKNOWN, but a defensive branch keeps
            // the reduction total rather than panicking.
            Value::Unknown => observations.push(None),
            Value::Missing => return Err(evaluator.missing(span, "quantifier member")),
            other => {
                return Err(evaluator.operand(
                    span,
                    format!(
                        "a quantifier member must be BOOLEAN, found {}",
                        other.family()
                    ),
                ))
            }
        }
    }
    reduce_quantifier(name, &observations)
        .ok_or_else(|| evaluator.operand(span, format!("{name} is not a quantifier")))
}
