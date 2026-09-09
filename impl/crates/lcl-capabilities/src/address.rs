//! Address classes, and the two closed axis vocabularies they resolve.
//!
//! Authority: `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt`,
//! `10_REGISTRIES/operations_v0.1.0.json#/axis_contract` and the per-row
//! `invocation_resolution` sentences.
//!
//! ## Maxima are not invocation sets
//!
//! The single rule this module exists to enforce:
//!
//! > possible_dependencies and possible_effects are registry maxima, not claims
//! > that every listed capability is selected by every invocation. The actual
//! > invocation sets resolve from the target, source, destination, selected
//! > profile, arguments, and referenced execution graph.
//!
//! So a `core.write` whose target is a `PATH` resolves `{filesystem}`, and the
//! same row whose target is an `OUTPUT` resolves `{state}` — one row, two
//! invocation sets, neither of them the row's three-class maximum.
//!
//! ## Syntax never decides an address class
//!
//! > A REFERENCE used as an address is classified by the address class of its
//! > resolved target, never by REFERENCE syntax.
//!
//! This type therefore names *resolved* classes only. There is no `Reference`
//! variant to accidentally classify by spelling; the caller resolves first and
//! classifies what it found.
//!
//! ## Empty sets are the sentinels
//!
//! > For bounds checking, declared_state_only denotes an empty set of external
//! > dependency classes and none denotes an empty set of concrete effect
//! > classes.
//!
//! [`Axes`] holds only concrete members, so `declared_state_only` and `none`
//! are the empty sets rather than members that could be combined with a
//! concrete class. That makes "exclusive singleton" a property of the type
//! instead of an invariant someone has to remember to check.

use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::BTreeSet;
use std::fmt;

/// One external dependency class. `declared_state_only` is the empty set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dependency {
    Host,
    Network,
    Model,
    Human,
}

impl Dependency {
    pub const ALL: [Dependency; 4] = [
        Dependency::Host,
        Dependency::Network,
        Dependency::Model,
        Dependency::Human,
    ];

    /// The registry sentinel for "no external dependency class".
    pub const NONE: &'static str = "declared_state_only";

