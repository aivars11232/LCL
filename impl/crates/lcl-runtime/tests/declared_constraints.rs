//! The demanding layer's consumer of declared value constraints the static
//! stage could not decide: `DemandKind::DeclaredBound` and `DeclaredPattern`.
//!
//! `expression_demand_resolution` makes both demand-eligible: "A dynamically
//! supplied material value violates an exact registered bound" and "A
//! dynamically supplied value fails a declared GLOB or REGEX value constraint".
//!
//! Regressions, PRETEST-01 F01 and F03.

mod common;

use common::{decimal, execute, fixture, fixture_with};
use lcl_runtime::RuntimeError;
use lcl_semantics::Invocation;

const BOUNDED: &str = "\nDEFINE:\n    ID: type.bounded\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: ratio\n        TYPE: DECIMAL\n        REQUIRED: TRUE\n        MAXIMUM: 1\n";

fn data_document(blocks: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.constraints\n    \
         NAME: \"Constraint fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n{BOUNDED}{blocks}"
    )
}

fn object_reading(ratio: &str) -> String {
    data_document(&format!(
        "\nDATA:\n    ID: data.ratio\n    TYPE: DECIMAL\n    VALUE: {ratio}\n\n\
         DATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF(type.bounded)]\n    VALUE:\n        ratio: REF(data.ratio)\n\n\
         DATA:\n    ID: data.subject\n    TYPE: DECIMAL\n    VALUE: REF(data.bounded).ratio\n"
    ))
}

#[test]
fn a_deferred_object_field_bound_is_judged_when_the_object_is_demanded() {
    let fault = fixture(&object_reading("1.5"))
        .demand("data.subject")
        .expect_err("ratio exceeds its declared MAXIMUM");
    assert_eq!(fault.id, RuntimeError::ValueOutOfRange);
    assert!(fault.demand_resolved);

    assert_eq!(
        fixture(&object_reading("0.75")).demand("data.subject"),
        Ok(decimal("0.75"))
    );
}

#[test]
fn a_nested_object_uses_its_own_types_constraints_at_demand() {
    let source = data_document(
        "\nDEFINE:\n    ID: type.outer\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: inner\n        TYPE: OBJECT[REF(type.bounded)]\n        REQUIRED: TRUE\n\n\
         DATA:\n    ID: data.ratio\n    TYPE: DECIMAL\n    VALUE: 2.0\n\n\
         DATA:\n    ID: data.outer\n    TYPE: OBJECT[REF(type.outer)]\n    VALUE:\n        inner:\n            ratio: REF(data.ratio)\n\n\
         DATA:\n    ID: data.subject\n    TYPE: OBJECT[REF(type.outer)]\n    VALUE: REF(data.outer)\n",
    );
    let fault = fixture(&source)
        .demand("data.subject")
        .expect_err("the nested ratio exceeds its declared MAXIMUM");
    assert_eq!(fault.id, RuntimeError::ValueOutOfRange);
}

/// A supplied invocation datum is constructed against the same schema.
#[test]
fn a_supplied_object_is_judged_against_its_schema_when_demanded() {
    let source = format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.supplied\n    \
         NAME: \"Supplied fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n{BOUNDED}\n\
         INPUT:\n    ID: input.bounded\n    TYPE: OBJECT[REF(type.bounded)]\n    REQUIRED: FALSE\n    DEFAULT: REF(const.fallback)\n\n\
         DEFINE:\n    ID: const.fallback\n    KIND: kind.constant\n    TYPE: OBJECT[REF(type.bounded)]\n    VALUE:\n        ratio: 0.5\n\n\
         DATA:\n    ID: data.subject\n    TYPE: DECIMAL\n    VALUE: REF(input.bounded).ratio\n\n\
         GOAL:\n    ID: goal.supplied\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.supplied\n    OPERATION: core.inspect\n    TARGET: REF(goal.supplied)\n\n\
         SUCCESS:\n    ID: success.supplied\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.supplied\n    GOAL: REF(goal.supplied)\n    INPUT: REF(input.bounded)\n    \
         ACTION: REF(action.supplied)\n    SUCCESS: REF(success.supplied)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.supplied)\n"
    );
    let supplied = lcl_runtime::Value::Object(
        [("ratio".to_string(), decimal("1.5"))]
            .into_iter()
            .collect(),
    );
    let fault = fixture_with(&source, Invocation::new().with("input.bounded", supplied))
        .demand("data.subject")
        .expect_err("the supplied ratio exceeds its declared MAXIMUM");
    assert_eq!(fault.id, RuntimeError::ValueOutOfRange);
}

fn calculate_document(expression: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.parameter\n    \
         NAME: \"Parameter fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
         INPUT:\n    ID: input.value\n    TYPE: INTEGER\n    VALUE: 7\n\n\
         DATA:\n    ID: data.expression\n    TYPE: STRING\n    VALUE: {expression}\n\n\
         OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n\
         GOAL:\n    ID: goal.calculate\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.calculate\n    OPERATION: core.calculate\n    TARGET: REF(input.value)\n    \
         PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        \
         VALUE: REF(data.expression)\n        PATTERN: REGEX(\"[0-9 +]+\")\n    OUTPUT: REF(output.value)\n\n\
         SUCCESS:\n    ID: success.calculate\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.calculate\n    GOAL: REF(goal.calculate)\n    INPUT: REF(input.value)\n    \
         ACTION: REF(action.calculate)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.calculate)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.calculate)\n"
    )
}

/// A PARAMETER's own declared constraint over a value only the invocation
/// knows is judged when the parameter is demanded, before dispatch.
#[test]
fn a_deferred_parameter_pattern_is_judged_before_dispatch() {
    let (execution, host) = execute(&calculate_document("\"REF(input.value)\""));
    let errors: Vec<RuntimeError> = execution.diagnostics().iter().map(|d| d.id).collect();
    assert!(
        errors.contains(&RuntimeError::PatternMismatch),
        "{errors:?}"
    );
    assert!(host.requests().is_empty());

    let (execution, _host) = execute(&calculate_document("\"1 + 1\""));
    let errors: Vec<RuntimeError> = execution.diagnostics().iter().map(|d| d.id).collect();
    assert!(
        !errors.contains(&RuntimeError::PatternMismatch),
        "{errors:?}"
    );
}
