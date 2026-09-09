//! Phase A: the role map, the four selection faults, bounds, and determinism.

mod common;

use lcl_capabilities::{
    profile::axes, AddressClass, Axes, Dependency, Determinism, Effect, Profile, ProfileCatalog,
    ProfileFault, Role, RowDeterminism, Selection, TargetClass,
};

fn catalog() -> ProfileCatalog {
    ProfileCatalog::load(common::spec()).expect("the role map loads from the approved package")
}

/// A complete, in-bounds profile for one row and role.
fn complete(operation: &str, role: &str, implementation: &str, axes: Axes) -> Profile {
    Profile::builder(operation, Role::new(role), implementation, "1.0.0")
        .determinism(
            Determinism::Deterministic,
            "the exact declared inputs and one immutable implementation version",
        )
        .axes(axes)
        .resolving("resolve the invocation sets from the resolved target address class")
}

fn select<'a>(
    catalog: &'a ProfileCatalog,
    operation: &'a str,
    role: &str,
    class: AddressClass,
) -> Result<&'a Profile, ProfileFault> {
    catalog.select(&Selection {
        operation,
        role: Role::new(role),
        target_class: class,
        implementation: None,
    })
}

#[test]
fn every_registered_row_loads_with_its_axes_and_schema() {
    let catalog = catalog();
    let rows: Vec<&str> = catalog.rows().map(|r| r.operation.as_str()).collect();
    assert_eq!(rows.len(), 39, "the registry closes at 39 operation rows");

    let read = catalog.row("core.read").expect("core.read is registered");
    assert!(read.is_read_only());
    // "read_only requires possible effects exactly {none}."
    assert!(read.maximum.is_effect_free());
    assert_eq!(read.result_schema, "result.value");
    assert_eq!(read.determinism, RowDeterminism::Deterministic);

    let execute = catalog
        .row("core.execute")
        .expect("core.execute is registered");
    assert_eq!(execute.determinism, RowDeterminism::Derived);
    assert!(execute.maximum.effects.contains(&Effect::Process));

    let retry = catalog.row("core.retry").expect("core.retry is registered");
    assert_eq!(retry.determinism, RowDeterminism::Inherited);
}

#[test]
fn the_role_map_is_the_registrys_and_nothing_else() {
    let catalog = catalog();

    // "all applies to every invocation of that core operation."
    assert_eq!(
        catalog.required_roles("core.write", None),
        vec![Role::new("write")]
    );
    assert_eq!(
        catalog.required_roles("core.delete", None),
        vec![Role::new("delete")]
    );

    // "core.download=source+transfer": two roles, both required.
    assert_eq!(
        catalog.required_roles("core.download", None),
        vec![Role::new("source"), Role::new("transfer")]
    );

    // "For core.execute, non_graph applies exactly to PATH, URI, or STRING
    // targets and graph applies exactly to REFERENCE[...] targets; graph mode
    // has no local execution profile."
    assert_eq!(
        catalog.required_roles("core.execute", Some("non_graph")),
        vec![Role::new("execution")]
    );
    assert!(catalog
        .required_roles("core.execute", Some("graph"))
        .is_empty());

    // "A core operation absent from required_roles_by_operation requires no
    // local core profile."
    assert!(catalog.required_roles("core.read", None).is_empty());
    assert!(catalog.required_roles("core.calculate", None).is_empty());
    assert!(catalog.required_roles("core.retry", None).is_empty());
}

#[test]
fn no_profile_installed_is_the_missing_fault() {
    let catalog = catalog();
    let fault = select(&catalog, "core.write", "write", AddressClass::Path)
        .expect_err("nothing is installed");
    match fault {
        ProfileFault::Missing {
            operation, role, ..
        } => {
            assert_eq!(operation, "core.write");
            assert_eq!(role, Role::new("write"));
        }
        other => panic!("expected Missing, got {other}"),
    }
}

#[test]
fn two_profiles_for_one_role_is_the_ambiguous_fault() {
    let filesystem = axes(&[Dependency::Host], &[Effect::Filesystem]);
    let catalog = catalog()
        .with(complete("core.write", "write", "posix", filesystem.clone()))
        .with(complete("core.write", "write", "memory", filesystem));
    let fault =
        select(&catalog, "core.write", "write", AddressClass::Path).expect_err("two candidates");
    match fault {
        ProfileFault::Ambiguous { candidates, .. } => assert_eq!(candidates.len(), 2),
        other => panic!("expected Ambiguous, got {other}"),
    }
}

