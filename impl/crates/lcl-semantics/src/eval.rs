//! The bounded pure demand evaluator.
//!
//! Phase D fills in evaluation. What exists here now is the one piece the
//! authority model needs first: a canonical rendering of a written expression,
//! used to compare two clauses' declared targets.

use lcl_parser::syntax::Expr;

/// Render one expression exactly as its structure declares it.
///
/// Used to compare declared targets across clauses. It is a *rendering*, not an
/// evaluation: `PATH("/a")` renders as `PATH("/a")` and never as a resolved
/// filesystem path, so two clauses match only when they wrote the same thing.
/// Matching by resolved meaning is a separate question that
/// `types_v0.1.0.json#/material_identity_contract` governs, and guessing it
/// here would silently widen a `FORBID`.
pub(crate) fn render_static(expr: &Expr) -> String {
    match expr {
        Expr::Literal(literal) => match literal.kind {
            lcl_parser::syntax::LiteralKind::String
            | lcl_parser::syntax::LiteralKind::MultilineString => format!("{:?}", literal.text),
            _ => literal.text.clone(),
        },
        Expr::Identifier(ident) => ident.text.clone(),
        Expr::Call(call) => {
            let arguments: Vec<String> = call.arguments.iter().map(render_static).collect();
            format!("{}({})", call.callable.text, arguments.join(", "))
        }
        Expr::Collection(collection) => {
            let members: Vec<String> = collection.members.iter().map(render_static).collect();
            format!("[{}]", members.join(", "))
        }
        Expr::Group(group) => format!("({})", render_static(&group.inner)),
        Expr::Unary(unary) => format!(
            "{}{}",
            unary.operator.lexeme(),
            render_static(&unary.operand)
        ),
        Expr::Binary(binary) => format!(
            "{} {} {}",
            render_static(&binary.left),
            binary.operator.lexeme(),
            render_static(&binary.right)
        ),
        Expr::Property(property) => {
            format!("{}.{}", render_static(&property.base), property.name)
        }
        Expr::Index(index) => format!(
            "{}[{}]",
            render_static(&index.base),
            render_static(&index.index)
        ),
        Expr::Type(ty) => render_type(ty),
    }
}

/// Render one type expression as written.
fn render_type(ty: &lcl_parser::syntax::TypeExpr) -> String {
    use lcl_parser::syntax::TypeExpr;
    match ty {
        TypeExpr::Scalar(word) => word.text.clone(),
        TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
            format!("{}[{}]", b.word.text, render_static(&b.argument))
        }
    }
}

// ---------------------------------------------------------------------------
// The bounded pure demand evaluator
// ---------------------------------------------------------------------------

use crate::engine::Engine;
use crate::value::Value;
use lcl_checker::numeric::Decimal;
use lcl_checker::ty::UnitId;
use lcl_parser::syntax::{BinaryOp, LiteralKind, UnaryOp};
use lcl_resolver::SourceId;

/// The evaluator's own budget, so a pathological document costs time, never
/// termination. Depth is counted rather than trusted to the stack.
const MAX_DEPTH: usize = 128;

/// Evaluate one expression at a pre-effect demand point.
///
/// Returns `None` for an expression whose value this layer cannot establish
/// without performing or observing an effect. `None` is **not** `MISSING`:
/// `MISSING` is a value the language defines and this layer may report, while
/// `None` means "not decidable here", and the caller must leave the obligation
/// to the layer that demands it rather than inventing an outcome.
///
/// ## Purity
///
/// Every path through this function reads only: literals in the source, the
/// resolved values of declarations, and the registered operator and function
/// tables. It calls no capability, starts no producer, and reads no `OUTPUT`
/// binding — an unbound `OUTPUT` read yields `MISSING`, exactly as
/// `05_SEMANTICS/05` requires, and never activates its producer.
pub(crate) fn literal_value(engine: &Engine, source: &SourceId, expr: &Expr) -> Option<Value> {
    evaluate(engine, source, expr, 0)
}

