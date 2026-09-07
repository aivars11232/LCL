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
    ///
    /// ## Why this is a machine rather than a recursion
    ///
    /// Blocks, nested fields, object data and control forms all nest without
    /// bound, and the canonical language sets no depth limit (see the note in
    /// `expr`). A recursive `block → indented_body → statements → statement →
    /// body_after_colon → indented_body` cycle therefore paid native stack per
    /// nesting level and aborted the process rather than reporting anything.
    ///
    /// The nesting is already explicit in the token stream — M1 emits `Indent`
    /// and `Dedent` — so this driver keeps one [`Frame`] per open container and
    /// closes it when its `Dedent` arrives. Depth costs heap.
    ///
    /// Every diagnostic, every recovery point and the progress guarantee are
    /// the ones the recursive version had, at the same loci.
    pub(crate) fn document(&mut self, c: &mut Cursor<'a>) -> Document {
        let mut frames: Vec<Frame> = vec![Frame::Document { items: Vec::new() }];

        loop {
            let before = (c.index(), frames.len());

            match frames.last() {
                Some(Frame::Document { .. }) => {
                    while c.eat(TokenKind::BlankLine).is_some() {}
                    if c.at_end() {
                        let items = match frames.pop() {
                            Some(Frame::Document { items }) => items,
                            _ => Vec::new(),
                        };
                        return Document {
                            span: Span::new(0, self.eof),
                            items,
                        };
                    }
                    self.step_top_level(c, &mut frames);
                }
                Some(Frame::Block { .. }) | Some(Frame::Nested { .. }) => {
                    if self.at_body_end(c) {
                        self.close(c, &mut frames);
                    } else {
                        self.step_statement(c, &mut frames);
                    }
                }
                Some(Frame::Executable { .. }) => {
                    if self.at_body_end(c) {
                        self.close(c, &mut frames);
                    } else {
                        self.step_executable(c, &mut frames);
                    }
                }
                None => {
                    return Document {
                        span: Span::new(0, self.eof),
                        items: Vec::new(),
                    }
                }
            }

            if (c.index(), frames.len()) == before {
                // Absolute progress guarantee: the parser is total, so no
                // dispatch may leave the cursor and the frame stack where it
                // found them.
                self.recover(c);
                if (c.index(), frames.len()) == before {
                    c.bump();
                }
            }
        }
    }

    /// Is the current body finished, or is a blank line to be reported first?
    ///
    /// A `BLANK_LINE` is a document-level separator only, so one inside a body
    /// is reported and skipped exactly where the recursive version reported it.
    fn at_body_end(&mut self, c: &mut Cursor<'a>) -> bool {
        match c.peek_kind() {
            None | Some(TokenKind::Eof) | Some(TokenKind::Dedent) => true,
            Some(TokenKind::BlankLine) => {
                let span = c.peek().map(|t| t.span).unwrap_or(Span::empty(self.eof));
                self.invalid(
                    span,
                    "blank_line_in_body",
                    "a blank line separates top-level blocks and is not a statement".to_string(),
                );
                c.bump();
                false
            }
            _ => false,
        }
    }

    /// One top-level item: a control form or a `CORE_BLOCK`.
    fn step_top_level(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) {
        match self.try_control(c, frames) {
            ControlStep::Opened => return,
            ControlStep::Failed => {
                self.recover(c);
                return;
            }
            ControlStep::NotControl => {}
        }
        if self.open_block(c, frames).is_none() {
            self.recover(c);
        }
    }

    /// One `EXECUTABLE_STATEMENT`.
    ///
    /// `EXECUTABLE_STATEMENT = STEP_BLOCK | CONDITIONAL | FOR_EACH |
    /// COMMENT_BLOCK`. Which block words are legal here is a schema question —
    /// `STEP` and `COMMENT` are exactly the blocks whose registered contexts
    /// include `IF`, `FOR_EACH` and `ELSE` — so any block parses and `schema`
    /// judges it.
    fn step_executable(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) {
        match self.try_control(c, frames) {
            ControlStep::Opened => return,
            ControlStep::Failed => {
                self.recover(c);
                return;
            }
            ControlStep::NotControl => {}
        }
        if self.open_block(c, frames).is_none() {
            self.recover(c);
        }
    }

    /// One `BLOCK_STATEMENT` or `OBJECT_PROPERTY`.
    fn step_statement(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) {
        match self.try_control(c, frames) {
            ControlStep::Opened => return,
            ControlStep::Failed => {
                self.recover(c);
                return;
            }
            ControlStep::NotControl => {}
        }
        let Some(token) = c.peek() else {
            self.recover(c);
            return;
        };
        let opened = match token.kind {
            TokenKind::ReservedWord => {
                let key = self.word(token);
                c.bump();
                self.open_after_key(c, frames, Key::Field(key))
            }
            TokenKind::SimpleIdentifier => {
                let key = Ident {
                    text: self.text(token.span).to_string(),
                    span: token.span,
                    qualified: false,
                };
                c.bump();
                self.open_after_key(c, frames, Key::Property(key))
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
        };
        if opened.is_none() {
            self.recover(c);
        }
    }

    /// `CORE_BLOCK = BLOCK_WORD, ":", NEWLINE, INDENT, BLOCK_BODY, DEDENT`
    ///
    /// Opens the body; the matching `DEDENT` closes it in [`Self::close`].
    fn open_block(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) -> Option<()> {
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
        self.open_indent(c, &key.text)?;
        frames.push(Frame::Block {
            key,
            statements: Vec::new(),
        });
        Some(())
    }

    /// What follows a key's colon.
    ///
    /// "A colon followed by NEWLINE opens one indented block. A colon followed
    /// by one space and value is inline." — `04_GRAMMAR/02`
    fn open_after_key(
        &mut self,
        c: &mut Cursor<'a>,
        frames: &mut Vec<Frame>,
        key: Key,
    ) -> Option<()> {
        let owner = key.text().to_string();
        self.expect_colon(c, &owner)?;

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
            let statement = key.statement(Body::Inline(value));
            self.attach(frames, Item::Statement(statement));
            return Some(());
        }

        if c.eat(TokenKind::Newline).is_some() {
            let start = c.locus(self.eof).start;
            self.open_indent(c, &owner)?;
            frames.push(Frame::Nested {
                key,
                start,
                statements: Vec::new(),
            });
            return Some(());
        }

        let locus = c.locus(self.eof);
        self.invalid(
            locus,
            "after_colon",
            format!("`{owner}:` is followed by one SPACE and a value, or by a NEWLINE"),
        );
        None
    }

    /// The `INDENT` that opens one body.
    fn open_indent(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<()> {
        if c.eat(TokenKind::Indent).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "block_body",
                format!("`{owner}` opens one indented level"),
            );
            return None;
        }
        Some(())
    }

    /// `CONDITIONAL` and `FOR_EACH`, which share their trigger position.
    fn try_control(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) -> ControlStep {
        let Some(token) = c.peek() else {
            return ControlStep::NotControl;
        };
        if token.kind != TokenKind::ReservedWord {
            return ControlStep::NotControl;
        }
        let opened = match self.text(token.span) {
            "IF" => self.open_conditional(c, frames),
            // `FOR` opens `FOR EACH`; the lexer has already rejected every
            // unregistered loop word, so `FOR` not followed by `EACH` is a
            // grammar defect rather than another construct.
            "FOR" => self.open_for_each(c, frames),
            _ => return ControlStep::NotControl,
        };
        match opened {
            Some(()) => ControlStep::Opened,
            None => ControlStep::Failed,
        }
    }

    /// `IF (…) THEN:` — opens the `THEN` branch.
    fn open_conditional(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) -> Option<()> {
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
        self.open_executable_indent(c, "THEN")?;
        frames.push(Frame::Executable {
            owner: "THEN".to_string(),
            items: Vec::new(),
            cont: ControlCont::Then {
                keyword_span,
                condition: Box::new(condition),
            },
        });
        Some(())
    }

    /// `FOR EACH <binding> IN <collection>:` — opens the loop body.
    fn open_for_each(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) -> Option<()> {
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
        self.open_executable_indent(c, "FOR EACH")?;
        frames.push(Frame::Executable {
            owner: "FOR EACH".to_string(),
            items: Vec::new(),
            cont: ControlCont::ForBody {
                keyword_span,
                binding,
                collection: Box::new(collection),
            },
        });
        Some(())
    }

    /// The `INDENT` that opens one `EXECUTABLE_BODY`.
    fn open_executable_indent(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<()> {
        if c.eat(TokenKind::Indent).is_none() {
            let locus = c.locus(self.eof);
            self.invalid(
                locus,
                "executable_body",
                format!("`{owner}` opens one indented level"),
            );
            return None;
        }
        Some(())
    }

    /// Close the innermost open container and attach what it built.
    fn close(&mut self, c: &mut Cursor<'a>, frames: &mut Vec<Frame>) {
        let Some(frame) = frames.pop() else { return };
        match frame {
            // Handled by the driver, which returns the finished document.
            Frame::Document { items } => frames.push(Frame::Document { items }),

            Frame::Block { key, statements } => {
                let end = self.body_end(c, statements.last().map(|s| s.span().end));
                if statements.is_empty() {
                    let locus = c.locus(self.eof);
                    self.invalid(
                        locus,
                        "empty_body",
                        format!("`{}` requires at least one statement", key.text),
                    );
                }
                c.eat(TokenKind::Dedent);
                let block = Block {
                    span: Span::new(key.span.start, end),
                    key,
                    body: statements,
                };
                self.attach(frames, Item::Block(block));
            }

            Frame::Nested {
                key,
                start,
                statements,
            } => {
                let end = self.body_end(c, statements.last().map(|s| s.span().end));
                if statements.is_empty() {
                    let locus = c.locus(self.eof);
                    self.invalid(
                        locus,
                        "empty_body",
                        format!("`{}` requires at least one statement", key.text()),
                    );
                }
                c.eat(TokenKind::Dedent);
                let body = Body::Nested(Nested {
                    span: Span::new(start, end),
                    statements,
                });
                self.attach(frames, Item::Statement(key.statement(body)));
            }

            Frame::Executable { owner, items, cont } => {
                let end = self.body_end(c, items.last().map(|s| s.span().end));
                if items.is_empty() {
                    let locus = c.locus(self.eof);
                    self.invalid(
                        locus,
                        "empty_body",
                        format!("`{owner}` requires at least one executable statement"),
                    );
                }
                c.eat(TokenKind::Dedent);
                self.close_control(c, frames, cont, items, end);
            }
        }
    }

    /// The end offset of a finished body.
    fn body_end(&self, c: &Cursor<'a>, last: Option<usize>) -> usize {
        last.unwrap_or_else(|| c.locus(self.eof).start)
    }

    /// Finish a control form once its body has closed.
    fn close_control(
        &mut self,
        c: &mut Cursor<'a>,
        frames: &mut Vec<Frame>,
        cont: ControlCont,
        items: Vec<Executable>,
        end: usize,
    ) {
        match cont {
            ControlCont::Then {
                keyword_span,
                condition,
            } => {
                // "ELSE is optional and aligned with its IF." — 04_GRAMMAR/04
                let has_else = c.peek().is_some_and(|t| {
                    t.kind == TokenKind::ReservedWord && self.text(t.span) == "ELSE"
                });
                if has_else {
                    let Some(else_token) = c.peek() else { return };
                    let else_span = else_token.span;
                    c.bump();
                    if self.expect_colon(c, "ELSE").is_none() {
                        self.recover(c);
                        return;
                    }
                    if c.eat(TokenKind::Newline).is_none() {
                        let locus = c.locus(self.eof);
                        self.invalid(
                            locus,
                            "conditional",
                            "`ELSE:` opens a branch and must end the line".to_string(),
                        );
                        self.recover(c);
                        return;
                    }
                    if self.open_executable_indent(c, "ELSE").is_none() {
                        self.recover(c);
                        return;
                    }
                    frames.push(Frame::Executable {
                        owner: "ELSE".to_string(),
                        items: Vec::new(),
                        cont: ControlCont::Else {
                            keyword_span,
                            condition,
                            then_body: items,
                            else_span,
                        },
                    });
                    return;
                }
                let node = Conditional {
                    span: Span::new(keyword_span.start, end),
                    keyword_span,
                    condition,
                    then_body: items,
                    else_body: None,
                };
                self.attach(frames, Item::Conditional(node));
            }

            ControlCont::Else {
                keyword_span,
                condition,
                then_body,
                else_span,
            } => {
                let node = Conditional {
                    span: Span::new(keyword_span.start, end),
                    keyword_span,
                    condition,
                    then_body,
                    else_body: Some(ElseArm {
                        span: Span::new(else_span.start, end),
                        keyword_span: else_span,
                        body: items,
                    }),
                };
                self.attach(frames, Item::Conditional(node));
            }

            ControlCont::ForBody {
                keyword_span,
                binding,
                collection,
            } => {
                let node = ForEach {
                    span: Span::new(keyword_span.start, end),
                    keyword_span,
                    binding,
                    collection,
                    body: items,
                };
                self.attach(frames, Item::ForEach(node));
            }
        }
    }

    /// Attach a finished child to whichever container encloses it.
    fn attach(&mut self, frames: &mut [Frame], item: Item) {
        let Some(parent) = frames.last_mut() else {
            return;
        };
        match parent {
            Frame::Document { items } => match item {
                Item::Block(x) => items.push(TopLevel::Block(x)),
                Item::Conditional(x) => items.push(TopLevel::Conditional(x)),
                Item::ForEach(x) => items.push(TopLevel::ForEach(x)),
                // A bare statement has no top-level production; the grammar
                // stage has already reported whatever produced it.
                Item::Statement(_) => {}
            },
            Frame::Block { statements, .. } | Frame::Nested { statements, .. } => match item {
                Item::Statement(x) => statements.push(x),
                Item::Conditional(x) => statements.push(Statement::Conditional(x)),
                Item::ForEach(x) => statements.push(Statement::ForEach(x)),
                // Inside a body every child is a statement, including a block
                // word, which the grammar spells as a NESTED_FIELD.
                Item::Block(_) => {}
            },
            Frame::Executable { items, .. } => match item {
                Item::Block(x) => items.push(Executable::Block(x)),
                Item::Conditional(x) => items.push(Executable::Conditional(x)),
                Item::ForEach(x) => items.push(Executable::ForEach(x)),
                Item::Statement(_) => {}
            },
        }
    }

    /// `INLINE_VALUE = EXPRESSION | MULTILINE_COLLECTION`
    fn inline_value(&mut self, c: &mut Cursor<'a>) -> Option<Value> {
        let multiline = c
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Symbol && self.text(t.span) == "[")
            && c.peek_at(1).map(|t| t.kind) == Some(TokenKind::Newline);
        let mut exprs = self.exprs();
        if multiline {
            return exprs
                .multiline_collection(c)
                .map(Value::MultilineCollection);
        }
        exprs.expression(c).map(Value::Expression)
    }

    // -- token expectations ------------------------------------------------

    fn expect_colon(&mut self, c: &mut Cursor<'a>, owner: &str) -> Option<()> {
        if c.peek()
            .is_some_and(|t| t.kind == TokenKind::Symbol && self.text(t.span) == ":")
        {
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
        if c.peek()
            .is_some_and(|t| t.kind == TokenKind::ReservedWord && self.text(t.span) == word)
        {
            c.bump();
            return Some(());
        }
        let locus = c.locus(self.eof);
        self.invalid(locus, "keyword", format!("`{word}` is required here"));
        None
    }

    fn expect_symbol(&mut self, c: &mut Cursor<'a>, symbol: &str, cause: &str) -> Option<()> {
        if c.peek()
            .is_some_and(|t| t.kind == TokenKind::Symbol && self.text(t.span) == symbol)
        {
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

/// One open container.
///
/// Each variant holds exactly the state the recursive version kept in a native
/// frame, so nesting depth costs heap rather than stack.
enum Frame {
    /// The whole document.
    Document { items: Vec<TopLevel> },
    /// A `CORE_BLOCK` body.
    Block {
        key: Word,
        statements: Vec<Statement>,
    },
    /// A `NESTED_BODY` under one field or object property.
    Nested {
        key: Key,
        start: usize,
        statements: Vec<Statement>,
    },
    /// An `EXECUTABLE_BODY` under a control form.
    Executable {
        owner: String,
        items: Vec<Executable>,
        cont: ControlCont,
    },
}

/// The key that opened a nested body.
enum Key {
    Field(Word),
    Property(Ident),
}

impl Key {
    fn text(&self) -> &str {
        match self {
            Key::Field(word) => &word.text,
            Key::Property(ident) => &ident.text,
        }
    }

    /// Build the statement this key declares, given its finished body.
    fn statement(self, body: Body) -> Statement {
        match self {
            Key::Field(key) => Statement::Field(Field {
                span: Span::new(key.span.start, body.span().end),
                key,
                body,
            }),
            Key::Property(key) => Statement::Property(Property {
                span: Span::new(key.span.start, body.span().end),
                key,
                body,
            }),
        }
    }
}

/// What to build once a control form's body closes.
enum ControlCont {
    /// The `THEN` branch; an `ELSE` may still follow.
    Then {
        keyword_span: Span,
        condition: Box<Expr>,
    },
    /// The `ELSE` branch, holding the `THEN` body already parsed.
    Else {
        keyword_span: Span,
        condition: Box<Expr>,
        then_body: Vec<Executable>,
        else_span: Span,
    },
    /// A `FOR EACH` body.
    ForBody {
        keyword_span: Span,
        binding: Ident,
        collection: Box<Expr>,
    },
}

/// A finished child, awaiting attachment to its container.
enum Item {
    Block(Block),
    Conditional(Conditional),
    ForEach(ForEach),
    Statement(Statement),
}

/// Whether a control form opened at the cursor.
enum ControlStep {
    /// A control form opened and pushed its body.
    Opened,
    /// A control form began and failed; the caller recovers.
    Failed,
    /// The cursor is not on a control form.
    NotControl,
}
