//! Source type expressions, defined types, and the acyclic type catalog.
//!
//! `03_TYPES_AND_VALUES/01_TYPE_SYSTEM_RULES.txt`:
//!
//! > Source type designators follow types_v0.1.0.json#/source_type_contract and
//! > the TYPE_EXPRESSION grammar. A defined type is written REF(identifier),
//! > never as a bare identifier. Type references resolve acyclically before
//! > value checking.
//!
//! ## Aliases are resolved away
//!
//! "A kind.type definition whose BASE resolves to an existing scalar or
//! collection type is a transparent alias. It creates no nominal subtype …"
//! So a definition never survives into [`Type`]: `REF(type.counter)` over
//! `BASE: INTEGER` *is* [`Type::Integer`], and an alias of a defined enum or
//! object carries that exact domain or schema through.
//!
//! ## Cycles
//!
//! `03_TYPES_AND_VALUES/01`: "cycles use error.reference.cycle". That
//! identifier is registered at `stage: resolution`, and M3 checks the alias
//! chains of the four domains that resolve to a *core identifier*
//! (`kind.error`, `kind.event`, `kind.status`, `kind.format`). A `kind.type`
//! chain is written `BASE: REF(type.id)`, resolves to a declaration rather than
//! to a core identifier, and is therefore not covered there: every `REF` in the
//! chain binds successfully and the resolution stage is clean.
//!
//! This catalog must resolve those chains to do its own work, so it detects the
//! cycle here and reports it with its **registered** identifier and stage
//! through [`crate::EarlierStageDefect`] — a classification, not a schedule.
//! Nothing about M3's behavior changes, and no static identifier is
//! substituted for a resolution one.

use crate::schema;
use crate::ty::{EnumDomain, ObjectField, ObjectType, RefTarget, Type};
use lcl_lexer::Span;
use lcl_parser::syntax::{Block, Body, Expr, Statement, TypeExpr, Value};
use lcl_resolver::{BindingTarget, Declaration, FullId, Resolved, SourceId};
use std::collections::{BTreeMap, BTreeSet};

/// What one `DEFINE KIND kind.type` introduces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Definition {
    /// A transparent alias, an enum domain, or an object schema — all of which
    /// are just the resolved [`Type`].
    Type(Type),
    /// Its `BASE` chain is cyclic. `error.reference.cycle`.
    Cycle,
    /// Its `BASE` cannot be read as a type here. Earlier stages own the
    /// diagnostic; this catalog records only that no type is available.
    Unresolvable,
}

/// Why a type expression could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TypeDefect {
    /// The expression is not a `TYPE_EXPRESSION` at all.
    NotAType,
    /// A `REF` in the expression does not name a usable `kind.type`.
    /// Earlier stages own it.
    Unresolvable,
    /// The referenced definition's `BASE` chain is cyclic.
    Cycle,
    /// `OBJECT[REF(id)]` whose definition is not an object schema.
    NotAnObject,
    /// A bare `OBJECT` or `ENUM` word, which "cannot declare an unconstrained
    /// material value".
    Unconstrained(&'static str),
}

/// Every defined type of one program, resolved acyclically.
pub(crate) struct TypeCatalog {
    /// Declaration index -> what it defines.
    definitions: BTreeMap<usize, Definition>,
    /// `(source, identifier span)` -> the declaration a `REF` bound to.
    bindings: BTreeMap<(SourceId, Span), usize>,
    /// Declaration index -> its exact identity.
    identities: BTreeMap<usize, FullId>,
    /// Declaration indexes whose `BASE` chain is cyclic, with the locus to
    /// report against.
    cycles: Vec<(usize, Span)>,
}

/// Colours of the definition dependency walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Colour {
    InProgress,
    Done,
}

