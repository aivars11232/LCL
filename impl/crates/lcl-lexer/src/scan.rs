//! The scanner.
//!
//! One deterministic pass shape, in three parts:
//!
//! 1. **Encoding gate** — byte-order mark, then UTF-8 validity. Invalid UTF-8
//!    is the only condition that stops the lexer producing a token stream at
//!    all, because there is no text to scan.
//! 2. **Raw source validation** — the character rules of `02_LEXICAL/01`, which
//!    are stated over raw source and therefore hold *inside* string literals
//!    too: tabs, carriage returns, other prohibited controls, trailing spaces,
//!    and the required final LINE FEED. Every character it rejects is recorded
//!    as *quarantined*, so part 3 steps over it without raising a second
//!    diagnostic for the same byte.
//! 3. **Tokenization** — NORMAL, STRING and MULTILINE_STRING modes, with
//!    indentation structure, per `02_LEXICAL/02` and `02_LEXICAL/07`. The
//!    scanner keeps the bounded token context the lexical rules need: one
//!    context per indentation level (block, object data, collection), the
//!    lexeme classes of the current line, and open type/`REF` brackets, so
//!    every `error.keyword.case` position of `02_LEXICAL/02` is decided here.
//! 4. **Literal constructor arguments** — a pass over the token stream that
//!    checks the closed literal profiles (REGEX, GLOB, DATE, TIME, DATETIME,
//!    URI) on literal `STRING` arguments, per `03_TYPES_AND_VALUES/04` and
//!    `/07` and the `types` and `operators_and_functions` registries.
//!
//! ## Determinism
//!
//! The output is a pure function of the input bytes and the loaded lexicon.
//! There is no clock, no filesystem access, no environment read, no hashing of
//! addresses, and no iteration over an unordered container. Diagnostics are
//! ordered by the registry's `stable_order`, never by discovery order.
//!
//! ## Recovery
//!
//! The lexer never stops at the first defect: `multiplicity_rule` requires
//! every independent diagnostic. But recovery is chosen so that one defect does
//! not manufacture a second, unrelated-looking one:
//!
//! * a lexeme that raises a diagnostic emits no token and the scanner resumes
//!   after it;
//! * an unclosed single-line string ends at its LINE FEED, so the rest of the
//!   file still lexes in NORMAL mode;
//! * an indentation defect adopts the deepest level the previous line licensed,
//!   which is the level the source almost certainly meant, so a width error
//!   does not also report an empty block.

use crate::diagnostic::{Cause, Diagnostic, LexicalError};
use crate::lexicon::{Lexicon, LiteralProfile};
use crate::literal;
use crate::span::{LineIndex, Span};
use crate::token::{Token, TokenKind};
use crate::Lexed;

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
/// `02_LEXICAL/02`: "One indentation level is exactly four ASCII spaces."
const INDENT_WIDTH: usize = 4;

/// A diagnostic before registry facts and derived positions are attached.
struct Raw {
    id: LexicalError,
    span: Span,
    cause: &'static str,
    detail: Option<String>,
}

pub(crate) fn lex(lexicon: &Lexicon, source: &[u8]) -> Lexed {
    let mut raw: Vec<Raw> = Vec::new();

    // --- 1. Encoding gate ---------------------------------------------------
    let mut start = 0usize;
    if source.starts_with(BOM) {
        raw.push(Raw {
            id: LexicalError::SourceBom,
            span: Span::new(0, BOM.len()),
            cause: "byte_order_mark",
            detail: Some("source begins with a UTF-8 byte-order mark".into()),
        });
        start = BOM.len();
    }

    let text = match std::str::from_utf8(source) {
        Ok(text) => text,
        Err(error) => {
            let at = error.valid_up_to();
            let end = match error.error_len() {
                Some(len) => at.saturating_add(len).min(source.len()),
                None => source.len(),
            };
            raw.push(Raw {
                id: LexicalError::EncodingInvalid,
                span: Span::new(at, end),
                cause: "invalid_utf8",
                detail: Some(format!("invalid UTF-8 sequence at byte {at}")),
            });
            // No text, so no token stream. The source is rejected on encoding
            // alone and no later lexical rule can be evaluated against bytes
            // that are not characters.
            return finish(lexicon, String::new(), source.len(), Vec::new(), raw);
        }
    };

    let mut scan = Scan {
        lexicon,
        text,
        bytes: source,
        quarantined: vec![false; text.len()],
        tokens: Vec::new(),
        raw,
        levels: 0,
        line_end: LineEnd::Plain,
        contexts: vec![Context::Structural],
        line_classes: Vec::new(),
        line_key: None,
        type_brackets: Vec::new(),
        ref_parens: Vec::new(),
        line_indent_spaces: 0,
        line_first_lexeme: start,
        line_token_start: 0,
        line_last_lexeme: None,
        delimiters: Vec::new(),
        pending_blank_lines: Vec::new(),
        pos: start,
    };

    // --- 2. Raw source validation ------------------------------------------
    scan.validate_characters(start);
    scan.validate_lines(start);

    // --- 3. Tokenization ----------------------------------------------------
    scan.tokenize(start);
    scan.validate_constructor_literals();

    let Scan {
        tokens, raw, text, ..
    } = scan;
    finish(lexicon, text.to_string(), source.len(), tokens, raw)
}

fn finish(
    lexicon: &Lexicon,
    text: String,
    source_len: usize,
    tokens: Vec<Token>,
    raw: Vec<Raw>,
) -> Lexed {
    let index = LineIndex::new(&text);
    let diagnostics: Vec<Diagnostic> = raw
        .into_iter()
        .map(|r| {
            let registered = lexicon.error(r.id);
            Diagnostic {
                id: r.id,
                span: r.span,
                position: index.position(&text, r.span.start),
                meaning: registered.meaning.clone(),
                default_status: registered.default_status.clone(),
                specificity_rank: registered.specificity_rank,
                cause: Cause(r.cause.to_string()),
                detail: r.detail,
            }
        })
        .collect();
    let diagnostics = crate::diagnostic::select(diagnostics, lexicon.supersedes());
    let tokens = drop_tokens_inside_diagnostics(tokens, &diagnostics, source_len);
    Lexed {
        source: text,
        source_len,
        tokens,
        diagnostics,
    }
}

/// Enforce the token/diagnostic invariant of [`crate::token`]: a lexeme
/// yields either a token or diagnostics, never both. Any token whose bytes
/// intersect a diagnostic's bytes is the offending lexeme (or contains it,
/// as a string containing a bad escape does) and is withdrawn.
///
/// Zero-width diagnostics (an absent final LINE FEED, an omitted child block)
/// name a position, not bytes, and withdraw nothing. Zero-width tokens are
/// structure, not lexemes, and are never withdrawn.
fn drop_tokens_inside_diagnostics(
    tokens: Vec<Token>,
    diagnostics: &[Diagnostic],
    source_len: usize,
) -> Vec<Token> {
    // Prefix-sum coverage so the filter is linear in source length plus
    // diagnostic count, whatever the input shape.
    let mut delta = vec![0i64; source_len.saturating_add(2)];
    for d in diagnostics {
        if d.span.is_empty() {
            continue;
        }
        let start = d.span.start.min(source_len);
        let end = d.span.end.min(source_len);
        if let Some(slot) = delta.get_mut(start) {
            *slot += 1;
        }
        if let Some(slot) = delta.get_mut(end) {
            *slot -= 1;
        }
    }
    let mut covered = Vec::with_capacity(delta.len());
    let mut depth = 0i64;
    for d in &delta {
        depth += d;
        covered.push(depth > 0);
    }
    tokens
        .into_iter()
        .filter(|t| {
            t.span.is_empty()
                || !covered
                    .get(t.span.start..t.span.end.min(source_len))
                    .is_some_and(|bytes| bytes.iter().any(|c| *c))
        })
        .collect()
}

