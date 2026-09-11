//! Expressions, in the exact precedence `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`
//! declares.
//!
//! ```text
//! EXPRESSION     = OR_EXPRESSION
//! OR_EXPRESSION  = AND_EXPRESSION, { SPACE, "OR", SPACE, AND_EXPRESSION }
//! AND_EXPRESSION = COMPARISON, { SPACE, "AND", SPACE, COMPARISON }
//! COMPARISON     = ADDITIVE, [ SPACE, COMPARE_OPERATOR, SPACE, ADDITIVE ]
//! ADDITIVE       = MULTIPLICATIVE, { SPACE, ADD_OPERATOR, SPACE, MULTIPLICATIVE }
//! MULTIPLICATIVE = UNARY, { SPACE, MULTIPLY_OPERATOR, SPACE, UNARY }
//! UNARY          = (("NOT", SPACE) | "-"), UNARY | POSTFIX
//! POSTFIX        = NON_NULL_TYPE_EXPRESSION | VALUE_PRIMARY, { PROPERTY_ACCESS | INDEX_ACCESS }
//! ```
//!
//! Two shape rules the grammar states and this module enforces:
//!
//! * every binary operator is surrounded by exactly one `SPACE`, so `1 + 2` is
//!   an expression and `1+2` is not;
//! * `COMPARISON` is non-associative — it admits at most one comparison
//!   operator, so `a == b == c` is a grammar error rather than a nested tree.
//!
//! Nothing here resolves an identifier, types an operand, or evaluates
//! anything. `04_GRAMMAR/03`: "Every expression is side-effect free."
//!
//! ## Why this parser is iterative
//!
//! Every construct below nests without bound: a group inside a group, a
//! collection inside a call argument inside an index. A recursive-descent
//! implementation pays native stack per nesting level and dies by
//! `SIGABRT` — not by a diagnostic — when the level count exceeds the
//! thread's stack. Canonical LCL Core 0.1.0 declares **no** maximum nesting
//! depth for source syntax (`04_GRAMMAR/02` and the EBNF state the shape and
//! no bound; the only `maximum_depth` in the registries belongs to
//! `contract_type_notation`, which says in terms "This registry notation does
//! not extend LCL source syntax"), and no registered diagnostic permits an
//! implementation-defined nesting rejection. So the depth a document may reach
//! is not this implementation's to cap.
//!
//! This module therefore carries its own explicit stack. [`Frame`] holds
//! exactly what a native frame used to hold — the operand and operator stacks
//! of one expression level, its pending unary prefixes, and what to do when
//! the level finishes — and [`ExprParser::expression`] drives them in one
//! loop. Nesting costs heap, which fails as an allocation rather than as an
//! unrecoverable abort.
//!
//! The grammar is unchanged. Operator precedence is the same cascade the EBNF
//! spells, expressed as [`precedence`]; the operator-recognising helpers,
//! [`ExprParser::binary`] and every diagnostic are the same code they were, so
//! trees, spans and diagnostics are identical to the recursive version.

use crate::diagnostic::GrammarError;
use crate::grammar::Grammar;
use crate::parse::{Cursor, Emitter};
use crate::syntax::*;
use lcl_lexer::{Span, Token, TokenKind};

/// `COMPARE_OPERATOR` — the symbol-shaped alternatives.
const COMPARE_SYMBOLS: [(&str, BinaryOp); 6] = [
    ("==", BinaryOp::Equal),
    ("!=", BinaryOp::NotEqual),
    ("<=", BinaryOp::LessOrEqual),
    (">=", BinaryOp::GreaterOrEqual),
    ("<", BinaryOp::Less),
    (">", BinaryOp::Greater),
];

/// `COMPARE_OPERATOR` — the word-shaped alternatives.
const COMPARE_WORDS: [(&str, BinaryOp); 3] = [
    ("IN", BinaryOp::In),
    ("CONTAINS", BinaryOp::Contains),
    ("MATCHES", BinaryOp::Matches),
];