impl TypeCatalog {
    /// Build the catalog for one resolved program.
    ///
    /// Total and iterative: a dependency graph of any depth costs heap, not
    /// stack, and a cycle terminates the walk instead of it.
    pub(crate) fn build(resolved: &Resolved) -> TypeCatalog {
        let mut bindings = BTreeMap::new();
        for binding in resolved.bindings() {
            if let BindingTarget::Declaration(index) = binding.target {
                bindings.insert((binding.source.clone(), binding.span), index);
            }
        }

        let identities: BTreeMap<usize, FullId> = resolved
            .declarations()
            .all()
            .iter()
            .enumerate()
            .map(|(index, declaration)| (index, declaration.id.clone()))
            .collect();

        let mut catalog = TypeCatalog {
            definitions: BTreeMap::new(),
            bindings,
            identities,
            cycles: Vec::new(),
        };

        let type_definitions: Vec<usize> = resolved
            .declarations()
            .all()
            .iter()
            .enumerate()
            .filter(|(_, d)| is_type_definition(d))
            .map(|(index, _)| index)
            .collect();

        let mut colour: BTreeMap<usize, Colour> = BTreeMap::new();
        for root in type_definitions {
            let mut stack = vec![(root, false)];
            while let Some((current, expanded)) = stack.pop() {
                if expanded {
                    let definition = catalog.compute(resolved, current);
                    catalog.definitions.insert(current, definition);
                    colour.insert(current, Colour::Done);
                    continue;
                }
                match colour.get(&current) {
                    Some(Colour::Done) => continue,
                    Some(Colour::InProgress) => {
                        // A back edge: this definition is on its own BASE chain.
                        catalog.definitions.insert(current, Definition::Cycle);
                        if let Some(span) = base_locus(resolved, current) {
                            if !catalog.cycles.iter().any(|(d, _)| *d == current) {
                                catalog.cycles.push((current, span));
                            }
                        }
                        continue;
                    }
                    None => {}
                }
                colour.insert(current, Colour::InProgress);
                stack.push((current, true));
                for dependency in catalog.dependencies(resolved, current) {
                    if colour.get(&dependency) != Some(&Colour::Done) {
                        stack.push((dependency, false));
                    }
                }
            }
        }
        catalog
    }

    /// Every definition whose `BASE` chain is cyclic, with its locus.
    pub(crate) fn cycles(&self) -> &[(usize, Span)] {
        &self.cycles
    }

    /// The declaration a `REF` at this exact locus bound to.
    pub(crate) fn binding(&self, source: &SourceId, span: Span) -> Option<usize> {
        self.bindings.get(&(source.clone(), span)).copied()
    }

    /// What a `DEFINE kind.type` declaration defines.
    pub(crate) fn definition(&self, declaration: usize) -> Option<&Definition> {
        self.definitions.get(&declaration)
    }

    /// Resolve one source `TYPE_EXPRESSION` to a static type.
    pub(crate) fn resolve(&self, source: &SourceId, expr: &Expr) -> Result<Type, TypeDefect> {
        match expr {
            Expr::Type(type_expr) => self.resolve_type_expr(source, type_expr),
            // `TYPE_EXPRESSION = NON_NULL_TYPE_EXPRESSION | REFERENCE_CALL |
            // "NULL"`. The parser gives a bare `REF(id)` as a call and `NULL` as
            // a literal, because both derivations are shared with ordinary
            // expressions.
            Expr::Call(call) if call.is_reference() => self.resolve_reference_type(source, call),
            Expr::Literal(literal) if literal.kind == lcl_parser::syntax::LiteralKind::Null => {
                Ok(Type::Null)
            }
            _ => Err(TypeDefect::NotAType),
        }
    }

