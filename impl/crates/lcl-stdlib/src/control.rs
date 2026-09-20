//! Comparison, checking, and the rows that coordinate execution itself.
//!
//! ## Why the control rows are not all alike
//!
//! `control` is the registry's category for rows that "coordinate, delegate,
//! stop, resume, retry, or cancel execution, or request authoritative input",
//! and the five members reach five different places:
//!
//! * `core.retry` and `core.continue` are meaningful **only inside a selected
//!   handler**, and the runtime already owns that context. Outside one, their
//!   own contract makes them a precondition failure, which is decided here.
//! * `core.cancel` and `core.stop` act on the runtime's lifecycle state, which
//!   is the runtime's to transition.
//! * `core.ask` reaches a person, and crosses the boundary like any other
//!   external row.
//! * `core.test` compares, and only delegates to a graph when its target names
//!   one.

use crate::contracts::OperationContract;
use crate::{params, pure, schema, Stdlib};
use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::operations::{Invocation, Resolution};
use lcl_runtime::pattern::{MatchFault, PatternFault};
use lcl_runtime::{capability::CapabilityRequest, order_profile, strict_equal, Value};
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Resolve one invocation of a control row.
pub(crate) fn invoke(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    match contract.operation.as_str() {
        // "core.retry consumes that same ACTION invocation's existing budget and
        // is invalid outside its selected handler context." A reachable ACTION
        // is not a handler, and the runtime resolves the handler case before a
        // request is ever built.
        "core.retry" => Resolution::failed(
            RuntimeError::OperationPrecondition,
            "handler_context",
            "core.retry is valid only inside the selected handler of the ACTION whose \
             RETRY it authorizes",
        ),
        // "core.continue likewise requires its selected same-origin handler
        // context; a missing or mismatched context uses
        // error.operation.precondition."
        "core.continue" => Resolution::failed(
            RuntimeError::OperationPrecondition,
            "handler_context",
            "core.continue is valid only while a selected handler is handling its \
             originating event",
        ),
        "core.test" => test(stdlib, cx, request, contract, parameters),
        // core.cancel, core.stop and core.ask reach the runtime's lifecycle or
        // the host, neither of which this module decides.
        _ => {
            // "every option is compatible with expected_type" is a core.ask
            // precondition, so an incompatible option refuses before the
            // question is ever put to a person.
            if contract.operation == "core.ask" {
                if let Some(failure) = incompatible_option(parameters) {
                    return failure;
                }
            }
            // "Each listed role applies to every invocation except
            // core.execute": core.stop selects its stop role before crossing.
            let target_class = request
                .target
                .as_ref()
                .map(|value| params::classify(cx, &pure::read_through(cx, value)))
                .unwrap_or(lcl_capabilities::AddressClass::Material);
            if let Some(failure) =
                crate::data::select_profiles(stdlib, contract, target_class, None)
            {
                return failure;
            }
            let mut resolved = request.clone();
            resolved.parameters = parameters.clone();
            Resolution::Host(Box::new(resolved))
        }
    }
}

/// The first `core.ask` option its `expected_type` does not admit.
fn incompatible_option(parameters: &BTreeMap<String, Value>) -> Option<Resolution> {
    let expected_type = match parameters.get("expected_type")? {
        Value::Text(name) | Value::Identifier(name) => name.clone(),
        _ => return None,
    };
    let Some(Value::List(options)) = parameters.get("options") else {
        return None;
    };
    let option = options
        .iter()
        .find(|option| !crate::host::compatible_answer(option, &expected_type))?;
    Some(Resolution::failed(
        RuntimeError::TypeMismatch,
        "options",
        format!(
            "a core.ask option of family {} is not compatible with expected_type \
             {expected_type}",
            option.family()
        ),
    ))
}

