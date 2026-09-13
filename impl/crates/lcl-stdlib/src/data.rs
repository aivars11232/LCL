//! Filesystem and addressable-data rows, and the engine's own stores.
//!
//! ## What this module decides, and what it hands over
//!
//! Everything before the first byte moves: which address class the target and
//! destination resolve to, which concrete effect and dependency classes that
//! makes this *invocation* select, whether those are inside the row's maxima,
//! which implementation profile serves the role the row requires, and whether
//! the row's own preconditions hold. Only then does a request cross the
//! boundary, carrying the sets it actually resolved rather than the row's
//! maxima.
//!
//! ## Why MEMORY and STATE are refused here
//!
//! > core.create, core.write, core.append, core.modify, core.rename,
//! > core.delete, and core.generate prohibit MEMORY and STATE targets;
//! > core.move prohibits MEMORY and STATE sources. Those mutations use
//! > core.memory_write or core.state_update.
//!
//! The prohibition is a language rule about which operation may touch which
//! declaration, so it is decided here and never reaches an adapter. An adapter
//! that had to know about MEMORY would be an adapter that could get it wrong.

use crate::contracts::OperationContract;
use crate::{params, pure, schema, Stdlib};
use lcl_capabilities::{AddressClass, Axes, Grant, ProfileFault, Refusal, Selection};
use lcl_runtime::capability::CapabilityRequest;
use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::operations::{Invocation, Resolution};
use lcl_runtime::Value;
use std::collections::BTreeMap;

/// The rows that prohibit an internal store as their mutation target.
const PROHIBIT_INTERNAL_STORE: [&str; 7] = [
    "core.create",
    "core.write",
    "core.append",
    "core.modify",
    "core.rename",
    "core.delete",
    "core.generate",
];

/// Resolve one invocation of a filesystem or addressable-data row.
pub(crate) fn invoke(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    // Read every reference through to the address it names, so the host sees a
    // resolved address and never a language-level reference.
    let target = request.target.as_ref().map(|t| pure::read_through(cx, t));
    let parameters = resolved_parameters(cx, parameters);

    let target_class = target
        .as_ref()
        .map(|value| params::classify(cx, value))
        .unwrap_or(AddressClass::Material);

    // "MEMORY and STATE targets are prohibited; use core.memory_write or
    // core.state_update."
    if PROHIBIT_INTERNAL_STORE.contains(&contract.operation.as_str())
        && target_class.is_internal_store()
    {
        return Resolution::failed(
            RuntimeError::OperationPrecondition,
            "target",
            format!(
                "{} prohibits a {target_class} target; use core.memory_write or \
                 core.state_update",
                contract.operation
            ),
        );
    }
    // "core.move prohibits MEMORY and STATE sources."
    if contract.operation == "core.move" && target_class.is_internal_store() {
        return Resolution::failed(
            RuntimeError::OperationPrecondition,
            "target",
            format!("core.move prohibits a {target_class} source"),
        );
    }

    if let Some(failure) = row_preconditions(contract, &target, &parameters) {
        return failure;
    }

    let resolved = match resolve_axes(cx, contract, target_class, &parameters) {
        Ok(axes) => axes,
        Err(failure) => return failure,
    };
    // "For core.execute, non_graph applies exactly to PATH, URI, or STRING
    // targets and graph applies exactly to REFERENCE[...] targets."
    let mode = if contract.operation == "core.execute" {
        Some(crate::dispatch::execute_mode(
            Some(target_class),
            params::referenced_declaration(cx, "TARGET")
                .and_then(|id| params::declaring_block(cx, &id))
                .is_some_and(|block| {
                    matches!(
                        block.as_str(),
                        "TASK" | "PHASE" | "SEQUENCE" | "ACTION" | "TEST"
                    )
                }),
        ))
    } else {
        None
    };
    if let Some(failure) = select_profiles(stdlib, contract, target_class, mode) {
        return failure;
    }

    let mut host_request = request.clone();
    host_request.target = target;
    host_request.parameters = parameters;
    host_request.possible_dependencies = resolved
        .dependencies
        .iter()
        .map(|d| d.to_string())
        .collect();
    host_request.possible_effects = resolved.effects.iter().map(|e| e.to_string()).collect();
    Resolution::Host(Box::new(host_request))
}

