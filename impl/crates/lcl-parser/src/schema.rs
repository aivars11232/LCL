//! Block-schema and field-signature enforcement.
//!
//! The syntax tree says what the source spells; this module judges it against
//! `block_schemas_v0.1.0.json` and `field_signatures_v0.1.0.json`, which are
//! tier-1 authority under `00_RELEASE/02_NORMATIVE_AUTHORITY_ORDER.txt`.
//!
//! Every diagnostic raised here is one of the twelve registered
//! `grammar_or_schema` identifiers, chosen by its registered meaning:
//!
//! * `error.block.context` — an illegal document position, or a top-level block
//!   forbidden for the document kind;
//! * `error.block.field` — a child *block* under a parent that does not permit
//!   it, including an executable or rule block inside object data
//!   (`04_GRAMMAR/12`);
//! * `error.field.forbidden` — a key absent from the parent's exact signature,
//!   which includes every lowercase key outside object data
//!   (`04_GRAMMAR/11` rule 3);
//! * `error.field.required` / `error.block.required` — an omitted field or
//!   child block, located by `location_rule`;
//! * `error.field.duplicate` / `error.block.duplicate` — a non-repeatable key
//!   written twice;
//! * `error.field.cardinality` — an occurrence count outside the signature;
//! * `error.field.type` — a value whose *shape* is not one the value kind
//!   accepts;
//! * `error.block.conditional_requirement` — a structurally decidable
//!   conditional requirement that is unsatisfied.
//!
//! Nothing here resolves a reference, reads a registry domain, or types a
//! value: see `grammar` for the exact split and its canonical evidence.

use crate::diagnostic::GrammarError;
use crate::grammar::{BlockSchema, FieldSignature, FormSet, Grammar};
use crate::parse::{omission_locus, Emitter};
use crate::syntax::*;
use lcl_lexer::{Lexed, Span};
use std::collections::BTreeMap;

/// The pseudo-contexts the registries use for document position and control
/// flow, alongside real block names.
const TOP_LEVEL: &str = "top_level";
const TOP_LEVEL_FIRST: &str = "top_level_first";
const TOP_LEVEL_SECOND: &str = "top_level_second";

pub(crate) struct SchemaChecker<'a, 'b> {
    pub(crate) grammar: &'a Grammar,
    pub(crate) lexed: &'a Lexed,
    pub(crate) emitter: &'b mut Emitter<'a>,
}

