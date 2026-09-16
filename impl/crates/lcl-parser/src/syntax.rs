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
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    pub key: Word,
    /// From the first byte of `key` through the last byte of the body.
    pub span: Span,
    pub body: Body,
}

/// `OBJECT_PROPERTY = SIMPLE_IDENTIFIER, ":", (SPACE, INLINE_VALUE, NEWLINE |
///  NEWLINE, INDENT, NESTED_BODY, DEDENT)`
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
///
/// [`Clone`] is hand-written and iterative, for the same reason [`Drop`] is:
/// see the implementation below.
#[derive(Debug, PartialEq, Eq)]
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

/// Render one expression exactly as its structure declares it.
///
/// A *rendering*, not an evaluation: `PATH("/a")` renders as `PATH("/a")` and
/// never as a resolved filesystem path. It carries no span, so two occurrences
/// of the same written expression render identically wherever they appear.
pub fn render(expr: &Expr) -> String {
    match expr {
        Expr::Literal(literal) => match literal.kind {
            LiteralKind::String | LiteralKind::MultilineString => format!("{:?}", literal.text),
            _ => literal.text.clone(),
        },
        Expr::Identifier(ident) => ident.text.clone(),
        Expr::Call(call) => {
            let arguments: Vec<String> = call.arguments.iter().map(render).collect();
            format!("{}({})", call.callable.text, arguments.join(", "))
        }
        Expr::Collection(collection) => {
            let members: Vec<String> = collection.members.iter().map(render).collect();
            format!("[{}]", members.join(", "))
        }
        Expr::Group(group) => format!("({})", render(&group.inner)),
        Expr::Unary(unary) => format!("{}{}", unary.operator.lexeme(), render(&unary.operand)),
        Expr::Binary(binary) => format!(
            "{} {} {}",
            render(&binary.left),
            binary.operator.lexeme(),
            render(&binary.right)
        ),
        Expr::Property(property) => format!("{}.{}", render(&property.base), property.name),
        Expr::Index(index) => format!("{}[{}]", render(&index.base), render(&index.index)),
        Expr::Type(ty) => render_type(ty),
    }
}

