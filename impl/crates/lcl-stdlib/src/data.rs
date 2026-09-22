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
use lcl_runtime::{EffectState, Observation};
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
    // "A REFERENCE used as an address is classified by the address class of
    // its resolved target, never by REFERENCE syntax": the declaration a
    // reference names decides its class, not the value it currently holds.
    let target_class = params::referenced_declaration(cx, "TARGET")
        .map(|id| params::classify(cx, &Value::Reference(id)))
        .or_else(|| {
            request
                .target
                .as_ref()
                .map(|value| params::classify(cx, value))
        })
        .unwrap_or(AddressClass::Material);
    let destination_class = parameters
        .get("destination")
        .map(|value| params::classify(cx, value));
    // core.send: "Resolve host and optional network from the recipient or
    // endpoint". Observing the recipient's address adds its dependency.
    let recipient_axes = match (contract.operation.as_str(), parameters.get("recipient")) {
        ("core.send", Some(recipient)) => params::classify(cx, recipient).observation(),
        _ => Axes::inert(),
    };
    let parameters = resolved_parameters(cx, parameters);

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

    let resolved = match resolve_axes(contract, target_class, destination_class) {
        Ok(axes) => axes.union(&recipient_axes),
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
    let selected = match selected_axes(stdlib, contract, target_class, mode) {
        Ok(axes) => axes,
        Err(failure) => return failure,
    };
    // "Every such non-graph invocation has the process effect because it runs
    // the executable; union process with any other dependencies and effects
    // selected by the profile within this row's maxima."
    let resolved = match (&selected, contract.operation.as_str(), mode) {
        (Some(profile), "core.execute", Some("non_graph")) => resolved.union(profile),
        _ => resolved,
    };

    // A referenced execution unit "has no mandatory local process effect and
    // resolves the final determinism category and normalized transitive
    // dependency and effect unions of its reachable graph". The engine runs it;
    // no host is asked, and nothing about a process is synthesized.
    if mode == Some("graph") {
        return graph_mode(cx, contract);
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

/// `core.execute` over a `REFERENCE[TASK|PHASE|SEQUENCE|ACTION|TEST]`.
///
/// The first invocation names the unit; the runtime executes it on the one
/// executor and invokes the row again with what it did.
pub(crate) fn graph_mode(cx: &mut Invocation<'_>, contract: &OperationContract) -> Resolution {
    let Some(target) = params::referenced_declaration(cx, "TARGET") else {
        return Resolution::failed(
            RuntimeError::ReferenceKind,
            "target",
            format!(
                "{} in graph mode requires a REFERENCE to an execution unit",
                contract.operation
            ),
        );
    };
    let Some(outcome) = cx.graph.clone() else {
        return Resolution::Graph(target);
    };
    graph_result(contract, &outcome)
}

/// Turn one executed graph into this row's result.
pub(crate) fn graph_result(
    contract: &OperationContract,
    outcome: &lcl_runtime::operations::GraphOutcome,
) -> Resolution {
    // "error.reference.cycle ... fails before axis resolution" is decided in
    // preflight; what remains here is the union of what the graph actually
    // raised. A required action of the graph that failed to produce its result
    // is this row's error.execution.action — the identifier its own closed list
    // admits — and the graph's own identifiers stay in the retained evidence
    // where its invocations raised them.
    if !outcome.succeeded {
        let union: Vec<String> = outcome
            .invocations
            .iter()
            .flat_map(|invocation| invocation.errors.clone())
            .collect();
        let detail = format!(
            "the graph `{}` did not complete: [{}]",
            outcome.target,
            union.join(", ")
        );
        // What the graph had already done does not stop being true because a
        // later child failed. `05_SEMANTICS/09` lets a row report a pre-effect
        // failure only with "effect_state none, an empty observed_effects list,
        // and no bound or partial OUTPUT", and "Absence of evidence never
        // proves absence of effects" — so this row may claim one only when
        // every invocation it aggregates established that nothing began.
        let observed = graph_effects(outcome);
        let proven_effect_free = observed.is_empty()
            && outcome
                .invocations
                .iter()
                .all(|invocation| invocation.effect_state == EffectState::None);
        if proven_effect_free {
            return Resolution::failed(RuntimeError::ExecutionAction, "graph", detail);
        }
        let mut observation = Observation::none();
        observation.proven_effect_free = false;
        for effect in observed {
            observation = observation.with_effect(effect);
        }
        return Resolution::refused(RuntimeError::ExecutionAction, "graph", detail, observation);
    }
    let primary = match outcome.primaries.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    };
    let mut observation = schema::graph_command(primary);
    for effect in graph_effects(outcome) {
        observation = observation.with_effect(effect);
    }
    let _ = contract;
    Resolution::Completed(observation)
}

/// Every effect the graph's invocations recorded, in the order they began.
///
/// The row "resolves ... normalized transitive dependency and effect unions of
/// its reachable graph", and those unions are over the *axes* — the dependency
/// and effect classes, which the request already carries. The observed-effect
/// list is different evidence: it says what began and on what. Collapsing it by
/// class and state alone would report two writes to two files as one write,
/// which is not a normalization of anything — it is the loss of a target. So
/// identical re-reports of one effect are folded and distinct targets are not.
fn graph_effects(
    outcome: &lcl_runtime::operations::GraphOutcome,
) -> Vec<lcl_runtime::ObservedEffect> {
    let mut seen = std::collections::BTreeSet::new();
    let mut effects = Vec::new();
    for effect in outcome
        .invocations
        .iter()
        .flat_map(|invocation| invocation.observed.iter())
    {
        let identity = (
            effect.class,
            effect.state,
            effect.target.clone(),
            effect.evidence.clone(),
        );
        if seen.insert(identity) {
            effects.push(effect.clone());
        }
    }
    effects
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
    contract: &OperationContract,
    target_class: AddressClass,
    destination_class: Option<AddressClass>,
) -> Result<Axes, Resolution> {
    // "Resolve source and destination address classes independently … removing
    // OUTPUT or another authorized non-filesystem, non-network, non-memory,
    // non-STATE addressable source adds state effect and its required
    // dependency": a row that *removes* its source changes it, so the source
    // side is a mutation rather than an observation.
    let removes_source = contract.operation == "core.move";
    // "an omitted destination binds only the declared result or OUTPUT state",
    // so such an invocation changes nothing outside the document.
    let result_only = destination_class.is_none() && contract.operation == "core.convert";
    let from_addresses = match destination_class {
        // With a declared destination, the target is read — or removed — and
        // the destination is mutated.
        Some(destination) => {
            let source = if removes_source {
                target_class.mutation()
            } else {
                target_class.observation()
            };
            source.union(&destination.mutation())
        }
        // Without one, the target itself is what changes — unless the row only
        // reads, which its maximum states by admitting no effect.
        None if result_only || contract.maximum.is_effect_free() => target_class.observation(),
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
    if !result_only && !contract.maximum.is_effect_free() && resolved.is_effect_free() {
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

/// The axis check a custom `kind.operation` invocation carries itself.
///
/// `operations_v0.1.0.json#/axis_contract/custom_operation_resolution`:
///
/// > Resolve the invocation effect set from the address classes of the
/// > resolved target and declared destination arguments under the same
/// > address-class rules that bind core rows, bounded by the declared maximum;
/// > an invocation that resolves an effect class outside that maximum, and an
/// > effect-class declaration whose invocation resolves no concrete effect
/// > class, each emit error.operation.precondition before effects.
///
/// A declaration this cannot read imposes nothing: `SIDE_EFFECT FALSE`
/// "declares possible_effects exactly none", which no invocation can exceed.
pub(crate) fn custom_axes(cx: &Invocation<'_>, request: &CapabilityRequest) -> Option<Resolution> {
    let index = params::declaration_index(cx, &request.operation)?;
    let block = lcl_runtime::syntax::declaration_block(cx.resolved, index)?;
    let declared = effect_classes(&lcl_runtime::syntax::field_text(&block, "SIDE_EFFECT")?);
    if declared.is_empty() {
        return None;
    }
    let maximum = lcl_capabilities::Axes::from_registry(&[], &declared);
    let target_class = request
        .target
        .as_ref()
        .map(|value| params::classify(cx, &crate::pure::read_through(cx, value)))
        .unwrap_or(AddressClass::Material);
    let resolved = target_class.mutation();
    let outside = resolved.effects_outside(&maximum);
    if !outside.is_empty() {
        return Some(Resolution::failed(
            RuntimeError::OperationPrecondition,
            "axes",
            format!(
                "{} resolved {} on a {target_class} target, outside its declared SIDE_EFFECT",
                request.operation,
                outside
                    .iter()
                    .map(|effect| effect.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    if resolved.is_effect_free() {
        return Some(Resolution::failed(
            RuntimeError::OperationPrecondition,
            "axes",
            format!(
                "{} declares effect classes but resolved none on a {target_class} target",
                request.operation
            ),
        ));
    }
    None
}

/// The concrete effect classes one written `SIDE_EFFECT` declares.
///
/// "SIDE_EFFECT FALSE declares possible_effects exactly none. A SIDE_EFFECT
/// effect-class LIST declares one or more distinct concrete effect classes,
/// excludes none".
fn effect_classes(written: &str) -> Vec<String> {
    written
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|name| name.trim().to_string())
        .filter(|name| lcl_capabilities::Effect::from_registry_str(name).is_some())
        .collect()
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
pub(crate) fn select_profiles(
    stdlib: &Stdlib,
    contract: &OperationContract,
    target_class: AddressClass,
    mode: Option<&str>,
) -> Option<Resolution> {
    selected_axes(stdlib, contract, target_class, mode).err()
}

/// Select every required role, and return what those profiles themselves
/// declare.
///
/// > A missing, ambiguous, incomplete, or out-of-bounds required profile role
/// > emits error.operation.precondition before effects.
///
/// A profile's own axes "may narrow the row's but never widen it", so a
/// selected profile outside the row's maxima is the out-of-bounds case.
pub(crate) fn selected_axes(
    stdlib: &Stdlib,
    contract: &OperationContract,
    target_class: AddressClass,
    mode: Option<&str>,
) -> Result<Option<lcl_capabilities::Axes>, Resolution> {
    let mut selected: Option<lcl_capabilities::Axes> = None;
    for role in stdlib.catalog().required_roles(&contract.operation, mode) {
        let selection = Selection {
            operation: &contract.operation,
            role: role.clone(),
            target_class,
            implementation: None,
        };
        let profile = match stdlib.catalog().select(&selection) {
            Ok(profile) => profile,
            Err(fault) => return Err(profile_failure(&fault)),
        };
        if !profile.axes.within(&contract.maximum) {
            return Err(Resolution::failed(
                RuntimeError::OperationPrecondition,
                "implementation_profile",
                format!(
                    "the selected {role} profile {} declares axes outside {}'s maxima",
                    profile.implementation_id, contract.operation
                ),
            ));
        }
        selected = Some(match selected {
            Some(axes) => axes.union(&profile.axes),
            None => profile.axes.clone(),
        });
    }
    Ok(selected)
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
        // "at least content or target_type is supplied".
        "core.create" => (!parameters.contains_key("content")
            && !parameters.contains_key("target_type"))
        .then(|| {
            Resolution::failed(
                RuntimeError::OperationPrecondition,
                "content",
                "core.create requires content or target_type",
            )
        }),
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
                Some(
                    Value::Constructed { text, .. } | Value::WorkspacePath { resolved: text, .. },
                ) => text.rsplit('/').next().unwrap_or(text.as_str()).to_string(),
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
    // "MEMORY mode permits write" and "STATE mode permits write": a store
    // declared read-only refuses before any effect.
    if let Some(mode) = params::declaration_index(cx, id)
        .and_then(|index| lcl_runtime::syntax::declaration_block(cx.resolved, index))
        .and_then(|block| lcl_runtime::syntax::field_text(&block, "MODE"))
    {
        if mode == "mode.read_only" {
            return Resolution::failed(
                RuntimeError::OperationPrecondition,
                "mode",
                format!("{id} declares {mode}, which does not permit a write"),
            );
        }
    }
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
    let storage = match selected_axes(stdlib, contract, target_class, None) {
        Ok(axes) => axes,
        Err(failure) => return failure,
    };

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
    // be OBJECT", and the row states both as preconditions: "when merge is
    // TRUE, the current MEMORY value and value parameter are OBJECT and the
    // computed merged OBJECT matches the declared MEMORY type".
    let merging = matches!(parameters.get("merge"), Some(Value::Boolean(true)));
    let written = if merging {
        match (&current, &value) {
            (Value::Object(existing), Value::Object(incoming)) => {
                let mut merged = existing.clone();
                for (field, field_value) in incoming {
                    merged.insert(field.clone(), field_value.clone());
                }
                Value::Object(merged)
            }
            (Value::Object(_), _) => {
                return Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "merge",
                    "merge TRUE requires the value parameter to be OBJECT",
                )
            }
            _ => {
                return Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "merge",
                    "merge TRUE requires the current value to be OBJECT",
                )
            }
        }
    } else {
        value
    };

    // "when merge is FALSE, value matches the declared MEMORY type"; "when
    // merge is TRUE, ... the computed merged OBJECT matches the declared
    // MEMORY type"; and for STATE, "value type matches".
    if let Some(declared) =
        params::declaration_index(cx, id).and_then(|index| cx.checked.declaration_type(index))
    {
        if !params::value_matches(declared, &written) {
            return Resolution::failed(
                RuntimeError::OperationPrecondition,
                if merging { "merge" } else { "value" },
                format!(
                    "{} requires the written value to match the declared {expected_block} type",
                    contract.operation
                ),
            );
        }
    }

    // How the store is performed is the selected profile's own declaration.
    //
    // Both rows resolve "the authorized MEMORY storage profile" / "STATE
    // storage profile", and their maxima admit the `host` dependency. The
    // shipped profile narrows that to nothing and states its own resolution:
    // "Write the declared engine-owned store in place within one invocation;
    // the invocation selects no external dependency". A profile that instead
    // declares the row's `host` dependency is an externally backed store, and
    // the write it describes is performed by crossing the boundary — where a
    // limitation, a failure and an unestablished effect extent are exactly the
    // registered error.host.constraint, error.execution.action and
    // error.operation.postcondition the row lists.
    let host_backed = storage
        .as_ref()
        .is_some_and(|axes| !axes.dependencies.is_empty());
    if host_backed {
        let mut host_request = request.clone();
        // The store is named, not read through: the identity is what is
        // written, and its current value is not it.
        host_request.target = Some(Value::Reference(id.to_string()));
        let mut asked = parameters.clone();
        // The profile is asked to persist exactly the value this invocation
        // resolved, merge and type checks included.
        asked.insert("value".to_string(), written);
        host_request.parameters = asked;
        if let Some(axes) = storage {
            host_request.possible_dependencies =
                axes.dependencies.iter().map(|d| d.to_string()).collect();
            host_request.possible_effects = axes.effects.iter().map(|e| e.to_string()).collect();
        }
        return Resolution::Host(Box::new(host_request));
    }

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
