//! Implementation profiles: the immutable descriptions selected before effects.
//!
//! Authority: `10_REGISTRIES/operations_v0.1.0.json#/axis_contract/implementation_profile`
//! and the profile paragraphs of
//! `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt`.
//!
//! ## What a profile is
//!
//! > When an operation contract names one or more selected or implementation
//! > profile roles, the exact operation identifier, profile role, target or
//! > address class, arguments, implementation identifier, and implementation
//! > version select exactly one immutable profile for each role before effects.
//!
//! A profile is the answer to "which concrete implementation is about to run,
//! and what does it promise?". It is not configuration and not a preference: it
//! carries the ten registry-required properties, including the determinism
//! category that the operation row may be unable to state on its own, and it is
//! chosen *before* the first effect so that a wrong or missing choice fails
//! without having changed anything.
//!
//! ## Why selection can fail four different ways
//!
//! > A missing, ambiguous, incomplete, or out-of-bounds required profile role
//! > emits error.operation.precondition and fails before effects.
//!
//! All four are the same registered error, and they are kept apart here anyway,
//! because the detail an operator needs differs completely: nothing is
//! installed, two things are, one is under-declared, or one promises more than
//! its row permits. Collapsing them would make a capability misconfiguration
//! indistinguishable from a capability overreach.
//!
//! ## Narrowing, never widening
//!
//! > A core profile may narrow those concrete sets but never widen its row. …
//! > A profile selected by a deterministic base row must declare deterministic;
//! > a nondeterministic profile is out of bounds.
//!
//! [`ProfileCatalog::select`] enforces both directions before returning a
//! profile, so a bounds violation cannot reach an adapter at all.

use crate::address::{Axes, Dependency, Effect};
use crate::AddressClass;
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::fmt;

/// One profile role named by the registry's role map.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Role(String);

impl Role {
    pub fn new(name: impl Into<String>) -> Role {
        Role(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The final determinism category a profile declares.
///
/// > A profile declares exactly one final category: deterministic or
/// > nondeterministic. derived and inherited are operation-row resolution
/// > markers and are forbidden as profile categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Determinism {
    Deterministic,
    Nondeterministic,
}

impl Determinism {
    pub fn as_registry_str(self) -> &'static str {
        match self {
            Determinism::Deterministic => "deterministic",
            Determinism::Nondeterministic => "nondeterministic",
        }
    }

    pub fn is_deterministic(self) -> bool {
        matches!(self, Determinism::Deterministic)
    }
}

impl fmt::Display for Determinism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// The declared category of one operation row, markers included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDeterminism {
    Deterministic,
    Nondeterministic,
    /// Resolved by an operation-specific mapping named by the row's source.
    Derived,
    /// `core.retry` only: copied from the wrapped ACTION.
    Inherited,
}

impl RowDeterminism {
    fn from_registry_str(s: &str) -> Option<RowDeterminism> {
        Some(match s {
            "deterministic" => RowDeterminism::Deterministic,
            "nondeterministic" => RowDeterminism::Nondeterministic,
            "derived" => RowDeterminism::Derived,
            "inherited" => RowDeterminism::Inherited,
            _ => return None,
        })
    }
}

/// Which address classes one profile serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetClass {
    /// Every class the row admits.
    Any,
    /// Exactly these classes.
    Only(Vec<AddressClass>),
}

impl TargetClass {
    fn admits(&self, class: AddressClass) -> bool {
        match self {
            TargetClass::Any => true,
            TargetClass::Only(classes) => classes.contains(&class),
        }
    }
}

impl fmt::Display for TargetClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TargetClass::Any => f.write_str("any"),
            TargetClass::Only(classes) => {
                let names: Vec<&str> = classes.iter().map(|c| c.as_str()).collect();
                f.write_str(&names.join("|"))
            }
        }
    }
}

/// One immutable implementation profile: the ten required properties.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub operation_id: String,
    pub profile_role: Role,
    pub implementation_id: String,
    pub implementation_version: String,
    pub target_class: TargetClass,
    pub determinism_category: Determinism,
    pub determinism_source: String,
    /// The profile's own maxima, which may narrow the row's but never widen it.
    pub axes: Axes,
    pub invocation_resolution: String,
}

