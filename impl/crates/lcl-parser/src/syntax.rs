//! The syntax model.
//!
//! Every node here corresponds to a production that
//! `04_GRAMMAR/10_COMPLETE_EBNF.ebnf` actually names. Nothing is invented for
//! implementation convenience, and the parser assigns no meaning: a
//! [`Field`] records that the source spelled a registered key with a value at a
//! span, not what that key does.
//!
//! ## One tree, not two
//!
//! This crate produces a **source-faithful syntax tree and no second lowered
//! AST**. The architecture contract gives the parser "source-faithful
//! syntax/AST plus grammar/schema diagnostics" and forbids it from resolving
//! imports or `REF`, type-checking, or evaluating. A lowered form would either
//! restate this tree or begin making the binding and typing decisions that
//! `lcl-resolver` and `lcl-checker` own, so lowering is deliberately deferred:
//! the resolved program graph is M3's artifact, built *from* this tree.
//!
//! ## Source identity and spans
//!
//! Byte offsets are normative (`location_rule`), so every node stores the exact
//! [`Span`] of the bytes it came from, measured against the same buffer the
//! lexer was handed. A node's span always covers its children. Derived
//! line/column values appear only on diagnostics, never here.
//!
//! ## Recovery cannot fabricate
//!
//! There is no "error node" carrying an invented identifier, type, reference or
//! value, and no default is applied here. Where the parser cannot build a node
//! it emits a registered diagnostic and records nothing, so a later stage can
//! never mistake a repair for source. [`Value`] has no `Missing` variant for
//! the same reason.

use lcl_lexer::Span;

/// A registered uppercase word occurring as a block word, field key, type word
/// or operator, with its exact source span.
///
/// The text is an owned copy of the lexeme rather than a borrow, so a syntax
/// tree outlives the buffer it was parsed from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Word {
    pub text: String,
    pub span: Span,
}

/// A lowercase identifier: `SIMPLE_IDENTIFIER` or `QUALIFIED_IDENTIFIER`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ident {
    pub text: String,
    pub span: Span,
    /// True when the lexeme contained at least one `.` separator.
    pub qualified: bool,
}

impl Ident {
    /// The dot-separated segments, in source order. Never empty.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.text.split('.')
    }
}

/// A complete parsed document.
///
/// `DOCUMENT = { BLANK_LINE }, LCL_HEADER, { BLANK_LINE },
///  SPECIFICATION_HEADER, { BLANK_LINE }, { TOP_LEVEL_BLOCK, { BLANK_LINE } },
///  EOF`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The whole source, from its first byte to the end-of-file offset.
    pub span: Span,
    /// Top-level items in exact source order.
    ///
    /// The `LCL` and `SPECIFICATION` headers, when present, are the first two
    /// entries and are ordinary [`Block`]s: the EBNF gives them their own
    /// productions only to pin their position and their restricted bodies, and
    /// the block-schema registry expresses that as the `top_level_first` and
    /// `top_level_second` contexts.
    pub items: Vec<TopLevel>,
}

impl Document {
    /// The first top-level block spelled `name`, if any.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|b| b.key.text == name)
    }

    /// Every top-level block, in source order, skipping conditionals and loops.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.items.iter().filter_map(|i| match i {
            TopLevel::Block(b) => Some(b),
            _ => None,
        })
    }
}

/// `TOP_LEVEL_BLOCK = CORE_BLOCK | CONDITIONAL | FOR_EACH`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopLevel {
    Block(Block),
    Conditional(Conditional),
    ForEach(ForEach),
}

impl TopLevel {
    pub fn span(&self) -> Span {
        match self {
            TopLevel::Block(b) => b.span,
            TopLevel::Conditional(c) => c.span,
            TopLevel::ForEach(f) => f.span,
        }
    }
}

/// `CORE_BLOCK = BLOCK_WORD, ":", NEWLINE, INDENT, BLOCK_BODY, DEDENT`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The `BLOCK_WORD` itself. Its span is the block's header locus, which is
    /// what `error.block.context`, `error.block.duplicate` and
    /// `error.block.occurrence` report against.
    pub key: Word,
    /// From the first byte of `key` through the last byte of the body.
    pub span: Span,
    /// `BLOCK_BODY = BLOCK_STATEMENT, { BLOCK_STATEMENT }` — one or more.
    pub body: Vec<Statement>,
}