    pub fn as_registry_str(self) -> &'static str {
        match self {
            Dependency::Host => "host",
            Dependency::Network => "network",
            Dependency::Model => "model",
            Dependency::Human => "human",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<Dependency> {
        Dependency::ALL
            .into_iter()
            .find(|d| d.as_registry_str() == s)
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// One concrete effect class. `none` is the empty set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    Filesystem,
    Network,
    Process,
    Package,
    Message,
    Memory,
    State,
}

impl Effect {
    pub const ALL: [Effect; 7] = [
        Effect::Filesystem,
        Effect::Network,
        Effect::Process,
        Effect::Package,
        Effect::Message,
        Effect::Memory,
        Effect::State,
    ];

    /// The registry sentinel for "no concrete effect class".
    pub const NONE: &'static str = "none";

    pub fn as_registry_str(self) -> &'static str {
        match self {
            Effect::Filesystem => "filesystem",
            Effect::Network => "network",
            Effect::Process => "process",
            Effect::Package => "package",
            Effect::Message => "message",
            Effect::Memory => "memory",
            Effect::State => "state",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<Effect> {
        Effect::ALL.into_iter().find(|e| e.as_registry_str() == s)
    }
}

impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// One invocation's resolved dependency and effect sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Axes {
    pub dependencies: BTreeSet<Dependency>,
    pub effects: BTreeSet<Effect>,
}

impl Axes {
    /// `declared_state_only` and `none`: the invocation that selects nothing.
    pub fn inert() -> Axes {
        Axes::default()
    }

    pub fn with_dependency(mut self, dependency: Dependency) -> Axes {
        self.dependencies.insert(dependency);
        self
    }

    pub fn with_effect(mut self, effect: Effect) -> Axes {
        self.effects.insert(effect);
        self
    }

    /// Union two resolved sets. Source and destination resolve independently,
    /// "so one operation may produce more than one concrete effect class".
    pub fn union(mut self, other: &Axes) -> Axes {
        self.dependencies.extend(other.dependencies.iter().copied());
        self.effects.extend(other.effects.iter().copied());
        self
    }

    /// True when every member is inside `maximum`.
    ///
    /// "A core profile may narrow those concrete sets but never widen its row."
    pub fn within(&self, maximum: &Axes) -> bool {
        self.dependencies.is_subset(&maximum.dependencies)
            && self.effects.is_subset(&maximum.effects)
    }

    /// The concrete effect classes this set selects outside `maximum`.
    pub fn effects_outside(&self, maximum: &Axes) -> Vec<Effect> {
        self.effects.difference(&maximum.effects).copied().collect()
    }

    /// The dependency classes this set selects outside `maximum`.
    pub fn dependencies_outside(&self, maximum: &Axes) -> Vec<Dependency> {
        self.dependencies
            .difference(&maximum.dependencies)
            .copied()
            .collect()
    }

    pub fn is_effect_free(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn is_declared_state_only(&self) -> bool {
        self.dependencies.is_empty()
    }

    /// Read one registry axis list, mapping the sentinels to the empty set.
    ///
    /// `inherited` is the `core.retry` marker and selects nothing here; that row
    /// copies the wrapped ACTION's resolved sets instead.
    pub fn from_registry(dependencies: &[String], effects: &[String]) -> Axes {
        Axes {
            dependencies: dependencies
                .iter()
                .filter_map(|d| Dependency::from_registry_str(d))
                .collect(),
            effects: effects
                .iter()
                .filter_map(|e| Effect::from_registry_str(e))
                .collect(),
        }
    }

    /// The registry spelling of the dependency set, sentinel included.
    pub fn dependency_names(&self) -> Vec<String> {
        if self.dependencies.is_empty() {
            return vec![Dependency::NONE.to_string()];
        }
        self.dependencies.iter().map(|d| d.to_string()).collect()
    }

    /// The registry spelling of the effect set, sentinel included.
    pub fn effect_names(&self) -> Vec<String> {
        if self.effects.is_empty() {
            return vec![Effect::NONE.to_string()];
        }
        self.effects.iter().map(|e| e.to_string()).collect()
    }
}

/// The resolved class of one address, never its syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AddressClass {
    /// A filesystem path.
    Path,
    /// A network-addressed resource.
    Uri,
    /// A declared `OUTPUT`.
    Output,
    /// A declared `MEMORY`.
    Memory,
    /// A declared `STATE`.
    State,
    /// An authorized addressable object the host supplies that is not a path,
    /// a URI, MEMORY, STATE or OUTPUT.
    HostBound,
    /// A material value. Addressable operations reject it; analytical ones
    /// read it without any external capability.
    Material,
}

impl AddressClass {
    pub const ALL: [AddressClass; 7] = [
        AddressClass::Path,
        AddressClass::Uri,
        AddressClass::Output,
        AddressClass::Memory,
        AddressClass::State,
        AddressClass::HostBound,
        AddressClass::Material,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AddressClass::Path => "PATH",
            AddressClass::Uri => "URI",
            AddressClass::Output => "OUTPUT",
            AddressClass::Memory => "MEMORY",
            AddressClass::State => "STATE",
            AddressClass::HostBound => "host_bound",
            AddressClass::Material => "material",
        }
    }

    /// Whether an addressable operation may take this class as a target.
    pub fn is_addressable(self) -> bool {
        !matches!(self, AddressClass::Material)
    }

    /// `MEMORY` and `STATE`, which the mutating rows prohibit:
    ///
    /// > core.create, core.write, core.append, core.modify, core.rename,
    /// > core.delete, and core.generate prohibit MEMORY and STATE targets;
    /// > core.move prohibits MEMORY and STATE sources. Those mutations use
    /// > core.memory_write or core.state_update.
    pub fn is_internal_store(self) -> bool {
        matches!(self, AddressClass::Memory | AddressClass::State)
    }

    /// Reading this address: its required dependency, and never an effect.
    ///
    /// > Resolve host for PATH, host-bound, or host-backed REFERENCE access and
    /// > host plus network for URI access; otherwise resolve
    /// > declared_state_only.
    ///
    /// and, on the source side of a transfer:
    ///
    /// > observing a PATH, MEMORY, STATE, OUTPUT, or other host-bound source
    /// > adds its required dependency but no source-side effect.
    pub fn observation(self) -> Axes {
        match self {
            AddressClass::Path | AddressClass::HostBound => {
                Axes::inert().with_dependency(Dependency::Host)
            }
            AddressClass::Uri => Axes::inert()
                .with_dependency(Dependency::Host)
                .with_dependency(Dependency::Network),
            AddressClass::Memory | AddressClass::State => {
                Axes::inert().with_dependency(Dependency::Host)
            }
            // An OUTPUT is internal declared state, and a material value is not
            // an address at all.
            AddressClass::Output | AddressClass::Material => Axes::inert(),
        }
    }

    /// Mutating this address: exactly one concrete effect and its dependency.
    ///
    /// > PATH mutation adds host dependency and filesystem effect; URI mutation
    /// > adds network dependency and network effect; OUTPUT or another
    /// > authorized non-filesystem, non-network, non-memory, non-STATE
    /// > addressable target adds state effect and its required dependency.
    pub fn mutation(self) -> Axes {
        match self {
            AddressClass::Path => Axes::inert()
                .with_dependency(Dependency::Host)
                .with_effect(Effect::Filesystem),
            AddressClass::Uri => Axes::inert()
                .with_dependency(Dependency::Network)
                .with_effect(Effect::Network),
            AddressClass::Output => Axes::inert().with_effect(Effect::State),
            AddressClass::HostBound => Axes::inert()
                .with_dependency(Dependency::Host)
                .with_effect(Effect::State),
            AddressClass::Memory => Axes::inert()
                .with_dependency(Dependency::Host)
                .with_effect(Effect::Memory),
            AddressClass::State => Axes::inert()
                .with_dependency(Dependency::Host)
                .with_effect(Effect::State),
            // Not an address: it resolves no concrete effect, which the caller
            // reports as error.operation.precondition.
            AddressClass::Material => Axes::inert(),
        }
    }
}

impl fmt::Display for AddressClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A mirrored vocabulary that the registry does not agree with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabularyMismatch {
    pub axis: &'static str,
    pub missing_from_build: Vec<String>,
    pub missing_from_registry: Vec<String>,
}

impl fmt::Display for VocabularyMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} vocabulary disagrees with the registry: missing from build {:?}, missing from registry {:?}",
            self.axis, self.missing_from_build, self.missing_from_registry
        )
    }
}

