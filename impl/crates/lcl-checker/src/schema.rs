//! `FIELD` and `SCHEMA` declarations, and the object schemas they define.
//!
//! `03_TYPES_AND_VALUES/10`:
//!
//! > A schema is either REF(identifier) to a defined OBJECT type or a local
//! > SCHEMA block containing FIELD blocks. FIELD.NAME is unique. FIELD.TYPE is
//! > exact. … Recursive value schemas are forbidden in Core 0.1.0.
//!
//! ## Identity versus construction
//!
//! `types_v0.1.0.json#/object_type_contract` splits these deliberately:
//! requiredness is "part of type identity", while defaults and constraints
//! "govern construction/validation, not object type identity". [`Schema`]
//! therefore keeps both, and [`Schema::object_type`] projects out exactly the
//! identity half.
//!
//! ## Combined schema
//!
//! "When TYPE already selects an object schema, an additional SCHEMA must have
//! an identical field/type/requiredness map and identical defaults and
//! constraints after alias resolution; otherwise error.object.schema. No
//! implicit merging or overriding of two schemas occurs."

use crate::ty::{ObjectField, ObjectType};
use lcl_lexer::Span;
use lcl_parser::syntax::{Block, Body, Expr, Statement};
use std::collections::BTreeMap;

/// One `FIELD` block as written, before its type is resolved.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldDecl<'a> {
    /// `FIELD.NAME`, a simple identifier.
    pub(crate) name: Option<&'a str>,
    /// `FIELD.TYPE`, an unresolved `TYPE_EXPRESSION`.
    pub(crate) type_expr: Option<&'a Expr>,
    /// `FIELD.REQUIRED`, when the source states it.
    pub(crate) required: Option<bool>,
    /// `FIELD.DEFAULT`, when present.
    pub(crate) default: Option<&'a Expr>,
    /// `FIELD.MINIMUM` / `MAXIMUM` / `TOLERANCE` / `PATTERN`, when present.
    pub(crate) minimum: Option<&'a Expr>,
    pub(crate) maximum: Option<&'a Expr>,
    pub(crate) tolerance: Option<&'a Expr>,
    pub(crate) pattern: Option<&'a Expr>,
    /// The `FIELD` key's own locus.
    pub(crate) span: Span,
}

/// One resolved schema: the identity half plus the construction half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Schema {
    pub(crate) fields: BTreeMap<String, SchemaField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchemaField {
    pub(crate) ty: crate::ty::Type,
    pub(crate) required: bool,
    /// Source text of the declared default, for the combined-schema identity
    /// comparison. Values are not evaluated here.
    pub(crate) default: Option<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) span: Span,
}

impl Schema {
    /// The identity half: the exact field-name/type/requiredness map.
    pub(crate) fn object_type(&self) -> ObjectType {
        ObjectType::new(
            self.fields
                .iter()
                .map(|(name, field)| {
                    (
                        name.clone(),
                        ObjectField {
                            ty: field.ty.clone(),
                            required: field.required,
                        },
                    )
                })
                .collect(),
        )
    }

    /// "identical field/type/requiredness map and identical defaults and
    /// constraints after alias resolution".
    pub(crate) fn identical_to(&self, other: &Schema) -> bool {
        if self.fields.len() != other.fields.len() {
            return false;
        }
        self.fields.iter().all(|(name, field)| {
            other.fields.get(name).is_some_and(|theirs| {
                field.ty == theirs.ty
                    && field.required == theirs.required
                    && field.default == theirs.default
                    && field.constraints == theirs.constraints
            })
        })
    }
}

/// Every `FIELD` child block of one block, in source order.
///
/// A `FIELD` is written as a nested field whose registered value kind is
/// `nested_block(FIELD)`, so it reaches the syntax tree as a [`Statement::Field`]
/// with a nested body rather than as a block of its own.
pub(crate) fn field_declarations(block: &Block) -> Vec<FieldDecl<'_>> {
    let mut out = Vec::new();
    for statement in &block.body {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "FIELD" {
            continue;
        }
        let Some(nested) = field.body.as_nested() else {
            continue;
        };
        out.push(read_field(&nested.statements, field.key.span));
    }
    out
}

/// Every `FIELD` child of a nested `SCHEMA` body, in source order.
pub(crate) fn schema_fields(statements: &[Statement]) -> Vec<FieldDecl<'_>> {
    let mut out = Vec::new();
    for statement in statements {
        let Statement::Field(field) = statement else {
            continue;
        };
        if field.key.text != "FIELD" {
            continue;
        }
        let Some(nested) = field.body.as_nested() else {
            continue;
        };
        out.push(read_field(&nested.statements, field.key.span));
    }
    out
}

fn read_field(statements: &[Statement], span: Span) -> FieldDecl<'_> {
    let mut decl = FieldDecl {
        name: None,
        type_expr: None,
        required: None,
        default: None,
        minimum: None,
        maximum: None,
        tolerance: None,
        pattern: None,
        span,
    };
    for statement in statements {
        let Statement::Field(field) = statement else {
            continue;
        };
        let inline = inline_expr(&field.body);
        match field.key.text.as_str() {
            "NAME" => {
                decl.name = match inline {
                    Some(Expr::Identifier(ident)) => Some(ident.text.as_str()),
                    _ => None,
                }
            }
            "TYPE" => decl.type_expr = inline,
            "REQUIRED" => decl.required = inline.and_then(boolean_literal),
            "DEFAULT" => decl.default = inline,
            "MINIMUM" => decl.minimum = inline,
            "MAXIMUM" => decl.maximum = inline,
            "TOLERANCE" => decl.tolerance = inline,
            "PATTERN" => decl.pattern = inline,
            _ => {}
        }
    }
    decl
}

fn inline_expr(body: &Body) -> Option<&Expr> {
    match body.as_inline()? {
        lcl_parser::syntax::Value::Expression(expr) => Some(expr),
        lcl_parser::syntax::Value::MultilineCollection(_) => None,
    }
}

/// `TRUE` or `FALSE` written as a literal.
pub(crate) fn boolean_literal(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Literal(literal) => match literal.kind {
            lcl_parser::syntax::LiteralKind::True => Some(true),
            lcl_parser::syntax::LiteralKind::False => Some(false),
            _ => None,
        },
        _ => None,
    }
}