/// Resolve this invocation's actual dependency and effect sets.
///
/// > Source and destination address classes resolve independently, so one
/// > operation may produce more than one concrete effect class.
///
/// An invocation that resolves a class outside the row's maximum, and a
/// mutating invocation that resolves no concrete effect at all, "each emit
/// error.operation.precondition before effects".
fn resolve_axes(
    cx: &Invocation<'_>,
    contract: &OperationContract,
    target_class: AddressClass,
    parameters: &BTreeMap<String, Value>,
) -> Result<Axes, Resolution> {
    let destination_class = parameters
        .get("destination")
        .map(|value| params::classify(cx, value));

    let from_addresses = match destination_class {
        // With a declared destination, the target is observed and the
        // destination is mutated.
        Some(destination) => target_class.observation().union(&destination.mutation()),
        // Without one, the target itself is what changes — unless the row only
        // reads, which its maximum states by admitting no effect.
        None if contract.maximum.is_effect_free() => target_class.observation(),
        None => target_class.mutation(),
    };
    let mut resolved = from_addresses.union(&mandatory_axes(&contract.operation));

    // > network: Performs a primary transfer, copy, download, upload, or
    // > publication over a network …
    //
    // Observing a URI is a dependency; *transferring* over one is an effect.
    // The address-class rules above cannot tell the two apart, because the
    // difference is which row is asking.
    if is_primary_transfer(&contract.operation)
        && (target_class == AddressClass::Uri || destination_class == Some(AddressClass::Uri))
    {
        resolved = resolved
            .with_dependency(lcl_capabilities::Dependency::Network)
            .with_effect(lcl_capabilities::Effect::Network);
    }

    if !resolved.within(&contract.maximum) {
        let outside: Vec<String> = resolved
            .effects_outside(&contract.maximum)
            .iter()
            .map(|e| e.to_string())
            .chain(
                resolved
                    .dependencies_outside(&contract.maximum)
                    .iter()
                    .map(|d| d.to_string()),
            )
            .collect();
        return Err(Resolution::failed(
            RuntimeError::OperationPrecondition,
            "axes",
            format!(
                "{} resolved {} on a {target_class} target, which its row does not permit",
                contract.operation,
                outside.join(", ")
            ),
        ));
    }
    // "an effect-class declaration whose invocation resolves no concrete effect
    // class" is a precondition failure for a row that must change something.
    if !contract.maximum.is_effect_free() && resolved.is_effect_free() {
        return Err(Resolution::failed(
            RuntimeError::OperationPrecondition,
            "axes",
            format!(
                "{} resolved no concrete effect class on a {target_class} target",
                contract.operation
            ),
        ));
    }
    Ok(resolved)
}

/// The rows whose contact with a network *is* the transfer.
///
/// > The network effect is a declared primary transfer, copy, download,
/// > upload, or publication over a network, or a network-addressed content
/// > mutation. Incidental remote input is a dependency.
fn is_primary_transfer(operation: &str) -> bool {
    matches!(
        operation,
        "core.download" | "core.upload" | "core.copy" | "core.move" | "core.publish"
    )
}