fn evaluate(engine: &Engine, source: &SourceId, expr: &Expr, depth: usize) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    match expr {
        Expr::Literal(literal) => literal_of(literal),
        Expr::Group(group) => evaluate(engine, source, &group.inner, depth + 1),
        Expr::Identifier(ident) => {
            // A bare qualified identifier in value position is a registered
            // identifier, e.g. `unit.second` or `mode.sequential`. It is a
            // registered name, not a material value.
            Some(Value::Identifier(ident.text.clone()))
        }
        Expr::Collection(collection) => {
            let mut members = Vec::new();
            for member in &collection.members {
                members.push(evaluate(engine, source, member, depth + 1)?);
            }
            Some(Value::List(members))
        }
        Expr::Unary(unary) => {
            let operand = evaluate(engine, source, &unary.operand, depth + 1)?;
            fold_unary(unary.operator, &operand)
        }
        Expr::Binary(binary) => fold_binary(engine, source, binary, depth),
        Expr::Call(call) => call_value(engine, source, call, depth),
        // A property or index access over a resolved value is a demand this
        // layer does not answer: the receiver's binding may not exist before
        // effects, and guessing one would read an OUTPUT early.
        Expr::Property(_) | Expr::Index(_) | Expr::Type(_) => None,
    }
}

fn literal_of(literal: &lcl_parser::syntax::Literal) -> Option<Value> {
    Some(match literal.kind {
        LiteralKind::String | LiteralKind::MultilineString => Value::Text(literal.text.clone()),
        LiteralKind::Integer => Value::Integer(Decimal::parse_integer(&literal.text)?),
        LiteralKind::Decimal => Value::Decimal(Decimal::parse_decimal(&literal.text)?),
        LiteralKind::True => Value::Boolean(true),
        LiteralKind::False => Value::Boolean(false),
        LiteralKind::Null => Value::Null,
        LiteralKind::Missing => Value::Missing,
        LiteralKind::Unknown => Value::Unknown,
    })
}

/// `REF(x)` and the registered constructors and functions this layer can
/// evaluate purely.
fn call_value(
    engine: &Engine,
    source: &SourceId,
    call: &lcl_parser::syntax::Call,
    depth: usize,
) -> Option<Value> {
    if call.is_reference() {
        let target = match call.arguments.first()? {
            Expr::Identifier(ident) => ident.text.as_str(),
            _ => return None,
        };
        return reference_value(engine, target);
    }

    let name = call.callable.text.as_str();
    let mut arguments = Vec::new();
    for argument in &call.arguments {
        arguments.push(evaluate(engine, source, argument, depth + 1)?);
    }

    // A registered typed constructor over literal arguments has a value this
    // layer knows exactly. The constructor set is the registry's, so an
    // unregistered callable is never folded.
    if engine.contracts.statics().constructor(name).is_some() {
        return constructor_value(name, &arguments);
    }

    // A registered pure function over known arguments.
    if engine.contracts.statics().function(name).is_some() {
        return function_value(name, &arguments);
    }

    None
}

/// The resolved value of one declaration reference.
fn reference_value(engine: &Engine, id: &str) -> Option<Value> {
    if let Some(resolution) = engine.plan.resolutions.iter().find(|r| r.id == id) {
        return Some(resolution.value.clone());
    }
    // An `OUTPUT` that no producer has bound reads as `MISSING`, and reading it
    // does not start its producer. `05_SEMANTICS/05`: "Within a valid instance
    // an output not yet bound yields MISSING."
    let declaration = engine
        .resolved
        .declarations()
        .all()
        .iter()
        .find(|d| d.id.qualified() == id)?;
    if declaration.block == "OUTPUT" {
        return Some(Value::Missing);
    }
    None
}

fn constructor_value(name: &str, arguments: &[Value]) -> Option<Value> {
    match (name, arguments) {
        ("MEASURE" | "DURATION", [value, unit]) => {
            let number = value.number()?.clone();
            let unit = match unit {
                Value::Identifier(id) => UnitId(id.clone()),
                _ => return None,
            };
            Some(Value::Quantity(number, unit))
        }
        ("PERCENTAGE", [value]) => Some(Value::Percentage(value.number()?.clone())),
        ("BYTES", [value]) => Some(Value::Bytes(value.number()?.clone())),
        (_, [Value::Text(text)]) => Some(Value::Constructed {
            constructor: name.to_string(),
            text: text.clone(),
        }),
        _ => None,
    }
}

