//! Registry defaults, declared bounds, and what an address resolves to.
//!
//! Authority: `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt`.
//!
//! ## Three rules, kept apart on purpose
//!
//! > No positional parameters exist. A parameter absent from an operation
//! > contract is forbidden. A required parameter cannot be inferred. A default
//! > applies only to a MISSING optional parameter.
//!
//! and, about which stage owns which failure:
//!
//! > Every operation lists error.operation.parameter; that error is emitted
//! > during static/expression checking when an ACTION, HANDLER, or FALLBACK
//! > invocation site omits TARGET while the selected operation marks it
//! > required, omits a named parameter that the operation marks required,
//! > supplies any positional argument, duplicates a named parameter, or
//! > supplies an unregistered named parameter.
//!
//! > A declared value outside a numeric bound such as core.inspect depth or
//! > core.retry limit uses error.value.out_of_range, not
//! > error.operation.parameter.
//!
//! So the *shape* of an invocation is already settled — `lcl-checker` rejected
//! a missing required parameter long before this runs — and what is left here
//! is what only a runtime can do: supply defaults for what is MISSING now, and
//! check the values that only exist now against their declared bounds.

use crate::contracts::OperationContract;
use lcl_capabilities::AddressClass;
use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::operations::{Invocation, Resolution};
use lcl_runtime::Value;
use std::collections::BTreeMap;

/// Apply every registered default to the parameters one invocation supplied.
///
/// > A non-null default is the exact declared value applied only when an
/// > optional parameter is MISSING. A required parameter never acquires a
/// > default.
///
/// A parameter the invocation supplied as `MISSING` is treated as absent, which
/// is what "only when an optional parameter is MISSING" says: MISSING is the
/// absence, not a value that overrides the default.
pub fn with_defaults(
    contract: &OperationContract,
    supplied: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut resolved = supplied.clone();
    for (name, parameter) in &contract.parameters {
        if parameter.required {
            continue;
        }
        let Some(default) = parameter.default.clone() else {
            continue;
        };
        let absent = match resolved.get(name) {
            None => true,
            Some(Value::Missing) => true,
            Some(_) => false,
        };
        if absent {
            resolved.insert(name.clone(), default);
        }
    }
    resolved
}

