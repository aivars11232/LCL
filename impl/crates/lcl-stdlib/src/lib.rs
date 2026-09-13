//! # lcl-stdlib — the executable LCL Core operation surface
//!
//! Milestone M7, language side.
//!
//! `06_STANDARD_LIBRARY` and `10_REGISTRIES/operations_v0.1.0.json` close the
//! Core surface at thirty-nine operation rows and eleven pure functions. This
//! crate is what makes them run:
//!
//! * a row whose dependencies are exactly `declared_state_only` is **computed
//!   here**, from declared values, with no host involved at all;
//! * a row that genuinely needs the world has its parameters defaulted, its
//!   constraints checked, its implementation profile selected and its actual
//!   dependency and effect sets resolved **here**, and only then does the
//!   request cross the capability boundary.
//!
//! ## The two decisions this crate keeps apart
//!
//! > Host permission does not imply LCL authorization. LCL authorization does
//! > not force host permission. Both gates must pass for an effect.
//!
//! [`Stdlib`] is the language half: it selects registered errors, applies
//! registry defaults, resolves axes and decides what an operation *means*. The
//! host half decides only what the machine will do and reports only what
//! happened. They are separate types with separate inputs, wired into the
//! engine through separate parameters, so neither can quietly answer for the
//! other.
//!
//! ## What a custom operation does here
//!
//! Nothing, by default. A `DEFINE kind.operation` is a contract without a body
//! — canonical example `07_DOMAIN_EXTENSION_OPERATION.lcl` declares parameters,
//! axes and a result type and no implementation — so its execution is the
//! host's by construction. This crate recognises that it is not a core row and
//! defers, rather than inventing a meaning the language never gave it.
//!
//! The one exception is a *pure* custom operation named as a `core.sort` key or
//! a `core.select` predicate, which cannot cross the boundary without breaking
//! its caller's `declared_state_only` contract. An embedder installs those with
//! [`Stdlib::with_pure_operation`], and an absent one is the precondition
//! failure the registry names.

pub mod contracts;
pub mod control;
pub mod data;
pub mod dispatch;
pub mod fixtures;
pub mod fragment;
pub mod host;
pub mod params;
pub mod profiles;
pub mod pure;
pub mod schema;

pub use contracts::{
    Bound, Contracts, ContractsError, OperationContract, ParameterSpec, TargetSpec,
};
pub use dispatch::{family, Family, DISPATCHED};
pub use fixtures::MemoryFileSystem;
pub use host::HostAdapter;
pub use profiles::{
    checking_profiles, filesystem_profiles, process_profiles, store_profiles, transport_profiles,
};

use lcl_capabilities::ProfileCatalog;
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_runtime::capability::CapabilityRequest;
use lcl_runtime::operations::{Invocation, Operations, Resolution};
use lcl_runtime::Value;
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::fmt;

/// One installed implementation of a pure custom operation.
///
/// It receives the single declared argument and returns the declared result, or
/// a non-normative reason it could not. It is given no host, no bindings and no
/// document, because the contract it implements is
/// `SIDE_EFFECT FALSE, DETERMINISTIC TRUE, declared_state_only` — everything it
/// may read is in front of it.
pub type PureOperation = Box<dyn Fn(&Value) -> Result<Value, String> + Send + Sync>;

/// Why the standard library could not be assembled.
#[derive(Debug)]
pub enum StdlibError {
    Contracts(ContractsError),
    Lexicon(String),
    Grammar(String),
}

impl fmt::Display for StdlibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StdlibError::Contracts(inner) => write!(f, "{inner}"),
            StdlibError::Lexicon(detail) => write!(f, "the lexicon did not load: {detail}"),
            StdlibError::Grammar(detail) => write!(f, "the grammar did not load: {detail}"),
        }
    }
}

impl std::error::Error for StdlibError {}

/// The executable Core operation surface.
pub struct Stdlib {
    contracts: Contracts,
    catalog: ProfileCatalog,
    lexicon: Lexicon,
    grammar: Grammar,
    pure_operations: BTreeMap<String, PureOperation>,
    grants: lcl_capabilities::Grants,
}