const ADD_SYMBOLS: [(&str, BinaryOp); 2] = [("+", BinaryOp::Add), ("-", BinaryOp::Subtract)];
const MULTIPLY_SYMBOLS: [(&str, BinaryOp); 2] =
    [("*", BinaryOp::Multiply), ("/", BinaryOp::Divide)];

/// Binary operator precedence, exactly the EBNF cascade.
///
/// `OR_EXPRESSION` is loosest and `MULTIPLICATIVE` is tightest, so a larger
/// number binds tighter. Every operator is left-associative except the
/// comparison band, which the grammar makes non-associative and which
/// [`Frame::compare`] enforces.
fn precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::LessOrEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterOrEqual
        | BinaryOp::In
        | BinaryOp::Contains
        | BinaryOp::Matches => COMPARE_PRECEDENCE,
        BinaryOp::Add | BinaryOp::Subtract => 4,
        BinaryOp::Multiply | BinaryOp::Divide => 5,
    }
}

/// `COMPARISON = ADDITIVE, [ SPACE, COMPARE_OPERATOR, SPACE, ADDITIVE ]` — the
/// one band the grammar declares non-associative.
const COMPARE_PRECEDENCE: u8 = 3;

/// What to do with one expression level's finished value.
///
/// Each variant is a construct that was open when its inner expression began,
/// holding precisely the state the recursive version kept in a native frame.
enum Continuation {
    /// The outermost level: its value is the parsed expression.
    Root,
    /// `"(", EXPRESSION, ")"`
    Group { open: Span },
    /// `CALL = CALLABLE, "(", [ ARGUMENT, { ",", SPACE, ARGUMENT } ], ")"`
    Call {
        callable: Word,
        arguments: Vec<Expr>,
    },
    /// `COLLECTION_LITERAL = "[", [ EXPRESSION, { ",", SPACE, EXPRESSION } ], "]"`
    Collection { open: Span, members: Vec<Expr> },
    /// `INDEX_ACCESS = "[", EXPRESSION, "]"`, over an already-parsed base.
    Index { base: Expr },
    /// A bracketed type's argument, e.g. `LIST[...]`.
    TypeArgument { word: Word, name: String },
}

/// One expression level in progress.
struct Frame {
    /// Reduced operands awaiting their operators.
    operands: Vec<Expr>,
    /// Pending operators, innermost last, with their precedence.
    operators: Vec<(BinaryOp, Span, u8)>,
    /// Unary prefixes collected for the operand currently being built.
    ///
    /// `UNARY = (("NOT", SPACE) | "-"), UNARY | POSTFIX`, so prefixes apply
    /// after the operand's postfix accessors and in reverse order.
    prefixes: Vec<(UnaryOp, Span)>,
    /// The locus of a comparison operator already consumed in the current
    /// comparison group, if any.
    ///
    /// `COMPARISON` admits at most one operator, so a second one in the same
    /// group is a grammar error rather than a nested tree. An `AND` or `OR`
    /// begins a new group and clears this.
    compare: Option<Span>,
    cont: Continuation,
}

impl Frame {
    fn new(cont: Continuation) -> Frame {
        Frame {
            operands: Vec::new(),
            operators: Vec::new(),
            prefixes: Vec::new(),
            compare: None,
            cont,
        }
    }
}

/// The driver's position within one level.
enum State {
    /// An operand is required next.
    NeedOperand,
    /// An operand is in hand. The flag records a type expression, which
    /// "takes no trailing accessors".
    HaveOperand(Expr, bool),
}

/// What applying postfix accessors produced.
enum Accessed {
    /// The operand, with every accessor applied.
    Complete(Expr),
    /// An `INDEX_ACCESS` opened; its base awaits the index expression.
    Index(Expr),
}

/// What closing one level produced.
enum Closed {
    /// The whole expression is parsed.
    Done(Expr),
    /// A completed operand for the enclosing level. The flag suppresses
    /// postfix accessors.
    Operand(Expr, bool),
    /// The same construct continues with another member or argument.
    Continue(Continuation),
}