    fn resolve_type_expr(
        &self,
        source: &SourceId,
        type_expr: &TypeExpr,
    ) -> Result<Type, TypeDefect> {
        match type_expr {
            TypeExpr::Scalar(word) => Type::scalar(&word.text).ok_or(match word.text.as_str() {
                "OBJECT" => TypeDefect::Unconstrained("OBJECT"),
                "ENUM" => TypeDefect::Unconstrained("ENUM"),
                _ => TypeDefect::NotAType,
            }),
            TypeExpr::List(bracket) => Ok(Type::List(Box::new(
                self.resolve(source, &bracket.argument)?,
            ))),
            TypeExpr::Set(bracket) => Ok(Type::Set(Box::new(
                self.resolve(source, &bracket.argument)?,
            ))),
            TypeExpr::Object(bracket) => {
                // "OBJECT[REF(identifier)] requires a kind.type whose resolved
                // BASE is OBJECT and uses its exact schema."
                let resolved = self.resolve(source, &bracket.argument)?;
                match resolved {
                    Type::Object(_) => Ok(resolved),
                    _ => Err(TypeDefect::NotAnObject),
                }
            }
            TypeExpr::Reference(bracket) => {
                // "REFERENCE[REF(identifier)] constrains the referenced
                // declaration identity, or the value type when identifier names
                // kind.type."
                let Expr::Call(call) = bracket.argument.as_ref() else {
                    return Err(TypeDefect::NotAType);
                };
                let Some(identifier) = call.reference_target() else {
                    return Err(TypeDefect::NotAType);
                };
                let Some(declaration) = self.binding(source, identifier.span) else {
                    return Err(TypeDefect::Unresolvable);
                };
                Ok(match self.definitions.get(&declaration) {
                    Some(Definition::Type(ty)) => {
                        Type::Reference(Box::new(RefTarget::Value(Box::new(ty.clone()))))
                    }
                    Some(Definition::Cycle) => return Err(TypeDefect::Cycle),
                    Some(Definition::Unresolvable) => return Err(TypeDefect::Unresolvable),
                    // Not a `kind.type`: the reference constrains the referenced
                    // declaration's identity, which M3 already bound and whose
                    // legality in this slot M3 already judged.
                    None => {
                        let Some(id) = self.identities.get(&declaration) else {
                            return Err(TypeDefect::Unresolvable);
                        };
                        Type::Reference(Box::new(RefTarget::Declaration(id.clone())))
                    }
                })
            }
        }
    }

    fn resolve_reference_type(
        &self,
        source: &SourceId,
        call: &lcl_parser::syntax::Call,
    ) -> Result<Type, TypeDefect> {
        let Some(identifier) = call.reference_target() else {
            return Err(TypeDefect::NotAType);
        };
        let Some(declaration) = self.binding(source, identifier.span) else {
            return Err(TypeDefect::Unresolvable);
        };
        match self.definitions.get(&declaration) {
            Some(Definition::Type(ty)) => Ok(ty.clone()),
            Some(Definition::Cycle) => Err(TypeDefect::Cycle),
            Some(Definition::Unresolvable) | None => Err(TypeDefect::Unresolvable),
        }
    }

