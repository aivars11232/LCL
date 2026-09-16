//! The demand-driven evaluator.
//!
//! Authority: `05_SEMANTICS/12_OPERATOR_FUNCTION_AND_SPECIAL_VALUE_SEMANTICS.txt`
//! and `operators_and_functions_v0.1.0.json#/evaluation_contract`.
//!
//! ## Demand, not traversal
//!
//! `01_FOUNDATION/03`: "Value evaluation is separate and occurs only when the
//! containing reachable declaration demands the value under its operation,
//! condition, check, or completion contract. A skipped Boolean operand is not
//! demanded."
//!
//! So nothing here walks an expression because it exists. [`Evaluator::demand`]
//! is called at a demand point, and the only lazy forms — `AND` and `OR` —
//! genuinely do not evaluate the operand they skip. `CLOSURE-011` is exactly
//! that: `FALSE AND (1 / 0 == 0)` is `FALSE`, and the division never runs.
//!
//! ## Faults carry their resolved classification
//!
//! This is the first layer permitted to apply `expression_demand_resolution`,
//! and every fault it raises is by construction "actually demanded after the
//! document's preflight checks". So [`Fault`] asks the registry whether its
//! identifier is in the closed eligible map and records the answer; it never
//! decides eligibility itself. An ineligible identifier keeps its registered
//! stage, which is what `exclusion_rule` requires.
//!
//! ## Exactness
//!
//! Every number is `lcl_checker::numeric`'s exact base-10 value. There is no
//! float anywhere in this crate: "no overflow, wrap, saturation, Infinity, NaN,
//! or underflow-to-zero result exists in LCL", and the only way to guarantee
//! that is to never have one.

use crate::contracts::Contracts;
use crate::diagnostic::RuntimeError;
use crate::order_profile::{self, DURATION_UNIT};
use crate::pattern::{Flags, Glob, PatternFault, Regex};
use crate::state::{Bindings, IterationPath};
use crate::value::Value;
use lcl_checker::numeric::{Decimal, DivisionDefect, Integer, Rational};
use lcl_checker::ty::{Type, UnitId};
use lcl_checker::{Checked, Static};
use lcl_lexer::Span;
use lcl_parser::syntax::{
    BinaryOp, Call, Collection, Expr, Literal, LiteralKind, PropertyAccess, UnaryOp,
};
use lcl_resolver::{Resolved, SourceId};
use lcl_semantics::value::REGEX_FLAG_SEPARATOR;
use lcl_semantics::Plan;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// The evaluator's own budget, so a pathological document costs time, never
/// termination. Depth is counted rather than trusted to the stack.
const MAX_DEPTH: usize = 256;

/// One demand that produced a diagnostic instead of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub id: RuntimeError,
    pub span: Span,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: String,
    /// Non-normative human detail.
    pub detail: String,
    /// True when the identifier is in the closed `expression_demand_resolution`
    /// eligible map and therefore resolves to the execution stage here.
    pub demand_resolved: bool,
}

impl Fault {
    /// Raise one fault at a post-preflight demand.
    ///
    /// Whether the demand map applies is read from the registry, never decided
    /// at the call site.
    pub fn new(
        contracts: &Contracts,
        id: RuntimeError,
        span: Span,
        cause: impl Into<String>,
        detail: impl Into<String>,
    ) -> Fault {
        Fault {
            id,
            span,
            cause: cause.into(),
            detail: detail.into(),
            demand_resolved: contracts.demand().is_eligible(id),
        }
    }
}

/// The outcome of demanding one expression.
///
/// `Ok` carries a value, which may legitimately be `MISSING` or `UNKNOWN`:
/// those are values the language defines, not failures.
pub type Demand = Result<Value, Fault>;

/// One evaluation context.
pub struct Evaluator<'a> {
    pub contracts: &'a Contracts,
    pub resolved: &'a Resolved,
    pub checked: &'a Checked,
    pub plan: &'a Plan,
    pub bindings: &'a Bindings,
    /// The unit whose bytes the spans belong to.
    pub source: SourceId,
    /// The iteration context this demand happens in.
    pub iteration: IterationPath,
}

impl<'a> Evaluator<'a> {
    /// Demand the value of one expression.
    ///
    /// Total: returns for every input and never panics.
    pub fn demand(&self, expr: &Expr) -> Demand {
        self.eval(expr, 0)
    }

    fn eval(&self, expr: &Expr, depth: usize) -> Demand {
        if depth > MAX_DEPTH {
            return Err(Fault::new(
                self.contracts,
                RuntimeError::PatternResourceLimit,
                expr.span(),
                "expression depth",
                "the expression nests deeper than this runtime evaluates",
            ));
        }
        match expr {
            Expr::Literal(literal) => self.literal(literal),
            Expr::Group(group) => self.eval(&group.inner, depth + 1),
            Expr::Identifier(ident) => Ok(self.identifier(&ident.text)),
            Expr::Collection(collection) => self.collection(collection, depth),
            Expr::Unary(unary) => {
                let operand = self.eval(&unary.operand, depth + 1)?;
                self.unary(unary.operator, operand, unary.span)
            }
            Expr::Binary(binary) => self.binary(binary, depth),
            Expr::Call(call) => self.call(call, depth),
            Expr::Property(property) => self.property(property, depth),
            Expr::Index(index) => self.index(index, depth),
            // A type designator is not a material value; M4 admits it only
            // where a receiving slot requires a type, and no demand reads one.
            Expr::Type(ty) => Err(Fault::new(
                self.contracts,
                RuntimeError::OperatorOperand,
                ty.span(),
                "type designator demanded",
                "a type designator has no material value",
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Leaves
    // -----------------------------------------------------------------------

    fn literal(&self, literal: &Literal) -> Demand {
        Ok(match literal.kind {
            LiteralKind::String | LiteralKind::MultilineString => Value::Text(literal.text.clone()),
            LiteralKind::Integer => match Decimal::parse_integer(&literal.text) {
                Some(d) => Value::Integer(d),
                None => {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::LiteralInvalid,
                        literal.span,
                        "integer literal",
                        format!("{:?} is not an exact integer", literal.text),
                    ))
                }
            },
            LiteralKind::Decimal => match Decimal::parse_decimal(&literal.text) {
                Some(d) => Value::Decimal(d),
                None => {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::LiteralInvalid,
                        literal.span,
                        "decimal literal",
                        format!("{:?} is not an exact decimal", literal.text),
                    ))
                }
            },
            LiteralKind::True => Value::Boolean(true),
            LiteralKind::False => Value::Boolean(false),
            LiteralKind::Null => Value::Null,
            LiteralKind::Missing => Value::Missing,
            LiteralKind::Unknown => Value::Unknown,
        })
    }