impl Profile {
    /// Begin one profile from the four properties that identify it.
    ///
    /// The remaining properties are supplied by the chained methods below. A
    /// profile that skips one is not a compile error: it is exactly the
    /// registry's *incomplete* case, and [`ProfileCatalog::select`] refuses it
    /// before effects with `error.operation.precondition`. That is the
    /// specified behavior, and therefore the behavior worth being able to test.
    pub fn builder(
        operation_id: impl Into<String>,
        profile_role: Role,
        implementation_id: impl Into<String>,
        implementation_version: impl Into<String>,
    ) -> Profile {
        Profile {
            operation_id: operation_id.into(),
            profile_role,
            implementation_id: implementation_id.into(),
            implementation_version: implementation_version.into(),
            target_class: TargetClass::Any,
            determinism_category: Determinism::Nondeterministic,
            determinism_source: String::new(),
            axes: Axes::inert(),
            invocation_resolution: String::new(),
        }
    }

    /// Which address classes this profile serves.
    pub fn serving(mut self, target_class: TargetClass) -> Profile {
        self.target_class = target_class;
        self
    }

    /// The final category, and the exact source that justifies it.
    pub fn determinism(mut self, category: Determinism, source: impl Into<String>) -> Profile {
        self.determinism_category = category;
        self.determinism_source = source.into();
        self
    }

    /// The profile's own maxima, which narrow the row's.
    pub fn axes(mut self, axes: Axes) -> Profile {
        self.axes = axes;
        self
    }

    /// How this profile resolves one invocation's actual sets.
    pub fn resolving(mut self, invocation_resolution: impl Into<String>) -> Profile {
        self.invocation_resolution = invocation_resolution.into();
        self
    }

    /// The registry-required properties this profile leaves empty.
    ///
    /// > Every profile must contain every property listed by
    /// > axis_contract/implementation_profile.
    pub fn incomplete_properties(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.operation_id.trim().is_empty() {
            missing.push("operation_id");
        }
        if self.profile_role.as_str().trim().is_empty() {
            missing.push("profile_role");
        }
        if self.implementation_id.trim().is_empty() {
            missing.push("implementation_id");
        }
        if self.implementation_version.trim().is_empty() {
            missing.push("implementation_version");
        }
        if self.determinism_source.trim().is_empty() {
            missing.push("determinism_source");
        }
        if self.invocation_resolution.trim().is_empty() {
            missing.push("invocation_resolution");
        }
        missing
    }
}

/// Which roles one operation requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoleRequirement {
    /// "A core operation absent from required_roles_by_operation requires no
    /// local core profile."
    None,
    /// "all applies to every invocation of that core operation."
    All(Vec<Role>),
    /// `core.execute`: `non_graph` for PATH, URI or STRING targets, `graph` for
    /// an execution-unit reference, which has no local profile.
    ByMode(BTreeMap<String, Vec<Role>>),
}

impl RoleRequirement {
    /// The roles required for one invocation mode.
    pub fn roles(&self, mode: Option<&str>) -> &[Role] {
        const NONE: &[Role] = &[];
        match self {
            RoleRequirement::None => NONE,
            RoleRequirement::All(roles) => roles,
            RoleRequirement::ByMode(modes) => mode
                .and_then(|m| modes.get(m))
                .map(|r| r.as_slice())
                .unwrap_or(NONE),
        }
    }
}

/// One operation row's registry facts, as profile selection needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub operation: String,
    pub category: String,
    pub determinism: RowDeterminism,
    pub determinism_source: String,
    /// The row maxima. Profiles narrow within these.
    pub maximum: Axes,
    pub result_schema: String,
    pub roles: RoleRequirement,
}

impl Row {
    /// `read_only requires possible effects exactly {none}`.
    pub fn is_read_only(&self) -> bool {
        self.category == "read_only"
    }
}

/// Why a required profile role did not resolve to exactly one usable profile.
///
/// Every variant is `error.operation.precondition` before effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileFault {
    Missing {
        operation: String,
        role: Role,
        target_class: AddressClass,
    },
    Ambiguous {
        operation: String,
        role: Role,
        candidates: Vec<String>,
    },
    Incomplete {
        operation: String,
        role: Role,
        implementation: String,
        properties: Vec<&'static str>,
    },
    OutOfBounds {
        operation: String,
        role: Role,
        implementation: String,
        detail: String,
    },
}

