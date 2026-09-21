//! Building results that satisfy their registered schema.
//!
//! Authority: `built_in_groups_and_results_v0.1.0.json#/result_schemas` and
//! `#/result_contract`, plus `05_SEMANTICS/05`.
//!
//! ## Why a constructor per schema
//!
//! Each of the nine schemas states its own cardinalities and its own
//! constraints — "status.succeeded requires items and count", "items and count
//! are either both absent or both present exactly once" — and a producer that
//! filled a `BTreeMap` freehand could satisfy none of them and still compile.
//! A constructor per schema makes the required fields the arguments, so a
//! result missing a required field is a call that does not typecheck rather
//! than a record that fails validation later.
//!
//! ## What these deliberately do not set
//!
//! Nothing here touches `status`, `failure_phase`, `effect_state`,
//! `output_binding` or `execution_errors`. Those are the six common fields, and
//! `result_contract/invariants` keeps them independent of every schema-local
//! domain outcome: "status is producer execution status and is independent of
//! every schema-local domain outcome." The runtime sets them from what actually
//! happened. A domain outcome — `valid` FALSE, a nonzero `exit_code`, `passed`
//! FALSE — is a field, and never a failure.

use lcl_checker::numeric::{Decimal, Integer};
use lcl_runtime::capability::Observation;
use lcl_runtime::Value;

/// An exact non-negative count as an LCL INTEGER.
pub fn count(n: usize) -> Value {
    Value::Integer(Decimal::from_integer(Integer::from_u64(n as u64)))
}

/// `result.value`: one produced value.
///
/// > status.succeeded requires value exactly once.
/// > The evidence list may be empty when no declared EVIDENCE object is
/// > required.
pub fn value(produced: Value) -> Observation {
    Observation::none()
        .with("value", produced)
        .with("evidence", Value::List(Vec::new()))
}

/// `result.collection`: the produced members and their exact count.
///
/// > items and count are either both absent or both present exactly once.
/// > count is non-negative and equals the actual number of members in items.
///
/// The count is computed from the members here rather than accepted from a
/// caller, so the constraint holds by construction.
pub fn collection(items: Vec<Value>) -> Observation {
    let size = items.len();
    Observation::none()
        .with("items", Value::List(items))
        .with("count", count(size))
}

/// `result.operation`: whether a requested target state changed.
///
/// > changed is FALSE when no requested target state changed, TRUE when at
/// > least one requested target state is known to have changed, and UNKNOWN
/// > when [it cannot be established].
///
/// `target` is `exactly_one` in this schema, so it is an argument rather than
/// an option: an operation result that could not name its target would not
/// satisfy its own schema.
pub fn operation(target: Value, changed: Value) -> Observation {
    Observation::none()
        .with("changed", changed)
        .with("target", target)
}

/// `result.operation` carrying a produced value alongside the change.
pub fn operation_with_value(target: Value, changed: Value, produced: Value) -> Observation {
    operation(target, changed).with("value", produced)
}

/// `result.validation`: whether the target satisfies its declared rules.
///
/// > valid TRUE requires an empty errors list; valid FALSE requires at least
/// > one domain validation error.
///
/// The two are therefore derived from one another here: `valid` is exactly
/// "the domain findings are empty", which is the only pairing the schema
/// admits.
pub fn validation(errors: Vec<Value>) -> Observation {
    Observation::none()
        .with("valid", Value::Boolean(errors.is_empty()))
        .with("errors", Value::List(errors))
}

/// `result.verification`: whether a declared assertion held.
pub fn verification(
    verified: Value,
    observed: Value,
    errors: Vec<Value>,
    evidence: Vec<Value>,
) -> Observation {
    Observation::none()
        .with("verified", verified)
        .with("observed", observed)
        .with("errors", Value::List(errors))
        .with("evidence", Value::List(evidence))
}

/// `result.test`: whether one declared comparison passed.
pub fn test(passed: Value, expected: Option<Value>, actual: Option<Value>) -> Observation {
    let mut observation = Observation::none()
        .with("passed", passed)
        .with("evidence", Value::List(Vec::new()));
    if let Some(expected) = expected {
        observation = observation.with("expected", expected);
    }
    if let Some(actual) = actual {
        observation = observation.with("actual", actual);
    }
    observation
}

/// `result.message`: whether a message reached its recipient.
pub fn message(delivered: Value, recipient: Value, message_id: Value) -> Observation {
    Observation::none()
        .with("delivered", delivered)
        .with("recipient", recipient)
        .with("message_id", message_id)
}

/// `result.transfer`: what moved, from where, to where.
pub fn transfer(source: Value, destination: Value, bytes: Value) -> Observation {
    Observation::none()
        .with("source", source)
        .with("destination", destination)
        .with("bytes", bytes)
}

/// `result.command`: what a command did.
/// `result.command` in graph mode.
///
/// `built_in_groups_and_results_v0.1.0.json#/result.command`: "In graph mode,
/// started, completed, exit_code, stdout, and stderr are absent; graph
/// completion is represented by status and no command observation is
/// synthesized", and "value is present exactly when the completed graph exposes
/// one material primary result; otherwise value is absent."
pub fn graph_command(primary: Option<Value>) -> Observation {
    let observation = Observation::none().with("mode", Value::Identifier("graph".to_string()));
    match primary {
        Some(value) => observation.with("value", value),
        None => observation,
    }
}

pub fn command(
    mode: &str,
    started: bool,
    completed: bool,
    exit_code: Value,
    stdout: String,
    stderr: String,
) -> Observation {
    Observation::none()
        .with("mode", Value::Identifier(mode.to_string()))
        .with("started", Value::Boolean(started))
        .with("completed", Value::Boolean(completed))
        .with("exit_code", exit_code)
        .with("stdout", Value::Text(stdout))
        .with("stderr", Value::Text(stderr))
}
