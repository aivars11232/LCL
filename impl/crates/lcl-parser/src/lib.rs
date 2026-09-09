//! # lcl-parser — deterministic, non-executing parser for LCL Core 0.1.0
//!
//! Milestone M2.
//!
//! Turns the authoritative M1 token stream into either a source-faithful syntax
//! tree with exact byte spans or a stable-ordered list of registered
//! grammar-and-schema diagnostics, per the canonical specification at
//! `../canonical/LCL_Core_0.1.0`. It resolves nothing, types nothing, evaluates
//! nothing, and never rewrites source.
//!
//! This is step 3 of `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`: "Parse
//! the complete grammar and block schemas." Step 4 — exact version, import,
//! extension, namespace, ID and `REF` resolution — is the next milestone's and
//! is deliberately absent here.
//!
//! ## Authority
//!
//! The vocabulary is not written here. [`Grammar::load`] reads the 41 block
//! schemas, their field signatures and value kinds, the per-document-kind
//! top-level block sets, and every grammar-or-schema error identifier with its
//! meaning, default status, specificity rank and supersession edges out of the
//! verified registries, and refuses to load from any package that is not the
//! approved release ([`lcl_spec::Authority::Authoritative`]).
//!
//! The rules implemented are the complete EBNF of
//! `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`, the block and nesting prose of
//! `04_GRAMMAR/01`–`07` and `/11`–`/13`, the closed schemas of
//! `block_schemas_v0.1.0.json` and `field_signatures_v0.1.0.json`, and the
//! `diagnostic_selection` contract of `statuses_and_errors_v0.1.0.json`
//! restricted to one source-validation run.
//!
//! ## Stage monotonicity
//!
//! [`Parser::parse`] accepts a [`Lexed`] and **returns an error if the lexical
//! stage did not succeed**. `earliest_stage_rule` evaluates a stage only while
//! every earlier applicable stage is clean, so a token stream that carries a
//! lexical diagnostic has no grammar-stage verdict at all — not a passing one
//! and not a failing one. The signature makes that unskippable rather than
//! merely documented.
//!
//! ## What a result means
//!
//! [`Outcome::Parsed`] means **no grammar-or-schema diagnostic**. It is not an
//! acceptance of the document: resolution, static, validation and later stages
//! have not run and nothing here claims they would pass.
//! [`Outcome::Rejected`] means the source is invalid at the grammar-or-schema
//! stage, with the registered primary diagnostic and its default terminal
//! status.
//!
//! ## Guarantees
//!
//! * **Deterministic.** Output is a pure function of the tokens and the loaded
//!   grammar. Every collection iterated during parsing is ordered.
//! * **Total.** [`Parser::parse`] returns for every token stream and never
//!   panics; there is no `unwrap` on input-derived values and no unchecked
//!   indexing.
//! * **Exact.** Every node and diagnostic carries a zero-based byte span into
//!   the caller's own buffer.
//! * **Non-executing.** No evaluation, no I/O, no environment.
//!
//! ## Not in M2
//!
//! No resolver, type checker, evaluator, capability kernel, runtime, CLI or UI.
//! Value *shapes* are checked against each field's registered value kind;
//! value *domains* — an exact string, an integer range, a qualified-identifier
//! domain, a reference target block — are resolution- and static-stage
//! questions and are not attempted. See [`grammar`] for the exact split and the
//! canonical evidence for it.

mod block;
mod conditional;
pub mod diagnostic;
mod expr;
pub mod grammar;
mod parse;
mod schema;
pub mod syntax;

pub use diagnostic::{Cause, Diagnostic, GrammarError};
pub use grammar::{
    BlockSchema, FieldSignature, FormSet, Grammar, GrammarLoadError, Occurrence,
    RegisteredGrammarError,
};
pub use syntax::Document;

/// Conditional requirements the grammar stage deliberately leaves to a later
/// layer, each paired with the layer that owns it.
///
/// `field_signatures_v0.1.0.json` states 88 conditional requirements as prose.
/// The structurally decidable ones are enforced here; the rest state semantics
/// this milestone must not decide. Exposing the list keeps that boundary
/// legible rather than implicit.
/// See [`enforced_requirements`] for the complement.
pub fn deferred_requirements() -> &'static [(&'static str, &'static str)] {
    conditional::DEFERRED
}