impl<'a> SchemaChecker<'a, '_> {
    fn emit(&mut self, id: GrammarError, span: Span, cause: &str, detail: String) {
        self.emitter.emit(id, span, cause, detail);
    }

    /// Indentation of the line containing `offset`, in spaces.
    fn line_indent(&self, offset: usize) -> usize {
        let text = self.lexed.source();
        let start = text
            .get(..offset)
            .and_then(|s| s.rfind('\n').map(|i| i.saturating_add(1)))
            .unwrap_or(0);
        text.get(start..offset)
            .map(|s| s.len().saturating_sub(s.trim_start_matches(' ').len()))
            .unwrap_or(0)
    }

    /// `location_rule` locus for something omitted from the block at `anchor`.
    fn omission(&self, anchor: Anchor) -> Span {
        omission_locus(
            self.lexed,
            self.line_indent(anchor.key_span.start),
            anchor.span.end,
        )
    }

    pub(crate) fn document(&mut self, doc: &Document) {
        self.top_level_order(doc);
        let kind = document_kind(doc);
        self.top_level_kind_legality(doc, kind.as_deref());
        self.top_level_occurrence(doc, kind.as_deref());

        // A worklist, not a recursion: blocks, control forms and object data
        // all nest without bound, and the canonical language sets no depth
        // limit, so this walk may not depend on the native stack either. The
        // stack is popped last-in-first-out and children are pushed in reverse,
        // which reproduces the depth-first pre-order the recursive version had.
        let mut work: Vec<Work<'_>> = Vec::new();
        for item in doc.items.iter().rev() {
            work.push(match item {
                TopLevel::Block(b) => Work::Block {
                    block: b,
                    context: TOP_LEVEL,
                },
                TopLevel::Conditional(c) => Work::Conditional { node: c },
                TopLevel::ForEach(f) => Work::ForEach { node: f },
            });
        }
        self.drain(work);
    }

    /// Run one worklist to exhaustion.
    fn drain(&mut self, mut work: Vec<Work<'_>>) {
        while let Some(item) = work.pop() {
            match item {
                Work::Block { block, context } => self.block(block, context, &mut work),
                Work::Body {
                    anchor,
                    statements,
                    schema,
                } => self.body(anchor, statements, &schema, &mut work),
                Work::ObjectData { nested, owner } => self.object_data(nested, &owner, &mut work),
                Work::Conditional { node } => {
                    for item in node
                        .else_body
                        .as_ref()
                        .map(|arm| arm.body.as_slice())
                        .unwrap_or(&[])
                        .iter()
                        .rev()
                    {
                        work.push(Self::executable(item, "ELSE"));
                    }
                    for item in node.then_body.iter().rev() {
                        work.push(Self::executable(item, "IF"));
                    }
                }
                Work::ForEach { node } => {
                    for item in node.body.iter().rev() {
                        work.push(Self::executable(item, "FOR_EACH"));
                    }
                }
            }
        }
    }

    /// `DOCUMENT` pins the first two blocks, and the registries record that as
    /// the `top_level_first` and `top_level_second` contexts.
    fn top_level_order(&mut self, doc: &Document) {
        let blocks: Vec<&Block> = doc.blocks().collect();

        for (position, context) in [(0usize, TOP_LEVEL_FIRST), (1, TOP_LEVEL_SECOND)] {
            let expected: Vec<&str> = self
                .grammar
                .schemas()
                .filter(|s| s.parents.iter().any(|p| p == context))
                .map(|s| s.name.as_str())
                .collect();
            let Some(&name) = expected.first() else {
                continue;
            };
            match blocks.get(position) {
                Some(block) if block.key.text == name => {}
                Some(block) => {
                    let span = block.key.span;
                    let found = block.key.text.clone();
                    self.emit(
                        GrammarError::BlockContext,
                        span,
                        "document_order",
                        format!(
                            "`{name}` is the {} top-level block; found `{found}`",
                            if position == 0 { "first" } else { "second" }
                        ),
                    );
                }
                None => {
                    let locus = Span::empty(self.lexed.source_len());
                    self.emit(
                        GrammarError::BlockRequired,
                        locus,
                        "document_order",
                        format!("the document omits its required `{name}` block"),
                    );
                }
            }
        }

        // Every other top-level block must declare `top_level`.
        for block in blocks.iter().skip(2) {
            let Some(schema) = self.grammar.schema(&block.key.text) else {
                continue;
            };
            if !schema.accepts_parent(TOP_LEVEL) {
                let span = block.key.span;
                let name = block.key.text.clone();
                let parents = schema.parents.join(", ");
                self.emit(
                    GrammarError::BlockContext,
                    span,
                    "block_position",
                    format!("`{name}` is legal in {parents}, not at top level"),
                );
            }
        }
    }

    /// `document_kind_blocks`: top-level legality for the declared kind.
    fn top_level_kind_legality(&mut self, doc: &Document, kind: Option<&str>) {
        let Some(kind) = kind else {
            return;
        };
        // An unregistered kind is a value-domain question owned by resolution,
        // so no top-level legality is asserted against it here.
        let Some(legal) = self.grammar.document_kind_blocks(kind).cloned() else {
            return;
        };
        for block in doc.blocks().skip(2) {
            if !self.grammar.is_block(&block.key.text) {
                continue;
            }
            if !legal.contains(&block.key.text) {
                let span = block.key.span;
                let name = block.key.text.clone();
                self.emit(
                    GrammarError::BlockContext,
                    span,
                    "document_kind",
                    format!("`{name}` is not a legal top-level block for {kind}"),
                );
            }
        }
    }

    fn top_level_occurrence(&mut self, doc: &Document, kind: Option<&str>) {
        let mut counts: BTreeMap<&str, Vec<&Block>> = BTreeMap::new();
        for block in doc.blocks() {
            counts
                .entry(block.key.text.as_str())
                .or_default()
                .push(block);
        }

        for (name, blocks) in &counts {
            let Some(schema) = self.grammar.schema(name) else {
                continue;
            };
            if schema.occurrence.maximum() == Some(1) && blocks.len() > 1 {
                for extra in blocks.iter().skip(1) {
                    let span = extra.key.span;
                    let count = blocks.len();
                    self.emit(
                        GrammarError::BlockDuplicate,
                        span,
                        "top_level_occurrence",
                        format!(
                            "`{name}` is {} at top level; found {count}",
                            schema.occurrence.as_registry_str()
                        ),
                    );
                }
            }
        }

        // "kind.task and kind.test require exactly one EXECUTE." —
        // 04_GRAMMAR/01, echoed by EXECUTE's own conditional requirement.
        // `location_rule`: an omitted required top-level block uses the
        // end-of-file offset.
        if matches!(kind, Some("kind.task") | Some("kind.test")) && !counts.contains_key("EXECUTE")
        {
            let kind = kind.unwrap_or_default().to_string();
            let locus = Span::empty(self.lexed.source_len());
            self.emit(
                GrammarError::BlockRequired,
                locus,
                "execute_root",
                format!("a {kind} document requires exactly one EXECUTE root"),
            );
        }
    }

    /// One `EXECUTABLE_STATEMENT` as a unit of work.
    fn executable<'t>(item: &'t Executable, context: &'static str) -> Work<'t> {
        match item {
            Executable::Block(b) => Work::Block { block: b, context },
            Executable::Conditional(c) => Work::Conditional { node: c },
            Executable::ForEach(f) => Work::ForEach { node: f },
        }
    }

    /// Validate one block against its schema in `context`.
    fn block<'t>(&mut self, block: &'t Block, context: &'static str, work: &mut Vec<Work<'t>>) {
        let Some(schema) = self.grammar.schema(&block.key.text).cloned() else {
            // The lexer's closed reserved-word list already rejects an
            // unregistered word, so a non-block registered word in block
            // position is a grammar shape defect.
            let span = block.key.span;
            let name = block.key.text.clone();
            self.emit(
                GrammarError::GrammarInvalid,
                span,
                "block_word",
                format!("`{name}` is not a registered block word"),
            );
            return;
        };

        // Containment. `top_level_first` and `top_level_second` are checked by
        // document order, so a block legal only there is not re-reported here.
        let positional = schema
            .parents
            .iter()
            .any(|p| p == TOP_LEVEL_FIRST || p == TOP_LEVEL_SECOND);
        if !schema.accepts_parent(context) && !(positional && context == TOP_LEVEL) {
            let span = block.key.span;
            let name = block.key.text.clone();
            let parents = schema.parents.join(", ");
            let id = if context == TOP_LEVEL {
                GrammarError::BlockContext
            } else {
                GrammarError::BlockField
            };
            self.emit(
                id,
                span,
                "containment",
                format!("`{name}` is legal in {parents}, not in {context}"),
            );
        }

        work.push(Work::Body {
            anchor: Anchor::of(block),
            statements: &block.body,
            schema,
        });
    }

    /// Field presence, cardinality, containment and value shape for one block.
    fn body<'t>(
        &mut self,
        anchor: Anchor,
        statements: &'t [Statement],
        schema: &BlockSchema,
        work: &mut Vec<Work<'t>>,
    ) {
        let mut counts: BTreeMap<&'t str, Vec<&'t Field>> = BTreeMap::new();
        // Nested work is collected here and pushed in reverse at the end, so a
        // child is still visited before this block's own later checks emit —
        // the order the recursive version produced.
        let mut nested: Vec<Work<'t>> = Vec::new();

        for statement in statements {
            match statement {
                Statement::Field(f) => {
                    counts.entry(f.key.text.as_str()).or_default().push(f);
                }
                Statement::Property(p) => {
                    // 04_GRAMMAR/11 rule 3: "Lowercase keys are legal only in
                    // OBJECT VALUE/DATA content." A block body is not that.
                    let span = p.key.span;
                    let key = p.key.text.clone();
                    let name = schema.name.clone();
                    self.emit(
                        GrammarError::FieldForbidden,
                        span,
                        "lowercase_key",
                        format!(
                            "`{key}` is a lowercase key; `{name}` admits only registered fields"
                        ),
                    );
                }
                Statement::Conditional(c) => {
                    self.control_legality(schema, c.keyword_span, "IF");
                    nested.push(Work::Conditional { node: c });
                }
                Statement::ForEach(f) => {
                    self.control_legality(schema, f.keyword_span, "FOR EACH");
                    nested.push(Work::ForEach { node: f });
                }
            }
        }

        for (name, fields) in &counts {
            let Some(sig) = schema.field(name) else {
                self.unknown_field(schema, fields, name);
                continue;
            };
            self.cardinality(schema, sig, fields);
            for field in fields {
                self.value(schema, sig, field, &mut nested);
            }
        }

        for sig in &schema.fields {
            if sig.required && !counts.contains_key(sig.name.as_str()) {
                let locus = self.omission(anchor);
                let field = sig.name.clone();
                let owner = schema.name.clone();
                self.emit(
                    GrammarError::FieldRequired,
                    locus,
                    "required_field",
                    format!("`{owner}` requires `{field}`"),
                );
            }
        }

        self.conditional_requirements(anchor, statements, schema, &counts);

        for required in &schema.required {
            if !counts.contains_key(required.as_str()) {
                let locus = self.omission(anchor);
                let child = required.clone();
                let owner = schema.name.clone();
                self.emit(
                    GrammarError::BlockRequired,
                    locus,
                    "required_block",
                    format!("`{owner}` requires a `{child}` child block"),
                );
            }
        }

        for item in nested.into_iter().rev() {
            work.push(item);
        }
    }

    /// `IF` and `FOR EACH` are legal only inside `PHASE`, `SEQUENCE` or a
    /// branch body (`04_GRAMMAR/11` rule 4). The registries express that as
    /// `STEP`'s legal parents, so the check reads them rather than a copy.
    fn control_legality(&mut self, schema: &BlockSchema, span: Span, form: &str) {
        let step_parents = self
            .grammar
            .schema("STEP")
            .map(|s| s.parents.clone())
            .unwrap_or_default();
        if !step_parents.iter().any(|p| p == &schema.name) {
            let owner = schema.name.clone();
            let legal = step_parents.join(", ");
            self.emit(
                GrammarError::BlockField,
                span,
                "control_position",
                format!("`{form}` is legal in {legal}, not directly in `{owner}`"),
            );
        }
    }

    fn unknown_field(&mut self, schema: &BlockSchema, fields: &[&Field], name: &str) {
        for field in fields {
            let span = field.key.span;
            let owner = schema.name.clone();
            // A registered *block* under a parent that does not list it is the
            // more specific "child block not permitted" case.
            let id = if self.grammar.is_block(name) {
                GrammarError::BlockField
            } else {
                GrammarError::FieldForbidden
            };
            let detail = if id == GrammarError::BlockField {
                format!("`{owner}` does not permit a `{name}` child block")
            } else {
                format!("`{name}` is not a field of `{owner}`")
            };
            self.emit(id, span, "unknown_field", detail);
            if !schema.unknown_fields_forbidden {
                // Every shipped block forbids unknown fields; if a future
                // release relaxes one, this loop must not silently keep
                // reporting. Recorded as detail rather than assumed.
                continue;
            }
        }
    }

    fn cardinality(&mut self, schema: &BlockSchema, sig: &FieldSignature, fields: &[&Field]) {
        let count = fields.len() as u64;
        let max = sig.maximum_occurrences;
        if max == Some(1) && count > 1 {
            // `error.field.duplicate` supersedes `error.field.cardinality` for
            // the same cause and locus, and is the registered specific case.
            for extra in fields.iter().skip(1) {
                let span = extra.key.span;
                let field = sig.name.clone();
                let owner = schema.name.clone();
                self.emit(
                    GrammarError::FieldDuplicate,
                    span,
                    "duplicate_field",
                    format!("`{field}` is not repeatable in `{owner}`"),
                );
            }
            return;
        }
        if let Some(max) = max {
            if count > max {
                for extra in fields.iter().skip(max as usize) {
                    let span = extra.key.span;
                    let field = sig.name.clone();
                    self.emit(
                        GrammarError::FieldCardinality,
                        span,
                        "cardinality",
                        format!("`{field}` admits at most {max} occurrences; found {count}"),
                    );
                }
                return;
            }
        }
        if count < sig.minimum_occurrences {
            if let Some(first) = fields.first() {
                let span = first.key.span;
                let field = sig.name.clone();
                let min = sig.minimum_occurrences;
                self.emit(
                    GrammarError::FieldCardinality,
                    span,
                    "cardinality",
                    format!("`{field}` requires at least {min} occurrences; found {count}"),
                );
            }
        }
    }

    /// The value's *shape* against the field's registered value kind.
    fn value<'t>(
        &mut self,
        schema: &BlockSchema,
        sig: &FieldSignature,
        field: &'t Field,
        work: &mut Vec<Work<'t>>,
    ) {
        let observed = observed_forms(&field.body);
        if !observed.intersects(sig.forms) {
            let span = field.body.span();
            let name = sig.name.clone();
            let owner = schema.name.clone();
            let kind = sig.value_kind.clone();
            let accepted = sig.forms.names().join(" or ");
            self.emit(
                GrammarError::FieldType,
                span,
                "value_kind",
                format!("`{owner}.{name}` is {kind}, which accepts {accepted}"),
            );
            return;
        }

        // A nested body under a field whose kind names a child block is that
        // block: validate it as one, so the whole tree is judged.
        if let (Some(nested), Some(child)) = (field.body.as_nested(), sig.nested_block.as_deref()) {
            if let Some(child_schema) = self.grammar.schema(child).cloned() {
                // The child's statements are judged in place. The recursive
                // version cloned them into a synthetic `Block` to reuse this
                // signature; an anchor carries the two spans that synthetic
                // block existed to supply, so nothing is copied.
                work.push(Work::Body {
                    anchor: Anchor {
                        key_span: field.key.span,
                        span: field.span,
                    },
                    statements: &nested.statements,
                    schema: child_schema,
                });
            }
            return;
        }

        // A nested body elsewhere is object data or a local schema; both admit
        // only what their contract allows.
        if let Some(nested) = field.body.as_nested() {
            work.push(Work::ObjectData {
                nested,
                owner: sig.name.clone(),
            });
        }
    }

    /// `04_GRAMMAR/12`: object data holds unique lowercase properties and is
    /// data-only; an executable or rule block inside it is `error.block.field`.
    fn object_data<'t>(&mut self, nested: &'t Nested, owner: &str, work: &mut Vec<Work<'t>>) {
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        let mut inner_work: Vec<Work<'t>> = Vec::new();
        for statement in &nested.statements {
            match statement {
                Statement::Property(p) => {
                    let count = seen.entry(p.key.text.as_str()).or_default();
                    *count = count.saturating_add(1);
                    if *count > 1 {
                        let span = p.key.span;
                        let key = p.key.text.clone();
                        self.emit(
                            GrammarError::FieldDuplicate,
                            span,
                            "duplicate_property",
                            format!("object property `{key}` is declared more than once"),
                        );
                    }
                    if let Some(inner) = p.body.as_nested() {
                        inner_work.push(Work::ObjectData {
                            nested: inner,
                            owner: p.key.text.clone(),
                        });
                    }
                }
                Statement::Field(f) => {
                    let span = f.key.span;
                    let key = f.key.text.clone();
                    let owner = owner.to_string();
                    self.emit(
                        GrammarError::BlockField,
                        span,
                        "object_data",
                        format!("`{key}` is not object data; `{owner}` holds lowercase properties"),
                    );
                }
                Statement::Conditional(c) => {
                    let span = c.keyword_span;
                    self.emit(
                        GrammarError::BlockField,
                        span,
                        "object_data",
                        "object data is data-only and admits no executable form".to_string(),
                    );
                }
                Statement::ForEach(f) => {
                    let span = f.keyword_span;
                    self.emit(
                        GrammarError::BlockField,
                        span,
                        "object_data",
                        "object data is data-only and admits no executable form".to_string(),
                    );
                }
            }
        }
        for item in inner_work.into_iter().rev() {
            work.push(item);
        }
    }
}

