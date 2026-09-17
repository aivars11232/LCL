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
    /// A `PATH(REF(workspace), "relative")`. `types_v0.1.0.json` material
    /// identity: "WORKSPACE form uses the resolved workspace declaration
    /// identity and exact decoded relative STRING ... Different forms are
    /// unequal." `resolved` is the contained absolute target a capability
    /// addresses, and `root` is the declared WORKSPACE root it may not leave on
    /// the real filesystem; neither takes part in identity.
    WorkspacePath {
        workspace: String,
        relative: String,
        root: String,
        resolved: String,
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
            Value::WorkspacePath { resolved, .. } => Some(resolved),
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
            Value::WorkspacePath { .. } => "PATH",
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
            Value::WorkspacePath { resolved, .. } => write!(f, "PATH({resolved:?})"),
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

/// Joins a `REGEX` value's pattern and flags in its [`Value::Constructed`] text.
pub const REGEX_FLAG_SEPARATOR: char = '\u{0}';

/// The value `REGEX(pattern, flags)` constructs, shared by every evaluator.
///
/// `types_v0.1.0.json#/material_identity_contract/REGEX`: "omitted flags equal
/// empty flags". Empty flags therefore store the pattern alone, exactly as
/// `REGEX(pattern)` does, so the two forms compare equal.
pub fn regex(pattern: &str, flags: &str) -> Value {
    let text = if flags.is_empty() {
        pattern.to_string()
    } else {
        format!("{pattern}{REGEX_FLAG_SEPARATOR}{flags}")
    };
    Value::Constructed {
        constructor: "REGEX".to_string(),
        text,
    }
}

/// Strict value equality used when materializing a checked homogeneous SET.
/// Scalar numeric promotion is permitted; nested collections retain their
/// member families. SET storage order has no semantic significance.
pub fn strict_equal(left: &Value, right: &Value) -> bool {
    use std::cmp::Ordering;
    match (left, right) {
        (Value::Missing, Value::Missing)
        | (Value::Unknown, Value::Unknown)
        | (Value::Null, Value::Null) => true,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Text(a), Value::Text(b))
        | (Value::Identifier(a), Value::Identifier(b))
        | (Value::Reference(a), Value::Reference(b)) => a == b,
        (
            Value::Constructed {
                constructor: ca,
                text: ta,
            },
            Value::Constructed {
                constructor: cb,
                text: tb,
            },
        ) => {
            ca == cb
                && if matches!(ca.as_str(), "DATE" | "TIME" | "DATETIME") {
                    order_profile::compare(left, right) == Some(Ordering::Equal)
                } else {
                    ta == tb
                }
        }
        (
            Value::WorkspacePath {
                workspace: wa,
                relative: ra,
                ..
            },
            Value::WorkspacePath {
                workspace: wb,
                relative: rb,
                ..
            },
        ) => wa == wb && ra == rb,
        (Value::Quantity(a, ua), Value::Quantity(b, ub)) => {
            ua == ub && a.compare(b) == Ordering::Equal
        }
        (Value::Percentage(a), Value::Percentage(b))
        | (Value::Bytes(a), Value::Bytes(b))
        | (Value::Integer(a) | Value::Decimal(a), Value::Integer(b) | Value::Decimal(b)) => {
            a.compare(b) == Ordering::Equal
        }
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(x, y)| same_member_family(x, y) && strict_equal(x, y))
        }
        (Value::Set(a), Value::Set(b)) => {
            a.len() == b.len()
                && a.iter().all(|x| {
                    b.iter()
                        .any(|y| same_member_family(x, y) && strict_equal(x, y))
                })
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|((ka, va), (kb, vb))| {
                    ka == kb && same_member_family(va, vb) && strict_equal(va, vb)
                })
        }
        _ => false,
    }
}