impl Stdlib {
    /// Assemble the surface from the approved package.
    ///
    /// The lexicon and grammar are here because an expression fragment is
    /// "exactly one EXPRESSION from `04_GRAMMAR/10_COMPLETE_EBNF.ebnf`", and
    /// the only way to honour that is to use the same lexer and parser every
    /// other stage uses.
    ///
    /// The catalog starts with the registry's rows and no installed profiles.
    /// That is a valid, useful state: every row that requires a profile role
    /// then fails its precondition before effects, which is exactly what an
    /// engine with no installed implementations should do.
    pub fn load(spec: &SpecPackage) -> Result<Stdlib, StdlibError> {
        Ok(Stdlib {
            contracts: Contracts::load(spec).map_err(StdlibError::Contracts)?,
            catalog: ProfileCatalog::load(spec)
                .map_err(|e| StdlibError::Contracts(ContractsError::Malformed(e.to_string())))?,
            lexicon: Lexicon::load(spec).map_err(|e| StdlibError::Lexicon(e.to_string()))?,
            grammar: Grammar::load(spec).map_err(|e| StdlibError::Grammar(e.to_string()))?,
            pure_operations: BTreeMap::new(),
            // The engine's own MEMORY and STATE stores, and nothing external.
            // A document that touches no outside resource must still run.
            grants: lcl_capabilities::Grants::internal(),
        })
    }

    /// Install the profile catalog, with whichever implementations exist.
    pub fn with_catalog(mut self, catalog: ProfileCatalog) -> Stdlib {
        self.catalog = catalog;
        self
    }

    /// Install the profiles the shipped adapters declare about themselves.
    pub fn with_profiles(
        mut self,
        profiles: impl IntoIterator<Item = lcl_capabilities::Profile>,
    ) -> Stdlib {
        self.catalog = std::mem::take(&mut self.catalog).with_all(profiles);
        self
    }

    /// Install the host grants this surface consults for the engine's own
    /// MEMORY and STATE stores.
    ///
    /// Every *external* effect is gated by the host adapter's own grants. This
    /// copy exists because the two store rows change state the engine holds
    /// rather than state a host holds, and the host gate must still be real:
    /// authorization from the plan and permission from here, neither implying
    /// the other.
    pub fn with_grants(mut self, grants: lcl_capabilities::Grants) -> Stdlib {
        self.grants = grants;
        self
    }

    pub fn grants(&self) -> &lcl_capabilities::Grants {
        &self.grants
    }

    /// Install one pure custom operation's implementation.
    ///
    /// Its declared axes are checked against the row that names it before it is
    /// ever applied, so installing one cannot widen what the caller may do.
    pub fn with_pure_operation(
        mut self,
        id: impl Into<String>,
        implementation: PureOperation,
    ) -> Stdlib {
        self.pure_operations.insert(id.into(), implementation);
        self
    }

    pub fn catalog(&self) -> &ProfileCatalog {
        &self.catalog
    }

    pub fn contracts(&self) -> &Contracts {
        &self.contracts
    }

    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    pub fn grammar(&self) -> &Grammar {
        &self.grammar
    }

    pub(crate) fn pure_operation(&self, id: &str) -> Option<&PureOperation> {
        self.pure_operations.get(id)
    }
}

impl Operations for Stdlib {
    fn invoke(&mut self, cx: &mut Invocation<'_>, request: &CapabilityRequest) -> Resolution {
        // Not a core row. A custom kind.operation has no body of its own, so
        // its implementation is the host's.
        let Some(owner) = family(&request.operation) else {
            return Resolution::host(request.clone());
        };
        let Some(contract) = self.contracts.operation(&request.operation).cloned() else {
            return Resolution::host(request.clone());
        };

        // "A non-null default is the exact declared value applied only when an
        // optional parameter is MISSING."
        let parameters = params::with_defaults(&contract, &request.parameters);
        // "A declared value outside a numeric bound … uses
        // error.value.out_of_range."
        if let Some(failure) = params::check_bounds(&contract, &parameters) {
            return failure;
        }
        if let Some(failure) = params::check_required(&contract, &parameters) {
            return failure;
        }

        match owner {
            Family::Pure => pure::invoke(self, cx, request, &contract, &parameters),
            // Every row whose axes resolve from its target, destination and
            // its own invocation rule takes the same path: classify, resolve,
            // select a profile, then hand over a request carrying the sets it
            // actually resolved.
            Family::Control => control::invoke(self, cx, request, &contract, &parameters),
            // Three analytical rows compare or check declared values, and reach
            // no host to do it. The rest resolve their axes and cross over.
            Family::Analytical if contract.operation == "core.compare" => {
                control::compare(cx, request, &contract, &parameters)
            }
            Family::Analytical if contract.operation == "core.validate" => {
                control::validate(cx, request, &contract, &parameters)
            }
            Family::Analytical if contract.operation == "core.verify" => {
                control::verify(self, cx, request, &contract, &parameters)
            }
            Family::Analytical | Family::Data | Family::Network | Family::Process => {
                data::invoke(self, cx, request, &contract, &parameters)
            }
            Family::Store => data::store(self, cx, request, &contract, &parameters),
        }
    }
}
