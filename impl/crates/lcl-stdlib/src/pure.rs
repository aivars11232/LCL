//! The seven rows that need nothing outside the document.
//!
//! `core.calculate`, `core.select`, `core.filter`, `core.sort`, `core.group`
//! and `core.return` all declare `possible_dependencies` exactly
//! `declared_state_only` and `possible_effects` exactly `none`. They are
//! computed here, from declared values, and no host is asked anything — which
//! is not a performance choice but the row's own contract:
//!
//! > core.select, core.filter, core.sort, and core.group retain possible
//! > dependencies exactly {declared_state_only}. Any predicate or key REFERENCE
//! > used by those rows must therefore resolve to a deterministic
//! > kind.operation with SIDE_EFFECT FALSE and a fully resolved dependency set
//! > exactly {declared_state_only}; an external dependency cannot be hidden
//! > behind the reference.
//!
//! ## Where a referenced key or predicate operation comes from
//!
//! A `DEFINE kind.operation` is a contract without a body — the canonical
//! example declares parameters, axes and a result type and no implementation —
//! so a predicate or key REFERENCE names something this document does not
//! define. The registry answers what to do when its implementation is absent:
//!
//! > invalid key-operation axes, or a missing, ambiguous, incomplete, or
//! > out-of-bounds key-operation profile use error.operation.precondition.
//!
//! So an embedder installs the implementation with
//! [`crate::Stdlib::with_pure_operation`], its declared axes are checked before
//! it is used, and an absent one is a precondition failure before effects
//! rather than an invented answer.

use crate::contracts::OperationContract;
use crate::fragment::{self, Environment, FragmentFault};
use crate::{params, schema, Stdlib};
use lcl_runtime::capability::CapabilityRequest;
use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::operations::{Invocation, Resolution};
use lcl_runtime::{order_profile, strict_equal, Value};
use std::collections::BTreeMap;

/// Resolve one invocation of a pure row.
pub(crate) fn invoke(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    match contract.operation.as_str() {
        "core.return" => ret(cx, request),
        "core.calculate" => calculate(stdlib, cx, request, contract, parameters),
        "core.select" | "core.filter" => {
            select_or_filter(stdlib, cx, request, contract, parameters)
        }
        "core.sort" => sort(stdlib, cx, request, contract, parameters),
        "core.group" => group(stdlib, cx, request, contract, parameters),
        // Unreachable: `family` routes exactly the rows above here.
        other => Resolution::failed(
            RuntimeError::OperationPrecondition,
            "dispatch",
            format!("{other} is not a pure row"),
        ),
    }
}

/// `core.return`: "Return an exact declared value to the invoking context."
///
/// > core.return distinguishes an unresolved REFERENCE from one resolved to a
/// > non-material sentinel: unresolved uses error.reference.unresolved, MISSING
/// > uses error.required.missing, and UNKNOWN uses error.value.unknown.
fn ret(cx: &Invocation<'_>, request: &CapabilityRequest) -> Resolution {
    let Some(target) = request.target.as_ref() else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            "core.return requires a TARGET",
        );
    };
    let value = read_through(cx, target);
    match value {
        Value::Missing => Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            "core.return resolved a MISSING target",
        ),
        Value::Unknown => Resolution::failed(
            RuntimeError::ValueUnknown,
            "target",
            "core.return resolved an UNKNOWN target",
        ),
        value => Resolution::Completed(schema::value(value)),
    }
}

/// `core.calculate`: evaluate one declared expression fragment.
fn calculate(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let expression = match fragment_text(stdlib, cx, contract, parameters, "expression") {
        Ok(text) => text,
        Err(resolution) => return resolution,
    };

    // "The reserved target binding is the resolved TARGET value, or MISSING
    // when TARGET is omitted."
    let target = request
        .target
        .as_ref()
        .map(|value| read_through(cx, value))
        .unwrap_or(Value::Missing);

    let bindings = match binding_object(stdlib, cx, contract, parameters, &["target"]) {
        Ok(bindings) => bindings,
        Err(resolution) => return resolution,
    };

    let environment = Environment {
        reserved: vec![("target", target)],
        bindings,
    };
    let source = fragment::fragment_source(&contract.operation, "expression");
    match fragment::evaluate(
        cx,
        stdlib.lexicon(),
        stdlib.grammar(),
        &source,
        &expression,
        &environment,
    ) {
        // "a required final MISSING result uses error.required.missing and
        // UNKNOWN uses error.value.unknown"
        Ok(Value::Missing) => Resolution::failed(
            RuntimeError::RequiredMissing,
            "expression",
            "the calculation resolved MISSING, and result.value content is required",
        ),
        Ok(Value::Unknown) => Resolution::failed(
            RuntimeError::ValueUnknown,
            "expression",
            "the calculation resolved UNKNOWN, and result.value content is material",
        ),
        Ok(value) => Resolution::Completed(schema::value(value)),
        Err(fault) => fragment_failure(contract, &fault),
    }
}