    /// A bare identifier: a loop-local binding, or a registered qualified name.
    fn identifier(&self, text: &str) -> Value {
        if let Some(value) = self.bindings.local(text, &self.iteration) {
            return value.clone();
        }
        Value::Identifier(text.to_string())
    }

    /// The current value of one declaration.
    ///
    /// The order is the canonical reading order: a loop-local instance, then a
    /// resolved invocation datum, then a bound `OUTPUT`, then a scheduled check
    /// result. Anything else is absent, which is `MISSING`.
    pub fn declaration_value(&self, id: &str) -> Value {
        if let Some(value) = self.bindings.local(id, &self.iteration) {
            return value.clone();
        }
        // A written MEMORY or STATE store supersedes the declared value the
        // plan resolved. `05_SEMANTICS/07` calls MEMORY "retained data" and
        // STATE "mutable external/project data"; reading the declared value
        // after an authorized write would report the sample rather than the
        // state.
        if let Some(value) = self.bindings.store(id) {
            return value.clone();
        }
        if let Some(resolution) = self.plan.resolutions().iter().find(|r| r.id == id) {
            return resolution.value.clone();
        }
        if let Some(value) = self.bindings.output(id, &self.iteration) {
            return value.clone();
        }
        // "VALIDATE and VERIFY expose the Boolean result of their declared
        // check in value context." A skipped check has no result, and reading
        // an absent result is ordinary MISSING behaviour, "never implicit
        // TRUE".
        if let Some(check) = self.plan.checks().iter().find(|c| c.id == id) {
            return check.outcome.clone().unwrap_or(Value::Missing);
        }
        // "Within a valid instance an output not yet bound yields MISSING", and
        // reading it never starts its producer.
        Value::Missing
    }

    // -----------------------------------------------------------------------
    // Collections
    // -----------------------------------------------------------------------

