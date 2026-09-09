//! The operation contracts, read from the registry as data.
//!
//! Authority: `10_REGISTRIES/operations_v0.1.0.json#/contracts` and
//! `06_STANDARD_LIBRARY/10_CORE_OPERATION_PARAMETER_RULES.txt`.
//!
//! ## Nothing here is transcribed
//!
//! Every target type, parameter name, requiredness flag, default value and
//! constraint sentence is read from the package at load time. The contract this
//! implementation works under is explicit that it must be:
//!
//! > Closed registries must be consumed as data where practical. Do not
//! > transcribe large normative tables into Rust merely for convenience.
//!
//! So a registry whose `core.inspect` bound changed from `0..100` to something
//! else changes this implementation's behavior without a line of Rust changing,
//! which is the only way the two can be guaranteed to agree.
//!
//! ## What JSON null means here, and what it does not
//!
//! > In operations_v0.1.0.json only, JSON null in a parameter default field
//! > means no declared default and is not the LCL NULL value.
//!
//! [`ParameterSpec::default`] is therefore `Option<Value>`, and `None` is that
//! JSON null. No parameter in this release defaults to `Value::Null`, and if
//! one ever did, the registry would have to say so in a way this loader would
//! have to be taught — rather than it arriving silently through a conversion.

use lcl_capabilities::Axes;
use lcl_checker::numeric::{Decimal, Integer};
use lcl_runtime::Value;
use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// An inclusive numeric bound a parameter's constraint states, such as `0..100`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bound {
    pub minimum: i64,
    pub maximum: i64,
}

impl Bound {
    /// Parse the registry's `low..high` constraint spelling.
    ///
    /// Returns `None` for any other constraint sentence, which is prose for a
    /// rule that is not a numeric range and is enforced elsewhere.
    fn parse(constraint: &str) -> Option<Bound> {
        let (low, high) = constraint.trim().split_once("..")?;
        Some(Bound {
            minimum: low.trim().parse().ok()?,
            maximum: high.trim().parse().ok()?,
        })
    }

    pub fn admits(&self, value: i64) -> bool {
        value >= self.minimum && value <= self.maximum
    }
}

impl fmt::Display for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.minimum, self.maximum)
    }
}

/// One registered named parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterSpec {
    pub name: String,
    /// The contract type string, in the registry's closed notation.
    pub ty: String,
    pub required: bool,
    /// The declared default, or `None` for JSON null, which "means no declared
    /// default and is not the LCL NULL value".
    pub default: Option<Value>,
    /// The registry's constraint sentences, verbatim.
    pub constraints: Vec<String>,
    /// The inclusive numeric bound a constraint states, when one does.
    pub bound: Option<Bound>,
}

/// One registered target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub ty: String,
    pub required: bool,
}

impl TargetSpec {
    /// Whether the declared target type admits one named form.
    pub fn admits(&self, form: &str) -> bool {
        self.ty.split('|').any(|alternative| {
            let alternative = alternative.trim();
            alternative == form || alternative.starts_with(&format!("{form}["))
        })
    }
}

/// One registered operation row, as the executable surface needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationContract {
    pub operation: String,
    pub category: String,
    pub meaning: String,
    pub target: Option<TargetSpec>,
    pub parameters: BTreeMap<String, ParameterSpec>,
    pub result_schema: String,
    /// The row's applicable errors, verbatim.
    pub errors: BTreeSet<String>,
    /// The row's own `invocation_resolution` sentence.
    pub invocation_resolution: String,
    /// The registry maxima. An invocation resolves its actual sets within them.
    pub maximum: Axes,
}

impl OperationContract {
    /// The parameter of that name, when the row registers one.
    pub fn parameter(&self, name: &str) -> Option<&ParameterSpec> {
        self.parameters.get(name)
    }

    /// Whether this row lists one registered error as applicable.
    ///
    /// Used to keep an implementation from selecting an identifier its own row
    /// does not admit.
    pub fn admits_error(&self, id: &str) -> bool {
        self.errors.contains(id)
    }

    pub fn is_read_only(&self) -> bool {
        self.category == "read_only"
    }
}

/// Why the operation contracts could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractsError {
    UnverifiedPackage,
    MissingRegistry(&'static str),
    Malformed(String),
}

impl fmt::Display for ContractsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractsError::UnverifiedPackage => f.write_str(
                "the canonical package is not authoritative; operation contracts load only from the approved release",
            ),
            ContractsError::MissingRegistry(name) => {
                write!(f, "registry {name} is missing from the package")
            }
            ContractsError::Malformed(detail) => write!(f, "malformed registry: {detail}"),
        }
    }
}

impl std::error::Error for ContractsError {}

/// Every registered operation contract.
#[derive(Debug, Clone, Default)]
pub struct Contracts {
    operations: BTreeMap<String, OperationContract>,
    /// `#/parameter_binding`, which this release states as `named_only`.
    parameter_binding: String,
}

