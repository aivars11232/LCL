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

use crate::contracts::{ConstructorRow, Designator, FunctionRow, Operand, OperatorRow, ResultSpec};
use crate::numeric::{Decimal, DivisionDefect, Rational};
use crate::pattern::{self, PatternKind};
use crate::schema::Schema;
use crate::ty::{EnumDomain, ObjectField, ObjectType, RefTarget, Type, UnitId};
use crate::types::{TypeCatalog, TypeDefect};
use crate::{
    Annotation, Cause, Contracts, DemandKind, DemandObligation, Diagnostic, EarlierStageDefect,
    Emitter, Static, StaticError,
};
use lcl_lexer::Span;
use lcl_parser::syntax::{
    BinaryOp, Call, Collection, Expr, LiteralKind, PropertyAccess, UnaryOp,
};
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
}

impl Expected {
    fn ty(&self) -> Option<&Type> {
        match self {
            Expected::Type(ty) => Some(ty),
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
    pub(crate) raw: Vec<Diagnostic>,
    pub(crate) annotations: BTreeMap<(SourceId, Span), Annotation>,
    /// Statically known values, by the exact locus that produced them.
    pub(crate) values: BTreeMap<(SourceId, Span), Const>,
    pub(crate) deferred: Vec<DemandObligation>,
    pub(crate) earlier: Vec<EarlierStageDefect>,
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
        self.deferred.push(DemandObligation {
            source: source.clone(),
            span,
            kind,
            detail,
        });
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
        let judgement = self.judge(source, expr, expected);
        self.record(source, expr.span(), &judgement.outcome);
        if let Some(value) = &judgement.value {
            self.values
                .insert((source.clone(), expr.span()), value.clone());
        }
        judgement
    }

    fn judge(&mut self, source: &SourceId, expr: &Expr, expected: &Expected) -> Judgement {
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
        if let Some(defect) = self
            .emitter
            .earlier_stage(source, span, identifier, detail)
        {
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
        source: &SourceId,
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
        let member_expectation = expected
            .member()
            .cloned()
            .map(Expected::Type)
            .unwrap_or(Expected::None);
        let quantifier = matches!(expected, Expected::Quantifier);

        let mut member_types: Vec<(Type, Span)> = Vec::new();
        let mut rejected = false;
        for member in &collection.members {
            let expectation = if quantifier {
                Expected::Type(Type::Boolean)
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
                            StaticError::CollectionHeterogeneous,
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

        // With an expected member type, each member was already judged against
        // it by `expression`; anything incompatible has been reported there.
        for (ty, span) in &member_types {
            if !member_type.accepts(ty) {
                self.emit(
                    StaticError::CollectionHeterogeneous,
                    source,
                    *span,
                    "collection_member",
                    format!("this member is {ty}, not the declared member type {member_type}"),
                );
                return Judgement::rejected();
            }
        }

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
        let arguments = vec![
            (left, binary.left.span()),
            (right, binary.right.span()),
        ];
        if binary.operator == BinaryOp::Divide {
            return self.divide(source, binary, &row, &arguments, false);
        }
        self.apply(source, &row, &arguments, binary.span)
            .unwrap_or_else(Judgement::rejected)
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
                    Ok(value) => Judgement {
                        outcome: result.outcome,
                        value: Some(match result.ty() {
                            Some(Type::Measure(Some(unit))) => {
                                Const::Quantity(value, unit.clone())
                            }
                            _ => Const::Number(value),
                        }),
                    },
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
                        format!(
                            "`{}` is not a field of this object schema",
                            property.name
                        ),
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
        self.record(source, base.span(), &Static::Identity(Type::Reference(Box::new(
            RefTarget::Any,
        ))));
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
        // The field's own contract decides its type; this stage records that the
        // read is legal and leaves the field value to its own contract.
        Judgement::plain(Static::Value(Type::Reference(Box::new(RefTarget::Any))))
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
        let Some(result) = self.apply_overloads(source, &row.overloads, &arguments, call.span, &row.name)
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
        let arguments = vec![
            (left, binary.left.span()),
            (right, binary.right.span()),
        ];
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

    /// The statically known value already recorded at one locus.
    fn annotation_value(&mut self, source: &SourceId, expr: &Expr) -> Option<Const> {
        self.values.get(&(source.clone(), expr.span())).cloned()
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