    /// A bracket literal.
    ///
    /// `collection_expression/default_family`: "An ordinary bracket literal
    /// denotes LIST unless its immediate expected type is one exact SET[T]".
    /// M4 already resolved that expected type and annotated it, so the family
    /// is read from the annotation rather than re-derived.
    fn collection(&self, collection: &Collection, depth: usize) -> Demand {
        let mut members = Vec::new();
        // "Source order evaluates every SET source member before strict-equal
        // duplicates collapse."
        for member in &collection.members {
            let value = self.eval(member, depth + 1)?;
            match value {
                // "Every ordinary collection member is material. MISSING and
                // UNKNOWN cannot become material collection members."
                Value::Missing => {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::RequiredMissing,
                        member.span(),
                        "collection member",
                        "MISSING cannot become a material collection member",
                    ))
                }
                Value::Unknown => return Ok(Value::Unknown),
                other => members.push(other),
            }
        }
        Ok(lcl_semantics::value::collection(
            members,
            self.is_set(collection.span),
        ))
    }

    /// True when M4 annotated this expression as a `SET`.
    fn is_set(&self, span: Span) -> bool {
        matches!(
            self.checked
                .annotation(&self.source, span)
                .map(|a| &a.outcome),
            Some(Static::Value(Type::Set(_)))
        )
    }

    /// True when M4 annotated this expression as a retained reference identity.
    ///
    /// `types_v0.1.0.json#/reference_context_contract`: an identity context
    /// keeps the reference rather than reading through it.
    fn is_identity(&self, span: Span) -> bool {
        matches!(
            self.checked
                .annotation(&self.source, span)
                .map(|a| &a.outcome),
            Some(Static::Identity(_))
        )
    }

    // -----------------------------------------------------------------------
    // Operators
    // -----------------------------------------------------------------------

    fn unary(&self, operator: UnaryOp, operand: Value, span: Span) -> Demand {
        match operator {
            UnaryOp::Not => match operand {
                Value::Boolean(b) => Ok(Value::Boolean(!b)),
                // "NOT, AND, and OR use the complete strong Kleene truth
                // tables in unknown_logic."
                Value::Unknown => Ok(Value::Unknown),
                Value::Missing => Err(self.missing(span, "NOT operand")),
                other => Err(self.operand(
                    span,
                    format!("NOT requires BOOLEAN, found {}", other.family()),
                )),
            },
            UnaryOp::Negate => match &operand {
                Value::Integer(d) => Ok(Value::Integer(d.negated())),
                Value::Decimal(d) => Ok(Value::Decimal(d.negated())),
                Value::Quantity(d, unit) => Ok(Value::Quantity(d.negated(), unit.clone())),
                Value::Percentage(d) => Ok(Value::Percentage(d.negated())),
                Value::Unknown => Ok(Value::Unknown),
                Value::Missing => Err(self.missing(span, "negation operand")),
                other => Err(self.operand(
                    span,
                    format!("negation requires a number, found {}", other.family()),
                )),
            },
        }
    }

    fn binary(&self, binary: &lcl_parser::syntax::Binary, depth: usize) -> Demand {
        let span = binary.span;

        // The only lazy forms. "AND evaluates its right operand unless the left
        // result is FALSE; OR evaluates its right operand unless the left
        // result is TRUE. UNKNOWN on the left never skips the right."
        if matches!(binary.operator, BinaryOp::And | BinaryOp::Or) {
            let left = self.eval(&binary.left, depth + 1)?;
            match (binary.operator, &left) {
                // "An operand in a skipped Boolean branch is not consumed", so
                // the right operand is not evaluated and cannot fault.
                (BinaryOp::And, Value::Boolean(false)) => return Ok(Value::Boolean(false)),
                (BinaryOp::Or, Value::Boolean(true)) => return Ok(Value::Boolean(true)),
                _ => {}
            }
            if left == Value::Missing {
                return Err(self.missing(binary.left.span(), "Boolean operand"));
            }
            let right = self.eval(&binary.right, depth + 1)?;
            if right == Value::Missing {
                return Err(self.missing(binary.right.span(), "Boolean operand"));
            }
            return self.logic(binary.operator, &left, &right, span);
        }

        // "Evaluate operator operands ... in source order from left to right.
        // Stop evaluation at the first diagnostic."
        let left = self.eval(&binary.left, depth + 1)?;
        let right = self.eval(&binary.right, depth + 1)?;

        // "A consumed MISSING operand emits error.required.missing except for
        // ==, !=, and EXISTS."
        let equality = matches!(binary.operator, BinaryOp::Equal | BinaryOp::NotEqual);
        if !equality {
            if left == Value::Missing {
                return Err(self.missing(binary.left.span(), "left operand"));
            }
            if right == Value::Missing {
                return Err(self.missing(binary.right.span(), "right operand"));
            }
        }

        match binary.operator {
            BinaryOp::Equal => Ok(Value::Boolean(strict_equal(&left, &right))),
            BinaryOp::NotEqual => Ok(Value::Boolean(!strict_equal(&left, &right))),
            BinaryOp::Less
            | BinaryOp::LessOrEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterOrEqual => self.compare(binary.operator, &left, &right, span),
            BinaryOp::In => self.membership(&right, &left, span),
            BinaryOp::Contains => self.contains(&left, &right, span),
            BinaryOp::Add | BinaryOp::Subtract | BinaryOp::Multiply => {
                self.arithmetic(binary.operator, &left, &right, span)
            }
            BinaryOp::Divide => self.divide(&left, &right, span),
            BinaryOp::Matches => self.matches(&left, &right, span),
            BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
        }
    }

    /// Three-valued `AND` and `OR`, from the registered `unknown_logic` table.
    ///
    /// The table is read from the registry, never written here: a change to the
    /// canonical truth table changes this behaviour without a code change.
    fn logic(&self, operator: BinaryOp, left: &Value, right: &Value, span: Span) -> Demand {
        let (Some(a), Some(b)) = (logic_operand(left), logic_operand(right)) else {
            let offending = if logic_operand(left).is_none() {
                left
            } else {
                right
            };
            return Err(self.operand(
                span,
                format!(
                    "{} requires BOOLEAN, found {}",
                    operator.lexeme(),
                    offending.family()
                ),
            ));
        };
        let key = format!("{a} {} {b}", operator.lexeme());
        match self.contracts.statics().unknown_logic(&key) {
            Some("TRUE") => Ok(Value::Boolean(true)),
            Some("FALSE") => Ok(Value::Boolean(false)),
            Some("UNKNOWN") => Ok(Value::Unknown),
            _ => Err(self.operand(
                span,
                format!("the registered truth table has no row for {key:?}"),
            )),
        }
    }

    /// Ordered comparison under the registered total-order profile.
    fn compare(&self, operator: BinaryOp, left: &Value, right: &Value, span: Span) -> Demand {
        // "any UNKNOWN operand yields UNKNOWN for a ... ordered comparison"
        if left == &Value::Unknown || right == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        // "Every arithmetic, ordered-comparison, SUM, MIN, or MAX overload that
        // requires identical MEASURE units emits error.numeric.unit_mismatch
        // for unequal concrete unit identifiers."
        if let (Value::Quantity(_, a), Value::Quantity(_, b)) = (left, right) {
            if a != b {
                return Err(self.unit_mismatch(span, a, b));
            }
        }
        let Some(ordering) = order_profile::compare(left, right) else {
            return Err(self.operand(
                span,
                format!(
                    "{} is not registered for {} and {}",
                    operator.lexeme(),
                    left.family(),
                    right.family()
                ),
            ));
        };
        Ok(Value::Boolean(match operator {
            BinaryOp::Less => ordering == Ordering::Less,
            BinaryOp::LessOrEqual => ordering != Ordering::Greater,
            BinaryOp::Greater => ordering == Ordering::Greater,
            BinaryOp::GreaterOrEqual => ordering != Ordering::Less,
            _ => return Err(self.operand(span, "not an ordered comparison")),
        }))
    }

    /// `IN`: "tests strict-equality membership".
    fn membership(&self, collection: &Value, member: &Value, span: Span) -> Demand {
        if collection == &Value::Unknown || member == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        match collection {
            // "Membership in an empty collection is FALSE once both operands
            // are material."
            Value::List(items) | Value::Set(items) => Ok(Value::Boolean(
                items.iter().any(|item| strict_equal(item, member)),
            )),
            other => Err(self.operand(
                span,
                format!("IN requires a collection, found {}", other.family()),
            )),
        }
    }

    /// `CONTAINS`, which "reverses IN" for a collection and has its own rules
    /// for `OBJECT` and `STRING`.
    fn contains(&self, container: &Value, item: &Value, span: Span) -> Demand {
        if container == &Value::Unknown || item == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        match container {
            Value::List(items) | Value::Set(items) => Ok(Value::Boolean(
                items.iter().any(|member| strict_equal(member, item)),
            )),
            // "OBJECT CONTAINS tests exact STRING key presence."
            Value::Object(fields) => match item {
                Value::Text(key) => Ok(Value::Boolean(fields.contains_key(key))),
                other => Err(self.operand(
                    span,
                    format!(
                        "OBJECT CONTAINS requires a STRING key, found {}",
                        other.family()
                    ),
                )),
            },
            // "STRING CONTAINS tests a contiguous exact Unicode-scalar
            // substring, and the empty substring is present in every STRING."
            Value::Text(haystack) => match item {
                Value::Text(needle) => Ok(Value::Boolean(haystack.contains(needle.as_str()))),
                other => Err(self.operand(
                    span,
                    format!(
                        "STRING CONTAINS requires a STRING, found {}",
                        other.family()
                    ),
                )),
            },
            other => Err(self.operand(
                span,
                format!("CONTAINS is not registered for {}", other.family()),
            )),
        }
    }

    fn arithmetic(&self, operator: BinaryOp, left: &Value, right: &Value, span: Span) -> Demand {
        if left == &Value::Unknown || right == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        let (Some(a), Some(b)) = (left.number(), right.number()) else {
            return Err(self.operand(
                span,
                format!(
                    "{} requires numbers, found {} and {}",
                    operator.lexeme(),
                    left.family(),
                    right.family()
                ),
            ));
        };
        // Same-unit constraint for quantities.
        if let (Value::Quantity(_, ua), Value::Quantity(_, ub)) = (left, right) {
            if ua != ub {
                return Err(self.unit_mismatch(span, ua, ub));
            }
        }
        let result = match operator {
            BinaryOp::Add => a.add(b),
            BinaryOp::Subtract => a.sub(b),
            BinaryOp::Multiply => a.mul(b),
            _ => return Err(self.operand(span, "not an arithmetic operator")),
        };
        let value = self.numeric_result(left, right, result);
        // `03_TYPES_AND_VALUES/06`: "Subtracting one DURATION from another is
        // valid only when the result is non-negative; otherwise evaluation
        // produces error.value.out_of_range."
        if order_profile::is_duration(&value) && value.number().is_some_and(Decimal::is_negative) {
            return Err(Fault::new(
                self.contracts,
                RuntimeError::ValueOutOfRange,
                span,
                "DURATION",
                "a DURATION is not negative",
            ));
        }
        Ok(value)
    }

    /// The family of an arithmetic result.
    ///
    /// "INTEGER promotes to DECIMAL only when paired with DECIMAL outside
    /// division", and a quantity keeps its exact unit.
    fn numeric_result(&self, left: &Value, right: &Value, result: Decimal) -> Value {
        match (left, right) {
            (Value::Quantity(_, unit), _) | (_, Value::Quantity(_, unit)) => {
                Value::Quantity(result, unit.clone())
            }
            (Value::Percentage(_), _) | (_, Value::Percentage(_)) => Value::Percentage(result),
            (Value::Bytes(_), _) | (_, Value::Bytes(_)) => Value::Bytes(result),
            (Value::Integer(_), Value::Integer(_)) => Value::Integer(result),
            _ => Value::Decimal(result),
        }
    }

    /// Division, whose result "has static result type DECIMAL".
    fn divide(&self, left: &Value, right: &Value, span: Span) -> Demand {
        if left == &Value::Unknown || right == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        let (Some(a), Some(b)) = (left.number(), right.number()) else {
            return Err(self.operand(
                span,
                format!(
                    "division requires numbers, found {} and {}",
                    left.family(),
                    right.family()
                ),
            ));
        };
        // "MEASURE divided by a MEASURE with the same exact UNIT returns
        // dimensionless DECIMAL; different units produce
        // error.numeric.unit_mismatch, and numeric divided by MEASURE is not
        // registered."
        let unit = match (left, right) {
            (Value::Quantity(_, ua), Value::Quantity(_, ub)) => {
                if ua != ub {
                    return Err(self.unit_mismatch(span, ua, ub));
                }
                None
            }
            // "MEASURE divided by INTEGER or DECIMAL preserves its exact UNIT
            // and has a DECIMAL numeric component."
            (Value::Quantity(_, ua), _) => Some(ua.clone()),
            (_, Value::Quantity(_, _)) => {
                return Err(self.operand(span, "numeric divided by MEASURE is not registered"))
            }
            _ => None,
        };
        let quotient = self.exact_quotient(a, b, span)?;
        Ok(match unit {
            Some(unit) => Value::Quantity(quotient, unit),
            None => Value::Decimal(quotient),
        })
    }

    /// The exact finite base-10 quotient, or its registered defect.
    ///
    /// `05_SEMANTICS/12`: "Division computes an exact mathematical quotient. A
    /// finite base-10 result is required unless the quotient is the direct
    /// first argument of ROUND". The exact rational is reduced first and only
    /// then required to terminate, so the two defects stay distinguishable.
    fn exact_quotient(&self, a: &Decimal, b: &Decimal, span: Span) -> Result<Decimal, Fault> {
        Rational::of(a, b)
            .and_then(|rational| rational.to_terminating_decimal())
            .map_err(|defect| self.division_defect(defect, span))
    }

    pub(crate) fn division_defect(&self, defect: DivisionDefect, span: Span) -> Fault {
        match defect {
            // "Mathematical-zero denominators always produce
            // error.numeric.division_by_zero."
            DivisionDefect::Zero => Fault::new(
                self.contracts,
                RuntimeError::NumericDivisionByZero,
                span,
                "division",
                "the denominator is mathematically zero",
            ),
            // "Other non-terminating quotients produce
            // error.numeric.non_terminating."
            DivisionDefect::NonTerminating => Fault::new(
                self.contracts,
                RuntimeError::NumericNonTerminating,
                span,
                "division",
                "the exact quotient has no finite base-10 representation",
            ),
            // "Host limitations produce error.host.constraint without changing
            // the required value."
            DivisionDefect::TooLarge => Fault::new(
                self.contracts,
                RuntimeError::HostConstraint,
                span,
                "division",
                "the exact quotient exceeds the digits this host materializes",
            ),
        }
    }

    fn matches(&self, input: &Value, pattern: &Value, span: Span) -> Demand {
        // "any UNKNOWN operand yields UNKNOWN for a ... pattern match"
        if input == &Value::Unknown || pattern == &Value::Unknown {
            return Ok(Value::Unknown);
        }
        let Value::Constructed {
            constructor,
            text: pattern_text,
        } = pattern
        else {
            return Err(self.operand(
                span,
                format!(
                    "MATCHES requires a REGEX or GLOB, found {}",
                    pattern.family()
                ),
            ));
        };
        let Some(subject) = input.text() else {
            return Err(self.operand(
                span,
                format!(
                    "MATCHES requires a STRING or PATH, found {}",
                    input.family()
                ),
            ));
        };
        // "It compares the entire input under the selected closed pattern
        // profile. A non-match returns FALSE."
        let outcome = match constructor.as_str() {
            "REGEX" => {
                let (body, flags) = split_regex(pattern_text);
                Flags::parse(flags)
                    .and_then(|flags| Regex::compile(body, flags))
                    .and_then(|compiled| compiled.matches(subject))
            }
            // `03_TYPES_AND_VALUES/07`: "A PATH input requires an explicit
            // WORKSPACE root retained by the value ...; its normalized relative
            // segments are used. ... No root or filesystem expansion is
            // inferred."
            "GLOB" => {
                let relative;
                let subject = match input {
                    Value::WorkspacePath { relative: text, .. } => {
                        relative = text
                            .split('/')
                            .filter(|segment| !segment.is_empty() && *segment != ".")
                            .collect::<Vec<_>>()
                            .join("/");
                        relative.as_str()
                    }
                    _ => subject,
                };
                Glob::compile(pattern_text).and_then(|compiled| compiled.matches(subject))
            }
            other => {
                return Err(self.operand(span, format!("MATCHES is not registered for {other}")))
            }
        };
        match outcome {
            Ok(accepted) => Ok(Value::Boolean(accepted)),
            Err(PatternFault::ResourceLimit(detail)) => Err(Fault::new(
                self.contracts,
                RuntimeError::PatternResourceLimit,
                span,
                "pattern resource limit",
                detail,
            )),
            Err(PatternFault::Invalid(detail)) => Err(Fault::new(
                self.contracts,
                RuntimeError::LiteralInvalid,
                span,
                "pattern",
                detail,
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Postfix
    // -----------------------------------------------------------------------

    fn property(&self, property: &PropertyAccess, depth: usize) -> Demand {
        // "A reserved uppercase property directly after REF, with optional
        // parentheses around that REF, selects registered declaration metadata
        // before a bound-value read."
        if is_reserved_property(&property.name) {
            if let Some(id) = reference_target(&property.base) {
                return self.metadata(id, &property.name, depth);
            }
        }
        let base = self.eval(&property.base, depth + 1)?;
        match base {
            Value::Missing => Err(self.missing(property.span, "property base")),
            Value::Unknown => Ok(Value::Unknown),
            // "Lowercase property access selects an evaluated OBJECT field
            // under the same presence rules."
            Value::Object(fields) => Ok(fields
                .get(&property.name)
                .cloned()
                // "An absent optional schema field or absent schema-free OBJECT
                // field yields MISSING".
                .unwrap_or(Value::Missing)),
            other => Err(self.operand(
                property.span,
                format!(
                    "property access requires an OBJECT, found {}",
                    other.family()
                ),
            )),
        }
    }

    /// A registered declaration-metadata read, e.g. `REF(output.copy).TARGET`.
    ///
    /// `05_SEMANTICS/01`: "A metadata read such as REF(output.copy).TARGET
    /// reads the declaration field without requiring the OUTPUT's result
    /// binding." So this never touches a binding and never starts a producer.
    fn metadata(&self, id: &str, field: &str, depth: usize) -> Demand {
        let Some(index) = self
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == id)
        else {
            return Ok(Value::Missing);
        };
        let Some(block) = crate::syntax::declaration_block(self.resolved, index) else {
            return Ok(Value::Missing);
        };
        if let Some(expression) = crate::syntax::field_expr(&block, field) {
            // Metadata has the selected field's registered/inferred type. Its
            // source spelling is not a STRING value, and its spans belong to
            // the declaring document even when the read is in an importer.
            let declaring = &self.resolved.declarations().all()[index];
            let context = Evaluator {
                contracts: self.contracts,
                resolved: self.resolved,
                checked: self.checked,
                plan: self.plan,
                bindings: self.bindings,
                source: declaring.source.clone(),
                iteration: self.iteration.clone(),
            };
            return context.eval(expression, depth + 1);
        }
        Ok(
            crate::syntax::declaration_field_text(self.resolved, index, field)
                .map(Value::Text)
                .unwrap_or(Value::Missing),
        )
    }

    fn index(&self, index: &lcl_parser::syntax::IndexAccess, depth: usize) -> Demand {
        let base = self.eval(&index.base, depth + 1)?;
        let selector = self.eval(&index.index, depth + 1)?;
        if base == Value::Missing {
            return Err(self.missing(index.span, "index base"));
        }
        if selector == Value::Missing {
            return Err(self.missing(index.span, "index"));
        }
        if base == Value::Unknown || selector == Value::Unknown {
            return Ok(Value::Unknown);
        }
        match (&base, &selector) {
            // "LIST indexing is zero-based. Every INTEGER below zero or at
            // least the member count yields MISSING; negative indices never
            // wrap."
            (Value::List(items), Value::Integer(position)) => {
                if position.is_negative() {
                    return Ok(Value::Missing);
                }
                match position.to_i64() {
                    Some(i) if (i as usize) < items.len() => Ok(items[i as usize].clone()),
                    _ => Ok(Value::Missing),
                }
            }
            // "OBJECT STRING indexing selects an exact, statically known
            // property so its result has one exact field type; a runtime-
            // varying key uses error.operator.operand."
            (Value::Object(fields), Value::Text(key)) => {
                if !matches!(&*index.index, Expr::Literal(_)) {
                    return Err(
                        self.operand(index.span, "an OBJECT index must be a statically known key")
                    );
                }
                Ok(fields.get(key).cloned().unwrap_or(Value::Missing))
            }
            // "SET and STRING indexing are not admitted."
            (Value::Set(_), _) => Err(self.operand(index.span, "SET indexing is not admitted")),
            (Value::Text(_), _) => Err(self.operand(index.span, "STRING indexing is not admitted")),
            _ => Err(self.operand(
                index.span,
                format!(
                    "indexing is not registered for {} by {}",
                    base.family(),
                    selector.family()
                ),
            )),
        }
    }

    // -----------------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------------

    fn call(&self, call: &Call, depth: usize) -> Demand {
        // `REF(x)`.
        if call.is_reference() {
            let Some(target) = call.reference_target() else {
                return Err(self.operand(call.span, "REF requires one declaration reference"));
            };
            // An identity context retains the reference rather than reading it.
            if self.is_identity(call.span) {
                let loop_local = self.bindings.local(&target.text, &self.iteration).is_some();
                return Ok(Value::Reference(crate::value::reference_identity(
                    &target.text,
                    &self.iteration,
                    loop_local,
                )));
            }
            return Ok(self.declaration_value(&target.text));
        }

        let name = call.callable.text.as_str();

        // A quantifier over an immediate bracket sequence is the one argument
        // form that admits UNKNOWN members, so it must not go through the
        // ordinary collection path.
        if matches!(name, "ALL" | "ANY" | "NONE") {
            if let Some(members) = immediate_sequence(call) {
                return self.quantifier(name, members, call.span, depth);
            }
        }

        // "When its direct first argument is a division expression, ROUND
        // evaluates the exact mathematical quotient and rounds it once; this is
        // the only context in which an otherwise non-terminating quotient is
        // materialized."
        if name == "ROUND" {
            if let Some(value) = self.round(call, depth)? {
                return Ok(value);
            }
        }

        // "Evaluate ... constructor arguments, function arguments ... in source
        // order from left to right."
        let mut arguments = Vec::new();
        for argument in &call.arguments {
            arguments.push(self.eval(argument, depth + 1)?);
        }

        if self.contracts.statics().constructor(name).is_some() {
            return self.constructor(name, &arguments, call);
        }
        if self.contracts.statics().function(name).is_some() {
            return crate::functions::apply(self, name, &arguments, call.span);
        }
        Err(self.operand(
            call.span,
            format!("{name} is not a registered function or constructor"),
        ))
    }

    /// `ROUND` over a direct division argument.
    ///
    /// Returns `None` when the first argument is not a division, so the
    /// ordinary path applies.
    fn round(&self, call: &Call, depth: usize) -> Result<Option<Value>, Fault> {
        let Some(Expr::Binary(division)) = call.arguments.first().map(unwrap_group) else {
            return Ok(None);
        };
        if division.operator != BinaryOp::Divide {
            return Ok(None);
        }
        let Some(digits_expr) = call.arguments.get(1) else {
            return Ok(None);
        };
        let left = self.eval(&division.left, depth + 1)?;
        let right = self.eval(&division.right, depth + 1)?;
        let digits = self.eval(digits_expr, depth + 1)?;
        for (value, span) in [
            (&left, division.left.span()),
            (&right, division.right.span()),
            (&digits, digits_expr.span()),
        ] {
            if value == &Value::Missing {
                return Err(self.missing(span, "ROUND operand"));
            }
        }
        if left == Value::Unknown || right == Value::Unknown || digits == Value::Unknown {
            return Ok(Some(Value::Unknown));
        }
        let (Some(a), Some(b), Some(d)) = (left.number(), right.number(), digits.number()) else {
            return Err(self.operand(call.span, "ROUND requires numbers"));
        };
        // "a declared non-negative number of fractional digits"
        let Some(places) = d.to_i64().and_then(|v| u32::try_from(v).ok()) else {
            return Err(Fault::new(
                self.contracts,
                RuntimeError::ValueOutOfRange,
                digits_expr.span(),
                "ROUND digits",
                "ROUND requires a non-negative number of fractional digits",
            ));
        };
        // Same-unit and MEASURE rules apply to the quotient as usual.
        let unit = match (&left, &right) {
            (Value::Quantity(_, ua), Value::Quantity(_, ub)) => {
                if ua != ub {
                    return Err(self.unit_mismatch(call.span, ua, ub));
                }
                None
            }
            (Value::Quantity(_, ua), _) => Some(ua.clone()),
            (_, Value::Quantity(_, _)) => {
                return Err(self.operand(call.span, "numeric divided by MEASURE is not registered"))
            }
            _ => None,
        };
        let rational = Rational::of(a, b).map_err(|d| self.division_defect(d, division.span))?;
        // "rounds it once using half-even"
        let rounded = rational
            .round_half_even(places)
            .map_err(|d| self.division_defect(d, division.span))?;
        Ok(Some(match unit {
            Some(unit) => Value::Quantity(rounded, unit),
            None => Value::Decimal(rounded),
        }))
    }

    /// The absolute root one `WORKSPACE` declaration declares.
    ///
    /// `field_signatures#/blocks/WORKSPACE`: "PATH is absolute."
    fn workspace_root(&self, id: &str) -> Option<String> {
        let index = self
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == id && d.block == "WORKSPACE")?;
        let text = crate::syntax::declaration_field_text(self.resolved, index, "PATH")?;
        // The field is written `PATH("/root")`; take the quoted string.
        let inner = text.strip_prefix("PATH(")?.strip_suffix(')')?;
        Some(inner.trim_matches('"').to_string())
    }

    /// A registered typed constructor. "A constructor call is pure."
    fn constructor(&self, name: &str, arguments: &[Value], call: &Call) -> Demand {
        let span = call.span;
        // "UNKNOWN propagates through arithmetic, constructors, ..."
        if arguments.iter().any(|a| a == &Value::Unknown) {
            return Ok(Value::Unknown);
        }
        if let Some(index) = arguments.iter().position(|a| a == &Value::Missing) {
            let span = call.arguments.get(index).map(|a| a.span()).unwrap_or(span);
            return Err(self.missing(span, "constructor argument"));
        }

        let row = self.contracts.statics().constructor(name);
        let value = match (name, arguments) {
            ("DURATION", [magnitude, unit]) => {
                let (Some(number), Value::Identifier(unit_id)) = (magnitude.number(), unit) else {
                    return Err(self.operand(span, "DURATION requires a number and a unit"));
                };
                // The constructor row narrows DURATION to the Time category and
                // sets `minimum: 0`.
                if number.is_negative() {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::ValueOutOfRange,
                        span,
                        "DURATION",
                        "a DURATION is not negative",
                    ));
                }
                let Some(value) = self.contracts.duration().duration(number, unit_id) else {
                    return Err(self.unit_mismatch_single(span, unit_id));
                };
                value
            }
            ("MEASURE", [magnitude, unit]) => {
                let (Some(number), Value::Identifier(unit_id)) = (magnitude.number(), unit) else {
                    return Err(self.operand(span, "MEASURE requires a number and a unit"));
                };
                if !self.contracts.statics().is_unit(unit_id) {
                    return Err(self.unit_mismatch_single(span, unit_id));
                }
                Value::Quantity(number.clone(), UnitId(unit_id.clone()))
            }
            ("PERCENTAGE", [value]) => match value.number() {
                Some(number) => Value::Percentage(number.clone()),
                None => return Err(self.operand(span, "PERCENTAGE requires a number")),
            },
            ("BYTES", [value]) => match value.number() {
                Some(number) => {
                    if number.is_negative() {
                        return Err(Fault::new(
                            self.contracts,
                            RuntimeError::ValueOutOfRange,
                            span,
                            "BYTES",
                            "a BYTES count is not negative",
                        ));
                    }
                    Value::Bytes(number.clone())
                }
                None => return Err(self.operand(span, "BYTES requires a number")),
            },
            // "PATH(REFERENCE[WORKSPACE], STRING)": "The string is relative
            // and the resolved path is the WORKSPACE root or one of its
            // descendants." M5 already proved containment before effects; this
            // resolves the value the runtime hands to a capability.
            ("PATH", [reference, Value::Text(relative)]) => {
                let Some(id) = reference.text() else {
                    return Err(self.operand(
                        span,
                        "PATH requires a WORKSPACE reference and a relative string",
                    ));
                };
                let Some(root) = self.workspace_root(id) else {
                    return Err(
                        self.operand(span, format!("{id} does not declare a WORKSPACE PATH"))
                    );
                };
                // The registry names `error.value.out_of_range` as this
                // constructor's `workspace_escape_error`. Preflight decides
                // containment; this refuses the two spellings that could only
                // be an escape, rather than resolving them silently.
                if relative.starts_with('/') || relative.split('/').any(|segment| segment == "..") {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::ValueOutOfRange,
                        span,
                        "workspace escape",
                        format!("{relative:?} is not inside the WORKSPACE root"),
                    ));
                }
                Value::WorkspacePath {
                    workspace: id.to_string(),
                    relative: relative.clone(),
                    resolved: format!("{}/{}", root.trim_end_matches('/'), relative),
                }
            }
            // `REGEX(pattern)` and `REGEX(pattern, flags)` are both
            // registered. The flags travel with the value, joined by a
            // separator no LCL source string can contain unescaped, so the
            // pattern profile can recover both halves exactly.
            ("REGEX", [Value::Text(pattern)]) => Value::Constructed {
                constructor: name.to_string(),
                text: pattern.clone(),
            },
            ("REGEX", [Value::Text(pattern), Value::Text(flags)]) => {
                // An unadmitted flag is a value-domain defect of a statically
                // valid constructor, which is `error.literal.invalid` under
                // `expression_demand_resolution`.
                if let Err(fault) = Flags::parse(flags) {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::LiteralInvalid,
                        span,
                        "REGEX flags",
                        format!("{fault:?}"),
                    ));
                }
                lcl_semantics::value::regex(pattern, flags)
            }
            (_, [Value::Text(text)]) => Value::Constructed {
                constructor: name.to_string(),
                text: text.clone(),
            },
            _ => {
                return Err(self.operand(
                    span,
                    format!("{name} has no registered overload for these arguments"),
                ))
            }
        };

        // An exact registered bound over a dynamically supplied value is
        // `error.value.out_of_range` under `expression_demand_resolution`.
        if let (Some(row), Some(number)) = (row, value.number()) {
            if let Some(minimum) = row.minimum {
                if number.compare(&decimal_from_i64(minimum)) == Ordering::Less {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::ValueOutOfRange,
                        span,
                        name,
                        format!("{name} has a registered minimum of {minimum}"),
                    ));
                }
            }
            if let Some(maximum) = row.maximum {
                if number.compare(&decimal_from_i64(maximum)) == Ordering::Greater {
                    return Err(Fault::new(
                        self.contracts,
                        RuntimeError::ValueOutOfRange,
                        span,
                        name,
                        format!("{name} has a registered maximum of {maximum}"),
                    ));
                }
            }
        }
        Ok(value)
    }

    /// `ALL`, `ANY` and `NONE` over an immediate bracket sequence.
    ///
    /// The sequence is a local `Vec`, never a [`Value`]: "such a sequence is
    /// non-material and exists only while that call reduces it; it cannot be
    /// stored, nested as data, indexed, returned, or passed through another
    /// function."
    fn quantifier(&self, name: &str, members: &[Expr], span: Span, depth: usize) -> Demand {
        // "is evaluated left to right before reduction"
        let mut observations = Vec::new();
        for member in members {
            let value = self.eval(member, depth + 1)?;
            match value {
                Value::Boolean(b) => observations.push(Some(b)),
                // "This immediate quantifier argument explicitly permits
                // UNKNOWN literals even outside another condition."
                Value::Unknown => observations.push(None),
                // "A MISSING member emits error.required.missing"
                Value::Missing => return Err(self.missing(member.span(), "quantifier member")),
                // "a member of another material family emits
                // error.operator.operand"
                other => {
                    return Err(self.operand(
                        member.span(),
                        format!(
                            "a quantifier member must be BOOLEAN, found {}",
                            other.family()
                        ),
                    ))
                }
            }
        }
        reduce_quantifier(name, &observations)
            .ok_or_else(|| self.operand(span, format!("{name} is not a quantifier")))
    }

    // -----------------------------------------------------------------------
    // Fault constructors
    // -----------------------------------------------------------------------

    pub(crate) fn missing(&self, span: Span, what: &str) -> Fault {
        Fault::new(
            self.contracts,
            RuntimeError::RequiredMissing,
            span,
            what,
            format!("a required {what} yielded MISSING at its demand point"),
        )
    }

    pub(crate) fn operand(&self, span: Span, detail: impl Into<String>) -> Fault {
        Fault::new(
            self.contracts,
            RuntimeError::OperatorOperand,
            span,
            "operand",
            detail,
        )
    }

    fn unit_mismatch(&self, span: Span, left: &UnitId, right: &UnitId) -> Fault {
        Fault::new(
            self.contracts,
            RuntimeError::NumericUnitMismatch,
            span,
            "unit",
            format!("{} and {} are not the same exact unit", left.0, right.0),
        )
    }

    fn unit_mismatch_single(&self, span: Span, unit: &str) -> Fault {
        Fault::new(
            self.contracts,
            RuntimeError::NumericUnitMismatch,
            span,
            "unit",
            format!("{unit} is not a registered unit for this constructor"),
        )
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// An exact `Decimal` for one registered bound.
///
/// The registry states constructor bounds as JSON integers, so this is the one
/// place a bound crosses into the exact numeric model.
fn decimal_from_i64(value: i64) -> Decimal {
    let magnitude = Decimal::from_integer(Integer::from_u64(value.unsigned_abs()));
    if value < 0 {
        magnitude.negated()
    } else {
        magnitude
    }
}

/// The registered Boolean spelling of an operand, for the truth table.
fn logic_operand(value: &Value) -> Option<&'static str> {
    match value {
        Value::Boolean(true) => Some("TRUE"),
        Value::Boolean(false) => Some("FALSE"),
        Value::Unknown => Some("UNKNOWN"),
        _ => None,
    }
}

