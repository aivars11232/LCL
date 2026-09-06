//! The static type model.
//!
//! `03_TYPES_AND_VALUES/01_TYPE_SYSTEM_RULES.txt`: "Every value has exactly one
//! static type." [`Type`] is that one type, and nothing else: no "any", no
//! union, no unknown-type placeholder, no error type. Where a static type
//! cannot be determined, the checker records an outcome that is not a type
//! ([`Static`]) rather than inventing one, so a later stage can never mistake a
//! repair for a decision.
//!
//! ## Identity rules this model encodes
//!
//! * **Aliases are transparent.** A `kind.type` alias of `INTEGER` *is*
//!   [`Type::Integer`]; it is resolved away before it reaches this model, so no
//!   nominal subtype can exist ("It creates no nominal subtype, new material
//!   value, implicit conversion, refinement, or additional operator overload").
//! * **Enums are nominal.** "The enum domain is the defining declaration
//!   identity; a transparent type alias preserves it", so [`EnumDomain`] holds
//!   that declaration's exact [`FullId`] and two equal spellings in different
//!   domains are different types.
//! * **Objects are structural.** `types_v0.1.0.json#/object_type_contract`:
//!   "recursively equal field-name/type/requiredness maps identify the same
//!   object type regardless of defining ID or declaration order", and
//!   "Requiredness is part of type identity". Defaults and constraints are
//!   deliberately absent from [`ObjectType`]: they "govern construction and
//!   validation, not object type identity".
//! * **MEASURE carries its exact unit when one is statically known.** "MEASURE
//!   equality requires the same exact UNIT", and every same-unit overload
//!   compares unit identifiers, so the unit belongs to the static type. A
//!   source `TYPE: MEASURE` field pins no unit and yields
//!   [`Type::Measure(None)`].
//! * **REFERENCE is a material type distinct from its referent's value type.**

use lcl_resolver::FullId;
use std::collections::BTreeMap;
use std::fmt;

/// One registered unit identifier, e.g. `unit.meter`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub String);

impl UnitId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UnitId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A concrete enum domain: the identity of the `DEFINE BASE ENUM` that
/// introduced it, plus its closed member set in declaration order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EnumDomain {
    /// The defining declaration's exact identity. Domain identity is this, and
    /// only this.
    pub id: FullId,
    /// Its `ITEM` members, in declaration order. "Its member set and
    /// declaration order are closed", while "declaration order has no
    /// comparison, iteration, or sorting significance".
    pub items: Vec<String>,
}

impl EnumDomain {
    pub fn contains(&self, item: &str) -> bool {
        self.items.iter().any(|i| i == item)
    }
}

/// One field of a structural object type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectField {
    pub ty: Type,
    /// Part of type identity, per `object_type_contract`.
    pub required: bool,
}

/// A structural object type: the exact field-name/type/requiredness map.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectType {
    fields: BTreeMap<String, ObjectField>,
}

impl ObjectType {
    pub fn new(fields: BTreeMap<String, ObjectField>) -> Self {
        ObjectType { fields }
    }

    pub fn fields(&self) -> &BTreeMap<String, ObjectField> {
        &self.fields
    }

    pub fn field(&self, name: &str) -> Option<&ObjectField> {
        self.fields.get(name)
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}

/// What a `REFERENCE[...]` constrains.
///
/// `types_v0.1.0.json#/source_type_contract/reference`: "REFERENCE[REF(identifier)]
/// constrains the referenced declaration identity, or the value type when
/// identifier names kind.type."
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefTarget {
    /// The referenced declaration's exact identity.
    Declaration(FullId),
    /// A reference to a value of this type, written `REFERENCE[REF(type.id)]`.
    Value(Box<Type>),
    /// A reference constrained only by a receiving contract, never by source
    /// syntax: the registries' bare `REFERENCE` contract atom.
    Any,
}

/// One static type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    String,
    Integer,
    Decimal,
    Boolean,
    Null,
    Path,
    Uri,
    Glob,
    Regex,
    Date,
    Time,
    Datetime,
    Duration,
    Percentage,
    Bytes,
    /// `MEASURE`, with its exact unit when one is statically known.
    Measure(Option<UnitId>),
    List(Box<Type>),
    Set(Box<Type>),
    Object(ObjectType),
    Enum(EnumDomain),
    Reference(Box<RefTarget>),
}

