//! Reading exact inline field values out of the syntax tree.
//!
//! The grammar stage has already checked that each of these fields carries a
//! value of the right *shape*; these readers take the value the parser
//! recorded, with its exact span, and never reinterpret or normalize it.

use lcl_lexer::Span;
use lcl_parser::syntax::{Block, Expr, LiteralKind, Statement, Value};

/// The inline value expression of one direct field, with its span.
pub(crate) fn expression<'a>(block: &'a Block, key: &str) -> Option<&'a Expr> {
    statement_expression(&block.body, key)
}

pub(crate) fn statement_expression<'a>(statements: &'a [Statement], key: &str) -> Option<&'a Expr> {
    statements.iter().find_map(|s| match s {
        Statement::Field(f) if f.key.text == key => match f.body.as_inline() {
            Some(Value::Expression(e)) => Some(e),
            _ => None,
        },
        _ => None,
    })
}

/// The span of one direct field's body, whatever its form.
pub(crate) fn body_span(block: &Block, key: &str) -> Option<Span> {
    block.body.iter().find_map(|s| match s {
        Statement::Field(f) if f.key.text == key => Some(f.body.span()),
        _ => None,
    })
}

/// The locus a diagnostic about an absent field should use: the block header.
pub(crate) fn field_or_header_span(block: &Block, key: &str) -> Span {
    body_span(block, key).unwrap_or(block.key.span)
}

/// A lowercase identifier value, e.g. an `ID` or a `NAMESPACE`.
pub(crate) fn identifier(block: &Block, key: &str) -> Option<(String, Span)> {
    statement_identifier(&block.body, key)
}

pub(crate) fn statement_identifier(statements: &[Statement], key: &str) -> Option<(String, Span)> {
    match statement_expression(statements, key) {
        Some(Expr::Identifier(ident)) => Some((ident.text.clone(), ident.span)),
        _ => None,
    }
}

/// A single-line `STRING` value, already decoded by the lexer.
pub(crate) fn string(block: &Block, key: &str) -> Option<(String, Span)> {
    match statement_expression(&block.body, key) {
        Some(Expr::Literal(l)) if l.kind == LiteralKind::String => Some((l.text.clone(), l.span)),
        _ => None,
    }
}

/// A `TRUE` or `FALSE` value.
pub(crate) fn boolean(block: &Block, key: &str) -> Option<bool> {
    match statement_expression(&block.body, key) {
        Some(Expr::Literal(l)) if l.kind == LiteralKind::True => Some(true),
        Some(Expr::Literal(l)) if l.kind == LiteralKind::False => Some(false),
        _ => None,
    }
}

/// A one-argument constructor call, e.g. `PATH("x")`, returning the callable
/// name and its single `STRING` argument.
pub(crate) fn constructor_string(expr: &Expr) -> Option<(&str, &str)> {
    let Expr::Call(call) = expr else {
        return None;
    };
    if call.arguments.len() != 1 {
        return None;
    }
    match &call.arguments[0] {
        Expr::Literal(l) if l.kind == LiteralKind::String => {
            Some((call.callable.text.as_str(), l.text.as_str()))
        }
        _ => None,
    }
}