/// `core.select` and `core.filter`: retain the members a predicate accepts.
///
/// > core.filter and core.group accept LIST[T], not SET[T]. This preserves
/// > their deterministic result ordering without assigning source order to an
/// > intrinsically unordered SET. … passing SET directly to either operation
/// > produces error.type.mismatch before effects.
fn select_or_filter(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let Some(target) = request.target.as_ref().map(|t| read_through(cx, t)) else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            format!("{} requires a TARGET collection", contract.operation),
        );
    };
    let members = match members_of(&target, &contract.operation) {
        Ok(members) => members,
        Err(resolution) => return resolution,
    };

    let mut kept = Vec::new();
    for member in members {
        match predicate_holds(stdlib, cx, contract, parameters, &member) {
            Ok(true) => kept.push(member),
            Ok(false) => {}
            Err(resolution) => return resolution,
        }
    }
    Resolution::Completed(schema::collection(kept))
}

/// `core.sort`: one deterministic order over the target members.
fn sort(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let Some(target) = request.target.as_ref().map(|t| read_through(cx, t)) else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            "core.sort requires a TARGET collection",
        );
    };
    // "The result value and result.collection items are LIST[T]." A SET target
    // is admitted here — unlike core.filter — because sorting is exactly how a
    // SET "must first be given an explicit deterministic LIST order".
    let (members, from_set) = match &target {
        Value::List(members) => (members.clone(), false),
        Value::Set(members) => (members.clone(), true),
        other => {
            return Resolution::failed(
                RuntimeError::TypeMismatch,
                "target",
                format!(
                    "core.sort requires LIST[T] or SET[T], found {}",
                    other.family()
                ),
            )
        }
    };

    // Each member's ordering key, in source order.
    let mut keyed: Vec<(usize, Value, Value)> = Vec::new();
    for (position, member) in members.into_iter().enumerate() {
        let key = match key_of(stdlib, cx, contract, parameters, &member, "key") {
            Ok(Some(key)) => key,
            // "key may be omitted only for mutually order-compatible target
            // members"; the member is then its own key.
            Ok(None) => member.clone(),
            Err(resolution) => return resolution,
        };
        keyed.push((position, key, member));
    }

    // "Every key value is present, known, and mutually order-compatible."
    for (_, key, _) in &keyed {
        match key {
            Value::Missing => {
                return Resolution::failed(
                    RuntimeError::RequiredMissing,
                    "key",
                    "core.sort resolved a MISSING key",
                )
            }
            Value::Unknown => {
                return Resolution::failed(
                    RuntimeError::ValueUnknown,
                    "key",
                    "core.sort resolved an UNKNOWN key",
                )
            }
            _ => {}
        }
    }
    for window in keyed.windows(2) {
        if !order_profile::order_compatible(&window[0].1, &window[1].1) {
            return Resolution::failed(
                RuntimeError::OperationPrecondition,
                "key",
                format!(
                    "core.sort keys {} and {} are not mutually order-compatible",
                    window[0].1.family(),
                    window[1].1.family()
                ),
            );
        }
    }
    // "distinct SET members produce distinct keys"; equal keys for distinct SET
    // members "use error.operation.precondition".
    if from_set {
        for (index, (_, key, _)) in keyed.iter().enumerate() {
            if keyed[..index]
                .iter()
                .any(|(_, seen, _)| strict_equal(seen, key))
            {
                return Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "key",
                    "core.sort resolved equal keys for distinct SET members",
                );
            }
        }
    }

    let descending = matches!(
        parameters.get("direction"),
        Some(Value::Identifier(direction)) if direction == "descending"
    );

    // "descending reverses primary order but not original LIST source order
    // among equal-key members", so the sort stays stable in source order and
    // only the key comparison is reversed.
    let mut sorted = keyed;
    sorted.sort_by(|left, right| {
        let ordering =
            order_profile::compare(&left.1, &right.1).unwrap_or(std::cmp::Ordering::Equal);
        let ordering = if descending {
            ordering.reverse()
        } else {
            ordering
        };
        ordering.then(left.0.cmp(&right.0))
    });

    Resolution::Completed(schema::collection(
        sorted.into_iter().map(|(_, _, member)| member).collect(),
    ))
}