struct Scan<'a> {
    lexicon: &'a Lexicon,
    text: &'a str,
    bytes: &'a [u8],
    /// Byte offsets whose character was already rejected by part 2.
    quarantined: Vec<bool>,
    tokens: Vec<Token>,
    raw: Vec<Raw>,
    /// Open indentation levels.
    levels: usize,
    /// How the previous logical line ended, for this line's indentation and
    /// context rules.
    line_end: LineEnd,
    /// One context per open indentation level, innermost last; always
    /// `levels + 1` long. Decides whether a lowercase key is object data.
    contexts: Vec<Context>,
    /// Lexeme classes seen on the current line, in order, for the
    /// required-connector rule.
    line_classes: Vec<LexemeClass>,
    /// The reserved word keying the current line (`KEY:` at its head).
    line_key: Option<String>,
    /// Delimiter depths at which a type-argument bracket (`LIST[`) opened.
    type_brackets: Vec<usize>,
    /// Delimiter depths at which a `REF(` opened.
    ref_parens: Vec<usize>,
    /// Leading ASCII spaces of the current line, for multiline string prefixes.
    line_indent_spaces: usize,
    /// Offset of the first lexeme on the current line.
    line_first_lexeme: usize,
    /// Index into `tokens` where the current line's tokens begin.
    line_token_start: usize,
    /// Offset of the last non-space lexeme consumed on the current line,
    /// whether it became a token or a diagnostic.
    line_last_lexeme: Option<usize>,
    /// Open `(` and `[`, with their offsets.
    delimiters: Vec<(u8, usize)>,
    /// Blank lines seen since the last non-blank line, withheld until that
    /// line's DEDENTs have been emitted. See `begin_line`.
    pending_blank_lines: Vec<Span>,
    pos: usize,
}

impl<'a> Scan<'a> {
    // -- helpers ------------------------------------------------------------

    fn byte(&self, at: usize) -> Option<u8> {
        self.bytes.get(at).copied()
    }

    /// The character at `at`, which is always a character boundary because the
    /// scanner only ever advances by whole characters.
    fn char_at(&self, at: usize) -> Option<char> {
        self.text.get(at..).and_then(|rest| rest.chars().next())
    }

    fn push(&mut self, id: LexicalError, span: Span, cause: &'static str, detail: String) {
        self.raw.push(Raw {
            id,
            span,
            cause,
            detail: Some(detail),
        });
    }

