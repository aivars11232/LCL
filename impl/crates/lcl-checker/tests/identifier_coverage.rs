//! Every registered `static_or_expression` identifier has a fixture that
//! produces it.
//!
//! The task's acceptance criterion is that "all registered type/value contracts
//! are covered or explicitly proven non-executable at this layer". This is that
//! proof: the twelve identifiers come from the registry, and each is paired with
//! a source that raises it. A registry that gained a thirteenth would fail here
//! rather than pass silently.

mod common;

use common::{check, HEADER};
use lcl_checker::StaticError;
use std::collections::BTreeSet;

/// A `kind.data` document whose `DATA` block declares `ty` and holds `value`.
fn data(ty: &str, value: &str) -> String {
    common::data_document(ty, value)
}

/// A `DEFINE kind.operation` whose one parameter carries `extra` constraints.
fn parameter(ty: &str, value: &str, extra: &str) -> String {
    format!(
        "{HEADER}\nDEFINE:\n    ID: operation.one\n    KIND: kind.operation\n    MEANING: \"m\"\n    SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: subject\n        TYPE: {ty}\n        REQUIRED: TRUE\n        VALUE: {value}\n{extra}"
    )
}

fn fixtures() -> Vec<(StaticError, String)> {
    vec![
        (StaticError::TypeMismatch, data("INTEGER", "\"three\"")),
        (
            // "A LIST or SET contains a member incompatible with its declared
            // item type."
            StaticError::CollectionHeterogeneous,
            data("LIST[INTEGER]", "[1, \"two\"]"),
        ),
        (
            StaticError::ObjectSchema,
            format!(
                "{HEADER}\nDEFINE:\n    ID: type.record\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: title\n        TYPE: STRING\n        REQUIRED: TRUE\n\nDATA:\n    ID: data.record\n    TYPE: OBJECT[REF(type.record)]\n    VALUE:\n        other: \"x\"\n"
            ),
        ),
        (StaticError::OperatorOperand, data("BOOLEAN", "\"a\" < 1")),
        (StaticError::NumericDivisionByZero, data("DECIMAL", "1 / 0")),
        (StaticError::NumericNonTerminating, data("DECIMAL", "1 / 3")),
        (
            StaticError::NumericUnitMismatch,
            data("DURATION", "DURATION(5, unit.meter)"),
        ),
        (
            StaticError::ValueOutOfRange,
            data("PERCENTAGE", "PERCENTAGE(101)"),
        ),
        (StaticError::ValueUnknown, data("INTEGER", "UNKNOWN")),
        (
            StaticError::PatternMismatch,
            parameter("STRING", "\"Alpha1\"", "        PATTERN: REGEX(\"[a-z]+\")\n"),
        ),
        (
            StaticError::PatternResourceLimit,
            // "Exhausting a declared finite resource limit while compiling or
            // matching either profile produces error.pattern.resource_limit."
            parameter("STRING", "\"a\"", "        PATTERN: REGEX(\"a{999999}\")\n"),
        ),
        (
            StaticError::OperationParameter,
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: SET[INTEGER]\n    VALUE: [1]\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: LIST[INTEGER]\n    FORMAT: format.json\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.sort\n    TARGET: REF(input.value)\n",
                "    PARAMETER:\n        NAME: stable\n        TYPE: BOOLEAN\n        REQUIRED: FALSE\n        VALUE: TRUE\n",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            )
            .to_string(),
        ),
    ]
}

#[test]
fn every_registered_static_identifier_is_produced_by_a_fixture() {
    let mut produced: BTreeSet<StaticError> = BTreeSet::new();
    for (identifier, source) in fixtures() {
        let checked = check(&source);
        let raised: Vec<String> = checked
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect();
        assert!(
            raised.contains(&identifier.to_string()),
            "the fixture for {identifier} raised {raised:?}"
        );
        produced.insert(identifier);
    }

    let registered: BTreeSet<StaticError> = StaticError::ALL.into_iter().collect();
    let uncovered: Vec<String> = registered
        .difference(&produced)
        .map(ToString::to_string)
        .collect();
    assert!(
        uncovered.is_empty(),
        "these registered identifiers have no fixture: {uncovered:?}"
    );
    assert_eq!(produced.len(), 12);
}

#[test]
fn every_produced_diagnostic_carries_its_registered_metadata() {
    for (identifier, source) in fixtures() {
        let checked = check(&source);
        let raised = checked
            .diagnostics()
            .iter()
            .find(|d| d.id == identifier)
            .expect("the fixture raises it");
        let registered = common::contracts().error(identifier);
        assert_eq!(raised.default_status, registered.default_status);
        assert_eq!(raised.meaning, registered.meaning);
        assert_eq!(raised.stage(), lcl_diagnostics::Stage::StaticOrExpression);
    }
}