/// `core.group`: partition the members by their key, in first-occurrence order.
///
/// > result.value.value is a LIST of closed OBJECT records with exactly key and
/// > items, where items is LIST[T]. Strict == groups keys, first key occurrence
/// > orders the groups, and source order is preserved within each group. Empty
/// > input returns an empty LIST of that record schema.
fn group(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let Some(target) = request.target.as_ref().map(|t| read_through(cx, t)) else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            "core.group requires a TARGET collection",
        );
    };
    let members = match members_of(&target, &contract.operation) {
        Ok(members) => members,
        Err(resolution) => return resolution,
    };

    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for member in members {
        let key = match key_of(stdlib, cx, contract, parameters, &member, "key") {
            Ok(Some(key)) => key,
            Ok(None) => {
                return Resolution::failed(
                    RuntimeError::RequiredMissing,
                    "key",
                    "core.group requires a key",
                )
            }
            Err(resolution) => return resolution,
        };
        // "The same mapping applies to the required grouping key in
        // core.group": MISSING and UNKNOWN are refused, not grouped.
        match key {
            Value::Missing => {
                return Resolution::failed(
                    RuntimeError::RequiredMissing,
                    "key",
                    "core.group resolved a MISSING key",
                )
            }
            Value::Unknown => {
                return Resolution::failed(
                    RuntimeError::ValueUnknown,
                    "key",
                    "core.group resolved an UNKNOWN key",
                )
            }
            _ => {}
        }
        match groups.iter_mut().find(|(seen, _)| strict_equal(seen, &key)) {
            Some((_, items)) => items.push(member),
            None => groups.push((key, vec![member])),
        }
    }

    let records: Vec<Value> = groups
        .into_iter()
        .map(|(key, items)| {
            let mut record = BTreeMap::new();
            record.insert("key".to_string(), key);
            record.insert("items".to_string(), Value::List(items));
            Value::Object(record)
        })
        .collect();
    Resolution::Completed(schema::value(Value::List(records)))
}

// ---------------------------------------------------------------------------
// Shared resolution
// ---------------------------------------------------------------------------

/// The value behind a target, following a reference to its declaration.
///
/// An operation target may arrive as a retained reference identity when M4
/// annotated the site as one. Reading through it here is what
/// `reference_context_contract` calls a value read, which is what an operation
/// that consumes a value needs.
pub(crate) fn read_through(cx: &Invocation<'_>, value: &Value) -> Value {
    match value {
        Value::Reference(id) => {
            let id = id.split('#').next().unwrap_or(id);
            cx.declaration_value(id)
        }
        other => other.clone(),
    }
}

/// The members of a collection target, refusing a SET where the row forbids it.
pub(crate) fn members_of(target: &Value, operation: &str) -> Result<Vec<Value>, Resolution> {
    match target {
        Value::List(members) => Ok(members.clone()),
        // core.select takes meta.collection, which is "LIST[T] or SET[T]".
        Value::Set(members) if operation == "core.select" => Ok(members.clone()),
        Value::Set(_) => Err(Resolution::failed(
            RuntimeError::TypeMismatch,
            "target",
            format!(
                "{operation} accepts LIST[T], not SET[T]; give the SET an explicit \
                 deterministic order with core.sort first"
            ),
        )),
        other => Err(Resolution::failed(
            RuntimeError::TypeMismatch,
            "target",
            format!(
                "{operation} requires a collection, found {}",
                other.family()
            ),
        )),
    }
}

