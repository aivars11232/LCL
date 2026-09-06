//! The structural candidate graph.
//!
//! Authority: `block_schemas_v0.1.0.json#/execution_graph_contract` and
//! `05_SEMANTICS/01_DECLARATION_RESOLUTION_REACHABILITY_AND_EXECUTION_GRAPH.txt`.
//!
//! > Build the complete finite structural activation graph from EXECUTE.
//! > Include both IF branches and each FOR EACH body as source templates for
//! > static checking. TASK execution fields, PHASE and SEQUENCE executable
//! > children, the selected STEP arm, and a root TEST TASK or ACTION activate
//! > execution units. Ordinary REF, TARGET, check references, OUTPUT reads,
//! > BEFORE and AFTER never activate a producer.
//!
//! **Membership and templates only.** Ordering edges are step 9 of the
//! processing model — "Finalize and check ordering edges of that resolved
//! candidate graph before effects" — and `error.execution.order` is registered
//! at `stage: execution`. Neither is decided here. What this milestone owes the
//! later layers is the exact node set and child order they will order.

use lcl_lexer::Span;

use crate::source::SourceId;

/// Why a node is in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    /// The `EXECUTE` root.
    Root,
    /// An execution unit activated by a parent's execution-bearing field.
    ExecutionUnit,
    /// An `IF` branch body, included as a source template for static checking.
    /// Both arms are included; neither condition is evaluated.
    BranchTemplate,
    /// A `FOR EACH` body, included once as a source template. Iteration
    /// instances are a runtime notion and are not expanded here.
    LoopTemplate,
}

/// One node of the structural candidate graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub kind: NodeKind,
    /// The unit this node's source declaration lives in.
    pub source: SourceId,
    /// The declaring block, e.g. `ACTION`, or the control form `IF` /
    /// `FOR EACH`.
    pub block: String,
    /// Locus of the node's source declaration.
    pub span: Span,
    /// The declaration this node activates, when it activates one.
    pub declaration: Option<usize>,
    /// Parent node index. `None` for the root.
    pub parent: Option<usize>,
    /// Child node indexes, in the canonical child order.
    pub children: Vec<usize>,
}

/// The structural activation graph built from `EXECUTE`.
///
/// Empty when the document declares no `EXECUTE`, which is legal for
/// `kind.library`, `kind.data` and `kind.extension` documents.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CandidateGraph {
    nodes: Vec<GraphNode>,
}

impl CandidateGraph {
    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&GraphNode> {
        self.nodes.get(index)
    }

    pub fn root(&self) -> Option<&GraphNode> {
        self.nodes.first()
    }

    /// Every node that activates a declaration, in child order.
    pub fn activated(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.iter().filter(|n| n.declaration.is_some())
    }

    pub(crate) fn push(&mut self, node: GraphNode) -> usize {
        let index = self.nodes.len();
        if let Some(parent) = node.parent {
            if let Some(p) = self.nodes.get_mut(parent) {
                p.children.push(index);
            }
        }
        self.nodes.push(node);
        index
    }
}

// ---------------------------------------------------------------------------
// Phase E: building the graph
// ---------------------------------------------------------------------------

use crate::declarations::Declaration;
use crate::{BindingTarget, Diagnostic, Emitter, ResolutionError, Resolved, Resolver};
use lcl_parser::syntax::{Block, Body, Executable, Statement, TopLevel};
use std::collections::BTreeMap;

/// Fields through which each container activates execution units, in the order
/// `#/execution_graph_contract/child_order` fixes.
///
/// > TASK execution-bearing PHASE, SEQUENCE, and ACTION fields contribute
/// > children in source field order ... PHASE and SEQUENCE executable children
/// > contribute in lexical order ... A STEP selected reference LIST expands
/// > left to right.
const ACTIVATING_FIELDS: &[(&str, &[&str])] = &[
    ("TASK", &["PHASE", "SEQUENCE", "ACTION"]),
    ("PHASE", &["STEP", "SEQUENCE"]),
    ("SEQUENCE", &["STEP"]),
    ("STEP", &["ACTION", "SEQUENCE", "PHASE", "TASK"]),
    ("TEST", &["TASK", "ACTION"]),
];

fn activating_fields(block: &str) -> &'static [&'static str] {
    ACTIVATING_FIELDS
        .iter()
        .find(|(name, _)| *name == block)
        .map(|(_, fields)| *fields)
        .unwrap_or(&[])
}

