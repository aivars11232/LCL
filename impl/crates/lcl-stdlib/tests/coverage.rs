//! Phase A gate: the dispatch table is exactly the registry's closed row set.

mod common;

use lcl_capabilities::{Axes, Dependency};
use lcl_stdlib::{family, Family, DISPATCHED};
use std::collections::BTreeSet;

/// `built_in_groups_and_results_v0.1.0.json#/core_operation_ids`.
fn registered_ids() -> BTreeSet<String> {
    common::spec()
        .registry("built_in_groups_and_results")
        .and_then(|r| r.get("core_operation_ids"))
        .and_then(|ids| ids.as_array())
        .expect("the registry closes the core operation ids")
        .iter()
        .filter_map(|id| id.as_str())
        .map(|id| id.to_string())
        .collect()
}

#[test]
fn the_dispatch_table_is_the_registered_row_set_in_both_directions() {
    let registered = registered_ids();
    let dispatched: BTreeSet<String> = DISPATCHED.iter().map(|id| id.to_string()).collect();

    let missing_from_dispatch: Vec<&String> = registered.difference(&dispatched).collect();
    let missing_from_registry: Vec<&String> = dispatched.difference(&registered).collect();

    assert!(
        missing_from_dispatch.is_empty(),
        "registered rows with no dispatch entry: {missing_from_dispatch:?}"
    );
    assert!(
        missing_from_registry.is_empty(),
        "dispatch entries the registry does not register: {missing_from_registry:?}"
    );
    assert_eq!(registered.len(), 39, "the registry closes at 39 rows");
}

#[test]
fn the_two_registries_agree_on_the_row_set() {
    // `operations_v0.1.0.json#/contracts` and
    // `built_in_groups_and_results#/core_operation_ids` are separate closed
    // lists of the same thing. An implementation that read only one would not
    // notice them drifting apart.
    let contracts: BTreeSet<String> = common::spec()
        .registry("operations")
        .and_then(|r| r.get("contracts"))
        .and_then(|c| c.as_object())
        .expect("operation contracts")
        .iter()
        .map(|(id, _)| id.clone())
        .collect();
    assert_eq!(contracts, registered_ids());
}

#[test]
fn every_dispatched_row_has_exactly_one_family() {
    for id in DISPATCHED {
        assert!(
            family(id).is_some(),
            "{id} is dispatched but belongs to no family"
        );
    }
}

#[test]
fn an_unregistered_identifier_belongs_to_no_family() {
    // A custom kind.operation is not a core row. Failing closed here is what
    // keeps `image.generate` from accidentally acquiring core.generate's
    // contract.
    assert_eq!(family("image.generate"), None);
    assert_eq!(family("core.nonexistent"), None);
    assert_eq!(family(""), None);
}

/// The registry facts one row states about itself.
fn row(id: &str) -> (String, Axes) {
    let contract = common::spec()
        .registry("operations")
        .and_then(|r| r.get("contracts"))
        .and_then(|c| c.get(id))
        .unwrap_or_else(|| panic!("{id} is registered"));
    let list = |member: &str| -> Vec<String> {
        contract
            .get(member)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|i| i.as_str())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    };
    let category = contract
        .get("category")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    (
        category,
        Axes::from_registry(&list("possible_dependencies"), &list("possible_effects")),
    )
}

#[test]
fn the_pure_family_is_exactly_the_rows_that_need_nothing_external() {
    // Membership is derived from the registry, not asserted by name: a pure row
    // is a read_only row whose dependency maximum is the declared_state_only
    // sentinel. If a future release gave core.sort a host dependency, this test
    // would fail rather than let a pure implementation keep reading it.
    for id in DISPATCHED {
        let (category, axes) = row(id);
        let is_pure_by_registry = category == "read_only" && axes.is_declared_state_only();
        let is_pure_by_table = family(id) == Some(Family::Pure);
        assert_eq!(
            is_pure_by_table, is_pure_by_registry,
            "{id}: the table says pure={is_pure_by_table}, the registry says {is_pure_by_registry} \
             (category {category}, dependencies {:?})",
            axes.dependency_names()
        );
    }
}

#[test]
fn every_read_only_row_admits_no_effect() {
    // "read_only requires possible effects exactly {none}." Both read-only
    // families must satisfy it.
    for id in DISPATCHED {
        let (category, axes) = row(id);
        if category != "read_only" {
            continue;
        }
        assert!(
            axes.is_effect_free(),
            "{id} is read_only and must admit no effect"
        );
        let owner = family(id).expect("dispatched");
        assert!(
            matches!(owner, Family::Pure | Family::Analytical),
            "{id} is read_only but owned by {owner:?}"
        );
    }
}

