//! Every `DemandKind` the static stage retains has a production consumer.
//!
//! For each kind: M4 records the obligation for a value it cannot know, and
//! the runtime, demanding that value, raises the kind's registered identifier
//! under `expression_demand_resolution`. `DeclaredBound` and `DeclaredPattern`
//! over declared object fields and PARAMETERs are covered in
//! `declared_constraints.rs`.
//!
//! Audit, PRETEST-01 F03.

mod common;

use common::{decimal, dynamic_document, fixture_with, integer};
use lcl_checker::ty::UnitId;
use lcl_checker::DemandKind;
use lcl_runtime::{RuntimeError, Value};
use lcl_semantics::Invocation;

/// Check that `subject` is deferred as `kind`, then demand it with `supplied`
/// and return the registered identifier its fault carries.
fn consumed(
    input: (&str, &str, &str),
    subject_type: &str,
    subject: &str,
    supplied: Value,
    kind: DemandKind,
) -> RuntimeError {
    let source = dynamic_document(&[input], &[("data.subject", subject_type, subject)]);
    let fixture = fixture_with(&source, Invocation::new().with(input.0, supplied));
    assert!(
        fixture.checked.deferred().iter().any(|o| o.kind == kind),
        "{subject}: {:?}",
        fixture.checked.deferred()
    );
    let fault = fixture
        .demand("data.subject")
        .expect_err("the supplied value violates the obligation");
    assert!(fault.demand_resolved, "{subject}: {}", fault.id);
    assert_eq!(fault.id.as_registry_str(), kind.identifier(), "{subject}");
    fault.id
}

#[test]
fn division_value() {
    consumed(
        ("input.d", "INTEGER", "1"),
        "DECIMAL",
        "1 / REF(input.d)",
        integer(0),
        DemandKind::DivisionValue,
    );
}

#[test]
fn measure_unit() {
    consumed(
        ("input.length", "MEASURE", "MEASURE(1, unit.meter)"),
        "MEASURE",
        "REF(input.length) + MEASURE(1, unit.meter)",
        Value::Quantity(decimal_of("1"), UnitId("unit.kilometer".to_string())),
        DemandKind::MeasureUnit,
    );
}

#[test]
fn nonempty_reduction() {
    consumed(
        ("input.items", "LIST[INTEGER]", "[1]"),
        "INTEGER",
        "SUM(REF(input.items))",
        Value::List(Vec::new()),
        DemandKind::NonemptyReduction,
    );
}

#[test]
fn declared_bound_of_a_constructor() {
    consumed(
        ("input.n", "INTEGER", "1"),
        "PERCENTAGE",
        "PERCENTAGE(REF(input.n))",
        integer(150),
        DemandKind::DeclaredBound,
    );
}

#[test]
fn constructor_value() {
    consumed(
        ("input.text", "STRING", "\"2026-01-01\""),
        "DATE",
        "DATE(REF(input.text))",
        Value::Text("not a date".to_string()),
        DemandKind::ConstructorValue,
    );
}

#[test]
fn constructor_value_of_a_path() {
    consumed(
        ("input.text", "STRING", "\"/srv/data\""),
        "PATH",
        "PATH(REF(input.text))",
        Value::Text("relative/data".to_string()),
        DemandKind::ConstructorValue,
    );
}

#[test]
fn set_member_order() {
    let source = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.order\n    \
                  NAME: \"Order\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
                  INPUT:\n    ID: input.lengths\n    TYPE: SET[MEASURE]\n    REQUIRED: FALSE\n    \
                  DEFAULT: [MEASURE(1, unit.meter)]\n\n\
                  GOAL:\n    ID: goal.order\n    ASSERT: TRUE\n\n\
                  SEQUENCE:\n    ID: sequence.order\n    FOR EACH length IN REF(input.lengths):\n        \
                  STEP:\n            ID: step.length\n            ACTION:\n                \
                  ID: action.length\n                OPERATION: core.inspect\n                \
                  TARGET: REF(length)\n\n\
                  SUCCESS:\n    ID: success.order\n    ALL: [TRUE]\n\n\
                  TASK:\n    ID: task.order\n    GOAL: REF(goal.order)\n    INPUT: REF(input.lengths)\n    \
                  SEQUENCE: REF(sequence.order)\n    SUCCESS: REF(success.order)\n\n\
                  EXECUTE:\n    REFERENCE: REF(task.order)\n";
    let supplied = Value::Set(vec![
        Value::Quantity(decimal_of("1"), UnitId("unit.meter".to_string())),
        Value::Quantity(decimal_of("1"), UnitId("unit.second".to_string())),
    ]);
    let fixture = fixture_with(source, Invocation::new().with("input.lengths", supplied));
    assert!(fixture
        .checked
        .deferred()
        .iter()
        .any(|o| o.kind == DemandKind::SetMemberOrder));
    let mut host = lcl_runtime::MockHost::new();
    let execution = common::run(&fixture, &mut host).expect("planned");
    let errors: Vec<RuntimeError> = execution.diagnostics().iter().map(|d| d.id).collect();
    assert!(errors.contains(&RuntimeError::TypeMismatch), "{errors:?}");
}

fn decimal_of(text: &str) -> lcl_checker::numeric::Decimal {
    match decimal(text) {
        Value::Decimal(exact) => exact,
        _ => unreachable!(),
    }
}
