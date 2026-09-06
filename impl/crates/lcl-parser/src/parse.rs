//! The parser proper: a single forward pass over the M1 token stream.
//!
//! ## Whitespace is grammar, not trivia
//!
//! `02_LEXICAL/12` is explicit that "there are no line-comment symbols", so
//! there is nothing to skip. The EBNF spells `SPACE` and `NEWLINE` in its
//! productions — `FIELD_LINE = FIELD_KEY, ":", SPACE, INLINE_VALUE, NEWLINE` —
//! and several rules count spaces exactly. The cursor therefore consumes
//! whitespace tokens deliberately rather than filtering them away.
//!
//! ## Recovery
//!
//! On an unparsable statement the parser reports one registered diagnostic and
//! resynchronises to the next statement at a known indentation, discarding only
//! the bytes it could not interpret. It never substitutes a placeholder
//! identifier, type, reference or value, so no later stage can mistake a repair
//! for source.

use crate::diagnostic::{self, Cause, Diagnostic, GrammarError};
use crate::grammar::Grammar;
use crate::syntax::Document;
use crate::Parsed;
use lcl_lexer::{Lexed, Span, Token, TokenKind};

/// A forward cursor over the token stream.
pub(crate) struct Cursor<'a> {
    tokens: &'a [Token],
    index: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, index: 0 }
    }

    pub(crate) fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.index)
    }

    pub(crate) fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.index.saturating_add(offset))
    }

    pub(crate) fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    pub(crate) fn bump(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.index);
        if token.is_some() {
            self.index = self.index.saturating_add(1);
        }
        token
    }

    /// Consume one token of `kind`, or leave the cursor untouched.
    pub(crate) fn eat(&mut self, kind: TokenKind) -> Option<&'a Token> {
        if self.peek_kind() == Some(kind) {
            self.bump()
        } else {
            None
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        matches!(self.peek_kind(), None | Some(TokenKind::Eof))
    }

    /// The span to blame when the expected token is absent: the current
    /// token's start, as a zero-width locus when the stream is exhausted.
    pub(crate) fn locus(&self, eof: usize) -> Span {
        match self.peek() {
            Some(t) => t.span,
            None => Span::empty(eof),
        }
    }

    pub(crate) fn index(&self) -> usize {
        self.index
    }
}

/// Collects raw diagnostics and stamps each with its registry metadata.
pub(crate) struct Emitter<'a> {
    grammar: &'a Grammar,
    lexed: &'a Lexed,
    raw: Vec<Diagnostic>,
}

impl<'a> Emitter<'a> {
    pub(crate) fn new(grammar: &'a Grammar, lexed: &'a Lexed) -> Self {
        Self {
            grammar,
            lexed,
            raw: Vec::new(),
        }
    }

    /// Emit one diagnostic. `cause` is the `cause_identity` component of
    /// `duplicate_key`; two emissions agreeing on identifier, locus and cause
    /// are one diagnostic.
    pub(crate) fn emit(
        &mut self,
        id: GrammarError,
        span: Span,
        cause: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let registered = self.grammar.error(id);
        self.raw.push(Diagnostic {
            id,
            span,
            position: position(self.lexed, span.start),
            meaning: registered.meaning.clone(),
            default_status: registered.default_status.clone(),
            specificity_rank: registered.specificity_rank,
            cause: Cause(cause.into()),
            detail: Some(detail.into()),
        });
    }

    pub(crate) fn finish(self) -> Vec<Diagnostic> {
        diagnostic::select(self.raw, self.grammar.supersedes())
    }
}

/// Derive a human-facing position for a byte offset.
///
/// The offset is the normative part; line and column exist only for rendering,
/// so this recomputes them from the source rather than storing a second index.
fn position(lexed: &Lexed, offset: usize) -> lcl_lexer::Position {
    let text = lexed.source();
    let mut line = 1u32;
    let mut column = 1u32;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line = line.saturating_add(1);
            column = 1;
        } else {
            column = column.saturating_add(1);
        }
    }
    lcl_lexer::Position {
        offset,
        line,
        column,
    }
}

/// The `location_rule` locus for an omitted field or child block.
///
/// > An omitted field or child block uses the zero-width byte position at the
/// > first following nonblank line whose indentation is not greater than the
/// > parent block header; when no such line exists, it uses the end-of-file
/// > offset equal to the source byte length. An omitted required top-level
/// > block uses that end-of-file offset.
pub(crate) fn omission_locus(lexed: &Lexed, header_indent: usize, after: usize) -> Span {
    let text = lexed.source();
    let eof = lexed.source_len();
    let mut offset = after.min(text.len());
    // Advance to the start of the next line so the search begins after the
    // parent header's own line.
    while offset < text.len() {
        let line_end = text[offset..]
            .find('\n')
            .map(|i| offset.saturating_add(i).saturating_add(1))
            .unwrap_or(text.len());
        let line = text.get(offset..line_end).unwrap_or("");
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if !trimmed.trim().is_empty() {
            let indent = trimmed
                .len()
                .saturating_sub(trimmed.trim_start_matches(' ').len());
            if indent <= header_indent {
                return Span::empty(offset);
            }
        }
        offset = line_end;
        if line_end == offset && line_end >= text.len() {
            break;
        }
    }
    Span::empty(eof)
}

/// Parse one successfully lexed source.
pub(crate) fn parse(grammar: &Grammar, lexed: &Lexed) -> Parsed {
    let mut cursor = Cursor::new(lexed.tokens());
    let mut emitter = Emitter::new(grammar, lexed);
    let document = document(grammar, lexed, &mut cursor, &mut emitter);
    Parsed::new(document, emitter.finish())
}

/// Build the syntax tree, then judge it against the registries.
///
/// Both halves run: a structural defect does not suppress independent schema
/// evidence elsewhere in the document, and `multiplicity_rule` requires every
/// independent applicable diagnostic at the selected stage.
fn document<'a>(
    grammar: &'a Grammar,
    lexed: &'a Lexed,
    cursor: &mut Cursor<'a>,
    emitter: &mut Emitter<'a>,
) -> Document {
    let mut blocks = crate::block::BlockParser {
        grammar,
        source: lexed.source(),
        eof: lexed.source_len(),
        emitter,
    };
    let document = blocks.document(cursor);
    let mut schema = crate::schema::SchemaChecker {
        grammar,
        lexed,
        emitter,
    };
    schema.document(&document);
    document
}