/// Whether the declared predicate accepts one member.
fn predicate_holds(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
    member: &Value,
) -> Result<bool, Resolution> {
    let outcome = match parameters.get("predicate") {
        // "A STRING follows expression_fragment_contract with the reserved item
        // binding and must produce BOOLEAN."
        Some(Value::Text(fragment_text)) => {
            let environment = Environment {
                reserved: vec![("item", member.clone())],
                bindings: BTreeMap::new(),
            };
            let source = fragment::fragment_source(&contract.operation, "predicate");
            fragment::evaluate(
                cx,
                stdlib.lexicon(),
                stdlib.grammar(),
                &source,
                fragment_text,
                &environment,
            )
            .map_err(|fault| fragment_failure(contract, &fault))?
        }
        Some(reference @ Value::Reference(_)) => {
            apply_referenced(stdlib, cx, contract, reference, member, "predicate")?
        }
        Some(Value::Missing) | None => {
            return Err(Resolution::failed(
                RuntimeError::RequiredMissing,
                "predicate",
                format!("{} requires a predicate", contract.operation),
            ))
        }
        Some(other) => {
            return Err(Resolution::failed(
                RuntimeError::TypeMismatch,
                "predicate",
                format!(
                    "{} requires a STRING or REFERENCE predicate, found {}",
                    contract.operation,
                    other.family()
                ),
            ))
        }
    };

    // "a required predicate result of MISSING produces error.required.missing
    // and UNKNOWN produces error.value.unknown"
    match outcome {
        Value::Boolean(held) => Ok(held),
        Value::Missing => Err(Resolution::failed(
            RuntimeError::RequiredMissing,
            "predicate",
            format!("{} resolved a MISSING predicate result", contract.operation),
        )),
        Value::Unknown => Err(Resolution::failed(
            RuntimeError::ValueUnknown,
            "predicate",
            format!(
                "{} resolved an UNKNOWN predicate result",
                contract.operation
            ),
        )),
        other => Err(Resolution::failed(
            RuntimeError::OperatorOperand,
            "predicate",
            format!(
                "{} requires a BOOLEAN predicate result, found {}",
                contract.operation,
                other.family()
            ),
        )),
    }
}

/// One member's ordering or grouping key, when the row declares one.
///
/// `Ok(None)` means the parameter is absent, which `core.sort` permits and
/// `core.group` does not.
fn key_of(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
    member: &Value,
    name: &str,
) -> Result<Option<Value>, Resolution> {
    match parameters.get(name) {
        None | Some(Value::Missing) => Ok(None),
        // "A STRING is exactly one property_path defined for every member;
        // malformed or statically unregistered paths produce
        // error.operation.precondition."
        Some(Value::Text(path)) => match property_path(member, path) {
            Some(value) => Ok(Some(value)),
            // "A key result of MISSING produces error.required.missing": a
            // path the declared member type defines is registered, so a member
            // that carries no value there resolves to MISSING and the caller's
            // own sentinel rule decides it.
            None if params::declared_member_path(cx, "TARGET", path) => Ok(Some(Value::Missing)),
            None => Err(Resolution::failed(
                RuntimeError::OperationPrecondition,
                name,
                format!(
                    "the property path {path:?} is not defined for every member of {}",
                    contract.operation
                ),
            )),
        },
        Some(reference @ Value::Reference(_)) => Ok(Some(apply_referenced(
            stdlib, cx, contract, reference, member, name,
        )?)),
        Some(other) => Err(Resolution::failed(
            RuntimeError::TypeMismatch,
            name,
            format!(
                "{} requires a STRING property path or a REFERENCE, found {}",
                contract.operation,
                other.family()
            ),
        )),
    }
}

/// Select one property path through a value.
///
/// A path is dotted field selection through OBJECT values. `None` means the
/// path is not defined here, which the caller reports as a precondition.
pub(crate) fn property_path(value: &Value, path: &str) -> Option<Value> {
    if path.trim().is_empty() {
        return None;
    }
    let mut current = value.clone();
    for segment in path.split('.') {
        let Value::Object(fields) = &current else {
            return None;
        };
        current = fields.get(segment)?.clone();
    }
    Some(current)
}

