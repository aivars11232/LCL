//! Static checking of expressions.
//!
//! `05_SEMANTICS/12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt`:
//!
//! > Every expression is statically checked, including names and operand
//! > families in a branch that will not be evaluated.
//!
//! So this module walks *every* expression a document contains, including both
//! arms of a conditional and both operands of `AND`/`OR`, and judges each one
//! against the registered signature — never against a value it would have to
//! demand.
//!
//! ## Type, then value
//!
//! Each expression yields a [`Judgement`]: its static outcome, and its value
//! *only when that value is statically known*. A literal, a constructor over
//! literals and a `DEFINE kind.constant` built from them are statically known;
//! a read of `INPUT`, `OUTPUT`, `STATE`, `CONTEXT` or `MEMORY` is not, and no
//! attempt is made to guess one. Where a registered constraint needs a value
//! this stage does not have, the obligation is recorded for the demanding layer
//! instead of being decided here.
//!
//! ## Overload selection
//!
//! An operator, function or constructor is accepted only when one registered
//! overload admits the exact operand families, with one identical type bound to
//! each type variable across the whole application. "An unregistered name,
//! arity, or operand family is invalid; implementations do not infer extra
//! overloads."

use crate::contracts::{
    ConstructorRow, Designator, FunctionRow, Operand, OperatorRow, Overload, ResultSpec,
};
use crate::numeric::{Decimal, DivisionDefect, Rational};
use crate::pattern::PatternKind;
use crate::schema::Schema;
use crate::ty::{RefTarget, Type, UnitId};
use crate::types::{TypeCatalog, TypeDefect};
use crate::{
    Annotation, Contracts, DemandKind, DemandObligation, Diagnostic, EarlierStageDefect, Emitter,
    Static, StaticError,
};
use lcl_lexer::Span;
use lcl_parser::syntax::{BinaryOp, Call, Collection, Expr, LiteralKind, PropertyAccess, UnaryOp};
use lcl_resolver::{BindingTarget, Resolved, SourceId};
use std::collections::BTreeMap;

/// A statically known value.
///
/// Only what a registered static check actually consumes is represented. An
/// expression whose value is not one of these is not "unknown" in the LCL
/// sense — it simply has no statically known value, and its checks are the
/// demanding layer's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Const {
    Number(Decimal),
    Boolean(bool),
    Text(String),
    /// A registered qualified identifier, e.g. `unit.second`.
    Identifier(String),
    /// A `MEASURE` or `DURATION` with its exact unit.
    Quantity(Decimal, UnitId),
    /// A compiled pattern constructor argument.
    Pattern {
        kind: PatternKind,
        pattern: String,
        flags: String,
    },
    Null,
}

impl Const {
    pub(crate) fn number(&self) -> Option<&Decimal> {
        match self {
            Const::Number(value) => Some(value),
            Const::Quantity(value, _) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Const::Text(value) | Const::Identifier(value) => Some(value),
            _ => None,
        }
    }
}

/// One expression's static judgement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Judgement {
    pub(crate) outcome: Static,
    /// The value, when it is statically known.
    pub(crate) value: Option<Const>,
}

impl Judgement {
    pub(crate) fn value(ty: Type) -> Judgement {
        Judgement {
            outcome: Static::Value(ty),
            value: None,
        }
    }

    pub(crate) fn known(ty: Type, value: Const) -> Judgement {
        Judgement {
            outcome: Static::Value(ty),
            value: Some(value),
        }
    }

    pub(crate) fn rejected() -> Judgement {
        Judgement {
            outcome: Static::Rejected,
            value: None,
        }
    }

    pub(crate) fn plain(outcome: Static) -> Judgement {
        Judgement {
            outcome,
            value: None,
        }
    }

    pub(crate) fn ty(&self) -> Option<&Type> {
        self.outcome.ty()
    }

    pub(crate) fn is_rejected(&self) -> bool {
        self.outcome.is_rejected()
    }
}

/// What the receiving context requires of an expression.
///
/// `types_v0.1.0.json#/reference_context_contract` decides identity versus
/// value from exactly this: "A field-signature reference or reference-list
/// slot, a source type or schema slot, an explicitly REFERENCE-typed value
/// slot, and an operation target or argument whose contract explicitly names a
/// REFERENCE alternative … retain the referenced identity."
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expected {
    /// No declared type; the expression's own type stands.
    None,
    /// Exactly this type.
    Type(Type),
    /// A condition slot: `boolean_expression`.
    Boolean,
    /// A reference slot: the identity is retained, never read.
    Identity,
    /// A `type_expression` slot.
    TypeExpression,
    /// A `qualified_identifier(DOMAIN)` slot.
    Identifier(String),
    /// The immediate bracket argument of `ALL`, `ANY` or `NONE`.
    Quantifier,
    /// A member position of a collection whose item type is declared.
    ///
    /// `error.collection.heterogeneous` is registered as "A LIST or SET contains
    /// a member incompatible with its **declared** item type", while
    /// `03_TYPES_AND_VALUES/10` gives `error.type.mismatch` to "incompatible
    /// member types" where no item type was declared. The two cases have
    /// different identifiers, so they have different receiving contracts.
    Member(Type),
    /// A `DEFAULT` slot of this declared type. `03_TYPES_AND_VALUES/09` admits
    /// the literal `MISSING` here — "It may appear literally only in an
    /// equality/inequality test, DEFAULT, ASSUME condition, handler condition,
    /// or conformance case" — and `DEFAULT` "replaces MISSING only", so it
    /// never admits `UNKNOWN`.
    Default(Type),
}

impl Expected {
    fn ty(&self) -> Option<&Type> {
        match self {
            Expected::Type(ty) | Expected::Default(ty) | Expected::Member(ty) => Some(ty),
            _ => None,
        }
    }

    /// The member type an expected collection type supplies to a bracket
    /// literal, per `collection_expression/member_type`.
    fn member(&self) -> Option<&Type> {
        self.ty().and_then(Type::member)
    }
}

/// Everything one static check pass needs, and everything it produces.
pub(crate) struct Check<'a> {
    pub(crate) contracts: &'a Contracts,
    pub(crate) resolved: &'a Resolved,
    pub(crate) catalog: &'a TypeCatalog,
    pub(crate) emitter: Emitter<'a>,
    /// Declared type of each declaration, by declaration index.
    pub(crate) declaration_types: BTreeMap<usize, Type>,
    /// Statically known value of each `DEFINE kind.constant`.
    pub(crate) constants: BTreeMap<usize, Const>,
    /// Resolved schema of each declaration that declares one.
    pub(crate) schemas: BTreeMap<usize, Schema>,
    /// Loop-local bindings currently in scope, innermost last.
    pub(crate) locals: Vec<(String, Type)>,
    /// True only while checking `IMPORT.SOURCE` or `EXTENSION.SOURCE`, the two
    /// slots where a one-STRING relative `PATH` is legal.
    pub(crate) relative_path_allowed: bool,
    /// True only during the constant pre-pass, which learns statically known
    /// values. Every one of those expressions is checked again, with
    /// diagnostics, by the walk, so reporting them twice would be a duplicate,
    /// not a second defect.
    pub(crate) silent: bool,
    pub(crate) raw: Vec<Diagnostic>,
    pub(crate) annotations: BTreeMap<(SourceId, Span), Annotation>,
    /// Statically known values, by the exact locus that produced them.
    pub(crate) values: BTreeMap<(SourceId, Span), Const>,
    /// Memoized `(declaration, field)` static types for declaration-property
    /// reads.
    pub(crate) field_types: BTreeMap<(usize, String), Option<Type>>,
    /// The `(declaration, field)` pairs currently being resolved, so a field
    /// that reads itself terminates.
    pub(crate) resolving: std::collections::BTreeSet<(usize, String)>,
    pub(crate) property_depth: usize,
    pub(crate) deferred: Vec<DemandObligation>,
    pub(crate) earlier: Vec<EarlierStageDefect>,
    /// Judgements the driver computed ahead of the walk, by the child's address
    /// in the syntax tree. See [`Check::flatten`].
    pub(crate) ready: std::collections::HashMap<*const Expr, Judgement>,
    /// The same, for a child whose parent asks for a judgement without the
    /// receiving contract: the inner expression of a group.
    pub(crate) ready_judge: std::collections::HashMap<*const Expr, Judgement>,
}

/// How a parent asks for a child's judgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    /// Through `expression`: judged, then received and recorded at its own
    /// locus.
    Receiving,
    /// Through `judge`: judged alone, because the parent receives the contract
    /// at the parent's locus.
    JudgeOnly,
}

/// The receiving contract a collection's members are judged against.
///
/// `03_TYPES_AND_VALUES/10`: "An exact expected member type supplies the
/// context for empty and nested literals", and "The rule applies recursively to
/// the members of a reference-typed collection", so an identity slot's bracket
/// members are identities too.
fn member_expectation(expected: &Expected) -> Expected {
    if matches!(expected, Expected::Identity) {
        return Expected::Identity;
    }
    expected
        .member()
        .cloned()
        .map(Expected::Member)
        .unwrap_or(Expected::None)
}