/// `core.compare`: one declared criterion over two operands.
///
/// > core.compare evaluates one criterion under the registered operator and
/// > sentinel rules. Omitted criteria selects ==. That equality form accepts
/// > MISSING and UNKNOWN as singleton sentinels and produces material BOOLEAN.
/// > A supplied non-==/!= criterion that encounters MISSING uses
/// > error.required.missing; a criterion whose result remains UNKNOWN uses
/// > error.value.unknown because successful result.value content is material.
pub(crate) fn compare(
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    _contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let Some(left) = request.target.as_ref().map(|t| pure::read_through(cx, t)) else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "target",
            "core.compare requires a TARGET",
        );
    };
    let Some(right) = parameters.get("against").map(|v| pure::read_through(cx, v)) else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "against",
            "core.compare requires an against operand",
        );
    };

    let criteria = parameters
        .get("criteria")
        .map(|declared| pure::read_through(cx, declared));
    let (operator, left_path, right_path) = match criterion(criteria.as_ref()) {
        Ok(criterion) => criterion,
        Err(resolution) => return resolution,
    };

    // "Omitted paths select the complete corresponding operand; supplied paths
    // project through exact registered OBJECT fields."
    let left = match project(&left, left_path.as_deref()) {
        Some(value) => value,
        None => Value::Missing,
    };
    let right = match project(&right, right_path.as_deref()) {
        Some(value) => value,
        None => Value::Missing,
    };

    // "That equality form accepts MISSING and UNKNOWN as singleton sentinels."
    let equality = operator == "==" || operator == "!=";
    if !equality && (left == Value::Missing || right == Value::Missing) {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "criteria",
            format!("{operator} encountered a MISSING operand"),
        );
    }

    match apply(&operator, &left, &right) {
        Ok(Value::Unknown) => Resolution::failed(
            RuntimeError::ValueUnknown,
            "criteria",
            "the comparison remained UNKNOWN, and successful result.value content is material",
        ),
        // "result.value.value is exactly one BOOLEAN; FALSE is a successful
        // comparison result."
        Ok(outcome) => Resolution::Completed(schema::value(outcome)),
        Err(fault) => Resolution::failed(fault.0, "criteria", fault.1),
    }
}

/// A registered comparison this module could not apply.
struct CompareFault(RuntimeError, String);

/// Read the declared criterion: a token, a closed OBJECT, or a reference to one.
fn criterion(
    declared: Option<&Value>,
) -> Result<(String, Option<String>, Option<String>), Resolution> {
    const OPERATORS: [&str; 9] = [
        "==", "!=", "<", "<=", ">", ">=", "IN", "CONTAINS", "MATCHES",
    ];
    // "Unknown keys, malformed paths, absent operator, and an unregistered
    // token produce error.operation.parameter."
    let malformed =
        |detail: String| Resolution::failed(RuntimeError::OperationParameter, "criteria", detail);

    match declared {
        // The registry default is "==", so an omitted criterion arrives here
        // already defaulted.
        None | Some(Value::Missing) => Ok(("==".to_string(), None, None)),
        Some(Value::Text(token)) => {
            if !OPERATORS.contains(&token.as_str()) {
                return Err(malformed(format!("{token:?} is not a registered operator")));
            }
            Ok((token.clone(), None, None))
        }
        Some(Value::Object(fields)) => {
            let Some(Value::Text(operator)) = fields.get("operator") else {
                return Err(malformed(
                    "a criteria OBJECT requires a STRING operator".to_string(),
                ));
            };
            if !OPERATORS.contains(&operator.as_str()) {
                return Err(malformed(format!(
                    "{operator:?} is not a registered operator"
                )));
            }
            // "Unknown keys ... produce error.operation.parameter", which the
            // execution stage expresses through the row's own precondition.
            for key in fields.keys() {
                if !matches!(key.as_str(), "operator" | "left" | "right") {
                    return Err(malformed(format!("{key:?} is not a criteria key")));
                }
            }
            // A supplied path is a property_path STRING; any other form is
            // one of that sentence's malformed paths.
            let mut paths = [None, None];
            for (slot, name) in paths.iter_mut().zip(["left", "right"]) {
                *slot = match fields.get(name) {
                    Some(Value::Text(path)) => Some(path.clone()),
                    None => None,
                    Some(other) => {
                        return Err(malformed(format!(
                            "criteria {name} must be a property path STRING, found {}",
                            other.family()
                        )))
                    }
                };
            }
            let [left, right] = paths;
            Ok((operator.clone(), left, right))
        }
        // "One REFERENCE resolves once to a declared STRING or OBJECT value
        // satisfying the same contract", so a value of neither form leaves the
        // row's "criteria are type-valid" precondition unmet by its type.
        Some(other) => Err(Resolution::failed(
            RuntimeError::TypeMismatch,
            "criteria",
            format!(
                "criteria must be a STRING or an OBJECT, found {}",
                other.family()
            ),
        )),
    }
}

/// Project one operand through an optional property path.
fn project(operand: &Value, path: Option<&str>) -> Option<Value> {
    match path {
        None => Some(operand.clone()),
        Some(path) => pure::property_path(operand, path),
    }
}

