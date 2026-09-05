//! The token model.
//!
//! Every token kind here is a terminal that `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`
//! actually names. Nothing is invented for implementation convenience, and the
//! lexer assigns no meaning: a `ReservedWord` token records that the source
//! spelled a registered word at a span, not what that word does.
//!
//! ## The token/diagnostic invariant
//!
//! A lexeme yields **either** one token **or** one or more diagnostics, never
//! both. So `Lexed::tokens` is exactly the sequence of well-formed lexemes, and
//! a caller can rely on every token being lexically valid without re-checking.

use crate::span::Span;
use std::fmt;

/// A lexical terminal of LCL Core 0.1.0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TokenKind {
    /// A member of the closed 141-word reserved list (`keywords_v0.1.0.json`).
    ReservedWord,
    /// `[a-z][a-z0-9_]*`.
    SimpleIdentifier,
    /// `simple_identifier ("." simple_identifier)+`, consumed maximally.
    QualifiedIdentifier,
    /// `0|[1-9][0-9]*`.
    IntegerLiteral,
    /// `(0|[1-9][0-9]*)\.[0-9]+`.
    DecimalLiteral,
    /// A single-line `"..."` literal. The decoded value is on the token.
    String,
    /// A `"""` literal. The decoded value is on the token.
    MultilineString,
    /// One adopted symbol from `symbols_v0.1.0.json#/adopted`.
    Symbol,
    /// One U+0020 SPACE. The grammar's `SPACE` terminal is a single space, and
    /// several rules ("exactly one SPACE follows a comma") count them, so runs
    /// are not merged.
    Space,
    /// The U+000A that terminates a non-blank line.
    Newline,
    /// A line with no content. Spans the whole line including its LINE FEED.
    BlankLine,
    /// Indentation increased by exactly one four-space level. Zero-width.
    Indent,
    /// One closed indentation level. Zero-width.
    Dedent,
    /// End of input. Zero-width, at the source byte length.
    Eof,
}

impl TokenKind {
    /// The grammar's own name for this terminal.
    pub fn ebnf_name(self) -> &'static str {
        match self {
            TokenKind::ReservedWord => "RESERVED_WORD",
            TokenKind::SimpleIdentifier => "SIMPLE_IDENTIFIER",
            TokenKind::QualifiedIdentifier => "QUALIFIED_IDENTIFIER",
            TokenKind::IntegerLiteral => "INTEGER_LITERAL",
            TokenKind::DecimalLiteral => "DECIMAL_LITERAL",
            TokenKind::String => "STRING",
            TokenKind::MultilineString => "MULTILINE_STRING",
            TokenKind::Symbol => "SYMBOL",
            TokenKind::Space => "SPACE",
            TokenKind::Newline => "NEWLINE",
            TokenKind::BlankLine => "BLANK_LINE",
            TokenKind::Indent => "INDENT",
            TokenKind::Dedent => "DEDENT",
            TokenKind::Eof => "EOF",
        }
    }

    /// True for the structural tokens that carry no source bytes.
    pub fn is_zero_width(self) -> bool {
        matches!(self, TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof)
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ebnf_name())
    }
}

/// One lexical terminal at an exact source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Exact byte range of the lexeme in the original source.
    pub span: Span,
    /// The **decoded** value of a `String` or `MultilineString` token: escapes
    /// resolved, delimiters and the multiline indentation prefix removed.
    /// `None` for every other kind, whose value is its source slice.
    pub value: Option<String>,
    /// Which lowercase identifiers case-fold to a registered reserved word.
    ///
    /// `02_LEXICAL/02` makes case folding never *select* a keyword where an
    /// identifier is permitted: this token **is** a legal identifier (every
    /// syntax-required position already raised `error.keyword.case` instead of
    /// a token). The fold is recorded as information only, for tooling and
    /// diagnostics that want to mention the near-miss. Always `None` unless
    /// `kind` is `SimpleIdentifier`.
    ///
    /// The registry owns its spellings, so this is an owned copy of the
    /// registered word, not a borrow of a transcribed table.
    pub case_folds_to: Option<String>,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            value: None,
            case_folds_to: None,
        }
    }

    pub(crate) fn with_value(kind: TokenKind, span: Span, value: String) -> Self {
        Self {
            kind,
            span,
            value: Some(value),
            case_folds_to: None,
        }
    }
}