/// `ALL`/`ANY`/`NONE` over three-valued observations.
///
/// "ALL returns FALSE if any member is FALSE, else UNKNOWN if any member is
/// UNKNOWN, else TRUE. ANY returns TRUE if any member is TRUE, else UNKNOWN if
/// any member is UNKNOWN, else FALSE. NONE is NOT of ANY. Thus ALL([]) and
/// NONE([]) are TRUE and ANY([]) is FALSE."
pub(crate) fn reduce_quantifier(name: &str, observations: &[Option<bool>]) -> Option<Value> {
    let any_false = observations.iter().any(|o| o == &Some(false));
    let any_true = observations.iter().any(|o| o == &Some(true));
    let any_unknown = observations.iter().any(Option::is_none);
    Some(match name {
        "ALL" => {
            if any_false {
                Value::Boolean(false)
            } else if any_unknown {
                Value::Unknown
            } else {
                Value::Boolean(true)
            }
        }
        "ANY" => {
            if any_true {
                Value::Boolean(true)
            } else if any_unknown {
                Value::Unknown
            } else {
                Value::Boolean(false)
            }
        }
        "NONE" => {
            // "NONE is NOT of ANY."
            if any_true {
                Value::Boolean(false)
            } else if any_unknown {
                Value::Unknown
            } else {
                Value::Boolean(true)
            }
        }
        _ => return None,
    })
}