impl Type {
    /// The built-in scalar named by one `SCALAR_TYPE` word, if that word names
    /// one. `OBJECT` and `ENUM` are deliberately absent: bare `OBJECT` "denotes
    /// an object family at a receiving contract, not an additional nominal
    /// identity", and bare `ENUM` "cannot declare an unconstrained material
    /// value".
    pub fn scalar(word: &str) -> Option<Type> {
        Some(match word {
            "STRING" => Type::String,
            "INTEGER" => Type::Integer,
            "DECIMAL" => Type::Decimal,
            "BOOLEAN" => Type::Boolean,
            "NULL" => Type::Null,
            "PATH" => Type::Path,
            "URI" => Type::Uri,
            "GLOB" => Type::Glob,
            "REGEX" => Type::Regex,
            "DATE" => Type::Date,
            "TIME" => Type::Time,
            "DATETIME" => Type::Datetime,
            "DURATION" => Type::Duration,
            "PERCENTAGE" => Type::Percentage,
            "BYTES" => Type::Bytes,
            "MEASURE" => Type::Measure(None),
            _ => return None,
        })
    }

    /// The registry row name of this type's family, for diagnostics and for
    /// checking a family against a registered contract atom.
    pub fn family(&self) -> &'static str {
        match self {
            Type::String => "STRING",
            Type::Integer => "INTEGER",
            Type::Decimal => "DECIMAL",
            Type::Boolean => "BOOLEAN",
            Type::Null => "NULL",
            Type::Path => "PATH",
            Type::Uri => "URI",
            Type::Glob => "GLOB",
            Type::Regex => "REGEX",
            Type::Date => "DATE",
            Type::Time => "TIME",
            Type::Datetime => "DATETIME",
            Type::Duration => "DURATION",
            Type::Percentage => "PERCENTAGE",
            Type::Bytes => "BYTES",
            Type::Measure(_) => "MEASURE",
            Type::List(_) => "LIST",
            Type::Set(_) => "SET",
            Type::Object(_) => "OBJECT",
            Type::Enum(_) => "ENUM",
            Type::Reference(_) => "REFERENCE",
        }
    }

    /// `numeric`: "INTEGER or DECIMAL only."
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Integer | Type::Decimal)
    }

    pub fn is_collection(&self) -> bool {
        matches!(self, Type::List(_) | Type::Set(_))
    }

    /// The member type of a collection.
    pub fn member(&self) -> Option<&Type> {
        match self {
            Type::List(t) | Type::Set(t) => Some(t),
            _ => None,
        }
    }

    /// Exact type identity, with one deliberate accommodation: a `MEASURE`
    /// whose unit is not statically pinned is the same *type* as one whose unit
    /// is, because a source `TYPE: MEASURE` field pins none. Unit *equality* is
    /// a separate check every same-unit overload performs explicitly, and is
    /// never decided here.
    pub fn accepts(&self, value: &Type) -> bool {
        match (self, value) {
            (Type::Measure(None), Type::Measure(_)) | (Type::Measure(_), Type::Measure(None)) => {
                true
            }
            (Type::List(a), Type::List(b)) | (Type::Set(a), Type::Set(b)) => a.accepts(b),
            (Type::Reference(a), Type::Reference(b)) => match (a.as_ref(), b.as_ref()) {
                (RefTarget::Any, _) | (_, RefTarget::Any) => true,
                (RefTarget::Value(x), RefTarget::Value(y)) => x.accepts(y),
                (x, y) => x == y,
            },
            _ => self == value,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Measure(Some(unit)) => write!(f, "MEASURE({unit})"),
            Type::List(t) => write!(f, "LIST[{t}]"),
            Type::Set(t) => write!(f, "SET[{t}]"),
            Type::Enum(domain) => write!(f, "ENUM[{}]", domain.id),
            Type::Object(object) => {
                f.write_str("OBJECT{")?;
                for (index, (name, field)) in object.fields().iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{name}: {}", field.ty)?;
                    if !field.required {
                        f.write_str("?")?;
                    }
                }
                f.write_str("}")
            }
            Type::Reference(target) => match target.as_ref() {
                RefTarget::Declaration(id) => write!(f, "REFERENCE[{id}]"),
                RefTarget::Value(t) => write!(f, "REFERENCE[{t}]"),
                RefTarget::Any => f.write_str("REFERENCE"),
            },
            other => f.write_str(other.family()),
        }
    }
}
