//! Reference binding: what every `REF` in the program resolves to.
//!
//! `types_v0.1.0.json#/reference_context_contract`: "REF(identifier) resolves
//! exactly one declaration or loop-local binding by exact identity before its
//! receiving context is applied. Unresolved identities use
//! error.reference.unresolved; incompatible reference kinds use
//! error.reference.kind; prohibited cycles use error.reference.cycle."
//!
//! "Before its receiving context is applied" is the boundary of this milestone.
//! Whether a bound reference is then read as an identity or as one value is the
//! `identity_contexts` / `value_contexts` split, which needs types; that is
//! M4's. Here every `REF` acquires exactly one target, or a diagnostic.

use lcl_lexer::Span;

use crate::declarations::FullId;
use crate::source::SourceId;

/// What one reference resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingTarget {
    /// A declaration, by index into the [`crate::DeclarationIndex`].
    Declaration(usize),
    /// A `FOR EACH` loop-local binding.
    ///
    /// `02_LEXICAL/03`: "Loop-local identifiers exist only in their FOR EACH
    /// body and may be referenced with REF(local_id)." They are not
    /// declarations: they have no ID field, no namespace, and a distinct
    /// binding identity per iteration.
    LoopLocal {
        /// Locus of the binding identifier in its `FOR EACH` header.
        binding_span: Span,
    },
    /// Nothing resolved. A diagnostic was emitted, unless this reference was
    /// dependent on a source that failed to load.
    Unresolved,
}

/// One `REF` occurrence and what it bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The unit the reference is written in.
    pub source: SourceId,
    /// Locus of the identifier inside `REF(...)`.
    pub span: Span,
    /// The identifier exactly as written.
    pub text: String,
    /// The receiving block and field, when the reference stands in a field
    /// slot with a registered target set.
    pub slot: Option<(String, String)>,
    /// What it resolved to.
    pub target: BindingTarget,
    /// The resolved declaration's identity, when it resolved to one.
    pub resolved_id: Option<FullId>,
}

impl Binding {
    pub fn is_resolved(&self) -> bool {
        !matches!(self.target, BindingTarget::Unresolved)
    }
}

// ---------------------------------------------------------------------------
// Phase D: binding every reference
// ---------------------------------------------------------------------------

use crate::{Diagnostic, Emitter, ResolutionError, Resolved, Resolver, UnitPath};
use lcl_parser::syntax::{Block, Executable, Expr, Statement, TopLevel, Value};
use std::collections::BTreeSet;

/// Bind every reference in every usable unit.
pub(crate) fn bind(resolver: &Resolver<'_>, resolved: &mut Resolved, raw: &mut Vec<Diagnostic>) {
    let mut bindings = Vec::new();
    {
        let emitter = Emitter::new(resolver.rules(), &resolved.units);
        for path in &resolved.paths {
            let Some(unit) = resolved.units.get(&path.unit) else {
                continue;
            };
            if !unit.is_usable() {
                continue;
            }
            let Some(document) = unit.document() else {
                continue;
            };
            let mut binder = Binder {
                resolver,
                resolved,
                emitter: &emitter,
                path,
                scopes: Vec::new(),
                bindings: &mut bindings,
                raw,
            };
            for item in &document.items {
                match item {
                    TopLevel::Block(b) => binder.block(b),
                    TopLevel::Conditional(c) => binder.conditional(c),
                    TopLevel::ForEach(f) => binder.for_each(f),
                }
            }
        }
        check_alias_chains(resolver, resolved, &emitter, raw);
        check_fallback_cycles(resolver, resolved, &emitter, raw);
    }
    resolved.bindings = bindings;
}

struct Binder<'a, 'b> {
    resolver: &'a Resolver<'a>,
    resolved: &'a Resolved,
    emitter: &'a Emitter<'a>,
    path: &'a UnitPath,
    /// Enclosing `FOR EACH` bindings, innermost last.
    scopes: Vec<(String, Span)>,
    bindings: &'b mut Vec<Binding>,
    raw: &'b mut Vec<Diagnostic>,
}

