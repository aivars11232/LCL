//! # lcl-resolver — deterministic, non-executing resolver for LCL Core 0.1.0
//!
//! Milestone M3.
//!
//! Turns parsed source units into a resolved program graph: the exact LCL
//! version, every import and extension, every namespace, every declaration
//! identity, every `REF` binding, and the structural candidate graph — or a
//! stable-ordered list of registered resolution diagnostics. It evaluates
//! nothing, types nothing, and performs no external effect.
//!
//! This is step 4 of `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`:
//!
//! > Resolve exact LCL version, imports, extensions, namespaces, IDs, and REF.
//! > Resolve structural candidate graph membership and branch/loop templates
//! > from EXECUTE for check selection, without evaluating dynamic conditions or
//! > starting actions.
//!
//! Step 5 — static value families, expression types, constructors, parameters
//! and schemas — is the next milestone's and is deliberately absent here.
//!
//! ## Authority
//!
//! The vocabulary is not written here. [`Rules::load`] reads the reserved
//! namespaces, core operation identifiers, reference-target domains, the exact
//! supported LCL version and every reference slot's legal targets out of the
//! verified registries, and refuses any package that is not the approved
//! release ([`lcl_spec::Authority::Authoritative`]).
//!
//! ## The source boundary
//!
//! `05_SEMANTICS/02`: "Ambient current directory and implied nearby files do
//! not exist in portable LCL." Every byte this crate resolves arrives through
//! [`SourceProvider`], which has one method and no way to enumerate, search or
//! default a unit. A source that the document did not name cannot enter
//! resolution, because there is no interface through which it could.
//!
//! ## Stage monotonicity
//!
//! [`Resolver::resolve`] takes a root [`SourceUnit`] and returns
//! [`StageSkipped`] when that root does not survive the lexical and grammar
//! stages. `earliest_stage_rule` evaluates a stage only while every earlier
//! applicable stage is clean, so a source that failed earlier has no
//! resolution verdict at all — not a passing one and not a failing one. The
//! signature makes that unskippable rather than merely documented.
//!
//! An *imported* unit that fails an earlier stage is recorded against its own
//! identity ([`Resolved::stage_failures`]) and makes the whole resolution
//! [`Outcome::Rejected`], because "each source unit or invocation path advances
//! only while its earlier applicable stage has no unhandled unsuppressed
//! diagnostic".
//!
//! ## What a result means
//!
//! [`Outcome::Resolved`] means **no resolution diagnostic and no earlier-stage
//! failure in any loaded unit**. It is not an acceptance of the program: static
//! checking, semantic preflight and execution have not run and nothing here
//! claims they would pass.
//!
//! ## Guarantees
//!
//! * **Deterministic.** Output is a pure function of the root bytes, the
//!   provider's answers and the loaded rules. Every collection iterated is
//!   ordered; no `HashMap` appears in this crate.
//! * **Total.** [`Resolver::resolve`] returns for every input and never panics.
//! * **Exact.** Every binding and diagnostic carries a source identity and a
//!   zero-based byte span into that unit's own bytes.
//! * **Non-executing.** No evaluation, no I/O, no environment.

pub mod declarations;
pub mod diagnostic;
pub mod graph;
mod imports;
pub mod references;
pub mod rules;
pub mod source;

pub use declarations::{Declaration, DeclarationIndex, FullId};
pub use diagnostic::{Cause, Diagnostic, ResolutionError, DEFERRED};
pub use graph::{CandidateGraph, GraphNode, NodeKind};
pub use references::{Binding, BindingTarget};
pub use rules::{RefTarget, ReferenceSlot, Rules, RulesLoadError};
pub use source::{
    LoadError, MemoryProvider, SourceId, SourceProvider, SourceRef, SourceRequest, SourceUnit,
};

use lcl_lexer::{Lexed, Lexer, Lexicon, Span};
use lcl_parser::{Grammar, Parsed, Parser};
use std::collections::BTreeMap;
use std::fmt;

/// A resolver bound to loaded rules, grammar and lexicon.
///
/// Holds no mutable state: the same `Resolver` may resolve any number of
/// programs, in any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Resolver<'a> {
    rules: &'a Rules,
    grammar: &'a Grammar,
    lexicon: &'a Lexicon,
}

