//! Demand-time evaluation of a declared value that preflight left undecided.
//!
//! `01_FOUNDATION/03`: "Value evaluation is separate and occurs only when the
//! containing reachable declaration demands the value." Preflight does not fold
//! every expression (`/` is "M6 owns at demand"), so a declared `VALUE` or
//! `DEFAULT` it could not decide must be evaluated when a reader demands it,
//! not frozen as UNKNOWN.
//!
//! Regressions, PRETEST-01 F02.

mod common;

use common::{decimal, dynamic_document, execute, fixture, fixture_with};
use lcl_runtime::RuntimeError;
use lcl_semantics::Invocation;

const HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.demand\n    \
                      NAME: \"Demand fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n";

#[test]
fn a_declared_quotient_is_evaluated_when_a_reference_demands_it() {
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.q\n    TYPE: DECIMAL\n    VALUE: 3 / 2\n\n\
         DATA:\n    ID: data.subject\n    TYPE: DECIMAL\n    VALUE: REF(data.q)\n"
    );
    assert_eq!(fixture(&source).demand("data.subject"), Ok(decimal("1.5")));
}

#[test]
fn a_constant_quotient_is_evaluated_when_a_reference_demands_it() {
    let source = format!(
        "{HEADER}\nDEFINE:\n    ID: const.half\n    KIND: kind.constant\n    TYPE: DECIMAL\n    VALUE: 1 / 2\n\n\
         DATA:\n    ID: data.subject\n    TYPE: DECIMAL\n    VALUE: REF(const.half)\n"
    );
    assert_eq!(fixture(&source).demand("data.subject"), Ok(decimal("0.5")));
}

#[test]
fn an_undecided_default_is_evaluated_when_a_reference_demands_it() {
    let source = dynamic_document(
        &[("input.d", "DECIMAL", "3 / 2")],
        &[("data.subject", "DECIMAL", "REF(input.d)")],
    );
    assert_eq!(
        fixture_with(&source, Invocation::new()).demand("data.subject"),
        Ok(decimal("1.5"))
    );
}

#[test]
fn a_demanded_declaration_fault_keeps_its_registered_identifier() {
    let source = dynamic_document(
        &[("input.den", "INTEGER", "1")],
        &[
            ("data.q", "DECIMAL", "1 / REF(input.den)"),
            ("data.subject", "DECIMAL", "REF(data.q)"),
        ],
    );
    let invocation = Invocation::new().with("input.den", common::integer(0));
    let fault = fixture_with(&source, invocation)
        .demand("data.subject")
        .expect_err("the demanded quotient has a zero denominator");
    assert_eq!(fault.id, RuntimeError::NumericDivisionByZero);
    assert!(fault.demand_resolved);
}

fn operation_document(target: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.target\n    \
         NAME: \"Target fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
         INPUT:\n    ID: input.v\n    TYPE: INTEGER\n    VALUE: 7\n\n\
         DATA:\n    ID: data.q\n    TYPE: DECIMAL\n    VALUE: 1 / (REF(input.v) - 7)\n\n\
         OUTPUT:\n    ID: output.copy\n    TYPE: DECIMAL\n    FORMAT: format.plain_text\n\n\
         GOAL:\n    ID: goal.copy\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.copy\n    OPERATION: core.return\n    TARGET: {target}\n    \
         OUTPUT: REF(output.copy)\n\n\
         SUCCESS:\n    ID: success.copy\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.copy\n    GOAL: REF(goal.copy)\n    INPUT: REF(input.v)\n    \
         ACTION: REF(action.copy)\n    OUTPUT: REF(output.copy)\n    SUCCESS: REF(success.copy)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.copy)\n"
    )
}

#[test]
fn an_operation_target_demands_its_declaration_before_dispatch() {
    let (execution, _host) = execute(&operation_document("REF(data.q)"));
    let errors: Vec<RuntimeError> = execution.diagnostics().iter().map(|d| d.id).collect();
    assert!(
        errors.contains(&RuntimeError::NumericDivisionByZero),
        "{errors:?}"
    );
}

/// The same faulting declaration, never demanded, raises nothing.
#[test]
fn an_undemanded_declaration_is_never_evaluated() {
    let (execution, _host) = execute(&operation_document("REF(input.v)"));
    let errors: Vec<RuntimeError> = execution.diagnostics().iter().map(|d| d.id).collect();
    assert!(
        !errors.contains(&RuntimeError::NumericDivisionByZero),
        "{errors:?}"
    );
}
