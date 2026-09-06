//! Document, block, field and control-form structure.
//!
//! ```text
//! DOCUMENT         = { BLANK_LINE }, LCL_HEADER, { BLANK_LINE },
//!                    SPECIFICATION_HEADER, { BLANK_LINE },
//!                    { TOP_LEVEL_BLOCK, { BLANK_LINE } }, EOF
//! CORE_BLOCK       = BLOCK_WORD, ":", NEWLINE, INDENT, BLOCK_BODY, DEDENT
//! BLOCK_BODY       = BLOCK_STATEMENT, { BLOCK_STATEMENT }
//! BLOCK_STATEMENT  = FIELD_LINE | NESTED_FIELD | CONDITIONAL | FOR_EACH
//! FIELD_LINE       = FIELD_KEY, ":", SPACE, INLINE_VALUE, NEWLINE
//! NESTED_FIELD     = FIELD_KEY, ":", NEWLINE, INDENT, NESTED_BODY, DEDENT
//! NESTED_BODY      = (BLOCK_STATEMENT | OBJECT_PROPERTY), { … }
//! OBJECT_PROPERTY  = SIMPLE_IDENTIFIER, ":",
//!                    (SPACE, INLINE_VALUE, NEWLINE | NEWLINE, INDENT, NESTED_BODY, DEDENT)
//! ```
//!
//! A `BLANK_LINE` is a document-level separator only: no body production admits
//! one, and `04_GRAMMAR/11` rule 12 makes undefined syntax invalid rather than
//! implementation-defined, so a blank line inside a body is reported.
//!
//! Which blocks may contain which children is not decided here. This module
//! builds the tree the source actually spells; `schema` then judges it against
//! the registries, so an illegal child is reported as the registered
//! `error.block.context` rather than as a parse failure that loses the node.

use crate::diagnostic::GrammarError;
use crate::expr::ExprParser;
use crate::grammar::Grammar;
use crate::parse::{Cursor, Emitter};
use crate::syntax::*;
use lcl_lexer::{Span, Token, TokenKind};

pub(crate) struct BlockParser<'a, 'b> {
    pub(crate) grammar: &'a Grammar,
    pub(crate) source: &'a str,
    pub(crate) eof: usize,
    pub(crate) emitter: &'b mut Emitter<'a>,
}

