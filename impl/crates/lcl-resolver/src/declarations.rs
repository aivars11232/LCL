//! Declaration identity: the index of every ID a program declares.
//!
//! `02_LEXICAL/03_IDENTIFIERS_NAMESPACES_AND_RESERVED_NAMES.txt`: "IDs are
//! unique within one namespace." A declaration's identity is therefore a pair —
//! the namespace it lives in, and its document-local ID — never the local text
//! alone.
//!
//! `07_VERSIONING_AND_EXTENSIONS/02`: "The importing prefix is prepended to the
//! imported declaration's full source ID; nested imports prepend each prefix in
//! order." [`FullId::namespace_path`] is that ordered prefix chain, which is
//! why it is a list and not one string: a declaration reached through two
//! imports carries both prefixes, outermost first.

use lcl_lexer::Span;
use std::collections::BTreeMap;
use std::fmt;

use crate::source::SourceId;

/// A declaration's exact identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FullId {
    /// Importing namespace prefixes, outermost first. Empty for a declaration
    /// of the root unit itself.
    pub namespace_path: Vec<String>,
    /// The document-local ID exactly as written in its declaring document.
    pub local: String,
}

impl FullId {
    pub fn local(local: impl Into<String>) -> Self {
        FullId {
            namespace_path: Vec::new(),
            local: local.into(),
        }
    }

    /// This identity as seen from one further importing prefix.
    pub fn prefixed(&self, prefix: &str) -> FullId {
        let mut namespace_path = Vec::with_capacity(self.namespace_path.len() + 1);
        namespace_path.push(prefix.to_string());
        namespace_path.extend(self.namespace_path.iter().cloned());
        FullId {
            namespace_path,
            local: self.local.clone(),
        }
    }

    /// The dotted spelling a reference uses: `namespace.local`.
    pub fn qualified(&self) -> String {
        if self.namespace_path.is_empty() {
            return self.local.clone();
        }
        format!("{}.{}", self.namespace_path.join("."), self.local)
    }

    /// The first identifier segment of the local ID, which namespace ownership
    /// rules govern.
    pub fn first_segment(&self) -> &str {
        self.local.split('.').next().unwrap_or(&self.local)
    }
}

impl fmt::Display for FullId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.qualified())
    }
}

/// One declaration.
#[derive(Debug, Clone)]
pub struct Declaration {
    /// Exact identity.
    pub id: FullId,
    /// The unit that declares it.
    pub source: SourceId,
    /// The block that declares it, e.g. `ACTION`.
    pub block: String,
    /// `DEFINE.KIND`, for a `DEFINE` declaration only.
    pub definition_kind: Option<String>,
    /// Locus of the `ID` value.
    pub id_span: Span,
    /// Locus of the declaring block's header word.
    pub block_span: Span,
    /// Index of the enclosing declaration, for a nested block such as a `STEP`
    /// inside a `SEQUENCE` or an `ACTION` inside a `STEP`.
    pub parent: Option<usize>,
    /// `BASE` written as a bare qualified identifier, as an alias `DEFINE` of
    /// `kind.error`, `kind.event`, `kind.status` or `kind.format` writes it.
    /// `03_TYPES_AND_VALUES/05`: "BASE names one core identifier of the same
    /// domain or one acyclic same-kind alias."
    pub base_identifier: Option<(String, Span)>,
    /// `BASE` written as exactly `REF(identifier)`, as a `kind.type` alias
    /// writes it.
    pub base_reference: Option<(String, Span)>,
    /// `HANDLER.FALLBACK` written as exactly `REF(identifier)`.
    /// `#/errors/error.reference.cycle` names "direct HANDLER FALLBACK
    /// references" as one of the cycles this stage rejects.
    pub fallback_reference: Option<(String, Span)>,
}

/// Every declaration of every usable unit, indexed for exact lookup.
///
/// Every map is ordered, so a lookup that finds several declarations reports
/// them in a deterministic order and `error.id.duplicate` never depends on
/// which one was inserted first.
#[derive(Debug, Default)]
pub struct DeclarationIndex {
    decls: Vec<Declaration>,
    /// Fully qualified spelling -> declaration indexes.
    by_qualified: BTreeMap<String, Vec<usize>>,
    /// `(declaring unit, local ID)` -> declaration indexes. Unqualified
    /// resolution uses this: "Unqualified REF resolves local namespace only."
    by_local: BTreeMap<(SourceId, String), Vec<usize>>,
    /// `(declaring unit, block header offset)` -> declaration index.
    by_span: BTreeMap<(SourceId, usize), usize>,
}