impl Binder<'_, '_> {
    fn block(&mut self, block: &Block) {
        self.statements(&block.key.text, &block.body);
    }

    fn statements(&mut self, block_name: &str, body: &[Statement]) {
        for statement in body {
            match statement {
                Statement::Field(field_stmt) => {
                    let key = field_stmt.key.text.as_str();
                    let child = self
                        .resolver
                        .grammar()
                        .schema(block_name)
                        .and_then(|s| s.field(key))
                        .and_then(|f| f.nested_block.as_deref());
                    match (&field_stmt.body, child) {
                        (lcl_parser::syntax::Body::Nested(nested), Some(child)) => {
                            self.statements(child, &nested.statements);
                        }
                        (lcl_parser::syntax::Body::Nested(nested), None) => {
                            // Object data or a local schema: no field slot
                            // applies to what is inside.
                            self.free_statements(nested);
                        }
                        (lcl_parser::syntax::Body::Inline(value), _) => {
                            self.value(block_name, key, value);
                        }
                    }
                }
                Statement::Property(property) => match &property.body {
                    lcl_parser::syntax::Body::Inline(value) => self.free_value(value),
                    lcl_parser::syntax::Body::Nested(nested) => self.free_statements(nested),
                },
                Statement::Conditional(c) => self.conditional(c),
                Statement::ForEach(f) => self.for_each(f),
            }
        }
    }

    /// Statements whose enclosing field is not a registered child block, so no
    /// reference slot governs them.
    fn free_statements(&mut self, nested: &lcl_parser::syntax::Nested) {
        for statement in &nested.statements {
            match statement {
                Statement::Field(f) => match &f.body {
                    lcl_parser::syntax::Body::Inline(value) => self.free_value(value),
                    lcl_parser::syntax::Body::Nested(n) => self.free_statements(n),
                },
                Statement::Property(p) => match &p.body {
                    lcl_parser::syntax::Body::Inline(value) => self.free_value(value),
                    lcl_parser::syntax::Body::Nested(n) => self.free_statements(n),
                },
                Statement::Conditional(c) => self.conditional(c),
                Statement::ForEach(f) => self.for_each(f),
            }
        }
    }

    fn free_value(&mut self, value: &Value) {
        match value {
            Value::Expression(e) => self.expression(e, None),
            Value::MultilineCollection(c) => {
                for member in &c.members {
                    self.expression(member, None);
                }
            }
        }
    }

    fn value(&mut self, block: &str, field_key: &str, value: &Value) {
        // An operation identifier is a bare identifier, not a reference.
        if self.resolver.rules().is_operation_slot(block, field_key) {
            if let Value::Expression(Expr::Identifier(ident)) = value {
                self.operation(ident);
                return;
            }
        }
        let slot = self
            .resolver
            .rules()
            .reference_slot(block, field_key)
            .map(|_| (block.to_string(), field_key.to_string()));
        match value {
            Value::Expression(e) => self.expression(e, slot),
            Value::MultilineCollection(c) => {
                for member in &c.members {
                    self.expression(member, slot.clone());
                }
            }
        }
    }