/// Apply one registered comparison token.
fn apply(operator: &str, left: &Value, right: &Value) -> Result<Value, CompareFault> {
    match operator {
        "==" => Ok(Value::Boolean(strict_equal(left, right))),
        "!=" => Ok(Value::Boolean(!strict_equal(left, right))),
        "<" | "<=" | ">" | ">=" => {
            if left == &Value::Unknown || right == &Value::Unknown {
                return Ok(Value::Unknown);
            }
            let Some(ordering) = order_profile::compare(left, right) else {
                return Err(CompareFault(
                    RuntimeError::OperatorOperand,
                    format!(
                        "{} and {} are not mutually order-compatible",
                        left.family(),
                        right.family()
                    ),
                ));
            };
            Ok(Value::Boolean(match operator {
                "<" => ordering == Ordering::Less,
                "<=" => ordering != Ordering::Greater,
                ">" => ordering == Ordering::Greater,
                _ => ordering != Ordering::Less,
            }))
        }
        // "IN" asks whether the left operand is a member of the right.
        "IN" => membership(right, left),
        // "CONTAINS" is the same question with the operands the other way round.
        "CONTAINS" => membership(left, right),
        "MATCHES" => matches(left, right),
        other => Err(CompareFault(
            RuntimeError::OperatorOperand,
            format!("{other} is not a registered comparison"),
        )),
    }
}

fn membership(collection: &Value, member: &Value) -> Result<Value, CompareFault> {
    if collection == &Value::Unknown || member == &Value::Unknown {
        return Ok(Value::Unknown);
    }
    match collection {
        Value::List(members) | Value::Set(members) => Ok(Value::Boolean(
            members.iter().any(|held| strict_equal(held, member)),
        )),
        // A STRING contains a substring.
        Value::Text(haystack) => match member {
            Value::Text(needle) => Ok(Value::Boolean(haystack.contains(needle.as_str()))),
            other => Err(CompareFault(
                RuntimeError::OperatorOperand,
                format!("a STRING contains a STRING, not {}", other.family()),
            )),
        },
        other => Err(CompareFault(
            RuntimeError::OperatorOperand,
            format!("membership requires a collection, found {}", other.family()),
        )),
    }
}

/// `MATCHES` through the rule the expression operator uses.
///
/// Only the identifiers are this row's own: an invalid pattern is not one
/// `core.compare` registers, so it stays an operand defect here.
fn matches(subject: &Value, pattern: &Value) -> Result<Value, CompareFault> {
    lcl_runtime::pattern::matches(subject, pattern).map_err(|fault| match fault {
        MatchFault::Operand(detail) => CompareFault(RuntimeError::OperatorOperand, detail),
        // "MATCHES resource exhaustion uses error.pattern.resource_limit."
        MatchFault::Pattern(PatternFault::ResourceLimit(detail)) => {
            CompareFault(RuntimeError::PatternResourceLimit, detail)
        }
        MatchFault::Pattern(PatternFault::Invalid(detail)) => {
            CompareFault(RuntimeError::OperatorOperand, detail)
        }
    })
}