impl DeclarationIndex {
    pub fn len(&self) -> usize {
        self.decls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    pub fn all(&self) -> &[Declaration] {
        &self.decls
    }

    pub fn get(&self, index: usize) -> Option<&Declaration> {
        self.decls.get(index)
    }

    /// Declarations under one fully qualified spelling.
    pub fn by_qualified(&self, qualified: &str) -> &[usize] {
        self.by_qualified
            .get(qualified)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The declaration whose block header starts at one byte offset.
    ///
    /// The graph layer uses it to move from a nested child block in the syntax
    /// tree to the identity that block declares.
    pub fn by_span(&self, source: &SourceId, offset: usize) -> Option<&usize> {
        self.by_span.get(&(source.clone(), offset))
    }

    /// Declarations with one local ID inside one unit.
    pub fn by_local(&self, source: &SourceId, local: &str) -> &[usize] {
        self.by_local
            .get(&(source.clone(), local.to_string()))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn push(&mut self, decl: Declaration) -> usize {
        let index = self.decls.len();
        self.by_qualified
            .entry(decl.id.qualified())
            .or_default()
            .push(index);
        self.by_local
            .entry((decl.source.clone(), decl.id.local.clone()))
            .or_default()
            .push(index);
        self.by_span
            .entry((decl.source.clone(), decl.block_span.start))
            .or_insert(index);
        self.decls.push(decl);
        index
    }
}

/// Index every declaration of every usable unit path, then enforce identity
/// uniqueness and the reserved-namespace rule.
pub(crate) fn index(
    resolver: &crate::Resolver<'_>,
    resolved: &mut crate::Resolved,
    raw: &mut Vec<crate::Diagnostic>,
) {
    let mut found: Vec<Declaration> = Vec::new();
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
        let mut collector = Collector {
            grammar: resolver.grammar(),
            rules: resolver.rules(),
            path,
            found: &mut found,
        };
        collector.top_level(&document.items);
    }

    let emitter = crate::Emitter::new(resolver.rules(), &resolved.units);
    check_reserved_namespaces(resolver, &found, &emitter, raw);
    check_duplicate_ids(&found, &emitter, raw);

    for decl in found {
        resolved.declarations.push(decl);
    }
}

/// Walks one document, collecting every declaration it makes.
struct Collector<'a> {
    grammar: &'a lcl_parser::Grammar,
    rules: &'a crate::Rules,
    path: &'a crate::UnitPath,
    found: &'a mut Vec<Declaration>,
}

impl Collector<'_> {
    fn top_level(&mut self, items: &[lcl_parser::syntax::TopLevel]) {
        use lcl_parser::syntax::TopLevel;
        for item in items {
            match item {
                TopLevel::Block(b) => self.block(b, None),
                TopLevel::Conditional(c) => self.conditional(c, None),
                TopLevel::ForEach(f) => self.for_each(f, None),
            }
        }
    }

    /// Record `block` if it declares an identity, then descend.
    fn block(&mut self, block: &lcl_parser::syntax::Block, parent: Option<usize>) {
        let name = block.key.text.as_str();
        let mut here = parent;
        if self.rules.is_declaring_block(name) {
            if let Some((local, id_span)) = identifier_field(block, "ID") {
                let definition_kind = (name == "DEFINE")
                    .then(|| identifier_field(block, "KIND").map(|(k, _)| k))
                    .flatten();
                let (base_identifier, base_reference, fallback_reference) =
                    links(name, &block.body);
                self.found.push(Declaration {
                    id: self.path.qualify(&local),
                    source: self.path.unit.clone(),
                    block: name.to_string(),
                    definition_kind,
                    id_span,
                    block_span: block.key.span,
                    parent,
                    base_identifier,
                    base_reference,
                    fallback_reference,
                });
                here = Some(self.found.len() - 1);
            }
        }
        self.statements(name, &block.body, here);
    }

    /// Descend through one block body, following nested child blocks and
    /// control forms.
    fn statements(
        &mut self,
        block_name: &str,
        body: &[lcl_parser::syntax::Statement],
        parent: Option<usize>,
    ) {
        use lcl_parser::syntax::Statement;
        for statement in body {
            match statement {
                Statement::Field(field) => {
                    // A field whose registered value kind names a nested child
                    // block, written in its nested form, *is* that child block.
                    let child = self
                        .grammar
                        .schema(block_name)
                        .and_then(|s| s.field(&field.key.text))
                        .and_then(|f| f.nested_block.as_deref());
                    if let (Some(child), Some(nested)) = (child, field.body.as_nested()) {
                        self.child_block(child, field, nested, parent);
                    }
                }
                Statement::Property(_) => {}
                Statement::Conditional(c) => self.conditional(c, parent),
                Statement::ForEach(f) => self.for_each(f, parent),
            }
        }
    }

    /// One child block written in a field's nested body.
    fn child_block(
        &mut self,
        child: &str,
        field: &lcl_parser::syntax::Field,
        nested: &lcl_parser::syntax::Nested,
        parent: Option<usize>,
    ) {
        let mut here = parent;
        if self.rules.is_declaring_block(child) {
            if let Some((local, id_span)) = identifier_statement(&nested.statements, "ID") {
                let definition_kind = (child == "DEFINE")
                    .then(|| identifier_statement(&nested.statements, "KIND").map(|(k, _)| k))
                    .flatten();
                let (base_identifier, base_reference, fallback_reference) =
                    links(child, &nested.statements);
                self.found.push(Declaration {
                    id: self.path.qualify(&local),
                    source: self.path.unit.clone(),
                    block: child.to_string(),
                    definition_kind,
                    id_span,
                    block_span: field.key.span,
                    parent,
                    base_identifier,
                    base_reference,
                    fallback_reference,
                });
                here = Some(self.found.len() - 1);
            }
        }
        self.statements(child, &nested.statements, here);
    }