#[test]
fn an_under_declared_profile_is_the_incomplete_fault() {
    // "Every profile must contain every property listed by
    // axis_contract/implementation_profile."
    let catalog = catalog().with(
        Profile::builder("core.write", Role::new("write"), "posix", "1.0.0")
            .axes(axes(&[Dependency::Host], &[Effect::Filesystem])),
    );
    let fault = select(&catalog, "core.write", "write", AddressClass::Path)
        .expect_err("no determinism source and no invocation resolution");
    match fault {
        ProfileFault::Incomplete { properties, .. } => {
            assert!(properties.contains(&"determinism_source"));
            assert!(properties.contains(&"invocation_resolution"));
        }
        other => panic!("expected Incomplete, got {other}"),
    }
}

#[test]
fn a_profile_may_narrow_its_row_but_never_widen_it() {
    let catalog = catalog();

    // core.write's maximum is {filesystem, network, state}. Narrowing to
    // exactly {filesystem} is permitted.
    let narrowed = catalog.clone().with(complete(
        "core.write",
        "write",
        "posix",
        axes(&[Dependency::Host], &[Effect::Filesystem]),
    ));
    select(&narrowed, "core.write", "write", AddressClass::Path)
        .expect("a narrowed profile is in bounds");

    // Claiming `process` is widening, and the row does not permit it.
    let widened = catalog.with(complete(
        "core.write",
        "write",
        "posix",
        axes(&[Dependency::Host], &[Effect::Process]),
    ));
    let fault = select(&widened, "core.write", "write", AddressClass::Path)
        .expect_err("process is outside the row");
    match fault {
        ProfileFault::OutOfBounds { detail, .. } => assert!(
            detail.contains("process"),
            "the fault must name the widened class, got: {detail}"
        ),
        other => panic!("expected OutOfBounds, got {other}"),
    }
}

#[test]
fn a_deterministic_row_refuses_a_nondeterministic_profile() {
    // "A profile selected by a deterministic base row must declare
    // deterministic; a nondeterministic profile is out of bounds."
    let catalog = catalog().with(
        Profile::builder("core.write", Role::new("write"), "guessing", "1.0.0")
            .determinism(Determinism::Nondeterministic, "it varies")
            .axes(axes(&[Dependency::Host], &[Effect::Filesystem]))
            .resolving("resolve from the target address class"),
    );
    let fault = select(&catalog, "core.write", "write", AddressClass::Path)
        .expect_err("nondeterministic under a deterministic row");
    assert!(matches!(fault, ProfileFault::OutOfBounds { .. }));
}

#[test]
fn a_profile_serves_only_the_classes_it_declares() {
    let catalog = catalog().with(
        complete(
            "core.write",
            "write",
            "posix",
            axes(&[Dependency::Host], &[Effect::Filesystem]),
        )
        .serving(TargetClass::Only(vec![AddressClass::Path])),
    );
    select(&catalog, "core.write", "write", AddressClass::Path)
        .expect("the declared class selects it");
    let fault = select(&catalog, "core.write", "write", AddressClass::Uri)
        .expect_err("a URI target selects no PATH profile");
    assert!(matches!(fault, ProfileFault::Missing { .. }));
}

#[test]
fn a_deterministic_row_stays_deterministic() {
    let catalog = catalog();
    let profile = complete(
        "core.write",
        "write",
        "posix",
        axes(&[Dependency::Host], &[Effect::Filesystem]),
    );
    assert_eq!(
        catalog.resolve_determinism("core.write", &[&profile], None),
        Determinism::Deterministic
    );
}