/// Apply a referenced pure operation to one member.
///
/// The referenced declaration's own axes are checked first — "an external
/// dependency cannot be hidden behind the reference" — and its implementation
/// must be installed, because a `DEFINE kind.operation` declares a contract and
/// no body.
fn apply_referenced(
    stdlib: &Stdlib,
    cx: &Invocation<'_>,
    contract: &OperationContract,
    reference: &Value,
    member: &Value,
    parameter: &str,
) -> Result<Value, Resolution> {
    let Some(id) = params::reference_id(reference) else {
        return Err(Resolution::failed(
            RuntimeError::ReferenceKind,
            parameter,
            format!("{} {parameter} is not a reference", contract.operation),
        ));
    };
    // A reference that resolves nowhere is a resolution-stage defect that M3
    // already decided, so this arm is totality. It selects the reference
    // identifier the row itself lists rather than a resolution-stage one the
    // execution stage may not emit.
    let Some(block) = params::declaring_block(cx, id) else {
        return Err(Resolution::failed(
            RuntimeError::ReferenceKind,
            parameter,
            format!("{id} resolves to no visible declaration"),
        ));
    };
    if block != "DEFINE" {
        return Err(Resolution::failed(
            RuntimeError::ReferenceKind,
            parameter,
            format!("{id} declares a {block}, not a kind.operation"),
        ));
    }
    if let Some(detail) = impure_axes(cx, id) {
        return Err(Resolution::failed(
            RuntimeError::OperationPrecondition,
            parameter,
            format!(
                "{id} may not be a {parameter} of {}: {detail}",
                contract.operation
            ),
        ));
    }
    if let Some(detail) = incomplete_contract(cx, id, parameter) {
        return Err(Resolution::failed(
            RuntimeError::OperationPrecondition,
            parameter,
            format!(
                "{id} may not be a {parameter} of {}: {detail}",
                contract.operation
            ),
        ));
    }
    let Some(implementation) = stdlib.pure_operation(id) else {
        return Err(Resolution::failed(
            RuntimeError::OperationPrecondition,
            parameter,
            format!("no implementation profile is installed for the {parameter} operation {id}"),
        ));
    };
    implementation(member).map_err(|detail| {
        Resolution::failed(
            RuntimeError::OperationPrecondition,
            parameter,
            format!("the {parameter} operation {id} did not produce a key: {detail}"),
        )
    })
}

/// Why a referenced operation's declared contract is not usable for this
/// parameter.
///
/// `operations_v0.1.0.json#/contracts/core.sort/parameters/key`: "A REFERENCE
/// resolves to a kind.operation with SIDE_EFFECT FALSE, DETERMINISTIC TRUE, a
/// fully resolved dependency set of exactly declared_state_only, **exactly one
/// PARAMETER accepting T, and exactly one RESULT** of a concrete registered
/// ordered type."
///
/// The row's `error.operation.precondition` trigger names "a missing,
/// ambiguous, incomplete, or out-of-bounds immutable profile" for the key
/// operation, while `axis_contract.custom_operation_resolution` says a custom
/// `kind.operation` "selects no implementation profile". The two are consistent
/// once the four words are read as what they are — the registry's closed
/// vocabulary for a profile-role selection fault — applied to the thing that
/// stands in for a profile here: the operation's own declared contract, whose
/// required properties this sentence lists. So an absent implementation is
/// *missing*, more than one PARAMETER leaves which one accepts T *ambiguous*,
/// no RESULT leaves the key type *incomplete*, and a declared dependency beyond
/// declared_state_only is *out of bounds* — each one error.operation.precondition
/// before effects, which is the only identifier the row's closed list admits.
fn incomplete_contract(cx: &Invocation<'_>, id: &str, parameter: &str) -> Option<String> {
    let index = cx
        .resolved
        .declarations()
        .all()
        .iter()
        .position(|d| d.id.qualified() == id)?;
    let block = lcl_runtime::syntax::declaration_block(cx.resolved, index)?;
    let parameters = block.fields("PARAMETER").len();
    if parameters != 1 {
        return Some(format!(
            "it declares {parameters} PARAMETER blocks, and exactly one must accept the member type"
        ));
    }
    let Some(result) = block.nested("RESULT") else {
        return Some(
            "it declares no RESULT, so it states no ordered key type to sort by".to_string(),
        );
    };
    // `core.sort`'s key wants "exactly one RESULT of a concrete registered
    // ordered type"; `core.filter` and `core.select` want "exactly one BOOLEAN
    // RESULT" from their predicate.
    let declared = lcl_runtime::syntax::field_text(&result, "TYPE")?;
    let base = declared
        .split(['[', '('])
        .next()
        .unwrap_or(&declared)
        .trim()
        .to_string();
    let admitted = if parameter == "key" {
        ORDERED_TYPES.contains(&base.as_str())
    } else {
        base == "BOOLEAN"
    };
    if !admitted {
        return Some(format!(
            "its RESULT declares {declared}, which is outside the {} this {parameter} admits",
            if parameter == "key" {
                "registered ordered types"
            } else {
                "BOOLEAN result"
            }
        ));
    }
    None
}