    fn conditional(&mut self, c: &lcl_parser::syntax::Conditional, parent: Option<usize>) {
        // Both arms are source templates: 05_SEMANTICS/01 requires including
        // "both IF branches ... as source templates for static checking", so
        // both contribute declarations regardless of any condition.
        self.executables(&c.then_body, parent);
        if let Some(arm) = &c.else_body {
            self.executables(&arm.body, parent);
        }
    }

    fn for_each(&mut self, f: &lcl_parser::syntax::ForEach, parent: Option<usize>) {
        self.executables(&f.body, parent);
    }

    fn executables(&mut self, body: &[lcl_parser::syntax::Executable], parent: Option<usize>) {
        use lcl_parser::syntax::Executable;
        for item in body {
            match item {
                Executable::Block(b) => self.block(b, parent),
                Executable::Conditional(c) => self.conditional(c, parent),
                Executable::ForEach(f) => self.for_each(f, parent),
            }
        }
    }
}

/// One identifier written in a field, with its exact locus.
type Link = Option<(String, Span)>;

/// The declaration links this stage must be able to follow without revisiting
/// the syntax tree: alias `BASE` chains and direct `HANDLER FALLBACK` calls.
fn links(block: &str, statements: &[lcl_parser::syntax::Statement]) -> (Link, Link, Link) {
    let base_identifier = crate::field::statement_identifier(statements, "BASE");
    let base_reference =
        crate::field::statement_expression(statements, "BASE").and_then(reference_argument);
    let fallback_reference = (block == "HANDLER")
        .then(|| crate::field::statement_expression(statements, "FALLBACK"))
        .flatten()
        .and_then(reference_argument);
    (base_identifier, base_reference, fallback_reference)
}

/// The identifier of a bare `REF(identifier)` expression.
pub(crate) fn reference_argument(expr: &lcl_parser::syntax::Expr) -> Option<(String, Span)> {
    use lcl_parser::syntax::Expr;
    match expr {
        Expr::Call(call) if call.callable.text == "REF" => match call.arguments.as_slice() {
            [Expr::Identifier(ident)] => Some((ident.text.clone(), ident.span)),
            _ => None,
        },
        Expr::Group(group) => reference_argument(&group.inner),
        _ => None,
    }
}

/// The inline identifier value of one direct field of `block`.
fn identifier_field(block: &lcl_parser::syntax::Block, key: &str) -> Option<(String, Span)> {
    identifier_statement(&block.body, key)
}

fn identifier_statement(
    statements: &[lcl_parser::syntax::Statement],
    key: &str,
) -> Option<(String, Span)> {
    crate::field::statement_identifier(statements, key)
}

/// `02_LEXICAL/03`: "The built-in namespaces core, encoding, error, event,
/// format, kind, mode, status, and unit are reserved. User identifiers cannot
/// begin with those namespaces."
///
/// `#/errors/error.namespace.invalid`: "... or a declaration identifier begins
/// with a reserved namespace."
fn check_reserved_namespaces(
    resolver: &crate::Resolver<'_>,
    found: &[Declaration],
    emitter: &crate::Emitter<'_>,
    raw: &mut Vec<crate::Diagnostic>,
) {
    for decl in found {
        let first = decl.id.first_segment();
        if resolver.rules().is_reserved_namespace(first) {
            emitter.emit(
                raw,
                crate::ResolutionError::NamespaceInvalid,
                &decl.source,
                decl.id_span,
                &format!("reserved-prefix-id:{}", decl.id.local),
                format!(
                    "declaration ID `{}` begins with the reserved namespace `{first}`",
                    decl.id.local
                ),
            );
        }
    }
}

/// `02_LEXICAL/03`: "IDs are unique within one namespace." The identity that
/// must be unique is the fully qualified one, so a local ID repeated in two
/// different imported namespaces is not a duplicate.
fn check_duplicate_ids(
    found: &[Declaration],
    emitter: &crate::Emitter<'_>,
    raw: &mut Vec<crate::Diagnostic>,
) {
    let mut seen: BTreeMap<String, &Declaration> = BTreeMap::new();
    for decl in found {
        let qualified = decl.id.qualified();
        match seen.get(&qualified) {
            None => {
                seen.insert(qualified, decl);
            }
            Some(first) => {
                emitter.emit(
                    raw,
                    crate::ResolutionError::IdDuplicate,
                    &decl.source,
                    decl.id_span,
                    &format!("duplicate-id:{qualified}"),
                    format!(
                        "`{qualified}` is already declared by the {} at byte {}",
                        first.block, first.id_span.start
                    ),
                );
            }
        }
    }
}