impl<'a> Resolver<'a> {
    pub fn new(rules: &'a Rules, grammar: &'a Grammar, lexicon: &'a Lexicon) -> Self {
        Self {
            rules,
            grammar,
            lexicon,
        }
    }

    pub fn rules(&self) -> &'a Rules {
        self.rules
    }

    /// Resolve one root source unit and everything it explicitly imports.
    ///
    /// Total: never panics, for any bytes and any provider.
    ///
    /// Returns [`StageSkipped`] when the root does not pass the lexical and
    /// grammar stages, because the resolution stage is not evaluated for a
    /// source that failed an earlier stage.
    pub fn resolve(
        &self,
        root: &SourceUnit,
        provider: &dyn SourceProvider,
    ) -> Result<Resolved, StageSkipped> {
        let mut units = BTreeMap::new();
        let mut order = Vec::new();
        let root_unit = self.stage(root);

        if let Some(failure) = &root_unit.stage_failure {
            return Err(StageSkipped {
                source: root.id().clone(),
                stage: failure.stage,
                primary: failure.primary.clone(),
                span: failure.span,
            });
        }

        order.push(root.id().clone());
        units.insert(root.id().clone(), root_unit);

        let mut resolved = Resolved {
            root: root.id().clone(),
            order,
            units,
            imports: Vec::new(),
            namespaces: BTreeMap::new(),
            declarations: DeclarationIndex::default(),
            bindings: Vec::new(),
            graph: CandidateGraph::default(),
            diagnostics: Vec::new(),
        };

        let mut raw = Vec::new();
        imports::resolve_sources(self, provider, &mut resolved, &mut raw);
        declarations::index(self, &mut resolved, &mut raw);
        imports::check_versions(self, &mut resolved, &mut raw);
        references::bind(self, &mut resolved, &mut raw);
        graph::build(self, &mut resolved, &mut raw);

        resolved.diagnostics = diagnostic::select(raw, self.rules.supersedes());
        Ok(resolved)
    }

    /// Lex and parse one unit, recording whichever earlier stage failed.
    fn stage(&self, unit: &SourceUnit) -> ResolvedUnit {
        let lexed = Lexer::new(self.lexicon).lex(unit.bytes());
        if let Some(primary) = lexed.primary() {
            let failure = UnitStageFailure {
                stage: lcl_diagnostics::Stage::Lexical,
                primary: primary.id.to_string(),
                span: primary.span,
            };
            return ResolvedUnit {
                id: unit.id().clone(),
                digest: unit.digest(),
                lexed,
                parsed: None,
                stage_failure: Some(failure),
            };
        }
        let parsed = Parser::new(self.grammar).parse(&lexed).ok();
        let stage_failure = parsed.as_ref().and_then(|p| {
            p.primary().map(|d| UnitStageFailure {
                stage: lcl_diagnostics::Stage::GrammarOrSchema,
                primary: d.id.to_string(),
                span: d.span,
            })
        });
        ResolvedUnit {
            id: unit.id().clone(),
            digest: unit.digest(),
            lexed,
            parsed,
            stage_failure,
        }
    }

    pub(crate) fn grammar(&self) -> &'a Grammar {
        self.grammar
    }
}

/// The resolution stage was not evaluated because an earlier stage failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSkipped {
    /// The unit that failed.
    pub source: SourceId,
    /// Which earlier stage failed.
    pub stage: lcl_diagnostics::Stage,
    /// Registered identifier of that stage's primary diagnostic.
    pub primary: String,
    /// Its locus.
    pub span: Span,
}

impl fmt::Display for StageSkipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "resolution stage not evaluated: {} failed the {} stage with {} at byte {}",
            self.source,
            self.stage.as_registry_str(),
            self.primary,
            self.span.start
        )
    }
}

impl std::error::Error for StageSkipped {}

/// An earlier-stage failure of one loaded unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitStageFailure {
    pub stage: lcl_diagnostics::Stage,
    pub primary: String,
    pub span: Span,
}