impl Block {
    /// Every direct field spelled `name`, in source order.
    pub fn fields<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Field> {
        self.body.iter().filter_map(move |s| match s {
            Statement::Field(f) if f.key.text == name => Some(f),
            _ => None,
        })
    }

    /// The first direct field spelled `name`.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.body.iter().find_map(|s| match s {
            Statement::Field(f) if f.key.text == name => Some(f),
            _ => None,
        })
    }
}

/// `BLOCK_STATEMENT = FIELD_LINE | NESTED_FIELD | CONDITIONAL | FOR_EACH`,
/// widened by `NESTED_BODY` to admit `OBJECT_PROPERTY`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// A `FIELD_LINE` or `NESTED_FIELD`: an uppercase registered key.
    Field(Field),
    /// An `OBJECT_PROPERTY`: a lowercase key, legal only in object data.
    ///
    /// Whether this position *is* object data is a schema question, not a
    /// syntactic one, so the parser records the property faithfully and the
    /// schema layer decides.
    Property(Property),
    Conditional(Conditional),
    ForEach(ForEach),
}

impl Statement {
    pub fn span(&self) -> Span {
        match self {
            Statement::Field(f) => f.span,
            Statement::Property(p) => p.span,
            Statement::Conditional(c) => c.span,
            Statement::ForEach(f) => f.span,
        }
    }
}

/// `FIELD_LINE = FIELD_KEY, ":", SPACE, INLINE_VALUE, NEWLINE` or
/// `NESTED_FIELD = FIELD_KEY, ":", NEWLINE, INDENT, NESTED_BODY, DEDENT`.
///
/// `FIELD_KEY = RESERVED_WORD`, so the key is always an uppercase registered
/// word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: Word,
    /// From the first byte of `key` through the last byte of the body.
    pub span: Span,
    pub body: Body,
}

/// `OBJECT_PROPERTY = SIMPLE_IDENTIFIER, ":", (SPACE, INLINE_VALUE, NEWLINE |
///  NEWLINE, INDENT, NESTED_BODY, DEDENT)`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    pub key: Ident,
    pub span: Span,
    pub body: Body,
}

/// What follows a key's colon: an inline value, or an indented body.
///
/// The two forms are the whole of the "colon then space" versus "colon then
/// newline" distinction in `04_GRAMMAR/02`: "A colon followed by NEWLINE opens
/// one indented block. A colon followed by one space and value is inline."
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// `: <value>` on one line.
    Inline(Value),
    /// `:` then an indented `NESTED_BODY`.
    ///
    /// This one syntactic form covers a nested child block, an object-data
    /// value and a locally nested schema alike. Which one it is depends on the
    /// receiving field's registered value kind, so the parser does not choose.
    Nested(Nested),
}

impl Body {
    pub fn span(&self) -> Span {
        match self {
            Body::Inline(v) => v.span(),
            Body::Nested(n) => n.span,
        }
    }

    pub fn as_inline(&self) -> Option<&Value> {
        match self {
            Body::Inline(v) => Some(v),
            Body::Nested(_) => None,
        }
    }

    pub fn as_nested(&self) -> Option<&Nested> {
        match self {
            Body::Nested(n) => Some(n),
            Body::Inline(_) => None,
        }
    }
}

/// An indented `NESTED_BODY`, spanning its statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nested {
    pub span: Span,
    /// `NESTED_BODY = (BLOCK_STATEMENT | OBJECT_PROPERTY), { … }` — one or
    /// more.
    pub statements: Vec<Statement>,
}

/// `INLINE_VALUE = EXPRESSION | MULTILINE_COLLECTION`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Expression(Expr),
    /// `MULTILINE_COLLECTION = "[", NEWLINE, INDENT, EXPRESSION,
    ///  { ",", NEWLINE, EXPRESSION }, NEWLINE, DEDENT, "]"`
    MultilineCollection(Collection),
}

impl Value {
    pub fn span(&self) -> Span {
        match self {
            Value::Expression(e) => e.span(),
            Value::MultilineCollection(c) => c.span,
        }
    }

    pub fn as_expression(&self) -> Option<&Expr> {
        match self {
            Value::Expression(e) => Some(e),
            Value::MultilineCollection(_) => None,
        }
    }
}

/// A bracket literal, inline or multiline.
///
/// Whether it denotes a `LIST` or a `SET` is a receiving-type question that
/// `03_TYPES_AND_VALUES/10` assigns to the static stage, so the syntax model
/// records only the brackets and their members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub span: Span,
    pub members: Vec<Expr>,
}