/// `core.validate`: whether the target satisfies its declared rules.
///
/// The `rules` parameter names `VALIDATE` declarations, and preflight already
/// evaluated every selected check — "VALIDATE and VERIFY expose the Boolean
/// result of their declared check in value context". Re-evaluating them here
/// would be a second implementation of the same semantics, so this reads the
/// outcomes the plan carries.
pub(crate) fn validate(
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    _contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    let mut errors = Vec::new();
    // "The REFERENCE resolves exactly once, following transparent aliases, to a
    // kind.type whose resolved type is OBJECT and whose schema applies to the
    // target."
    if let Some(schema) = parameters.get("schema") {
        let Some(id) = params::reference_id(schema) else {
            return Resolution::failed(
                RuntimeError::ReferenceKind,
                "schema",
                "core.validate schema is a reference to a kind.type OBJECT",
            );
        };
        let index = params::declaration_index(cx, id);
        let kind = index
            .and_then(|index| lcl_runtime::syntax::declaration_block(cx.resolved, index))
            .and_then(|block| lcl_runtime::syntax::field_text(&block, "KIND"));
        let object = index
            .and_then(|index| cx.checked.declared_object_type(index))
            .cloned();
        match (kind.as_deref(), object) {
            (Some("kind.type"), Some(object)) => {
                let declared = lcl_checker::ty::Type::Object(object);
                // "whose schema applies to the target": a target the declared
                // OBJECT schema does not admit is a detected failure, recorded
                // under its registered identifier.
                let target = request.target.as_ref().map(|t| pure::read_through(cx, t));
                if !target
                    .as_ref()
                    .is_some_and(|value| params::value_matches(&declared, value))
                {
                    errors.push(Value::Identifier("error.validation.failed".to_string()));
                }
            }
            _ => {
                return Resolution::failed(
                    RuntimeError::ReferenceKind,
                    "schema",
                    format!("{id} is not a kind.type whose resolved type is OBJECT"),
                )
            }
        }
    }
    if let Some(Value::List(rules)) = parameters.get("rules") {
        for rule in rules {
            let Some(id) = params::reference_id(rule) else {
                return Resolution::failed(
                    RuntimeError::ReferenceKind,
                    "rules",
                    "core.validate rules are references to VALIDATE declarations",
                );
            };
            let Some(check) = cx.plan.checks().iter().find(|check| check.id == id) else {
                return Resolution::failed(
                    RuntimeError::ReferenceKind,
                    "rules",
                    format!("{id} is not a selected VALIDATE declaration"),
                );
            };
            match &check.outcome {
                Some(Value::Boolean(true)) => {}
                // A FALSE check is a domain finding, and "valid FALSE requires
                // at least one domain validation error".
                Some(Value::Boolean(false)) => {
                    errors.push(Value::Identifier("error.validation.failed".to_string()))
                }
                Some(Value::Unknown) => {
                    return Resolution::failed(
                        RuntimeError::ValueUnknown,
                        "rules",
                        format!("{id} resolved UNKNOWN"),
                    )
                }
                // "A skipped check has no result", and an inapplicable rule
                // contributes no finding.
                _ => {}
            }
        }
    }
    Resolution::Completed(schema::validation(errors))
}

/// `core.verify`: whether one declared assertion held.
///
/// > core.verify is deterministic exactly when its immutable verification
/// > profile is deterministic because its resolved assertion evaluation is
/// > always deterministic.
pub(crate) fn verify(
    stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    // "A missing, ambiguous, incomplete, or out-of-bounds required profile role
    // emits error.operation.precondition and fails before effects." core.verify
    // requires the `verification` role, so an engine with no verifier installed
    // refuses rather than verifying by default.
    let target_class = request
        .target
        .as_ref()
        .map(|value| params::classify(cx, value))
        .unwrap_or(lcl_capabilities::AddressClass::Material);
    for role in stdlib.catalog().required_roles(&contract.operation, None) {
        let selection = lcl_capabilities::Selection {
            operation: &contract.operation,
            role,
            target_class,
            implementation: None,
        };
        if let Err(fault) = stdlib.catalog().select(&selection) {
            return crate::data::profile_failure(&fault);
        }
    }
    let Some(assertion) = parameters.get("assertion") else {
        return Resolution::failed(
            RuntimeError::RequiredMissing,
            "assertion",
            "core.verify requires an assertion",
        );
    };
    // "result records TRUE, FALSE, or UNKNOWN and evidence." Whether each
    // declaration resolves is decided where the identifier is registered —
    // `verification_or_completion` — so this records what the invocation
    // required and step 12a judges it.
    let evidence = match parameters.get("evidence") {
        Some(Value::List(items)) => items.clone(),
        _ => Vec::new(),
    };
    let observed = request
        .target
        .as_ref()
        .map(|t| pure::read_through(cx, t))
        .unwrap_or(Value::Missing);
    // The schema records an OBJECT snapshot even when the observed target is
    // scalar. Its domain findings remain separate from execution diagnostics.
    let observed = Value::Object(BTreeMap::from([("target".to_string(), observed)]));

    match assertion_value(cx, assertion, contract, "assertion") {
        Ok(Value::Boolean(held)) => Resolution::Completed(schema::verification(
            Value::Boolean(held),
            observed,
            if held {
                Vec::new()
            } else {
                vec![Value::Identifier("error.verification.failed".to_string())]
            },
            evidence,
        )),
        // "verified ... UNKNOWN when [it] cannot be established."
        Ok(Value::Unknown) => Resolution::Completed(schema::verification(
            Value::Unknown,
            observed,
            Vec::new(),
            evidence,
        )),
        Ok(Value::Missing) => Resolution::failed(
            RuntimeError::RequiredMissing,
            "assertion",
            "core.verify resolved a MISSING assertion",
        ),
        Ok(other) => Resolution::failed(
            RuntimeError::OperatorOperand,
            "assertion",
            format!(
                "core.verify requires a BOOLEAN assertion, found {}",
                other.family()
            ),
        ),
        Err(resolution) => resolution,
    }
}