    fn token(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token::new(kind, span));
    }

    /// The end of the next line, and whether it is terminated by a LINE FEED.
    fn line_end(&self, from: usize) -> usize {
        let mut at = from;
        while at < self.text.len() && self.byte(at) != Some(b'\n') {
            at = at.saturating_add(1);
        }
        at
    }

    // -- 2. raw source validation -------------------------------------------

    /// The character prohibitions of `02_LEXICAL/01`.
    ///
    /// These are stated over *raw source*, and `02_LEXICAL/07` confirms the
    /// reading: the control scalars produced by `\n`, `\r`, `\t` and Unicode
    /// escapes "are legal values even though those controls, except source LF,
    /// cannot appear raw in source". So a raw TAB inside a STRING is still
    /// `error.source.tab`, and this pass runs without string context.
    fn validate_characters(&mut self, start: usize) {
        let mut at = start;
        while at < self.text.len() {
            let Some(c) = self.char_at(at) else { break };
            let width = c.len_utf8();
            let (id, cause, detail) = match c {
                '\n' => {
                    at = at.saturating_add(width);
                    continue;
                }
                '\t' => (
                    LexicalError::SourceTab,
                    "tab",
                    "U+0009 TAB is invalid in source, in indentation and elsewhere".to_string(),
                ),
                '\r' => (
                    LexicalError::NewlineInvalid,
                    "carriage_return",
                    "U+000D CARRIAGE RETURN: U+000A LINE FEED is the only line terminator"
                        .to_string(),
                ),
                c if is_prohibited_control(c) => (
                    LexicalError::SourceControlCharacter,
                    "control_character",
                    format!("prohibited control character U+{:04X}", c as u32),
                ),
                _ => {
                    at = at.saturating_add(width);
                    continue;
                }
            };
            self.push(id, Span::new(at, at.saturating_add(width)), cause, detail);
            if let Some(slot) = self.quarantined.get_mut(at) {
                *slot = true;
            }
            at = at.saturating_add(width);
        }
    }

    /// "A line cannot end in SPACE" and "Every non-empty source ends with one
    /// LINE FEED", both from `02_LEXICAL/01`.
    fn validate_lines(&mut self, start: usize) {
        let mut line_start = start;
        while line_start <= self.text.len() {
            let end = self.line_end(line_start);
            let mut trailing = end;
            while trailing > line_start && self.byte(trailing.saturating_sub(1)) == Some(b' ') {
                trailing = trailing.saturating_sub(1);
            }
            if trailing < end {
                self.push(
                    LexicalError::SourceTrailingSpace,
                    Span::new(trailing, end),
                    "trailing_space",
                    format!("line ends with {} ASCII space(s)", end - trailing),
                );
            }
            if end >= self.text.len() {
                break;
            }
            line_start = end.saturating_add(1);
        }

        // `location_rule`: "error.source.final_line_feed uses the end-of-file
        // offset equal to the non-empty source byte length because the required
        // byte is absent."
        if !self.bytes.is_empty() && self.bytes.last() != Some(&b'\n') {
            self.push(
                LexicalError::SourceFinalLineFeed,
                Span::empty(self.bytes.len()),
                "final_line_feed",
                "non-empty source does not end with U+000A LINE FEED".to_string(),
            );
        }
    }

    // -- 3. tokenization ----------------------------------------------------

    fn tokenize(&mut self, start: usize) {
        self.pos = start;
        let mut at_line_start = true;
        while self.pos < self.text.len() {
            if at_line_start {
                if self.begin_line() {
                    // Blank line: consumed whole, indentation untouched.
                    continue;
                }
                at_line_start = false;
            }
            if self.byte(self.pos) == Some(b'\n') {
                self.token(
                    TokenKind::Newline,
                    Span::new(self.pos, self.pos.saturating_add(1)),
                );
                self.close_line();
                self.pos = self.pos.saturating_add(1);
                at_line_start = true;
                continue;
            }
            let at = self.pos;
            let tokens_before = self.tokens.len();
            let raw_before = self.raw.len();
            self.scan_token();
            self.record_lexeme(at, tokens_before, raw_before);
        }
        if !at_line_start {
            // Final line with no LINE FEED; already diagnosed by part 2.
            self.close_line();
        }
        self.end_of_input();
    }

    /// Handle the start of a line. Returns true when the line was blank and has
    /// been consumed entirely.
    fn begin_line(&mut self) -> bool {
        let line_start = self.pos;
        let mut at = line_start;
        let mut spaces = 0usize;
        let mut has_tab = false;
        while let Some(b) = self.byte(at) {
            match b {
                b' ' => {
                    spaces = spaces.saturating_add(1);
                    at = at.saturating_add(1);
                }
                b'\t' => {
                    has_tab = true;
                    at = at.saturating_add(1);
                }
                _ => break,
            }
        }
        let whitespace_end = at;

        // A line holding nothing but already-rejected characters is still a
        // blank line structurally; looking past them keeps one bad byte from
        // also inventing an indentation defect.
        let mut probe = whitespace_end;
        while probe < self.text.len()
            && self.quarantined.get(probe).copied().unwrap_or(false)
            && self.byte(probe) != Some(b'\n')
        {
            let width = self.char_at(probe).map_or(1, char::len_utf8);
            probe = probe.saturating_add(width);
        }
        if probe >= self.text.len() || self.byte(probe) == Some(b'\n') {
            let end = self.line_end(line_start);
            let end_with_feed = if end < self.text.len() {
                end.saturating_add(1)
            } else {
                end
            };
            // `02_LEXICAL/02`: "A blank line emits one BLANK_LINE token, emits
            // no INDENT or DEDENT token". The grammar places that token after
            // the DEDENT that closes a block (`{ TOP_LEVEL_BLOCK, { BLANK_LINE } }`),
            // so it is withheld until the next non-blank line has decided
            // whether it dedents.
            self.pending_blank_lines
                .push(Span::new(line_start, end_with_feed));
            self.pos = end_with_feed;
            return true;
        }

        let previous_level = self.levels;
        let line_end = std::mem::replace(&mut self.line_end, LineEnd::Plain);
        // An opener licenses one deeper level. So does an indeterminate line
        // end: its opener was followed by a rejected byte, so whether a block
        // opened is unknowable and neither reading may fabricate a diagnostic.
        let licensed =
            previous_level.saturating_add(usize::from(!matches!(line_end, LineEnd::Plain)));
        let indent_span = Span::new(line_start, whitespace_end);

        let level = if has_tab {
            // `error.source.tab` is already recorded and the registry gives it
            // `supersedes: [error.indentation.invalid]`, so this line's
            // indentation raises nothing further.
            licensed
        } else if spaces % INDENT_WIDTH != 0 {
            self.push(
                LexicalError::IndentationWidth,
                indent_span,
                "indentation",
                format!("{spaces} leading spaces is not a multiple of {INDENT_WIDTH}"),
            );
            nearest_level(spaces).min(licensed)
        } else {
            let requested = spaces / INDENT_WIDTH;
            if requested > previous_level.saturating_add(1) {
                self.push(
                    LexicalError::IndentationJump,
                    indent_span,
                    "indentation",
                    format!(
                        "indentation increases from level {previous_level} to {requested}; at most one level is permitted"
                    ),
                );
                licensed
            } else if requested > licensed {
                self.push(
                    LexicalError::IndentationInvalid,
                    indent_span,
                    "indentation",
                    "indentation increases without a preceding block-opening colon or MULTILINE_COLLECTION `[`".to_string(),
                );
                licensed
            } else {
                requested
            }
        };

        // A block closes where its last line ended: before any blank lines
        // that separate it from the line that dedents.
        let dedent_at = self
            .pending_blank_lines
            .first()
            .map_or(line_start, |blank| blank.start);
        while self.levels > level {
            self.token(TokenKind::Dedent, Span::empty(dedent_at));
            self.levels = self.levels.saturating_sub(1);
            if self.contexts.len() > 1 {
                self.contexts.pop();
            }
        }
        self.flush_blank_lines();
        if level > self.levels {
            self.token(TokenKind::Indent, Span::empty(whitespace_end));
            self.levels = level;
            self.contexts.push(match &line_end {
                LineEnd::Opener { child, .. } => child.clone(),
                _ => Context::Indeterminate,
            });
        }

        if matches!(line_end, LineEnd::Opener { .. }) && level != previous_level.saturating_add(1) {
            // `location_rule`: an omitted child block "uses the zero-width byte
            // position at the first following nonblank line whose indentation is
            // not greater than the parent block header".
            self.push(
                LexicalError::IndentationEmptyBlock,
                Span::empty(whitespace_end),
                "empty_block",
                "block-opening declaration has no indented child statement".to_string(),
            );
        }

        self.line_indent_spaces = spaces;
        self.line_first_lexeme = whitespace_end;
        self.line_token_start = self.tokens.len();
        self.line_last_lexeme = None;
        self.line_classes.clear();
        self.line_key = None;
        self.pos = whitespace_end;
        false
    }

    /// Emit the withheld BLANK_LINE tokens, in source order.
    fn flush_blank_lines(&mut self) {
        for span in std::mem::take(&mut self.pending_blank_lines) {
            self.token(TokenKind::BlankLine, span);
        }
    }

    /// Decide how this line ended, for the next line's rules.
    ///
    /// `02_LEXICAL/02`: "A colon followed immediately by LINE FEED opens an
    /// indented block", and indentation may also increase "after the opening
    /// [ of MULTILINE_COLLECTION". The opener must be the last **lexeme** on
    /// the line, not merely the last token: when a rejected lexeme followed
    /// the colon (`VERSION: '0.1.0'`), the colon introduced an inline value,
    /// and treating it as an opener would manufacture an empty-block cascade.
    ///
    /// Between the opener and the LINE FEED only ASCII spaces may stand. Those
    /// are already `error.source.trailing_space` and do not change the
    /// structural reading. A *quarantined* byte there — a TAB, CARRIAGE RETURN
    /// or other control already rejected as raw source — means the colon was
    /// not followed by LINE FEED and the line's end is not knowable: the
    /// result is [`LineEnd::Indeterminate`], which neither demands nor forbids
    /// a child block, so the one raw-source diagnostic stays the only one.
    fn close_line(&mut self) {
        let newline_at = self.pos.min(self.text.len());
        let last_lexeme = self.line_last_lexeme;
        let opener = self
            .tokens
            .get(self.line_token_start..)
            .unwrap_or(&[])
            .iter()
            .rev()
            .find(|t| !matches!(t.kind, TokenKind::Space | TokenKind::Newline))
            .filter(|t| t.kind == TokenKind::Symbol)
            .filter(|t| Some(t.span.start) == last_lexeme)
            .filter(|t| matches!(t.span.slice(self.text), Some(":") | Some("[")))
            .map(|t| t.span);
        let Some(span) = opener else {
            self.line_end = LineEnd::Plain;
            return;
        };
        let corrupted =
            (span.end..newline_at).any(|at| self.quarantined.get(at).copied().unwrap_or(false));
        if corrupted {
            self.line_end = LineEnd::Indeterminate;
            return;
        }
        let child = self.child_context(span);
        self.line_end = LineEnd::Opener { span, child };
    }

    /// The context of the block a just-closed opener line introduces.
    ///
    /// * `[` opens a MULTILINE_COLLECTION: member lines are expressions.
    /// * `KEY:` with a registered block name opens that block.
    /// * `KEY:` whose `(enclosing block, KEY)` signature is
    ///   `value_or_object_expression` opens object data ("VALUE contains an
    ///   inline expression or an indented OBJECT-data body", `04_GRAMMAR/12`).
    /// * A lowercase key inside object data opens nested object data.
    /// * Everything else — a nested schema, a conditional or loop body, an
    ///   `ELSE:` — is structural: only reserved words may head its lines.
    fn child_context(&self, opener: Span) -> Context {
        if opener.slice(self.text) == Some("[") {
            return Context::Collection;
        }
        let line = self.tokens.get(self.line_token_start..).unwrap_or(&[]);
        let Some(head) = line.first() else {
            return Context::Structural;
        };
        if head.span.start != self.line_first_lexeme {
            return Context::Structural;
        }
        let head_text = head.span.slice(self.text).unwrap_or("");
        match head.kind {
            TokenKind::ReservedWord => {
                let bare_key = line.get(1).map(|t| t.span) == Some(opener);
                if !bare_key {
                    Context::Structural
                } else if self.lexicon.is_block_name(head_text) {
                    Context::Block(head_text.to_string())
                } else if self
                    .lexicon
                    .is_object_data_field(self.enclosing_block(), head_text)
                {
                    Context::ObjectData
                } else {
                    Context::Structural
                }
            }
            TokenKind::SimpleIdentifier | TokenKind::QualifiedIdentifier => {
                match self.contexts.last() {
                    Some(Context::ObjectData) => Context::ObjectData,
                    Some(Context::Indeterminate) => Context::Indeterminate,
                    _ => Context::Structural,
                }
            }
            _ => Context::Structural,
        }
    }

    /// The nearest enclosing registered block, if any.
    fn enclosing_block(&self) -> Option<&str> {
        self.contexts.iter().rev().find_map(|c| match c {
            Context::Block(name) => Some(name.as_str()),
            _ => None,
        })
    }

    /// Record what the lexeme starting at `at` became, for the line rules.
    fn record_lexeme(&mut self, at: usize, tokens_before: usize, raw_before: usize) {
        if self.quarantined.get(at).copied().unwrap_or(false) {
            return;
        }
        if self.byte(at) == Some(b' ') {
            self.line_classes.push(LexemeClass::Space);
            return;
        }
        self.line_last_lexeme = Some(at);
        if self.tokens.len() > tokens_before {
            let Some(token) = self.tokens.last() else {
                return;
            };
            let lexeme = token.span.slice(self.text).unwrap_or("");
            if token.kind == TokenKind::ReservedWord
                && token.span.start == self.line_first_lexeme
                && self.byte(token.span.end) == Some(b':')
            {
                self.line_key = Some(lexeme.to_string());
            }
            let class = match token.kind {
                TokenKind::IntegerLiteral
                | TokenKind::DecimalLiteral
                | TokenKind::String
                | TokenKind::MultilineString
                | TokenKind::SimpleIdentifier
                | TokenKind::QualifiedIdentifier => LexemeClass::Operand,
                TokenKind::Symbol if matches!(lexeme, ")" | "]") => LexemeClass::Operand,
                TokenKind::ReservedWord if self.lexicon.is_literal_word(lexeme) => {
                    LexemeClass::Operand
                }
                _ => LexemeClass::Other,
            };
            self.line_classes.push(class);
        } else if self.raw.len() > raw_before {
            self.line_classes.push(LexemeClass::Diagnosed);
        }
    }

    fn end_of_input(&mut self) {
        let eof = self.text.len();
        let line_end = std::mem::replace(&mut self.line_end, LineEnd::Plain);
        if matches!(line_end, LineEnd::Opener { .. }) {
            self.push(
                LexicalError::IndentationEmptyBlock,
                Span::empty(eof),
                "empty_block",
                "block-opening declaration has no indented child statement before end of input"
                    .to_string(),
            );
        }
        let dedent_at = self
            .pending_blank_lines
            .first()
            .map_or(eof, |blank| blank.start);
        while self.levels > 0 {
            self.token(TokenKind::Dedent, Span::empty(dedent_at));
            self.levels = self.levels.saturating_sub(1);
        }
        self.flush_blank_lines();
        for (open, at) in std::mem::take(&mut self.delimiters) {
            self.push(
                LexicalError::DelimiterUnclosed,
                Span::new(at, at.saturating_add(1)),
                "delimiter",
                format!("`{}` is never closed", open as char),
            );
        }
        self.token(TokenKind::Eof, Span::empty(eof));
    }

    fn scan_token(&mut self) {
        let at = self.pos;
        if self.quarantined.get(at).copied().unwrap_or(false) {
            let width = self.char_at(at).map_or(1, char::len_utf8);
            self.pos = at.saturating_add(width);
            return;
        }
        let Some(b) = self.byte(at) else {
            self.pos = self.text.len();
            return;
        };
        match b {
            b' ' => {
                self.token(TokenKind::Space, Span::new(at, at.saturating_add(1)));
                self.pos = at.saturating_add(1);
            }
            b'"' => self.scan_string(),
            b'0'..=b'9' => self.scan_number(),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.scan_word(),
            b if b >= 0x80 => {
                let c = self.char_at(at).unwrap_or('\u{FFFD}');
                let width = c.len_utf8();
                // `02_LEXICAL/01`: "Outside STRING and MULTILINE_STRING
                // literals, only ASCII letters, digits, U+0020 SPACE, LINE FEED,
                // and symbols in symbols_v0.1.0.json are legal."
                self.push(
                    LexicalError::SourceNonAsciiOutsideString,
                    Span::new(at, at.saturating_add(width)),
                    "non_ascii",
                    format!("U+{:04X} occurs outside a string literal", c as u32),
                );
                self.pos = at.saturating_add(width);
            }
            _ => self.scan_symbol(),
        }
    }
}

