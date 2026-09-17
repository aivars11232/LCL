//! PRETEST-02 F06: which operands a GLOB `MATCHES` consumes.
//!
//! `types_v0.1.0.json#/pattern_profiles/GLOB/input`: "A STRING operand denotes
//! either the empty relative path or nonempty slash-separated segments, with no
//! empty, . or .. segment and no leading or trailing slash. A PATH operand
//! requires an explicit WORKSPACE root retained by that value or its declared
//! context and is compared using its normalized relative segment sequence. An
//! input that cannot supply this form uses error.operator.operand; no root is
//! inferred."

mod common;

use lcl_runtime::diagnostic::RuntimeError;
use lcl_runtime::Value;

const WORKSPACE: &str =
    "\nWORKSPACE:\n    ID: workspace.case\n    PATH: PATH(\"/case\")\n    MODE: mode.read_only\n";

fn demand(expression: &str) -> lcl_runtime::Demand {
    let source = format!(
        "{}{WORKSPACE}",
        common::dynamic_document(&[], &[("data.subject", "BOOLEAN", expression)])
    );
    common::fixture(&source).demand("data.subject")
}

fn operand_fault(expression: &str) {
    match demand(expression) {
        Err(fault) => assert_eq!(fault.id, RuntimeError::OperatorOperand, "{expression}"),
        Ok(value) => panic!("{expression} must be error.operator.operand, not {value}"),
    }
}

#[test]
fn a_workspace_path_matches_by_its_retained_relative_segments() {
    for (expression, expected) in [
        (
            r#"PATH(REF(workspace.case), "src/a.py") MATCHES GLOB("src/*.py")"#,
            true,
        ),
        (
            r#"PATH(REF(workspace.case), "./src//a.py") MATCHES GLOB("src/*.py")"#,
            true,
        ),
        (
            r#"PATH(REF(workspace.case), "lib/a.py") MATCHES GLOB("src/*.py")"#,
            false,
        ),
    ] {
        assert_eq!(
            demand(expression).ok(),
            Some(Value::Boolean(expected)),
            "{expression}"
        );
    }
}

#[test]
fn an_absolute_path_is_not_reverse_inferred_into_a_relative_subject() {
    // The absolute text lies under the WORKSPACE root, and still has no
    // retained WORKSPACE identity.
    operand_fault(r#"PATH("/case/src/a.py") MATCHES GLOB("**")"#);
    operand_fault(r#"PATH("/case/src/a.py") MATCHES GLOB("case/src/*.py")"#);
}

#[test]
fn a_string_outside_the_relative_segment_form_is_an_operand_defect() {
    operand_fault(r#""/src/a.py" MATCHES GLOB("**")"#);
    operand_fault(r#""src/" MATCHES GLOB("**")"#);
    operand_fault(r#""a//b" MATCHES GLOB("a/*/b")"#);
    operand_fault(r#""a/./b" MATCHES GLOB("a/*/b")"#);
    operand_fault(r#""a/../b" MATCHES GLOB("a/*/b")"#);
}

#[test]
fn string_glob_matching_stays_string_matching() {
    assert_eq!(
        demand(r#""" MATCHES GLOB("**")"#).ok(),
        Some(Value::Boolean(true))
    );
    assert_eq!(
        demand(r#""src/a/b.py" MATCHES GLOB("src/**/*.py")"#).ok(),
        Some(Value::Boolean(true))
    );
    assert_eq!(
        demand(r#""a/b/c" MATCHES GLOB("a/*")"#).ok(),
        Some(Value::Boolean(false))
    );
}

#[test]
fn an_unknown_subject_yields_unknown() {
    // "any UNKNOWN operand yields UNKNOWN for a ... pattern match"
    let demanded = common::eval_dynamic(
        &[("input.name", "STRING", "\"a\"")],
        "BOOLEAN",
        r#"(REF(input.name) MATCHES GLOB("**")) == UNKNOWN"#,
        &[("input.name", Value::Unknown)],
    );
    assert_eq!(demanded.ok(), Some(Value::Boolean(true)));
}