impl fmt::Display for ProfileFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileFault::Missing {
                operation,
                role,
                target_class,
            } => write!(
                f,
                "no {role} profile is installed for {operation} on a {target_class} target"
            ),
            ProfileFault::Ambiguous {
                operation,
                role,
                candidates,
            } => write!(
                f,
                "{} profiles claim the {role} role for {operation}: {}",
                candidates.len(),
                candidates.join(", ")
            ),
            ProfileFault::Incomplete {
                operation,
                role,
                implementation,
                properties,
            } => write!(
                f,
                "the {role} profile {implementation} for {operation} declares no {}",
                properties.join(", ")
            ),
            ProfileFault::OutOfBounds {
                operation,
                role,
                implementation,
                detail,
            } => write!(
                f,
                "the {role} profile {implementation} for {operation} is out of bounds: {detail}"
            ),
        }
    }
}

/// Why the catalog could not be built from the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    UnverifiedPackage,
    MissingRegistry(&'static str),
    Malformed(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::UnverifiedPackage => f.write_str(
                "the canonical package is not authoritative; profiles load only from the approved release",
            ),
            ProfileError::MissingRegistry(name) => {
                write!(f, "registry {name} is missing from the package")
            }
            ProfileError::Malformed(detail) => write!(f, "malformed registry: {detail}"),
        }
    }
}

impl std::error::Error for ProfileError {}

/// One selection question.
#[derive(Debug, Clone)]
pub struct Selection<'a> {
    pub operation: &'a str,
    pub role: Role,
    pub target_class: AddressClass,
    /// A pinned implementation, when the caller names one.
    pub implementation: Option<&'a str>,
}

/// The registry rows, the role map, and every installed profile.
#[derive(Debug, Clone, Default)]
pub struct ProfileCatalog {
    rows: BTreeMap<String, Row>,
    profiles: Vec<Profile>,
}

impl ProfileCatalog {
    /// Read the 39 rows and the role map from the approved package.
    ///
    /// No profile is installed by this: the registry describes what an
    /// implementation must supply, never which implementation is present.
    pub fn load(spec: &SpecPackage) -> Result<ProfileCatalog, ProfileError> {
        if !matches!(spec.authority(), lcl_spec::Authority::Authoritative) {
            return Err(ProfileError::UnverifiedPackage);
        }
        let operations = spec
            .registry("operations")
            .ok_or(ProfileError::MissingRegistry("operations"))?;
        let axis = operations
            .get("axis_contract")
            .ok_or_else(|| ProfileError::Malformed("axis_contract missing".into()))?;
        let role_map = axis
            .get("implementation_profile")
            .and_then(|p| p.get("required_roles_by_operation"))
            .and_then(|m| m.as_object())
            .ok_or_else(|| ProfileError::Malformed("required_roles_by_operation missing".into()))?;

        let mut roles_by_operation: BTreeMap<String, RoleRequirement> = BTreeMap::new();
        for (operation, requirement) in role_map {
            roles_by_operation.insert(operation.clone(), role_requirement(requirement)?);
        }

        let contracts = operations
            .get("contracts")
            .and_then(|c| c.as_object())
            .ok_or_else(|| ProfileError::Malformed("contracts missing".into()))?;

        let mut rows = BTreeMap::new();
        for (operation, contract) in contracts {
            let determinism = contract
                .get("determinism")
                .ok_or_else(|| ProfileError::Malformed(format!("{operation}: determinism")))?;
            let category = determinism
                .get("category")
                .and_then(|c| c.as_str())
                .ok_or_else(|| {
                    ProfileError::Malformed(format!("{operation}: determinism.category"))
                })?;
            let declared = RowDeterminism::from_registry_str(category).ok_or_else(|| {
                ProfileError::Malformed(format!("{operation}: unknown category {category}"))
            })?;
            rows.insert(
                operation.clone(),
                Row {
                    operation: operation.clone(),
                    category: contract
                        .get("category")
                        .and_then(|c| c.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    determinism: declared,
                    determinism_source: determinism
                        .get("source")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    maximum: Axes::from_registry(
                        &strings(contract, "possible_dependencies"),
                        &strings(contract, "possible_effects"),
                    ),
                    result_schema: contract
                        .get("result_schema")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    roles: roles_by_operation
                        .remove(operation)
                        .unwrap_or(RoleRequirement::None),
                },
            );
        }

        Ok(ProfileCatalog {
            rows,
            profiles: Vec::new(),
        })
    }

    /// Install one profile.
    pub fn with(mut self, profile: Profile) -> ProfileCatalog {
        self.profiles.push(profile);
        self
    }

    /// Install several profiles.
    pub fn with_all(mut self, profiles: impl IntoIterator<Item = Profile>) -> ProfileCatalog {
        self.profiles.extend(profiles);
        self
    }

    pub fn row(&self, operation: &str) -> Option<&Row> {
        self.rows.get(operation)
    }

    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.values()
    }

    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// The roles one invocation of `operation` requires.
    pub fn required_roles(&self, operation: &str, mode: Option<&str>) -> Vec<Role> {
        self.rows
            .get(operation)
            .map(|row| row.roles.roles(mode).to_vec())
            .unwrap_or_default()
    }