#[test]
fn a_nondeterministic_row_resolves_deterministic_only_with_an_exact_source() {
    let catalog = catalog();

    // core.generate is a nondeterministic base row.
    let varying = Profile::builder("core.generate", Role::new("generation"), "llm", "1.0.0")
        .determinism(Determinism::Nondeterministic, "sampling varies")
        .resolving("resolve model from the generation profile");
    assert_eq!(
        catalog.resolve_determinism("core.generate", &[&varying], None),
        Determinism::Nondeterministic
    );

    // "may resolve deterministic only when its immutable profile removes every
    // permitted variation and supplies an exact source"
    let fixed = Profile::builder("core.generate", Role::new("generation"), "fixture", "1.0.0")
        .determinism(
            Determinism::Deterministic,
            "one immutable fixture keyed by the exact declared specification",
        )
        .resolving("resolve model from the generation profile");
    assert_eq!(
        catalog.resolve_determinism("core.generate", &[&fixed], None),
        Determinism::Deterministic
    );

    // With no profile selected at all there is nothing that removed the
    // variation, so the row keeps its declared category.
    assert_eq!(
        catalog.resolve_determinism("core.generate", &[], None),
        Determinism::Nondeterministic
    );
}

#[test]
fn the_six_derived_rows_use_their_closed_mappings() {
    let catalog = catalog();
    let deterministic = complete("core.sort", "none", "x", Axes::inert());
    let nondeterministic = Profile::builder("core.publish", Role::new("publication"), "x", "1.0.0")
        .determinism(Determinism::Nondeterministic, "the endpoint may vary")
        .resolving("resolve destination and policy");

    // "core.sort is deterministic for every valid invocation and otherwise
    // fails."
    assert_eq!(
        catalog.resolve_determinism("core.sort", &[], None),
        Determinism::Deterministic
    );

    // "core.verify is deterministic exactly when its immutable verification
    // profile is deterministic".
    assert_eq!(
        catalog.resolve_determinism("core.verify", &[&deterministic], None),
        Determinism::Deterministic
    );
    assert_eq!(
        catalog.resolve_determinism("core.verify", &[], None),
        Determinism::Nondeterministic
    );

    // "core.publish copies the publication-profile category".
    assert_eq!(
        catalog.resolve_determinism("core.publish", &[&nondeterministic], None),
        Determinism::Nondeterministic
    );

    // "core.test is deterministic in comparison-only mode and otherwise copies
    // the graph category."
    assert_eq!(
        catalog.resolve_determinism("core.test", &[], None),
        Determinism::Deterministic
    );
    assert_eq!(
        catalog.resolve_determinism("core.test", &[], Some(Determinism::Nondeterministic)),
        Determinism::Nondeterministic
    );

    // "core.execute copies the execution-profile category in non-graph mode and
    // the graph category in graph mode."
    assert_eq!(
        catalog.resolve_determinism("core.execute", &[&deterministic], None),
        Determinism::Deterministic
    );
    assert_eq!(
        catalog.resolve_determinism("core.execute", &[], Some(Determinism::Nondeterministic)),
        Determinism::Nondeterministic
    );

    // "core.download is deterministic exactly when its source profile fixes one
    // immutable source identity and content snapshot and both its source and
    // transfer profiles are deterministic".
    assert_eq!(
        catalog.resolve_determinism("core.download", &[&deterministic, &deterministic], None),
        Determinism::Deterministic,
        "both roles resolved and both deterministic"
    );
    assert_eq!(
        catalog.resolve_determinism("core.download", &[&deterministic], None),
        Determinism::Nondeterministic,
        "one of the two required roles is unresolved"
    );
}

#[test]
fn core_retry_inherits_the_wrapped_action_category() {
    // "The determinism category and source are inherited exactly from the
    // wrapped ACTION contract." Retry itself decides nothing.
    let catalog = catalog();
    assert_eq!(
        catalog.resolve_determinism("core.retry", &[], Some(Determinism::Deterministic)),
        Determinism::Deterministic
    );
    assert_eq!(
        catalog.resolve_determinism("core.retry", &[], Some(Determinism::Nondeterministic)),
        Determinism::Nondeterministic
    );
}

#[test]
fn an_unregistered_operation_never_resolves_deterministic() {
    // Failing closed: an operation this catalog does not know is not promoted
    // to a determinism promise it never made.
    let catalog = catalog();
    assert_eq!(
        catalog.resolve_determinism("vendor.unknown", &[], None),
        Determinism::Nondeterministic
    );
}