/// The classes a row's own `invocation_resolution` resolves unconditionally.
///
/// Address classes answer for the rows whose axes follow their target and
/// destination. These do not: each sentence below names a class the invocation
/// selects whatever it is pointed at.
fn mandatory_axes(operation: &str) -> Axes {
    use lcl_capabilities::{Dependency, Effect};
    match operation {
        // "Resolve model from the generation profile", and the same sentence
        // shape on the analysis and reporting rows.
        "core.analyze" | "core.report" | "core.generate" => {
            Axes::inert().with_dependency(Dependency::Model)
        }
        // "Its rule always includes the process effect because the invocation
        // runs the executable."
        "core.execute" | "core.start" => Axes::inert()
            .with_dependency(Dependency::Host)
            .with_effect(Effect::Process),
        // Package state, and the process that changes it.
        "core.install" | "core.uninstall" => Axes::inert()
            .with_dependency(Dependency::Host)
            .with_effect(Effect::Package)
            .with_effect(Effect::Process),
        // "Terminate the selected path, process, or service."
        "core.stop" => Axes::inert()
            .with_dependency(Dependency::Host)
            .with_effect(Effect::Process),
        // core.send's meaning is the message it sends.
        "core.send" => Axes::inert()
            .with_dependency(Dependency::Host)
            .with_effect(Effect::Message),
        // "Resolve one authoritative human responder. The request channel is
        // internal to that human capability and adds no separate host or
        // network dependency; the request is a message effect."
        "core.ask" => Axes::inert()
            .with_dependency(Dependency::Human)
            .with_effect(Effect::Message),
        _ => Axes::inert(),
    }
}

/// Select every profile role the row requires, before effects.
fn select_profiles(
    stdlib: &Stdlib,
    contract: &OperationContract,
    target_class: AddressClass,
    mode: Option<&str>,
) -> Option<Resolution> {
    for role in stdlib.catalog().required_roles(&contract.operation, mode) {
        let selection = Selection {
            operation: &contract.operation,
            role: role.clone(),
            target_class,
            implementation: None,
        };
        if let Err(fault) = stdlib.catalog().select(&selection) {
            return Some(profile_failure(&fault));
        }
    }
    None
}

/// One profile fault as the registered pre-effect failure it is.
pub(crate) fn profile_failure(fault: &ProfileFault) -> Resolution {
    Resolution::failed(
        RuntimeError::OperationPrecondition,
        "implementation_profile",
        fault.to_string(),
    )
}

/// Row-specific preconditions that hold before any effect.
fn row_preconditions(
    contract: &OperationContract,
    target: &Option<Value>,
    parameters: &BTreeMap<String, Value>,
) -> Option<Resolution> {
    match contract.operation.as_str() {
        // "core.move requires resolved source and destination addresses to be
        // distinct; overwrite never permits a source to be its own destination."
        "core.move" => {
            let (Some(source), Some(destination)) = (target, parameters.get("destination")) else {
                return None;
            };
            if source == destination {
                return Some(Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "destination",
                    "core.move requires distinct source and destination addresses",
                ));
            }
            None
        }
        // "core.rename requires new_name to differ from the current name", and
        // its registry constraints require a non-empty name with no separator.
        "core.rename" => {
            let Some(Value::Text(new_name)) = parameters.get("new_name") else {
                return None;
            };
            if new_name.is_empty() {
                return Some(Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "new_name",
                    "core.rename requires a non-empty new_name",
                ));
            }
            if new_name.contains('/') || new_name.contains('\\') {
                return Some(Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "new_name",
                    "core.rename requires a new_name containing no path separator",
                ));
            }
            let current = match target {
                Some(Value::Constructed { text, .. }) => {
                    text.rsplit('/').next().unwrap_or(text.as_str()).to_string()
                }
                _ => String::new(),
            };
            if !current.is_empty() && &current == new_name {
                return Some(Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "new_name",
                    "core.rename requires new_name to differ from the current name",
                ));
            }
            None
        }
        _ => None,
    }
}

/// Every parameter, with references read through to their values.
fn resolved_parameters(
    cx: &Invocation<'_>,
    parameters: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    parameters
        .iter()
        .map(|(name, value)| (name.clone(), pure::read_through(cx, value)))
        .collect()
}

// ---------------------------------------------------------------------------
// The engine's own stores
// ---------------------------------------------------------------------------