/// The immediate bracket sequence of a quantifier call, allowing parentheses.
///
/// "ALL, ANY, and NONE ... accept ... one immediate bracket sequence, allowing
/// parentheses around that sequence". Parentheses "do not change that rule".
fn immediate_sequence(call: &Call) -> Option<&[Expr]> {
    if call.arguments.len() != 1 {
        return None;
    }
    match unwrap_group(call.arguments.first()?) {
        Expr::Collection(collection) => Some(&collection.members),
        _ => None,
    }
}

/// Strip any number of parentheses.
fn unwrap_group(expr: &Expr) -> &Expr {
    let mut current = expr;
    while let Expr::Group(group) = current {
        current = &group.inner;
    }
    current
}

/// The declaration a `REF(...)` names, allowing parentheses around it.
fn reference_target(expr: &Expr) -> Option<&str> {
    match unwrap_group(expr) {
        Expr::Call(call) if call.is_reference() => {
            call.reference_target().map(|ident| ident.text.as_str())
        }
        _ => None,
    }
}

/// True for a reserved uppercase declaration-metadata property.
fn is_reserved_property(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
}

/// Split a `REGEX` value's stored text into its pattern and flags.
///
/// The constructor is written `REGEX("pattern")` or `REGEX("pattern", "flags")`;
/// `lcl_semantics::value::regex` builds the one shared text, joining non-empty
/// flags to the pattern with a NUL.
fn split_regex(text: &str) -> (&str, &str) {
    match text.split_once(REGEX_FLAG_SEPARATOR) {
        Some((pattern, flags)) => (pattern, flags),
        None => (text, ""),
    }
}