fn render_type(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Scalar(word) => word.text.clone(),
        TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
            format!("{}[{}]", b.word.text, render(&b.argument))
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
#[derive(Debug, PartialEq, Eq)]
pub struct Conditional {
    pub span: Span,
    /// Span of the `IF` word, the header locus for diagnostics.
    pub keyword_span: Span,
    pub condition: Box<Expr>,
    pub then_body: Vec<Executable>,
    /// `ELSE` is optional and aligns with its `IF`.
    pub else_body: Option<ElseArm>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ElseArm {
    pub span: Span,
    pub keyword_span: Span,
    pub body: Vec<Executable>,
}

/// `FOR_EACH = "FOR", SPACE, "EACH", SPACE, SIMPLE_IDENTIFIER, SPACE, "IN",
///  SPACE, EXPRESSION, ":", NEWLINE, INDENT, EXECUTABLE_BODY, DEDENT`
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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

// ---------------------------------------------------------------------------
// Teardown
// ---------------------------------------------------------------------------

/// Dropping a deep tree must not recurse either.
///
/// The parser builds trees as deep as the source nests, and the compiler's
/// generated glue for `Box<Expr>`, `Vec<Statement>` and `Vec<Executable>` is
/// recursive: it walks one native frame per level. Without these impls the
/// iterative parser would simply move the stack overflow from construction to
/// teardown — measured at roughly 24,000 levels on a 2 MiB stack, which is a
/// larger number and the same defect.
///
/// Each impl takes its children into an explicit worklist and drains it, so the
/// deepest tree costs one heap `Vec` and a constant number of native frames.
/// A node reached through the worklist has already had its children taken, so
/// its own `drop` is shallow and does not re-enter.
enum Debris {
    Expr(Expr),
    Statement(Statement),
    Executable(Executable),
}

/// A childless `Expr`, cheap enough to swap into a box being emptied.
///
/// `String::new` does not allocate, so this costs nothing.
fn leaf() -> Expr {
    Expr::Identifier(Ident {
        text: String::new(),
        span: Span::new(0, 0),
        qualified: false,
    })
}

/// Move one boxed child out, leaving a leaf behind.
fn take(boxed: &mut Box<Expr>, out: &mut Vec<Debris>) {
    out.push(Debris::Expr(std::mem::replace(&mut **boxed, leaf())));
}

fn take_expr_children(expr: &mut Expr, out: &mut Vec<Debris>) {
    match expr {
        Expr::Group(node) => take(&mut node.inner, out),
        Expr::Unary(node) => take(&mut node.operand, out),
        Expr::Binary(node) => {
            take(&mut node.left, out);
            take(&mut node.right, out);
        }
        Expr::Property(node) => take(&mut node.base, out),
        Expr::Index(node) => {
            take(&mut node.base, out);
            take(&mut node.index, out);
        }
        Expr::Call(node) => out.extend(
            std::mem::take(&mut node.arguments)
                .into_iter()
                .map(Debris::Expr),
        ),
        Expr::Collection(node) => out.extend(
            std::mem::take(&mut node.members)
                .into_iter()
                .map(Debris::Expr),
        ),
        Expr::Type(node) => match node {
            TypeExpr::Scalar(_) => {}
            TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
                take(&mut b.argument, out)
            }
        },
        Expr::Literal(_) | Expr::Identifier(_) => {}
    }
}

fn take_body_children(body: &mut Body, out: &mut Vec<Debris>) {
    match body {
        Body::Inline(Value::Expression(expr)) => {
            out.push(Debris::Expr(std::mem::replace(expr, leaf())))
        }
        Body::Inline(Value::MultilineCollection(collection)) => out.extend(
            std::mem::take(&mut collection.members)
                .into_iter()
                .map(Debris::Expr),
        ),
        Body::Nested(nested) => out.extend(
            std::mem::take(&mut nested.statements)
                .into_iter()
                .map(Debris::Statement),
        ),
    }
}

fn take_conditional_children(node: &mut Conditional, out: &mut Vec<Debris>) {
    take(&mut node.condition, out);
    out.extend(
        std::mem::take(&mut node.then_body)
            .into_iter()
            .map(Debris::Executable),
    );
    if let Some(arm) = &mut node.else_body {
        out.extend(
            std::mem::take(&mut arm.body)
                .into_iter()
                .map(Debris::Executable),
        );
    }
}

fn take_for_each_children(node: &mut ForEach, out: &mut Vec<Debris>) {
    take(&mut node.collection, out);
    out.extend(
        std::mem::take(&mut node.body)
            .into_iter()
            .map(Debris::Executable),
    );
}

fn take_statement_children(statement: &mut Statement, out: &mut Vec<Debris>) {
    match statement {
        Statement::Field(node) => take_body_children(&mut node.body, out),
        Statement::Property(node) => take_body_children(&mut node.body, out),
        Statement::Conditional(node) => take_conditional_children(node, out),
        Statement::ForEach(node) => take_for_each_children(node, out),
    }
}

fn take_executable_children(executable: &mut Executable, out: &mut Vec<Debris>) {
    match executable {
        Executable::Block(block) => out.extend(
            std::mem::take(&mut block.body)
                .into_iter()
                .map(Debris::Statement),
        ),
        Executable::Conditional(node) => take_conditional_children(node, out),
        Executable::ForEach(node) => take_for_each_children(node, out),
    }
}

/// Drain a worklist of already-orphaned nodes.
fn dismantle(mut work: Vec<Debris>) {
    while let Some(item) = work.pop() {
        match item {
            Debris::Expr(mut node) => take_expr_children(&mut node, &mut work),
            Debris::Statement(mut node) => take_statement_children(&mut node, &mut work),
            Debris::Executable(mut node) => take_executable_children(&mut node, &mut work),
        }
    }
}

impl Drop for Expr {
    fn drop(&mut self) {
        let mut work = Vec::new();
        take_expr_children(self, &mut work);
        dismantle(work);
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        let mut work = Vec::new();
        take_statement_children(self, &mut work);
        dismantle(work);
    }
}

impl Drop for Executable {
    fn drop(&mut self) {
        let mut work = Vec::new();
        take_executable_children(self, &mut work);
        dismantle(work);
    }
}

// ---------------------------------------------------------------------------
// Cloning without recursion
// ---------------------------------------------------------------------------

/// A deep copy that costs heap rather than native stack.
///
/// Expression nesting is unbounded — `04_GRAMMAR/10` states the shape and no
/// limit — which is why this module already dismantles a tree through a
/// worklist rather than letting derived drop glue recurse. A *derived* `Clone`
/// has exactly the same defect at exactly the same depths, and every consumer
/// that copies a subtree to satisfy the borrow checker pays it: the static
/// checker cloned a document's top-level items, and a deeply nested expression
/// ended the process by `SIGABRT` rather than by a diagnostic.
///
/// The shape is the mirror of `take_expr_children`: list every node in
/// pre-order, then rebuild in reverse, so a node is assembled only after all of
/// its descendants are already built and waiting.
impl Clone for Expr {
    fn clone(&self) -> Expr {
        let mut order: Vec<&Expr> = Vec::new();
        let mut stack: Vec<&Expr> = vec![self];
        while let Some(node) = stack.pop() {
            order.push(node);
            // Pushed in reverse so that popping yields source order, which is
            // what the rebuild below relies on.
            for child in expr_children(node).into_iter().rev() {
                stack.push(child);
            }
        }
        let mut done: Vec<Expr> = Vec::with_capacity(order.len());
        for node in order.into_iter().rev() {
            let built = rebuild_expr(node, &mut done);
            done.push(built);
        }
        done.pop().expect("the root was listed")
    }
}

/// Every direct child expression, in source order.
fn expr_children(expr: &Expr) -> Vec<&Expr> {
    match expr {
        Expr::Literal(_) | Expr::Identifier(_) => Vec::new(),
        Expr::Group(node) => vec![&node.inner],
        Expr::Unary(node) => vec![&node.operand],
        Expr::Binary(node) => vec![&node.left, &node.right],
        Expr::Property(node) => vec![&node.base],
        Expr::Index(node) => vec![&node.base, &node.index],
        Expr::Call(node) => node.arguments.iter().collect(),
        Expr::Collection(node) => node.members.iter().collect(),
        Expr::Type(node) => match node {
            TypeExpr::Scalar(_) => Vec::new(),
            TypeExpr::List(b) | TypeExpr::Set(b) | TypeExpr::Object(b) | TypeExpr::Reference(b) => {
                vec![&b.argument]
            }
        },
    }
}

/// Rebuild one node from children already on `done`, deepest last.
///
/// Reverse pre-order leaves this node's children on top of `done` in source
/// order, so each is popped in turn.
fn rebuild_expr(expr: &Expr, done: &mut Vec<Expr>) -> Expr {
    let take = |done: &mut Vec<Expr>| {
        Box::new(done.pop().expect("every child was built before its parent"))
    };
    match expr {
        Expr::Literal(node) => Expr::Literal(node.clone()),
        Expr::Identifier(node) => Expr::Identifier(node.clone()),
        Expr::Group(node) => Expr::Group(Group {
            span: node.span,
            inner: take(done),
        }),
        Expr::Unary(node) => Expr::Unary(Unary {
            operator: node.operator,
            operator_span: node.operator_span,
            span: node.span,
            operand: take(done),
        }),
        Expr::Binary(node) => {
            let left = take(done);
            let right = take(done);
            Expr::Binary(Binary {
                operator: node.operator,
                operator_span: node.operator_span,
                span: node.span,
                left,
                right,
            })
        }
        Expr::Property(node) => Expr::Property(PropertyAccess {
            span: node.span,
            base: take(done),
            name: node.name.clone(),
            name_span: node.name_span,
            reserved: node.reserved,
        }),
        Expr::Index(node) => {
            let base = take(done);
            let index = take(done);
            Expr::Index(IndexAccess {
                span: node.span,
                base,
                index,
            })
        }
        Expr::Call(node) => Expr::Call(Call {
            callable: node.callable.clone(),
            span: node.span,
            arguments: (0..node.arguments.len())
                .map(|_| done.pop().expect("every argument was built"))
                .collect(),
        }),
        Expr::Collection(node) => Expr::Collection(Collection {
            span: node.span,
            members: (0..node.members.len())
                .map(|_| done.pop().expect("every member was built"))
                .collect(),
        }),
        Expr::Type(node) => Expr::Type(match node {
            TypeExpr::Scalar(word) => TypeExpr::Scalar(word.clone()),
            TypeExpr::List(b) => TypeExpr::List(rebuild_bracket(b, take(done))),
            TypeExpr::Set(b) => TypeExpr::Set(rebuild_bracket(b, take(done))),
            TypeExpr::Object(b) => TypeExpr::Object(rebuild_bracket(b, take(done))),
            TypeExpr::Reference(b) => TypeExpr::Reference(rebuild_bracket(b, take(done))),
        }),
    }
}

fn rebuild_bracket(bracket: &BracketType, argument: Box<Expr>) -> BracketType {
    BracketType {
        word: bracket.word.clone(),
        span: bracket.span,
        argument,
    }
}

// ---------------------------------------------------------------------------
// Cloning a statement forest without recursion
// ---------------------------------------------------------------------------

/// The same defect [`Expr`] had, at the other nesting axis.
///
/// A nested body is a `Vec<Statement>` inside a `Body` inside a `Field` inside
/// a `Statement`, and derived `Clone` walks that cycle one native frame per
/// level. `04_GRAMMAR/02` states the indented-body shape and no depth limit, so
/// a document may nest as far as its author indents, and the static checker
/// copies a document's items to satisfy the borrow checker. Past roughly a
/// thousand levels on a small stack the copy ended the process by `SIGABRT`
/// rather than returning a diagnostic, which `LCL_RELEASE_REPORT.md` section 10
/// recorded as a bound instead of a defect.
///
/// So these ten types clone through the same worklist their [`Drop`] already
/// uses: list every node in pre-order, then rebuild in reverse, so a node is
/// assembled only once all of its descendants are built and waiting. Depth
/// costs heap, which fails as an allocation rather than as an abort.
///
/// [`Value`], [`Collection`] and the expression types keep derived or
/// hand-written clones: every path out of them reaches [`Expr::clone`], which
/// is already iterative, in one frame.
#[derive(Clone, Copy)]
enum Node<'a> {
    Statement(&'a Statement),
    Executable(&'a Executable),
}

/// One rebuilt node, waiting for the parent that will take it.
enum BuiltNode {
    Statement(Statement),
    Executable(Executable),
}

impl BuiltNode {
    fn statement(self) -> Statement {
        match self {
            BuiltNode::Statement(node) => node,
            BuiltNode::Executable(_) => {
                unreachable!("a statement body holds statements, and the listing agrees")
            }
        }
    }

    fn executable(self) -> Executable {
        match self {
            BuiltNode::Executable(node) => node,
            BuiltNode::Statement(_) => {
                unreachable!("an executable body holds executables, and the listing agrees")
            }
        }
    }
}

/// Every direct child of one node, in source order.
fn node_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    match node {
        Node::Statement(Statement::Field(field)) => body_children(&field.body),
        Node::Statement(Statement::Property(property)) => body_children(&property.body),
        Node::Statement(Statement::Conditional(node))
        | Node::Executable(Executable::Conditional(node)) => conditional_children(node),
        Node::Statement(Statement::ForEach(node)) | Node::Executable(Executable::ForEach(node)) => {
            node.body.iter().map(Node::Executable).collect()
        }
        Node::Executable(Executable::Block(block)) => {
            block.body.iter().map(Node::Statement).collect()
        }
    }
}

fn body_children(body: &Body) -> Vec<Node<'_>> {
    match body {
        // An inline value bottoms out in `Expr`, whose own clone is iterative.
        Body::Inline(_) => Vec::new(),
        Body::Nested(nested) => nested.statements.iter().map(Node::Statement).collect(),
    }
}

