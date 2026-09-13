//! The profiles the shipped adapters declare about themselves.
//!
//! `axis_contract/implementation_profile` requires that an operation naming a
//! profile role select "exactly one immutable profile for each role before
//! effects", carrying ten properties including the determinism category and the
//! concrete axes the implementation actually uses. A profile is therefore a
//! *claim an implementation makes*, and these are the claims the filesystem
//! adapter in this crate makes about itself.
//!
//! Installing them is deliberate rather than automatic. An engine with no
//! profiles installed refuses every row that requires one, before effects,
//! which is the correct behavior for an engine with no implementations — so
//! acquiring an implementation has to be something an embedder does on purpose.

use lcl_capabilities::profile::axes;
use lcl_capabilities::{AddressClass, Dependency, Determinism, Effect, Profile, Role, TargetClass};

/// The identifier the filesystem adapter in this crate publishes.
pub const FILESYSTEM_IMPLEMENTATION: &str = "lcl.stdlib.filesystem";

/// The version of that implementation's behavior.
pub const FILESYSTEM_VERSION: &str = "0.1.0";

/// Every profile the shipped filesystem adapter declares.
///
/// Each narrows its row to exactly the axes a filesystem implementation
/// selects: a `host` dependency and, for the rows that change something, a
/// `filesystem` effect. None claims `network` or `state`, because this adapter
/// reaches neither — and a profile "may narrow those concrete sets but never
/// widen its row".
pub fn filesystem_profiles() -> Vec<Profile> {
    let mutating = [
        ("core.create", "target"),
        ("core.write", "write"),
        ("core.modify", "change"),
        ("core.delete", "delete"),
        ("core.rename", "rename"),
        ("core.copy", "copy"),
        ("core.move", "move"),
    ];
    let mut profiles: Vec<Profile> = mutating
        .into_iter()
        .map(|(operation, role)| {
            profile(operation, role)
                .axes(axes(&[Dependency::Host], &[Effect::Filesystem]))
                .resolving(
                    "Classify the resolved PATH target and destination, and select the \
                     filesystem effect and host dependency each side's address class resolves.",
                )
        })
        .collect();

    // core.convert and core.generate name roles this adapter does not fill:
    // conversion and generation are content decisions, not byte moves. Leaving
    // them uninstalled is what makes their precondition fail honestly.
    profiles.push(
        profile("core.publish", "publication")
            .serving(TargetClass::Only(vec![AddressClass::Path]))
            .axes(axes(&[Dependency::Host], &[Effect::Filesystem]))
            .resolving(
                "A PATH destination adds host dependency and filesystem effect; this \
                     adapter publishes to a filesystem destination only.",
            ),
    );
    profiles
}

/// The identifier the in-language checking implementation publishes.
pub const CHECKING_IMPLEMENTATION: &str = "lcl.stdlib.checking";

/// The profile the in-language verifier declares about itself.
///
/// `core.verify` requires the `verification` role, and this implementation
/// fills it by evaluating the declared assertion with the language's own
/// evaluator: "its resolved assertion evaluation is always deterministic", and
/// it selects no external capability at all.
pub fn checking_profiles() -> Vec<Profile> {
    vec![Profile::builder(
        "core.verify",
        Role::new("verification"),
        CHECKING_IMPLEMENTATION,
        FILESYSTEM_VERSION,
    )
    .serving(TargetClass::Any)
    .determinism(
        Determinism::Deterministic,
        "The declared assertion is evaluated by the language's own evaluator over the \
         exact declared values, which fixes the result.",
    )
    .axes(axes(&[], &[]))
    .resolving(
        "Evaluate the declared assertion over the resolved target; the invocation selects \
         no external dependency and no effect.",
    )]
}

/// The identifier the engine's own MEMORY and STATE stores publish.
pub const STORE_IMPLEMENTATION: &str = "lcl.stdlib.store";

/// The profiles the engine's own MEMORY and STATE stores declare about
/// themselves.
///
/// `axis_contract/implementation_profile/required_roles_by_operation` names the
/// `storage` role for every `core.memory_write` and every `core.state_update`
/// invocation. The rows' resolutions begin "Resolve the authorized MEMORY
/// storage profile" and "Resolve the authorized STATE storage profile".
///
/// The stores this crate writes are the engine's own, reaching no host
/// resource. Each profile therefore narrows its row's `host` dependency to none
/// and keeps exactly the row's one effect class, which a profile "may narrow
/// ... but never widen". Both rows are deterministic base rows, and "A profile
/// selected by a deterministic base row must declare deterministic".
///
/// Installing them is deliberate, as for every other profile set. An engine
/// that installs none refuses both rows before effects.
pub fn store_profiles() -> Vec<Profile> {
    [
        ("core.memory_write", AddressClass::Memory, Effect::Memory),
        ("core.state_update", AddressClass::State, Effect::State),
    ]
    .into_iter()
    .map(|(operation, class, effect)| {
        Profile::builder(
            operation,
            Role::new("storage"),
            STORE_IMPLEMENTATION,
            FILESYSTEM_VERSION,
        )
        .serving(TargetClass::Only(vec![class]))
        .determinism(
            Determinism::Deterministic,
            "The exact store snapshot, the declared value, the merge flag or expected-before \
             guard, and one immutable implementation version fix the post-state.",
        )
        .axes(axes(&[], &[effect]))
        .resolving(
            "Write the declared engine-owned store in place within one invocation; the \
             invocation selects no external dependency and exactly the row's one effect class.",
        )
    })
    .collect()
}