/// One source unit as it entered resolution.
#[derive(Debug)]
pub struct ResolvedUnit {
    id: SourceId,
    digest: String,
    lexed: Lexed,
    parsed: Option<Parsed>,
    stage_failure: Option<UnitStageFailure>,
}

impl ResolvedUnit {
    pub fn id(&self) -> &SourceId {
        &self.id
    }

    /// Lowercase hex SHA-256 of the exact bytes that were resolved.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn source(&self) -> &str {
        self.lexed.source()
    }

    pub fn lexed(&self) -> &Lexed {
        &self.lexed
    }

    /// The syntax tree, absent only when the lexical stage failed.
    pub fn parsed(&self) -> Option<&Parsed> {
        self.parsed.as_ref()
    }

    pub fn document(&self) -> Option<&lcl_parser::syntax::Document> {
        self.parsed.as_ref().map(Parsed::document)
    }

    /// The earlier stage this unit failed, if any.
    pub fn stage_failure(&self) -> Option<&UnitStageFailure> {
        self.stage_failure.as_ref()
    }

    /// True when this unit passed the lexical and grammar stages, so its
    /// declarations may take part in resolution.
    pub fn is_usable(&self) -> bool {
        self.stage_failure.is_none() && self.parsed.is_some()
    }
}

/// The resolution-stage verdict on one program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No resolution diagnostic, and every loaded unit passed its earlier
    /// stages.
    ///
    /// A statement about the resolution stage only. Static checking, preflight
    /// and execution have not run.
    Resolved,
    /// At least one resolution diagnostic, or a loaded unit that failed an
    /// earlier stage.
    Rejected,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Resolved => f.write_str("resolved"),
            Outcome::Rejected => f.write_str("rejected"),
        }
    }
}

/// The complete result of resolving one program.
#[derive(Debug)]
pub struct Resolved {
    pub(crate) root: SourceId,
    /// Units in the deterministic order they were loaded: the root, then each
    /// import in source-declaration order, depth first.
    pub(crate) order: Vec<SourceId>,
    pub(crate) units: BTreeMap<SourceId, ResolvedUnit>,
    pub(crate) imports: Vec<imports::ImportRecord>,
    /// Namespace prefix -> the import or extension that owns it, per unit.
    pub(crate) namespaces: BTreeMap<(SourceId, String), imports::NamespaceOwner>,
    pub(crate) declarations: DeclarationIndex,
    pub(crate) bindings: Vec<Binding>,
    pub(crate) graph: CandidateGraph,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Resolved {
    /// The root unit's identity.
    pub fn root(&self) -> &SourceId {
        &self.root
    }

    /// Every loaded unit, in load order.
    pub fn units(&self) -> impl Iterator<Item = &ResolvedUnit> {
        self.order.iter().filter_map(|id| self.units.get(id))
    }

    pub fn unit(&self, id: &SourceId) -> Option<&ResolvedUnit> {
        self.units.get(id)
    }

    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    /// Every resolved import and extension, in source-declaration order.
    pub fn imports(&self) -> &[imports::ImportRecord] {
        &self.imports
    }

    /// The declaration index across every usable unit.
    pub fn declarations(&self) -> &DeclarationIndex {
        &self.declarations
    }

    /// Every reference binding, in source order.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The structural candidate graph.
    pub fn graph(&self) -> &CandidateGraph {
        &self.graph
    }

    /// Every emitted resolution diagnostic, in `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Units that failed an earlier stage, in load order.
    pub fn stage_failures(&self) -> impl Iterator<Item = (&SourceId, &UnitStageFailure)> {
        self.units()
            .filter_map(|u| u.stage_failure().map(|f| (u.id(), f)))
    }

    /// `primary_rule`: the first unhandled diagnostic is primary. Nothing is
    /// handled at the resolution stage, so this is the first in `stable_order`.
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.first()
    }

    pub fn outcome(&self) -> Outcome {
        if self.diagnostics.is_empty() && self.stage_failures().next().is_none() {
            Outcome::Resolved
        } else {
            Outcome::Rejected
        }
    }

    /// Registered `default_status` of the primary diagnostic, if any.
    pub fn terminal_status(&self) -> Option<&str> {
        self.primary().map(|d| d.default_status.as_str())
    }
}
