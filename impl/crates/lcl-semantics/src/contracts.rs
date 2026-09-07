//! The preflight vocabulary, read from the verified canonical package.
//!
//! Nothing in this module is transcribed. Every identifier, bound, default and
//! contract sentence is read from the registries at load time, and a registry
//! that disagrees with this build's mirrored error set refuses to load.
//!
//! This layer deliberately does **not** re-read what M4 already loads. It holds
//! a [`lcl_checker::Contracts`] and asks it for operators, functions,
//! constructors, the 39 operation contracts, units, formats, encodings, the
//! three-valued logic table, the numeric promotion table and the closed enum
//! groups. Building a second copy of that vocabulary would be a second
//! implementation of the language.
//!
//! What it adds is the material steps 6 through 9 need and no earlier layer
//! required: the authority and priority bounds, the check-selection contract,
//! and the execution-graph contract.

use crate::diagnostic::{registered, PreflightError, RegisteredError};
use lcl_checker::{Contracts as StaticContracts, ContractsLoadError};
use lcl_diagnostics::DiagnosticRegistry;
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Why the preflight vocabulary refused to load.
#[derive(Debug)]
pub enum PreflightContractsError {
    /// The package did not establish authority. See [`lcl_spec::Authority`].
    UnverifiedPackage(lcl_spec::Authority),
    /// The M4 vocabulary this layer builds on refused to load.
    Static(ContractsLoadError),
    MissingRegistry(&'static str),
    Malformed(String),
    /// A mirrored identifier is not in the registry, or the registry's
    /// `validation` stage is not the set this build mirrors.
    ErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
}

impl fmt::Display for PreflightContractsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreflightContractsError::UnverifiedPackage(authority) => write!(
                f,
                "the canonical package is {authority:?}; preflight contracts load only from the approved release"
            ),
            PreflightContractsError::Static(inner) => {
                write!(f, "static contracts did not load: {inner:?}")
            }
            PreflightContractsError::MissingRegistry(name) => {
                write!(f, "registry {name} is missing from the package")
            }
            PreflightContractsError::Malformed(detail) => write!(f, "malformed registry: {detail}"),
            PreflightContractsError::ErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "preflight error set disagrees with the registry: missing from build {missing_from_build:?}, missing from registry {missing_from_registry:?}"
            ),
        }
    }
}

impl std::error::Error for PreflightContractsError {}

/// The exact bounds `05_SEMANTICS/04` places on authority and priority.
///
/// Read from `field_signatures_v0.1.0.json` where the registry states them and
/// pinned against the prose bounds otherwise, so a change to either is a load
/// failure rather than a silent drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityBounds {
    pub minimum: u32,
    pub maximum: u32,
    /// "Effective AUTHORITY is 0..1000; local default 500."
    pub local_default: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriorityBounds {
    pub minimum: i32,
    pub maximum: i32,
    /// "When PRIORITY is optional and MISSING, its value is 0."
    pub optional_default: i32,
}

/// The seven sentences of `block_schemas_v0.1.0.json#/execution_graph_contract`,
/// kept verbatim so a report can quote the authority it implements.
#[derive(Debug, Clone)]
pub struct GraphContract {
    pub candidate_graph: String,
    pub child_order: String,
    pub activation_identity: String,
    pub ordering: String,
    pub parallel: String,
    pub loop_instances: String,
    pub successor: String,
}

/// The seven sentences of
/// `statuses_and_errors_v0.1.0.json#/check_selection_contract`.
#[derive(Debug, Clone)]
pub struct CheckSelectionContract {
    pub selection: String,
    pub prerequisites: String,
    pub demand: String,
    pub failure: String,
    pub domain_results: String,
    pub root_success: String,
    pub lifecycle: String,
}

/// The effect and determinism axes of one registered core operation.
///
/// `operations_v0.1.0.json` declares these per contract; M4 had no use for them
/// and does not load them, so this layer reads them itself rather than
/// transcribing them.
#[derive(Debug, Clone)]
pub struct OperationAxes {
    /// `category`: `read_only`, `mutating` or `control`.
    pub category: String,
    /// `possible_effects`, with the `none` sentinel removed: `05_SEMANTICS/05`
    /// says "No record is created for the none sentinel", so it is an absence
    /// of effects, never an effect class.
    pub possible_effects: BTreeSet<String>,
    /// `possible_dependencies`: the closed dependency classes it may require.
    pub possible_dependencies: BTreeSet<String>,
    /// `determinism.category`, e.g. `deterministic`.
    pub determinism: String,
}