/// Build the structural candidate graph from `EXECUTE`.
pub(crate) fn build(resolver: &Resolver<'_>, resolved: &mut Resolved, raw: &mut Vec<Diagnostic>) {
    let mut graph = CandidateGraph::default();
    {
        let emitter = Emitter::new(resolver.rules(), &resolved.units);
        let Some(document) = resolved
            .units
            .get(&resolved.root)
            .and_then(|u| u.document())
        else {
            return;
        };
        // 05_SEMANTICS/01: "EXECUTE identifies one root." A document without
        // one — kind.library, kind.data, kind.extension — has no graph, which
        // is not a defect.
        let Some(execute) = document.block("EXECUTE") else {
            return;
        };
        let Some((_, reference_span)) = crate::field::expression(execute, "REFERENCE")
            .and_then(crate::declarations::reference_argument)
        else {
            return;
        };

        let mut bodies = BTreeMap::new();
        let mut binding_at = BTreeMap::new();
        for unit in resolved
            .order
            .iter()
            .filter_map(|id| resolved.units.get(id))
        {
            if let Some(document) = unit.document() {
                collect_bodies(unit.id(), &document.items, resolver, &mut bodies);
            }
        }
        for binding in &resolved.bindings {
            if let BindingTarget::Declaration(index) = binding.target {
                binding_at.insert((binding.source.clone(), binding.span.start), index);
            }
        }

        let Some(&root_declaration) =
            binding_at.get(&(resolved.root.clone(), reference_span.start))
        else {
            // The EXECUTE reference did not resolve; that is already reported.
            return;
        };

        let mut builder = Builder {
            resolved,
            emitter: &emitter,
            raw,
            bodies,
            binding_at,
            graph: &mut graph,
        };
        let declaration = builder
            .resolved
            .declarations
            .get(root_declaration)
            .expect("bound index");
        let root_node = builder.graph.push(GraphNode {
            kind: NodeKind::Root,
            source: declaration.source.clone(),
            block: declaration.block.clone(),
            span: declaration.block_span,
            declaration: Some(root_declaration),
            parent: None,
            children: Vec::new(),
        });
        let mut path = vec![root_declaration];
        builder.expand(root_declaration, root_node, &mut path);
    }
    resolved.graph = graph;
}

/// Map every declaration's own statement list by `(unit, block span start)`,
/// which is the key `declarations` records as `Declaration::block_span`.
fn collect_bodies<'a>(
    source: &crate::SourceId,
    items: &'a [TopLevel],
    resolver: &Resolver<'_>,
    out: &mut BTreeMap<(crate::SourceId, usize), &'a [Statement]>,
) {
    for item in items {
        match item {
            TopLevel::Block(b) => collect_block(source, b, resolver, out),
            TopLevel::Conditional(c) => {
                collect_executables(source, &c.then_body, resolver, out);
                if let Some(arm) = &c.else_body {
                    collect_executables(source, &arm.body, resolver, out);
                }
            }
            TopLevel::ForEach(f) => collect_executables(source, &f.body, resolver, out),
        }
    }
}

fn collect_block<'a>(
    source: &crate::SourceId,
    block: &'a Block,
    resolver: &Resolver<'_>,
    out: &mut BTreeMap<(crate::SourceId, usize), &'a [Statement]>,
) {
    out.insert((source.clone(), block.key.span.start), &block.body);
    collect_statements(source, &block.key.text, &block.body, resolver, out);
}

fn collect_statements<'a>(
    source: &crate::SourceId,
    block_name: &str,
    body: &'a [Statement],
    resolver: &Resolver<'_>,
    out: &mut BTreeMap<(crate::SourceId, usize), &'a [Statement]>,
) {
    for statement in body {
        match statement {
            Statement::Field(field) => {
                let child = resolver
                    .grammar()
                    .schema(block_name)
                    .and_then(|s| s.field(&field.key.text))
                    .and_then(|f| f.nested_block.as_deref());
                if let (Some(child), Body::Nested(nested)) = (child, &field.body) {
                    out.insert((source.clone(), field.key.span.start), &nested.statements);
                    collect_statements(source, child, &nested.statements, resolver, out);
                }
            }
            Statement::Conditional(c) => {
                collect_executables(source, &c.then_body, resolver, out);
                if let Some(arm) = &c.else_body {
                    collect_executables(source, &arm.body, resolver, out);
                }
            }
            Statement::ForEach(f) => collect_executables(source, &f.body, resolver, out),
            Statement::Property(_) => {}
        }
    }
}

