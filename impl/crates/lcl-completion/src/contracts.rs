//! The completion vocabulary, read from the verified canonical package.
//!
//! Nothing here is transcribed. Every identifier, status, sentence and rank is
//! read from the registries at load time, and a registry that disagrees with
//! this build refuses to load.
//!
//! Like every layer below it, this one does not re-read what an earlier layer
//! already loads. It holds an [`lcl_runtime::Contracts`] — which holds M5's,
//! which holds M4's — and asks it for operations, schemas, the diagnostic
//! registry and the check-selection contract.
//!
//! What it adds is what only *completion* needs: the registered metadata of the
//! seven identifiers this layer may emit, the twelve statuses with their exact
//! `allowed_next` sets and scopes, and the verbatim
//! `check_selection_contract` and `failure_lifecycle` sentences that govern
//! selection, failure mapping and terminal transitions.
//!
//! ## The parity check that matters
//!
//! [`Contracts::load`] refuses a package whose `verification_or_completion`
//! stage is not exactly [`CompletionError::OWNED`]. A registry that gained a
//! fourth completion error would otherwise go silently unimplemented while
//! every gate stayed green, which is precisely the false-success this layer
//! exists to prevent.

use crate::diagnostic::CompletionError;
use lcl_diagnostics::{DiagnosticRegistry, Stage, StatusDef};
use lcl_runtime::{Contracts as RuntimeContracts, RuntimeContractsError};
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Why the completion vocabulary refused to load.
#[derive(Debug)]
pub enum CompletionContractsError {
    /// The package did not establish authority.
    UnverifiedPackage(lcl_spec::Authority),
    /// The M6 vocabulary this layer builds on refused to load.
    Runtime(RuntimeContractsError),
    MissingRegistry(&'static str),
    Malformed(String),
    /// A mirrored identifier is absent from the registry.
    UnknownIdentifier(String),
    /// The registry's `verification_or_completion` stage is not the set this
    /// build owns.
    OwnedSetMismatch {
        missing_from_build: Vec<String>,
        missing_from_registry: Vec<String>,
    },
}

impl fmt::Display for CompletionContractsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompletionContractsError::UnverifiedPackage(authority) => write!(
                f,
                "the canonical package is {authority:?}; completion contracts load only from the approved release"
            ),
            CompletionContractsError::Runtime(inner) => {
                write!(f, "runtime contracts did not load: {inner}")
            }
            CompletionContractsError::MissingRegistry(name) => {
                write!(f, "registry {name} is missing from the package")
            }
            CompletionContractsError::Malformed(detail) => {
                write!(f, "malformed registry: {detail}")
            }
            CompletionContractsError::UnknownIdentifier(id) => write!(
                f,
                "this build mirrors {id}, which the registry does not define"
            ),
            CompletionContractsError::OwnedSetMismatch {
                missing_from_build,
                missing_from_registry,
            } => write!(
                f,
                "the verification_or_completion stage disagrees with this build: missing from build {missing_from_build:?}, missing from registry {missing_from_registry:?}"
            ),
        }
    }
}

impl std::error::Error for CompletionContractsError {}

/// One mirrored identifier's registered metadata, copied verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredError {
    pub id: CompletionError,
    /// `errors.<id>.stage`, verbatim.
    pub stage: Stage,
    pub meaning: String,
    pub default_status: String,
    pub specificity_rank: u64,
    /// `errors.<id>.event`, verbatim.
    pub event: Option<String>,
    pub recoverable: bool,
}

/// The verbatim sentences of `#/check_selection_contract`.
///
/// Kept as text, not paraphrased, so a report can quote the exact authority a
/// behavior implements.
#[derive(Debug, Clone)]
pub struct CheckSelection {
    pub selection: String,
    pub prerequisites: String,
    pub demand: String,
    pub failure: String,
    pub domain_results: String,
    pub root_success: String,
    pub lifecycle: String,
}

/// The verbatim rules of `#/failure_lifecycle` that completion applies.
#[derive(Debug, Clone)]
pub struct FailureLifecycle {
    pub status_rule: String,
    pub failure_mapping_rule: String,
    pub terminal_invocation_rule: String,
    pub output_binding_rule: String,
    pub indeterminate_state_rule: String,
}

