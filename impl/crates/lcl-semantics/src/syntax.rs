//! Locating declared fields in the source-faithful syntax tree.
//!
//! This module navigates; it decides nothing. Every function here answers a
//! structural question — "which block declares this declaration", "what
//! expression is inline in this field", "which declaration does this `REF`
//! name" — and none of them interprets a value, resolves a type or judges a
//! contract. Those belong to M3, M4 and the rest of this crate respectively.

use lcl_lexer::Span;
use lcl_parser::syntax::{
    Block, Body, Collection, Executable, Expr, Field, Statement, TopLevel, Value, Word,
};
use lcl_resolver::Resolved;

/// One declaring block, however the grammar spells it.
///
/// A top-level block is a [`Block`]. A *nested* block — a `STEP` under a
/// `SEQUENCE`, an inline `ACTION` under a `STEP`, a `RETRY` under an `ACTION` —
/// is written with the same `KEY:` then indented body syntax, and the parser
/// records it faithfully as a [`Field`] whose body is `Body::Nested`, because
/// "This one syntactic form covers a nested child block, an object-data value
/// and a locally nested schema alike" and the parser does not choose between
/// them.
///
/// Both spellings declare fields the same way, so this type presents them the
/// same way. Without it, every rule in this crate would silently apply to
/// top-level declarations only, which is exactly the kind of half-applied rule
/// a preflight layer must not have.
#[derive(Clone, Copy)]
pub(crate) struct DeclBlock<'a> {
    key: &'a Word,
    statements: &'a [Statement],
}

impl<'a> DeclBlock<'a> {
    /// Present a top-level block through the same interface.
    pub(crate) fn of(block: &'a Block) -> DeclBlock<'a> {
        DeclBlock {
            key: &block.key,
            statements: &block.body,
        }
    }

    /// The block word, e.g. `ACTION`.
    pub(crate) fn key_text(&self) -> &'a str {
        &self.key.text
    }

    /// Every direct statement of the block's body.
    pub(crate) fn statements(&self) -> &'a [Statement] {
        self.statements
    }

    /// The first direct field spelled `name`.
    pub(crate) fn field(&self, name: &str) -> Option<&'a Field> {
        self.statements.iter().find_map(|s| match s {
            Statement::Field(f) if f.key.text == name => Some(f),
            _ => None,
        })
    }
}

/// The block that declares one declaration, at any depth.
pub(crate) fn declaration_block(resolved: &Resolved, declaration: usize) -> Option<DeclBlock<'_>> {
    let decl = resolved.declarations().get(declaration)?;
    let document = resolved.unit(&decl.source)?.document()?;
    find_block(document, decl.block_span.start)
}

/// Find the declaring block whose header starts at one byte offset, at any
/// depth and in either spelling.
fn find_block(document: &lcl_parser::syntax::Document, offset: usize) -> Option<DeclBlock<'_>> {
    let mut blocks: Vec<&Block> = Vec::new();
    for item in &document.items {
        collect_top_level(item, &mut blocks);
    }
    while let Some(block) = blocks.pop() {
        if block.key.span.start == offset {
            return Some(DeclBlock {
                key: &block.key,
                statements: &block.body,
            });
        }
        let mut statements: Vec<&Statement> = block.body.iter().collect();
        while let Some(statement) = statements.pop() {
            match statement {
                Statement::Field(field) => {
                    if let Body::Nested(nested) = &field.body {
                        if field.key.span.start == offset {
                            return Some(DeclBlock {
                                key: &field.key,
                                statements: &nested.statements,
                            });
                        }
                        statements.extend(nested.statements.iter());
                    }
                }
                Statement::Property(property) => {
                    if let Body::Nested(nested) = &property.body {
                        statements.extend(nested.statements.iter());
                    }
                }
                Statement::Conditional(c) => {
                    for executable in c.then_body.iter().chain(
                        c.else_body
                            .as_ref()
                            .map(|e| e.body.iter())
                            .unwrap_or_default(),
                    ) {
                        collect_executable(executable, &mut blocks);
                    }
                }
                Statement::ForEach(f) => {
                    for executable in &f.body {
                        collect_executable(executable, &mut blocks);
                    }
                }
            }
        }
    }
    None
}