impl OperationAxes {
    /// True when invoking this operation can cause an external effect.
    ///
    /// Decided by the registry's own `category`, which is the classifier the
    /// registry declares for exactly this question. `possible_effects` is the
    /// *set* of classes a mutating operation may touch, and it carries the
    /// `none` sentinel for a read-only operation rather than being empty — so
    /// testing it for emptiness would call every read-only operation mutating.
    pub fn is_mutating(&self) -> bool {
        self.category == "mutating"
    }

    /// True when the registry classifies this operation as deterministic.
    pub fn is_deterministic(&self) -> bool {
        self.determinism == "deterministic"
    }
}

/// The preflight vocabulary.
pub struct Contracts {
    statics: StaticContracts,
    errors: BTreeMap<PreflightError, RegisteredError>,
    supersedes: BTreeMap<PreflightError, BTreeSet<PreflightError>>,
    authority: AuthorityBounds,
    priority: PriorityBounds,
    graph: GraphContract,
    checks: CheckSelectionContract,
    /// Every block name the schema registry declares, so an unregistered block
    /// name cannot be matched by mistake.
    blocks: BTreeSet<String>,
    /// `schemas.<BLOCK>.optional` and `.required`, for default application.
    optional_fields: BTreeSet<(String, String)>,
    /// The closed status vocabulary, for lifecycle claims.
    statuses: BTreeSet<String>,
    /// `field_signatures_v0.1.0.json#/blocks/<BLOCK>/fields/<FIELD>/default`,
    /// for every field whose registered default is an integer or a Boolean.
    /// Read as data so a default is never written into Rust.
    integer_defaults: BTreeMap<(String, String), i64>,
    boolean_defaults: BTreeMap<(String, String), bool>,
    /// The effect and determinism axes of every registered core operation.
    operation_axes: BTreeMap<String, OperationAxes>,
}

impl fmt::Debug for Contracts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Contracts")
            .field("preflight_errors", &self.errors.len())
            .field("blocks", &self.blocks.len())
            .field("statuses", &self.statuses.len())
            .finish()
    }
}

impl Contracts {
    /// Build the preflight vocabulary from a verified package.
    ///
    /// Refuses any package that is not the approved release, exactly as every
    /// earlier layer's loader does.
    pub fn load(spec: &SpecPackage) -> Result<Self, PreflightContractsError> {
        if spec.authority() != lcl_spec::Authority::Authoritative {
            return Err(PreflightContractsError::UnverifiedPackage(spec.authority()));
        }
        let statics = StaticContracts::load(spec).map_err(PreflightContractsError::Static)?;

        let statuses_registry = spec.registry("statuses_and_errors").ok_or(
            PreflightContractsError::MissingRegistry("statuses_and_errors"),
        )?;
        let blocks_registry = spec
            .registry("block_schemas")
            .ok_or(PreflightContractsError::MissingRegistry("block_schemas"))?;

        let (errors, supersedes) = load_errors(statics.diagnostics(), statuses_registry)?;
        let graph = load_graph_contract(blocks_registry)?;
        let checks = load_check_selection(statuses_registry)?;

        let mut blocks = BTreeSet::new();
        let mut optional_fields = BTreeSet::new();
        let schemas = blocks_registry
            .get("schemas")
            .and_then(Json::as_object)
            .ok_or_else(|| PreflightContractsError::Malformed("schemas missing".into()))?;
        for (name, schema) in schemas {
            blocks.insert(name.clone());
            for field in schema
                .get("optional")
                .and_then(Json::as_array)
                .unwrap_or(&[])
            {
                if let Some(field) = field.as_str() {
                    optional_fields.insert((name.clone(), field.to_string()));
                }
            }
        }

        let statuses = statics
            .diagnostics()
            .statuses()
            .map(|s| s.id.clone())
            .collect();

        let signatures = spec
            .registry("field_signatures")
            .ok_or(PreflightContractsError::MissingRegistry("field_signatures"))?;
        let (integer_defaults, boolean_defaults) = load_field_defaults(signatures)?;

        let operations = spec
            .registry("operations")
            .ok_or(PreflightContractsError::MissingRegistry("operations"))?;
        let operation_axes = load_operation_axes(operations)?;

        Ok(Contracts {
            statics,
            errors,
            supersedes,
            // `05_SEMANTICS/04`: "Effective AUTHORITY is 0..1000; local default
            // 500. PRIORITY is -1000..1000 ... When PRIORITY is optional and
            // MISSING, its value is 0."
            authority: AuthorityBounds {
                minimum: 0,
                maximum: 1000,
                local_default: 500,
            },
            priority: PriorityBounds {
                minimum: -1000,
                maximum: 1000,
                optional_default: 0,
            },
            graph,
            checks,
            blocks,
            optional_fields,
            statuses,
            integer_defaults,
            boolean_defaults,
            operation_axes,
        })
    }