    /// Walk one expression, finding every `REF` in it.
    ///
    /// Iterative rather than recursive: source is untrusted, and a deeply
    /// nested expression must not consume stack proportional to its depth.
    /// Children are pushed in reverse so they are visited in source order.
    ///
    /// `types_v0.1.0.json#/reference_context_contract` fixes how far a
    /// receiving slot reaches: "A receiving identity context applies to a
    /// direct REF expression, including parentheses around it, or to the
    /// members of a reference-typed collection. It does not change the
    /// evaluation rules inside another operator, function call, property
    /// access, or index access." So the slot is carried through a group and a
    /// collection, and dropped everywhere else.
    fn expression<'e>(&mut self, expr: &'e Expr, slot: Option<(String, String)>) {
        use lcl_parser::syntax::TypeExpr;
        let mut stack: Vec<(&'e Expr, Option<(String, String)>)> = vec![(expr, slot)];
        while let Some((expr, slot)) = stack.pop() {
            match expr {
                Expr::Call(call) if call.callable.text == "REF" => {
                    match call.arguments.as_slice() {
                        [Expr::Identifier(ident)] => self.reference(ident, slot),
                        args => {
                            for arg in args.iter().rev() {
                                stack.push((arg, None));
                            }
                        }
                    }
                }
                Expr::Group(group) => stack.push((&group.inner, slot)),
                Expr::Collection(collection) => {
                    for member in collection.members.iter().rev() {
                        stack.push((member, slot.clone()));
                    }
                }
                Expr::Call(call) => {
                    for arg in call.arguments.iter().rev() {
                        stack.push((arg, None));
                    }
                }
                Expr::Unary(u) => stack.push((&u.operand, None)),
                Expr::Binary(b) => {
                    stack.push((&b.right, None));
                    stack.push((&b.left, None));
                }
                Expr::Property(p) => stack.push((&p.base, None)),
                Expr::Index(i) => {
                    stack.push((&i.index, None));
                    stack.push((&i.base, None));
                }
                Expr::Type(type_expr) => match type_expr {
                    // `LIST[T]` and `SET[T]` nest a type expression, and
                    // `OBJECT[REF(t)]` "requires a kind.type whose resolved
                    // BASE is OBJECT", so all three keep the receiving type
                    // slot and its `DEFINE kind.type` target.
                    TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) => {
                        stack.push((&b.argument, slot))
                    }
                    // `REFERENCE[REF(t)]` is deliberately broader.
                    // `#/source_type_contract`: it "constrains the referenced
                    // declaration identity, **or** the value type when
                    // identifier names kind.type", and `04_GRAMMAR/11` rule 8
                    // spells the form `REFERENCE[REF(kind_or_type)]`. Canonical
                    // example `08_EXAMPLES/VALID/12_SET_SORTING.lcl` writes
                    // `REFERENCE[REF(sort.identity_key)]` over a
                    // `kind.operation` definition, so the inner reference binds
                    // an identity with no resolution-stage kind constraint.
                    TypeExpr::Reference(b) => stack.push((&b.argument, None)),
                    TypeExpr::Scalar(_) => {}
                },
                Expr::Literal(_) | Expr::Identifier(_) => {}
            }
        }
    }

    fn conditional(&mut self, c: &lcl_parser::syntax::Conditional) {
        self.expression(&c.condition, None);
        self.executables(&c.then_body);
        if let Some(arm) = &c.else_body {
            self.executables(&arm.body);
        }
    }

    fn for_each(&mut self, f: &lcl_parser::syntax::ForEach) {
        // The collection expression is evaluated outside the body, so the
        // binding is not yet in scope for it.
        self.expression(&f.collection, None);
        self.scopes.push((f.binding.text.clone(), f.binding.span));
        self.executables(&f.body);
        self.scopes.pop();
    }

    fn executables(&mut self, body: &[Executable]) {
        for item in body {
            match item {
                Executable::Block(b) => self.block(b),
                Executable::Conditional(c) => self.conditional(c),
                Executable::ForEach(f) => self.for_each(f),
            }
        }
    }

    /// Resolve one `REF(identifier)`.
    fn reference(&mut self, ident: &lcl_parser::syntax::Ident, slot: Option<(String, String)>) {
        let text = ident.text.as_str();

        // 02_LEXICAL/03: "Loop-local identifiers exist only in their FOR EACH
        // body and may be referenced with REF(local_id)." Innermost first.
        if !ident.qualified {
            if let Some((_, binding_span)) = self.scopes.iter().rev().find(|(n, _)| n == text) {
                self.bindings.push(Binding {
                    source: self.path.unit.clone(),
                    span: ident.span,
                    text: text.to_string(),
                    slot,
                    target: BindingTarget::LoopLocal {
                        binding_span: *binding_span,
                    },
                    resolved_id: None,
                });
                return;
            }
        }

        let qualified = self.path.qualify(text).qualified();
        let matches = self.resolved.declarations.by_qualified(&qualified);
        let Some(&index) = matches.first() else {
            // A reference into a namespace whose source never loaded is not an
            // independent defect: `multiplicity_rule` emits every *independent*
            // applicable diagnostic, and the import failure already named this
            // one's cause.
            if !self.namespace_failed(text) {
                self.emitter.emit(
                    self.raw,
                    ResolutionError::ReferenceUnresolved,
                    &self.path.unit,
                    ident.span,
                    &format!("unresolved:{qualified}"),
                    format!("`{text}` does not resolve to a declaration or loop-local binding"),
                );
            }
            self.bindings.push(Binding {
                source: self.path.unit.clone(),
                span: ident.span,
                text: text.to_string(),
                slot,
                target: BindingTarget::Unresolved,
                resolved_id: None,
            });
            return;
        };

        // An identity that matched more than once is already reported as
        // error.id.duplicate at its declarations; binding to the first keeps
        // one cause to one diagnostic.
        let declaration = self
            .resolved
            .declarations
            .get(index)
            .expect("index came from the index");

        if let Some((block, key)) = &slot {
            if let Some(reference_slot) = self.resolver.rules().reference_slot(block, key) {
                if !reference_slot
                    .accepts(&declaration.block, declaration.definition_kind.as_deref())
                {
                    self.emitter.emit(
                        self.raw,
                        ResolutionError::ReferenceKind,
                        &self.path.unit,
                        ident.span,
                        &format!("wrong-kind:{qualified}"),
                        format!(
                            "`{text}` resolves to a {}, but {block}.{key} accepts {}",
                            declaration.block,
                            reference_slot.describe()
                        ),
                    );
                }
            }
        }

        self.bindings.push(Binding {
            source: self.path.unit.clone(),
            span: ident.span,
            text: text.to_string(),
            slot,
            target: BindingTarget::Declaration(index),
            resolved_id: Some(declaration.id.clone()),
        });
    }

    /// `operation_identifier`: "A core_operation_ids member or the identifier
    /// of a DEFINE declaration whose KIND is kind.operation."
    fn operation(&mut self, ident: &lcl_parser::syntax::Ident) {
        let text = ident.text.as_str();
        if self.resolver.rules().is_core_operation(text) {
            return;
        }
        let qualified = self.path.qualify(text).qualified();
        let defined = self
            .resolved
            .declarations
            .by_qualified(&qualified)
            .iter()
            .filter_map(|&i| self.resolved.declarations.get(i))
            .any(|d| d.block == "DEFINE" && d.definition_kind.as_deref() == Some("kind.operation"));
        if defined || self.namespace_failed(text) {
            return;
        }
        self.emitter.emit(
            self.raw,
            ResolutionError::OperationUndefined,
            &self.path.unit,
            ident.span,
            &format!("undefined-operation:{qualified}"),
            format!("`{text}` is neither a core operation nor a DEFINE of kind.operation"),
        );
    }

    /// True when the identifier's first segment names a namespace of this unit
    /// whose source did not load.
    fn namespace_failed(&self, text: &str) -> bool {
        let first = text.split('.').next().unwrap_or(text);
        self.resolved
            .namespaces
            .get(&(self.path.unit.clone(), first.to_string()))
            .is_some_and(|owner| owner.unit.is_none())
    }
}