/// Strict equality, which "always returns BOOLEAN".
///
/// `05_SEMANTICS/12`: "Each singleton sentinel equals itself and differs from
/// the other sentinel and every material value. Different material static types
/// are unequal except for exact INTEGER/DECIMAL comparison. MEASURE equality
/// compares both exact unit identifier and numeric magnitude; different units
/// produce FALSE."
pub fn strict_equal(left: &Value, right: &Value) -> bool {
    lcl_semantics::value::strict_equal(left, right)
}

/// The number of members `COUNT` reports, when the family has one.
pub(crate) fn count_of(value: &Value) -> Option<Decimal> {
    let count = match value {
        // "COUNT counts Unicode scalars in STRING"
        Value::Text(text) => text.chars().count(),
        // "occurrences in LIST, unique SET members"
        Value::List(items) | Value::Set(items) => items.len(),
        // "present OBJECT properties"
        Value::Object(fields) => fields.len(),
        // "COUNT(BYTES(n)) returns INTEGER n."
        Value::Bytes(n) => return Some(n.clone()),
        _ => return None,
    };
    Some(Decimal::from_integer(Integer::from_u64(count as u64)))
}

/// A `DURATION`'s normalized magnitude, for reports.
pub fn duration_magnitude(value: &Value) -> Option<&Decimal> {
    match value {
        Value::Quantity(d, unit) if unit.0 == DURATION_UNIT => Some(d),
        _ => None,
    }
}

/// The registered order used by `core.sort` and direct `SET` iteration.
pub fn sorted(values: &[Value]) -> Option<Vec<Value>> {
    for pair in values.windows(2) {
        if !order_profile::order_compatible(&pair[0], &pair[1]) {
            return None;
        }
    }
    let mut sorted = values.to_vec();
    // A stable sort, so "original LIST source position for ties" holds.
    sorted.sort_by(|a, b| order_profile::compare(a, b).unwrap_or(Ordering::Equal));
    Some(sorted)
}

/// An `OBJECT` built from named fields, in declaration order.
pub fn object(fields: impl IntoIterator<Item = (String, Value)>) -> Value {
    Value::Object(fields.into_iter().collect::<BTreeMap<_, _>>())
}