#[test]
fn the_store_family_is_exactly_the_memory_state_category() {
    for id in DISPATCHED {
        let (category, _) = row(id);
        assert_eq!(
            family(id) == Some(Family::Store),
            category == "memory_state",
            "{id}: category {category}"
        );
    }
}

#[test]
fn the_control_family_is_exactly_the_control_category() {
    for id in DISPATCHED {
        let (category, _) = row(id);
        assert_eq!(
            family(id) == Some(Family::Control),
            category == "control",
            "{id}: category {category}"
        );
    }
}

#[test]
fn only_the_pure_family_is_barred_from_the_host() {
    assert!(!Family::Pure.reaches_host());
    for owner in [
        Family::Analytical,
        Family::Data,
        Family::Network,
        Family::Process,
        Family::Store,
        Family::Control,
    ] {
        assert!(owner.reaches_host(), "{owner:?} may cross the boundary");
    }
}

#[test]
fn a_pure_row_declares_no_external_dependency_anywhere_in_the_registry() {
    // The strongest statement of the same rule: a pure row's *maximum* contains
    // no external class at all, so no invocation of it can resolve one.
    for id in DISPATCHED {
        if family(id) != Some(Family::Pure) {
            continue;
        }
        let (_, axes) = row(id);
        for dependency in Dependency::ALL {
            assert!(
                !axes.dependencies.contains(&dependency),
                "{id} is pure but its row admits {dependency}"
            );
        }
    }
}

/// A request naming one operation and nothing else.
fn bare_request(operation: &str) -> lcl_runtime::capability::CapabilityRequest {
    lcl_runtime::capability::CapabilityRequest {
        operation: operation.to_string(),
        target: None,
        parameters: std::collections::BTreeMap::new(),
        authorization: lcl_runtime::Authorized {
            operation: operation.to_string(),
            target: None,
            scope: None,
            permitted_by: Vec::new(),
            overridden: Vec::new(),
        },
        category: String::new(),
        possible_effects: Default::default(),
        possible_dependencies: Default::default(),
        result_schema: "result.operation".to_string(),
        invocation: lcl_runtime::InvocationId::first(0, lcl_runtime::IterationPath::root()),
        source: lcl_resolver::SourceId::new("root.lcl"),
        span: lcl_lexer::Span::empty(0),
    }
}

#[test]
fn every_registered_row_resolves_without_panicking() {
    // Contract 6: "Malformed/untrusted source must not panic." A row invoked
    // with no target and no parameters is the most degenerate request the
    // dispatcher can receive, and every one of the thirty-nine must answer.
    use lcl_runtime::operations::{Invocation, Operations};

    let source = common::task(
        &common::data("data.subject", "INTEGER", "3"),
        &["ID: action.subject\nOPERATION: core.return\nTARGET: REF(data.subject)"],
    );
    let fixture = common::fixture(&source);
    let plan = fixture.planned.plan().expect("the fixture planned");
    let mut stdlib = common::stdlib();

    for operation in DISPATCHED {
        let mut bindings = lcl_runtime::Bindings::new();
        let mut cx = Invocation {
            contracts: common::contracts(),
            resolved: &fixture.resolved,
            checked: &fixture.checked,
            plan,
            bindings: &mut bindings,
            source: fixture.source.clone(),
            iteration: lcl_runtime::IterationPath::root(),
            span: lcl_lexer::Span::empty(0),
            declaration: None,
        };
        // The assertion is that this returns at all: a panic here would fail
        // the test by unwinding, and a resolution of any shape is an answer.
        let _ = stdlib.invoke(&mut cx, &bare_request(operation));
    }
}

#[test]
fn an_unregistered_operation_is_deferred_rather_than_guessed() {
    use lcl_runtime::operations::{Invocation, Operations, Resolution};

    let source = common::task(
        &common::data("data.subject", "INTEGER", "3"),
        &["ID: action.subject\nOPERATION: core.return\nTARGET: REF(data.subject)"],
    );
    let fixture = common::fixture(&source);
    let plan = fixture.planned.plan().expect("the fixture planned");
    let mut stdlib = common::stdlib();
    let mut bindings = lcl_runtime::Bindings::new();
    let mut cx = Invocation {
        contracts: common::contracts(),
        resolved: &fixture.resolved,
        checked: &fixture.checked,
        plan,
        bindings: &mut bindings,
        source: fixture.source.clone(),
        iteration: lcl_runtime::IterationPath::root(),
        span: lcl_lexer::Span::empty(0),
        declaration: None,
    };
    // A custom kind.operation declares a contract and no body, so its
    // implementation is the host's and this crate invents nothing for it.
    let resolution = stdlib.invoke(&mut cx, &bare_request("image.generate"));
    assert!(matches!(resolution, Resolution::Host(_)));
}