    /// The definitions one definition's own resolution depends on.
    fn dependencies(&self, resolved: &Resolved, declaration: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let Some(block) = declaration_block(resolved, declaration) else {
            return out;
        };
        let Some(decl) = resolved.declarations().get(declaration) else {
            return out;
        };
        let mut expressions: Vec<&Expr> = Vec::new();
        if let Some(base) = block.field("BASE").and_then(|f| inline_expression(&f.body)) {
            expressions.push(base);
        }
        for field in schema::field_declarations(block) {
            if let Some(expr) = field.type_expr {
                expressions.push(expr);
            }
        }
        for expr in expressions {
            for span in reference_spans(expr) {
                if let Some(target) = self.binding(&decl.source, span) {
                    if target != declaration {
                        out.push(target);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Resolve one definition, with every dependency already resolved.
    fn compute(&self, resolved: &Resolved, declaration: usize) -> Definition {
        let Some(block) = declaration_block(resolved, declaration) else {
            return Definition::Unresolvable;
        };
        let Some(decl) = resolved.declarations().get(declaration) else {
            return Definition::Unresolvable;
        };
        let Some(base) = block.field("BASE") else {
            return Definition::Unresolvable;
        };
        let Some(base_expr) = inline_expression(&base.body) else {
            return Definition::Unresolvable;
        };

        // A direct `BASE ENUM` introduces a concrete domain; a direct
        // `BASE OBJECT` introduces the exact declared schema. Every other BASE
        // is a transparent alias of its resolved type.
        if let Expr::Type(TypeExpr::Scalar(word)) = base_expr {
            match word.text.as_str() {
                "ENUM" => {
                    let items: Vec<String> = block
                        .fields("ITEM")
                        .filter_map(|f| inline_identifier(&f.body))
                        .collect();
                    if items.is_empty() {
                        return Definition::Unresolvable;
                    }
                    return Definition::Type(Type::Enum(EnumDomain {
                        id: decl.id.clone(),
                        items,
                    }));
                }
                "OBJECT" => {
                    let mut fields = BTreeMap::new();
                    for field in schema::field_declarations(block) {
                        let Some(name) = field.name else { continue };
                        let Some(type_expr) = field.type_expr else {
                            return Definition::Unresolvable;
                        };
                        let Ok(ty) = self.resolve(&decl.source, type_expr) else {
                            return Definition::Unresolvable;
                        };
                        fields.insert(
                            name.to_string(),
                            ObjectField {
                                ty,
                                required: field.required.unwrap_or(true),
                            },
                        );
                    }
                    if fields.is_empty() {
                        return Definition::Unresolvable;
                    }
                    return Definition::Type(Type::Object(ObjectType::new(fields)));
                }
                _ => {}
            }
        }

        match self.resolve(&decl.source, base_expr) {
            Ok(ty) => Definition::Type(ty),
            Err(TypeDefect::Cycle) => Definition::Cycle,
            Err(_) => Definition::Unresolvable,
        }
    }
}

/// True for a `DEFINE` whose `KIND` is `kind.type`.
fn is_type_definition(declaration: &Declaration) -> bool {
    declaration.block == "DEFINE" && declaration.definition_kind.as_deref() == Some("kind.type")
}

/// The syntax block that declares one declaration.
pub(crate) fn declaration_block(resolved: &Resolved, declaration: usize) -> Option<&Block> {
    let decl = resolved.declarations().get(declaration)?;
    let unit = resolved.unit(&decl.source)?;
    let document = unit.document()?;
    find_block(document, decl.block_span.start)
}

/// Find the block whose header starts at one byte offset, at any depth.
fn find_block(document: &lcl_parser::syntax::Document, offset: usize) -> Option<&Block> {
    let mut stack: Vec<&Block> = Vec::new();
    for item in &document.items {
        collect_top_level(item, &mut stack);
    }
    while let Some(block) = stack.pop() {
        if block.key.span.start == offset {
            return Some(block);
        }
        for statement in &block.body {
            collect_statement(statement, &mut stack);
        }
    }
    None
}

/// One node still to be walked while collecting blocks.
enum Reachable<'a> {
    TopLevel(&'a lcl_parser::syntax::TopLevel),
    Executable(&'a lcl_parser::syntax::Executable),
    Statement(&'a Statement),
}

/// Append every `Block` reachable from one node, without recursing.
///
/// A block's own body is not walked here: [`find_block`] owns that step, and
/// walking it from inside would collect the same blocks twice.
///
/// The walk is a worklist because `04_GRAMMAR/02` declares no depth limit for
/// an indented body, and the three functions this replaced called one another
/// once per level. A document nested a few thousand deep ended the process
/// instead of returning a diagnostic. Children are pushed in reverse so
/// popping yields source order, which is the order the recursive form appended
/// in.
fn collect_reachable<'a>(root: Reachable<'a>, out: &mut Vec<&'a Block>) {
    use lcl_parser::syntax::{Executable, TopLevel};

    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let before = pending.len();
        match node {
            Reachable::TopLevel(TopLevel::Block(block))
            | Reachable::Executable(Executable::Block(block)) => {
                out.push(block);
            }
            Reachable::TopLevel(TopLevel::Conditional(node))
            | Reachable::Executable(Executable::Conditional(node))
            | Reachable::Statement(Statement::Conditional(node)) => {
                for executable in node.then_body.iter().chain(
                    node.else_body
                        .as_ref()
                        .map(|arm| arm.body.iter())
                        .unwrap_or_default(),
                ) {
                    pending.push(Reachable::Executable(executable));
                }
            }
            Reachable::TopLevel(TopLevel::ForEach(node))
            | Reachable::Executable(Executable::ForEach(node))
            | Reachable::Statement(Statement::ForEach(node)) => {
                for executable in &node.body {
                    pending.push(Reachable::Executable(executable));
                }
            }
            Reachable::Statement(Statement::Field(field)) => {
                if let Body::Nested(nested) = &field.body {
                    for inner in &nested.statements {
                        pending.push(Reachable::Statement(inner));
                    }
                }
            }
            Reachable::Statement(Statement::Property(property)) => {
                if let Body::Nested(nested) = &property.body {
                    for inner in &nested.statements {
                        pending.push(Reachable::Statement(inner));
                    }
                }
            }
        }
        pending[before..].reverse();
    }
}

fn collect_top_level<'a>(item: &'a lcl_parser::syntax::TopLevel, out: &mut Vec<&'a Block>) {
    collect_reachable(Reachable::TopLevel(item), out);
}

fn collect_statement<'a>(statement: &'a Statement, out: &mut Vec<&'a Block>) {
    collect_reachable(Reachable::Statement(statement), out);
}

/// The locus a definition's `BASE` defect reports against.
fn base_locus(resolved: &Resolved, declaration: usize) -> Option<Span> {
    let block = declaration_block(resolved, declaration)?;
    block
        .field("BASE")
        .map(|f| f.body.span())
        .or(Some(block.key.span))
}

/// The inline expression of a field body, if it has one.
pub(crate) fn inline_expression(body: &Body) -> Option<&Expr> {
    match body.as_inline()? {
        Value::Expression(expr) => Some(expr),
        Value::MultilineCollection(_) => None,
    }
}

/// The lowercase identifier an inline field body spells, if it spells one.
pub(crate) fn inline_identifier(body: &Body) -> Option<String> {
    match inline_expression(body)? {
        Expr::Identifier(ident) => Some(ident.text.clone()),
        _ => None,
    }
}

/// Every `REF(identifier)` identifier span inside one expression.
///
/// Iterative: expression depth costs heap, not stack.
pub(crate) fn reference_spans(expr: &Expr) -> Vec<Span> {
    let mut out = Vec::new();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut stack = vec![expr];
    while let Some(current) = stack.pop() {
        match current {
            Expr::Call(call) => {
                if let Some(identifier) = call.reference_target() {
                    if seen.insert(identifier.span.start) {
                        out.push(identifier.span);
                    }
                }
                for argument in &call.arguments {
                    stack.push(argument);
                }
            }
            Expr::Collection(collection) => {
                for member in &collection.members {
                    stack.push(member);
                }
            }
            Expr::Group(group) => stack.push(&group.inner),
            Expr::Unary(unary) => stack.push(&unary.operand),
            Expr::Binary(binary) => {
                stack.push(&binary.left);
                stack.push(&binary.right);
            }
            Expr::Property(property) => stack.push(&property.base),
            Expr::Index(index) => {
                stack.push(&index.base);
                stack.push(&index.index);
            }
            Expr::Type(type_expr) => match type_expr {
                TypeExpr::List(b)
                | TypeExpr::Set(b)
                | TypeExpr::Object(b)
                | TypeExpr::Reference(b) => stack.push(&b.argument),
                TypeExpr::Scalar(_) => {}
            },
            Expr::Literal(_) | Expr::Identifier(_) => {}
        }
    }
    out.sort_by_key(|s| s.start);
    out
}