impl Contracts {
    /// Read the closed contract set from the approved package.
    pub fn load(spec: &SpecPackage) -> Result<Contracts, ContractsError> {
        if !matches!(spec.authority(), lcl_spec::Authority::Authoritative) {
            return Err(ContractsError::UnverifiedPackage);
        }
        let registry = spec
            .registry("operations")
            .ok_or(ContractsError::MissingRegistry("operations"))?;
        let contracts = registry
            .get("contracts")
            .and_then(|c| c.as_object())
            .ok_or_else(|| ContractsError::Malformed("contracts missing".into()))?;

        let mut operations = BTreeMap::new();
        for (operation, contract) in contracts {
            operations.insert(operation.clone(), row(operation, contract)?);
        }

        Ok(Contracts {
            operations,
            parameter_binding: registry
                .get("parameter_binding")
                .and_then(|b| b.as_str())
                .unwrap_or_default()
                .to_string(),
        })
    }

    pub fn operation(&self, id: &str) -> Option<&OperationContract> {
        self.operations.get(id)
    }

    pub fn operations(&self) -> impl Iterator<Item = &OperationContract> {
        self.operations.values()
    }

    pub fn len(&self) -> usize {
        self.operations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// `named_only` in this release: "No positional parameters exist."
    pub fn parameter_binding(&self) -> &str {
        &self.parameter_binding
    }
}

fn row(operation: &str, contract: &Json) -> Result<OperationContract, ContractsError> {
    let malformed = |what: &str| ContractsError::Malformed(format!("{operation}: {what}"));

    let target = match contract.get("target") {
        Some(Json::Null) | None => None,
        Some(target) => Some(TargetSpec {
            ty: target
                .get("type")
                .and_then(|t| t.as_str())
                .ok_or_else(|| malformed("target.type"))?
                .to_string(),
            required: target
                .get("required")
                .and_then(|r| r.as_bool())
                .unwrap_or(false),
        }),
    };

    let mut parameters = BTreeMap::new();
    if let Some(members) = contract.get("parameters").and_then(|p| p.as_object()) {
        for (name, spec) in members {
            let ty = spec
                .get("type")
                .and_then(|t| t.as_str())
                .ok_or_else(|| malformed(&format!("parameter {name} type")))?
                .to_string();
            let constraints: Vec<String> = strings(spec, "constraints");
            parameters.insert(
                name.clone(),
                ParameterSpec {
                    name: name.clone(),
                    default: default_value(spec.get("default"), &ty),
                    required: spec
                        .get("required")
                        .and_then(|r| r.as_bool())
                        .unwrap_or(false),
                    bound: constraints.iter().find_map(|c| Bound::parse(c)),
                    constraints,
                    ty,
                },
            );
        }
    }

    Ok(OperationContract {
        operation: operation.to_string(),
        category: contract
            .get("category")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string(),
        meaning: contract
            .get("meaning")
            .and_then(|m| m.as_str())
            .unwrap_or_default()
            .to_string(),
        target,
        parameters,
        result_schema: contract
            .get("result_schema")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        errors: strings(contract, "errors").into_iter().collect(),
        invocation_resolution: contract
            .get("invocation_resolution")
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_string(),
        maximum: Axes::from_registry(
            &strings(contract, "possible_dependencies"),
            &strings(contract, "possible_effects"),
        ),
    })
}

/// One registry default, read as the LCL value the declared type calls for.
///
/// JSON null is "no declared default", not `NULL`. Every other JSON form maps
/// by the parameter's own contract type, so a string default under an
/// identifier type becomes an identifier and a string default under `STRING`
/// becomes text — the same two things a source document would have written.
fn default_value(default: Option<&Json>, ty: &str) -> Option<Value> {
    match default? {
        Json::Null => None,
        Json::Bool(flag) => Some(Value::Boolean(*flag)),
        Json::Number(number) => {
            let exact = Integer::parse_digits(&format!("{}", *number as i64))?;
            Some(Value::Integer(Decimal::from_integer(exact)))
        }
        Json::Array(items) if items.is_empty() => Some(Value::List(Vec::new())),
        Json::Array(_) => None,
        Json::Object(members) if members.is_empty() => Some(Value::Object(BTreeMap::new())),
        Json::Object(_) => None,
        Json::String(text) => Some(if is_identifier_type(ty) {
            // `ENUM[ascending|descending]` and `qualified_identifier(format)`
            // are written in source as bare identifiers, and the evaluator
            // reads a bare identifier as exactly this.
            Value::Identifier(text.clone())
        } else {
            Value::Text(text.clone())
        }),
    }
}

/// Whether a contract type string denotes identifiers rather than text.
fn is_identifier_type(ty: &str) -> bool {
    ty.split('|').all(|alternative| {
        let alternative = alternative.trim();
        alternative.starts_with("ENUM[") || alternative.starts_with("qualified_identifier(")
    })
}

fn strings(value: &Json, member: &str) -> Vec<String> {
    value
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