/// C0 controls other than LINE FEED, DELETE, and the C1 controls.
///
/// TAB and CARRIAGE RETURN are excluded here only because they have their own
/// registered identifiers; they are prohibited just the same.
fn is_prohibited_control(c: char) -> bool {
    let v = c as u32;
    (v < 0x20 && c != '\n' && c != '\t' && c != '\r') || v == 0x7F || (0x80..=0x9F).contains(&v)
}

/// The level a malformed indentation width most nearly names, rounding halves
/// up. Only ever used as recovery, never to accept the source.
fn nearest_level(spaces: usize) -> usize {
    spaces
        .saturating_add(INDENT_WIDTH / 2)
        .saturating_div(INDENT_WIDTH)
}

// ---------------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------------

impl Scan<'_> {
    /// `symbols_v0.1.0.json#/selection_rule`: "In NORMAL mode choose the longest
    /// exact lexeme across adopted and excluded_exact_lexemes before accepting
    /// or rejecting it."
    ///
    /// A registered notation *pattern* is tried first, because
    /// `02_LEXICAL/02` requires an excluded multi-character form to "be reported
    /// as that invalid form rather than split into otherwise adopted prefix
    /// symbols", and a tag is longer than the `<` or `<=` it starts with.
    fn scan_symbol(&mut self) {
        let at = self.pos;
        let rest = self.text.get(at..).unwrap_or("");

        if let Some(length) = xml_tag_length(rest) {
            let lexeme = rest.get(..length).unwrap_or("").to_string();
            self.push(
                LexicalError::SymbolInvalid,
                Span::new(at, at.saturating_add(length)),
                "excluded_notation_pattern",
                format!("`{lexeme}` is the excluded xml_tag notation pattern"),
            );
            self.pos = at.saturating_add(length);
            return;
        }

        match self.lexicon.longest_lexeme_at(rest) {
            Some((lexeme, true)) => {
                let length = lexeme.len();
                let span = Span::new(at, at.saturating_add(length));
                if lexeme == "\\" {
                    // Adopted, but registered as "Introduce an escape inside a
                    // string only". In NORMAL mode it starts no registered
                    // token, which is exactly what error.lexical.malformed_token
                    // describes.
                    self.push(
                        LexicalError::LexicalMalformedToken,
                        span,
                        "malformed_token",
                        "`\\` is registered only inside a string literal".to_string(),
                    );
                } else {
                    self.track_delimiter(lexeme, at);
                    self.token(TokenKind::Symbol, span);
                }
                self.pos = at.saturating_add(length);
            }
            Some((lexeme, false)) => {
                let length = lexeme.len();
                self.push(
                    LexicalError::SymbolInvalid,
                    Span::new(at, at.saturating_add(length)),
                    "excluded_lexeme",
                    format!("`{lexeme}` is an excluded symbol"),
                );
                self.pos = at.saturating_add(length);
            }
            None => {
                let c = self.char_at(at).unwrap_or('\u{FFFD}');
                let width = c.len_utf8();
                self.push(
                    LexicalError::LexicalUnknownSymbol,
                    Span::new(at, at.saturating_add(width)),
                    "unknown_symbol",
                    format!(
                        "`{c}` is in neither the adopted-symbol registry nor the excluded-symbol inventory"
                    ),
                );
                self.pos = at.saturating_add(width);
            }
        }
    }

    /// Track `(` / `[` pairing, the only lexical structure the two registered
    /// delimiter errors describe.
    fn track_delimiter(&mut self, lexeme: &str, at: usize) {
        let span = Span::new(at, at.saturating_add(lexeme.len()));
        // The word immediately before this delimiter, with no space between.
        let adjacent_word = self
            .tokens
            .last()
            .filter(|t| t.kind == TokenKind::ReservedWord && t.span.end == at)
            .and_then(|t| t.span.slice(self.text));
        match lexeme {
            "(" => {
                self.delimiters.push((b'(', at));
                if adjacent_word == Some("REF") {
                    self.ref_parens.push(self.delimiters.len());
                }
            }
            "[" => {
                self.delimiters.push((b'[', at));
                // `LIST[`, `SET[`, `OBJECT[`, `REFERENCE[`: a type argument.
                if adjacent_word.is_some_and(|w| self.lexicon.is_type_word(w)) {
                    self.type_brackets.push(self.delimiters.len());
                }
            }
            ")" | "]" => {
                let expected = if lexeme == ")" { b'(' } else { b'[' };
                match self.delimiters.pop() {
                    Some((open, _)) if open == expected => {}
                    Some((open, open_at)) => self.push(
                        LexicalError::DelimiterMismatch,
                        span,
                        "delimiter",
                        format!(
                            "`{lexeme}` closes `{}` opened at byte {open_at}",
                            open as char
                        ),
                    ),
                    None => self.push(
                        LexicalError::DelimiterMismatch,
                        span,
                        "delimiter",
                        format!("`{lexeme}` has no open delimiter"),
                    ),
                }
                let depth = self.delimiters.len();
                while self.ref_parens.last().is_some_and(|&d| d > depth) {
                    self.ref_parens.pop();
                }
                while self.type_brackets.last().is_some_and(|&d| d > depth) {
                    self.type_brackets.pop();
                }
            }
            _ => {}
        }
    }
}