/// Check every declared numeric bound the invocation's values violate.
///
/// Returns the first violation as a resolved failure, because a bound is a
/// pre-effect check and one violated bound is enough to refuse the invocation.
pub fn check_bounds(
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Option<Resolution> {
    for (name, parameter) in &contract.parameters {
        let Some(bound) = parameter.bound else {
            continue;
        };
        let Some(value) = parameters.get(name) else {
            continue;
        };
        let number = match value {
            Value::Integer(number) => number.to_i64(),
            // A non-integer under an INTEGER bound is a type question, and the
            // checker owns it. Nothing to say here.
            _ => continue,
        };
        match number {
            Some(number) if bound.admits(number) => {}
            Some(number) => {
                return Some(Resolution::failed(
                    RuntimeError::ValueOutOfRange,
                    "declared_bound",
                    format!(
                        "{} {name} is {number}, outside the declared bound {bound}",
                        contract.operation
                    ),
                ))
            }
            None => {
                return Some(Resolution::failed(
                    RuntimeError::ValueOutOfRange,
                    "declared_bound",
                    format!(
                        "{} {name} is outside the declared bound {bound}",
                        contract.operation
                    ),
                ))
            }
        }
    }
    None
}

/// One required parameter that is MISSING at demand.
///
/// The checker rejects an *omitted* required parameter statically. What it
/// cannot see is a supplied expression that evaluates to MISSING now, which
/// `05_SEMANTICS/06` sends to `error.required.missing`.
pub fn check_required(
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Option<Resolution> {
    for (name, parameter) in &contract.parameters {
        if !parameter.required {
            continue;
        }
        match parameters.get(name) {
            Some(Value::Missing) => {
                return Some(Resolution::failed(
                    RuntimeError::RequiredMissing,
                    "required_parameter",
                    format!(
                        "{} requires {name}, which resolved MISSING",
                        contract.operation
                    ),
                ))
            }
            Some(Value::Unknown) => {
                return Some(Resolution::failed(
                    RuntimeError::ValueUnknown,
                    "required_parameter",
                    format!(
                        "{} requires {name}, which resolved UNKNOWN",
                        contract.operation
                    ),
                ))
            }
            _ => {}
        }
    }
    None
}

/// The address class one resolved value belongs to.
///
/// > A REFERENCE used as an address is classified by the address class of its
/// > resolved target, never by REFERENCE syntax.
///
/// A reference is therefore followed to its declaration before anything is
/// decided, and the declaring block is what answers.
pub fn classify(cx: &Invocation<'_>, value: &Value) -> AddressClass {
    match value {
        Value::Constructed { constructor, .. } => match constructor.as_str() {
            "PATH" => AddressClass::Path,
            "URI" => AddressClass::Uri,
            _ => AddressClass::Material,
        },
        Value::WorkspacePath { .. } => AddressClass::Path,
        Value::Reference(id) => classify_reference(cx, id),
        _ => AddressClass::Material,
    }
}

/// The class of whatever a reference resolves to.
fn classify_reference(cx: &Invocation<'_>, id: &str) -> AddressClass {
    // A loop-local reference identity carries its instance path; the
    // declaration it names is the part before it.
    let declaration_id = id.split('#').next().unwrap_or(id);
    match declaring_block(cx, declaration_id).as_deref() {
        Some("OUTPUT") => AddressClass::Output,
        Some("MEMORY") => AddressClass::Memory,
        Some("STATE") => AddressClass::State,
        // Any other declaration is classified by the value it holds, so a DATA
        // holding PATH("/srv/x") is a filesystem address and a DATA holding a
        // number is not an address at all.
        _ => match cx.declaration_value(declaration_id) {
            Value::Constructed { constructor, .. } if constructor == "PATH" => AddressClass::Path,
            Value::WorkspacePath { .. } => AddressClass::Path,
            Value::Constructed { constructor, .. } if constructor == "URI" => AddressClass::Uri,
            _ => AddressClass::Material,
        },
    }
}

/// The block that declares one qualified id.
pub fn declaring_block(cx: &Invocation<'_>, id: &str) -> Option<String> {
    cx.resolved
        .declarations()
        .all()
        .iter()
        .find(|d| d.id.qualified() == id)
        .map(|d| d.block.clone())
}

/// Whether one reference names a declaration of the given block.
pub fn references_block(cx: &Invocation<'_>, value: &Value, block: &str) -> bool {
    match value {
        Value::Reference(id) => {
            let id = id.split('#').next().unwrap_or(id);
            declaring_block(cx, id).as_deref() == Some(block)
        }
        _ => false,
    }
}

/// The declaration id one reference names, without its instance path.
pub fn reference_id(value: &Value) -> Option<&str> {
    match value {
        Value::Reference(id) => Some(id.split('#').next().unwrap_or(id)),
        _ => None,
    }
}

/// The declaration id one field of the invocation site names with `REF(...)`.
///
/// Some rows are stated over the *reference* a field names rather than the
/// value it reads: `core.memory_write` and `core.state_update` take
/// `REFERENCE[MEMORY]` and `REFERENCE[STATE]` targets, and which declaration is
/// being written cannot be recovered from the value that reading it produced.
/// This reads the invocation site's own syntax, which is where that identity
/// still exists.
pub fn referenced_declaration(cx: &Invocation<'_>, field: &str) -> Option<String> {
    let block = lcl_runtime::syntax::declaration_block(cx.resolved, cx.declaration?)?;
    let expr = lcl_runtime::syntax::field_expr(&block, field)?;
    match expr {
        lcl_parser::syntax::Expr::Call(call) => Some(call.reference_target()?.text.clone()),
        _ => None,
    }
}

/// The declaration index one qualified id names.
pub fn declaration_index(cx: &Invocation<'_>, id: &str) -> Option<usize> {
    cx.resolved
        .declarations()
        .all()
        .iter()
        .position(|d| d.id.qualified() == id)
}

/// Whether a property path is declared by the type of one operation field.
///
/// `06_STANDARD_LIBRARY/10` separates two key defects: "a malformed or
/// unregistered path uses error.operation.precondition", while "a key result of
/// MISSING produces error.required.missing". A path the declared member type
/// defines is registered, so a member that carries no value there has a MISSING
/// key result rather than an unregistered path. A field whose declaration this
/// cannot read answers `false`, leaving the row's own precondition in place.
pub fn declared_member_path(cx: &Invocation<'_>, field: &str, path: &str) -> bool {
    use lcl_checker::ty::Type;
    let Some(declared) = referenced_declaration(cx, field)
        .and_then(|id| declaration_index(cx, &id))
        .and_then(|index| cx.checked.declaration_type(index))
    else {
        return false;
    };
    let mut current = match declared {
        Type::List(member) | Type::Set(member) => member.as_ref(),
        other => other,
    };
    for segment in path.split('.') {
        let Type::Object(schema) = current else {
            return false;
        };
        let Some(field) = schema.field(segment) else {
            return false;
        };
        current = &field.ty;
    }
    true
}

/// Whether one runtime value satisfies a declared type.
///
/// `05_SEMANTICS/05` and the store rows state the obligation this answers:
/// "when merge is FALSE, value matches the declared MEMORY type" and "value
/// type matches". The comparison is by registered family, with LIST and SET
/// members and OBJECT fields compared the same way, because that is exactly
/// what the declared type fixes. A type this function cannot decide — a
/// REFERENCE target, an ENUM domain resolved elsewhere — is not rejected:
/// inventing a refusal would be inventing language, and the declared-type
/// checks those forms already have own them.
pub fn value_matches(declared: &lcl_checker::ty::Type, value: &Value) -> bool {
    use lcl_checker::ty::Type;
    // "MISSING, UNKNOWN and NULL are not material values"; a store write of a
    // sentinel is decided by the row's own required-value rules, not here.
    if !value.is_material() {
        return true;
    }
    match (declared, value) {
        (Type::String, Value::Text(_)) => true,
        (Type::Integer, Value::Integer(_)) => true,
        // "INTEGER/DECIMAL numeric promotion".
        (Type::Decimal, Value::Integer(_) | Value::Decimal(_)) => true,
        (Type::Boolean, Value::Boolean(_)) => true,
        (Type::Bytes, Value::Bytes(_)) => true,
        (Type::Percentage, Value::Percentage(_)) => true,
        (Type::Measure(_), Value::Quantity(_, _)) => true,
        (Type::Path, Value::Constructed { constructor, .. }) => constructor == "PATH",
        (Type::Path, Value::WorkspacePath { .. }) => true,
        (Type::Uri, Value::Constructed { constructor, .. }) => constructor == "URI",
        (Type::Glob, Value::Constructed { constructor, .. }) => constructor == "GLOB",
        (Type::Regex, Value::Constructed { constructor, .. }) => constructor == "REGEX",
        (Type::Date, Value::Constructed { constructor, .. }) => constructor == "DATE",
        (Type::Time, Value::Constructed { constructor, .. }) => constructor == "TIME",
        (Type::Datetime, Value::Constructed { constructor, .. }) => constructor == "DATETIME",
        (Type::Duration, Value::Quantity(_, _)) => true,
        (Type::List(member), Value::List(items)) | (Type::Set(member), Value::Set(items)) => {
            items.iter().all(|item| value_matches(member, item))
        }
        (Type::ObjectFamily, Value::Object(_)) => true,
        (Type::Object(schema), Value::Object(fields)) => {
            // Every required field is present, no field is unknown, and each
            // present field satisfies its own declared type.
            schema
                .fields()
                .iter()
                .all(|(name, field)| match fields.get(name) {
                    Some(value) => value_matches(&field.ty, value),
                    None => !field.required,
                })
                && fields.keys().all(|name| schema.field(name).is_some())
        }
        // A declared type this comparison does not decide.
        (Type::Enum(_) | Type::Reference(_) | Type::Null, _) => true,
        _ => false,
    }
}