/// The identifier the process adapter in this crate publishes.
pub const PROCESS_IMPLEMENTATION: &str = "lcl.stdlib.process";

/// The identifier the transport adapter in this crate publishes.
pub const TRANSPORT_IMPLEMENTATION: &str = "lcl.stdlib.transport";

/// Every profile the shipped process adapter declares.
///
/// `core.execute` names its role only in `non_graph` mode — "graph mode has no
/// local execution profile" — so exactly the three rows that run a program are
/// covered here.
pub fn process_profiles() -> Vec<Profile> {
    [
        ("core.execute", "execution"),
        ("core.start", "start"),
        ("core.stop", "stop"),
    ]
    .into_iter()
    .map(|(operation, role)| {
        // `core.start` and `core.stop` are deterministic base rows, and "A
        // profile selected by a deterministic base row must declare
        // deterministic". `core.execute` is derived, and copies whatever its
        // execution profile declares — so it is the one row here that may
        // honestly say a program decides its own output.
        let (category, source) = if operation == "core.execute" {
            (
                Determinism::Nondeterministic,
                "The executed program decides its own output; this adapter fixes only the \
                 exact program, arguments, working directory and environment it was given.",
            )
        } else {
            (
                Determinism::Deterministic,
                "The exact program, arguments, working directory, environment and one \
                 immutable implementation version fix whether the process started or \
                 stopped.",
            )
        };
        Profile::builder(
            operation,
            Role::new(role),
            PROCESS_IMPLEMENTATION,
            FILESYSTEM_VERSION,
        )
        .serving(TargetClass::Any)
        .determinism(category, source)
        // Exactly the one class this adapter selects. `core.start` and
        // `core.stop` admit only {process, state}, so a profile claiming
        // `package` would be widening two of the three rows it serves.
        .axes(axes(&[Dependency::Host], &[Effect::Process]))
        .resolving(
            "The exact executable, arguments, working directory and environment select \
             one immutable execution, whose rule always includes the process effect.",
        )
    })
    .collect()
}

/// Every profile the shipped transport adapter declares.
pub fn transport_profiles() -> Vec<Profile> {
    let mut profiles: Vec<Profile> = [
        ("core.download", "source"),
        ("core.download", "transfer"),
        ("core.upload", "transfer"),
        ("core.send", "transport"),
    ]
    .into_iter()
    .map(|(operation, role)| {
        let effects: &[Effect] = if operation == "core.send" {
            // core.send admits exactly {message}.
            &[Effect::Message]
        } else {
            &[Effect::Network, Effect::Filesystem]
        };
        transport_profile(operation, role, effects).serving(TargetClass::Any)
    })
    .collect();
    // `core.publish` already has a PATH profile from the filesystem adapter;
    // this one serves the URI destinations that one cannot.
    profiles.push(
        transport_profile(
            "core.publish",
            "publication",
            &[Effect::Network, Effect::Filesystem],
        )
        .serving(TargetClass::Only(vec![AddressClass::Uri])),
    );
    profiles
}

fn transport_profile(operation: &str, role: &str, effects: &[Effect]) -> Profile {
    // `core.send` and `core.upload` are deterministic base rows: what they do
    // is fixed by the exact body and address they are given, and neither reads
    // an answer back into the language. `core.download` and `core.publish` are
    // derived, and a `source` role that cannot fix an immutable content
    // snapshot says so, which is what makes `core.download` resolve
    // nondeterministic.
    let (category, source) = if matches!(operation, "core.send" | "core.upload") {
        (
            Determinism::Deterministic,
            "The exact address, the exact body and one immutable implementation version \
             fix what is transferred.",
        )
    } else {
        (
            Determinism::Nondeterministic,
            "The addressed endpoint decides its own content; this adapter fixes only the \
             exact address and body it was given.",
        )
    };
    Profile::builder(
        operation,
        Role::new(role),
        TRANSPORT_IMPLEMENTATION,
        FILESYSTEM_VERSION,
    )
    .determinism(category, source)
    .axes(axes(&[Dependency::Host, Dependency::Network], effects))
    .resolving(
        "Resolve the address class of each side independently: a URI side adds the \
         network dependency and effect, and a PATH side adds the host dependency and \
         filesystem effect.",
    )
}

fn profile(operation: &str, role: &str) -> Profile {
    Profile::builder(
        operation,
        Role::new(role),
        FILESYSTEM_IMPLEMENTATION,
        FILESYSTEM_VERSION,
    )
    .serving(TargetClass::Only(vec![AddressClass::Path]))
    .determinism(
        Determinism::Deterministic,
        "The exact resolved path, the exact declared content and one immutable \
         implementation version fix the resulting bytes.",
    )
}
