//! Phase A: the two closed axis vocabularies, and what an address resolves.

mod common;

use lcl_capabilities::{verify_vocabulary, AddressClass, Axes, Dependency, Effect};

#[test]
fn the_mirrored_vocabularies_are_exactly_the_registrys() {
    // Contract 5.5: "Mirrored enums require explicit parity tests." This is
    // that test, and it is the only thing standing between a registry change
    // and an implementation that silently keeps the old vocabulary.
    verify_vocabulary(common::spec()).expect("both axis vocabularies agree with the registry");
}

#[test]
fn the_sentinels_are_the_empty_sets_and_never_members() {
    let inert = Axes::inert();
    assert!(inert.is_declared_state_only());
    assert!(inert.is_effect_free());
    assert_eq!(inert.dependency_names(), vec!["declared_state_only"]);
    assert_eq!(inert.effect_names(), vec!["none"]);

    // "declared_state_only and none are exclusive singleton values." Adding a
    // concrete class removes the sentinel from the spelling rather than
    // combining with it, because the sentinel is the empty set.
    let host = inert.clone().with_dependency(Dependency::Host);
    assert_eq!(host.dependency_names(), vec!["host"]);
    assert!(!host.is_declared_state_only());
    assert!(host.is_effect_free(), "a dependency is not an effect");
}

#[test]
fn a_sentinel_in_the_registry_reads_back_as_the_empty_set() {
    // The registry writes a read-only row as possible_effects ["none"], which
    // must not become a phantom eighth effect class.
    let axes = Axes::from_registry(&["declared_state_only".to_string()], &["none".to_string()]);
    assert_eq!(axes, Axes::inert());
}

#[test]
fn reading_an_address_resolves_a_dependency_and_never_an_effect() {
    // "Resolve host for PATH, host-bound, or host-backed REFERENCE access and
    // host plus network for URI access; otherwise resolve declared_state_only."
    for class in AddressClass::ALL {
        let axes = class.observation();
        assert!(
            axes.is_effect_free(),
            "{class} observation must resolve no effect"
        );
    }
    assert_eq!(
        AddressClass::Path.observation().dependency_names(),
        vec!["host"]
    );
    assert_eq!(
        AddressClass::Uri.observation().dependency_names(),
        vec!["host", "network"]
    );
    assert!(AddressClass::Material
        .observation()
        .is_declared_state_only());
    assert!(AddressClass::Output.observation().is_declared_state_only());
}

#[test]
fn mutating_an_address_resolves_exactly_the_registered_class() {
    // "PATH mutation adds host dependency and filesystem effect; URI mutation
    // adds network dependency and network effect; OUTPUT or another authorized
    // non-filesystem, non-network, non-memory, non-STATE addressable target
    // adds state effect and its required dependency."
    let path = AddressClass::Path.mutation();
    assert_eq!(path.effect_names(), vec!["filesystem"]);
    assert_eq!(path.dependency_names(), vec!["host"]);

    let uri = AddressClass::Uri.mutation();
    assert_eq!(uri.effect_names(), vec!["network"]);
    assert_eq!(uri.dependency_names(), vec!["network"]);

    let output = AddressClass::Output.mutation();
    assert_eq!(output.effect_names(), vec!["state"]);
    assert!(output.is_declared_state_only());

    assert_eq!(
        AddressClass::Memory.mutation().effect_names(),
        vec!["memory"]
    );
    assert_eq!(AddressClass::State.mutation().effect_names(), vec!["state"]);
    assert_eq!(
        AddressClass::HostBound.mutation().effect_names(),
        vec!["state"]
    );
}

#[test]
fn a_material_value_is_not_an_address() {
    // It resolves no concrete effect class, which is the condition the axis
    // contract turns into error.operation.precondition before effects.
    assert!(!AddressClass::Material.is_addressable());
    assert!(AddressClass::Material.mutation().is_effect_free());
    for class in AddressClass::ALL {
        if class != AddressClass::Material {
            assert!(class.is_addressable(), "{class} is addressable");
        }
    }
}

#[test]
fn memory_and_state_are_the_two_internal_stores() {
    // The mutating rows prohibit them by name and redirect to core.memory_write
    // and core.state_update.
    let internal: Vec<AddressClass> = AddressClass::ALL
        .into_iter()
        .filter(|c| c.is_internal_store())
        .collect();
    assert_eq!(
        internal,
        vec![AddressClass::Memory, AddressClass::State],
        "exactly MEMORY and STATE are internal stores"
    );
}

#[test]
fn source_and_destination_resolve_independently_and_union() {
    // "Source and destination address classes resolve independently, so one
    // operation may produce more than one concrete effect class." A URI source
    // observed into a PATH destination is the canonical two-class case.
    let resolved = AddressClass::Uri
        .observation()
        .union(&AddressClass::Path.mutation());
    assert_eq!(resolved.effect_names(), vec!["filesystem"]);
    assert_eq!(resolved.dependency_names(), vec!["host", "network"]);
}

#[test]
fn bounds_are_checked_by_membership_not_by_count() {
    let maximum = Axes::inert()
        .with_effect(Effect::Filesystem)
        .with_effect(Effect::Network)
        .with_dependency(Dependency::Host);

    let inside = Axes::inert().with_effect(Effect::Filesystem);
    assert!(inside.within(&maximum));
    assert!(inside.effects_outside(&maximum).is_empty());

    let outside = Axes::inert().with_effect(Effect::Process);
    assert!(!outside.within(&maximum));
    assert_eq!(outside.effects_outside(&maximum), vec![Effect::Process]);

    let dependency_outside = Axes::inert().with_dependency(Dependency::Model);
    assert_eq!(
        dependency_outside.dependencies_outside(&maximum),
        vec![Dependency::Model]
    );

    // The empty set is inside every maximum: an invocation that selects nothing
    // has narrowed its row as far as narrowing goes.
    assert!(Axes::inert().within(&maximum));
}