/// What opening an operand produced.
enum Opened {
    /// A complete operand. The flag suppresses postfix accessors.
    Node(Expr, bool),
    /// A nested construct was opened; parse its inner expression first.
    Push(Continuation),
}

pub(crate) struct ExprParser<'a, 'b> {
    pub(crate) grammar: &'a Grammar,
    pub(crate) source: &'a str,
    pub(crate) emitter: &'b mut Emitter<'a>,
}

impl<'a> ExprParser<'a, '_> {
    fn text(&self, span: Span) -> &'a str {
        span.slice(self.source).unwrap_or("")
    }

    fn word(&self, token: &Token) -> Word {
        Word {
            text: self.text(token.span).to_string(),
            span: token.span,
        }
    }

    fn fail(&mut self, span: Span, cause: &str, detail: String) -> Option<Expr> {
        self.emitter
            .emit(GrammarError::GrammarInvalid, span, cause, detail);
        None
    }

    /// `EXPRESSION = OR_EXPRESSION`, parsed with an explicit stack.
    ///
    /// Total in native stack: the loop's depth is constant, and one heap
    /// [`Frame`] is pushed per open construct. A document may nest as deeply as
    /// memory allows, because the language sets no limit and this parser may
    /// not invent one.
    pub(crate) fn expression(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let mut stack: Vec<Frame> = vec![Frame::new(Continuation::Root)];
        let mut state = State::NeedOperand;

        loop {
            match state {
                State::NeedOperand => {
                    self.collect_prefixes(c, stack.last_mut()?)?;
                    match self.open_operand(c)? {
                        Opened::Node(node, skip) => state = State::HaveOperand(node, skip),
                        Opened::Push(cont) => {
                            stack.push(Frame::new(cont));
                            state = State::NeedOperand;
                        }
                    }
                }

                State::HaveOperand(node, skip) => {
                    // `POSTFIX = NON_NULL_TYPE_EXPRESSION | VALUE_PRIMARY,
                    //  { PROPERTY_ACCESS | INDEX_ACCESS }`
                    let node = match self.accessors(c, node, skip)? {
                        Accessed::Complete(node) => node,
                        Accessed::Index(base) => {
                            stack.push(Frame::new(Continuation::Index { base }));
                            state = State::NeedOperand;
                            continue;
                        }
                    };

                    // A second comparison operator is a grammar error, and it
                    // is detected before consuming anything.
                    if let Some((_, second)) = self.peek_spaced_compare_operator(c) {
                        if stack.last()?.compare.is_some() {
                            return self.fail(
                                second,
                                "comparison_chain",
                                format!(
                                    "`{}` chains a second comparison operator; COMPARISON admits at most one",
                                    self.text(second)
                                ),
                            );
                        }
                    }

                    let operator = self.next_binary_operator(c);

                    let frame = stack.last_mut()?;
                    let node = Self::apply_prefixes(&mut frame.prefixes, node);
                    frame.operands.push(node);

                    match operator {
                        Some((op, span)) => {
                            let prec = precedence(op);
                            reduce(frame, prec);
                            if prec == COMPARE_PRECEDENCE {
                                frame.compare = Some(span);
                            } else if prec < COMPARE_PRECEDENCE {
                                // `AND` and `OR` begin a new comparison group.
                                frame.compare = None;
                            }
                            frame.operators.push((op, span, prec));
                            state = State::NeedOperand;
                        }
                        None => {
                            let mut frame = stack.pop()?;
                            reduce(&mut frame, 0);
                            let value = frame.operands.pop()?;
                            match self.close(c, frame.cont, value)? {
                                Closed::Done(expr) => return Some(expr),
                                Closed::Operand(node, skip) => {
                                    state = State::HaveOperand(node, skip)
                                }
                                Closed::Continue(cont) => {
                                    stack.push(Frame::new(cont));
                                    state = State::NeedOperand;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Consume every `NOT` or `-` prefix at the cursor.
    ///
    /// The recursive version spelled this as `UNARY` calling itself; a chain of
    /// prefixes is a list, so it is collected as one.
    fn collect_prefixes(&mut self, c: &mut Cursor<'a>, frame: &mut Frame) -> Option<()> {
        loop {
            let Some(t) = c.peek() else { return Some(()) };
            if t.kind == TokenKind::ReservedWord && self.text(t.span) == "NOT" {
                // `NOT` requires the following SPACE; the grammar spells it.
                if c.peek_at(1).map(|n| n.kind) != Some(TokenKind::Space) {
                    let span = t.span;
                    self.fail(
                        span,
                        "not_without_space",
                        "`NOT` must be followed by one SPACE".to_string(),
                    )?;
                    return None;
                }
                let span = t.span;
                c.bump();
                c.bump();
                frame.prefixes.push((UnaryOp::Not, span));
                continue;
            }
            if t.kind == TokenKind::Symbol && self.text(t.span) == "-" {
                let span = t.span;
                c.bump();
                frame.prefixes.push((UnaryOp::Negate, span));
                continue;
            }
            return Some(());
        }
    }

    /// Apply collected prefixes to an operand, innermost last.
    fn apply_prefixes(prefixes: &mut Vec<(UnaryOp, Span)>, mut node: Expr) -> Expr {
        while let Some((operator, operator_span)) = prefixes.pop() {
            node = Expr::Unary(Unary {
                operator,
                operator_span,
                span: Span::new(operator_span.start, node.span().end),
                operand: Box::new(node),
            });
        }
        node
    }

    /// The next binary operator, or `None` when the expression ends here.
    ///
    /// Tried in cascade order over the same helpers the recursive version used,
    /// each of which consumes only on a complete `SPACE, op, SPACE` triple.
    fn next_binary_operator(&mut self, c: &mut Cursor<'a>) -> Option<(BinaryOp, Span)> {
        if let Some(found) = self.spaced_word_operator(c, &[("OR", BinaryOp::Or)]) {
            return Some(found);
        }
        if let Some(found) = self.spaced_word_operator(c, &[("AND", BinaryOp::And)]) {
            return Some(found);
        }
        if let Some(found) = self.spaced_compare_operator(c) {
            return Some(found);
        }
        if let Some(found) = self.spaced_symbol_operator(c, &ADD_SYMBOLS) {
            return Some(found);
        }
        self.spaced_symbol_operator(c, &MULTIPLY_SYMBOLS)
    }

    /// `NON_NULL_TYPE_EXPRESSION | VALUE_PRIMARY`, opening a nested construct
    /// rather than descending into one.
    fn open_operand(&mut self, c: &mut Cursor<'a>) -> Option<Opened> {
        if let Some(opened) = self.open_type_expression(c) {
            return Some(opened);
        }

        let Some(token) = c.peek() else {
            self.fail(
                Span::empty(self.source.len()),
                "missing_value",
                "a value is required here".to_string(),
            )?;
            return None;
        };
        let span = token.span;
        match token.kind {
            TokenKind::String | TokenKind::MultilineString => {
                let kind = if token.kind == TokenKind::String {
                    LiteralKind::String
                } else {
                    LiteralKind::MultilineString
                };
                let text = token.value.clone().unwrap_or_default();
                c.bump();
                Some(Opened::Node(
                    Expr::Literal(Literal { kind, span, text }),
                    false,
                ))
            }
            TokenKind::IntegerLiteral | TokenKind::DecimalLiteral => {
                let kind = if token.kind == TokenKind::IntegerLiteral {
                    LiteralKind::Integer
                } else {
                    LiteralKind::Decimal
                };
                let text = self.text(span).to_string();
                c.bump();
                Some(Opened::Node(
                    Expr::Literal(Literal { kind, span, text }),
                    false,
                ))
            }
            TokenKind::SimpleIdentifier | TokenKind::QualifiedIdentifier => {
                let text = self.text(span).to_string();
                let qualified = token.kind == TokenKind::QualifiedIdentifier;
                c.bump();
                Some(Opened::Node(
                    Expr::Identifier(Ident {
                        text,
                        span,
                        qualified,
                    }),
                    false,
                ))
            }
            TokenKind::ReservedWord => {
                let text = self.text(span).to_string();
                if self.grammar.is_literal_word(&text) {
                    let kind = match text.as_str() {
                        "TRUE" => LiteralKind::True,
                        "FALSE" => LiteralKind::False,
                        "NULL" => LiteralKind::Null,
                        "MISSING" => LiteralKind::Missing,
                        _ => LiteralKind::Unknown,
                    };
                    c.bump();
                    return Some(Opened::Node(
                        Expr::Literal(Literal { kind, span, text }),
                        false,
                    ));
                }
                if self.grammar.is_callable(&text) {
                    return self.open_call(c);
                }
                self.fail(
                    span,
                    "unexpected_word",
                    format!("`{text}` is not a literal, a callable or a type in a value position"),
                )?;
                None
            }
            TokenKind::Symbol => match self.text(span) {
                "(" => {
                    c.bump();
                    Some(Opened::Push(Continuation::Group { open: span }))
                }
                "[" => {
                    let open = span;
                    c.bump();
                    if self.at_symbol(c, "]") {
                        let close = self.expect_symbol(c, "]", "collection")?;
                        return Some(Opened::Node(
                            Expr::Collection(Collection {
                                span: Span::new(open.start, close.end),
                                members: Vec::new(),
                            }),
                            false,
                        ));
                    }
                    Some(Opened::Push(Continuation::Collection {
                        open,
                        members: Vec::new(),
                    }))
                }
                other => {
                    let detail = format!("`{other}` does not begin a value");
                    self.fail(span, "unexpected_symbol", detail)?;
                    None
                }
            },
            other => {
                self.fail(
                    span,
                    "unexpected_token",
                    format!("{other} does not begin a value"),
                )?;
                None
            }
        }
    }

    /// `NON_NULL_TYPE_EXPRESSION`, or `None` when the cursor is not on one.
    ///
    /// A word that is a callable followed by `(` is a `CALL`, not a type: the
    /// constructor spelling wins over the scalar-type spelling for `PATH`,
    /// `REGEX`, `DATE` and the rest.
    fn open_type_expression(&mut self, c: &mut Cursor<'a>) -> Option<Opened> {
        let token = c.peek()?;
        if token.kind != TokenKind::ReservedWord {
            return None;
        }
        let text = self.text(token.span);
        let opens_call = self.grammar.is_callable(text)
            && c.peek_at(1).map(|n| n.kind) == Some(TokenKind::Symbol)
            && c.peek_at(1).map(|n| self.text(n.span)) == Some("(");
        if opens_call {
            return None;
        }
        let opens_bracket = c.peek_at(1).map(|n| n.kind) == Some(TokenKind::Symbol)
            && c.peek_at(1).map(|n| self.text(n.span)) == Some("[");
        if self.grammar.is_bracket_type(text) && opens_bracket {
            let word = self.word(token);
            let name = word.text.clone();
            c.bump();
            c.bump();
            // LIST and SET take a TYPE_EXPRESSION; OBJECT and REFERENCE take a
            // REFERENCE_CALL. Both are parsed as an expression and the
            // alternative is checked, so the tree stays source-faithful.
            return Some(Opened::Push(Continuation::TypeArgument { word, name }));
        }
        if self.grammar.is_scalar_type(text) && !self.grammar.is_literal_word(text) {
            let word = self.word(token);
            c.bump();
            return Some(Opened::Node(Expr::Type(TypeExpr::Scalar(word)), true));
        }
        None
    }

    /// `CALL = CALLABLE, "(", [ ARGUMENT, { ",", SPACE, ARGUMENT } ], ")"`
    ///
    /// Arguments are positional only. `04_GRAMMAR/03`: "Named, mixed positional
    /// and named, and variadic calls are invalid syntax."
    fn open_call(&mut self, c: &mut Cursor<'a>) -> Option<Opened> {
        let callable = self.word(c.peek()?);
        c.bump();
        let Some(open) = c.peek() else {
            self.fail(
                callable.span,
                "call",
                format!("`{}` must be followed by `(`", callable.text),
            )?;
            return None;
        };
        if open.kind != TokenKind::Symbol || self.text(open.span) != "(" {
            let span = open.span;
            self.fail(
                span,
                "call",
                format!("`{}` must be followed by `(`", callable.text),
            )?;
            return None;
        }
        c.bump();
        if self.at_symbol(c, ")") {
            let close = self.expect_symbol(c, ")", "call")?;
            return Some(Opened::Node(
                Expr::Call(Call {
                    span: Span::new(callable.span.start, close.end),
                    callable,
                    arguments: Vec::new(),
                }),
                false,
            ));
        }
        Some(Opened::Push(Continuation::Call {
            callable,
            arguments: Vec::new(),
        }))
    }

    /// `{ PROPERTY_ACCESS | INDEX_ACCESS }` over one operand.
    ///
    /// Property access is a loop. An index access opens a nested expression, so
    /// it hands its base back to the driver instead of descending.
    fn accessors(&mut self, c: &mut Cursor<'a>, mut node: Expr, skip: bool) -> Option<Accessed> {
        if skip {
            return Some(Accessed::Complete(node));
        }
        loop {
            match c.peek() {
                Some(t) if t.kind == TokenKind::Symbol && self.text(t.span) == "." => {
                    let dot = t.span;
                    c.bump();
                    let Some(name) = c.peek() else {
                        self.fail(
                            dot,
                            "property_access",
                            "`.` must be followed by a property name".to_string(),
                        )?;
                        return None;
                    };
                    let reserved = match name.kind {
                        TokenKind::SimpleIdentifier => false,
                        TokenKind::ReservedWord => true,
                        _ => {
                            let span = name.span;
                            let kind = name.kind;
                            self.fail(
                                span,
                                "property_access",
                                format!(
                                    "PROPERTY_ACCESS names a SIMPLE_IDENTIFIER or RESERVED_WORD, not {kind}"
                                ),
                            )?;
                            return None;
                        }
                    };
                    let name_span = name.span;
                    let text = self.text(name_span).to_string();
                    c.bump();
                    node = Expr::Property(PropertyAccess {
                        span: Span::new(node.span().start, name_span.end),
                        base: Box::new(node),
                        name: text,
                        name_span,
                        reserved,
                    });
                }
                Some(t) if t.kind == TokenKind::Symbol && self.text(t.span) == "[" => {
                    c.bump();
                    return Some(Accessed::Index(node));
                }
                _ => return Some(Accessed::Complete(node)),
            }
        }
    }

    /// Finish one level and hand its value to the construct that opened it.
    fn close(&mut self, c: &mut Cursor<'a>, cont: Continuation, value: Expr) -> Option<Closed> {
        match cont {
            Continuation::Root => Some(Closed::Done(value)),
            Continuation::Group { open } => {
                let close = self.expect_symbol(c, ")", "group")?;
                Some(Closed::Operand(
                    Expr::Group(Group {
                        span: Span::new(open.start, close.end),
                        inner: Box::new(value),
                    }),
                    false,
                ))
            }
            Continuation::Index { base } => {
                let close = self.expect_symbol(c, "]", "index_access")?;
                Some(Closed::Operand(
                    Expr::Index(IndexAccess {
                        span: Span::new(base.span().start, close.end),
                        base: Box::new(base),
                        index: Box::new(value),
                    }),
                    false,
                ))
            }
            Continuation::TypeArgument { word, name } => {
                let close = self.expect_symbol(c, "]", "type_argument")?;
                let span = Span::new(word.span.start, close.end);
                let bracket = BracketType {
                    word,
                    span,
                    argument: Box::new(value),
                };
                let type_expr = match name.as_str() {
                    "LIST" => TypeExpr::List(bracket),
                    "SET" => TypeExpr::Set(bracket),
                    "OBJECT" => TypeExpr::Object(bracket),
                    _ => TypeExpr::Reference(bracket),
                };
                // Concatenation binds tighter than alternation in ISO 14977, so
                // a type expression takes no trailing accessors.
                Some(Closed::Operand(Expr::Type(type_expr), true))
            }
            Continuation::Call {
                callable,
                mut arguments,
            } => {
                arguments.push(value);
                if self.at_symbol(c, ",") {
                    let comma = c.peek()?.span;
                    c.bump();
                    // The grammar spells `",", SPACE` between arguments.
                    if c.eat(TokenKind::Space).is_none() {
                        self.fail(
                            comma,
                            "argument_separator",
                            "one SPACE must follow an argument comma".to_string(),
                        )?;
                        return None;
                    }
                    return Some(Closed::Continue(Continuation::Call {
                        callable,
                        arguments,
                    }));
                }
                let close = self.expect_symbol(c, ")", "call")?;
                Some(Closed::Operand(
                    Expr::Call(Call {
                        span: Span::new(callable.span.start, close.end),
                        callable,
                        arguments,
                    }),
                    false,
                ))
            }
            Continuation::Collection { open, mut members } => {
                members.push(value);
                if self.at_symbol(c, ",") {
                    let comma = c.peek()?.span;
                    c.bump();
                    if c.eat(TokenKind::Space).is_none() {
                        self.emitter.emit(
                            GrammarError::GrammarInvalid,
                            comma,
                            "member_separator",
                            "one SPACE must follow a collection comma",
                        );
                        return None;
                    }
                    return Some(Closed::Continue(Continuation::Collection { open, members }));
                }
                let close = self.expect_symbol(c, "]", "collection")?;
                Some(Closed::Operand(
                    Expr::Collection(Collection {
                        span: Span::new(open.start, close.end),
                        members,
                    }),
                    false,
                ))
            }
        }
    }

    /// `MULTILINE_COLLECTION = "[", NEWLINE, INDENT, EXPRESSION,
    ///  { ",", NEWLINE, EXPRESSION }, NEWLINE, DEDENT, "]"`
    pub(crate) fn multiline_collection(&mut self, c: &mut Cursor<'a>) -> Option<Collection> {
        let open = c.peek()?.span;
        c.bump();
        c.bump(); // NEWLINE, already established by the caller's lookahead.
        if c.eat(TokenKind::Indent).is_none() {
            self.emitter.emit(
                GrammarError::GrammarInvalid,
                open,
                "multiline_collection",
                "a multiline collection opens one indented level",
            );
            return None;
        }
        let mut members = Vec::new();
        loop {
            members.push(self.expression(c)?);
            if self.at_symbol(c, ",") {
                c.bump();
                if c.eat(TokenKind::Newline).is_none() {
                    let locus = c.locus(self.source.len());
                    self.emitter.emit(
                        GrammarError::GrammarInvalid,
                        locus,
                        "multiline_collection",
                        "a NEWLINE must follow each multiline member comma",
                    );
                    return None;
                }
                continue;
            }
            break;
        }
        if c.eat(TokenKind::Newline).is_none() {
            let locus = c.locus(self.source.len());
            self.emitter.emit(
                GrammarError::GrammarInvalid,
                locus,
                "multiline_collection",
                "the final multiline member is followed by a NEWLINE",
            );
            return None;
        }
        if c.eat(TokenKind::Dedent).is_none() {
            let locus = c.locus(self.source.len());
            self.emitter.emit(
                GrammarError::GrammarInvalid,
                locus,
                "multiline_collection",
                "a multiline collection closes its indented level before `]`",
            );
            return None;
        }
        let close = self.expect_symbol(c, "]", "multiline_collection")?;
        Some(Collection {
            span: Span::new(open.start, close.end),
            members,
        })
    }

    // -- helpers ----------------------------------------------------------

    pub(crate) fn at_symbol(&self, c: &Cursor<'a>, symbol: &str) -> bool {
        c.peek()
            .is_some_and(|t| t.kind == TokenKind::Symbol && self.text(t.span) == symbol)
    }

    fn expect_symbol(&mut self, c: &mut Cursor<'a>, symbol: &str, cause: &str) -> Option<Span> {
        if self.at_symbol(c, symbol) {
            let span = c.peek()?.span;
            c.bump();
            return Some(span);
        }
        let locus = c.locus(self.source.len());
        self.emitter.emit(
            GrammarError::GrammarInvalid,
            locus,
            cause,
            format!("`{symbol}` is required here"),
        );
        None
    }

    /// A `SPACE, <word>, SPACE` operator triple, consumed only when complete.
    fn spaced_word_operator(
        &mut self,
        c: &mut Cursor<'a>,
        table: &[(&str, BinaryOp)],
    ) -> Option<(BinaryOp, Span)> {
        let word = c.peek_at(1)?;
        if c.peek()?.kind != TokenKind::Space || word.kind != TokenKind::ReservedWord {
            return None;
        }
        let text = self.text(word.span);
        let op = table.iter().find(|(w, _)| *w == text).map(|(_, o)| *o)?;
        if c.peek_at(2)?.kind != TokenKind::Space {
            return None;
        }
        let span = word.span;
        c.bump();
        c.bump();
        c.bump();
        Some((op, span))
    }

    fn spaced_symbol_operator(
        &mut self,
        c: &mut Cursor<'a>,
        table: &[(&str, BinaryOp)],
    ) -> Option<(BinaryOp, Span)> {
        let symbol = c.peek_at(1)?;
        if c.peek()?.kind != TokenKind::Space || symbol.kind != TokenKind::Symbol {
            return None;
        }
        let text = self.text(symbol.span);
        let op = table.iter().find(|(w, _)| *w == text).map(|(_, o)| *o)?;
        if c.peek_at(2)?.kind != TokenKind::Space {
            return None;
        }
        let span = symbol.span;
        c.bump();
        c.bump();
        c.bump();
        Some((op, span))
    }

    fn peek_spaced_compare_operator(&self, c: &Cursor<'a>) -> Option<(BinaryOp, Span)> {
        let candidate = c.peek_at(1)?;
        if c.peek()?.kind != TokenKind::Space {
            return None;
        }
        if c.peek_at(2)?.kind != TokenKind::Space {
            return None;
        }
        let text = self.text(candidate.span);
        let op = match candidate.kind {
            TokenKind::Symbol => COMPARE_SYMBOLS
                .iter()
                .find(|(s, _)| *s == text)
                .map(|(_, o)| *o),
            TokenKind::ReservedWord => COMPARE_WORDS
                .iter()
                .find(|(w, _)| *w == text)
                .map(|(_, o)| *o),
            _ => None,
        }?;
        Some((op, candidate.span))
    }

    fn spaced_compare_operator(&mut self, c: &mut Cursor<'a>) -> Option<(BinaryOp, Span)> {
        let found = self.peek_spaced_compare_operator(c)?;
        c.bump();
        c.bump();
        c.bump();
        Some(found)
    }
}

// ---------------------------------------------------------------------------
// Operator reduction
// ---------------------------------------------------------------------------
//
// Free functions rather than associated ones. Neither takes `self`, and neither
// mentions the parser's source lifetime, but while they sat inside
// `impl<'a> ExprParser<'a, '_>` a `Self::` call bound them to it anyway: on the
// declared minimum toolchain that made the shunting-yard reduction require
// `'a` to outlive the borrow of its own frame, and `lcl-parser` did not
// compile. Lifting them out states what was already true, and builds on every
// version from the declared minimum upward.

/// Reduce pending operators at or above `min_prec`, left-associatively.
fn reduce(frame: &mut Frame, min_prec: u8) {
    while let Some(&(_, _, prec)) = frame.operators.last() {
        if prec < min_prec {
            break;
        }
        let Some((op, span, _)) = frame.operators.pop() else {
            return;
        };
        let (Some(right), Some(left)) = (frame.operands.pop(), frame.operands.pop()) else {
            return;
        };
        frame.operands.push(binary(left, op, span, right));
    }
}

/// One binary node, spanning from its left operand through its right.
fn binary(left: Expr, op: BinaryOp, operator_span: Span, right: Expr) -> Expr {
    Expr::Binary(Binary {
        operator: op,
        operator_span,
        span: Span::new(left.span().start, right.span().end),
        left: Box::new(left),
        right: Box::new(right),
    })
}