/// Length of the `excluded_notation_patterns.xml_tag` form at the start of
/// `rest`, if it is one. Requires the closing `>` on the same token, so an
/// ordinary `<` comparison operator can never match.
fn xml_tag_length(rest: &str) -> Option<usize> {
    let b = rest.as_bytes();
    if b.first() != Some(&b'<') {
        return None;
    }
    let mut at = 1usize;
    if b.get(at) == Some(&b'/') {
        at = at.saturating_add(1);
    }
    match b.get(at) {
        Some(c) if c.is_ascii_alphabetic() => at = at.saturating_add(1),
        _ => return None,
    }
    while let Some(c) = b.get(at) {
        if c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b':') {
            at = at.saturating_add(1);
        } else {
            break;
        }
    }
    if b.get(at) == Some(&b'/') {
        at = at.saturating_add(1);
    }
    if b.get(at) == Some(&b'>') {
        Some(at.saturating_add(1))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Words: reserved words and identifiers
// ---------------------------------------------------------------------------

impl Scan<'_> {
    /// `02_LEXICAL/02`: "A reserved word is complete only when neither adjacent
    /// source character is an ASCII letter, digit, or underscore. An
    /// unregistered uppercase run is one invalid token and is not split into
    /// shorter reserved words."
    ///
    /// Consuming the maximal `[A-Za-z0-9_]` run first is that rule: the run is
    /// classified as a whole, never re-split.
    fn scan_word(&mut self) {
        let start = self.pos;
        let mut at = start;
        while matches!(self.byte(at), Some(c) if is_word_byte(c)) {
            at = at.saturating_add(1);
        }
        self.pos = at;
        let run = self.text.get(start..at).unwrap_or("");

        if is_simple_identifier(run) {
            self.finish_identifier(start, run);
        } else if is_uppercase_word(run) {
            if self.lexicon.is_reserved_word(run) {
                self.token(TokenKind::ReservedWord, Span::new(start, at));
            } else {
                self.push(
                    LexicalError::KeywordUnknown,
                    Span::new(start, at),
                    "keyword",
                    format!("`{run}` is not in the closed reserved word list"),
                );
            }
        } else if let Some(word) = self.lexicon.case_folded_word(run) {
            // "A mixed-case token matching a registered word also uses
            // error.keyword.case, because mixed-case identifiers are forbidden."
            let word = word.to_string();
            self.push(
                LexicalError::KeywordCase,
                Span::new(start, at),
                "keyword_case",
                format!("`{run}` is a mixed-case spelling of the registered word `{word}`"),
            );
        } else {
            self.push(
                LexicalError::IdentifierInvalid,
                Span::new(start, at),
                "identifier",
                format!("`{run}` is neither `[a-z][a-z0-9_]*` nor a registered word"),
            );
        }
    }

    /// Complete a lowercase run into a simple or qualified identifier.
    ///
    /// `02_LEXICAL/02`: "A lowercase identifier is consumed maximally. If it
    /// contains one or more complete dot-separated lowercase segments, the whole
    /// sequence is one QUALIFIED_IDENTIFIER; otherwise it is one
    /// SIMPLE_IDENTIFIER."
    fn finish_identifier(&mut self, start: usize, first_segment: &str) {
        let mut segments = 1usize;
        let mut malformed_segment = false;
        loop {
            if self.byte(self.pos) != Some(b'.') {
                break;
            }
            // A dot only continues the identifier when a lowercase segment
            // follows it; otherwise it is the adopted `.` symbol, which is what
            // makes the dot in `REF(a).b` a property access.
            match self.byte(self.pos.saturating_add(1)) {
                Some(c) if c.is_ascii_lowercase() => {}
                _ => break,
            }
            self.pos = self.pos.saturating_add(1);
            let segment_start = self.pos;
            while matches!(self.byte(self.pos), Some(c) if is_word_byte(c)) {
                self.pos = self.pos.saturating_add(1);
            }
            let segment = self.text.get(segment_start..self.pos).unwrap_or("");
            if !is_simple_identifier(segment) {
                malformed_segment = true;
            }
            segments = segments.saturating_add(1);
        }
        let span = Span::new(start, self.pos);
        let lexeme = span.slice(self.text).unwrap_or("");

        if malformed_segment {
            self.push(
                LexicalError::IdentifierInvalid,
                span,
                "identifier",
                format!("`{lexeme}` has a segment that is not `[a-z][a-z0-9_]*`"),
            );
            return;
        }

        if segments == 1 {
            if let Some(word) = self.lexicon.case_folded_word(first_segment) {
                let word = word.to_string();
                if let Some(position) = self.required_keyword_position(start, &word) {
                    self.push(
                        LexicalError::KeywordCase,
                        span,
                        "keyword_case",
                        format!(
                            "`{first_segment}` is the lowercase spelling of the registered word `{word}` in {position}"
                        ),
                    );
                    return;
                }
            }
        }

        let kind = if segments == 1 {
            TokenKind::SimpleIdentifier
        } else {
            TokenKind::QualifiedIdentifier
        };
        let case_folds_to = if segments == 1 {
            self.lexicon
                .case_folded_word(first_segment)
                .map(str::to_string)
        } else {
            None
        };
        self.tokens.push(Token {
            kind,
            span,
            value: None,
            case_folds_to,
        });
    }

    /// The syntax-required positions of `error.keyword.case`, decided from
    /// the token context this lexer already tracks and with no name lookup.
    ///
    /// `02_LEXICAL/02` lists four, and each is a fact of the grammar or a
    /// registry, not a guess:
    ///
    /// * **A block or field key outside object data.** At the head of a line
    ///   in a structural context the grammar admits only a reserved word
    ///   (`FIELD_KEY`, `BLOCK_WORD`, `IF`, `FOR`, `ELSE`). Object data — the
    ///   indented body of a `value_or_object_expression` field or of a nested
    ///   lowercase key — is where `OBJECT_PROPERTY` keys are legal, and a
    ///   MULTILINE_COLLECTION member line is an expression, so neither is
    ///   checked. An indeterminate context (an opener line corrupted by a
    ///   rejected byte) is not checked either.
    /// * **A required connector or operator.** After a complete operand and a
    ///   SPACE the grammar admits only an operator word or symbol
    ///   (`OR_EXPRESSION`, `AND_EXPRESSION`, `COMPARISON`, `ADDITIVE`,
    ///   `MULTIPLICATIVE`), `THEN` after a condition's `)`, or `IN` after a
    ///   loop variable. No lowercase identifier is ever legal there.
    /// * **A registered callable immediately before `(`.** `CALL` requires an
    ///   uppercase `CALLABLE` before the parenthesis.
    /// * **A built-in type position.** The inline value of a field whose
    ///   signature is `type_expression` or `type_or_format_base`, and the
    ///   argument of a type bracket (`LIST[`, `SET[`, `OBJECT[`,
    ///   `REFERENCE[`). `TYPE_EXPRESSION` admits a lowercase identifier only
    ///   inside `REF(...)`, which is excluded.
    ///
    /// Everywhere else a lowercase identifier is permitted, and "its
    /// case-folded spelling never selects a keyword or literal".
    fn required_keyword_position(&self, start: usize, word: &str) -> Option<&'static str> {
        let head = start == self.line_first_lexeme;
        let context = self.contexts.last().cloned().unwrap_or(Context::Structural);
        if head && matches!(context, Context::Structural | Context::Block(_)) {
            return Some("a block or field key position outside object data");
        }
        if self.byte(self.pos) == Some(b'(') && self.lexicon.is_callable(word) {
            return Some("a registered callable position before `(`");
        }
        let n = self.line_classes.len();
        if n >= 2
            && self.line_classes[n - 1] == LexemeClass::Space
            && self.line_classes[n - 2] == LexemeClass::Operand
        {
            return Some("a required connector or operator position");
        }
        let in_type_field = self
            .line_key
            .as_deref()
            .is_some_and(|key| self.lexicon.is_type_position_field(key));
        if self.ref_parens.is_empty() && (in_type_field || !self.type_brackets.is_empty()) {
            return Some("a built-in type position");
        }
        None
    }
}

