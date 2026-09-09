//! The closed dispatch table: every registered row, and who owns it.
//!
//! ## Closure is the point
//!
//! `operations_v0.1.0.json` is `"closed": true` and
//! `built_in_groups_and_results_v0.1.0.json#/core_operation_ids` lists exactly
//! thirty-nine identifiers. [`family`] is a total function over that list and a
//! partial one over everything else: an identifier the registry does not carry
//! resolves to no family, reaches no implementation, and fails closed.
//!
//! That is checked rather than asserted in prose — `tests/coverage.rs` compares
//! this table against the registry in both directions, so adding a row to one
//! without the other is a test failure rather than a silent gap.
//!
//! ## Why families and not one flat match
//!
//! The seven families are the seven *kinds of trust* a row needs, and grouping
//! by them keeps a row from quietly acquiring a capability that its family does
//! not have. A [`Family::Pure`] row reads declared values and nothing else; it
//! has no path to a host at all, which is a stronger guarantee than a
//! convention that it should not use one.

use lcl_capabilities::AddressClass;

/// Which part of this crate owns one registered row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Family {
    /// Computed in-language from declared values. Reaches no host.
    ///
    /// Every member has `possible_dependencies` exactly `declared_state_only`
    /// and `possible_effects` exactly `none`.
    Pure,
    /// Read-only, but dependent on a host, a network or a model.
    ///
    /// `read_only requires possible effects exactly {none}`, so a member may
    /// observe the world and may never change it.
    Analytical,
    /// Filesystem and addressable-data mutation.
    Data,
    /// Network transfer, publication and messaging.
    Network,
    /// Process, service and package state.
    Process,
    /// The engine's own MEMORY and STATE stores.
    Store,
    /// Execution control: retry, continue, cancel, stop, test, ask.
    Control,
}

impl Family {
    /// Whether a member may ever cross the capability boundary.
    pub fn reaches_host(self) -> bool {
        !matches!(self, Family::Pure)
    }
}

/// The family that owns one registered operation, or `None` for an identifier
/// this release does not register.
///
/// The grouping follows the registry's own `category` and axis columns:
/// `read_only` rows split into [`Family::Pure`] and [`Family::Analytical`] by
/// whether their `possible_dependencies` is exactly `declared_state_only`;
/// `mutating` rows split by which concrete effect classes their row admits;
/// `memory_state` is [`Family::Store`]; `control` is [`Family::Control`].
pub fn family(operation: &str) -> Option<Family> {
    Some(match operation {
        // read_only, dependencies exactly declared_state_only.
        "core.calculate" | "core.select" | "core.filter" | "core.sort" | "core.group"
        | "core.return" => Family::Pure,

        // read_only, but host, network or model dependent.
        //
        // core.compare belongs here rather than with the pure rows: its target
        // and `against` are `meta.material_value|meta.addressable`, so the row
        // admits host and network for the addressable case. An invocation whose
        // operands are both material still resolves declared_state_only — the
        // row maximum is what decides the family, and the invocation decides
        // its own axes.
        "core.inspect" | "core.read" | "core.compare" | "core.analyze" | "core.report"
        | "core.validate" | "core.verify" => Family::Analytical,

        // mutating, filesystem- and addressable-data centred.
        "core.create" | "core.write" | "core.append" | "core.modify" | "core.delete"
        | "core.rename" | "core.move" | "core.copy" | "core.convert" | "core.generate" => {
            Family::Data
        }

        // mutating, network centred.
        "core.download" | "core.upload" | "core.publish" | "core.send" => Family::Network,

        // mutating, process and package centred.
        "core.execute" | "core.start" | "core.install" | "core.uninstall" => Family::Process,

        // memory_state.
        "core.memory_write" | "core.state_update" => Family::Store,

        // control.
        "core.ask" | "core.cancel" | "core.continue" | "core.retry" | "core.stop" | "core.test" => {
            Family::Control
        }

        _ => return None,
    })
}

/// Every operation this table knows, in registry order.
pub const DISPATCHED: [&str; 39] = [
    "core.analyze",
    "core.append",
    "core.ask",
    "core.calculate",
    "core.cancel",
    "core.compare",
    "core.continue",
    "core.convert",
    "core.copy",
    "core.create",
    "core.delete",
    "core.download",
    "core.execute",
    "core.filter",
    "core.generate",
    "core.group",
    "core.inspect",
    "core.install",
    "core.memory_write",
    "core.modify",
    "core.move",
    "core.publish",
    "core.read",
    "core.rename",
    "core.report",
    "core.retry",
    "core.return",
    "core.select",
    "core.send",
    "core.sort",
    "core.start",
    "core.state_update",
    "core.stop",
    "core.test",
    "core.uninstall",
    "core.upload",
    "core.validate",
    "core.verify",
    "core.write",
];

/// Which invocation mode a `core.execute` target selects.
///
/// > For core.execute, non_graph applies exactly to PATH, URI, or STRING
/// > targets and graph applies exactly to REFERENCE[TASK|PHASE|SEQUENCE|ACTION|
/// > TEST] targets.
///
/// The mode is a property of the resolved target, so it is decided here from
/// the address class rather than from the spelling of the TARGET field.
pub fn execute_mode(target: Option<AddressClass>, is_execution_unit: bool) -> &'static str {
    if is_execution_unit {
        return "graph";
    }
    match target {
        Some(AddressClass::Path) | Some(AddressClass::Uri) | Some(AddressClass::Material) => {
            "non_graph"
        }
        // An address that is neither a path, a URI nor a command string is not
        // a non-graph execution target; the row's own precondition rejects it.
        _ => "non_graph",
    }
}