fn collect_top_level<'a>(item: &'a TopLevel, out: &mut Vec<&'a Block>) {
    match item {
        TopLevel::Block(b) => out.push(b),
        TopLevel::Conditional(c) => {
            for executable in c.then_body.iter().chain(
                c.else_body
                    .as_ref()
                    .map(|e| e.body.iter())
                    .unwrap_or_default(),
            ) {
                collect_executable(executable, out);
            }
        }
        TopLevel::ForEach(f) => {
            for executable in &f.body {
                collect_executable(executable, out);
            }
        }
    }
}

fn collect_executable<'a>(executable: &'a Executable, out: &mut Vec<&'a Block>) {
    match executable {
        Executable::Block(b) => out.push(b),
        Executable::Conditional(c) => {
            for inner in c.then_body.iter().chain(
                c.else_body
                    .as_ref()
                    .map(|e| e.body.iter())
                    .unwrap_or_default(),
            ) {
                collect_executable(inner, out);
            }
        }
        Executable::ForEach(f) => {
            for inner in &f.body {
                collect_executable(inner, out);
            }
        }
    }
}

/// The expression inline in one field body, if the body is inline.
pub(crate) fn inline_expr(body: &Body) -> Option<&Expr> {
    match body {
        Body::Inline(Value::Expression(expr)) => Some(expr),
        _ => None,
    }
}

/// The collection inline in one field body, whether written inline or as a
/// `MULTILINE_COLLECTION`.
pub(crate) fn inline_collection(body: &Body) -> Option<&Collection> {
    match body {
        Body::Inline(Value::MultilineCollection(c)) => Some(c),
        Body::Inline(Value::Expression(Expr::Collection(c))) => Some(c),
        _ => None,
    }
}

/// The declaration id one `REF(...)` expression names.
///
/// Returns `None` for anything that is not a reference call, so a value can
/// never be mistaken for an identity.
pub(crate) fn reference_target(expr: &Expr) -> Option<&str> {
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
/// expands left to right." That order is normative, so it is preserved here
/// rather than sorted.
pub(crate) fn reference_list(body: &Body) -> Vec<(String, Span)> {
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

/// The exact text of one inline string or identifier field.
pub(crate) fn field_text(block: DeclBlock, name: &str) -> Option<String> {
    let field = block.field(name)?;
    let expr = inline_expr(&field.body)?;
    literal_text(expr)
}

/// The text of a literal, identifier or single-argument constructor.
pub(crate) fn literal_text(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(ident) => Some(ident.text.clone()),
        Expr::Literal(literal) => Some(literal.text.clone()),
        Expr::Group(group) => literal_text(&group.inner),
        _ => None,
    }
}

/// The integer value of one inline field, when it is written as an integer
/// literal with an optional sign.
///
/// Deliberately narrow: `AUTHORITY` and `PRIORITY` are "a strict INTEGER", so
/// an expression that merely evaluates to one is not accepted here. A field
/// that is not a plain integer literal yields `None`, and the caller reports
/// rather than guesses.
pub(crate) fn field_integer(block: DeclBlock, name: &str) -> Option<i64> {
    let field = block.field(name)?;
    let expr = inline_expr(&field.body)?;
    integer_of(expr)
}

fn integer_of(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Literal(literal) if literal.kind == lcl_parser::syntax::LiteralKind::Integer => {
            literal.text.parse::<i64>().ok()
        }
        Expr::Group(group) => integer_of(&group.inner),
        Expr::Unary(unary) => {
            let inner = integer_of(&unary.operand)?;
            match unary.operator {
                lcl_parser::syntax::UnaryOp::Negate => Some(-inner),
                _ => None,
            }
        }
        _ => None,
    }
}