/// `EXPRESSION`, in the exact precedence the EBNF declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Literal(Literal),
    /// `IDENTIFIER` — a lowercase simple or qualified identifier.
    Identifier(Ident),
    /// `CALL = CALLABLE, "(", [ ARGUMENT, { ",", SPACE, ARGUMENT } ], ")"`.
    ///
    /// `REFERENCE_CALL = "REF", "(", IDENTIFIER, ")"` is a [`Call`] whose
    /// callable is `REF`; its single-identifier argument contract is enforced
    /// where the grammar requires a `REFERENCE_CALL`.
    Call(Call),
    Collection(Collection),
    /// `"(", EXPRESSION, ")"` — retained so the tree stays source-faithful.
    Group(Group),
    Unary(Unary),
    Binary(Binary),
    /// `PROPERTY_ACCESS = ".", (SIMPLE_IDENTIFIER | RESERVED_WORD)`
    Property(PropertyAccess),
    /// `INDEX_ACCESS = "[", EXPRESSION, "]"`
    Index(IndexAccess),
    /// A `TYPE_EXPRESSION` used where the grammar admits one.
    Type(TypeExpr),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Literal(l) => l.span,
            Expr::Identifier(i) => i.span,
            Expr::Call(c) => c.span,
            Expr::Collection(c) => c.span,
            Expr::Group(g) => g.span,
            Expr::Unary(u) => u.span,
            Expr::Binary(b) => b.span,
            Expr::Property(p) => p.span,
            Expr::Index(i) => i.span,
            Expr::Type(t) => t.span(),
        }
    }
}

/// `LITERAL = STRING | MULTILINE_STRING | INTEGER_LITERAL | DECIMAL_LITERAL |
///  "TRUE" | "FALSE" | "NULL" | "MISSING" | "UNKNOWN"`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    pub kind: LiteralKind,
    pub span: Span,
    /// For `String` and `MultilineString`, the **decoded** value the lexer
    /// produced: escapes resolved, delimiters and the multiline indentation
    /// prefix removed. For every other kind, the exact source lexeme.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LiteralKind {
    String,
    MultilineString,
    Integer,
    Decimal,
    True,
    False,
    Null,
    Missing,
    Unknown,
}

impl LiteralKind {
    /// The registered word for a word-shaped literal.
    pub fn word(self) -> Option<&'static str> {
        match self {
            LiteralKind::True => Some("TRUE"),
            LiteralKind::False => Some("FALSE"),
            LiteralKind::Null => Some("NULL"),
            LiteralKind::Missing => Some("MISSING"),
            LiteralKind::Unknown => Some("UNKNOWN"),
            _ => None,
        }
    }
}

/// A constructor or pure-function call. Arguments are positional only:
/// `04_GRAMMAR/03` makes named, mixed and variadic calls invalid syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The `CALLABLE` word, including `REF`.
    pub callable: Word,
    pub span: Span,
    pub arguments: Vec<Expr>,
}

impl Call {
    /// True when this is a `REFERENCE_CALL`.
    pub fn is_reference(&self) -> bool {
        self.callable.text == "REF"
    }