fn conditional_children(node: &Conditional) -> Vec<Node<'_>> {
    let mut children: Vec<Node<'_>> = node.then_body.iter().map(Node::Executable).collect();
    if let Some(arm) = &node.else_body {
        children.extend(arm.body.iter().map(Node::Executable));
    }
    children
}

/// Clone a whole forest of nodes, deepest first, and return the roots in the
/// order they were given.
fn clone_forest(roots: Vec<Node<'_>>) -> Vec<BuiltNode> {
    let count = roots.len();
    let mut order: Vec<Node<'_>> = Vec::new();
    let mut stack: Vec<Node<'_>> = roots.into_iter().rev().collect();
    while let Some(node) = stack.pop() {
        order.push(node);
        // Pushed in reverse so that popping yields source order, which is what
        // the rebuild below relies on.
        for child in node_children(node).into_iter().rev() {
            stack.push(child);
        }
    }
    let mut done: Vec<BuiltNode> = Vec::with_capacity(order.len());
    for node in order.into_iter().rev() {
        let built = rebuild_node(node, &mut done);
        done.push(built);
    }
    // Every non-root was consumed by its parent, so `done` now holds the roots
    // with the first one on top.
    let mut roots: Vec<BuiltNode> = Vec::with_capacity(count);
    for _ in 0..count {
        roots.push(done.pop().expect("every root was built"));
    }
    roots
}