/// `operators_and_functions_v0.1.0.json#/ordered_types`.
const ORDERED_TYPES: [&str; 10] = [
    "INTEGER",
    "DECIMAL",
    "STRING",
    "DATE",
    "TIME",
    "DATETIME",
    "DURATION",
    "PERCENTAGE",
    "BYTES",
    "MEASURE",
];
/// Why a referenced operation may not serve as a pure key or predicate.
///
/// `None` means its declared axes satisfy the row: `SIDE_EFFECT FALSE`,
/// `DETERMINISTIC TRUE`, and a dependency set of exactly `declared_state_only`,
/// which an omitted `DEPENDENCY` already gives.
fn impure_axes(cx: &Invocation<'_>, id: &str) -> Option<String> {
    let index = cx
        .resolved
        .declarations()
        .all()
        .iter()
        .position(|d| d.id.qualified() == id)?;
    let block = lcl_runtime::syntax::declaration_block(cx.resolved, index)?;

    match lcl_runtime::syntax::field_text(&block, "SIDE_EFFECT").as_deref() {
        Some("FALSE") => {}
        Some(other) => return Some(format!("it declares SIDE_EFFECT {other}, not FALSE")),
        None => return Some("it declares no SIDE_EFFECT FALSE".to_string()),
    }
    match lcl_runtime::syntax::field_text(&block, "DETERMINISTIC").as_deref() {
        Some("TRUE") => {}
        Some(other) => return Some(format!("it declares DETERMINISTIC {other}, not TRUE")),
        None => return Some("it declares no DETERMINISTIC TRUE".to_string()),
    }
    // "DEPENDENCY declares the possible_dependencies maximum … an omitted
    // DEPENDENCY declares exactly declared_state_only."
    match lcl_runtime::syntax::field_text(&block, "DEPENDENCY") {
        None => None,
        Some(declared) if declared.contains("declared_state_only") => None,
        Some(declared) => Some(format!(
            "it declares DEPENDENCY {declared}, and an external dependency cannot be hidden \
             behind a reference"
        )),
    }
}

/// The fragment text of one parameter, from a STRING or a `kind.constant` REF.
///
/// > core.calculate also accepts REF to a kind.constant STRING containing that
/// > same fragment.
fn fragment_text(
    _stdlib: &Stdlib,
    cx: &Invocation<'_>,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
    name: &str,
) -> Result<String, Resolution> {
    match parameters.get(name) {
        Some(Value::Text(text)) => Ok(text.clone()),
        Some(reference @ Value::Reference(_)) => {
            let id = params::reference_id(reference).unwrap_or_default();
            let Some(block) = params::declaring_block(cx, id) else {
                return Err(Resolution::failed(
                    RuntimeError::ReferenceKind,
                    name,
                    format!("{id} resolves to no visible declaration"),
                ));
            };
            if block != "DEFINE" {
                return Err(Resolution::failed(
                    RuntimeError::ReferenceKind,
                    name,
                    format!("{id} declares a {block}, not a kind.constant"),
                ));
            }
            match cx.declaration_value(id) {
                Value::Text(text) => Ok(text),
                other => Err(Resolution::failed(
                    RuntimeError::ReferenceKind,
                    name,
                    format!(
                        "{} {name} must reference a kind.constant STRING, found {}",
                        contract.operation,
                        other.family()
                    ),
                )),
            }
        }
        Some(Value::Missing) | None => Err(Resolution::failed(
            RuntimeError::RequiredMissing,
            name,
            format!("{} requires {name}", contract.operation),
        )),
        Some(other) => Err(Resolution::failed(
            RuntimeError::TypeMismatch,
            name,
            format!(
                "{} {name} must be a STRING or a REFERENCE, found {}",
                contract.operation,
                other.family()
            ),
        )),
    }
}

