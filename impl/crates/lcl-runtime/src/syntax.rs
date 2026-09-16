//! Locating declared fields in the source-faithful syntax tree.
//!
//! This module navigates; it decides nothing. Every function answers a
//! structural question — "which block declares this declaration", "what
//! expression is inline in this field" — and none interprets a value, resolves
//! a type or judges a contract. Those belong to M2, M3, M4 and the rest of this
//! crate respectively.
//!
//! M5 has a module of the same shape and keeps it private. Rather than widen
//! that crate's surface for this one, the navigation is repeated here, so M5's
//! public API is exactly what it was before this milestone.

use lcl_parser::syntax::{Block, Executable, Expr, Field, Statement, TopLevel, Value, Word};
use lcl_resolver::Resolved;

/// One declaring block, however the grammar spells it.
///
/// A top-level block is a [`Block`]. A *nested* block — a `STEP` under a
/// `SEQUENCE`, an inline `ACTION` under a `STEP`, a `RETRY` under an `ACTION` —
/// is written with the same `KEY:` then indented body syntax, and the parser
/// records it faithfully as a [`Field`] whose body is `Body::Nested`.
#[derive(Clone, Copy)]
pub struct DeclBlock<'a> {
    key: &'a Word,
    statements: &'a [Statement],
}

impl<'a> DeclBlock<'a> {
    pub fn of(block: &'a Block) -> DeclBlock<'a> {
        DeclBlock {
            key: &block.key,
            statements: &block.body,
        }
    }

    /// The block word, e.g. `ACTION`.
    pub fn key_text(&self) -> &'a str {
        &self.key.text
    }

    pub fn statements(&self) -> &'a [Statement] {
        self.statements
    }

    /// The first direct field spelled `name`.
    pub fn field(&self, name: &str) -> Option<&'a Field> {
        self.statements.iter().find_map(|s| match s {
            Statement::Field(f) if f.key.text == name => Some(f),
            _ => None,
        })
    }

    /// Every direct field spelled `name`, in source order.
    pub fn fields(&self, name: &str) -> Vec<&'a Field> {
        self.statements
            .iter()
            .filter_map(|s| match s {
                Statement::Field(f) if f.key.text == name => Some(f),
                _ => None,
            })
            .collect()
    }

    /// A nested child block spelled `name`, e.g. `RETRY` under an `ACTION`.
    pub fn nested(&self, name: &str) -> Option<DeclBlock<'a>> {
        let field = self.field(name)?;
        let nested = field.body.as_nested()?;
        Some(DeclBlock {
            key: &field.key,
            statements: &nested.statements,
        })
    }
}

/// The inline expression of one field, when it has one.
pub fn field_expr<'a>(block: &DeclBlock<'a>, name: &str) -> Option<&'a Expr> {
    block.field(name)?.body.as_inline()?.as_expression()
}

/// The written text of one field's inline value, when it has one.
pub fn field_text(block: &DeclBlock<'_>, name: &str) -> Option<String> {
    let value = block.field(name)?.body.as_inline()?;
    Some(match value {
        Value::Expression(expr) => render(expr),
        other => render_value(other),
    })
}

fn render_value(value: &Value) -> String {
    match value {
        Value::Expression(expr) => render(expr),
        _ => String::new(),
    }
}

/// The block that declares one declaration, at any depth.
pub fn declaration_block(resolved: &Resolved, declaration: usize) -> Option<DeclBlock<'_>> {
    let decl = resolved.declarations().get(declaration)?;
    let document = resolved.unit(&decl.source)?.document()?;
    find_block(document, decl.block_span.start)
}

/// The written text of one declaration's field, e.g. a metadata read.
pub fn declaration_field_text(
    resolved: &Resolved,
    declaration: usize,
    field: &str,
) -> Option<String> {
    let block = declaration_block(resolved, declaration)?;
    field_text(&block, field)
}

/// Find the declaring block whose header starts at one byte offset.
///
/// Iterative, not recursive: a deeply nested document costs heap, never stack,
/// which is the same discipline the parser adopted when its stack-safety defect
/// was repaired.
fn find_block(document: &lcl_parser::syntax::Document, offset: usize) -> Option<DeclBlock<'_>> {
    blocks(document)
        .into_iter()
        .find(|block| block.key.span.start == offset)
}

/// Every declaring block in one document, in document order.
///
/// A top-level `BLOCK` and a nested `KEY:` block are both declaring blocks;
/// `03_TYPES_AND_VALUES` notes the two share "one syntactic form", so the walk
/// yields both rather than privileging the top-level spelling.
pub fn blocks(document: &lcl_parser::syntax::Document) -> Vec<DeclBlock<'_>> {
    let mut out = Vec::new();
    let mut statements: Vec<&Statement> = Vec::new();

    for item in &document.items {
        match item {
            TopLevel::Block(block) => {
                out.push(DeclBlock::of(block));
                statements.extend(block.body.iter());
            }
            TopLevel::Conditional(conditional) => {
                push_conditional(conditional, &mut out, &mut statements)
            }
            TopLevel::ForEach(for_each) => {
                push_executables(&for_each.body, &mut out, &mut statements)
            }
        }
    }

    while let Some(statement) = statements.pop() {
        match statement {
            Statement::Field(field) => {
                // A field whose body is nested is a nested declaring block.
                if let Some(nested) = field.body.as_nested() {
                    out.push(DeclBlock {
                        key: &field.key,
                        statements: &nested.statements,
                    });
                    statements.extend(nested.statements.iter());
                }
            }
            Statement::Property(_) => {}
            Statement::Conditional(conditional) => {
                push_conditional(conditional, &mut out, &mut statements)
            }
            Statement::ForEach(for_each) => {
                push_executables(&for_each.body, &mut out, &mut statements)
            }
        }
    }
    out
}