/// The completion vocabulary.
#[derive(Debug)]
pub struct Contracts {
    runtime: RuntimeContracts,
    errors: BTreeMap<CompletionError, RegisteredError>,
    statuses: BTreeMap<String, StatusDef>,
    terminal: BTreeSet<String>,
    non_success: BTreeSet<String>,
    check_selection: CheckSelection,
    failure_lifecycle: FailureLifecycle,
}

impl Contracts {
    pub fn load(spec: &SpecPackage) -> Result<Contracts, CompletionContractsError> {
        if !matches!(spec.authority(), lcl_spec::Authority::Authoritative) {
            return Err(CompletionContractsError::UnverifiedPackage(
                spec.authority(),
            ));
        }
        let runtime = RuntimeContracts::load(spec).map_err(CompletionContractsError::Runtime)?;

        let registry = spec.registry("statuses_and_errors").ok_or(
            CompletionContractsError::MissingRegistry("statuses_and_errors"),
        )?;
        let selection = registry.get("diagnostic_selection").ok_or_else(|| {
            CompletionContractsError::Malformed("diagnostic_selection missing".into())
        })?;

        let diagnostics = runtime.diagnostics();
        check_owned_stage_parity(diagnostics)?;

        let errors = load_errors(diagnostics, selection)?;
        let statuses: BTreeMap<String, StatusDef> = diagnostics
            .statuses()
            .map(|s| (s.id.clone(), s.clone()))
            .collect();
        if statuses.is_empty() {
            return Err(CompletionContractsError::Malformed(
                "the registry defines no statuses".into(),
            ));
        }
        let terminal = statuses
            .values()
            .filter(|s| s.terminal)
            .map(|s| s.id.clone())
            .collect();

        // "status.partial is explicitly non-success" and "At a TASK execution
        // root, status.succeeded is legal only when ...": succeeded is the one
        // success status, so every other terminal status is non-success. The
        // set is derived from the registry rather than listed here.
        let non_success = statuses
            .values()
            .filter(|s| s.terminal && s.id != "status.succeeded")
            .map(|s| s.id.clone())
            .collect();

        let check_selection = load_check_selection(diagnostics.check_selection_contract())?;
        let failure_lifecycle = load_failure_lifecycle(diagnostics.failure_lifecycle())?;

        Ok(Contracts {
            runtime,
            errors,
            statuses,
            terminal,
            non_success,
            check_selection,
            failure_lifecycle,
        })
    }

    /// The M6 vocabulary, and through it M5's and M4's.
    pub fn runtime(&self) -> &RuntimeContracts {
        &self.runtime
    }

    pub fn diagnostics(&self) -> &DiagnosticRegistry {
        self.runtime.diagnostics()
    }

    /// The registered metadata of one mirrored identifier.
    ///
    /// Total over [`CompletionError::ALL`]: [`Contracts::load`] refuses a
    /// package missing any of them, so this cannot fail after loading.
    pub fn error(&self, id: CompletionError) -> &RegisteredError {
        self.errors
            .get(&id)
            .expect("load refuses a package missing a mirrored identifier")
    }

    /// One registered status, by canonical identifier.
    pub fn status(&self, id: &str) -> Option<&StatusDef> {
        self.statuses.get(id)
    }

    /// Every registered status, in identifier order.
    pub fn statuses(&self) -> impl Iterator<Item = &StatusDef> {
        self.statuses.values()
    }

    /// True when the identifier is a registered terminal status.
    pub fn is_terminal(&self, id: &str) -> bool {
        self.terminal.contains(id)
    }

    /// True when the identifier is a registered terminal non-success status.
    ///
    /// `FAILURE` may map only to one of these: "FAILURE contains WHEN and one
    /// terminal non-success STATUS."
    pub fn is_terminal_non_success(&self, id: &str) -> bool {
        self.non_success.contains(id)
    }

    /// True when `to` is in `from`'s registered `allowed_next` set.
    ///
    /// `failure_mapping_rule`: "The selected canonical STATUS must be terminal,
    /// non-success, and present in the current invocation state's allowed_next."
    pub fn permits(&self, from: &str, to: &str) -> bool {
        self.statuses
            .get(from)
            .is_some_and(|s| s.allowed_next.iter().any(|next| next == to))
    }