/// Take `count` already-built statements off the stack, in source order.
fn take_statements(done: &mut Vec<BuiltNode>, count: usize) -> Vec<Statement> {
    (0..count)
        .map(|_| {
            done.pop()
                .expect("every child was built before its parent")
                .statement()
        })
        .collect()
}

/// Take `count` already-built executables off the stack, in source order.
fn take_executables(done: &mut Vec<BuiltNode>, count: usize) -> Vec<Executable> {
    (0..count)
        .map(|_| {
            done.pop()
                .expect("every child was built before its parent")
                .executable()
        })
        .collect()
}

fn rebuild_node(node: Node<'_>, done: &mut Vec<BuiltNode>) -> BuiltNode {
    match node {
        Node::Statement(statement) => BuiltNode::Statement(rebuild_statement(statement, done)),
        Node::Executable(executable) => BuiltNode::Executable(rebuild_executable(executable, done)),
    }
}

fn rebuild_statement(statement: &Statement, done: &mut Vec<BuiltNode>) -> Statement {
    match statement {
        Statement::Field(field) => Statement::Field(Field {
            key: field.key.clone(),
            span: field.span,
            body: rebuild_body(&field.body, done),
        }),
        Statement::Property(property) => Statement::Property(Property {
            key: property.key.clone(),
            span: property.span,
            body: rebuild_body(&property.body, done),
        }),
        Statement::Conditional(node) => Statement::Conditional(rebuild_conditional(node, done)),
        Statement::ForEach(node) => Statement::ForEach(rebuild_for_each(node, done)),
    }
}