/// `core.memory_write` and `core.state_update`.
///
/// `05_SEMANTICS/07`: "Writes require reachable core.memory_write or
/// core.state_update and applicable scope/authorization." Both gates apply here
/// exactly as they do to an external effect: the plan authorized the ACTION,
/// and the host's grants must permit the engine's own stores.
pub(crate) fn store(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let expected_block = if contract.operation == "core.memory_write" {
        "MEMORY"
    } else {
        "STATE"
    };
    let Some(target) = request.target.as_ref() else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            format!("{} requires a TARGET", contract.operation),
        );
    };
    // The declaration being written, taken from the invocation site's own
    // `REF(...)`. Reading the target as a value would yield what the store
    // currently holds, which is not an identity and cannot be written back.
    let Some(id) = params::reference_id(target)
        .map(|id| id.to_string())
        .or_else(|| params::referenced_declaration(cx, "TARGET"))
    else {
        return Resolution::failed(
            RuntimeError::ReferenceKind,
            "target",
            format!(
                "{} requires a REFERENCE[{expected_block}] target",
                contract.operation
            ),
        );
    };
    let id = id.as_str();
    match params::declaring_block(cx, id).as_deref() {
        Some(block) if block == expected_block => {}
        other => {
            return Resolution::failed(
                RuntimeError::ReferenceKind,
                "target",
                format!(
                    "{} requires a {expected_block} target; {id} declares a {}",
                    contract.operation,
                    other.unwrap_or("nothing")
                ),
            )
        }
    }

    // Both rows name the `storage` role for every invocation, and their
    // resolutions begin "Resolve the authorized MEMORY storage profile" and
    // "Resolve the authorized STATE storage profile". "A missing, ambiguous,
    // incomplete, or out-of-bounds required profile role emits
    // error.operation.precondition before effects."
    let target_class = if expected_block == "MEMORY" {
        AddressClass::Memory
    } else {
        AddressClass::State
    };
    if let Some(failure) = select_profiles(stdlib, contract, target_class, None) {
        return failure;
    }

    // The host gate, decided independently of what the plan authorized.
    if let Err(refusal) = stdlib.grants().decide(&Grant::InternalStore) {
        return match refusal {
            Refusal::Denied(detail) => {
                Resolution::failed(RuntimeError::PermissionDenied, "grant", detail)
            }
            Refusal::Unavailable(detail) => {
                Resolution::failed(RuntimeError::HostConstraint, "grant", detail)
            }
        };
    }

    let Some(value) = parameters.get("value").cloned() else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "value",
            format!("{} requires a value", contract.operation),
        );
    };
    let current = cx.declaration_value(id);

    // "expected_before" is a compare-and-set: the write happens only if the
    // store still holds what the document expected.
    if let Some(expected) = parameters.get("expected_before") {
        if !lcl_runtime::strict_equal(&current, expected) {
            return Resolution::failed(
                RuntimeError::OperationPrecondition,
                "expected_before",
                format!("{id} holds {current}, not the expected {expected}"),
            );
        }
    }

    // "merge TRUE requires the current MEMORY value and value parameter both to
    // be OBJECT."
    let written =
        if matches!(parameters.get("merge"), Some(Value::Boolean(true))) {
            match (&current, &value) {
                (Value::Object(existing), Value::Object(incoming)) => {
                    let mut merged = existing.clone();
                    for (field, field_value) in incoming {
                        merged.insert(field.clone(), field_value.clone());
                    }
                    Value::Object(merged)
                }
                _ => return Resolution::failed(
                    RuntimeError::TypeMismatch,
                    "merge",
                    "merge TRUE requires the current value and the value parameter to be OBJECT",
                ),
            }
        } else {
            value
        };

    let changed = !lcl_runtime::strict_equal(&current, &written);
    cx.write_store(id, written);

    // The effect class is the store's own: `memory` for MEMORY, `state` for
    // STATE, exactly as the row's maximum admits.
    let class = if expected_block == "MEMORY" {
        lcl_runtime::EffectClass::Memory
    } else {
        lcl_runtime::EffectClass::State
    };
    Resolution::Completed(
        schema::operation(target.clone(), Value::Boolean(changed)).with_effect(
            lcl_runtime::ObservedEffect {
                class,
                state: lcl_runtime::RecordState::Applied,
                target: Some(id.to_string()),
                evidence: Vec::new(),
            },
        ),
    )
}