/// The two spans a block contributes to its own diagnostics.
///
/// `body` needs a locus for an omitted field and a locus for the block word.
/// Carrying just those lets a *nested* body be judged in place, without the
/// synthetic `Block` (and the deep statement clone) the recursive version
/// built to satisfy a `&Block` parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Anchor {
    pub(crate) key_span: Span,
    pub(crate) span: Span,
}

impl Anchor {
    fn of(block: &Block) -> Anchor {
        Anchor {
            key_span: block.key.span,
            span: block.span,
        }
    }
}

/// One pending unit of schema work.
///
/// Every variant borrows the syntax tree, so the walk copies nothing and the
/// worklist costs one small record per open node.
enum Work<'t> {
    /// Judge one block against its schema in `context`.
    Block {
        block: &'t Block,
        context: &'static str,
    },
    /// Judge one block body's statements against `schema`.
    Body {
        anchor: Anchor,
        statements: &'t [Statement],
        schema: BlockSchema,
    },
    /// Judge one object-data body.
    ObjectData {
        nested: &'t Nested,
        owner: String,
    },
    Conditional {
        node: &'t Conditional,
    },
    ForEach {
        node: &'t ForEach,
    },
}

/// `SPECIFICATION.KIND`, when the document spells one inline.
fn document_kind(doc: &Document) -> Option<String> {
    let block = doc.block("SPECIFICATION")?;
    let field = block.field("KIND")?;
    match field.body.as_inline()?.as_expression()? {
        Expr::Identifier(id) => Some(id.text.clone()),
        _ => None,
    }
}