impl Check<'_> {
    pub(crate) fn emit(
        &mut self,
        id: StaticError,
        source: &SourceId,
        span: Span,
        cause: &str,
        detail: String,
    ) {
        if self.silent {
            return;
        }
        let mut raw = std::mem::take(&mut self.raw);
        self.emitter.emit(&mut raw, id, source, span, cause, detail);
        self.raw = raw;
    }

    pub(crate) fn defer(
        &mut self,
        source: &SourceId,
        span: Span,
        kind: DemandKind,
        detail: String,
    ) {
        if self.silent {
            return;
        }
        self.deferred.push(DemandObligation {
            source: source.clone(),
            span,
            kind,
            detail,
        });
    }

    /// Record a resolved type designator without judging it again.
    pub(crate) fn record_designator(&mut self, source: &SourceId, span: Span, ty: Type) {
        let outcome = Static::TypeDesignator(ty);
        self.record(source, span, &outcome);
    }

    fn record(&mut self, source: &SourceId, span: Span, outcome: &Static) {
        self.annotations.insert(
            (source.clone(), span),
            Annotation {
                source: source.clone(),
                span,
                outcome: outcome.clone(),
            },
        );
    }

    /// Check one expression against its receiving contract.
    pub(crate) fn expression(
        &mut self,
        source: &SourceId,
        expr: &Expr,
        expected: &Expected,
    ) -> Judgement {
        // A judgement the driver already computed for exactly this node.
        if let Some(ready) = self.ready.remove(&(expr as *const Expr)) {
            return ready;
        }
        self.flatten(source, expr, expected);
        if let Some(ready) = self.ready.remove(&(expr as *const Expr)) {
            return ready;
        }
        self.judge_one(source, expr, expected)
    }

    /// Judge every predictable descendant of one expression, deepest first.
    ///
    /// ## Why this exists
    ///
    /// Expression nesting is unbounded: `04_GRAMMAR/10` states the shape and no
    /// limit, and no registered diagnostic permits an implementation-defined
    /// nesting rejection, so the depth a document may reach is not this
    /// implementation's to cap. M2 drew the same conclusion and made the
    /// parser's four nesting paths iterative. This stage walked the tree it
    /// received with native recursion and died by `SIGABRT` — not by a
    /// diagnostic — at a few thousand levels of groups, collections, call
    /// arguments, unary prefixes or binary operands.
    ///
    /// ## How it works
    ///
    /// [`Check::children_of`] answers, without judging anything, which child
    /// expressions a node's own judging function will ask for and with which
    /// receiving contract. The driver descends that spine, judges the nodes it
    /// collected deepest-first, and leaves each result where the judging
    /// function will find it. A judging function therefore never recurses into
    /// a child it did not have to: it asks, and the answer is already there.
    ///
    /// ## Where it stops
    ///
    /// `children_of` answers `None` for any node whose children it cannot
    /// predict exactly. That node is judged the way it always was, natively,
    /// and its own children are flattened by a fresh driver call. An unsure
    /// answer therefore costs one stack frame and never changes a judgement.
    fn flatten(&mut self, source: &SourceId, root: &Expr, expected: &Expected) {
        let mut order: Vec<(&Expr, Expected, Via)> = Vec::new();
        let mut stack: Vec<(&Expr, Expected, Via)> = vec![(root, expected.clone(), Via::Receiving)];
        while let Some((node, expectation, via)) = stack.pop() {
            let children = self.children_of(node, &expectation);
            order.push((node, expectation, via));
            let Some(children) = children else {
                continue;
            };
            stack.extend(children);
        }
        // One node is the caller's own, and judging it here would gain nothing.
        if order.len() <= 1 {
            return;
        }
        // A pre-order listing reversed puts every node after its descendants.
        for (node, expectation, via) in order.into_iter().rev() {
            let key = node as *const Expr;
            match via {
                Via::Receiving => {
                    let judgement = self.judge_one(source, node, &expectation);
                    self.ready.insert(key, judgement);
                }
                Via::JudgeOnly => {
                    let judgement = self.judge(source, node, &expectation);
                    self.ready_judge.insert(key, judgement);
                }
            }
        }
    }

    /// Which children a node's judging function will ask for, and how.
    ///
    /// `None` means "not predictable here", which is always safe: the node is
    /// then judged natively. Every answer must match what the judging function
    /// actually does, because a child judged under the wrong receiving contract
    /// would be judged twice, once wrongly.
    fn children_of<'e>(
        &mut self,
        expr: &'e Expr,
        expected: &Expected,
    ) -> Option<Vec<(&'e Expr, Expected, Via)>> {
        match expr {
            Expr::Literal(_) | Expr::Identifier(_) | Expr::Type(_) => Some(Vec::new()),
            // `judge` delegates a group to its inner expression *as a judgement*,
            // because "A receiving identity context applies to a direct REF
            // expression, including parentheses around it": the contract is
            // received once, at the group's own locus.
            Expr::Group(group) => Some(vec![(&group.inner, expected.clone(), Via::JudgeOnly)]),
            Expr::Unary(unary) => Some(vec![(&unary.operand, Expected::None, Via::Receiving)]),
            Expr::Binary(binary) => Some(vec![
                (&binary.left, Expected::None, Via::Receiving),
                (&binary.right, Expected::None, Via::Receiving),
            ]),
            Expr::Index(index) => Some(vec![
                (&index.base, Expected::None, Via::Receiving),
                (&index.index, Expected::None, Via::Receiving),
            ]),
            // A reserved property reads the declaration's registered field
            // rather than judging its base as a value.
            Expr::Property(property) if property.reserved => None,
            Expr::Property(property) => {
                Some(vec![(&property.base, Expected::None, Via::Receiving)])
            }
            Expr::Collection(collection) => {
                let quantifier = matches!(expected, Expected::Quantifier);
                let member = member_expectation(expected);
                Some(
                    collection
                        .members
                        .iter()
                        .map(|m| {
                            let expectation = match quantifier {
                                true => Expected::Quantifier,
                                false => member.clone(),
                            };
                            (m, expectation, Via::Receiving)
                        })
                        .collect(),
                )
            }
            Expr::Call(call) => self.call_children(call),
        }
    }

    /// A call's arguments, when this build can say what contract receives them.
    fn call_children<'e>(&mut self, call: &'e Call) -> Option<Vec<(&'e Expr, Expected, Via)>> {
        // `REF(...)` reads a binding rather than judging an argument.
        if call.is_reference() {
            return None;
        }
        let name = call.callable.text.clone();
        if let Some(row) = self.contracts.constructor(&name).cloned() {
            return Some(
                call.arguments
                    .iter()
                    .enumerate()
                    .map(|(position, argument)| {
                        (
                            argument,
                            self.constructor_expectation(&row, position),
                            Via::Receiving,
                        )
                    })
                    .collect(),
            );
        }
        if let Some(row) = self.contracts.function(&name).cloned() {
            // ROUND's first argument takes a different path when it *is* a
            // direct division, which the row materializes once as an exact
            // rational rather than judging as an ordinary operand. Any other
            // first argument is judged exactly like the rest.
            if row.name == "ROUND" && call.arguments.first().is_some_and(is_direct_division) {
                return None;
            }
            let quantifier = row.argument_contract.as_deref() == Some("quantifier_argument");
            let expectation = match quantifier {
                true => Expected::Quantifier,
                false => Expected::None,
            };
            return Some(
                call.arguments
                    .iter()
                    .map(|argument| (argument, expectation.clone(), Via::Receiving))
                    .collect(),
            );
        }
        // An unregistered callable judges no argument at all.
        Some(Vec::new())
    }

    /// Judge one node and apply the receiving contract at its own locus.
    fn judge_one(&mut self, source: &SourceId, expr: &Expr, expected: &Expected) -> Judgement {
        let judgement = self.judge(source, expr, expected);
        let judgement = self.receive(source, expr.span(), expected, judgement);
        self.record(source, expr.span(), &judgement.outcome);
        if let Some(value) = &judgement.value {
            self.values
                .insert((source.clone(), expr.span()), value.clone());
        }
        judgement
    }

    /// Judge one expression against the contract that receives it.
    ///
    /// `01_FOUNDATION/03`: every expression has "its names, arity, operand
    /// families, and receiving contract checked before effects". The first
    /// three are the signature's; this is the receiving contract.
    fn receive(
        &mut self,
        source: &SourceId,
        span: Span,
        expected: &Expected,
        judgement: Judgement,
    ) -> Judgement {
        if judgement.is_rejected() {
            return judgement;
        }
        match expected {
            // A member incompatible with its declared item type has its own
            // registered identifier.
            Expected::Member(declared) => match &judgement.outcome {
                Static::Value(actual) | Static::Identity(actual) => {
                    if declared.accepts(actual) {
                        judgement
                    } else {
                        let (actual, declared) = (actual.clone(), declared.clone());
                        self.emit(
                            StaticError::CollectionHeterogeneous,
                            source,
                            span,
                            "collection_member",
                            format!(
                                "this member is {actual}, not the declared item type {declared}"
                            ),
                        );
                        Judgement::rejected()
                    }
                }
                // A sentinel is never a material collection member, and each has
                // its own registered rule.
                Static::Missing => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        span,
                        "collection_member",
                        "MISSING is a non-material sentinel and cannot be a collection member"
                            .to_string(),
                    );
                    Judgement::rejected()
                }
                Static::Unknown => {
                    self.emit(
                        StaticError::ValueUnknown,
                        source,
                        span,
                        "collection_member",
                        "UNKNOWN is a non-material sentinel and cannot be a collection member"
                            .to_string(),
                    );
                    Judgement::rejected()
                }
                _ => judgement,
            },
            // `DEFAULT` admits the literal MISSING it exists to replace.
            Expected::Default(_) if matches!(judgement.outcome, Static::Missing) => judgement,
            Expected::Type(declared) | Expected::Default(declared) => {
                match &judgement.outcome {
                    Static::Value(actual) | Static::Identity(actual) => {
                        if declared.accepts(actual) {
                            judgement
                        } else {
                            self.emit(
                                StaticError::TypeMismatch,
                                source,
                                span,
                                "declared_type",
                                format!("this value is {actual}, not the declared type {declared}"),
                            );
                            Judgement::rejected()
                        }
                    }
                    // "MISSING … has no storable user type."
                    Static::Missing => {
                        self.emit(
                            StaticError::TypeMismatch,
                            source,
                            span,
                            "sentinel_placement",
                            format!("MISSING has no storable type and cannot satisfy {declared}"),
                        );
                        Judgement::rejected()
                    }
                    // "A required material destination rejects UNKNOWN with
                    // error.value.unknown."
                    Static::Unknown => {
                        self.emit(
                            StaticError::ValueUnknown,
                            source,
                            span,
                            "sentinel_placement",
                            format!("UNKNOWN cannot bind a required {declared} destination"),
                        );
                        Judgement::rejected()
                    }
                    Static::Identifier(name) => {
                        self.emit(
                            StaticError::TypeMismatch,
                            source,
                            span,
                            "declared_type",
                            format!("`{name}` is a registered identifier, not a {declared} value"),
                        );
                        Judgement::rejected()
                    }
                    Static::TypeDesignator(_) => {
                        // "A type designator is not a first-class material value."
                        self.emit(
                        StaticError::TypeMismatch,
                        source,
                        span,
                        "type_designator",
                        format!("a type designator cannot stand where a {declared} value is required"),
                    );
                        Judgement::rejected()
                    }
                    Static::Opaque | Static::Rejected => judgement,
                }
            }
            // "boolean_expression: Expression statically producing BOOLEAN or
            // UNKNOWN under special-value rules."
            Expected::Boolean => match &judgement.outcome {
                Static::Value(Type::Boolean) | Static::Unknown => judgement,
                Static::Value(other) => {
                    let other = other.clone();
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        span,
                        "boolean_condition",
                        format!("a condition is BOOLEAN, not {other}"),
                    );
                    Judgement::rejected()
                }
                Static::Missing => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        span,
                        "boolean_condition",
                        "MISSING is not a condition".to_string(),
                    );
                    Judgement::rejected()
                }
                _ => judgement,
            },
            Expected::TypeExpression => match &judgement.outcome {
                Static::TypeDesignator(_) | Static::Identity(_) => judgement,
                _ => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        span,
                        "type_expression",
                        "this slot requires a source type expression".to_string(),
                    );
                    Judgement::rejected()
                }
            },
            Expected::None
            | Expected::Identity
            | Expected::Identifier(_)
            | Expected::Quantifier => judgement,
        }
    }

    fn judge(&mut self, source: &SourceId, expr: &Expr, expected: &Expected) -> Judgement {
        if let Some(ready) = self.ready_judge.remove(&(expr as *const Expr)) {
            return ready;
        }
        match expr {
            Expr::Literal(literal) => self.literal(source, literal, expected),
            Expr::Identifier(ident) => self.identifier(source, ident, expected),
            Expr::Collection(collection) => self.collection(source, collection, expected),
            // "A receiving identity context applies to a direct REF expression,
            // including parentheses around it."
            Expr::Group(group) => self.judge(source, &group.inner, expected),
            Expr::Call(call) => self.call(source, call, expected),
            Expr::Unary(unary) => self.unary(source, unary),
            Expr::Binary(binary) => self.binary(source, binary),
            Expr::Property(property) => self.property(source, property),
            Expr::Index(index) => self.index(source, index),
            Expr::Type(_) => match self.catalog.resolve(source, expr) {
                Ok(ty) => Judgement::plain(Static::TypeDesignator(ty)),
                Err(defect) => {
                    self.type_defect(source, expr.span(), defect);
                    Judgement::rejected()
                }
            },
        }
    }

    /// Report a type-expression defect at its canonical stage.
    pub(crate) fn type_defect(&mut self, source: &SourceId, span: Span, defect: TypeDefect) {
        match defect {
            // "cycles use error.reference.cycle" — registered at stage
            // resolution, reported with that classification.
            TypeDefect::Cycle => self.earlier_defect(
                source,
                span,
                "error.reference.cycle",
                "a type reference chain resolves to itself".to_string(),
            ),
            TypeDefect::Unconstrained(word) => self.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "unconstrained_type",
                format!("bare `{word}` does not declare a material value type"),
            ),
            TypeDefect::NotAnObject => self.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "object_type_argument",
                "`OBJECT[REF(id)]` requires a definition whose resolved BASE is an object schema"
                    .to_string(),
            ),
            TypeDefect::NotAType => self.emit(
                StaticError::TypeMismatch,
                source,
                span,
                "type_expression",
                "this is not a source type expression".to_string(),
            ),
            // An unresolvable type reference is an earlier stage's verdict and
            // is not re-decided or duplicated here.
            TypeDefect::Unresolvable => {}
        }
    }

    pub(crate) fn earlier_defect(
        &mut self,
        source: &SourceId,
        span: Span,
        identifier: &str,
        detail: String,
    ) {
        if self.silent {
            return;
        }
        if let Some(defect) = self.emitter.earlier_stage(source, span, identifier, detail) {
            if !self.earlier.iter().any(|existing| {
                existing.identifier == defect.identifier
                    && existing.source == defect.source
                    && existing.span == defect.span
            }) {
                self.earlier.push(defect);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Leaves
    // -----------------------------------------------------------------------

    fn literal(
        &mut self,
        _source: &SourceId,
        literal: &lcl_parser::syntax::Literal,
        expected: &Expected,
    ) -> Judgement {
        match literal.kind {
            LiteralKind::String | LiteralKind::MultilineString => {
                Judgement::known(Type::String, Const::Text(literal.text.clone()))
            }
            LiteralKind::Integer => match Decimal::parse_integer(&literal.text) {
                Some(value) => Judgement::known(Type::Integer, Const::Number(value)),
                None => Judgement::value(Type::Integer),
            },
            LiteralKind::Decimal => match Decimal::parse_decimal(&literal.text) {
                Some(value) => Judgement::known(Type::Decimal, Const::Number(value)),
                None => Judgement::value(Type::Decimal),
            },
            LiteralKind::True => Judgement::known(Type::Boolean, Const::Boolean(true)),
            LiteralKind::False => Judgement::known(Type::Boolean, Const::Boolean(false)),
            LiteralKind::Null => {
                // "NULL denotes the NULL type in a type-required field or type
                // argument and denotes the material NULL value elsewhere."
                if matches!(expected, Expected::TypeExpression) {
                    Judgement::plain(Static::TypeDesignator(Type::Null))
                } else {
                    Judgement::known(Type::Null, Const::Null)
                }
            }
            LiteralKind::Missing => Judgement::plain(Static::Missing),
            LiteralKind::Unknown => Judgement::plain(Static::Unknown),
        }
    }

    fn identifier(
        &mut self,
        source: &SourceId,
        ident: &lcl_parser::syntax::Ident,
        expected: &Expected,
    ) -> Judgement {
        match expected {
            // "Enum ITEM values resolve against the one concrete enum type
            // required by the containing typed value or operator operand …
            // Without one expected enum domain, an unqualified enum ITEM is not
            // resolved by guessing."
            Expected::Type(Type::Enum(domain)) => {
                if domain.contains(&ident.text) {
                    Judgement::known(
                        Type::Enum(domain.clone()),
                        Const::Identifier(ident.text.clone()),
                    )
                } else {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        ident.span,
                        "enum_member",
                        format!(
                            "`{}` is not a member of the enum domain `{}`",
                            ident.text, domain.id
                        ),
                    );
                    Judgement::rejected()
                }
            }
            Expected::Identifier(domain) if domain == "unit" => {
                if self.contracts.is_unit(&ident.text) {
                    Judgement::plain(Static::Identifier(ident.text.clone()))
                } else {
                    self.emit(
                        StaticError::OperatorOperand,
                        source,
                        ident.span,
                        "unit_identifier",
                        format!("`{}` is not a registered unit identifier", ident.text),
                    );
                    Judgement::rejected()
                }
            }
            // Every other qualified-identifier domain is a field value kind,
            // judged where value kinds are judged. This stage records the
            // registered name and claims nothing more.
            _ => Judgement {
                outcome: Static::Identifier(ident.text.clone()),
                value: Some(Const::Identifier(ident.text.clone())),
            },
        }
    }

    // -----------------------------------------------------------------------
    // Collections
    // -----------------------------------------------------------------------

    fn collection(
        &mut self,
        source: &SourceId,
        collection: &Collection,
        expected: &Expected,
    ) -> Judgement {
        // "Brackets denote LIST unless the receiving contract uniquely requires
        // SET[T]."
        let as_set = matches!(expected.ty(), Some(Type::Set(_)));
        // "The rule applies recursively to the members of a reference-typed
        // collection", so an identity slot's bracket members are identities too.
        let identities = matches!(expected, Expected::Identity);
        let member_expectation = member_expectation(expected);
        let quantifier = matches!(expected, Expected::Quantifier);

        let mut member_types: Vec<(Type, Span)> = Vec::new();
        let mut rejected = false;
        for member in &collection.members {
            // The immediate quantifier sequence "explicitly permits UNKNOWN
            // literals, including outside another condition", so its members
            // are not judged against a required material BOOLEAN destination.
            let expectation = if quantifier {
                Expected::Quantifier
            } else {
                member_expectation.clone()
            };
            let judgement = self.expression(source, member, &expectation);
            match &judgement.outcome {
                Static::Value(ty) => {
                    if quantifier && *ty != Type::Boolean {
                        self.emit(
                            StaticError::OperatorOperand,
                            source,
                            member.span(),
                            "quantifier_member",
                            format!("a quantifier sequence member is BOOLEAN, not {ty}"),
                        );
                        rejected = true;
                    } else {
                        member_types.push((ty.clone(), member.span()));
                    }
                }
                Static::Identity(ty) => member_types.push((ty.clone(), member.span())),
                // "MISSING and UNKNOWN are never material collection members",
                // with the immediate quantifier sequence as the sole exception.
                Static::Unknown if quantifier => {}
                Static::Missing if quantifier => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        member.span(),
                        "quantifier_member",
                        "MISSING is not a quantifier sequence member".to_string(),
                    );
                    rejected = true;
                }
                Static::Missing | Static::Unknown => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        member.span(),
                        "collection_member",
                        format!(
                            "{} is a non-material sentinel and cannot be a collection member",
                            judgement.outcome
                        ),
                    );
                    rejected = true;
                }
                Static::Rejected => rejected = true,
                // A declared field's own contract governs it; its membership is
                // judged where that contract is judged.
                Static::Opaque => {}
                Static::TypeDesignator(_) | Static::Identifier(_) => {
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        member.span(),
                        "collection_member",
                        "a collection member is a material value".to_string(),
                    );
                    rejected = true;
                }
            }
        }

        if quantifier {
            return if rejected {
                Judgement::rejected()
            } else {
                Judgement::value(Type::List(Box::new(Type::Boolean)))
            };
        }
        if rejected {
            return Judgement::rejected();
        }
        // A reference list carries one identity per member and has no material
        // member type to unify: which targets the slot admits is the resolution
        // stage's verdict, already given.
        if identities {
            return Judgement::value(Type::List(Box::new(Type::Reference(Box::new(
                RefTarget::Any,
            )))));
        }

        let member_type = match expected.member() {
            Some(ty) => ty.clone(),
            None => {
                let Some((first, _)) = member_types.first().cloned() else {
                    // "An empty bracket literal without one expected member type
                    // uses error.type.mismatch."
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        collection.span,
                        "empty_collection",
                        "an empty bracket literal needs one expected member type".to_string(),
                    );
                    return Judgement::rejected();
                };
                // "otherwise every member must have one identical static type;
                // no INTEGER-to-DECIMAL promotion occurs merely to make a
                // collection homogeneous."
                for (ty, span) in &member_types[1..] {
                    if *ty != first {
                        self.emit(
                            StaticError::TypeMismatch,
                            source,
                            *span,
                            "collection_member",
                            format!("this member is {ty}; earlier members are {first}"),
                        );
                        return Judgement::rejected();
                    }
                }
                first
            }
        };

        // Each member was already judged against the item type by
        // `expression`, which owns that identifier.

        let ty = if as_set {
            Type::Set(Box::new(member_type))
        } else {
            Type::List(Box::new(member_type))
        };
        Judgement::value(ty)
    }

    // -----------------------------------------------------------------------
    // Operators
    // -----------------------------------------------------------------------

    fn unary(&mut self, source: &SourceId, unary: &lcl_parser::syntax::Unary) -> Judgement {
        let name = match unary.operator {
            UnaryOp::Not => "NOT",
            UnaryOp::Negate => "unary -",
        };
        let operand = self.expression(source, &unary.operand, &Expected::None);
        let Some(row) = self.contracts.operator(name).cloned() else {
            self.unregistered(source, unary.span, name);
            return Judgement::rejected();
        };
        let arguments = vec![(operand, unary.operand.span())];
        let Some(result) = self.apply(source, &row, &arguments, unary.span) else {
            return Judgement::rejected();
        };
        let result = Judgement {
            value: fold_unary(unary.operator, &arguments[0].0),
            outcome: result.outcome,
        };
        // "DURATION is non-negative. Unary negation does not accept DURATION."
        if unary.operator == UnaryOp::Negate {
            if let Some(Type::Duration) = arguments[0].0.ty() {
                self.emit(
                    StaticError::OperatorOperand,
                    source,
                    unary.span,
                    "operand_family",
                    "unary `-` does not accept DURATION".to_string(),
                );
                return Judgement::rejected();
            }
        }
        result
    }

    fn binary(&mut self, source: &SourceId, binary: &lcl_parser::syntax::Binary) -> Judgement {
        let name = binary.operator.lexeme();
        // "AND skips its right operand only when the left result is FALSE" —
        // skipping is an evaluation rule; both operands are checked here.
        let left = self.expression(source, &binary.left, &Expected::None);
        let right = self.expression(source, &binary.right, &Expected::None);
        let Some(row) = self.contracts.operator(name).cloned() else {
            self.unregistered(source, binary.span, name);
            return Judgement::rejected();
        };
        let arguments = vec![(left, binary.left.span()), (right, binary.right.span())];
        if binary.operator == BinaryOp::Divide {
            return self.divide(source, binary, &row, &arguments, false);
        }
        match self.apply(source, &row, &arguments, binary.span) {
            Some(result) => Judgement {
                value: fold_binary(binary.operator, &arguments[0].0, &arguments[1].0),
                outcome: result.outcome,
            },
            None => Judgement::rejected(),
        }
    }

    /// `/`, with its exactness contract.
    fn divide(
        &mut self,
        source: &SourceId,
        binary: &lcl_parser::syntax::Binary,
        row: &OperatorRow,
        arguments: &[(Judgement, Span)],
        rounding_context: bool,
    ) -> Judgement {
        let Some(result) = self.apply(source, row, arguments, binary.span) else {
            return Judgement::rejected();
        };
        let (left, right) = (&arguments[0].0, &arguments[1].0);
        let (Some(numerator), Some(denominator)) = (
            left.value.as_ref().and_then(Const::number),
            right.value.as_ref().and_then(Const::number),
        ) else {
            // "A demanded, well-typed division" is the demanding layer's.
            self.defer(
                source,
                binary.span,
                DemandKind::DivisionValue,
                "an operand's value is not statically known".to_string(),
            );
            return result;
        };
        match Rational::of(numerator, denominator) {
            Ok(rational) => {
                if rounding_context {
                    // ROUND materializes the exact quotient itself.
                    return result;
                }
                match rational.to_terminating_decimal() {
                    Ok(value) => {
                        let carried = match result.ty() {
                            Some(Type::Measure(Some(unit))) => Const::Quantity(value, unit.clone()),
                            _ => Const::Number(value),
                        };
                        Judgement {
                            outcome: result.outcome,
                            value: Some(carried),
                        }
                    }
                    Err(DivisionDefect::NonTerminating) => {
                        self.emit(
                            StaticError::NumericNonTerminating,
                            source,
                            binary.span,
                            "exact_quotient",
                            "this exact quotient has no finite base-10 value outside the direct first argument of ROUND"
                                .to_string(),
                        );
                        Judgement::rejected()
                    }
                    Err(DivisionDefect::Zero) => {
                        self.division_by_zero(source, binary.span);
                        Judgement::rejected()
                    }
                    Err(DivisionDefect::TooLarge) => result,
                }
            }
            Err(DivisionDefect::Zero) => {
                self.division_by_zero(source, binary.span);
                Judgement::rejected()
            }
            Err(_) => result,
        }
    }

    fn division_by_zero(&mut self, source: &SourceId, span: Span) {
        self.emit(
            StaticError::NumericDivisionByZero,
            source,
            span,
            "zero_denominator",
            "a mathematical-zero denominator has no quotient, including inside ROUND".to_string(),
        );
    }

    fn unregistered(&mut self, source: &SourceId, span: Span, name: &str) {
        self.emit(
            StaticError::OperatorOperand,
            source,
            span,
            "unregistered_name",
            format!("`{name}` is not a registered operator, function or constructor"),
        );
    }

    // -----------------------------------------------------------------------
    // Postfix
    // -----------------------------------------------------------------------

    fn property(&mut self, source: &SourceId, property: &PropertyAccess) -> Judgement {
        // "A reserved uppercase property immediately following REF, allowing
        // parentheses around REF, reads the declaration's registered field
        // before reading its bound value."
        if property.reserved {
            return self.declaration_property(source, property);
        }
        let base = self.expression(source, &property.base, &Expected::None);
        match base.outcome {
            Static::Value(Type::Object(object)) => match object.field(&property.name) {
                Some(field) => Judgement::value(field.ty.clone()),
                None => {
                    // A closed schema rejects the name; a schema-free object
                    // yields MISSING for an absent key.
                    self.emit(
                        StaticError::OperatorOperand,
                        source,
                        property.name_span,
                        "object_property",
                        format!("`{}` is not a field of this object schema", property.name),
                    );
                    Judgement::rejected()
                }
            },
            Static::Rejected => Judgement::rejected(),
            Static::Unknown => Judgement::plain(Static::Unknown),
            Static::Missing => Judgement::plain(Static::Missing),
            other => {
                self.emit(
                    StaticError::OperatorOperand,
                    source,
                    property.span,
                    "property_family",
                    format!("property access needs an OBJECT value, not {other}"),
                );
                Judgement::rejected()
            }
        }
    }

    /// `REF(id).FIELD` — a registered declaration field, read before any value.
    fn declaration_property(&mut self, source: &SourceId, property: &PropertyAccess) -> Judgement {
        let mut base = property.base.as_ref();
        while let Expr::Group(group) = base {
            base = &group.inner;
        }
        let Expr::Call(call) = base else {
            self.emit(
                StaticError::OperatorOperand,
                source,
                property.span,
                "declaration_property",
                "a reserved uppercase property follows REF(identifier) only".to_string(),
            );
            return Judgement::rejected();
        };
        let Some(identifier) = call.reference_target() else {
            self.emit(
                StaticError::OperatorOperand,
                source,
                property.span,
                "declaration_property",
                "a reserved uppercase property follows REF(identifier) only".to_string(),
            );
            return Judgement::rejected();
        };
        self.record(
            source,
            base.span(),
            &Static::Identity(Type::Reference(Box::new(RefTarget::Any))),
        );
        let Some(index) = self.catalog.binding(source, identifier.span) else {
            return Judgement::rejected();
        };
        let Some(declaration) = self.resolved.declarations().get(index) else {
            return Judgement::rejected();
        };
        let block = declaration.block.clone();
        // "An unregistered declared field … uses error.operator.operand."
        if self.contracts.value_kind(&block, &property.name).is_none() {
            self.emit(
                StaticError::OperatorOperand,
                source,
                property.name_span,
                "declaration_property",
                format!("`{}` is not a registered field of {block}", property.name),
            );
            return Judgement::rejected();
        }
        // "declared_property_type: The selected … declaration field's registered
        // or inferred static type." The field's own expression carries it, so
        // it is resolved here rather than guessed.
        match self.declaration_field_type(index, &property.name) {
            Some(ty) => Judgement::value(ty),
            // A field whose own type this stage cannot infer is left to its own
            // contract rather than given an invented one.
            None => Judgement::plain(Static::Opaque),
        }
    }

    /// The static type of one declaration's registered field, from that field's
    /// own expression.
    ///
    /// Memoized and depth-bounded, so a field that reads another declaration's
    /// field terminates whatever the source does.
    fn declaration_field_type(&mut self, declaration: usize, field: &str) -> Option<Type> {
        let key = (declaration, field.to_string());
        if let Some(known) = self.field_types.get(&key) {
            return known.clone();
        }
        if self.property_depth >= 8 || !self.resolving.insert(key.clone()) {
            return None;
        }
        self.property_depth += 1;

        let resolved = (|| {
            let block = crate::types::declaration_block(self.resolved, declaration)?;
            let source = self
                .resolved
                .declarations()
                .get(declaration)?
                .source
                .clone();
            let expr = crate::types::inline_expression(&block.field(field)?.body)?;
            let silent = self.silent;
            self.silent = true;
            let judgement = self.judge(&source, expr, &Expected::None);
            self.silent = silent;
            judgement.ty().cloned()
        })();

        self.property_depth -= 1;
        self.resolving.remove(&key);
        self.field_types.insert(key, resolved.clone());
        resolved
    }

    fn index(&mut self, source: &SourceId, index: &lcl_parser::syntax::IndexAccess) -> Judgement {
        let base = self.expression(source, &index.base, &Expected::None);
        let subscript = self.expression(source, &index.index, &Expected::None);
        match (base.outcome.clone(), subscript.outcome.clone()) {
            (Static::Rejected, _) | (_, Static::Rejected) => Judgement::rejected(),
            // "LIST indexing is zero-based … SET and STRING indexing are not
            // admitted."
            (Static::Value(Type::List(member)), Static::Value(Type::Integer)) => {
                Judgement::value(*member)
            }
            (Static::Value(Type::Object(object)), Static::Value(Type::String)) => {
                // "A selected key must be statically known so the result has one
                // exact field type."
                let Some(key) = subscript.value.as_ref().and_then(Const::text) else {
                    self.emit(
                        StaticError::OperatorOperand,
                        source,
                        index.index.span(),
                        "index_key",
                        "an OBJECT index selects a statically known key".to_string(),
                    );
                    return Judgement::rejected();
                };
                match object.field(key) {
                    Some(field) => Judgement::value(field.ty.clone()),
                    None => {
                        self.emit(
                            StaticError::OperatorOperand,
                            source,
                            index.index.span(),
                            "index_key",
                            format!("`{key}` is not a field of this object schema"),
                        );
                        Judgement::rejected()
                    }
                }
            }
            (base_outcome, subscript_outcome) => {
                self.emit(
                    StaticError::OperatorOperand,
                    source,
                    index.span,
                    "index_family",
                    format!(
                        "no registered index overload accepts {base_outcome} indexed by {subscript_outcome}"
                    ),
                );
                Judgement::rejected()
            }
        }
    }

    // -----------------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------------

    fn call(&mut self, source: &SourceId, call: &Call, expected: &Expected) -> Judgement {
        if call.is_reference() {
            return self.reference(source, call, expected);
        }
        let name = call.callable.text.clone();
        if let Some(row) = self.contracts.constructor(&name).cloned() {
            return self.constructor(source, call, &row);
        }
        if let Some(row) = self.contracts.function(&name).cloned() {
            return self.function(source, call, &row);
        }
        self.unregistered(source, call.span, &name);
        Judgement::rejected()
    }

    fn function(&mut self, source: &SourceId, call: &Call, row: &FunctionRow) -> Judgement {
        // ALL, ANY and NONE "supply BOOLEAN context to an immediate bracket
        // argument", which is the one place an UNKNOWN literal may appear
        // outside a condition.
        let quantifier = row.argument_contract.as_deref() == Some("quantifier_argument");
        let rounding = row.name == "ROUND";

        let mut arguments = Vec::new();
        for (position, argument) in call.arguments.iter().enumerate() {
            let expectation = if quantifier {
                Expected::Quantifier
            } else {
                Expected::None
            };
            if rounding && position == 0 {
                let judgement = self.rounded_first_argument(source, argument);
                arguments.push((judgement, argument.span()));
                continue;
            }
            let judgement = self.expression(source, argument, &expectation);
            arguments.push((judgement, argument.span()));
        }

        if !row.arities().contains(&arguments.len()) {
            self.arity(source, call, &row.name, &row.arities());
            return Judgement::rejected();
        }
        let Some(result) =
            self.apply_overloads(source, &row.overloads, &arguments, call.span, &row.name)
        else {
            return Judgement::rejected();
        };

        // "SUM, MIN, and MAX require nonempty collections and reject a typed
        // empty collection with error.operator.operand" — an emptiness this
        // stage can only know for a literal.
        if row.minimum_count.is_some() {
            if let Some(Expr::Collection(collection)) = unwrap_group(call.arguments.first()) {
                if collection.members.is_empty() {
                    self.emit(
                        StaticError::OperatorOperand,
                        source,
                        call.span,
                        "empty_reduction",
                        format!("{} requires at least one member", row.name),
                    );
                    return Judgement::rejected();
                }
            } else {
                self.defer(
                    source,
                    call.span,
                    DemandKind::NonemptyReduction,
                    format!("{} requires a nonempty collection", row.name),
                );
            }
        }

        if rounding {
            return self.round_value(source, call, result);
        }
        Judgement {
            outcome: result.outcome,
            value: None,
        }
    }

    /// ROUND's first argument, whose direct division is materialized once.
    fn rounded_first_argument(&mut self, source: &SourceId, argument: &Expr) -> Judgement {
        let mut inner = argument;
        while let Expr::Group(group) = inner {
            inner = &group.inner;
        }
        let Expr::Binary(binary) = inner else {
            return self.expression(source, argument, &Expected::None);
        };
        if binary.operator != BinaryOp::Divide {
            return self.expression(source, argument, &Expected::None);
        }
        let left = self.expression(source, &binary.left, &Expected::None);
        let right = self.expression(source, &binary.right, &Expected::None);
        let Some(row) = self.contracts.operator("/").cloned() else {
            self.unregistered(source, binary.span, "/");
            return Judgement::rejected();
        };
        let arguments = vec![(left, binary.left.span()), (right, binary.right.span())];
        let judgement = self.divide(source, binary, &row, &arguments, true);
        self.record(source, argument.span(), &judgement.outcome);
        // Keep the operands' exact values so ROUND can materialize the quotient.
        Judgement {
            outcome: judgement.outcome,
            value: rational_of(&arguments[0].0, &arguments[1].0),
        }
    }

    /// The statically known value of a `ROUND` application, when there is one.
    fn round_value(&mut self, source: &SourceId, call: &Call, result: Judgement) -> Judgement {
        let outcome = result.outcome.clone();
        let plain = Judgement {
            outcome: outcome.clone(),
            value: None,
        };
        let Some(first) = call.arguments.first() else {
            return plain;
        };
        let Some(digits_expr) = call.arguments.get(1) else {
            return plain;
        };
        let digits = self
            .annotation_value(source, digits_expr)
            .and_then(|value| value.number().cloned())
            .and_then(|value| value.to_i64());
        let Some(digits) = digits else {
            self.defer(
                source,
                call.span,
                DemandKind::DeclaredBound,
                "ROUND digits are not statically known".to_string(),
            );
            return plain;
        };
        if digits < 0 {
            // "a non-negative INTEGER number of fractional digits"
            self.emit(
                StaticError::ValueOutOfRange,
                source,
                digits_expr.span(),
                "round_digits",
                "ROUND takes a non-negative number of fractional digits".to_string(),
            );
            return Judgement::rejected();
        }
        let Some(value) = self.rounding_operand(source, first) else {
            return plain;
        };
        let Ok(digits) = u32::try_from(digits) else {
            return plain;
        };
        match value.round_half_even(digits) {
            Ok(rounded) => Judgement {
                outcome,
                value: Some(match &result.outcome {
                    Static::Value(Type::Measure(Some(unit))) => {
                        Const::Quantity(rounded, unit.clone())
                    }
                    _ => Const::Number(rounded),
                }),
            },
            Err(DivisionDefect::Zero) => {
                self.division_by_zero(source, first.span());
                Judgement::rejected()
            }
            Err(_) => plain,
        }
    }

    /// ROUND's first argument as an exact rational, when it is statically known.
    fn rounding_operand(&mut self, source: &SourceId, argument: &Expr) -> Option<Rational> {
        let mut inner = argument;
        while let Expr::Group(group) = inner {
            inner = &group.inner;
        }
        if let Expr::Binary(binary) = inner {
            if binary.operator == BinaryOp::Divide {
                let left = self.annotation_value(source, &binary.left)?;
                let right = self.annotation_value(source, &binary.right)?;
                return Rational::of(left.number()?, right.number()?).ok();
            }
        }
        let value = self.annotation_value(source, argument)?;
        Rational::of(
            value.number()?,
            &Decimal::from_integer(crate::numeric::Integer::from_u64(1)),
        )
        .ok()
    }

    fn constructor(&mut self, source: &SourceId, call: &Call, row: &ConstructorRow) -> Judgement {
        let mut arguments = Vec::new();
        for (position, argument) in call.arguments.iter().enumerate() {
            let expectation = self.constructor_expectation(row, position);
            let judgement = self.expression(source, argument, &expectation);
            arguments.push((judgement, argument.span()));
        }
        if !row.arities().contains(&arguments.len()) {
            self.arity(source, call, &row.name, &row.arities());
            return Judgement::rejected();
        }
        let Some(result) =
            self.apply_overloads(source, &row.overloads, &arguments, call.span, &row.name)
        else {
            return Judgement::rejected();
        };
        self.constructed_value(source, call, row, &arguments, result)
    }

    /// The receiving contract of one constructor argument position.
    fn constructor_expectation(&self, row: &ConstructorRow, position: usize) -> Expected {
        for overload in &row.overloads {
            if let Some(operand) = overload.parameters.get(position) {
                if operand
                    .alternatives()
                    .iter()
                    .any(|d| matches!(d, Designator::UnitIdentifier))
                {
                    return Expected::Identifier("unit".to_string());
                }
            }
        }
        Expected::None
    }

    fn arity(
        &mut self,
        source: &SourceId,
        call: &Call,
        name: &str,
        arities: &std::collections::BTreeSet<usize>,
    ) {
        let admitted: Vec<String> = arities.iter().map(usize::to_string).collect();
        self.emit(
            StaticError::OperatorOperand,
            source,
            call.span,
            "arity",
            format!(
                "`{name}` admits {} argument(s); found {}",
                admitted.join(" or "),
                call.arguments.len()
            ),
        );
    }

    /// The statically known value recorded at one exact locus, if any.
    pub(crate) fn value_at(&self, source: &SourceId, span: Span) -> Option<Const> {
        self.values.get(&(source.clone(), span)).cloned()
    }

    /// The statically known value already recorded at one locus.
    fn annotation_value(&mut self, source: &SourceId, expr: &Expr) -> Option<Const> {
        self.values.get(&(source.clone(), expr.span())).cloned()
    }

    // -----------------------------------------------------------------------
    // Overload selection
    // -----------------------------------------------------------------------

    /// Apply one operator row to its arguments.
    fn apply(
        &mut self,
        source: &SourceId,
        row: &OperatorRow,
        arguments: &[(Judgement, Span)],
        span: Span,
    ) -> Option<Judgement> {
        if arguments.len() != row.arity {
            self.emit(
                StaticError::OperatorOperand,
                source,
                span,
                "arity",
                format!(
                    "`{}` takes {} operand(s); found {}",
                    row.name,
                    row.arity,
                    arguments.len()
                ),
            );
            return None;
        }
        self.apply_overloads(source, &row.overloads, arguments, span, &row.name)
    }

    /// Select the first registered overload that admits these operands.
    ///
    /// "An unregistered name, arity, or operand family is invalid;
    /// implementations do not infer extra overloads."
    fn apply_overloads(
        &mut self,
        source: &SourceId,
        overloads: &[Overload],
        arguments: &[(Judgement, Span)],
        span: Span,
        name: &str,
    ) -> Option<Judgement> {
        // An operand that already failed carries no type, so no overload can be
        // judged and no second diagnostic is invented for the same defect.
        if arguments
            .iter()
            .any(|(judgement, _)| judgement.is_rejected())
        {
            return None;
        }

        for overload in overloads {
            if overload.parameters.len() != arguments.len() {
                continue;
            }
            let mut bindings: BTreeMap<char, Type> = BTreeMap::new();
            let matched =
                overload
                    .parameters
                    .iter()
                    .zip(arguments)
                    .all(|(operand, (judgement, _))| {
                        self.operand_admits(operand, judgement, &mut bindings)
                    });
            if !matched {
                continue;
            }
            return self.overload_result(source, overload, arguments, span, name, &bindings);
        }

        // No overload matched. A sentinel operand is a placement defect with its
        // own identifier; anything else is an unregistered operand family.
        for (judgement, argument_span) in arguments {
            match judgement.outcome {
                Static::Unknown => {
                    // "A required material receiving site rejects UNKNOWN with
                    // error.value.unknown."
                    self.emit(
                        StaticError::ValueUnknown,
                        source,
                        *argument_span,
                        "sentinel_placement",
                        format!("`{name}` has no registered overload that admits UNKNOWN here"),
                    );
                    return None;
                }
                Static::Missing => {
                    // MISSING "has no storable user type"; only ==, != and
                    // EXISTS admit it, and those admit it through their own
                    // registered designators.
                    self.emit(
                        StaticError::TypeMismatch,
                        source,
                        *argument_span,
                        "sentinel_placement",
                        format!("`{name}` has no registered overload that admits MISSING here"),
                    );
                    return None;
                }
                _ => {}
            }
        }
        let families: Vec<String> = arguments
            .iter()
            .map(|(judgement, _)| judgement.outcome.to_string())
            .collect();
        self.emit(
            StaticError::OperatorOperand,
            source,
            span,
            "operand_family",
            format!(
                "no registered `{name}` overload accepts ({})",
                families.join(", ")
            ),
        );
        None
    }

    /// True when one operand position admits this judgement.
    fn operand_admits(
        &self,
        operand: &Operand,
        judgement: &Judgement,
        bindings: &mut BTreeMap<char, Type>,
    ) -> bool {
        operand
            .alternatives()
            .iter()
            .any(|designator| self.designator_admits(designator, judgement, bindings))
    }

    fn designator_admits(
        &self,
        designator: &Designator,
        judgement: &Judgement,
        bindings: &mut BTreeMap<char, Type>,
    ) -> bool {
        // The two whole-application designators admit sentinels explicitly.
        match designator {
            Designator::EqualityCompatible => {
                return matches!(
                    judgement.outcome,
                    Static::Value(_) | Static::Identity(_) | Static::Missing | Static::Unknown
                )
            }
            Designator::AnyExpressionOrReference => {
                return !matches!(
                    judgement.outcome,
                    Static::Rejected | Static::TypeDesignator(_)
                )
            }
            Designator::Unknown => return matches!(judgement.outcome, Static::Unknown),
            Designator::Missing => return matches!(judgement.outcome, Static::Missing),
            Designator::UnitIdentifier => {
                return match &judgement.outcome {
                    Static::Identifier(id) => self.contracts.is_unit(id),
                    _ => false,
                }
            }
            Designator::WorkspaceReference => {
                return matches!(judgement.outcome, Static::Identity(_))
            }
            Designator::AnyReference => {
                return matches!(
                    judgement.outcome,
                    Static::Identity(_) | Static::Value(Type::Reference(_))
                )
            }
            _ => {}
        }

        let Some(ty) = judgement.ty() else {
            return false;
        };
        if matches!(judgement.outcome, Static::TypeDesignator(_)) {
            return false;
        }
        self.type_admits(designator, ty, bindings)
    }

    fn type_admits(
        &self,
        designator: &Designator,
        ty: &Type,
        bindings: &mut BTreeMap<char, Type>,
    ) -> bool {
        match designator {
            Designator::Exact(expected) => expected.accepts(ty),
            Designator::AnyList => matches!(ty, Type::List(_)),
            Designator::AnySet => matches!(ty, Type::Set(_)),
            Designator::AnyObject => matches!(ty, Type::Object(_)),
            Designator::List(inner) => match ty {
                Type::List(member) => self.type_admits(inner, member, bindings),
                _ => false,
            },
            Designator::Set(inner) => match ty {
                Type::Set(member) => self.type_admits(inner, member, bindings),
                _ => false,
            },
            // "T binds one identical static type throughout one signature
            // application."
            Designator::Variable(name) => match bindings.get(name) {
                Some(bound) => bound.accepts(ty) || ty.accepts(bound),
                None => {
                    bindings.insert(*name, ty.clone());
                    true
                }
            },
            Designator::Numeric => ty.is_numeric(),
            Designator::SameUnitMeasure => matches!(ty, Type::Measure(_)),
            Designator::SameUnitMeasureCollection => matches!(ty.member(), Some(Type::Measure(_))),
            // "Material values mutually order-compatible under ordered_types
            // and ordered_type_rules".
            Designator::Ordered | Designator::OrderCompatible => {
                self.contracts.is_ordered_family(ty.family())
            }
            Designator::NonemptyOrderedCollection => ty
                .member()
                .is_some_and(|member| self.contracts.is_ordered_family(member.family())),
            Designator::BooleanSequence => matches!(ty.member(), Some(Type::Boolean)),
            Designator::AnyReference => matches!(ty, Type::Reference(_)),
            Designator::Unknown
            | Designator::Missing
            | Designator::EqualityCompatible
            | Designator::AnyExpressionOrReference
            | Designator::UnitIdentifier
            | Designator::WorkspaceReference
            | Designator::PropertyName => false,
        }
    }

    /// The result of one matched overload, with its cross-operand constraints.
    fn overload_result(
        &mut self,
        source: &SourceId,
        overload: &Overload,
        arguments: &[(Judgement, Span)],
        span: Span,
        name: &str,
        bindings: &BTreeMap<char, Type>,
    ) -> Option<Judgement> {
        // "Every arithmetic, ordered-comparison, SUM, MIN, or MAX overload that
        // requires identical MEASURE units emits error.numeric.unit_mismatch for
        // unequal concrete unit identifiers."
        if self.requires_same_unit(overload) && !self.same_units(source, arguments, span, name)? {
            return None;
        }
        // "order_compatible: Two ordered values of the same static type, or an
        // INTEGER/DECIMAL pair."
        if overload.parameters.iter().any(|operand| {
            operand
                .alternatives()
                .contains(&Designator::OrderCompatible)
        }) && !self.order_compatible(source, arguments, span, name)
        {
            return None;
        }

        let ty = self.result_type(&overload.result.spec, overload, arguments, bindings);
        match ty {
            Some(ty) => Some(Judgement::value(ty)),
            None => {
                self.emit(
                    StaticError::OperatorOperand,
                    source,
                    span,
                    "result_family",
                    format!("`{name}` has no registered result family for these operands"),
                );
                None
            }
        }
    }

    fn requires_same_unit(&self, overload: &Overload) -> bool {
        overload.constraint.as_deref() == Some("same_exact_unit")
            || overload.parameters.iter().any(|operand| {
                operand.alternatives().iter().any(|designator| {
                    matches!(
                        designator,
                        Designator::SameUnitMeasure | Designator::SameUnitMeasureCollection
                    )
                })
            })
    }

    /// Every `MEASURE` operand carries the same exact unit, or the check is the
    /// demanding layer's because a unit is not statically known.
    fn same_units(
        &mut self,
        source: &SourceId,
        arguments: &[(Judgement, Span)],
        span: Span,
        name: &str,
    ) -> Option<bool> {
        let mut known: Option<(UnitId, Span)> = None;
        let mut unknown = false;
        for (judgement, argument_span) in arguments {
            let unit = match judgement.ty() {
                Some(Type::Measure(unit)) => unit.clone(),
                Some(other) => match other.member() {
                    Some(Type::Measure(unit)) => unit.clone(),
                    _ => continue,
                },
                None => continue,
            };
            let Some(unit) = unit else {
                unknown = true;
                continue;
            };
            match &known {
                None => known = Some((unit, *argument_span)),
                Some((first, _)) if *first == unit => {}
                Some((first, _)) => {
                    self.emit(
                        StaticError::NumericUnitMismatch,
                        source,
                        *argument_span,
                        "measure_unit",
                        format!("`{name}` requires one exact unit; found {first} and {unit}"),
                    );
                    return Some(false);
                }
            }
        }
        if unknown {
            self.defer(
                source,
                span,
                DemandKind::MeasureUnit,
                format!("`{name}` requires one exact MEASURE unit"),
            );
        }
        Some(true)
    }

    fn order_compatible(
        &mut self,
        source: &SourceId,
        arguments: &[(Judgement, Span)],
        span: Span,
        name: &str,
    ) -> bool {
        let types: Vec<&Type> = arguments
            .iter()
            .filter_map(|(judgement, _)| judgement.ty())
            .collect();
        let (Some(left), Some(right)) = (types.first(), types.get(1)) else {
            return true;
        };
        let numeric_pair = left.is_numeric() && right.is_numeric();
        if numeric_pair || left.accepts(right) {
            return true;
        }
        self.emit(
            StaticError::OperatorOperand,
            source,
            span,
            "order_compatible",
            format!("`{name}` compares two values of one ordered type; found {left} and {right}"),
        );
        false
    }

    /// The registered result designator, resolved against these operands.
    fn result_type(
        &self,
        spec: &ResultSpec,
        overload: &Overload,
        arguments: &[(Judgement, Span)],
        bindings: &BTreeMap<char, Type>,
    ) -> Option<Type> {
        let first = arguments.first().and_then(|(j, _)| j.ty());
        let second = arguments.get(1).and_then(|(j, _)| j.ty());
        match spec {
            ResultSpec::Exact(ty) => Some(match (ty, &overload.unit) {
                // `MEASURE / INTEGER` keeps the numerator's exact unit.
                (Type::Measure(_), Some(rule)) if rule == "preserve_left_exact_unit" => {
                    first.cloned().unwrap_or(Type::Measure(None))
                }
                (Type::Measure(_), Some(rule)) if rule == "preserve_exact_unit" => {
                    first.cloned().unwrap_or(Type::Measure(None))
                }
                _ => ty.clone(),
            }),
            ResultSpec::SameNumericFamily | ResultSpec::SameFamilyNonnegative => first.cloned(),
            ResultSpec::PromotedFamily | ResultSpec::PromotedNumericOrMeasure => {
                self.promote(first?, second?)
            }
            ResultSpec::PromotedMemberFamily => {
                let member = first?.member()?;
                Some(member.clone())
            }
            ResultSpec::MemberType => match first? {
                Type::List(member) | Type::Set(member) => Some(member.as_ref().clone()),
                _ => bindings.get(&'T').cloned(),
            },
            // Property and index access compute their own result from the
            // selected field, so the designator is never resolved here.
            ResultSpec::DeclaredPropertyType => None,
        }
    }

    /// `#/numeric_promotion`, plus the same-family rules for the non-scalar
    /// numeric families.
    fn promote(&self, left: &Type, right: &Type) -> Option<Type> {
        if left.is_numeric() && right.is_numeric() {
            let promoted = self
                .contracts
                .numeric_promotion(left.family(), right.family())?;
            return Type::scalar(promoted);
        }
        match (left, right) {
            (Type::Duration, Type::Duration) => Some(Type::Duration),
            // "same-unit MEASURE with scalar promotion of its numeric
            // components" — the unit is the one both operands carry.
            (Type::Measure(a), Type::Measure(b)) => {
                Some(Type::Measure(a.clone().or_else(|| b.clone())))
            }
            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // References
    // -----------------------------------------------------------------------

    /// `REF(identifier)` under `#/reference_context_contract`.
    fn reference(&mut self, source: &SourceId, call: &Call, expected: &Expected) -> Judgement {
        let Some(identifier) = call.reference_target() else {
            // `REFERENCE_CALL = "REF", "(", IDENTIFIER, ")"` — the grammar stage
            // owns any other shape.
            return Judgement::rejected();
        };
        let Some(binding) = self
            .resolved
            .bindings()
            .iter()
            .find(|b| b.source == *source && b.span == identifier.span)
        else {
            return Judgement::rejected();
        };

        let identity_context = matches!(expected, Expected::Identity | Expected::TypeExpression)
            || matches!(expected.ty(), Some(Type::Reference(_)));

        match &binding.target {
            // "Loop-local identifiers exist only in their FOR EACH body."
            BindingTarget::LoopLocal { .. } => {
                match self
                    .locals
                    .iter()
                    .rev()
                    .find(|(name, _)| *name == identifier.text)
                {
                    Some((_, ty)) => {
                        let ty = ty.clone();
                        if identity_context {
                            Judgement::plain(Static::Identity(Type::Reference(Box::new(
                                RefTarget::Value(Box::new(ty)),
                            ))))
                        } else {
                            Judgement::value(ty)
                        }
                    }
                    None => Judgement::rejected(),
                }
            }
            BindingTarget::Declaration(index) => {
                let index = *index;
                let Some(declaration) = self.resolved.declarations().get(index) else {
                    return Judgement::rejected();
                };
                let block = declaration.block.clone();
                let kind = declaration.definition_kind.clone();
                let id = declaration.id.clone();

                if identity_context {
                    let target = match self.catalog.definition(index) {
                        Some(crate::types::Definition::Type(ty)) => {
                            RefTarget::Value(Box::new(ty.clone()))
                        }
                        _ => RefTarget::Declaration(id),
                    };
                    return Judgement::plain(Static::Identity(Type::Reference(Box::new(target))));
                }

                // "Every other occurrence is a value context. REF reads exactly
                // one bound value of INPUT, DATA, CONTEXT, MEMORY, STATE,
                // OUTPUT, DEFINE kind.constant, or a loop-local binding. REF to
                // VALIDATE or VERIFY reads its Boolean check result. Other
                // declarations remain reference identities."
                match block.as_str() {
                    "INPUT" | "DATA" | "CONTEXT" | "MEMORY" | "STATE" | "OUTPUT" => {
                        match self.declaration_types.get(&index).cloned() {
                            Some(ty) => Judgement::value(ty),
                            // The declaration's own TYPE field failed to
                            // resolve; its diagnostic is recorded there.
                            None => Judgement::rejected(),
                        }
                    }
                    "VALIDATE" | "VERIFY" => Judgement::value(Type::Boolean),
                    "DEFINE" if kind.as_deref() == Some("kind.constant") => {
                        match self.declaration_types.get(&index).cloned() {
                            Some(ty) => Judgement {
                                outcome: Static::Value(ty),
                                value: self.constants.get(&index).cloned(),
                            },
                            None => Judgement::rejected(),
                        }
                    }
                    _ => Judgement::plain(Static::Identity(Type::Reference(Box::new(
                        RefTarget::Declaration(id),
                    )))),
                }
            }
            BindingTarget::Unresolved => Judgement::rejected(),
        }
    }

    // -----------------------------------------------------------------------
    // Constructors
    // -----------------------------------------------------------------------

    /// The constructed value's declared value-domain constraints.
    ///
    /// M1 owns every closed *literal profile* — REGEX, GLOB, DATE, TIME,
    /// DATETIME and URI text is already validated at the lexical stage. What is
    /// left is the numeric and unit domains the registry states as row fields,
    /// and the `PATH` form legality M1 explicitly deferred because it "depends
    /// on the receiving field and on resolution".
    fn constructed_value(
        &mut self,
        source: &SourceId,
        call: &Call,
        row: &ConstructorRow,
        arguments: &[(Judgement, Span)],
        result: Judgement,
    ) -> Judgement {
        let first = arguments.first();
        let known = first.and_then(|(judgement, _)| judgement.value.clone());

        // A unit argument narrowed by category, e.g. DURATION's Time units.
        if let Some(category) = &row.unit_category {
            if let Some((judgement, span)) = arguments.get(1) {
                if let Static::Identifier(unit) = &judgement.outcome {
                    if !self.contracts.unit_in_category(unit, category) {
                        self.emit(
                            StaticError::NumericUnitMismatch,
                            source,
                            *span,
                            "unit_category",
                            format!(
                                "`{}` requires a {category}-category unit; `{unit}` is not one",
                                row.name
                            ),
                        );
                        return Judgement::rejected();
                    }
                }
            }
        }

        // An inclusive declared bound over a statically known number.
        if row.minimum.is_some() || row.maximum.is_some() {
            match known.as_ref().and_then(Const::number) {
                Some(value) => {
                    if let Some(minimum) = row.minimum {
                        if value.compare(&Decimal::from_integer(crate::numeric::Integer::from_u64(
                            minimum.unsigned_abs(),
                        ))) == std::cmp::Ordering::Less
                            || (minimum == 0 && value.is_negative())
                        {
                            self.out_of_range(source, call.span, &row.name, "minimum", minimum);
                            return Judgement::rejected();
                        }
                    }
                    if let Some(maximum) = row.maximum {
                        if maximum >= 0
                            && value.compare(&Decimal::from_integer(
                                crate::numeric::Integer::from_u64(maximum.unsigned_abs()),
                            )) == std::cmp::Ordering::Greater
                        {
                            self.out_of_range(source, call.span, &row.name, "maximum", maximum);
                            return Judgement::rejected();
                        }
                    }
                }
                None => self.defer(
                    source,
                    call.span,
                    DemandKind::DeclaredBound,
                    format!("`{}` bounds its constructed value", row.name),
                ),
            }
        }

        // BYTES "accepts only a non-negative INTEGER".
        if row.name == "BYTES" {
            if let Some(Type::Decimal) = first.and_then(|(judgement, _)| judgement.ty()) {
                self.emit(
                    StaticError::OperatorOperand,
                    source,
                    call.span,
                    "operand_family",
                    "BYTES accepts a non-negative INTEGER".to_string(),
                );
                return Judgement::rejected();
            }
        }

        match row.name.as_str() {
            "MEASURE" | "DURATION" => {
                let unit = arguments
                    .get(1)
                    .and_then(|(judgement, _)| match &judgement.outcome {
                        Static::Identifier(id) => Some(UnitId(id.clone())),
                        _ => None,
                    });
                let value = known.as_ref().and_then(Const::number).cloned();
                let ty = match (&row.result, &unit) {
                    (Type::Measure(_), Some(unit)) => Type::Measure(Some(unit.clone())),
                    _ => row.result.clone(),
                };
                Judgement {
                    outcome: Static::Value(ty),
                    value: match (value, unit) {
                        (Some(value), Some(unit)) => Some(Const::Quantity(value, unit)),
                        _ => None,
                    },
                }
            }
            "REGEX" | "GLOB" => {
                let kind = if row.name == "REGEX" {
                    PatternKind::Regex
                } else {
                    PatternKind::Glob
                };
                let flags = arguments
                    .get(1)
                    .and_then(|(judgement, _)| judgement.value.as_ref())
                    .and_then(Const::text)
                    .unwrap_or_default()
                    .to_string();
                match known.as_ref().and_then(Const::text) {
                    Some(text) => Judgement {
                        outcome: result.outcome,
                        value: Some(Const::Pattern {
                            kind,
                            pattern: text.to_string(),
                            flags,
                        }),
                    },
                    None => {
                        self.defer(
                            source,
                            call.span,
                            DemandKind::ConstructorValue,
                            format!("`{}` compiles a pattern from its argument", row.name),
                        );
                        result
                    }
                }
            }
            "PATH" => self.path_value(source, call, arguments, result),
            _ => {
                if known.is_none() {
                    self.defer(
                        source,
                        call.span,
                        DemandKind::ConstructorValue,
                        format!("`{}` validates its material argument", row.name),
                    );
                }
                Judgement {
                    outcome: result.outcome,
                    value: known,
                }
            }
        }
    }

    fn out_of_range(&mut self, source: &SourceId, span: Span, name: &str, bound: &str, value: i64) {
        self.emit(
            StaticError::ValueOutOfRange,
            source,
            span,
            "declared_bound",
            format!("`{name}` declares an inclusive {bound} of {value}"),
        );
    }

    /// `PATH`'s form legality.
    ///
    /// "A one-STRING relative PATH is legal only as IMPORT.SOURCE or
    /// EXTENSION.SOURCE and resolves from the importing document; otherwise
    /// that form requires an absolute path."
    fn path_value(
        &mut self,
        source: &SourceId,
        call: &Call,
        arguments: &[(Judgement, Span)],
        result: Judgement,
    ) -> Judgement {
        // The two-argument form is the WORKSPACE form, whose relative string is
        // required; containment is checked on the resolved target, which is not
        // a static question.
        if arguments.len() > 1 {
            return result;
        }
        let Some(text) = arguments
            .first()
            .and_then(|(judgement, _)| judgement.value.as_ref())
            .and_then(Const::text)
        else {
            self.defer(
                source,
                call.span,
                DemandKind::ConstructorValue,
                "`PATH` requires an absolute form outside IMPORT.SOURCE and EXTENSION.SOURCE"
                    .to_string(),
            );
            return result;
        };
        if text.starts_with('/') || self.relative_path_allowed {
            return result;
        }
        self.earlier_defect(
            source,
            call.span,
            "error.literal.invalid",
            "a one-STRING relative PATH is legal only as IMPORT.SOURCE or EXTENSION.SOURCE"
                .to_string(),
        );
        Judgement::rejected()
    }
}

/// The exact value of a unary operator over a statically known operand.
///
/// Only the exactly specified arithmetic is folded. Nothing here decides a
/// language rule: it makes the value available so the rules that need one — a
/// declared bound, a zero denominator — can be applied at this stage instead of
/// being deferred unnecessarily.
fn fold_unary(operator: UnaryOp, operand: &Judgement) -> Option<Const> {
    match (operator, operand.value.as_ref()?) {
        (UnaryOp::Negate, Const::Number(value)) => Some(Const::Number(value.negated())),
        (UnaryOp::Negate, Const::Quantity(value, unit)) => {
            Some(Const::Quantity(value.negated(), unit.clone()))
        }
        (UnaryOp::Not, Const::Boolean(value)) => Some(Const::Boolean(!value)),
        _ => None,
    }
}

/// The exact value of an arithmetic operator over two statically known
/// operands. `INTEGER` and `DECIMAL` "never wrap, saturate, or overflow", so
/// the arithmetic is the exact one.
fn fold_binary(operator: BinaryOp, left: &Judgement, right: &Judgement) -> Option<Const> {
    let (left, right) = (left.value.as_ref()?, right.value.as_ref()?);
    let apply = |a: &Decimal, b: &Decimal| match operator {
        BinaryOp::Add => Some(a.add(b)),
        BinaryOp::Subtract => Some(a.sub(b)),
        BinaryOp::Multiply => Some(a.mul(b)),
        _ => None,
    };
    match (left, right) {
        (Const::Number(a), Const::Number(b)) => apply(a, b).map(Const::Number),
        // A same-unit MEASURE keeps that exact unit; a different one is a unit
        // mismatch the overload already reported.
        (Const::Quantity(a, unit), Const::Quantity(b, other)) if unit == other => {
            apply(a, b).map(|value| Const::Quantity(value, unit.clone()))
        }
        (Const::Quantity(a, unit), Const::Number(b)) if operator == BinaryOp::Multiply => {
            apply(a, b).map(|value| Const::Quantity(value, unit.clone()))
        }
        _ => None,
    }
}

/// The exact quotient of two statically known operands, when both are known.
fn rational_of(left: &Judgement, right: &Judgement) -> Option<Const> {
    let numerator = left.value.as_ref().and_then(Const::number)?;
    let denominator = right.value.as_ref().and_then(Const::number)?;
    Rational::of(numerator, denominator)
        .ok()
        .and_then(|r| r.to_terminating_decimal().ok())
        .map(Const::Number)
}

fn unwrap_group(expr: Option<&Expr>) -> Option<&Expr> {
    let mut current = expr?;
    while let Expr::Group(group) = current {
        current = &group.inner;
    }
    Some(current)
}

/// True when an expression is a division, looking through parentheses.
///
/// `rounded_first_argument` unwraps groups the same way before deciding
/// whether ROUND's first argument is the direct quotient the row materializes.
fn is_direct_division(expr: &Expr) -> bool {
    let mut inner = expr;
    while let Expr::Group(group) = inner {
        inner = &group.inner;
    }
    matches!(inner, Expr::Binary(binary) if binary.operator == BinaryOp::Divide)
}