fn same_member_family(left: &Value, right: &Value) -> bool {
    // Source collections are already checked against one exact member type.
    // Keep the scalar INTEGER/DECIMAL promotion out of recursive comparisons.
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

/// Construct from already evaluated material members. Callers must evaluate
/// every source member before invoking this helper, including duplicates.
pub fn collection(members: Vec<Value>, as_set: bool) -> Value {
    if !as_set {
        return Value::List(members);
    }
    let mut unique = Vec::new();
    for member in members {
        if !unique.iter().any(|kept| strict_equal(kept, &member)) {
            unique.push(member);
        }
    }
    Value::Set(unique)
}

pub mod order_profile {
    //! Canonical normalization shared with the execution layer. The existing
    //! runtime profile lives beside Value so preflight can use it without
    //! depending on runtime.
    //!
    //! The registered total-order profile, as canonical order keys.
    //!
    //! Authority: `operators_and_functions_v0.1.0.json#/ordered_types`,
    //! `#/ordered_type_rules`, `#/ordered_value_equality`, `#/string_order`,
    //! `#/cross_type_numeric_order`, `#/non_material_ordering` and
    //! `formats_encodings_units_v0.1.0.json#/duration_normalization`.
    //!
    //! `05_SEMANTICS/12` makes this profile do double duty:
    //!
    //! > The registered total-order profile is also the sole natural-order source
    //! > for SET iteration and core.sort.
    //!
    //! and it closes the door on every alternative:
    //!
    //! > No locale, insertion order, host collation, or inferred ENUM order may
    //! > change these rules.
    //!
    //! So ordering here is computed from the value alone. Nothing consults the
    //! environment, and two runs of the same comparison cannot disagree.
    //!
    //! ## Equal keys are equal values
    //!
    //! > Within one declared ordered type, equal canonical order keys are
    //! > strict-equal semantic values; equal offset-normalized TIME or DATETIME
    //! > values and equal normalized DURATION magnitudes therefore collapse as SET
    //! > duplicates before ordering.
    //!
    //! That is why an order key is the *same* computation SET duplicate collapse
    //! uses. Having two would let a SET hold two members that compare equal.
    //!
    //! ## How `DURATION` is told apart from `MEASURE`
    //!
    //! The two are different types with opposite comparison rules — `DURATION`
    //! compares by normalized magnitude, `MEASURE` requires "the identical unit
    //! identifier" — and the `MEASURE` constructor row admits Time-category units
    //! explicitly: "Any registered unit is accepted, including Time-category
    //! units." So `MEASURE(1, unit.minute)` and `DURATION(1, unit.minute)` are
    //! distinct values that `lcl_semantics::Value::Quantity` spells identically.
    //!
    //! This layer resolves that without forking the value model: a `DURATION` is
    //! normalized at construction, exactly as the registry requires — "Multiply the
    //! exact DURATION numeric component by its factor; the normalized exact
    //! magnitude defines DURATION equality, arithmetic, and order" — and carries
    //! [`DURATION_UNIT`] as its unit. That identifier is deliberately outside the
    //! `unit.` namespace, so it is not a registered unit and can never collide with
    //! a `MEASURE`. Two `DURATION`s then compare by magnitude under the ordinary
    //! same-unit rule, two `MEASURE`s keep their written units, and no `MEASURE`
    //! is ever equal to a `DURATION`.

    use crate::value::Value;
    use lcl_checker::numeric::{Decimal, Integer};
    use lcl_checker::ty::UnitId;
    use lcl_spec::json::Json;
    use std::cmp::Ordering;
    use std::collections::BTreeMap;

    /// The unit a normalized `DURATION` carries.
    ///
    /// Outside the `unit.` namespace on purpose: it is not a registered unit, so
    /// `Contracts::is_unit` is truthfully false for it and no `MEASURE` can be
    /// written with it.
    pub const DURATION_UNIT: &str = "duration.normalized_nanoseconds";

    /// The registry's `duration_normalization` factors.
    #[derive(Debug, Clone)]
    pub struct DurationProfile {
        base_unit: String,
        factors: BTreeMap<String, u64>,
    }

    impl DurationProfile {
        /// Read the profile from `formats_encodings_units_v0.1.0.json`.
        pub fn load(units: &Json) -> Option<DurationProfile> {
            let normalization = units.get("duration_normalization")?;
            let base_unit = normalization.get("base_unit")?.as_str()?.to_string();
            let mut factors = BTreeMap::new();
            for (unit, factor) in normalization.get("factors")?.as_object()? {
                factors.insert(unit.clone(), factor.as_u64()?);
            }
            if factors.is_empty() {
                return None;
            }
            Some(DurationProfile { base_unit, factors })
        }

        pub fn base_unit(&self) -> &str {
            &self.base_unit
        }

        /// The exact count of base units per source unit.
        pub fn factor(&self, unit: &str) -> Option<u64> {
            self.factors.get(unit).copied()
        }

        pub fn units(&self) -> impl Iterator<Item = &String> {
            self.factors.keys()
        }

        /// Normalize one written `DURATION` to its exact base-unit magnitude.
        ///
        /// `None` for a unit the profile does not register, which is a unit outside
        /// the Time category and therefore not a legal `DURATION` unit.
        pub fn normalize(&self, magnitude: &Decimal, unit: &str) -> Option<Decimal> {
            let factor = self.factor(unit)?;
            Some(magnitude.mul(&Decimal::from_integer(Integer::from_u64(factor))))
        }

        /// Build the normalized `DURATION` value for a written magnitude and unit.
        pub fn duration(&self, magnitude: &Decimal, unit: &str) -> Option<Value> {
            Some(Value::Quantity(
                self.normalize(magnitude, unit)?,
                UnitId(DURATION_UNIT.to_string()),
            ))
        }
    }

    /// True when a value is a normalized `DURATION` rather than a `MEASURE`.
    pub fn is_duration(value: &Value) -> bool {
        matches!(value, Value::Quantity(_, unit) if unit.0 == DURATION_UNIT)
    }

    /// The canonical order key of one value, when it has one.
    ///
    /// `None` for a value the profile does not order. `non_material_ordering`:
    /// "MISSING and UNKNOWN are not orderable values."
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum OrderKey {
        /// INTEGER, DECIMAL, PERCENTAGE, BYTES, DURATION and same-unit MEASURE all
        /// order by exact mathematical magnitude. `cross_type_numeric_order`:
        /// "INTEGER and DECIMAL compare by exact mathematical value after
        /// INTEGER-to-DECIMAL promotion."
        Number(Decimal),
        /// `string_order`: "lexicographic by Unicode scalar value".
        Text(String),
        /// A signed exact temporal magnitude: days for DATE, nanoseconds from a
        /// common nominal local midnight for TIME, nanoseconds from the Unix epoch
        /// for DATETIME.
        Temporal(i128),
    }

    impl OrderKey {
        /// Compare two keys of the same kind.
        ///
        /// `None` when the kinds differ, which the caller reports rather than
        /// guessing an order across families.
        pub fn compare(&self, other: &OrderKey) -> Option<Ordering> {
            match (self, other) {
                (OrderKey::Number(a), OrderKey::Number(b)) => Some(a.compare(b)),
                (OrderKey::Text(a), OrderKey::Text(b)) => {
                    // Rust's `str` ordering is by Unicode scalar value, which is
                    // exactly what the registry requires. No locale is consulted.
                    Some(a.chars().cmp(b.chars()))
                }
                (OrderKey::Temporal(a), OrderKey::Temporal(b)) => Some(a.cmp(b)),
                _ => None,
            }
        }
    }

    /// The canonical order key of `value`, if the profile orders its family.
    pub fn order_key(value: &Value) -> Option<OrderKey> {
        match value {
            Value::Integer(d)
            | Value::Decimal(d)
            | Value::Percentage(d)
            | Value::Bytes(d)
            | Value::Quantity(d, _) => Some(OrderKey::Number(d.clone())),
            Value::Text(t) => Some(OrderKey::Text(t.clone())),
            Value::Constructed { constructor, text } => match constructor.as_str() {
                "DATE" => date_key(text).map(OrderKey::Temporal),
                "TIME" => time_key(text).map(OrderKey::Temporal),
                "DATETIME" => datetime_key(text).map(OrderKey::Temporal),
                // PATH, URI, GLOB and REGEX are not registered ordered types.
                _ => None,
            },
            // "MISSING and UNKNOWN are not orderable values." Neither are BOOLEAN,
            // collections, objects, references, NULL or identifiers: none is a
            // registered ordered type.
            _ => None,
        }
    }

    /// True when two values are mutually order-compatible.
    ///
    /// `05_SEMANTICS/08`: direct `SET` iteration "is legal only when every pair of
    /// actual members is mutually order-compatible under the registered
    /// total-order profile". A same-unit `MEASURE` requirement is part of that:
    /// "MEASURE values are order-compatible only with the same exact UNIT".
    pub fn order_compatible(left: &Value, right: &Value) -> bool {
        if let (Value::Quantity(_, a), Value::Quantity(_, b)) = (left, right) {
            if a != b {
                return false;
            }
        }
        // A quantity is order-compatible only with another quantity: a bare number
        // and a MEASURE are different families and the profile pairs neither.
        if matches!(left, Value::Quantity(_, _)) != matches!(right, Value::Quantity(_, _)) {
            return false;
        }
        match (order_key(left), order_key(right)) {
            (Some(a), Some(b)) => a.compare(&b).is_some(),
            _ => false,
        }
    }

    /// Compare two values under the registered profile.
    ///
    /// `None` when the profile does not order this pair, which the caller turns
    /// into the exact registered diagnostic rather than an invented order.
    pub fn compare(left: &Value, right: &Value) -> Option<Ordering> {
        if !order_compatible(left, right) {
            return None;
        }
        order_key(left)?.compare(&order_key(right)?)
    }

    // ---------------------------------------------------------------------------
    // Temporal order keys
    // ---------------------------------------------------------------------------
    //
    // The lexer already proved every temporal literal well formed against
    // `types_v0.1.0.json#/temporal_literal_contract`, so these functions compute an
    // order key from validated text. They do not re-validate, and they return
    // `None` rather than guessing if the text is not the shape the lexer admits.

    const NANOS_PER_SECOND: i128 = 1_000_000_000;
    const NANOS_PER_MINUTE: i128 = 60 * NANOS_PER_SECOND;
    const NANOS_PER_HOUR: i128 = 60 * NANOS_PER_MINUTE;
    const NANOS_PER_DAY: i128 = 24 * NANOS_PER_HOUR;

    /// "chronological Gregorian value of the RFC 3339 full-date".
    ///
    /// The key is days from the proleptic Gregorian epoch, which is order-faithful
    /// and exact.
    fn date_key(text: &str) -> Option<i128> {
        let (year, month, day) = parse_full_date(text)?;
        Some(days_from_civil(year, month, day))
    }

    fn parse_full_date(text: &str) -> Option<(i64, u32, u32)> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year: i64 = text.get(0..4)?.parse().ok()?;
        let month: u32 = text.get(5..7)?.parse().ok()?;
        let day: u32 = text.get(8..10)?.parse().ok()?;
        Some((year, month, day))
    }

    /// Days from 1970-01-01 in the proleptic Gregorian calendar.
    ///
    /// The standard civil-from-days algorithm, exact for every year the RFC 3339
    /// full-date admits.
    fn days_from_civil(year: i64, month: u32, day: u32) -> i128 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let m = month as i64;
        let d = day as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        (era as i128) * 146_097 + doe as i128 - 719_468
    }

    /// "subtract the declared offset, or +00:00 when omitted, from the RFC 3339
    /// partial-time on a common nominal local date; retain signed previous-day or
    /// next-day displacement without modulo-24-hour wrapping".
    fn time_key(text: &str) -> Option<i128> {
        let (local, offset) = split_offset(text)?;
        Some(partial_time_nanos(local)? - offset)
    }

    /// "exact UTC instant after applying the RFC 3339 declared offset, or +00:00
    /// when omitted".
    fn datetime_key(text: &str) -> Option<i128> {
        let separator = text.find(['T', 't'])?;
        let (date, rest) = text.split_at(separator);
        let (year, month, day) = parse_full_date(date)?;
        let (local, offset) = split_offset(&rest[1..])?;
        let days = days_from_civil(year, month, day);
        Some(days * NANOS_PER_DAY + partial_time_nanos(local)? - offset)
    }

    /// Split a partial-time from its offset, returning the offset in nanoseconds.
    ///
    /// An omitted offset is `+00:00`, per the rule above.
    fn split_offset(text: &str) -> Option<(&str, i128)> {
        if let Some(local) = text.strip_suffix(['Z', 'z']) {
            return Some((local, 0));
        }
        // A signed offset is the last `+HH:MM` or `-HH:MM`.
        if text.len() >= 6 {
            let (local, tail) = text.split_at(text.len() - 6);
            let sign = match tail.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return Some((text, 0)),
            };
            if tail.as_bytes().get(3) != Some(&b':') {
                return Some((text, 0));
            }
            let hours: i128 = tail.get(1..3)?.parse().ok()?;
            let minutes: i128 = tail.get(4..6)?.parse().ok()?;
            return Some((
                local,
                sign * (hours * NANOS_PER_HOUR + minutes * NANOS_PER_MINUTE),
            ));
        }
        Some((text, 0))
    }

    /// `HH:MM:SS` with an optional fractional part, in nanoseconds from midnight.
    ///
    /// Leap second `60` is admitted because the lexer admits it; it orders after
    /// `59`, which is its chronological position.
    fn partial_time_nanos(text: &str) -> Option<i128> {
        let (clock, fraction) = match text.split_once('.') {
            Some((clock, fraction)) => (clock, Some(fraction)),
            None => (text, None),
        };
        let bytes = clock.as_bytes();
        if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
            return None;
        }
        let hours: i128 = clock.get(0..2)?.parse().ok()?;
        let minutes: i128 = clock.get(3..5)?.parse().ok()?;
        let seconds: i128 = clock.get(6..8)?.parse().ok()?;
        let mut nanos =
            hours * NANOS_PER_HOUR + minutes * NANOS_PER_MINUTE + seconds * NANOS_PER_SECOND;
        if let Some(fraction) = fraction {
            if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            // Exact: pad or truncate to nanosecond precision. The lexer bounds the
            // fractional length, so this cannot silently lose a significant digit
            // the language admits.
            let mut digits = fraction.to_string();
            digits.truncate(9);
            while digits.len() < 9 {
                digits.push('0');
            }
            nanos += digits.parse::<i128>().ok()?;
        }
        Some(nanos)
    }
}
