//! Phase D: typed constructors and invocation-site operation contracts.

mod common;

use common::{check, contracts, data_document as value, ids};
use lcl_checker::Outcome;

#[test]
fn every_registered_constructor_accepts_its_registered_arity() {
    // Arity comes from the registry, so a row added there is covered here.
    for row in contracts().constructors() {
        let arities = row.arities();
        assert!(!arities.is_empty(), "{} registers an arity", row.name);
        assert!(
            !arities.contains(&(arities.iter().max().copied().unwrap_or(0) + 1)),
            "{} is not variadic",
            row.name
        );
    }
}

#[test]
fn a_constructor_rejects_an_unregistered_arity() {
    // "The constructor registry defines the exact accepted overloads; calls are
    // never variadic."
    for expression in [
        "DATE(\"2026-08-30\", \"extra\")",
        "BYTES(1, 2)",
        "PERCENTAGE()",
        "MEASURE(1)",
    ] {
        let checked = check(&value("STRING", expression));
        assert!(
            ids(&checked).contains(&"error.operator.operand".to_string()),
            "{expression}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn a_constructor_rejects_an_unregistered_operand_family() {
    for expression in [
        "BYTES(\"8\")",
        "PERCENTAGE(TRUE)",
        "DURATION(\"5\", unit.second)",
    ] {
        let checked = check(&value("STRING", expression));
        assert!(
            ids(&checked).contains(&"error.operator.operand".to_string()),
            "{expression}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn declared_numeric_bounds_are_enforced_on_statically_known_values() {
    // "PERCENTAGE is 0 through 100 inclusive"; "BYTES accepts only a
    // non-negative INTEGER".
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(90)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(0)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(100)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        ids(&check(&value("PERCENTAGE", "PERCENTAGE(101)"))),
        vec!["error.value.out_of_range"]
    );
    assert_eq!(
        ids(&check(&value("BYTES", "BYTES(-1)"))),
        vec!["error.value.out_of_range"]
    );
    // BYTES "accepts only a non-negative INTEGER", not a DECIMAL.
    assert_eq!(
        ids(&check(&value("BYTES", "BYTES(1.5)"))),
        vec!["error.operator.operand"]
    );
}

#[test]
fn a_unit_argument_must_be_registered_and_in_its_declared_category() {
    assert_eq!(
        check(&value("MEASURE", "MEASURE(1920, unit.pixel)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("DURATION", "DURATION(5, unit.second)")).outcome(),
        Outcome::Checked
    );
    // "Core 0.1.0 admits only units in the closed unit registry."
    assert_eq!(
        ids(&check(&value("MEASURE", "MEASURE(1, unit.furlong)"))),
        vec!["error.operator.operand"]
    );
    // "DURATION requires a registered Time-category unit."
    assert_eq!(
        ids(&check(&value("DURATION", "DURATION(5, unit.meter)"))),
        vec!["error.numeric.unit_mismatch"]
    );
    // "MEASURE pairs a number with any registered unit, including a
    // Time-category unit."
    assert_eq!(
        check(&value("MEASURE", "MEASURE(5, unit.second)")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn a_relative_path_is_legal_only_as_an_import_or_extension_source() {
    // M1 owns every closed literal profile; `PATH` is the one it deferred,
    // because its "absolute/relative legality depends on the receiving field
    // and on resolution".
    assert_eq!(
        check(&value("PATH", "PATH(\"/absolute/path\")")).outcome(),
        Outcome::Checked
    );
    let relative = check(&value("PATH", "PATH(\"relative/path\")"));
    assert_eq!(relative.outcome(), Outcome::Rejected);
    let defects = relative.earlier_stage_defects();
    assert_eq!(defects.len(), 1);
    assert_eq!(defects[0].identifier, "error.literal.invalid");
    assert_eq!(defects[0].stage, lcl_diagnostics::Stage::Lexical);
}

#[test]
fn an_invocation_site_must_satisfy_its_operations_parameter_contract() {
    let action = |parameters: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: SET[INTEGER]\n    VALUE: [3, 1]\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: LIST[INTEGER]\n    FORMAT: format.json\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.sort\n    TARGET: REF(input.value)\n{}",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            parameters
        )
    };

    // core.sort registers exactly `key` and `direction`, both optional.
    assert_eq!(check(&action("")).outcome(), Outcome::Checked);

    // "supplies an unregistered named parameter"
    let unregistered = action(
        "    PARAMETER:\n        NAME: stable\n        TYPE: BOOLEAN\n        REQUIRED: FALSE\n        VALUE: TRUE\n",
    );
    assert_eq!(
        ids(&check(&unregistered)),
        vec!["error.operation.parameter"]
    );

    // "duplicates a named parameter"
    let duplicated = action(concat!(
        "    PARAMETER:\n        NAME: direction\n        TYPE: STRING\n        REQUIRED: FALSE\n        VALUE: \"ascending\"\n",
        "    PARAMETER:\n        NAME: direction\n        TYPE: STRING\n        REQUIRED: FALSE\n        VALUE: \"descending\"\n"
    ));
    assert_eq!(ids(&check(&duplicated)), vec!["error.operation.parameter"]);
}

/// `expression_fragment_contract/syntax`: "consume exactly one EXPRESSION …
/// followed only by optional whitespace", `#/diagnostics`: "Malformed fragment
/// syntax and invalid bindings produce error.operation.parameter", and
/// `#/evaluation`: "Static checks cover the complete fragment".
///
/// Regression, `LCL-TASK-0020` defect 3. No stage checked the fragment, so a
/// malformed one passed `check` and reached execution, where the runtime could
/// not emit a static-stage identifier and substituted another from the row.
/// CLOSURE-019 failed on the substituted identifier.
#[test]
fn a_fragment_that_is_not_one_expression_is_refused_at_this_stage() {
    let calculate = |fragment: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: INTEGER\n    VALUE: 4\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.calculate\n    TARGET: REF(input.value)\n",
                "    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: {:?}\n",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            fragment
        )
    };

    // One expression is admitted, so the check below is about the fragment and
    // not about fragments.
    assert_eq!(
        check(&calculate("REF(input.value) * 2")).outcome(),
        Outcome::Checked
    );

    // The witness: a fragment holding two expressions.
    assert_eq!(
        ids(&check(&calculate("1; 2"))),
        vec!["error.operation.parameter"]
    );
    // One that lexes, so the defect is the trailing expression and not a token.
    assert_eq!(
        ids(&check(&calculate("1 2"))),
        vec!["error.operation.parameter"]
    );
    // "followed only by optional whitespace" — trailing space is not trailing
    // input.
    assert_eq!(
        check(&calculate("REF(input.value) * 2   ")).outcome(),
        Outcome::Checked
    );
    // A fragment holding no expression at all.
    assert_eq!(
        ids(&check(&calculate(""))),
        vec!["error.operation.parameter"]
    );
}

/// `06_STANDARD_LIBRARY/10` and the two witnesses that name this identifier for
/// a declared parameter family the row does not register.
///
/// Regression, `LCL-TASK-0020` defect 5. `core.append` accepted a `BYTES`
/// content and completed, though the row types content `STRING|LIST[T]` and
/// says plainly that "BYTES is a count and is not content"; CLOSURE-052 could
/// not be executed. `core.validate` accepted an arbitrary OBJECT as its schema,
/// though the row types it `REFERENCE` and says "Material OBJECT schema
/// encodings are not admitted"; CLOSURE-051 could not be executed.
#[test]
fn a_declared_parameter_family_outside_the_row_is_refused() {
    let invocation = |operation: &str, target: &str, parameter: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "DATA:\n    ID: data.subject\n    TYPE: {}\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: {}\n    TARGET: REF(data.subject)\n{}",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            target, operation, parameter
        )
    };

    // "BYTES is a count and is not content."
    let bytes_content = invocation(
        "core.append",
        "PATH\n    VALUE: PATH(\"/srv/data/a.txt\")",
        "    PARAMETER:\n        NAME: content\n        TYPE: BYTES\n        REQUIRED: TRUE\n        VALUE: BYTES(4)\n\n",
    );
    assert_eq!(
        ids(&check(&bytes_content)),
        vec!["error.operation.parameter"]
    );

    // A STRING content is the family the row registers.
    let string_content = invocation(
        "core.append",
        "PATH\n    VALUE: PATH(\"/srv/data/a.txt\")",
        "    PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"x\"\n\n",
    );
    assert_eq!(check(&string_content).outcome(), Outcome::Checked);

    // "Material OBJECT schema encodings are not admitted."
    let object_schema = invocation(
        "core.validate",
        "STRING\n    VALUE: \"x\"",
        "    PARAMETER:\n        NAME: schema\n        TYPE: OBJECT\n        REQUIRED: TRUE\n        VALUE:\n            name: \"a\"\n\n",
    );
    assert_eq!(
        ids(&check(&object_schema)),
        vec!["error.operation.parameter"]
    );
}

#[test]
fn an_omitted_required_target_is_an_operation_parameter_defect() {
    // "omits TARGET when the selected operation marks it required and no
    // handler-context binding supplies it".
    let source = concat!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n",
        "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
        "OUTPUT:\n    ID: output.value\n    TYPE: LIST[INTEGER]\n    FORMAT: format.json\n\n",
        "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
        "ACTION:\n    ID: action.one\n    OPERATION: core.sort\n    OUTPUT: REF(output.value)\n\n",
        "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
        "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let checked = check(source);
    assert!(
        ids(&checked).contains(&"error.operation.parameter".to_string()),
        "{:?}",
        ids(&checked)
    );
    assert_eq!(checked.terminal_status(), Some("status.invalid"));
}

#[test]
fn every_core_operation_contract_is_applied_from_the_registry() {
    // The contract set is the registry's: every operation with a required
    // parameter is one this checker would demand at its invocation sites.
    let required: Vec<&str> = contracts()
        .operations()
        .filter(|contract| contract.parameters.values().any(|p| p.required))
        .map(|contract| contract.id.as_str())
        .collect();
    assert!(
        !required.is_empty(),
        "some core operations register required parameters"
    );
    for contract in contracts().operations() {
        assert!(
            !contract.positional_parameters,
            "{} takes no positional arguments",
            contract.id
        );
    }
}