    /// The identifier argument of a well-formed `REF(identifier)`.
    pub fn reference_target(&self) -> Option<&Ident> {
        if !self.is_reference() {
            return None;
        }
        match self.arguments.as_slice() {
            [Expr::Identifier(id)] => Some(id),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub span: Span,
    pub inner: Box<Expr>,
}

/// `UNARY = (("NOT", SPACE) | "-"), UNARY | POSTFIX`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unary {
    pub operator: UnaryOp,
    /// Span of the operator lexeme alone.
    pub operator_span: Span,
    pub span: Span,
    pub operand: Box<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryOp {
    Not,
    Negate,
}

impl UnaryOp {
    pub fn lexeme(self) -> &'static str {
        match self {
            UnaryOp::Not => "NOT",
            UnaryOp::Negate => "-",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binary {
    pub operator: BinaryOp,
    /// Span of the operator lexeme alone.
    pub operator_span: Span,
    pub span: Span,
    pub left: Box<Expr>,
    pub right: Box<Expr>,
}

/// Every operator the EBNF declares, grouped by the production that admits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    In,
    Contains,
    Matches,
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl BinaryOp {
    pub fn lexeme(self) -> &'static str {
        match self {
            BinaryOp::Or => "OR",
            BinaryOp::And => "AND",
            BinaryOp::Equal => "==",
            BinaryOp::NotEqual => "!=",
            BinaryOp::Less => "<",
            BinaryOp::LessOrEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterOrEqual => ">=",
            BinaryOp::In => "IN",
            BinaryOp::Contains => "CONTAINS",
            BinaryOp::Matches => "MATCHES",
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
        }
    }

    /// The `COMPARE_OPERATOR` alternatives.
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessOrEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterOrEqual
                | BinaryOp::In
                | BinaryOp::Contains
                | BinaryOp::Matches
        )
    }
}

/// `PROPERTY_ACCESS = ".", (SIMPLE_IDENTIFIER | RESERVED_WORD)`
///
/// The uppercase/lowercase distinction selects declaration metadata versus an
/// evaluated `OBJECT` field, which `types_v0.1.0.json#/reference_context_contract`
/// decides at a later stage. The parser records which was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyAccess {
    pub span: Span,
    pub base: Box<Expr>,
    pub name: String,
    pub name_span: Span,
    /// True when the property was spelled as a registered uppercase word.
    pub reserved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexAccess {
    pub span: Span,
    pub base: Box<Expr>,
    pub index: Box<Expr>,
}

/// `TYPE_EXPRESSION = NON_NULL_TYPE_EXPRESSION | REFERENCE_CALL | "NULL"`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// One `SCALAR_TYPE` word.
    Scalar(Word),
    /// `"LIST", "[", TYPE_EXPRESSION, "]"`
    List(BracketType),
    /// `"SET", "[", TYPE_EXPRESSION, "]"`
    Set(BracketType),
    /// `"OBJECT", "[", REFERENCE_CALL, "]"`
    Object(BracketType),
    /// `"REFERENCE", "[", REFERENCE_CALL, "]"`
    Reference(BracketType),
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Scalar(w) => w.span,
            TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
                b.span
            }
        }
    }

    /// The constructor word: `LIST`, `SET`, `OBJECT`, `REFERENCE`, or the
    /// scalar type word itself.
    pub fn word(&self) -> &Word {
        match self {
            TypeExpr::Scalar(w) => w,
            TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
                &b.word
            }
        }
    }
}

/// One of the four bracketed type forms, with its single argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BracketType {
    pub word: Word,
    pub span: Span,
    /// A nested `TYPE_EXPRESSION` for `LIST`/`SET`, or a `REFERENCE_CALL` for
    /// `OBJECT`/`REFERENCE`.
    pub argument: Box<Expr>,
}

/// `CONDITIONAL = "IF", SPACE, "(", EXPRESSION, ")", SPACE, "THEN", ":",
///  NEWLINE, INDENT, EXECUTABLE_BODY, DEDENT,
///  [ "ELSE", ":", NEWLINE, INDENT, EXECUTABLE_BODY, DEDENT ]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conditional {
    pub span: Span,
    /// Span of the `IF` word, the header locus for diagnostics.
    pub keyword_span: Span,
    pub condition: Box<Expr>,
    pub then_body: Vec<Executable>,
    /// `ELSE` is optional and aligns with its `IF`.
    pub else_body: Option<ElseArm>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElseArm {
    pub span: Span,
    pub keyword_span: Span,
    pub body: Vec<Executable>,
}

/// `FOR_EACH = "FOR", SPACE, "EACH", SPACE, SIMPLE_IDENTIFIER, SPACE, "IN",
///  SPACE, EXPRESSION, ":", NEWLINE, INDENT, EXECUTABLE_BODY, DEDENT`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForEach {
    pub span: Span,
    /// Span of the `FOR` word, the header locus for diagnostics.
    pub keyword_span: Span,
    /// The loop-local binding. `04_GRAMMAR/04` gives it no key or comparator
    /// syntax.
    pub binding: Ident,
    pub collection: Box<Expr>,
    pub body: Vec<Executable>,
}

/// `EXECUTABLE_STATEMENT = STEP_BLOCK | CONDITIONAL | FOR_EACH | COMMENT_BLOCK`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executable {
    /// `STEP_BLOCK` or `COMMENT_BLOCK`; both are `BLOCK_WORD, ":", …` with a
    /// `BLOCK_BODY`, so both are ordinary [`Block`]s.
    Block(Block),
    Conditional(Conditional),
    ForEach(ForEach),
}

impl Executable {
    pub fn span(&self) -> Span {
        match self {
            Executable::Block(b) => b.span,
            Executable::Conditional(c) => c.span,
            Executable::ForEach(f) => f.span,
        }
    }
}