fn collect_executables<'a>(
    source: &crate::SourceId,
    body: &'a [Executable],
    resolver: &Resolver<'_>,
    out: &mut BTreeMap<(crate::SourceId, usize), &'a [Statement]>,
) {
    for item in body {
        match item {
            Executable::Block(b) => collect_block(source, b, resolver, out),
            Executable::Conditional(c) => {
                collect_executables(source, &c.then_body, resolver, out);
                if let Some(arm) = &c.else_body {
                    collect_executables(source, &arm.body, resolver, out);
                }
            }
            Executable::ForEach(f) => collect_executables(source, &f.body, resolver, out),
        }
    }
}

struct Builder<'a, 'b> {
    resolved: &'a Resolved,
    emitter: &'a Emitter<'a>,
    raw: &'b mut Vec<Diagnostic>,
    bodies: BTreeMap<(crate::SourceId, usize), &'a [Statement]>,
    binding_at: BTreeMap<(crate::SourceId, usize), usize>,
    graph: &'b mut CandidateGraph,
}

impl Builder<'_, '_> {
    fn declaration(&self, index: usize) -> Option<&Declaration> {
        self.resolved.declarations.get(index)
    }

    /// Expand one execution unit's children in canonical child order.
    fn expand(&mut self, declaration: usize, node: usize, path: &mut Vec<usize>) {
        let Some(decl) = self.declaration(declaration).cloned() else {
            return;
        };
        let key = (decl.source.clone(), decl.block_span.start);
        let Some(body) = self.bodies.get(&key).copied() else {
            return;
        };
        let fields = activating_fields(&decl.block);
        if fields.is_empty() {
            return;
        }

        match decl.block.as_str() {
            // "TASK execution-bearing PHASE, SEQUENCE, and ACTION fields
            // contribute children in source field order."
            "TASK" | "STEP" | "TEST" => {
                for statement in body {
                    let Statement::Field(field) = statement else {
                        continue;
                    };
                    if !fields.contains(&field.key.text.as_str()) {
                        continue;
                    }
                    self.field_children(&decl, field, node, path);
                }
            }
            // "PHASE and SEQUENCE executable children contribute in lexical
            // order, including STEP, nested SEQUENCE where legal, IF and
            // FOR EACH."
            "PHASE" | "SEQUENCE" => {
                for statement in body {
                    match statement {
                        Statement::Field(field) if fields.contains(&field.key.text.as_str()) => {
                            self.field_children(&decl, field, node, path);
                        }
                        Statement::Conditional(c) => {
                            self.branch(&decl, c, node, path);
                        }
                        Statement::ForEach(f) => {
                            self.loop_template(&decl, f, node, path);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// One activating field: a reference, a reference LIST expanded left to
    /// right, or a nested child block.
    fn field_children(
        &mut self,
        parent_decl: &Declaration,
        field: &lcl_parser::syntax::Field,
        node: usize,
        path: &mut Vec<usize>,
    ) {
        match &field.body {
            Body::Nested(_) => {
                // A nested child block declares its own identity; find it by
                // the field key span the declaration index recorded.
                if let Some(&child) = self
                    .resolved
                    .declarations
                    .by_span(&parent_decl.source, field.key.span.start)
                {
                    self.activate(child, field.key.span, &parent_decl.source, node, path);
                }
            }
            Body::Inline(value) => {
                for span in reference_spans(value) {
                    let key = (parent_decl.source.clone(), span.start);
                    if let Some(&child) = self.binding_at.get(&key) {
                        self.activate(child, span, &parent_decl.source, node, path);
                    }
                }
            }
        }
    }

    /// Add one activated execution unit, unless doing so would close a cycle.
    fn activate(
        &mut self,
        declaration: usize,
        span: Span,
        source: &crate::SourceId,
        parent: usize,
        path: &mut Vec<usize>,
    ) {
        // "Structural cycles use error.reference.cycle."
        if path.contains(&declaration) {
            let name = self
                .declaration(declaration)
                .map(|d| d.id.qualified())
                .unwrap_or_default();
            self.emitter.emit(
                self.raw,
                ResolutionError::ReferenceCycle,
                source,
                span,
                &format!("graph-cycle:{name}"),
                format!("activating `{name}` closes a structural cycle in the candidate graph"),
            );
            return;
        }
        let Some(decl) = self.declaration(declaration).cloned() else {
            return;
        };
        let child = self.graph.push(GraphNode {
            kind: NodeKind::ExecutionUnit,
            source: decl.source.clone(),
            block: decl.block.clone(),
            span: decl.block_span,
            declaration: Some(declaration),
            parent: Some(parent),
            children: Vec::new(),
        });
        path.push(declaration);
        self.expand(declaration, child, path);
        path.pop();
    }

    /// Both arms of an `IF`, as source templates.
    ///
    /// 05_SEMANTICS/01: "Include both IF branches and each FOR EACH body as
    /// source templates for static checking." No condition is evaluated, so
    /// neither arm is preferred and neither is omitted.
    fn branch(
        &mut self,
        parent_decl: &Declaration,
        conditional: &lcl_parser::syntax::Conditional,
        node: usize,
        path: &mut Vec<usize>,
    ) {
        let then_node = self.graph.push(GraphNode {
            kind: NodeKind::BranchTemplate,
            source: parent_decl.source.clone(),
            block: "IF".to_string(),
            span: conditional.keyword_span,
            declaration: None,
            parent: Some(node),
            children: Vec::new(),
        });
        self.executables(parent_decl, &conditional.then_body, then_node, path);
        if let Some(arm) = &conditional.else_body {
            let else_node = self.graph.push(GraphNode {
                kind: NodeKind::BranchTemplate,
                source: parent_decl.source.clone(),
                block: "ELSE".to_string(),
                span: arm.keyword_span,
                declaration: None,
                parent: Some(node),
                children: Vec::new(),
            });
            self.executables(parent_decl, &arm.body, else_node, path);
        }
    }

    /// One `FOR EACH` body, as a single source template.
    ///
    /// Iteration instances are a runtime notion — "FOR EACH evaluates its
    /// finite snapshot once at reachability" — so the body appears once here
    /// and is not unrolled.
    fn loop_template(
        &mut self,
        parent_decl: &Declaration,
        for_each: &lcl_parser::syntax::ForEach,
        node: usize,
        path: &mut Vec<usize>,
    ) {
        let loop_node = self.graph.push(GraphNode {
            kind: NodeKind::LoopTemplate,
            source: parent_decl.source.clone(),
            block: "FOR EACH".to_string(),
            span: for_each.keyword_span,
            declaration: None,
            parent: Some(node),
            children: Vec::new(),
        });
        self.executables(parent_decl, &for_each.body, loop_node, path);
    }

    fn executables(
        &mut self,
        parent_decl: &Declaration,
        body: &[Executable],
        node: usize,
        path: &mut Vec<usize>,
    ) {
        for item in body {
            match item {
                Executable::Block(b) => {
                    if let Some(&child) = self
                        .resolved
                        .declarations
                        .by_span(&parent_decl.source, b.key.span.start)
                    {
                        self.activate(child, b.key.span, &parent_decl.source, node, path);
                    }
                }
                Executable::Conditional(c) => self.branch(parent_decl, c, node, path),
                Executable::ForEach(f) => self.loop_template(parent_decl, f, node, path),
            }
        }
    }
}

/// The spans of every `REF` identifier directly in one field value: a single
/// reference, or a LIST expanded left to right.
fn reference_spans(value: &lcl_parser::syntax::Value) -> Vec<Span> {
    use lcl_parser::syntax::{Expr, Value};
    // Iterative: untrusted source must not turn nesting depth into stack depth.
    fn collect(expr: &Expr, out: &mut Vec<Span>) {
        let mut stack = vec![expr];
        while let Some(expr) = stack.pop() {
            match expr {
                Expr::Call(call) if call.callable.text == "REF" => {
                    if let [Expr::Identifier(ident)] = call.arguments.as_slice() {
                        out.push(ident.span);
                    }
                }
                Expr::Group(group) => stack.push(&group.inner),
                Expr::Collection(collection) => {
                    for member in collection.members.iter().rev() {
                        stack.push(member);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    match value {
        Value::Expression(e) => collect(e, &mut out),
        Value::MultilineCollection(c) => {
            for member in &c.members {
                collect(member, &mut out);
            }
        }
    }
    out
}