fn push_conditional<'a>(
    conditional: &'a lcl_parser::syntax::Conditional,
    out: &mut Vec<DeclBlock<'a>>,
    statements: &mut Vec<&'a Statement>,
) {
    push_executables(&conditional.then_body, out, statements);
    if let Some(arm) = &conditional.else_body {
        push_executables(&arm.body, out, statements);
    }
}

fn push_executables<'a>(
    body: &'a [Executable],
    out: &mut Vec<DeclBlock<'a>>,
    statements: &mut Vec<&'a Statement>,
) {
    let mut pending: Vec<&'a Executable> = body.iter().collect();
    while let Some(executable) = pending.pop() {
        match executable {
            Executable::Block(block) => {
                out.push(DeclBlock::of(block));
                statements.extend(block.body.iter());
            }
            Executable::Conditional(conditional) => {
                pending.extend(conditional.then_body.iter());
                if let Some(arm) = &conditional.else_body {
                    pending.extend(arm.body.iter());
                }
            }
            Executable::ForEach(for_each) => pending.extend(for_each.body.iter()),
        }
    }
}

pub use lcl_parser::syntax::render;

// ---------------------------------------------------------------------------
// Control forms
// ---------------------------------------------------------------------------
//
// The candidate graph records an `IF` / `ELSE` / `FOR EACH` node by its keyword
// span, and deliberately does not carry the condition or collection
// expression — evaluating those is step 10's, not step 4's. These lookups
// recover the exact control form the graph node came from, so the runtime reads
// the same source the resolver walked rather than a copy of it.

/// Every control form in one document, by the keyword span the graph records.
pub fn control_forms(
    document: &lcl_parser::syntax::Document,
) -> (
    Vec<&lcl_parser::syntax::Conditional>,
    Vec<&lcl_parser::syntax::ForEach>,
) {
    let mut conditionals = Vec::new();
    let mut loops = Vec::new();
    let mut executables: Vec<&Executable> = Vec::new();
    let mut statements: Vec<&Statement> = Vec::new();

    for item in &document.items {
        match item {
            TopLevel::Block(block) => statements.extend(block.body.iter()),
            TopLevel::Conditional(conditional) => {
                conditionals.push(conditional);
                executables.extend(conditional.then_body.iter());
                if let Some(arm) = &conditional.else_body {
                    executables.extend(arm.body.iter());
                }
            }
            TopLevel::ForEach(for_each) => {
                loops.push(for_each);
                executables.extend(for_each.body.iter());
            }
        }
    }

    while !statements.is_empty() || !executables.is_empty() {
        while let Some(statement) = statements.pop() {
            match statement {
                Statement::Field(field) => {
                    if let Some(nested) = field.body.as_nested() {
                        statements.extend(nested.statements.iter());
                    }
                }
                Statement::Property(_) => {}
                Statement::Conditional(conditional) => {
                    conditionals.push(conditional);
                    executables.extend(conditional.then_body.iter());
                    if let Some(arm) = &conditional.else_body {
                        executables.extend(arm.body.iter());
                    }
                }
                Statement::ForEach(for_each) => {
                    loops.push(for_each);
                    executables.extend(for_each.body.iter());
                }
            }
        }
        while let Some(executable) = executables.pop() {
            match executable {
                Executable::Block(block) => statements.extend(block.body.iter()),
                Executable::Conditional(conditional) => {
                    conditionals.push(conditional);
                    executables.extend(conditional.then_body.iter());
                    if let Some(arm) = &conditional.else_body {
                        executables.extend(arm.body.iter());
                    }
                }
                Executable::ForEach(for_each) => {
                    loops.push(for_each);
                    executables.extend(for_each.body.iter());
                }
            }
        }
    }
    (conditionals, loops)
}

/// The `IF` whose keyword starts at one byte offset.
pub fn conditional_at(
    document: &lcl_parser::syntax::Document,
    offset: usize,
) -> Option<&lcl_parser::syntax::Conditional> {
    control_forms(document)
        .0
        .into_iter()
        .find(|c| c.keyword_span.start == offset)
}

/// The `IF` whose `ELSE` keyword starts at one byte offset.
pub fn conditional_with_else_at(
    document: &lcl_parser::syntax::Document,
    offset: usize,
) -> Option<&lcl_parser::syntax::Conditional> {
    control_forms(document).0.into_iter().find(|c| {
        c.else_body
            .as_ref()
            .is_some_and(|arm| arm.keyword_span.start == offset)
    })
}

/// The `FOR EACH` whose keyword starts at one byte offset.
pub fn for_each_at(
    document: &lcl_parser::syntax::Document,
    offset: usize,
) -> Option<&lcl_parser::syntax::ForEach> {
    control_forms(document)
        .1
        .into_iter()
        .find(|f| f.keyword_span.start == offset)
}