/// The registered pure functions this layer evaluates.
///
/// Deliberately a short list: a function whose result depends on anything other
/// than its supplied arguments is not decidable before effects, and a function
/// this layer does not know returns `None` rather than a guess.
fn function_value(name: &str, arguments: &[Value]) -> Option<Value> {
    match (name, arguments) {
        ("COUNT", [collection]) => {
            let members = collection.members()?;
            Some(Value::Integer(Decimal::from_integer(
                lcl_checker::numeric::Integer::from_u64(members.len() as u64),
            )))
        }
        ("EMPTY", [collection]) => match collection {
            Value::List(items) | Value::Set(items) => Some(Value::Boolean(items.is_empty())),
            Value::Text(text) => Some(Value::Boolean(text.is_empty())),
            _ => None,
        },
        // `EXISTS` asks whether a value is material. It is answerable here for
        // the two non-material sentinels and for a known material value; for
        // anything whose existence depends on an effect it is not.
        ("EXISTS", [value]) => match value {
            Value::Missing => Some(Value::Boolean(false)),
            Value::Unknown => None,
            other => Some(Value::Boolean(other.is_material())),
        },
        _ => None,
    }
}

fn fold_unary(operator: UnaryOp, operand: &Value) -> Option<Value> {
    match operator {
        UnaryOp::Not => match operand {
            Value::Boolean(value) => Some(Value::Boolean(!value)),
            // `unknown_logic`: `NOT UNKNOWN` is `UNKNOWN`.
            Value::Unknown => Some(Value::Unknown),
            _ => None,
        },
        UnaryOp::Negate => {
            let number = operand.number()?;
            Some(match operand {
                Value::Integer(_) => Value::Integer(number.negated()),
                Value::Decimal(_) => Value::Decimal(number.negated()),
                _ => return None,
            })
        }
    }
}

fn fold_binary(
    engine: &Engine,
    source: &SourceId,
    binary: &lcl_parser::syntax::Binary,
    depth: usize,
) -> Option<Value> {
    // `01_FOUNDATION/03`: "A skipped Boolean operand is not demanded." So the
    // short-circuiting operators are evaluated left first, and the right
    // operand is demanded only when the left cannot decide the result.
    if matches!(binary.operator, BinaryOp::And | BinaryOp::Or) {
        let left = evaluate(engine, source, &binary.left, depth + 1)?;
        match (binary.operator, &left) {
            (BinaryOp::And, Value::Boolean(false)) => return Some(Value::Boolean(false)),
            (BinaryOp::Or, Value::Boolean(true)) => return Some(Value::Boolean(true)),
            _ => {}
        }
        let right = evaluate(engine, source, &binary.right, depth + 1)?;
        return logic(engine, binary.operator, &left, &right);
    }

    let left = evaluate(engine, source, &binary.left, depth + 1)?;
    let right = evaluate(engine, source, &binary.right, depth + 1)?;

    match binary.operator {
        BinaryOp::Equal => equality(&left, &right).map(Value::Boolean),
        BinaryOp::NotEqual => equality(&left, &right).map(|equal| Value::Boolean(!equal)),
        BinaryOp::Less | BinaryOp::LessOrEqual | BinaryOp::Greater | BinaryOp::GreaterOrEqual => {
            compare(binary.operator, &left, &right)
        }
        BinaryOp::In => match &right {
            Value::List(items) | Value::Set(items) => Some(Value::Boolean(
                items.iter().any(|item| equality(&left, item) == Some(true)),
            )),
            _ => None,
        },
        BinaryOp::Contains => match &left {
            Value::List(items) | Value::Set(items) => Some(Value::Boolean(
                items
                    .iter()
                    .any(|item| equality(item, &right) == Some(true)),
            )),
            Value::Text(haystack) => right
                .text()
                .map(|needle| Value::Boolean(haystack.contains(needle))),
            _ => None,
        },
        BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
            arithmetic(binary.operator, &left, &right)
        }
        // Division has a value-domain contract of its own — zero denominators
        // and non-terminating quotients — which M4 owns statically and M6 owns
        // at demand. This layer does not re-decide it.
        BinaryOp::Divide => None,
        // `MATCHES` needs the closed pattern profile, which the standard
        // library layer owns.
        BinaryOp::Matches => None,
        BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
    }
}