/// The declared `bindings` OBJECT, with every key checked.
///
/// > keys must not collide with any visible declaration ID or its simple name,
/// > any registered reserved word, or target.
fn binding_object(
    stdlib: &Stdlib,
    cx: &Invocation<'_>,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
    reserved: &[&str],
) -> Result<BTreeMap<String, Value>, Resolution> {
    let bindings = match parameters.get("bindings") {
        Some(Value::Object(fields)) => fields.clone(),
        None | Some(Value::Missing) => BTreeMap::new(),
        Some(other) => {
            return Err(Resolution::failed(
                RuntimeError::TypeMismatch,
                "bindings",
                format!(
                    "{} bindings must be an OBJECT, found {}",
                    contract.operation,
                    other.family()
                ),
            ))
        }
    };
    for name in bindings.keys() {
        if let Some(detail) = fragment::invalid_binding_name(cx, name, stdlib.lexicon(), reserved) {
            return Err(fragment_failure(
                contract,
                &FragmentFault::Malformed(detail),
            ));
        }
    }
    Ok(bindings)
}

/// One fragment fault as a resolved failure under its own row's error set.
fn fragment_failure(contract: &OperationContract, fault: &FragmentFault) -> Resolution {
    Resolution::failed(fault.error(contract), fault.cause(), fault.detail())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `core.filter` and `core.group` refuse a SET; `core.select` takes one.
    ///
    /// This is a unit test rather than a document test because a `SET`-typed
    /// declaration does not currently reach the runtime as a `Value::Set`: M5's
    /// `data.rs` builds `Value::List` for every inline collection regardless of
    /// the declared type, so an end-to-end fixture cannot present a SET here.
    /// The rule itself is this crate's, and it is checked directly.
    #[test]
    fn a_set_is_refused_by_the_rows_that_take_only_a_list() {
        let set = Value::Set(vec![Value::Boolean(true)]);
        let list = Value::List(vec![Value::Boolean(true)]);

        for row in ["core.filter", "core.group"] {
            let refused = members_of(&set, row).expect_err("a SET is refused");
            match refused {
                Resolution::Failed { error, .. } => assert_eq!(error, RuntimeError::TypeMismatch),
                other => panic!("expected a type mismatch, got {other:?}"),
            }
            members_of(&list, row).expect("a LIST is accepted");
        }

        // "core.select ... target: meta.collection", which is "LIST[T] or SET[T]".
        members_of(&set, "core.select").expect("core.select accepts a SET");
        members_of(&list, "core.select").expect("core.select accepts a LIST");
    }

    #[test]
    fn a_value_that_is_not_a_collection_is_refused_by_every_row() {
        for row in ["core.filter", "core.group", "core.select"] {
            let refused = members_of(&Value::Boolean(true), row).expect_err("not a collection");
            match refused {
                Resolution::Failed { error, .. } => assert_eq!(error, RuntimeError::TypeMismatch),
                other => panic!("expected a type mismatch, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_property_path_walks_object_fields_and_stops_where_it_is_undefined() {
        let mut inner = BTreeMap::new();
        inner.insert("name".to_string(), Value::Text("x".to_string()));
        let mut outer = BTreeMap::new();
        outer.insert("row".to_string(), Value::Object(inner));
        let value = Value::Object(outer);

        assert_eq!(
            property_path(&value, "row.name"),
            Some(Value::Text("x".to_string()))
        );
        assert!(property_path(&value, "row.absent").is_none());
        assert!(property_path(&value, "absent").is_none());
        assert!(property_path(&value, "").is_none());
        // A value with no properties defines no path at all.
        assert!(property_path(&Value::Boolean(true), "anything").is_none());
    }
}
