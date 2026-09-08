//! The runtime vocabulary, read from the verified canonical package.
//!
//! Nothing here is transcribed. Every identifier, bound, default, schema and
//! contract sentence is read from the registries at load time, and a registry
//! that disagrees with this build's mirrored error set refuses to load.
//!
//! Like M5, this layer does not re-read what an earlier layer already loads. It
//! holds an [`lcl_semantics::Contracts`] — which in turn holds M4's — and asks
//! it for operators, functions, constructors, the 39 operation contracts,
//! units, formats, the three-valued logic table, the operation axes, the
//! execution-graph contract and the check-selection contract.
//!
//! What it adds is what only a *runtime* needs: the exact
//! `expression_demand_resolution` map, the nine result schemas with their
//! default projections and partial-binding permissions, the retry bounds, and
//! the registered metadata of the identifiers this layer emits.

use crate::diagnostic::RuntimeError;
use crate::order_profile::DurationProfile;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_semantics::{Contracts as PreflightContracts, PreflightContractsError};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Why the runtime vocabulary refused to load.
#[derive(Debug)]
pub enum RuntimeContractsError {
    /// The package did not establish authority.
    UnverifiedPackage(lcl_spec::Authority),
    /// The M5 vocabulary this layer builds on refused to load.
    Preflight(PreflightContractsError),
    MissingRegistry(&'static str),
    Malformed(String),
    /// A mirrored identifier is not in the registry, or the registry's
    /// `execution` stage is not the set this build mirrors.
    ErrorSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
    /// The registry's `expression_demand_resolution` eligible set is not the
    /// set this build can raise at demand.
    DemandSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
}

impl fmt::Display for RuntimeContractsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeContractsError::UnverifiedPackage(authority) => write!(
                f,
                "the canonical package is {authority:?}; runtime contracts load only from the approved release"
            ),
            RuntimeContractsError::Preflight(inner) => {
                write!(f, "preflight contracts did not load: {inner}")
            }
            RuntimeContractsError::MissingRegistry(name) => {
                write!(f, "registry {name} is missing from the package")
            }
            RuntimeContractsError::Malformed(detail) => write!(f, "malformed registry: {detail}"),
            RuntimeContractsError::ErrorSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "runtime error set disagrees with the registry: missing from build {missing_from_build:?}, missing from registry {missing_from_registry:?}"
            ),
            RuntimeContractsError::DemandSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "expression-demand set disagrees with the registry: missing from build {missing_from_build:?}, missing from registry {missing_from_registry:?}"
            ),
        }
    }
}

impl std::error::Error for RuntimeContractsError {}

/// One mirrored identifier's registered metadata, copied verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredError {
    pub id: RuntimeError,
    /// `errors.<id>.stage`, verbatim. Never assumed by this crate.
    pub stage: Stage,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    /// `errors.<id>.event`, verbatim.
    pub event: Option<String>,
    pub recoverable: bool,
}

/// The exact `expression_demand_resolution` map.
#[derive(Debug, Clone)]
pub struct DemandResolution {
    /// Every eligible identifier and its exact trigger sentence.
    eligible: BTreeMap<RuntimeError, String>,
    /// `resolved_stage`, e.g. `execution`.
    pub resolved_stage: Stage,
    /// `default_status`, e.g. `status.failed`.
    pub default_status: String,
    /// `default_status_overrides`, e.g. MISSING and UNKNOWN keeping
    /// `status.blocked`.
    overrides: BTreeMap<RuntimeError, String>,
    /// `context`, kept verbatim so a report can quote its authority.
    pub context: String,
    /// `exclusion_rule`, verbatim.
    pub exclusion_rule: String,
    /// `phase_rule`, verbatim.
    pub phase_rule: String,
}

impl DemandResolution {
    /// True when this identifier may be demand-resolved at all.
    pub fn is_eligible(&self, id: RuntimeError) -> bool {
        self.eligible.contains_key(&id)
    }