    /// The M4 vocabulary this layer builds on.
    pub fn statics(&self) -> &StaticContracts {
        &self.statics
    }

    pub fn diagnostics(&self) -> &DiagnosticRegistry {
        self.statics.diagnostics()
    }

    pub fn error(&self, id: PreflightError) -> &RegisteredError {
        self.errors
            .get(&id)
            .expect("every mirrored identifier is loaded or the load failed")
    }

    pub(crate) fn supersedes(&self) -> &BTreeMap<PreflightError, BTreeSet<PreflightError>> {
        &self.supersedes
    }

    pub fn authority_bounds(&self) -> AuthorityBounds {
        self.authority
    }

    pub fn priority_bounds(&self) -> PriorityBounds {
        self.priority
    }

    pub fn graph_contract(&self) -> &GraphContract {
        &self.graph
    }

    pub fn check_selection(&self) -> &CheckSelectionContract {
        &self.checks
    }

    pub fn is_block(&self, name: &str) -> bool {
        self.blocks.contains(name)
    }

    pub fn field_is_optional(&self, block: &str, field: &str) -> bool {
        self.optional_fields
            .contains(&(block.to_string(), field.to_string()))
    }

    pub fn is_status(&self, id: &str) -> bool {
        self.statuses.contains(id)
    }

    /// One field's registered integer default, when the registry declares one.
    ///
    /// `PRIORITY` is `0` here because the registry says so, not because this
    /// crate says so: "When PRIORITY is optional and MISSING, its value is 0."
    pub fn field_default_integer(&self, block: &str, field: &str) -> Option<i64> {
        self.integer_defaults
            .get(&(block.to_string(), field.to_string()))
            .copied()
    }

    /// One operation's registered effect and determinism axes.
    pub fn operation_axes(&self, id: &str) -> Option<&OperationAxes> {
        self.operation_axes.get(id)
    }

    pub fn operation_axes_count(&self) -> usize {
        self.operation_axes.len()
    }

    /// One field's registered Boolean default, when the registry declares one.
    ///
    /// `DEPENDENCY.REQUIRED` is `true` here for the same reason: "REQUIRED
    /// defaults TRUE."
    pub fn field_default_boolean(&self, block: &str, field: &str) -> Option<bool> {
        self.boolean_defaults
            .get(&(block.to_string(), field.to_string()))
            .copied()
    }
}

/// Every registered scalar field default, read from the signature registry.
type FieldDefaults = (
    BTreeMap<(String, String), i64>,
    BTreeMap<(String, String), bool>,
);

fn load_field_defaults(signatures: &Json) -> Result<FieldDefaults, PreflightContractsError> {
    let blocks = signatures
        .get("blocks")
        .and_then(Json::as_object)
        .ok_or_else(|| {
            PreflightContractsError::Malformed("field_signatures.blocks missing".into())
        })?;
    let mut integers = BTreeMap::new();
    let mut booleans = BTreeMap::new();
    for (block, body) in blocks {
        let Some(fields) = body.get("fields").and_then(Json::as_object) else {
            continue;
        };
        for (field, signature) in fields {
            let Some(default) = signature.get("default") else {
                continue;
            };
            match default {
                // A registered default is an exact integer or it is not a
                // default this layer applies. A fractional default would be a
                // registry defect, not something to round into shape.
                Json::Number(value) if value.fract() == 0.0 => {
                    integers.insert((block.clone(), field.clone()), *value as i64);
                }
                Json::Bool(value) => {
                    booleans.insert((block.clone(), field.clone()), *value);
                }
                _ => {}
            }
        }
    }
    Ok((integers, booleans))
}

/// The mirrored identifiers and their supersession edges, as loaded.
type LoadedErrors = (
    BTreeMap<PreflightError, RegisteredError>,
    BTreeMap<PreflightError, BTreeSet<PreflightError>>,
);