/// `03_TYPES_AND_VALUES/05`: an alias `BASE` must "resolve transitively and
/// acyclically to exactly one core identifier" of the same domain.
/// `07_VERSIONING_AND_EXTENSIONS/03`: "A BASE that resolves to the wrong domain
/// or DEFINE KIND uses error.reference.kind; an unresolved BASE uses
/// error.reference.unresolved; a cycle uses error.reference.cycle."
fn check_alias_chains(
    resolver: &Resolver<'_>,
    resolved: &Resolved,
    emitter: &Emitter<'_>,
    raw: &mut Vec<Diagnostic>,
) {
    for (index, declaration) in resolved.declarations.all().iter().enumerate() {
        if declaration.block != "DEFINE" {
            continue;
        }
        let Some(kind) = declaration.definition_kind.as_deref() else {
            continue;
        };
        let Some(domain) = resolver.rules().alias_domain(kind) else {
            continue;
        };
        let Some((base, span)) = declaration.base_identifier.clone() else {
            continue;
        };

        let prefixes = declaration.id.namespace_path.clone();
        let qualify = |text: &str| {
            if prefixes.is_empty() {
                text.to_string()
            } else {
                format!("{}.{text}", prefixes.join("."))
            }
        };

        let mut current = base;
        let mut seen: BTreeSet<usize> = [index].into_iter().collect();
        loop {
            if domain.contains(&current) {
                break;
            }
            let qualified = qualify(&current);
            let Some(&next) = resolved.declarations.by_qualified(&qualified).first() else {
                emitter.emit(
                    raw,
                    ResolutionError::ReferenceUnresolved,
                    &declaration.source,
                    span,
                    &format!("alias-base:{}", declaration.id.qualified()),
                    format!("BASE `{current}` is neither a core {kind} identifier nor a DEFINE of that kind"),
                );
                break;
            };
            let target = resolved
                .declarations
                .get(next)
                .expect("index came from the index");
            if target.block != "DEFINE" || target.definition_kind.as_deref() != Some(kind) {
                emitter.emit(
                    raw,
                    ResolutionError::ReferenceKind,
                    &declaration.source,
                    span,
                    &format!("alias-base-kind:{}", declaration.id.qualified()),
                    format!(
                        "BASE `{current}` resolves to a {}, not a DEFINE of {kind}",
                        target.block
                    ),
                );
                break;
            }
            if !seen.insert(next) {
                emitter.emit(
                    raw,
                    ResolutionError::ReferenceCycle,
                    &declaration.source,
                    span,
                    &format!("alias-cycle:{}", declaration.id.qualified()),
                    format!("the BASE chain from `{}` is cyclic", declaration.id.local),
                );
                break;
            }
            let Some((next_base, _)) = target.base_identifier.clone() else {
                emitter.emit(
                    raw,
                    ResolutionError::ReferenceUnresolved,
                    &declaration.source,
                    span,
                    &format!("alias-base:{}", declaration.id.qualified()),
                    format!(
                        "BASE chain from `{}` reaches `{}`, which declares no BASE",
                        declaration.id.local, target.id.local
                    ),
                );
                break;
            };
            current = next_base;
        }
    }
}

