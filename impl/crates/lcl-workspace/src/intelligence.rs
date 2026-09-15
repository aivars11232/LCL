//! Token spans, and the analysis the editor runs as you type.
//!
//! ## Why the browser asks for token spans
//!
//! Every editor in the world highlights with a regular expression, and for
//! most languages that is fine, because the highlighter is allowed to be
//! approximately right. Here it is not: the implementation contract says
//! neither the CLI nor the UI "may contain a second private implementation of
//! language semantics", and a pattern that decides which words are keywords is
//! exactly that, transcribed into JavaScript where it will drift the first time
//! a registry moves.
//!
//! So the real [`Lexer`](lcl_lexer::Lexer) produces the spans, classified from the real
//! [`Lexicon`](lcl_lexer::Lexicon) — `is_block_name`, `is_type_word`, `is_literal_word`, each a
//! registry lookup — and the browser paints what it is given. LCL has no
//! comment form at all, which a hand-written highlighter would almost certainly
//! have invented one for.
//!
//! ## Analysing the buffer, not the file
//!
//! The unit handed to the engine is built from the text in the editor, under
//! the document's own root-relative identity, so what is judged is what is on
//! screen. Imports still resolve through the project's provider, from disk,
//! because an import names a file and that file is whatever it currently is.

use lcl_lexer::TokenKind;
use lcl_protocol::json::{Node, Object};
use lcl_protocol::Engine;
use lcl_resolver::{SourceId, SourceUnit};

/// One painted span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan {
    pub start: usize,
    pub end: usize,
    /// The grammar's own terminal name, e.g. `RESERVED_WORD`.
    pub terminal: &'static str,
    /// Which paint class the frontend applies.
    pub class: &'static str,
}

impl TokenSpan {
    fn to_json(&self) -> Node {
        Object::new()
            .with("start", Node::usize(self.start))
            .with("end", Node::usize(self.end))
            .with("terminal", Node::string(self.terminal))
            .with("class", Node::string(self.class))
            .into()
    }
}

/// Lex one buffer and classify every token that has width.
///
/// Zero-width tokens — `INDENT`, `DEDENT`, `EOF` — carry no glyphs, and
/// whitespace needs no class, so neither is emitted. What comes back covers
/// only the bytes a reader can see.
///
/// The buffer is lexed exactly as the engine that judges it stages it. Under
/// the Core 0.2.0 engine that is after localization, so a localized reserved
/// word is classified by its canonical word while its span stays the author's
/// own bytes. A buffer whose localization fails is lexed canonically.
///
/// This is total. Staging never panics for any input, which matters because it
/// runs on every keystroke over a half-typed document.
pub fn tokens(engine: &Engine, unit: &SourceUnit) -> Vec<TokenSpan> {
    let staged = engine.stage(unit);
    let lexicon = engine.lexicon();
    let text = staged.source();
    staged
        .lexed()
        .tokens()
        .iter()
        .filter(|token| token.span.end > token.span.start)
        .filter_map(|token| {
            let class = match token.kind {
                TokenKind::ReservedWord => {
                    let word = token.word(text).unwrap_or_default();
                    if lexicon.is_block_name(word) {
                        "block"
                    } else if lexicon.is_type_word(word) {
                        "type"
                    } else if lexicon.is_literal_word(word) {
                        "literal"
                    } else {
                        "keyword"
                    }
                }
                TokenKind::SimpleIdentifier | TokenKind::QualifiedIdentifier => "ident",
                TokenKind::IntegerLiteral | TokenKind::DecimalLiteral => "literal",
                TokenKind::String | TokenKind::MultilineString => "string",
                TokenKind::Symbol => "symbol",
                // Whitespace and the zero-width structural tokens paint nothing.
                TokenKind::Space
                | TokenKind::Newline
                | TokenKind::BlankLine
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Eof => return None,
            };
            Some(TokenSpan {
                start: token.span.start,
                end: token.span.end,
                terminal: token.kind.ebnf_name(),
                class,
            })
        })
        .collect()
}

/// The token spans of one buffer, as JSON.
pub fn tokens_json(engine: &Engine, unit: &SourceUnit) -> String {
    Object::new()
        .with(
            "tokens",
            Node::array(tokens(engine, unit).iter().map(TokenSpan::to_json)),
        )
        .pretty()
}

/// One source unit built from the editor's buffer under a document's identity.
///
/// The identity is the root-relative one the project would give the file on
/// disk, so an import of it from a sibling document resolves to the same
/// identity, and a diagnostic's `source` field matches the tab it belongs to.
pub fn unit_of(id: &str, text: &str) -> SourceUnit {
    SourceUnit::new(SourceId::new(id), text.as_bytes())
}