    /// The registry's exact trigger sentence for one eligible identifier.
    pub fn trigger(&self, id: RuntimeError) -> Option<&str> {
        self.eligible.get(&id).map(String::as_str)
    }

    /// Every eligible identifier, in registry order.
    pub fn eligible(&self) -> impl Iterator<Item = RuntimeError> + '_ {
        self.eligible.keys().copied()
    }

    /// The status an eligible identifier resolves to at a post-preflight
    /// demand: the map's `default_status`, or its exact override.
    pub fn status_for(&self, id: RuntimeError) -> &str {
        self.overrides
            .get(&id)
            .map(String::as_str)
            .unwrap_or(&self.default_status)
    }
}

/// One of the nine closed result schemas, as the registry declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSchema {
    pub id: String,
    /// `primary_output.default_property`: the field a zero-`PROPERTY` `OUTPUT`
    /// selects.
    pub default_property: Option<String>,
    /// `primary_output.projectable_fields`: "Every selected name must occur in
    /// the schema's projectable_fields list; common bookkeeping fields cannot
    /// be projected."
    pub projectable_fields: Vec<String>,
    /// `partial_output.supported`.
    pub partial_supported: bool,
    /// `partial_output.fields`: the fields partial binding may cover.
    pub partial_fields: Vec<String>,
    /// Schema-local field names and their registered cardinality string.
    pub fields: BTreeMap<String, String>,
    /// The registry's constraint sentences, verbatim.
    pub constraints: Vec<String>,
}

impl ResultSchema {
    /// True when this schema permits a partial `OUTPUT` over exactly `fields`.
    ///
    /// `05_SEMANTICS/05`: partial is permitted "only when the result schema
    /// both permits partial output and lists every selected field as
    /// partial-bindable".
    pub fn permits_partial(&self, fields: &[String]) -> bool {
        self.partial_supported
            && !fields.is_empty()
            && fields.iter().all(|f| self.partial_fields.contains(f))
    }
}

/// The registered contract of a `RETRY` block.
///
/// Read from `field_signatures_v0.1.0.json#/blocks/RETRY`, which declares all
/// three machine-readably: `LIMIT.value_kind` is `integer[0..100]`,
/// `WHEN.default` is `true` and `DELAY.default` is `DURATION(0, unit.second)`.
/// The prose in `05_SEMANTICS/08` states the same three; reading the registry
/// means a change to either is a load failure rather than a silent drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryBounds {
    /// "LIMIT counts additional attempts and is 0 through 100."
    pub minimum_limit: i64,
    pub maximum_limit: i64,
    /// `RETRY.WHEN` default. "Omitted WHEN is TRUE."
    pub when_default: bool,
    /// `RETRY.DELAY` default, verbatim: "omitted DELAY is
    /// DURATION(0, unit.second)".
    pub delay_default: String,
}

/// The runtime vocabulary.
pub struct Contracts {
    preflight: PreflightContracts,
    errors: BTreeMap<RuntimeError, RegisteredError>,
    supersedes: BTreeMap<RuntimeError, Vec<RuntimeError>>,
    demand: DemandResolution,
    schemas: BTreeMap<String, ResultSchema>,
    retry: RetryBounds,
    /// The registry's `duration_normalization` factors.
    duration: DurationProfile,
    /// The closed canonical event vocabulary: every registered non-null event.
    events: BTreeSet<String>,
}

impl fmt::Debug for Contracts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Contracts")
            .field("errors", &self.errors.len())
            .field("schemas", &self.schemas.len())
            .field("events", &self.events.len())
            .finish()
    }
}