/// The conditional requirements this stage does enforce, as
/// `(block, exact registry string)`.
///
/// Each string is quoted verbatim from
/// `field_signatures_v0.1.0.json#/blocks/<block>/conditional_requirements`, so
/// a caller — or a test — can confirm that what is enforced is what the
/// specification currently states.
pub fn enforced_requirements() -> Vec<(&'static str, &'static str)> {
    conditional::IMPLEMENTED
        .iter()
        .map(|(block, text, _)| (*block, *text))
        .collect()
}

use lcl_lexer::{Lexed, Span};
use std::fmt;

/// A parser bound to a loaded grammar.
///
/// Holds no mutable state: the same `Parser` may parse any number of sources,
/// in any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Parser<'a> {
    grammar: &'a Grammar,
}

impl<'a> Parser<'a> {
    pub fn new(grammar: &'a Grammar) -> Self {
        Self { grammar }
    }

    pub fn grammar(&self) -> &'a Grammar {
        self.grammar
    }

    /// Parse a successfully lexed source. Total: never panics, for any token
    /// stream.
    ///
    /// Returns [`StageSkipped`] when `lexed` carries a lexical diagnostic,
    /// because the grammar stage is not evaluated for a source that failed an
    /// earlier stage.
    pub fn parse(&self, lexed: &Lexed) -> Result<Parsed, StageSkipped> {
        if let Some(primary) = lexed.primary() {
            return Err(StageSkipped {
                lexical_primary: primary.id.to_string(),
                span: primary.span,
            });
        }
        Ok(parse::parse(self.grammar, lexed))
    }

    /// Parse one **expression fragment**: exactly one `EXPRESSION`, followed
    /// only by optional whitespace.
    ///
    /// `operations_v0.1.0.json#/expression_fragment_contract/syntax`:
    ///
    /// > After STRING decoding, consume exactly one EXPRESSION from
    /// > `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`, followed only by optional
    /// > whitespace. No declarations, statements, operation invocation, or
    /// > named expression-call argument is admitted.
    ///
    /// The fragment forms of `core.calculate`, `core.select` and `core.filter`
    /// are the only callers. They exist here, in the crate that owns the
    /// grammar, because "exactly one EXPRESSION from the EBNF" is a statement
    /// about *this* parser: a fragment parser living anywhere else would be a
    /// second, quietly diverging expression language.
    ///
    /// ## Why the fragment is lexed inside a field context
    ///
    /// A fragment is a decoded STRING, not a line of a document, and the
    /// contract says so: "The fragment is one logical expression and introduces
    /// no indentation blocks", lexed under "the source character, string,
    /// identifier, and token rules".
    ///
    /// Lexing the bare text would apply a rule the fragment is not subject to.
    /// A word at the start of a line is in a "syntax-required block/field key"
    /// position, where `error.keyword.case` is exactly right for a document —
    /// and the reserved fragment bindings the contract names, `item` and
    /// `target`, are registered words in lower case. Presented as a line, the
    /// two bindings the contract *requires* would be miscased keywords, while
    /// the same names in the same expression inside a document lex cleanly:
    /// "A legal lowercase identifier is never promoted to a keyword by case
    /// folding."
    ///
    /// So the fragment is lexed where every expression in every document is
    /// lexed: after a field key. [`FRAGMENT_CONTEXT`] is that key, the parse
    /// starts after it, and the returned spans index the contextualised text —
    /// subtract `FRAGMENT_CONTEXT.len()` for an offset into the author's own
    /// fragment.
    ///
    /// Returns the parsed expression, or exactly why it is not one.
    pub fn expression_fragment(
        &self,
        lexicon: &lcl_lexer::Lexicon,
        fragment: &str,
    ) -> Result<syntax::Expr, FragmentError> {
        // "followed only by optional whitespace", and "Every non-empty source
        // ends with one LINE FEED" for the text actually handed to the lexer.
        let text = format!("{FRAGMENT_CONTEXT}{}\n", fragment.trim_end());
        let lexed = lcl_lexer::Lexer::new(lexicon).lex_str(&text);
        if let Some(primary) = lexed.primary() {
            return Err(FragmentError::Lexical(StageSkipped {
                lexical_primary: primary.id.to_string(),
                span: primary.span,
            }));
        }
        parse::expression_fragment(self.grammar, &lexed)
    }
}