    /// The registered scope of one status.
    ///
    /// `status.skipped` is the only status scoped `declaration_or_result`
    /// rather than `execution_root_declaration_or_result`, which is what makes
    /// a root request for it illegal.
    pub fn scope(&self, id: &str) -> Option<&str> {
        self.statuses.get(id).map(|s| s.scope.as_str())
    }

    /// True when a status may be the terminal status of an execution *root*.
    pub fn permitted_at_root(&self, id: &str) -> bool {
        self.scope(id) == Some("execution_root_declaration_or_result")
    }

    pub fn check_selection(&self) -> &CheckSelection {
        &self.check_selection
    }

    pub fn failure_lifecycle(&self) -> &FailureLifecycle {
        &self.failure_lifecycle
    }
}

/// Refuse a package whose `verification_or_completion` stage is not exactly the
/// set this build owns.
fn check_owned_stage_parity(registry: &DiagnosticRegistry) -> Result<(), CompletionContractsError> {
    let registered: BTreeSet<String> = registry
        .errors_by_stage(Stage::VerificationOrCompletion)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    let owned: BTreeSet<String> = CompletionError::OWNED
        .iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    if registered == owned {
        return Ok(());
    }
    Err(CompletionContractsError::OwnedSetMismatch {
        missing_from_build: registered.difference(&owned).cloned().collect(),
        missing_from_registry: owned.difference(&registered).cloned().collect(),
    })
}

fn load_errors(
    registry: &DiagnosticRegistry,
    selection: &Json,
) -> Result<BTreeMap<CompletionError, RegisteredError>, CompletionContractsError> {
    let default_rank = selection
        .get("specificity_rank")
        .and_then(|r| r.get("default_for_every_error"))
        .and_then(Json::as_u64)
        .ok_or_else(|| {
            CompletionContractsError::Malformed(
                "specificity_rank.default_for_every_error missing".into(),
            )
        })?;
    let mut overrides = BTreeMap::new();
    for (id, rank) in selection
        .get("specificity_rank")
        .and_then(|r| r.get("overrides"))
        .and_then(Json::as_object)
        .unwrap_or(&[])
    {
        if let Some(rank) = rank.as_u64() {
            overrides.insert(id.clone(), rank);
        }
    }

    let mut errors = BTreeMap::new();
    for id in CompletionError::ALL {
        let def = registry.error(id.as_registry_str()).ok_or_else(|| {
            CompletionContractsError::UnknownIdentifier(id.as_registry_str().to_string())
        })?;
        errors.insert(
            id,
            RegisteredError {
                id,
                stage: def.stage,
                meaning: def.meaning.clone(),
                default_status: def.default_status.clone(),
                specificity_rank: overrides
                    .get(id.as_registry_str())
                    .copied()
                    .unwrap_or(default_rank),
                event: def.event.clone(),
                recoverable: def.recoverable_with_declared_handler,
            },
        );
    }
    Ok(errors)
}

fn sentence(body: &Json, key: &'static str) -> Result<String, CompletionContractsError> {
    body.get(key)
        .and_then(Json::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            CompletionContractsError::Malformed(format!("{key} missing or not a string"))
        })
}

fn load_check_selection(body: &Json) -> Result<CheckSelection, CompletionContractsError> {
    Ok(CheckSelection {
        selection: sentence(body, "selection")?,
        prerequisites: sentence(body, "prerequisites")?,
        demand: sentence(body, "demand")?,
        failure: sentence(body, "failure")?,
        domain_results: sentence(body, "domain_results")?,
        root_success: sentence(body, "root_success")?,
        lifecycle: sentence(body, "lifecycle")?,
    })
}

fn load_failure_lifecycle(body: &Json) -> Result<FailureLifecycle, CompletionContractsError> {
    Ok(FailureLifecycle {
        status_rule: sentence(body, "status_rule")?,
        failure_mapping_rule: sentence(body, "failure_mapping_rule")?,
        terminal_invocation_rule: sentence(body, "terminal_invocation_rule")?,
        output_binding_rule: sentence(body, "output_binding_rule")?,
        indeterminate_state_rule: sentence(body, "indeterminate_state_rule")?,
    })
}