impl Contracts {
    /// Build from a verified specification package.
    ///
    /// Refuses an unverified package for the same reason every layer below
    /// does: a runtime that executed against unverified canonical bytes would
    /// have no authority for anything it did.
    pub fn load(spec: &SpecPackage) -> Result<Contracts, RuntimeContractsError> {
        if !matches!(spec.authority(), lcl_spec::Authority::Authoritative) {
            return Err(RuntimeContractsError::UnverifiedPackage(spec.authority()));
        }
        let preflight = PreflightContracts::load(spec).map_err(RuntimeContractsError::Preflight)?;

        let statuses =
            spec.registry("statuses_and_errors")
                .ok_or(RuntimeContractsError::MissingRegistry(
                    "statuses_and_errors",
                ))?;
        let results = spec.registry("built_in_groups_and_results").ok_or(
            RuntimeContractsError::MissingRegistry("built_in_groups_and_results"),
        )?;
        let signatures = spec
            .registry("field_signatures")
            .ok_or(RuntimeContractsError::MissingRegistry("field_signatures"))?;
        let units = spec.registry("formats_encodings_units").ok_or(
            RuntimeContractsError::MissingRegistry("formats_encodings_units"),
        )?;

        let selection = statuses.get("diagnostic_selection").ok_or_else(|| {
            RuntimeContractsError::Malformed("diagnostic_selection missing".into())
        })?;

        let (errors, supersedes) = load_errors(preflight.diagnostics(), selection)?;
        let demand = load_demand(selection)?;
        let schemas = load_schemas(results)?;
        let retry = load_retry_bounds(signatures)?;
        let duration = DurationProfile::load(units).ok_or_else(|| {
            RuntimeContractsError::Malformed("duration_normalization is missing or empty".into())
        })?;
        let events = preflight
            .diagnostics()
            .errors()
            .filter_map(|e| e.event.clone())
            .collect();

        Ok(Contracts {
            preflight,
            errors,
            supersedes,
            demand,
            schemas,
            retry,
            duration,
            events,
        })
    }

    /// The M5 vocabulary, and through it M4's.
    pub fn preflight(&self) -> &PreflightContracts {
        &self.preflight
    }

    /// The M4 static vocabulary: operators, functions, constructors, units.
    pub fn statics(&self) -> &lcl_checker::Contracts {
        self.preflight.statics()
    }

    pub fn diagnostics(&self) -> &DiagnosticRegistry {
        self.preflight.diagnostics()
    }

    /// The registered metadata of one mirrored identifier.
    ///
    /// Total over [`RuntimeError::ALL`]: [`Contracts::load`] refuses a package
    /// missing any of them, so this cannot fail after loading.
    pub fn error(&self, id: RuntimeError) -> &RegisteredError {
        self.errors
            .get(&id)
            .expect("load refuses a package missing a mirrored identifier")
    }

    /// The supersession edges among mirrored identifiers.
    pub fn supersedes(&self) -> &BTreeMap<RuntimeError, Vec<RuntimeError>> {
        &self.supersedes
    }

    pub fn demand(&self) -> &DemandResolution {
        &self.demand
    }

    pub fn schema(&self, id: &str) -> Option<&ResultSchema> {
        self.schemas.get(id)
    }

    pub fn schemas(&self) -> impl Iterator<Item = &ResultSchema> {
        self.schemas.values()
    }

    pub fn retry_bounds(&self) -> &RetryBounds {
        &self.retry
    }

    /// The registry's `DURATION` normalization factors.
    pub fn duration(&self) -> &DurationProfile {
        &self.duration
    }

    /// The closed canonical event vocabulary.
    pub fn events(&self) -> &BTreeSet<String> {
        &self.events
    }

    /// The registered result schema of one core operation.
    pub fn operation_schema(&self, operation: &str) -> Option<&ResultSchema> {
        let contract = self.statics().operation(operation)?;
        self.schema(&contract.result_schema)
    }
}

/// The mirrored identifiers and their supersession edges, as loaded.
type LoadedErrors = (
    BTreeMap<RuntimeError, RegisteredError>,
    BTreeMap<RuntimeError, Vec<RuntimeError>>,
);

