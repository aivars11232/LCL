//! Syntax accessors this layer needs and the layers below do not export.
//!
//! Everything here is a *reader* over `lcl_parser::syntax`: it extracts what a
//! field literally says. None of it decides language meaning, so it is a
//! narrower thing than the semantics each layer owns, and reading a `REF(...)`
//! argument here is not a second implementation of reference resolution — that
//! already happened in M3, and this only re-reads the identifier M3 bound.
//!
//! [`lcl_runtime::syntax`] already exports block lookup, field lookup,
//! `field_expr`, `field_text` and expression rendering. This module adds only
//! the three accessors it does not: reference targets, reference lists and
//! inline collections.

use lcl_lexer::Span;
use lcl_parser::syntax::{Body, Collection, Expr, Value};

/// The expression inline in one field body, if the body is inline.
pub fn inline_expr(body: &Body) -> Option<&Expr> {
    match body {
        Body::Inline(Value::Expression(expr)) => Some(expr),
        _ => None,
    }
}

/// The collection inline in one field body, written inline or as a
/// `MULTILINE_COLLECTION`.
pub fn inline_collection(body: &Body) -> Option<&Collection> {
    match body {
        Body::Inline(Value::MultilineCollection(c)) => Some(c),
        Body::Inline(Value::Expression(Expr::Collection(c))) => Some(c),
        _ => None,
    }
}

/// The declaration id one `REF(...)` expression names.
///
/// `None` for anything that is not a reference call, so a material value can
/// never be mistaken for an identity. That distinction is load-bearing in
/// `check_selection_contract`, which treats a reference target and a material
/// target as different selection reasons.
pub fn reference_target(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call(call) if call.is_reference() => match call.arguments.first()? {
            Expr::Identifier(ident) => Some(&ident.text),
            _ => None,
        },
        Expr::Group(group) => reference_target(&group.inner),
        _ => None,
    }
}

/// Every declaration id a reference field names, in left-to-right order.
///
/// `block_schemas#/execution_graph_contract/child_order`: "each reference LIST
/// expands left to right." That order is normative, so it is preserved rather
/// than sorted.
pub fn reference_list(body: &Body) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    if let Some(collection) = inline_collection(body) {
        for member in &collection.members {
            if let Some(id) = reference_target(member) {
                out.push((id.to_string(), member.span()));
            }
        }
        return out;
    }
    if let Some(expr) = inline_expr(body) {
        if let Some(id) = reference_target(expr) {
            out.push((id.to_string(), expr.span()));
        }
    }
    out
}

/// Every declaration id an expression references, at any depth.
///
/// Iterative, not recursive: a deeply nested expression costs heap, never
/// stack.
pub fn referenced_ids(expr: &Expr) -> Vec<String> {
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

/// The text of a literal, identifier or grouped literal.
pub fn literal_text(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.text.clone()),
        Expr::Literal(literal) => Some(literal.text.clone()),
        Expr::Group(group) => literal_text(&group.inner),
        _ => None,
    }
}