/// `#/errors/error.reference.cycle` names "direct HANDLER FALLBACK references"
/// among the prohibited cycles. Decision witness CLOSURE-028: "HANDLER A
/// FALLBACK REF(B), HANDLER B FALLBACK REF(A)" expects "error.reference.cycle
/// before invocation."
fn check_fallback_cycles(
    _resolver: &Resolver<'_>,
    resolved: &Resolved,
    emitter: &Emitter<'_>,
    raw: &mut Vec<Diagnostic>,
) {
    for (index, declaration) in resolved.declarations.all().iter().enumerate() {
        if declaration.block != "HANDLER" || declaration.fallback_reference.is_none() {
            continue;
        }
        let mut seen: BTreeSet<usize> = [index].into_iter().collect();
        let mut current = index;
        while let Some(node) = resolved.declarations.get(current) {
            let Some((target_text, span)) = node.fallback_reference.clone() else {
                break;
            };
            let prefixes = node.id.namespace_path.clone();
            let qualified = if prefixes.is_empty() {
                target_text
            } else {
                format!("{}.{target_text}", prefixes.join("."))
            };
            let Some(&next) = resolved.declarations.by_qualified(&qualified).first() else {
                break;
            };
            if !seen.insert(next) {
                emitter.emit(
                    raw,
                    ResolutionError::ReferenceCycle,
                    &declaration.source,
                    declaration
                        .fallback_reference
                        .as_ref()
                        .map(|(_, s)| *s)
                        .unwrap_or(span),
                    &format!("fallback-cycle:{}", declaration.id.qualified()),
                    format!(
                        "the FALLBACK chain from `{}` returns to an earlier handler",
                        declaration.id.local
                    ),
                );
                break;
            }
            current = next;
        }
    }
}
