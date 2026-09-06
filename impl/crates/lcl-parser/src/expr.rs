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

    /// `EXPRESSION = OR_EXPRESSION`
    pub(crate) fn expression(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        self.or_expression(c)
    }

    fn or_expression(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let mut left = self.and_expression(c)?;
        while let Some((op, op_span)) = self.spaced_word_operator(c, &[("OR", BinaryOp::Or)]) {
            let right = self.and_expression(c)?;
            left = Self::binary(left, op, op_span, right);
        }
        Some(left)
    }

    fn and_expression(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let mut left = self.comparison(c)?;
        while let Some((op, op_span)) = self.spaced_word_operator(c, &[("AND", BinaryOp::And)]) {
            let right = self.comparison(c)?;
            left = Self::binary(left, op, op_span, right);
        }
        Some(left)
    }

    /// `COMPARISON = ADDITIVE, [ SPACE, COMPARE_OPERATOR, SPACE, ADDITIVE ]`
    ///
    /// The bracket is optional-once, not repeated, so a second comparison
    /// operator is a grammar error rather than a left-nested tree.
    fn comparison(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let left = self.additive(c)?;
        let Some((op, op_span)) = self.spaced_compare_operator(c) else {
            return Some(left);
        };
        let right = self.additive(c)?;
        let node = Self::binary(left, op, op_span, right);
        if let Some((_, second)) = self.peek_spaced_compare_operator(c) {
            return self.fail(
                second,
                "comparison_chain",
                format!(
                    "`{}` chains a second comparison operator; COMPARISON admits at most one",
                    self.text(second)
                ),
            );
        }
        Some(node)
    }

    fn additive(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let mut left = self.multiplicative(c)?;
        while let Some((op, op_span)) = self.spaced_symbol_operator(c, &ADD_SYMBOLS) {
            let right = self.multiplicative(c)?;
            left = Self::binary(left, op, op_span, right);
        }
        Some(left)
    }

    fn multiplicative(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let mut left = self.unary(c)?;
        while let Some((op, op_span)) = self.spaced_symbol_operator(c, &MULTIPLY_SYMBOLS) {
            let right = self.unary(c)?;
            left = Self::binary(left, op, op_span, right);
        }
        Some(left)
    }

    /// `UNARY = (("NOT", SPACE) | "-"), UNARY | POSTFIX`
    fn unary(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        if let Some(t) = c.peek() {
            if t.kind == TokenKind::ReservedWord && self.text(t.span) == "NOT" {
                // `NOT` requires the following SPACE; the grammar spells it.
                if c.peek_at(1).map(|n| n.kind) == Some(TokenKind::Space) {
                    let op_span = t.span;
                    c.bump();
                    c.bump();
                    let operand = self.unary(c)?;
                    return Some(Expr::Unary(Unary {
                        operator: UnaryOp::Not,
                        operator_span: op_span,
                        span: Span::new(op_span.start, operand.span().end),
                        operand: Box::new(operand),
                    }));
                }
                return self.fail(
                    t.span,
                    "not_without_space",
                    "`NOT` must be followed by one SPACE".to_string(),
                );
            }
            if t.kind == TokenKind::Symbol && self.text(t.span) == "-" {
                let op_span = t.span;
                c.bump();
                let operand = self.unary(c)?;
                return Some(Expr::Unary(Unary {
                    operator: UnaryOp::Negate,
                    operator_span: op_span,
                    span: Span::new(op_span.start, operand.span().end),
                    operand: Box::new(operand),
                }));
            }
        }
        self.postfix(c)
    }

    /// `POSTFIX = NON_NULL_TYPE_EXPRESSION | VALUE_PRIMARY, { PROPERTY_ACCESS | INDEX_ACCESS }`
    ///
    /// Concatenation binds tighter than alternation in ISO 14977, so a type
    /// expression takes no trailing accessors.
    fn postfix(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        if let Some(type_expr) = self.non_null_type_expression(c) {
            return Some(Expr::Type(type_expr));
        }
        let mut node = self.value_primary(c)?;
        loop {
            match c.peek() {
                Some(t) if t.kind == TokenKind::Symbol && self.text(t.span) == "." => {
                    c.bump();
                    let Some(name) = c.peek() else {
                        return self.fail(
                            t.span,
                            "property_access",
                            "`.` must be followed by a property name".to_string(),
                        );
                    };
                    let reserved = match name.kind {
                        TokenKind::SimpleIdentifier => false,
                        TokenKind::ReservedWord => true,
                        _ => {
                            let span = name.span;
                            return self.fail(
                                span,
                                "property_access",
                                format!(
                                    "PROPERTY_ACCESS names a SIMPLE_IDENTIFIER or RESERVED_WORD, not {}",
                                    name.kind
                                ),
                            );
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
                    let index = self.expression(c)?;
                    let close = self.expect_symbol(c, "]", "index_access")?;
                    node = Expr::Index(IndexAccess {
                        span: Span::new(node.span().start, close.end),
                        base: Box::new(node),
                        index: Box::new(index),
                    });
                }
                _ => return Some(node),
            }
        }
    }

    /// `NON_NULL_TYPE_EXPRESSION`, or `None` when the cursor is not on one.
    ///
    /// A word that is a callable followed by `(` is a `CALL`, not a type: the
    /// constructor spelling wins over the scalar-type spelling for `PATH`,
    /// `REGEX`, `DATE` and the rest.
    fn non_null_type_expression(&mut self, c: &mut Cursor<'a>) -> Option<TypeExpr> {
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
            let argument = self.expression(c)?;
            let close = self.expect_symbol(c, "]", "type_argument")?;
            let span = Span::new(word.span.start, close.end);
            let bracket = BracketType {
                word,
                span,
                argument: Box::new(argument),
            };
            return Some(match name.as_str() {
                "LIST" => TypeExpr::List(bracket),
                "SET" => TypeExpr::Set(bracket),
                "OBJECT" => TypeExpr::Object(bracket),
                _ => TypeExpr::Reference(bracket),
            });
        }
        if self.grammar.is_scalar_type(text) && !self.grammar.is_literal_word(text) {
            let word = self.word(token);
            c.bump();
            return Some(TypeExpr::Scalar(word));
        }
        None
    }

    /// `VALUE_PRIMARY = LITERAL | IDENTIFIER | REFERENCE_CALL | CALL |
    ///  COLLECTION_LITERAL | "(", EXPRESSION, ")"`
    fn value_primary(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let Some(token) = c.peek() else {
            return self.fail(
                Span::empty(self.source.len()),
                "missing_value",
                "a value is required here".to_string(),
            );
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
                Some(Expr::Literal(Literal { kind, span, text }))
            }
            TokenKind::IntegerLiteral | TokenKind::DecimalLiteral => {
                let kind = if token.kind == TokenKind::IntegerLiteral {
                    LiteralKind::Integer
                } else {
                    LiteralKind::Decimal
                };
                let text = self.text(span).to_string();
                c.bump();
                Some(Expr::Literal(Literal { kind, span, text }))
            }
            TokenKind::SimpleIdentifier | TokenKind::QualifiedIdentifier => {
                let text = self.text(span).to_string();
                let qualified = token.kind == TokenKind::QualifiedIdentifier;
                c.bump();
                Some(Expr::Identifier(Ident {
                    text,
                    span,
                    qualified,
                }))
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
                    return Some(Expr::Literal(Literal { kind, span, text }));
                }
                if self.grammar.is_callable(&text) {
                    return self.call(c);
                }
                self.fail(
                    span,
                    "unexpected_word",
                    format!("`{text}` is not a literal, a callable or a type in a value position"),
                )
            }
            TokenKind::Symbol => match self.text(span) {
                "(" => {
                    c.bump();
                    let inner = self.expression(c)?;
                    let close = self.expect_symbol(c, ")", "group")?;
                    Some(Expr::Group(Group {
                        span: Span::new(span.start, close.end),
                        inner: Box::new(inner),
                    }))
                }
                "[" => self.collection(c).map(Expr::Collection),
                other => self.fail(
                    span,
                    "unexpected_symbol",
                    format!("`{other}` does not begin a value"),
                ),
            },
            other => self.fail(
                span,
                "unexpected_token",
                format!("{other} does not begin a value"),
            ),
        }
    }

    /// `CALL = CALLABLE, "(", [ ARGUMENT, { ",", SPACE, ARGUMENT } ], ")"`
    ///
    /// Arguments are positional only. `04_GRAMMAR/03`: "Named, mixed positional
    /// and named, and variadic calls are invalid syntax."
    fn call(&mut self, c: &mut Cursor<'a>) -> Option<Expr> {
        let callable = self.word(c.peek()?);
        c.bump();
        let Some(open) = c.peek() else {
            return self.fail(
                callable.span,
                "call",
                format!("`{}` must be followed by `(`", callable.text),
            );
        };
        if open.kind != TokenKind::Symbol || self.text(open.span) != "(" {
            let span = open.span;
            return self.fail(
                span,
                "call",
                format!("`{}` must be followed by `(`", callable.text),
            );
        }
        c.bump();
        let mut arguments = Vec::new();
        if !self.at_symbol(c, ")") {
            loop {
                arguments.push(self.expression(c)?);
                if self.at_symbol(c, ",") {
                    let comma = c.peek()?.span;
                    c.bump();
                    // The grammar spells `",", SPACE` between arguments.
                    if c.eat(TokenKind::Space).is_none() {
                        return self.fail(
                            comma,
                            "argument_separator",
                            "one SPACE must follow an argument comma".to_string(),
                        );
                    }
                    continue;
                }
                break;
            }
        }
        let close = self.expect_symbol(c, ")", "call")?;
        Some(Expr::Call(Call {
            span: Span::new(callable.span.start, close.end),
            callable,
            arguments,
        }))
    }

    /// `COLLECTION_LITERAL = "[", [ EXPRESSION, { ",", SPACE, EXPRESSION } ], "]"`
    fn collection(&mut self, c: &mut Cursor<'a>) -> Option<Collection> {
        let open = c.peek()?.span;
        c.bump();
        let mut members = Vec::new();
        if !self.at_symbol(c, "]") {
            loop {
                members.push(self.expression(c)?);
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
                    continue;
                }
                break;
            }
        }
        let close = self.expect_symbol(c, "]", "collection")?;
        Some(Collection {
            span: Span::new(open.start, close.end),
            members,
        })
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

    fn binary(left: Expr, op: BinaryOp, operator_span: Span, right: Expr) -> Expr {
        Expr::Binary(Binary {
            operator: op,
            operator_span,
            span: Span::new(left.span().start, right.span().end),
            left: Box::new(left),
            right: Box::new(right),
        })
    }

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