/// The field key one expression fragment is lexed after.
///
/// `VALUE` is the ordinary inline-expression field, so this places the fragment
/// in the one position the lexer and grammar already treat as an expression.
pub const FRAGMENT_CONTEXT: &str = "VALUE: ";

/// Why one expression fragment is not exactly one expression.
///
/// Every variant is one defect the fragment contract names, and the standard
/// library maps all of them to `error.operation.parameter`: "Malformed fragment
/// syntax and invalid bindings produce error.operation.parameter." They stay
/// distinct here because the detail an author needs differs, and because a
/// caller that wanted to treat them alike can, while one that could not tell
/// them apart never could.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragmentError {
    /// The fragment did not lex.
    Lexical(StageSkipped),
    /// The fragment lexed but did not parse as an expression.
    Grammar(Vec<Diagnostic>),
    /// One expression parsed, and something other than whitespace followed it.
    Trailing(Span),
    /// The fragment holds no expression at all.
    Empty(Span),
}

impl fmt::Display for FragmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FragmentError::Lexical(skipped) => write!(f, "{skipped}"),
            FragmentError::Grammar(diagnostics) => {
                let first = diagnostics
                    .first()
                    .map(|d| d.id.to_string())
                    .unwrap_or_else(|| "a grammar defect".to_string());
                write!(f, "the fragment is not one expression: {first}")
            }
            FragmentError::Trailing(span) => write!(
                f,
                "one expression parsed, and input remains at {span}; a fragment admits \
                 exactly one expression followed only by whitespace"
            ),
            FragmentError::Empty(_) => f.write_str("the fragment holds no expression"),
        }
    }
}

impl std::error::Error for FragmentError {}

/// The grammar stage was not evaluated because the lexical stage did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSkipped {
    /// Registered identifier of the lexical primary diagnostic.
    pub lexical_primary: String,
    /// Its locus.
    pub span: Span,
}

impl fmt::Display for StageSkipped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "grammar stage not evaluated: the lexical stage failed with {} at {}",
            self.lexical_primary, self.span
        )
    }
}

impl std::error::Error for StageSkipped {}

/// The grammar-and-schema-stage verdict on one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No grammar-or-schema diagnostic was raised. The syntax tree is complete.
    ///
    /// This is a statement about the grammar-and-schema stage only. Per
    /// `earliest_stage_rule`, later stages are evaluated only when this one is
    /// clean, and none of them has run.
    Parsed,
    /// At least one grammar-or-schema diagnostic was raised. The source is
    /// invalid; the primary diagnostic and its registered default status are
    /// available on the [`Parsed`].
    Rejected,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Parsed => f.write_str("parsed"),
            Outcome::Rejected => f.write_str("rejected"),
        }
    }
}

/// The complete result of parsing one source.
#[derive(Debug, Clone)]
pub struct Parsed {
    document: Document,
    /// Already in the registry's `stable_order`, after supersession and
    /// duplicate suppression.
    diagnostics: Vec<Diagnostic>,
}

impl Parsed {
    /// The syntax tree.
    ///
    /// A rejected source still yields whatever independent structure the parser
    /// recovered, so a later consumer can report more than one defect. Recovery
    /// never invents a node: see [`syntax`].
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Every emitted diagnostic, in `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// `primary_rule`: "the first unhandled diagnostic is primary". Nothing is
    /// handled at the grammar stage, so this is the first in `stable_order`.
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.first()
    }

    pub fn outcome(&self) -> Outcome {
        if self.diagnostics.is_empty() {
            Outcome::Parsed
        } else {
            Outcome::Rejected
        }
    }

    /// Registered `default_status` of the primary diagnostic, if rejected.
    pub fn terminal_status(&self) -> Option<&str> {
        self.primary().map(|d| d.default_status.as_str())
    }

    pub(crate) fn new(document: Document, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            document,
            diagnostics,
        }
    }
}