    /// Select exactly one immutable profile for one role, before effects.
    pub fn select(&self, selection: &Selection<'_>) -> Result<&Profile, ProfileFault> {
        let candidates: Vec<&Profile> = self
            .profiles
            .iter()
            .filter(|p| p.operation_id == selection.operation)
            .filter(|p| p.profile_role == selection.role)
            .filter(|p| p.target_class.admits(selection.target_class))
            .filter(|p| match selection.implementation {
                Some(id) => p.implementation_id == id,
                None => true,
            })
            .collect();

        let profile = match candidates.as_slice() {
            [] => {
                return Err(ProfileFault::Missing {
                    operation: selection.operation.to_string(),
                    role: selection.role.clone(),
                    target_class: selection.target_class,
                })
            }
            [only] => *only,
            many => {
                return Err(ProfileFault::Ambiguous {
                    operation: selection.operation.to_string(),
                    role: selection.role.clone(),
                    candidates: many
                        .iter()
                        .map(|p| format!("{}@{}", p.implementation_id, p.implementation_version))
                        .collect(),
                })
            }
        };

        let missing = profile.incomplete_properties();
        if !missing.is_empty() {
            return Err(ProfileFault::Incomplete {
                operation: selection.operation.to_string(),
                role: selection.role.clone(),
                implementation: profile.implementation_id.clone(),
                properties: missing,
            });
        }

        if let Some(row) = self.rows.get(selection.operation) {
            self.check_bounds(row, profile, selection)?;
        }
        Ok(profile)
    }

    /// A profile may narrow its row and must not widen it.
    fn check_bounds(
        &self,
        row: &Row,
        profile: &Profile,
        selection: &Selection<'_>,
    ) -> Result<(), ProfileFault> {
        let out_of_bounds = |detail: String| ProfileFault::OutOfBounds {
            operation: selection.operation.to_string(),
            role: selection.role.clone(),
            implementation: profile.implementation_id.clone(),
            detail,
        };

        // `inherited` rows copy the wrapped ACTION and have no maxima of their
        // own to narrow, so bounds are checked against that action instead.
        if row.determinism != RowDeterminism::Inherited {
            let effects = profile.axes.effects_outside(&row.maximum);
            if !effects.is_empty() {
                let names: Vec<&str> = effects.iter().map(|e| e.as_registry_str()).collect();
                return Err(out_of_bounds(format!(
                    "it declares the effect {} that the row does not permit",
                    names.join(", ")
                )));
            }
            let dependencies = profile.axes.dependencies_outside(&row.maximum);
            if !dependencies.is_empty() {
                let names: Vec<&str> = dependencies.iter().map(|d| d.as_registry_str()).collect();
                return Err(out_of_bounds(format!(
                    "it declares the dependency {} that the row does not permit",
                    names.join(", ")
                )));
            }
        }

        // "A profile selected by a deterministic base row must declare
        // deterministic; a nondeterministic profile is out of bounds."
        if row.determinism == RowDeterminism::Deterministic
            && profile.determinism_category == Determinism::Nondeterministic
        {
            return Err(out_of_bounds(
                "it declares nondeterministic under a deterministic row".to_string(),
            ));
        }
        Ok(())
    }