fn rebuild_executable(executable: &Executable, done: &mut Vec<BuiltNode>) -> Executable {
    match executable {
        Executable::Block(block) => Executable::Block(rebuild_block(block, done)),
        Executable::Conditional(node) => Executable::Conditional(rebuild_conditional(node, done)),
        Executable::ForEach(node) => Executable::ForEach(rebuild_for_each(node, done)),
    }
}

fn rebuild_body(body: &Body, done: &mut Vec<BuiltNode>) -> Body {
    match body {
        Body::Inline(value) => Body::Inline(value.clone()),
        Body::Nested(nested) => Body::Nested(Nested {
            span: nested.span,
            statements: take_statements(done, nested.statements.len()),
        }),
    }
}

fn rebuild_block(block: &Block, done: &mut Vec<BuiltNode>) -> Block {
    Block {
        key: block.key.clone(),
        span: block.span,
        body: take_statements(done, block.body.len()),
    }
}

fn rebuild_conditional(node: &Conditional, done: &mut Vec<BuiltNode>) -> Conditional {
    let then_body = take_executables(done, node.then_body.len());
    let else_body = node.else_body.as_ref().map(|arm| ElseArm {
        span: arm.span,
        keyword_span: arm.keyword_span,
        body: take_executables(done, arm.body.len()),
    });
    Conditional {
        span: node.span,
        keyword_span: node.keyword_span,
        condition: Box::new((*node.condition).clone()),
        then_body,
        else_body,
    }
}