fn load_errors(
    registry: &DiagnosticRegistry,
    selection: &Json,
) -> Result<LoadedErrors, RuntimeContractsError> {
    let default_rank = selection
        .get("specificity_rank")
        .and_then(|r| r.get("default_for_every_error"))
        .and_then(Json::as_u64)
        .ok_or_else(|| {
            RuntimeContractsError::Malformed(
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

    let mut supersedes: BTreeMap<RuntimeError, Vec<RuntimeError>> = BTreeMap::new();
    for (id, targets) in selection
        .get("supersedes")
        .and_then(|s| s.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        let Some(winner) = RuntimeError::from_registry_str(id) else {
            continue;
        };
        let mirrored: Vec<RuntimeError> = targets
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(Json::as_str)
            .filter_map(RuntimeError::from_registry_str)
            .collect();
        if !mirrored.is_empty() {
            supersedes.insert(winner, mirrored);
        }
    }

    let mut missing_from_registry = Vec::new();
    let mut errors = BTreeMap::new();
    for id in RuntimeError::ALL {
        match registry.error(id.as_registry_str()) {
            Some(def) => {
                errors.insert(
                    id,
                    RegisteredError {
                        id,
                        stage: def.stage,
                        meaning: def.meaning.clone(),
                        default_status: def.default_status.clone(),
                        specificity_rank: rank_overrides
                            .get(id.as_registry_str())
                            .copied()
                            .unwrap_or(default_rank),
                        event: def.event.clone(),
                        recoverable: def.recoverable_with_declared_handler,
                    },
                );
            }
            None => missing_from_registry.push(id.as_registry_str().to_string()),
        }
    }

    // Parity: the registry's whole `execution` stage must be mirrored here.
    // That is the stage this milestone owns, so a registry identifier this
    // build cannot name is a load failure, not a silent gap.
    let missing_from_build: Vec<String> = registry
        .errors_by_stage(Stage::Execution)
        .into_iter()
        .filter(|def| RuntimeError::from_registry_str(&def.id).is_none())
        .map(|def| def.id.clone())
        .collect();

    if !missing_from_build.is_empty() || !missing_from_registry.is_empty() {
        return Err(RuntimeContractsError::ErrorSetMismatch {
            missing_from_build,
            missing_from_registry,
        });
    }
    Ok((errors, supersedes))
}

fn load_demand(selection: &Json) -> Result<DemandResolution, RuntimeContractsError> {
    let map = selection
        .get("expression_demand_resolution")
        .ok_or_else(|| {
            RuntimeContractsError::Malformed("expression_demand_resolution missing".into())
        })?;

    let mut eligible = BTreeMap::new();
    let mut missing_from_build = Vec::new();
    for (id, trigger) in map
        .get("eligible_errors")
        .and_then(Json::as_object)
        .ok_or_else(|| RuntimeContractsError::Malformed("eligible_errors missing".into()))?
    {
        match RuntimeError::from_registry_str(id) {
            Some(mirrored) => {
                eligible.insert(mirrored, trigger.as_str().unwrap_or_default().to_string());
            }
            None => missing_from_build.push(id.clone()),
        }
    }
    if !missing_from_build.is_empty() {
        return Err(RuntimeContractsError::DemandSetMismatch {
            missing_from_build,
            missing_from_registry: Vec::new(),
        });
    }

    let resolved = map
        .get("resolved_stage")
        .and_then(Json::as_str)
        .ok_or_else(|| RuntimeContractsError::Malformed("resolved_stage missing".into()))?;
    let resolved_stage = Stage::from_registry_str(resolved).ok_or_else(|| {
        RuntimeContractsError::Malformed(format!("resolved_stage {resolved:?} is unregistered"))
    })?;

    let default_status = map
        .get("default_status")
        .and_then(Json::as_str)
        .ok_or_else(|| RuntimeContractsError::Malformed("demand default_status missing".into()))?
        .to_string();

    let mut overrides = BTreeMap::new();
    for (id, status) in map
        .get("default_status_overrides")
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        if let (Some(mirrored), Some(status)) =
            (RuntimeError::from_registry_str(id), status.as_str())
        {
            overrides.insert(mirrored, status.to_string());
        }
    }

    let sentence = |name: &str| -> String {
        map.get(name)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    };

    Ok(DemandResolution {
        eligible,
        resolved_stage,
        default_status,
        overrides,
        context: sentence("context"),
        exclusion_rule: sentence("exclusion_rule"),
        phase_rule: sentence("phase_rule"),
    })
}

fn load_schemas(results: &Json) -> Result<BTreeMap<String, ResultSchema>, RuntimeContractsError> {
    let declared = results
        .get("result_schemas")
        .and_then(Json::as_object)
        .ok_or_else(|| RuntimeContractsError::Malformed("result_schemas missing".into()))?;

    let mut schemas = BTreeMap::new();
    for (id, body) in declared {
        let primary = body.get("primary_output");
        let partial = body.get("partial_output");
        let strings = |value: Option<&Json>| -> Vec<String> {
            value
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .map(str::to_string)
                .collect()
        };
        let mut fields = BTreeMap::new();
        for (name, signature) in body.get("fields").and_then(Json::as_object).unwrap_or(&[]) {
            fields.insert(
                name.clone(),
                signature
                    .get("cardinality")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        schemas.insert(
            id.clone(),
            ResultSchema {
                id: id.clone(),
                default_property: primary
                    .and_then(|p| p.get("default_property"))
                    .and_then(Json::as_str)
                    .map(str::to_string),
                projectable_fields: strings(primary.and_then(|p| p.get("projectable_fields"))),
                partial_supported: partial
                    .and_then(|p| p.get("supported"))
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                partial_fields: strings(partial.and_then(|p| p.get("fields"))),
                fields,
                constraints: strings(body.get("constraints")),
            },
        );
    }
    if schemas.is_empty() {
        return Err(RuntimeContractsError::Malformed(
            "result_schemas is empty".into(),
        ));
    }
    Ok(schemas)
}

/// The `RETRY` contract, read from the field-signature registry.
///
/// `value_kind` is a structured designator, so the bound is parsed from
/// `integer[<min>..<max>]` rather than scraped out of a sentence. A registry
/// that stops declaring it in that exact shape fails to load.
fn load_retry_bounds(signatures: &Json) -> Result<RetryBounds, RuntimeContractsError> {
    let retry = signatures
        .get("blocks")
        .and_then(|b| b.get("RETRY"))
        .and_then(|r| r.get("fields"))
        .ok_or_else(|| {
            RuntimeContractsError::Malformed("field_signatures.blocks.RETRY.fields missing".into())
        })?;

    let kind = retry
        .get("LIMIT")
        .and_then(|l| l.get("value_kind"))
        .and_then(Json::as_str)
        .ok_or_else(|| RuntimeContractsError::Malformed("RETRY.LIMIT.value_kind missing".into()))?;
    let (minimum_limit, maximum_limit) = parse_integer_range(kind).ok_or_else(|| {
        RuntimeContractsError::Malformed(format!(
            "RETRY.LIMIT.value_kind {kind:?} is not integer[min..max]"
        ))
    })?;

    let when_default = retry
        .get("WHEN")
        .and_then(|w| w.get("default"))
        .and_then(Json::as_bool)
        .ok_or_else(|| {
            RuntimeContractsError::Malformed("RETRY.WHEN.default is not a boolean".into())
        })?;

    let delay_default = retry
        .get("DELAY")
        .and_then(|d| d.get("default"))
        .and_then(Json::as_str)
        .ok_or_else(|| {
            RuntimeContractsError::Malformed("RETRY.DELAY.default is not a string".into())
        })?
        .to_string();

    Ok(RetryBounds {
        minimum_limit,
        maximum_limit,
        when_default,
        delay_default,
    })
}

/// Parse the registry's `integer[<min>..<max>]` value-kind designator.
fn parse_integer_range(kind: &str) -> Option<(i64, i64)> {
    let inner = kind.strip_prefix("integer[")?.strip_suffix(']')?;
    let (low, high) = inner.split_once("..")?;
    Some((low.trim().parse().ok()?, high.trim().parse().ok()?))
}