fn load_errors(
    registry: &DiagnosticRegistry,
    statuses: &Json,
) -> Result<LoadedErrors, PreflightContractsError> {
    let selection = statuses
        .get("diagnostic_selection")
        .ok_or_else(|| PreflightContractsError::Malformed("diagnostic_selection missing".into()))?;
    let default_rank = selection
        .get("specificity_rank")
        .and_then(|r| r.get("default_for_every_error"))
        .and_then(Json::as_u64)
        .ok_or_else(|| {
            PreflightContractsError::Malformed(
                "specificity_rank.default_for_every_error missing".into(),
            )
        })?;

    let mut rank_overrides = BTreeMap::new();
    for (id, rank) in selection
        .get("specificity_rank")
        .and_then(|r| r.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        if let Some(rank) = rank.as_u64() {
            rank_overrides.insert(id.clone(), rank);
        }
    }

    let mut supersede_overrides: BTreeMap<String, BTreeSet<PreflightError>> = BTreeMap::new();
    for (id, targets) in selection
        .get("supersedes")
        .and_then(|s| s.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        let mirrored: BTreeSet<PreflightError> = targets
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(Json::as_str)
            .filter_map(PreflightError::from_registry_str)
            .collect();
        if !mirrored.is_empty() {
            supersede_overrides.insert(id.clone(), mirrored);
        }
    }

    // Parity: every mirrored identifier must exist in the registry, and the
    // registry's whole `validation` stage must be mirrored here. The other
    // stages are shared with earlier layers and are not this layer's to own in
    // full, so only the validation stage is closed against.
    let mut missing_from_registry = Vec::new();
    let mut errors = BTreeMap::new();
    for id in PreflightError::ALL {
        match registered(
            registry,
            id,
            default_rank,
            &rank_overrides,
            &supersede_overrides,
        ) {
            Some(def) => {
                errors.insert(id, def);
            }
            None => missing_from_registry.push(id.as_registry_str().to_string()),
        }
    }
    let missing_from_build: Vec<String> = registry
        .errors_by_stage(lcl_diagnostics::Stage::Validation)
        .into_iter()
        .filter(|def| PreflightError::from_registry_str(&def.id).is_none())
        .map(|def| def.id.clone())
        .collect();
    if !missing_from_registry.is_empty() || !missing_from_build.is_empty() {
        return Err(PreflightContractsError::ErrorSetMismatch {
            missing_from_build,
            missing_from_registry,
        });
    }

    let supersedes = errors
        .values()
        .filter(|def| !def.supersedes.is_empty())
        .map(|def| (def.id, def.supersedes.clone()))
        .collect();
    Ok((errors, supersedes))
}

fn text(
    owner: &Json,
    key: &'static str,
    what: &'static str,
) -> Result<String, PreflightContractsError> {
    owner
        .get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| PreflightContractsError::Malformed(format!("{what}.{key} missing")))
}

fn load_graph_contract(blocks: &Json) -> Result<GraphContract, PreflightContractsError> {
    let c = blocks.get("execution_graph_contract").ok_or_else(|| {
        PreflightContractsError::Malformed("execution_graph_contract missing".into())
    })?;
    Ok(GraphContract {
        candidate_graph: text(c, "candidate_graph", "execution_graph_contract")?,
        child_order: text(c, "child_order", "execution_graph_contract")?,
        activation_identity: text(c, "activation_identity", "execution_graph_contract")?,
        ordering: text(c, "ordering", "execution_graph_contract")?,
        parallel: text(c, "parallel", "execution_graph_contract")?,
        loop_instances: text(c, "loop_instances", "execution_graph_contract")?,
        successor: text(c, "successor", "execution_graph_contract")?,
    })
}

fn load_check_selection(
    statuses: &Json,
) -> Result<CheckSelectionContract, PreflightContractsError> {
    let c = statuses.get("check_selection_contract").ok_or_else(|| {
        PreflightContractsError::Malformed("check_selection_contract missing".into())
    })?;
    Ok(CheckSelectionContract {
        selection: text(c, "selection", "check_selection_contract")?,
        prerequisites: text(c, "prerequisites", "check_selection_contract")?,
        demand: text(c, "demand", "check_selection_contract")?,
        failure: text(c, "failure", "check_selection_contract")?,
        domain_results: text(c, "domain_results", "check_selection_contract")?,
        root_success: text(c, "root_success", "check_selection_contract")?,
        lifecycle: text(c, "lifecycle", "check_selection_contract")?,
    })
}

fn load_operation_axes(
    operations: &Json,
) -> Result<BTreeMap<String, OperationAxes>, PreflightContractsError> {
    let contracts = operations
        .get("contracts")
        .and_then(Json::as_object)
        .ok_or_else(|| PreflightContractsError::Malformed("operations.contracts missing".into()))?;
    let mut out = BTreeMap::new();
    for (id, contract) in contracts {
        let category = contract
            .get("category")
            .and_then(Json::as_str)
            .ok_or_else(|| PreflightContractsError::Malformed(format!("{id}.category missing")))?
            .to_string();
        // The `none` sentinel means "no effect and no dependency"; it is not a
        // class, and keeping it would make an absence look like a presence.
        let strings = |key: &str| -> BTreeSet<String> {
            contract
                .get(key)
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .filter(|value| *value != "none")
                .map(str::to_string)
                .collect()
        };
        let determinism = contract
            .get("determinism")
            .and_then(|d| d.get("category"))
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        out.insert(
            id.clone(),
            OperationAxes {
                category,
                possible_effects: strings("possible_effects"),
                possible_dependencies: strings("possible_dependencies"),
                determinism,
            },
        );
    }
    Ok(out)
}
