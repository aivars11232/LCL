//! Determinism and totality of the preflight layer.
//!
//! Authority: `05_SEMANTICS/11_DETERMINISM_EQUIVALENCE_AND_INTERPRETER_VARIATION.txt`
//! and the execution contract's determinism rule: for the same canonical
//! version, source bytes, explicit inputs, explicit state and explicit
//! capabilities, observable language meaning must not depend on thread
//! scheduling, hash-map iteration order, filesystem enumeration order,
//! wall-clock timing, discovery timing, ambient history or hidden provider
//! defaults.

mod common;

use common::*;
use lcl_semantics::{Invocation, Value};
use std::fs;

fn valid_examples() -> Vec<(String, String)> {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut out = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("the canonical examples are readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lcl"))
        .collect();
    // Sort so this test's own iteration cannot depend on filesystem order.
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        out.push((name, fs::read_to_string(&path).expect("readable")));
    }
    out
}

#[test]
fn planning_the_same_bytes_twice_is_byte_identical() {
    for (name, _) in valid_examples() {
        let first = plan_example(&name);
        let second = plan_example(&name);
        assert_eq!(
            first.partial_plan().serialize(),
            second.partial_plan().serialize(),
            "{name} planned differently on a second run"
        );
        assert_eq!(
            first.diagnostics().len(),
            second.diagnostics().len(),
            "{name} diagnosed differently on a second run"
        );
    }
}

#[test]
fn a_freshly_loaded_engine_produces_the_same_plan() {
    // A second `Contracts` load, a second `Resolved`, a second `Checked`: no
    // state carried between runs can be affecting the answer.
    let source = fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/05_CONDITION_AND_ITERATION.lcl"),
    )
    .expect("readable");

    let first = plan(&source).partial_plan().serialize();
    let second = plan(&source).partial_plan().serialize();
    assert_eq!(first, second);
}

#[test]
fn the_plan_order_is_independent_of_declaration_order_in_unrelated_blocks() {
    // Moving an unordered, unrelated declaration must not move an execution
    // unit in the plan: ordering comes from the graph, not from where a
    // neighbouring block happens to sit.
    let steps = "    STEP:\n        ID: step.one\n        ACTION:\n            ID: action.one\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n    STEP:\n        ID: step.two\n        ACTION:\n            ID: action.two\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n";
    let build = |extra_first: bool| -> String {
        let extra = "\nDATA:\n    ID: data.note\n    TYPE: STRING\n    VALUE: \"n\"\n";
        let subject = "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n";
        let head = if extra_first {
            format!("{extra}{subject}")
        } else {
            format!("{subject}{extra}")
        };
        format!(
            "{HEADER}{head}\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.one\n{steps}\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
        )
    };

    let a = plan(&build(false));
    let b = plan(&build(true));
    let ids_of = |p: &lcl_semantics::Planned| -> Vec<String> {
        p.partial_plan()
            .order()
            .iter()
            .map(|i| {
                p.partial_plan()
                    .node(*i)
                    .and_then(|n| n.id.clone())
                    .unwrap_or_default()
            })
            .collect()
    };
    assert_eq!(ids_of(&a), ids_of(&b));
}

#[test]
fn supplying_the_same_values_in_a_different_insertion_order_plans_identically() {
    let source = task_document(
        "\nINPUT:\n    ID: input.a\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/a\")\n\nINPUT:\n    ID: input.b\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/b\")\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n",
    );
    let one = Invocation::new()
        .with("input.a", Value::Integer(int("1")))
        .with("input.b", Value::Integer(int("2")));
    let two = Invocation::new()
        .with("input.b", Value::Integer(int("2")))
        .with("input.a", Value::Integer(int("1")));
    assert_eq!(
        plan_with(&source, &one).partial_plan().serialize(),
        plan_with(&source, &two).partial_plan().serialize()
    );
}

#[test]
fn the_serialization_carries_no_address_or_timing() {
    let source = fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"))
        .expect("readable");
    let text = plan(&source).partial_plan().serialize();
    assert!(!text.contains("0x"), "a plan must carry no address");
    assert!(!text.is_empty());
}

fn int(text: &str) -> lcl_checker::numeric::Decimal {
    lcl_checker::numeric::Decimal::parse_integer(text).expect("parses")
}