fn rebuild_for_each(node: &ForEach, done: &mut Vec<BuiltNode>) -> ForEach {
    ForEach {
        span: node.span,
        keyword_span: node.keyword_span,
        binding: node.binding.clone(),
        collection: Box::new((*node.collection).clone()),
        body: take_executables(done, node.body.len()),
    }
}

impl Clone for Statement {
    fn clone(&self) -> Statement {
        clone_forest(vec![Node::Statement(self)])
            .pop()
            .expect("the root was listed")
            .statement()
    }
}

impl Clone for Executable {
    fn clone(&self) -> Executable {
        clone_forest(vec![Node::Executable(self)])
            .pop()
            .expect("the root was listed")
            .executable()
    }
}

impl Clone for Block {
    fn clone(&self) -> Block {
        let mut done = clone_children_of(self.body.iter().map(Node::Statement).collect());
        rebuild_block(self, &mut done)
    }
}

impl Clone for Field {
    fn clone(&self) -> Field {
        let mut done = clone_children_of(body_children(&self.body));
        Field {
            key: self.key.clone(),
            span: self.span,
            body: rebuild_body(&self.body, &mut done),
        }
    }
}

impl Clone for Property {
    fn clone(&self) -> Property {
        let mut done = clone_children_of(body_children(&self.body));
        Property {
            key: self.key.clone(),
            span: self.span,
            body: rebuild_body(&self.body, &mut done),
        }
    }
}

impl Clone for Body {
    fn clone(&self) -> Body {
        let mut done = clone_children_of(body_children(self));
        rebuild_body(self, &mut done)
    }
}

impl Clone for Nested {
    fn clone(&self) -> Nested {
        let mut done = clone_children_of(self.statements.iter().map(Node::Statement).collect());
        Nested {
            span: self.span,
            statements: take_statements(&mut done, self.statements.len()),
        }
    }
}

impl Clone for Conditional {
    fn clone(&self) -> Conditional {
        let mut done = clone_children_of(conditional_children(self));
        rebuild_conditional(self, &mut done)
    }
}

impl Clone for ElseArm {
    fn clone(&self) -> ElseArm {
        let mut done = clone_children_of(self.body.iter().map(Node::Executable).collect());
        ElseArm {
            span: self.span,
            keyword_span: self.keyword_span,
            body: take_executables(&mut done, self.body.len()),
        }
    }
}

impl Clone for ForEach {
    fn clone(&self) -> ForEach {
        let mut done = clone_children_of(self.body.iter().map(Node::Executable).collect());
        rebuild_for_each(self, &mut done)
    }
}

/// Clone a node's children into the stack its own rebuild will pop from.
///
/// `clone_forest` returns the roots with the first on top; a rebuild pops in
/// the same direction, so the two agree without reversing anything.
fn clone_children_of(children: Vec<Node<'_>>) -> Vec<BuiltNode> {
    let mut built = clone_forest(children);
    built.reverse();
    built
}
