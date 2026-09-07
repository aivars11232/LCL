//! The material value model this layer resolves and compares.
//!
//! ## Why this is not `lcl_checker`'s static value
//!
//! M4 has a statically known value (`Const`) covering exactly what a *static*
//! check consumes: numbers, Booleans, text, registered identifiers, quantities
//! and compiled patterns. Preflight consumes more, because steps 6 through 8
//! resolve declared data and evaluate check assertions over it: collections and
//! objects are ordinary operands here, and the three non-material sentinels are
//! first-class outcomes rather than an absence of knowledge.
//!
//! What is *not* duplicated is arithmetic. Exact base-10 `Integer`, `Decimal`
//! and `Rational` — including round-half-even and the terminating-quotient test
//! — live in `lcl_checker::numeric` and are used from there. A second
//! implementation of exact division would be a second implementation of the
//! language.
//!
//! ## The three sentinels are values, not errors
//!
//! `05_SEMANTICS/06`: "MISSING: no value/source exists. UNKNOWN: value exists
//! but cannot be determined. NULL: known explicit absence." All three are
//! representable, because the resolution order distinguishes them: "DEFAULT
//! never replaces NULL/UNKNOWN unless a rule explicitly maps them first."
//! Collapsing them would make that rule unimplementable.

use lcl_checker::numeric::Decimal;
use lcl_checker::ty::UnitId;
use std::collections::BTreeMap;
use std::fmt;

/// One material or non-material value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Boolean(bool),
    /// An exact base-10 integer.
    Integer(Decimal),
    /// An exact base-10 decimal.
    Decimal(Decimal),
    /// A `PERCENTAGE`, held as its written number.
    Percentage(Decimal),
    /// A `BYTES` count.
    Bytes(Decimal),
    Text(String),
    /// A `MEASURE` or `DURATION` with its exact registered unit.
    Quantity(Decimal, UnitId),
    /// A `PATH`, `URI`, `GLOB`, `REGEX`, `DATE`, `TIME` or `DATETIME`, with the
    /// constructor that built it, so a comparison cannot silently cross
    /// families.
    Constructed {
        constructor: String,
        text: String,
    },
    /// A registered qualified identifier: a format, encoding, kind, mode,
    /// status, event, error or unit.
    Identifier(String),
    /// An ordered `LIST`.
    List(Vec<Value>),
    /// A `SET`, held in the registered member order its declaration fixed.
    Set(Vec<Value>),
    /// An `OBJECT`, keyed by field name in declaration order.
    Object(BTreeMap<String, Value>),
    /// A retained reference identity, never its referent's value.
    Reference(String),
    /// Known explicit absence.
    Null,
    /// No value or source exists.
    Missing,
    /// A value exists but cannot be determined.
    Unknown,
}

impl Value {
    /// True for a value that can bind an OUTPUT or satisfy a required read.
    ///
    /// `05_SEMANTICS/05`: "MISSING and UNKNOWN are non-material and never bind
    /// or partially bind OUTPUT."
    pub fn is_material(&self) -> bool {
        !matches!(self, Value::Missing | Value::Unknown)
    }

    /// The Boolean this value is, if it is one.
    ///
    /// Deliberately narrow: nothing here coerces. `05_SEMANTICS/05` records
    /// that "FALSE, zero, zero BYTES, an empty STRING, and an empty collection
    /// are completed material outcomes", so truthiness by emptiness would
    /// invent a rule the language does not have.
    pub fn boolean(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// The exact number this value carries, if it carries one.
    pub fn number(&self) -> Option<&Decimal> {
        match self {
            Value::Integer(d)
            | Value::Decimal(d)
            | Value::Percentage(d)
            | Value::Bytes(d)
            | Value::Quantity(d, _) => Some(d),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(t) | Value::Identifier(t) | Value::Reference(t) => Some(t),
            Value::Constructed { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The registry family name of this value, for diagnostics.
    pub fn family(&self) -> &str {
        match self {
            Value::Boolean(_) => "BOOLEAN",
            Value::Integer(_) => "INTEGER",
            Value::Decimal(_) => "DECIMAL",
            Value::Percentage(_) => "PERCENTAGE",
            Value::Bytes(_) => "BYTES",
            Value::Text(_) => "STRING",
            Value::Quantity(_, _) => "MEASURE",
            Value::Constructed { constructor, .. } => constructor,
            Value::Identifier(_) => "identifier",
            Value::List(_) => "LIST",
            Value::Set(_) => "SET",
            Value::Object(_) => "OBJECT",
            Value::Reference(_) => "REFERENCE",
            Value::Null => "NULL",
            Value::Missing => "MISSING",
            Value::Unknown => "UNKNOWN",
        }
    }

    /// The number of members, for a collection.
    pub fn members(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) | Value::Set(items) => Some(items),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Boolean(true) => f.write_str("TRUE"),
            Value::Boolean(false) => f.write_str("FALSE"),
            Value::Integer(d) | Value::Decimal(d) => write!(f, "{d}"),
            Value::Percentage(d) => write!(f, "PERCENTAGE({d})"),
            Value::Bytes(d) => write!(f, "BYTES({d})"),
            Value::Text(t) => write!(f, "{t:?}"),
            Value::Quantity(d, unit) => write!(f, "{d} {}", unit.0),
            Value::Constructed { constructor, text } => write!(f, "{constructor}({text:?})"),
            Value::Identifier(id) => f.write_str(id),
            Value::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Set(items) => {
                f.write_str("SET[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Object(fields) => {
                f.write_str("{")?;
                for (i, (name, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {value}")?;
                }
                f.write_str("}")
            }
            Value::Reference(id) => write!(f, "REF({id})"),
            Value::Null => f.write_str("NULL"),
            Value::Missing => f.write_str("MISSING"),
            Value::Unknown => f.write_str("UNKNOWN"),
        }
    }
}