/// Check both mirrored vocabularies against `axis_contract`.
///
/// The contract requires mirrored enums to carry explicit parity tests. This is
/// that check as a function, so it runs in a test and can also run at load time.
pub fn verify_vocabulary(spec: &SpecPackage) -> Result<(), VocabularyMismatch> {
    let axis = spec
        .registry("operations")
        .and_then(|r| r.get("axis_contract"))
        .ok_or_else(|| VocabularyMismatch {
            axis: "axis_contract",
            missing_from_build: Vec::new(),
            missing_from_registry: vec!["operations#/axis_contract".to_string()],
        })?;

    let dependencies = definition_keys(axis, "dependency_definitions");
    let mut build: BTreeSet<String> = Dependency::ALL.iter().map(|d| d.to_string()).collect();
    build.insert(Dependency::NONE.to_string());
    compare("dependency", &build, &dependencies)?;

    let effects = definition_keys(axis, "effect_definitions");
    let mut build: BTreeSet<String> = Effect::ALL.iter().map(|e| e.to_string()).collect();
    build.insert(Effect::NONE.to_string());
    compare("effect", &build, &effects)
}

fn definition_keys(axis: &Json, member: &str) -> BTreeSet<String> {
    axis.get(member)
        .and_then(|d| d.as_object())
        .map(|members| members.iter().map(|(k, _)| k.clone()).collect())
        .unwrap_or_default()
}

fn compare(
    axis: &'static str,
    build: &BTreeSet<String>,
    registry: &BTreeSet<String>,
) -> Result<(), VocabularyMismatch> {
    let missing_from_build: Vec<String> = registry.difference(build).cloned().collect();
    let missing_from_registry: Vec<String> = build.difference(registry).cloned().collect();
    if missing_from_build.is_empty() && missing_from_registry.is_empty() {
        return Ok(());
    }
    Err(VocabularyMismatch {
        axis,
        missing_from_build,
        missing_from_registry,
    })
}
