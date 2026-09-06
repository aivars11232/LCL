//! # lcl-lexer — deterministic, non-executing lexer for LCL Core 0.1.0
//!
//! Milestone M1.
//!
//! Turns source bytes into either a token stream with exact byte spans or a
//! stable-ordered list of registered lexical diagnostics, per the canonical
//! specification at `../canonical/LCL_Core_0.1.0`. It evaluates nothing,
//! resolves nothing, and never rewrites source.
//!
//! ## Authority
//!
//! The vocabulary is not written here. [`Lexicon::load`] reads the reserved
//! words, adopted and excluded symbols, and every lexical error identifier with
//! its meaning, default status, specificity rank and supersession edges out of
//! the verified registries, and refuses to load from any package that is not
//! the approved release ([`lcl_spec::Authority::Authoritative`]).
//!
//! The rules implemented are those of `02_LEXICAL/01`, `02`, `03`, `07`, `08`,
//! `09`, `10` and `12`, the terminals of `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`,
//! the closed literal profiles of `03_TYPES_AND_VALUES/04` and `/07` for
//! literal constructor arguments, and the `diagnostic_selection` contract of
//! `statuses_and_errors_v0.1.0.json` restricted to one source-validation run.
//!
//! Where a lexical decision needs context — whether a lowercase key sits in
//! object data, whether a word follows a complete operand, whether a field is a
//! type position — the scanner keeps exactly that bounded token context
//! itself. Nothing is parsed into a tree and nothing is resolved.
//!
//! ## What a result means
//!
//! [`Outcome::Tokenized`] means **no lexical diagnostic**. It is not an
//! acceptance of the document: grammar, resolution, static, validation and
//! later stages have not run and nothing here claims they would pass.
//! [`Outcome::Rejected`] means the source is invalid at the lexical stage, with
//! the registered primary diagnostic and its default terminal status.
//!
//! ## Guarantees
//!
//! * **Deterministic.** Output is a pure function of the input bytes and the
//!   loaded lexicon.
//! * **Total.** [`Lexer::lex`] returns for every byte sequence and never panics;
//!   there is no `unwrap` on input-derived values and no unchecked indexing.
//! * **Exact.** Every token and diagnostic carries a zero-based byte span into
//!   the caller's own buffer; a token's span slices back to its lexeme.
//! * **Non-executing.** No evaluation, no I/O, no environment.
//!
//! ## Not in M1
//!
//! No resolver, type checker, evaluator, capability kernel, runtime, CLI or
//! UI. The grammar and block schemas are M2's, in `lcl-parser`, which consumes
//! this crate's output. Every rule the registry stages as lexical is decided
//! here; nothing lexical is deferred. Value-domain checks on *dynamically
//! supplied* constructor arguments (a `REF`, an expression) are execution-stage
//! by `expression_demand_resolution` and are not attempted.

pub mod diagnostic;
pub mod lexicon;
mod literal;
mod scan;
pub mod span;
pub mod token;

pub use diagnostic::{Cause, Diagnostic, LexicalError};
pub use lexicon::{
    Lexicon, LexiconError, LiteralConstructor, LiteralProfile, RegexFlagContract,
    RegisteredLexicalError,
};
pub use span::{Position, Span};
pub use token::{Token, TokenKind};

use std::fmt;

/// A lexer bound to a loaded vocabulary.
///
/// Holds no mutable state: the same `Lexer` may lex any number of sources, in
/// any order, with identical results for identical input.
#[derive(Debug, Clone, Copy)]
pub struct Lexer<'a> {
    lexicon: &'a Lexicon,
}

impl<'a> Lexer<'a> {
    pub fn new(lexicon: &'a Lexicon) -> Self {
        Self { lexicon }
    }

    pub fn lexicon(&self) -> &'a Lexicon {
        self.lexicon
    }

    /// Lex raw source bytes. Total: never panics, for any input.
    pub fn lex(&self, source: &[u8]) -> Lexed {
        scan::lex(self.lexicon, source)
    }

    /// Lex source already known to be a Rust string. Byte spans are relative to
    /// `source.as_bytes()`.
    pub fn lex_str(&self, source: &str) -> Lexed {
        self.lex(source.as_bytes())
    }
}

/// The lexical-stage verdict on one source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// No lexical diagnostic was raised. The token stream is complete.
    ///
    /// This is a statement about the lexical stage only. Per
    /// `earliest_stage_rule`, later stages are evaluated only when this one is
    /// clean, and none of them has run.
    Tokenized,
    /// At least one lexical diagnostic was raised. The source is invalid; the
    /// primary diagnostic and its registered default status are available on
    /// the [`Lexed`].
    Rejected,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Tokenized => f.write_str("tokenized"),
            Outcome::Rejected => f.write_str("rejected"),
        }
    }
}

/// The complete result of lexing one source.
#[derive(Debug, Clone)]
pub struct Lexed {
    /// The source as text. Empty when the source was not valid UTF-8, in which
    /// case the single diagnostic is `error.encoding.invalid` and there are no
    /// tokens.
    source: String,
    /// Byte length of the original input, including any byte-order mark.
    source_len: usize,
    tokens: Vec<Token>,
    /// Already in the registry's `stable_order`, after supersession and
    /// duplicate suppression.
    diagnostics: Vec<Diagnostic>,
}

impl Lexed {
    /// The token stream, in source order. Empty only when the source is empty
    /// (then just `EOF`) or was not valid UTF-8.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Every emitted diagnostic, in `stable_order`.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// `primary_rule`: "the first unhandled diagnostic is primary". Nothing is
    /// handled at the lexical stage, so this is the first in `stable_order`.
    pub fn primary(&self) -> Option<&Diagnostic> {
        self.diagnostics.first()
    }

    pub fn outcome(&self) -> Outcome {
        if self.diagnostics.is_empty() {
            Outcome::Tokenized
        } else {
            Outcome::Rejected
        }
    }

    /// Registered `default_status` of the primary diagnostic, if rejected.
    pub fn terminal_status(&self) -> Option<&str> {
        self.primary().map(|d| d.default_status.as_str())
    }

    /// The source text the spans index into. See [`Lexed`] for the invalid
    /// UTF-8 case.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Byte length of the original input.
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    /// The exact source bytes of a token. `None` only for a span that does not
    /// address this source, which no token produced by this lexer has.
    pub fn lexeme(&self, token: &Token) -> Option<&str> {
        token.span.slice(&self.source)
    }

    /// Tokens of one kind, in source order.
    pub fn tokens_of(&self, kind: TokenKind) -> impl Iterator<Item = &Token> {
        self.tokens.iter().filter(move |t| t.kind == kind)
    }

    /// A compact, deterministic rendering of the token stream for reports and
    /// golden comparisons: one `KIND[start..end]` (plus `=lexeme` for
    /// non-structural tokens) per line.
    pub fn render_tokens(&self) -> String {
        let mut out = String::new();
        for token in &self.tokens {
            out.push_str(token.kind.ebnf_name());
            out.push('[');
            out.push_str(&token.span.start.to_string());
            out.push_str("..");
            out.push_str(&token.span.end.to_string());
            out.push(']');
            match token.kind {
                TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof | TokenKind::Newline => {}
                TokenKind::BlankLine | TokenKind::Space => {}
                TokenKind::String | TokenKind::MultilineString => {
                    out.push('=');
                    out.push_str(&format!("{:?}", token.value.as_deref().unwrap_or("")));
                }
                _ => {
                    out.push('=');
                    out.push_str(self.lexeme(token).unwrap_or("<unsliceable>"));
                }
            }
            out.push('\n');
        }
        out
    }
}