/// Three-valued `AND` and `OR`, from the registered `unknown_logic` table.
///
/// The table is read from the registry, never written here: a change to the
/// canonical truth table changes this behaviour without a code change.
fn logic(engine: &Engine, operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    if let (Value::Boolean(a), Value::Boolean(b)) = (left, right) {
        return Some(Value::Boolean(match operator {
            BinaryOp::And => *a && *b,
            BinaryOp::Or => *a || *b,
            _ => return None,
        }));
    }
    let key = format!(
        "{} {} {}",
        render_operand(left)?,
        operator.lexeme(),
        render_operand(right)?
    );
    match engine.contracts.statics().unknown_logic(&key)? {
        "TRUE" => Some(Value::Boolean(true)),
        "FALSE" => Some(Value::Boolean(false)),
        "UNKNOWN" => Some(Value::Unknown),
        _ => None,
    }
}

fn render_operand(value: &Value) -> Option<&'static str> {
    match value {
        Value::Boolean(true) => Some("TRUE"),
        Value::Boolean(false) => Some("FALSE"),
        Value::Unknown => Some("UNKNOWN"),
        Value::Missing => Some("MISSING"),
        _ => None,
    }
}

/// Exact equality between two material values.
///
/// `None` when either side is non-material: comparing `MISSING` for equality is
/// a value-domain question the demanding layer answers, and answering `false`
/// here would assert that an absent value is known to differ.
fn equality(left: &Value, right: &Value) -> Option<bool> {
    if !left.is_material() || !right.is_material() {
        return None;
    }
    match (left, right) {
        (Value::Boolean(a), Value::Boolean(b)) => Some(a == b),
        (Value::Text(a), Value::Text(b)) => Some(a == b),
        (Value::Identifier(a), Value::Identifier(b)) => Some(a == b),
        (Value::Null, Value::Null) => Some(true),
        (Value::Null, _) | (_, Value::Null) => Some(false),
        (
            Value::Constructed {
                constructor: ca,
                text: ta,
            },
            Value::Constructed {
                constructor: cb,
                text: tb,
            },
        ) => Some(ca == cb && ta == tb),
        (Value::Quantity(a, ua), Value::Quantity(b, ub)) => {
            // Different units are not compared here: the exact unit rule is
            // `error.numeric.unit_mismatch`, which is a diagnostic, not a
            // Boolean, and it belongs to the layer that demands the value.
            if ua != ub {
                return None;
            }
            Some(a.compare(b) == std::cmp::Ordering::Equal)
        }
        (Value::List(a), Value::List(b)) | (Value::Set(a), Value::Set(b)) => {
            if a.len() != b.len() {
                return Some(false);
            }
            let mut all = true;
            for (x, y) in a.iter().zip(b.iter()) {
                match equality(x, y) {
                    Some(true) => {}
                    Some(false) => all = false,
                    None => return None,
                }
            }
            Some(all)
        }
        _ => {
            let (Some(a), Some(b)) = (left.number(), right.number()) else {
                return Some(false);
            };
            Some(a.compare(b) == std::cmp::Ordering::Equal)
        }
    }
}

fn compare(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    let (a, b) = (left.number()?, right.number()?);
    if let (Value::Quantity(_, ua), Value::Quantity(_, ub)) = (left, right) {
        if ua != ub {
            return None;
        }
    }
    let ordering = a.compare(b);
    Some(Value::Boolean(match operator {
        BinaryOp::Less => ordering == std::cmp::Ordering::Less,
        BinaryOp::LessOrEqual => ordering != std::cmp::Ordering::Greater,
        BinaryOp::Greater => ordering == std::cmp::Ordering::Greater,
        BinaryOp::GreaterOrEqual => ordering != std::cmp::Ordering::Less,
        _ => return None,
    }))
}

fn arithmetic(operator: BinaryOp, left: &Value, right: &Value) -> Option<Value> {
    let (a, b) = (left.number()?, right.number()?);
    let result = match operator {
        BinaryOp::Add => a.add(b),
        BinaryOp::Subtract => a.sub(b),
        BinaryOp::Multiply => a.mul(b),
        _ => return None,
    };
    Some(match (left, right) {
        (Value::Integer(_), Value::Integer(_)) => Value::Integer(result),
        _ => Value::Decimal(result),
    })
}