impl<'a> BlockParser<'a, '_> {
    fn text(&self, span: Span) -> &'a str {
        span.slice(self.source).unwrap_or("")
    }

    fn word(&self, token: &Token) -> Word {
        Word {
            text: self.text(token.span).to_string(),
            span: token.span,
        }
    }

    fn emit(&mut self, id: GrammarError, span: Span, cause: &str, detail: String) {
        self.emitter.emit(id, span, cause, detail);
    }

    fn invalid(&mut self, span: Span, cause: &str, detail: String) {
        self.emit(GrammarError::GrammarInvalid, span, cause, detail);
    }

    fn exprs<'s>(&'s mut self) -> ExprParser<'a, 's> {
        ExprParser {
            grammar: self.grammar,
            source: self.source,
            emitter: self.emitter,
        }
    }

    /// `DOCUMENT`
    pub(crate) fn document(&mut self, c: &mut Cursor<'a>) -> Document {
        let mut items = Vec::new();
        loop {
            while c.eat(TokenKind::BlankLine).is_some() {}
            if c.at_end() {
                break;
            }
            let before = c.index();
            match self.top_level(c) {
                Some(item) => items.push(item),
                None => self.recover(c),
            }
            if c.index() == before {
                // Absolute progress guarantee: the parser is total, so no
                // dispatch may leave the cursor where it found it.
                self.recover(c);
                if c.index() == before {
                    c.bump();
                }
            }
        }
        Document {
            span: Span::new(0, self.eof),
            items,
        }
    }

    fn top_level(&mut self, c: &mut Cursor<'a>) -> Option<TopLevel> {
        match self.control(c)? {
            Control::Conditional(x) => Some(TopLevel::Conditional(x)),
            Control::ForEach(x) => Some(TopLevel::ForEach(x)),
            Control::None => self.block(c).map(TopLevel::Block),
        }
    }

    /// `CORE_BLOCK`
    fn block(&mut self, c: &mut Cursor<'a>) -> Option<Block> {
        let token = c.peek()?;
        if token.kind != TokenKind::ReservedWord {
            let span = token.span;
            let kind = token.kind;
            self.invalid(
                span,
                "block_word",
                format!("a block opens with a BLOCK_WORD, not {kind}"),
            );
            return None;
        }
        let key = self.word(token);
        c.bump();
        self.expect_colon(c, &key.text)?;
        if c.eat(TokenKind::Newline).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "block_header",
                format!("`{}:` opens a block and must end the line", key.text),
            );
            return None;
        }
        let (statements, end) = self.indented_body(c, &key.text)?;
        Some(Block {
            span: Span::new(key.span.start, end),
            key,
            body: statements,
        })
    }

    /// `INDENT, <body>, DEDENT`, returning the body and its end offset.
    fn indented_body(
        &mut self,
        c: &mut Cursor<'a>,
        owner: &str,
    ) -> Option<(Vec<Statement>, usize)> {
        if c.eat(TokenKind::Indent).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "block_body",
                format!("`{owner}` opens one indented level"),
            );
            return None;
        }
        let statements = self.statements(c);
        let end = statements
            .last()
            .map(|s| s.span().end)
            .unwrap_or(c.locus(self.eof).start);
        if statements.is_empty() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "empty_body",
                format!("`{owner}` requires at least one statement"),
            );
        }
        c.eat(TokenKind::Dedent);
        Some((statements, end))
    }

    /// `BLOCK_BODY` / `NESTED_BODY`: statements until the level closes.
    fn statements(&mut self, c: &mut Cursor<'a>) -> Vec<Statement> {
        let mut out = Vec::new();
        loop {
            match c.peek_kind() {
                None | Some(TokenKind::Eof) | Some(TokenKind::Dedent) => break,
                Some(TokenKind::BlankLine) => {
                    let span = c.peek().map(|t| t.span).unwrap_or(Span::empty(self.eof));
                    self.invalid(
                        span,
                        "blank_line_in_body",
                        "a blank line separates top-level blocks and is not a statement"
                            .to_string(),
                    );
                    c.bump();
                    continue;
                }
                _ => {}
            }
            let before = c.index();
            match self.statement(c) {
                Some(s) => out.push(s),
                None => self.recover(c),
            }
            if c.index() == before {
                self.recover(c);
                if c.index() == before {
                    c.bump();
                }
            }
        }
        out
    }

    fn statement(&mut self, c: &mut Cursor<'a>) -> Option<Statement> {
        match self.control(c)? {
            Control::Conditional(x) => return Some(Statement::Conditional(x)),
            Control::ForEach(x) => return Some(Statement::ForEach(x)),
            Control::None => {}
        }
        let token = c.peek()?;
        match token.kind {
            TokenKind::ReservedWord => {
                let key = self.word(token);
                c.bump();
                self.expect_colon(c, &key.text)?;
                let body = self.body_after_colon(c, &key.text)?;
                Some(Statement::Field(Field {
                    span: Span::new(key.span.start, body.span().end),
                    key,
                    body,
                }))
            }
            TokenKind::SimpleIdentifier => {
                let key = Ident {
                    text: self.text(token.span).to_string(),
                    span: token.span,
                    qualified: false,
                };
                c.bump();
                self.expect_colon(c, &key.text)?;
                let body = self.body_after_colon(c, &key.text)?;
                Some(Statement::Property(Property {
                    span: Span::new(key.span.start, body.span().end),
                    key,
                    body,
                }))
            }
            other => {
                let span = token.span;
                self.invalid(
                    span,
                    "statement",
                    format!("a statement opens with a registered key or an object property, not {other}"),
                );
                None
            }
        }
    }

    /// What follows a key's colon.
    fn body_after_colon(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<Body> {
        // "A colon followed by NEWLINE opens one indented block. A colon
        // followed by one space and value is inline." — 04_GRAMMAR/02
        if c.eat(TokenKind::Space).is_some() {
            let value = self.inline_value(c)?;
            if c.eat(TokenKind::Newline).is_none() {
                let locus = c.locus(self.eof);
                self.invalid(
                    locus,
                    "field_line",
                    format!("the value of `{owner}` must end its line"),
                );
                return None;
            }
            return Some(Body::Inline(value));
        }
        if c.eat(TokenKind::Newline).is_some() {
            let start = c.locus(self.eof).start;
            let (statements, end) = self.indented_body(c, owner)?;
            return Some(Body::Nested(Nested {
                span: Span::new(start, end),
                statements,
            }));
        }
        let locus = c.locus(self.eof);
        self.invalid(
            locus,
            "after_colon",
            format!("`{owner}:` is followed by one SPACE and a value, or by a NEWLINE"),
        );
        None
    }

    /// `INLINE_VALUE = EXPRESSION | MULTILINE_COLLECTION`
    fn inline_value(&mut self, c: &mut Cursor<'a>) -> Option<Value> {
        let multiline = c
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Symbol && self.text(t.span) == "[")
            && c.peek_at(1).map(|t| t.kind) == Some(TokenKind::Newline);
        let mut exprs = self.exprs();
        if multiline {
            return exprs.multiline_collection(c).map(Value::MultilineCollection);
        }
        exprs.expression(c).map(Value::Expression)
    }

    /// `CONDITIONAL` and `FOR_EACH`, which share their trigger position.
    fn control(&mut self, c: &mut Cursor<'a>) -> Option<Control> {
        let token = c.peek()?;
        if token.kind != TokenKind::ReservedWord {
            return Some(Control::None);
        }
        match self.text(token.span) {
            "IF" => self.conditional(c).map(Control::Conditional),
            // `FOR` opens `FOR EACH`; the lexer has already rejected every
            // unregistered loop word, so `FOR` not followed by `EACH` is a
            // grammar defect rather than another construct.
            "FOR" => self.for_each(c).map(Control::ForEach),
            _ => Some(Control::None),
        }
    }

    fn conditional(&mut self, c: &mut Cursor<'a>) -> Option<Conditional> {
        let keyword_span = c.peek()?.span;
        c.bump();
        self.expect_space(c, "IF")?;
        self.expect_symbol(c, "(", "conditional")?;
        let condition = self.exprs().expression(c)?;
        self.expect_symbol(c, ")", "conditional")?;
        self.expect_space(c, "IF")?;
        self.expect_word(c, "THEN")?;
        self.expect_colon(c, "THEN")?;
        if c.eat(TokenKind::Newline).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "conditional",
                "`THEN:` opens a branch and must end the line".to_string(),
            );
            return None;
        }
        let (then_body, mut end) = self.executable_body(c, "THEN")?;

        // "ELSE is optional and aligned with its IF." — 04_GRAMMAR/04
        let mut else_body = None;
        if c.peek().is_some_and(|t| {
            t.kind == TokenKind::ReservedWord && self.text(t.span) == "ELSE"
        }) {
            let else_span = c.peek()?.span;
            c.bump();
            self.expect_colon(c, "ELSE")?;
            if c.eat(TokenKind::Newline).is_none() {
                let locus = c.locus(self.eof);
                self.invalid(
                    locus,
                    "conditional",
                    "`ELSE:` opens a branch and must end the line".to_string(),
                );
                return None;
            }
            let (body, else_end) = self.executable_body(c, "ELSE")?;
            end = else_end;
            else_body = Some(ElseArm {
                span: Span::new(else_span.start, else_end),
                keyword_span: else_span,
                body,
            });
        }

        Some(Conditional {
            span: Span::new(keyword_span.start, end),
            keyword_span,
            condition: Box::new(condition),
            then_body,
            else_body,
        })
    }

    fn for_each(&mut self, c: &mut Cursor<'a>) -> Option<ForEach> {
        let keyword_span = c.peek()?.span;
        c.bump();
        self.expect_space(c, "FOR")?;
        self.expect_word(c, "EACH")?;
        self.expect_space(c, "EACH")?;
        let Some(binding_token) = c.peek() else {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "for_each",
                "`FOR EACH` names one SIMPLE_IDENTIFIER binding".to_string(),
            );
            return None;
        };
        if binding_token.kind != TokenKind::SimpleIdentifier {
            let span = binding_token.span;
            let kind = binding_token.kind;
            self.invalid(
                span,
                "for_each",
                format!("`FOR EACH` names one SIMPLE_IDENTIFIER binding, not {kind}"),
            );
            return None;
        }
        let binding = Ident {
            text: self.text(binding_token.span).to_string(),
            span: binding_token.span,
            qualified: false,
        };
        c.bump();
        self.expect_space(c, "the loop binding")?;
        self.expect_word(c, "IN")?;
        self.expect_space(c, "IN")?;
        let collection = self.exprs().expression(c)?;
        self.expect_colon(c, "FOR EACH")?;
        if c.eat(TokenKind::Newline).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "for_each",
                "`FOR EACH …:` opens a body and must end the line".to_string(),
            );
            return None;
        }
        let (body, end) = self.executable_body(c, "FOR EACH")?;
        Some(ForEach {
            span: Span::new(keyword_span.start, end),
            keyword_span,
            binding,
            collection: Box::new(collection),
            body,
        })
    }

    /// `EXECUTABLE_BODY = EXECUTABLE_STATEMENT, { EXECUTABLE_STATEMENT }`
    ///
    /// `EXECUTABLE_STATEMENT = STEP_BLOCK | CONDITIONAL | FOR_EACH |
    /// COMMENT_BLOCK`. Which block words are legal here is a schema question —
    /// `STEP` and `COMMENT` are exactly the blocks whose registered contexts
    /// include `IF`, `FOR_EACH` and `ELSE` — so any block parses and `schema`
    /// judges it.
    fn executable_body(
        &mut self,
        c: &mut Cursor<'a>,
        owner: &str,
    ) -> Option<(Vec<Executable>, usize)> {
        if c.eat(TokenKind::Indent).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "executable_body",
                format!("`{owner}` opens one indented level"),
            );
            return None;
        }
        let mut out: Vec<Executable> = Vec::new();
        loop {
            match c.peek_kind() {
                None | Some(TokenKind::Eof) | Some(TokenKind::Dedent) => break,
                Some(TokenKind::BlankLine) => {
                    let span = c.peek().map(|t| t.span).unwrap_or(Span::empty(self.eof));
                    self.invalid(
                        span,
                        "blank_line_in_body",
                        "a blank line separates top-level blocks and is not a statement"
                            .to_string(),
                    );
                    c.bump();
                    continue;
                }
                _ => {}
            }
            let before = c.index();
            let parsed = match self.control(c) {
                Some(Control::Conditional(x)) => Some(Executable::Conditional(x)),
                Some(Control::ForEach(x)) => Some(Executable::ForEach(x)),
                Some(Control::None) => self.block(c).map(Executable::Block),
                None => None,
            };
            match parsed {
                Some(item) => out.push(item),
                None => self.recover(c),
            }
            if c.index() == before {
                self.recover(c);
                if c.index() == before {
                    c.bump();
                }
            }
        }
        let end = out
            .last()
            .map(|s| s.span().end)
            .unwrap_or(c.locus(self.eof).start);
        if out.is_empty() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "empty_body",
                format!("`{owner}` requires at least one executable statement"),
            );
        }
        c.eat(TokenKind::Dedent);
        Some((out, end))
    }

    // -- token expectations ------------------------------------------------

    fn expect_colon(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<()> {
        if c.peek().is_some_and(|t| {
            t.kind == TokenKind::Symbol && self.text(t.span) == ":"
        }) {
            c.bump();
            return Some(());
        }
        let locus = c.locus(self.eof);
        self.invalid(locus, "colon", format!("`{owner}` must be followed by `:`"));
        None
    }

    fn expect_space(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<()> {
        if c.eat(TokenKind::Space).is_some() {
            return Some(());
        }
        let locus = c.locus(self.eof);
        self.invalid(
            locus,
            "space",
            format!("exactly one SPACE must follow `{owner}`"),
        );
        None
    }

    fn expect_word(&mut self, c: &mut Cursor<'a>, word: &str) -> Option<()> {
        if c.peek().is_some_and(|t| {
            t.kind == TokenKind::ReservedWord && self.text(t.span) == word
        }) {
            c.bump();
            return Some(());
        }
        let locus = c.locus(self.eof);
        self.invalid(locus, "keyword", format!("`{word}` is required here"));
        None
    }

    fn expect_symbol(&mut self, c: &mut Cursor<'a>, symbol: &str, cause: &str) -> Option<()> {
        if c.peek().is_some_and(|t| {
            t.kind == TokenKind::Symbol && self.text(t.span) == symbol
        }) {
            c.bump();
            return Some(());
        }
        let locus = c.locus(self.eof);
        self.invalid(locus, cause, format!("`{symbol}` is required here"));
        None
    }

    /// Resynchronise to the next statement boundary without unbalancing the
    /// indentation stack, so later independent syntax remains evidence.
    fn recover(&mut self, c: &mut Cursor<'a>) {
        let mut depth = 0usize;
        loop {
            match c.peek_kind() {
                None | Some(TokenKind::Eof) => return,
                Some(TokenKind::Indent) => {
                    depth = depth.saturating_add(1);
                    c.bump();
                }
                Some(TokenKind::Dedent) => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    c.bump();
                }
                Some(TokenKind::Newline) | Some(TokenKind::BlankLine) => {
                    c.bump();
                    if depth == 0 {
                        return;
                    }
                }
                _ => {
                    c.bump();
                }
            }
        }
    }
}

enum Control {
    Conditional(Conditional),
    ForEach(ForEach),
    None,
}