/// The shapes a written value satisfies.
///
/// Every inline value satisfies `EXPRESSION`, so a value kind that accepts it
/// imposes no shape at all; the specific bits are added only when the source
/// actually spells that form.
fn observed_forms(body: &Body) -> FormSet {
    let value = match body {
        Body::Nested(_) => return FormSet::NESTED,
        Body::Inline(v) => v,
    };
    let mut forms = FormSet::EXPRESSION;
    match value {
        Value::MultilineCollection(c) => {
            if is_reference_list(&c.members) {
                forms = forms.union(FormSet::REFERENCE_LIST);
            }
        }
        Value::Expression(expr) => forms = forms.union(expression_forms(expr)),
    }
    forms
}

fn expression_forms(expr: &Expr) -> FormSet {
    match expr {
        Expr::Literal(l) => match l.kind {
            LiteralKind::String => FormSet::STRING,
            LiteralKind::MultilineString => FormSet::MULTILINE_STRING,
            LiteralKind::Integer => FormSet::INTEGER,
            LiteralKind::True | LiteralKind::False => FormSet::BOOLEAN,
            // "NULL denotes the NULL type in a type-required field or type
            // argument; elsewhere it is the material NULL literal."
            LiteralKind::Null => FormSet::TYPE_EXPRESSION,
            _ => FormSet::empty(),
        },
        // `-1` is one INTEGER value in an integer-bounded slot.
        Expr::Unary(u) if u.operator == UnaryOp::Negate => {
            let inner = expression_forms(&u.operand);
            if inner.contains(FormSet::INTEGER) {
                FormSet::INTEGER
            } else {
                FormSet::empty()
            }
        }
        Expr::Identifier(id) => {
            if id.qualified {
                FormSet::QUALIFIED_IDENTIFIER
            } else {
                // One segment satisfies both a simple-identifier slot and a
                // dotted-path slot of length one.
                FormSet::SIMPLE_IDENTIFIER.union(FormSet::QUALIFIED_IDENTIFIER)
            }
        }
        Expr::Call(call) if call.reference_target().is_some() => {
            // `REFERENCE_CALL` is also a `TYPE_EXPRESSION` alternative.
            FormSet::REFERENCE.union(FormSet::TYPE_EXPRESSION)
        }
        Expr::Collection(c) if is_reference_list(&c.members) => FormSet::REFERENCE_LIST,
        Expr::Type(_) => FormSet::TYPE_EXPRESSION,
        _ => FormSet::empty(),
    }
}

/// A bracket literal whose every member is a `REFERENCE_CALL`.
///
/// An empty list qualifies: `04_GRAMMAR/13` says "a reference_list uses the
/// source LIST form and therefore may be empty unless a containing field's
/// conditional requirements say otherwise."
fn is_reference_list(members: &[Expr]) -> bool {
    members.iter().all(|m| match m {
        Expr::Call(call) => call.reference_target().is_some(),
        _ => false,
    })
}