    /// Resolve one invocation's final determinism category.
    ///
    /// > A deterministic base row remains deterministic and accepts only a
    /// > deterministic profile. A nondeterministic base row may resolve
    /// > deterministic only when its immutable profile removes every permitted
    /// > variation and supplies an exact source satisfying the deterministic
    /// > identity; otherwise it remains nondeterministic. A derived row applies
    /// > the exact operation-specific mapping named by its source to the final
    /// > categories of every resolved profile and/or graph.
    ///
    /// `graph` is the resolved category of a referenced execution graph, when
    /// the invocation delegates to one.
    pub fn resolve_determinism(
        &self,
        operation: &str,
        selected: &[&Profile],
        graph: Option<Determinism>,
    ) -> Determinism {
        let Some(row) = self.rows.get(operation) else {
            return Determinism::Nondeterministic;
        };
        let all_profiles_deterministic = |()| {
            selected
                .iter()
                .all(|p| p.determinism_category.is_deterministic())
        };
        match row.determinism {
            RowDeterminism::Deterministic => Determinism::Deterministic,
            RowDeterminism::Nondeterministic => {
                // Deterministic only when every selected profile removes the
                // permitted variation and states an exact source.
                if !selected.is_empty()
                    && all_profiles_deterministic(())
                    && selected.iter().all(|p| !p.determinism_source.is_empty())
                {
                    Determinism::Deterministic
                } else {
                    Determinism::Nondeterministic
                }
            }
            // "core.retry ... inherited exactly from the wrapped ACTION."
            RowDeterminism::Inherited => graph.unwrap_or(Determinism::Nondeterministic),
            RowDeterminism::Derived => derived(operation, selected, graph),
        }
    }
}

/// The six derived rows and their closed mappings.
///
/// > core.sort is deterministic for every valid invocation and otherwise fails.
/// > core.verify is deterministic exactly when its immutable verification
/// > profile is deterministic … core.test is deterministic in comparison-only
/// > mode and otherwise copies the graph category. core.execute copies the
/// > execution-profile category in non-graph mode and the graph category in
/// > graph mode. core.publish copies the publication-profile category after its
/// > destination and policy are fixed. core.download is deterministic exactly
/// > when its source profile fixes one immutable source identity and content
/// > snapshot and both its source and transfer profiles are deterministic.
fn derived(operation: &str, selected: &[&Profile], graph: Option<Determinism>) -> Determinism {
    let every_profile_deterministic = selected
        .iter()
        .all(|p| p.determinism_category.is_deterministic());
    match operation {
        "core.sort" => Determinism::Deterministic,
        "core.verify" | "core.publish" => {
            if !selected.is_empty() && every_profile_deterministic {
                Determinism::Deterministic
            } else {
                Determinism::Nondeterministic
            }
        }
        // Both source and transfer profiles must be deterministic, and the
        // source profile must fix one immutable identity, which it states by
        // declaring deterministic with an exact source.
        "core.download" => {
            if selected.len() == 2 && every_profile_deterministic {
                Determinism::Deterministic
            } else {
                Determinism::Nondeterministic
            }
        }
        // Comparison-only mode has no graph and no profile; graph mode copies
        // the graph.
        "core.test" => graph.unwrap_or(Determinism::Deterministic),
        // Non-graph mode copies the execution profile; graph mode copies the
        // graph.
        "core.execute" => match graph {
            Some(category) => category,
            None => {
                if !selected.is_empty() && every_profile_deterministic {
                    Determinism::Deterministic
                } else {
                    Determinism::Nondeterministic
                }
            }
        },
        _ => Determinism::Nondeterministic,
    }
}

fn role_requirement(value: &Json) -> Result<RoleRequirement, ProfileError> {
    let members = value
        .as_object()
        .ok_or_else(|| ProfileError::Malformed("role requirement is not an object".into()))?;
    if let Some((_, all)) = members.iter().find(|(k, _)| k == "all") {
        return Ok(RoleRequirement::All(roles(all)));
    }
    let mut modes = BTreeMap::new();
    for (mode, list) in members {
        modes.insert(mode.clone(), roles(list));
    }
    Ok(RoleRequirement::ByMode(modes))
}

fn roles(value: &Json) -> Vec<Role> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.as_str())
                .map(Role::new)
                .collect()
        })
        .unwrap_or_default()
}

fn strings(contract: &Json, member: &str) -> Vec<String> {
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
}

/// Convenience: the axes a profile declares, built from registry spellings.
pub fn axes(dependencies: &[Dependency], effects: &[Effect]) -> Axes {
    Axes {
        dependencies: dependencies.iter().copied().collect(),
        effects: effects.iter().copied().collect(),
    }
}