/// What a line's opener licensed for the next line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LineEnd {
    /// No block opened; the next line may not indent.
    Plain,
    /// A block-opening `:` or MULTILINE_COLLECTION `[` was the last lexeme and
    /// reached the LINE FEED across spaces only.
    Opener { span: Span, child: Context },
    /// The opener was followed by a byte already rejected as raw source. The
    /// next line may indent or not; neither raises a structural diagnostic.
    Indeterminate,
}

/// The kind of body an indentation level holds.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Context {
    /// Top level, a conditional or loop body, an `ELSE:` body, a nested
    /// schema: lines are statements headed by reserved words.
    Structural,
    /// The body of a registered block, by name.
    Block(String),
    /// Lowercase-key object data.
    ObjectData,
    /// MULTILINE_COLLECTION members: expression lines.
    Collection,
    /// Unknown, because the opener line was corrupted by a rejected byte.
    Indeterminate,
}

/// What a lexeme on the current line was, for the connector rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexemeClass {
    /// Completes an operand: a literal, an identifier, `)`, `]`, or a literal
    /// word such as `TRUE`.
    Operand,
    Space,
    /// Any other token.
    Other,
    /// A lexeme that raised a diagnostic; breaks adjacency so a rejected
    /// lexeme never manufactures a connector position.
    Diagnosed,
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `SIMPLE_IDENTIFIER = LOWER, { LOWER | DIGIT | "_" }`.
fn is_simple_identifier(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The shape every registered reserved word has.
fn is_uppercase_word(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------------------
// Numeric literals
// ---------------------------------------------------------------------------

impl Scan<'_> {
    /// `INTEGER = 0|[1-9][0-9]*`, `DECIMAL = (0|[1-9][0-9]*)\.[0-9]+`.
    ///
    /// "The hyphen-minus is a separate unary operator token; it is not part of
    /// either numeric literal token", so this never consumes a sign. "Leading
    /// plus signs, numeric separators, exponent notation, NaN, and infinity are
    /// invalid in Core 0.1.0", which falls out of validating the consumed run
    /// against the two productions rather than parsing leniently.
    fn scan_number(&mut self) {
        let start = self.pos;
        let mut at = start;
        while matches!(self.byte(at), Some(c) if c.is_ascii_digit()) {
            at = at.saturating_add(1);
        }
        // A fraction part only continues the literal when a digit follows the
        // dot, so `1.foo` stays INTEGER + `.` + identifier.
        while self.byte(at) == Some(b'.')
            && matches!(self.byte(at.saturating_add(1)), Some(c) if c.is_ascii_digit())
        {
            at = at.saturating_add(1);
            while matches!(self.byte(at), Some(c) if c.is_ascii_digit()) {
                at = at.saturating_add(1);
            }
        }

        // A literal cannot abut an identifier character, by the same
        // completeness rule reserved words follow.
        let mut end = at;
        let abuts = matches!(self.byte(end), Some(c) if is_word_byte(c));
        while matches!(self.byte(end), Some(c) if is_word_byte(c)) {
            end = end.saturating_add(1);
        }

        let span = Span::new(start, end);
        let lexeme = span.slice(self.text).unwrap_or("");
        self.pos = end;

        if !abuts && is_integer_literal(lexeme) {
            self.token(TokenKind::IntegerLiteral, span);
        } else if !abuts && is_decimal_literal(lexeme) {
            self.token(TokenKind::DecimalLiteral, span);
        } else {
            self.push(
                LexicalError::LiteralInvalid,
                span,
                "numeric_literal",
                format!("`{lexeme}` is neither `0|[1-9][0-9]*` nor `(0|[1-9][0-9]*)\\.[0-9]+`"),
            );
        }
    }
}

fn is_integer_literal(lexeme: &str) -> bool {
    match lexeme.as_bytes() {
        [b'0'] => true,
        [first, rest @ ..] if (b'1'..=b'9').contains(first) => rest.iter().all(u8::is_ascii_digit),
        _ => false,
    }
}

fn is_decimal_literal(lexeme: &str) -> bool {
    match lexeme.split_once('.') {
        Some((whole, fraction)) => {
            is_integer_literal(whole)
                && !fraction.is_empty()
                && fraction.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

// ---------------------------------------------------------------------------
// String literals and escapes
// ---------------------------------------------------------------------------

impl Scan<'_> {
    fn scan_string(&mut self) {
        if self
            .text
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with("\"\"\""))
        {
            self.scan_multiline_string();
        } else {
            self.scan_single_line_string();
        }
    }

    /// `STRING = '"', { STRING_CHARACTER | ESCAPE }, '"'`, where
    /// `STRING_CHARACTER` is any "Unicode scalar except LF, CR, double quote,
    /// backslash, and prohibited controls".
    ///
    /// A LINE FEED therefore closes nothing: it ends the attempt with
    /// `error.literal.unclosed`, and the scanner resumes in NORMAL mode on the
    /// next line rather than swallowing the rest of the file.
    fn scan_single_line_string(&mut self) {
        let start = self.pos;
        self.pos = self.pos.saturating_add(1);
        let mut value = String::new();
        loop {
            let at = self.pos;
            if at >= self.text.len() {
                self.unclosed_string(start, self.text.len());
                return;
            }
            if self.byte(at) == Some(b'\n') {
                self.unclosed_string(start, at);
                return;
            }
            if self.quarantined.get(at).copied().unwrap_or(false) {
                // Already rejected as raw source; excluded from the decoded
                // value, which is undefined for a rejected document anyway.
                let width = self.char_at(at).map_or(1, char::len_utf8);
                self.pos = at.saturating_add(width);
                continue;
            }
            let Some(c) = self.char_at(at) else {
                self.unclosed_string(start, self.text.len());
                return;
            };
            match c {
                '"' => {
                    self.pos = at.saturating_add(1);
                    self.tokens.push(Token::with_value(
                        TokenKind::String,
                        Span::new(start, self.pos),
                        value,
                    ));
                    return;
                }
                '\\' => self.decode_escape(&mut value),
                _ => {
                    value.push(c);
                    self.pos = at.saturating_add(c.len_utf8());
                }
            }
        }
    }

    fn unclosed_string(&mut self, start: usize, end: usize) {
        // `location_rule` uses the first offending byte: the literal that is not
        // closed begins at its opening delimiter.
        self.push(
            LexicalError::LiteralUnclosed,
            Span::new(start, end),
            "string",
            "string literal is not closed before the end of its line".to_string(),
        );
        self.pos = end;
    }

    /// `MULTILINE STRING`, per `02_LEXICAL/07`.
    ///
    /// "Multiline STRING has an opening triple quote immediately followed by LF
    /// and a closing triple quote alone on a line at the containing
    /// declaration's indentation. ... Each nonblank content line must begin with
    /// the declaration's indentation plus four ASCII spaces; strip exactly that
    /// prefix." Content lines "do not emit structural INDENT, DEDENT, or
    /// BLANK_LINE tokens", which is why they are consumed here rather than by
    /// the line loop.
    fn scan_multiline_string(&mut self) {
        let start = self.pos;
        let declaration_indent = self.line_indent_spaces;
        let content_prefix = declaration_indent.saturating_add(INDENT_WIDTH);
        self.pos = start.saturating_add(3);

        if self.byte(self.pos) != Some(b'\n') {
            self.push(
                LexicalError::LiteralInvalid,
                Span::new(start, self.pos),
                "multiline_open",
                "opening `\"\"\"` must be followed immediately by LINE FEED".to_string(),
            );
            return;
        }
        self.pos = self.pos.saturating_add(1);

        let mut value = String::new();
        loop {
            let line_start = self.pos;
            if line_start >= self.text.len() {
                self.push(
                    LexicalError::LiteralUnclosed,
                    Span::new(start, self.text.len()),
                    "string",
                    "multiline string reaches end of input without its closing `\"\"\"`"
                        .to_string(),
                );
                self.pos = self.text.len();
                return;
            }
            let line_end = self.line_end(line_start);
            let line = self.text.get(line_start..line_end).unwrap_or("");

            if is_closing_delimiter_line(line, declaration_indent) {
                let end = line_start
                    .saturating_add(declaration_indent)
                    .saturating_add(3);
                self.tokens.push(Token::with_value(
                    TokenKind::MultilineString,
                    Span::new(start, end),
                    value,
                ));
                self.pos = end;
                return;
            }

            // "An unescaped triple quote is only that closing delimiter, never
            // content", so one anywhere else is a misaligned delimiter.
            if has_unescaped_triple_quote(line) {
                self.push(
                    LexicalError::LiteralInvalid,
                    Span::new(line_start, line_end),
                    "multiline_delimiter",
                    "closing `\"\"\"` must be alone on its line at the declaration's indentation"
                        .to_string(),
                );
                if line.trim_start_matches(' ') == "\"\"\"" {
                    // A lone but misaligned `\"\"\"` is the intended closer:
                    // end the literal here rather than swallowing the rest of
                    // the file into an unclosed string.
                    self.pos = line_end;
                    return;
                }
                // Otherwise the quote is inside content; keep scanning for the
                // real closer so one defect stays one diagnostic.
            }

            if line.is_empty() {
                // "A blank content line contributes one LF."
                value.push('\n');
                self.pos = line_end.saturating_add(1).min(self.text.len());
                continue;
            }

            let indent = line.len() - line.trim_start_matches(' ').len();
            let content_start = if indent >= content_prefix {
                line_start.saturating_add(content_prefix)
            } else {
                self.push(
                    LexicalError::LiteralInvalid,
                    Span::new(line_start, line_end),
                    "multiline_prefix",
                    format!(
                        "content line must begin with {content_prefix} ASCII spaces, found {indent}"
                    ),
                );
                line_start.saturating_add(indent)
            };

            self.decode_region(content_start, line_end, &mut value);
            // "Preserve any additional spaces and the LF ending every content
            // line."
            value.push('\n');
            self.pos = line_end.saturating_add(1).min(self.text.len());
        }
    }

    /// Decode `[from, to)` of the source into `value`, resolving escapes.
    fn decode_region(&mut self, from: usize, to: usize, value: &mut String) {
        self.pos = from;
        while self.pos < to {
            let at = self.pos;
            if self.quarantined.get(at).copied().unwrap_or(false) {
                let width = self.char_at(at).map_or(1, char::len_utf8);
                self.pos = at.saturating_add(width);
                continue;
            }
            let Some(c) = self.char_at(at) else { break };
            if c == '\\' {
                self.decode_escape(value);
            } else {
                value.push(c);
                self.pos = at.saturating_add(c.len_utf8());
            }
        }
        self.pos = to;
    }

    /// `ESCAPE = "\", ( '"' | "\" | "n" | "r" | "t" | "u", HEX, HEX, HEX, HEX )`.
    ///
    /// "decode escapes exactly once from left to right ... A decoded backslash
    /// is material text and does not begin a second escape-decoding pass."
    fn decode_escape(&mut self, value: &mut String) {
        let at = self.pos;
        let after = at.saturating_add(1);
        let Some(c) = self.char_at(after) else {
            self.bad_escape(at, after, "escape has no character after `\\`");
            return;
        };
        match c {
            '\n' => self.bad_escape(at, after, "escape has no character after `\\`"),
            '"' => {
                value.push('"');
                self.pos = at.saturating_add(2);
            }
            '\\' => {
                value.push('\\');
                self.pos = at.saturating_add(2);
            }
            'n' => {
                value.push('\n');
                self.pos = at.saturating_add(2);
            }
            'r' => {
                value.push('\r');
                self.pos = at.saturating_add(2);
            }
            't' => {
                value.push('\t');
                self.pos = at.saturating_add(2);
            }
            'u' => self.decode_unicode_escape(value),
            other => self.bad_escape(
                at,
                after.saturating_add(other.len_utf8()),
                &format!("`\\{other}` is not a registered escape"),
            ),
        }
    }

    /// `\uHHHH`, including the surrogate-pair contract of `02_LEXICAL/07`:
    /// a high surrogate "must immediately be followed by a low-surrogate escape
    /// in DC00..DFFF"; an unpaired surrogate or invalid pair is
    /// `error.literal.escape`.
    fn decode_unicode_escape(&mut self, value: &mut String) {
        let at = self.pos;
        let Some((high, after_high)) = self.read_hex_quad(at) else {
            return;
        };

        if (0xD800..=0xDBFF).contains(&high) {
            let paired = self
                .text
                .get(after_high..)
                .is_some_and(|rest| rest.starts_with("\\u"))
                .then(|| self.read_hex_quad_silent(after_high))
                .flatten()
                .filter(|(low, _)| (0xDC00..=0xDFFF).contains(low));
            match paired {
                Some((low, after_low)) => {
                    let scalar = 0x10000u32
                        .saturating_add(high.saturating_sub(0xD800).saturating_mul(0x400))
                        .saturating_add(low.saturating_sub(0xDC00));
                    match char::from_u32(scalar) {
                        Some(c) => {
                            value.push(c);
                            self.pos = after_low;
                        }
                        None => self.bad_escape(
                            at,
                            after_low,
                            "surrogate pair does not denote a Unicode scalar value",
                        ),
                    }
                }
                None => self.bad_escape(
                    at,
                    after_high,
                    "high surrogate escape is not immediately followed by a low surrogate escape",
                ),
            }
            return;
        }

        if (0xDC00..=0xDFFF).contains(&high) {
            self.bad_escape(at, after_high, "unpaired low surrogate escape");
            return;
        }

        match char::from_u32(high) {
            Some(c) => {
                value.push(c);
                self.pos = after_high;
            }
            None => self.bad_escape(
                at,
                after_high,
                "escape does not denote a Unicode scalar value",
            ),
        }
    }

    /// Read `\uHHHH` at `at`, reporting a malformed quad. Returns the value and
    /// the offset just past it.
    fn read_hex_quad(&mut self, at: usize) -> Option<(u32, usize)> {
        match self.read_hex_quad_silent(at) {
            Some(found) => Some(found),
            None => {
                let mut end = at.saturating_add(2);
                while end < self.text.len()
                    && end < at.saturating_add(6)
                    && matches!(self.byte(end), Some(c) if c.is_ascii_hexdigit())
                {
                    end = end.saturating_add(1);
                }
                self.bad_escape(at, end, "`\\u` needs exactly four hexadecimal digits");
                None
            }
        }
    }

    fn read_hex_quad_silent(&self, at: usize) -> Option<(u32, usize)> {
        let digits_start = at.saturating_add(2);
        let digits_end = digits_start.saturating_add(4);
        let digits = self.text.get(digits_start..digits_end)?;
        if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16)
            .ok()
            .map(|value| (value, digits_end))
    }

    fn bad_escape(&mut self, start: usize, end: usize, detail: &str) {
        let end = end.max(start.saturating_add(1)).min(self.text.len());
        self.push(
            LexicalError::LiteralEscape,
            Span::new(start, end),
            "escape",
            detail.to_string(),
        );
        self.pos = end;
    }
}

/// A closing multiline delimiter line: exactly the declaration's indentation
/// then `"""` and nothing else.
fn is_closing_delimiter_line(line: &str, declaration_indent: usize) -> bool {
    line.len() == declaration_indent.saturating_add(3)
        && line
            .get(..declaration_indent)
            .is_some_and(|prefix| prefix.bytes().all(|b| b == b' '))
        && line.get(declaration_indent..) == Some("\"\"\"")
}

fn has_unescaped_triple_quote(line: &str) -> bool {
    let b = line.as_bytes();
    let mut at = 0usize;
    while at < b.len() {
        match b.get(at) {
            Some(b'\\') => at = at.saturating_add(2),
            Some(b'"') if b.get(at..at.saturating_add(3)) == Some(b"\"\"\"") => return true,
            _ => at = at.saturating_add(1),
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Literal constructor arguments
// ---------------------------------------------------------------------------

impl Scan<'_> {
    /// Validate the literal `STRING` arguments of registered constructors that
    /// carry a closed literal profile and stage `error.literal.invalid` as
    /// lexical: REGEX (pattern and flags), GLOB, DATE, TIME, DATETIME, URI.
    ///
    /// This is token-context validation, not evaluation. Only a call whose
    /// arity is a registered all-`STRING` overload and whose every argument is
    /// one `STRING` token is checked; an argument that is a reference,
    /// expression or other constructor is dynamically supplied material whose
    /// value-domain check belongs to `expression_demand_resolution`, and a
    /// wrong arity is `error.operator.operand` at the static stage.
    fn validate_constructor_literals(&mut self) {
        let mut index = 0usize;
        while index < self.tokens.len() {
            let Some(head) = self.tokens.get(index) else {
                break;
            };
            let name = head.span.slice(self.text).unwrap_or("");
            let is_call = head.kind == TokenKind::ReservedWord
                && self
                    .tokens
                    .get(index.saturating_add(1))
                    .is_some_and(|next| {
                        next.kind == TokenKind::Symbol
                            && next.span.start == head.span.end
                            && next.span.slice(self.text) == Some("(")
                    });
            let Some(constructor) = self.lexicon.literal_constructor(name).filter(|_| is_call)
            else {
                index = index.saturating_add(1);
                continue;
            };
            let profile = constructor.profile;
            let arities = constructor.literal_arities.clone();
            let Some((arguments, close)) = self.call_arguments(index.saturating_add(2)) else {
                index = index.saturating_add(1);
                continue;
            };
            let literal_form = arities.contains(&arguments.len())
                && arguments.iter().all(|arg| {
                    arg.len() == 1 && self.token_kind(arg[0]) == Some(TokenKind::String)
                });
            if literal_form {
                for (position, argument) in arguments.iter().enumerate() {
                    let Some(token) = self.tokens.get(argument[0]) else {
                        continue;
                    };
                    let value = token.value.clone().unwrap_or_default();
                    let span = token.span;
                    let verdict = match (profile, position) {
                        (LiteralProfile::Regex, 0) => literal::regex_pattern(&value)
                            .map_err(|e| format!("REGEX pattern: {e}")),
                        (LiteralProfile::Regex, _) => {
                            literal::regex_flags(&value, self.lexicon.regex_flags())
                                .map_err(|e| format!("REGEX flags: {e}"))
                        }
                        (LiteralProfile::Glob, _) => {
                            literal::glob_pattern(&value).map_err(|e| format!("GLOB pattern: {e}"))
                        }
                        (LiteralProfile::Date, _) => {
                            literal::date(&value).map_err(|e| format!("DATE: {e}"))
                        }
                        (LiteralProfile::Time, _) => {
                            literal::time(&value).map_err(|e| format!("TIME: {e}"))
                        }
                        (LiteralProfile::Datetime, _) => {
                            literal::datetime(&value).map_err(|e| format!("DATETIME: {e}"))
                        }
                        (LiteralProfile::Uri, _) => {
                            literal::uri(&value).map_err(|e| format!("URI: {e}"))
                        }
                    };
                    if let Err(detail) = verdict {
                        self.push(
                            LexicalError::LiteralInvalid,
                            span,
                            "constructor_literal",
                            detail,
                        );
                    }
                }
            }
            index = close.saturating_add(1);
        }
    }

    fn token_kind(&self, index: usize) -> Option<TokenKind> {
        self.tokens.get(index).map(|t| t.kind)
    }

    /// The argument groups of a call whose `(` precedes `from`, as indices of
    /// their non-space tokens, plus the index of the closing `)`. `None` when
    /// the call does not close on its line.
    fn call_arguments(&self, from: usize) -> Option<(Vec<Vec<usize>>, usize)> {
        let mut arguments: Vec<Vec<usize>> = vec![Vec::new()];
        let mut depth = 0usize;
        let mut index = from;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::Symbol => {
                    let lexeme = token.span.slice(self.text).unwrap_or("");
                    match lexeme {
                        "(" | "[" => depth = depth.saturating_add(1),
                        ")" if depth == 0 => {
                            if arguments.len() == 1 && arguments[0].is_empty() {
                                arguments.clear();
                            }
                            return Some((arguments, index));
                        }
                        ")" | "]" => depth = depth.saturating_sub(1),
                        "," if depth == 0 => {
                            arguments.push(Vec::new());
                            index = index.saturating_add(1);
                            continue;
                        }
                        _ => {}
                    }
                    if let Some(last) = arguments.last_mut() {
                        last.push(index);
                    }
                }
                TokenKind::Space => {}
                TokenKind::Newline
                | TokenKind::BlankLine
                | TokenKind::Indent
                | TokenKind::Dedent
                | TokenKind::Eof => return None,
                _ => {
                    if let Some(last) = arguments.last_mut() {
                        last.push(index);
                    }
                }
            }
            index = index.saturating_add(1);
        }
        None
    }
}