/// `core.test`: one declared comparison, after any referenced graph.
fn test(
    _stdlib: &Stdlib,
    cx: &mut Invocation<'_>,
    request: &CapabilityRequest,
    contract: &OperationContract,
    parameters: &BTreeMap<String, Value>,
) -> Resolution {
    // "A TASK or ACTION TARGET executes before the comparison." Executing a
    // referenced graph is delegation, not comparison, so it crosses the
    // boundary; a host with no graph executor reports a limitation rather than
    // a comparison it did not make.
    if let Some(id) = params::referenced_declaration(cx, "TARGET") {
        if let Some(block) = params::declaring_block(cx, &id) {
            if matches!(
                block.as_str(),
                "TASK" | "ACTION" | "PHASE" | "SEQUENCE" | "TEST"
            ) {
                let mut resolved = request.clone();
                resolved.parameters = parameters.clone();
                return Resolution::Host(Box::new(resolved));
            }
        }
    }

    let assertion = parameters.get("assertion");
    let expected = parameters.get("expected");
    let actual = parameters.get("actual");
    let target = request.target.as_ref().map(|t| pure::read_through(cx, t));

    // "Exactly one comparison form is required: assertion; or expected with
    // exactly one actual source, either the actual parameter or a
    // material-value TARGET."
    match (assertion, expected) {
        (Some(assertion), None) => {
            if actual.is_some() || target.is_some() {
                return Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "comparison_form",
                    "an assertion cannot accompany an actual source",
                );
            }
            match assertion_value(cx, assertion, contract, "assertion") {
                Ok(Value::Boolean(held)) => {
                    Resolution::Completed(schema::test(Value::Boolean(held), None, None))
                }
                Ok(Value::Unknown) => {
                    Resolution::Completed(schema::test(Value::Unknown, None, None))
                }
                Ok(other) => Resolution::failed(
                    RuntimeError::OperatorOperand,
                    "assertion",
                    format!(
                        "core.test requires a BOOLEAN assertion, found {}",
                        other.family()
                    ),
                ),
                Err(resolution) => resolution,
            }
        }
        (None, Some(expected)) => {
            let sources = usize::from(actual.is_some()) + usize::from(target.is_some());
            if sources != 1 {
                return Resolution::failed(
                    RuntimeError::OperationPrecondition,
                    "comparison_form",
                    "expected requires exactly one actual source: the actual parameter or a \
                     material-value TARGET",
                );
            }
            let actual = actual
                .map(|value| pure::read_through(cx, value))
                .or(target)
                .unwrap_or(Value::Missing);
            // "Expected-and-actual form always uses the registered ==
            // strict-equality operator."
            let passed = strict_equal(expected, &actual);
            Resolution::Completed(schema::test(
                Value::Boolean(passed),
                Some(expected.clone()),
                Some(actual),
            ))
        }
        (Some(_), Some(_)) => Resolution::failed(
            RuntimeError::OperationPrecondition,
            "comparison_form",
            "core.test takes an assertion or an expected value, never both",
        ),
        (None, None) => Resolution::failed(
            RuntimeError::OperationPrecondition,
            "comparison_form",
            "core.test requires exactly one comparison form; TARGET alone is not a \
             complete test",
        ),
    }
}

/// The Boolean an assertion parameter resolves to.
///
/// > An assertion REFERENCE resolves only a declared BOOLEAN value or
/// > expression snapshot … neither invokes an operation or profile.
fn assertion_value(
    cx: &Invocation<'_>,
    assertion: &Value,
    contract: &OperationContract,
    parameter: &str,
) -> Result<Value, Resolution> {
    match assertion {
        Value::Reference(_) => {
            let id = params::reference_id(assertion).unwrap_or_default();
            // A VERIFY or VALIDATE declaration exposes its Boolean result; any
            // other declaration exposes its value.
            Ok(cx.declaration_value(id))
        }
        // The invocation site already demanded an inline boolean_expression.
        other => {
            if matches!(other, Value::Boolean(_) | Value::Unknown | Value::Missing) {
                Ok(other.clone())
            } else {
                Err(Resolution::failed(
                    RuntimeError::OperatorOperand,
                    parameter,
                    format!(
                        "{} requires a BOOLEAN {parameter}, found {}",
                        contract.operation,
                        other.family()
                    ),
                ))
            }
        }
    }
}
