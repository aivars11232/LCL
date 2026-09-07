//! Totality: malformed and hostile input must not panic.
//!
//! Execution contract, testing truthfulness: "Malformed/untrusted source must
//! not panic." A preflight layer that panics is worse than one that rejects,
//! because a panic is not a diagnostic and carries no stage, status or locus.

mod common;

use common::*;
use lcl_resolver::MemoryProvider;
use lcl_semantics::Invocation;

/// Push one source through every stage, asserting only that nothing panics.
fn survives(source: &str) {
    let provider = MemoryProvider::new();
    let Ok(resolved) = resolver().resolve(&unit("root.lcl", source), &provider) else {
        return;
    };
    let Ok(checked) = checker().check(&resolved) else {
        return;
    };
    let _ = preflight().plan(&checked, &resolved, &Invocation::new());
}

#[test]
fn empty_and_tiny_inputs_are_total() {
    for source in ["", "\n", "L", "LCL:\n", "LCL:\n    VERSION: \"0.1.0\"\n"] {
        survives(source);
    }
}

#[test]
fn every_prefix_of_a_valid_example_is_total() {
    // Truncation at an arbitrary byte is the cheapest way to reach states no
    // hand-written fixture would.
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl"),
    )
    .expect("readable");
    for end in 0..source.len() {
        if source.is_char_boundary(end) {
            survives(&source[..end]);
        }
    }
}

#[test]
fn deeply_nested_expressions_are_total_in_this_layer() {
    // This layer's evaluator counts depth rather than trusting the stack, so it
    // is bounded by its own budget and not by the machine's.
    //
    // The depth here is 120, not an arbitrarily large number, because **M2's
    // recursive-descent expression parser overflows the default 2 MiB test
    // stack at roughly 180 nested groups** — measured, on this machine, between
    // 160 (parses) and 180 (overflows). That is a pre-existing limitation of an
    // earlier, already-closed milestone; it is reported rather than repaired
    // here, because repairing another layer's parser is outside this task. What
    // this test can honestly assert is that everything M2 hands on, this layer
    // handles without panicking.
    let depth = 120;
    let expression = format!("{}TRUE{}", "(".repeat(depth), ")".repeat(depth));
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: {expression}\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n"
    ));
    survives(&source);
}

#[test]
fn the_evaluators_own_depth_budget_is_bounded_well_below_the_stack() {
    // A nesting depth this layer's evaluator refuses is answered with "not
    // decidable here", never with a panic and never with a guess.
    let depth = 150;
    let expression = format!("{}TRUE{}", "(".repeat(depth), ")".repeat(depth));
    let source = task_document(&format!(
        "\nVALIDATE:\n    ID: validate.one\n    ASSERT: {expression}\n    REQUIRED: FALSE\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n"
    ));
    let (resolved, checked) = check(&source);
    let planned = preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .expect("static stage succeeded");
    let check = planned
        .partial_plan()
        .checks()
        .iter()
        .find(|c| c.id == "validate.one")
        .expect("selected");
    assert_eq!(
        check.outcome, None,
        "beyond its budget the evaluator declines to decide rather than panicking"
    );
}

#[test]
fn a_long_chain_of_rules_is_total() {
    let mut body = String::from("\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n");
    for i in 0..200 {
        body.push_str(&format!(
            "\nREQUIRE:\n    ID: rule.r{i}\n    ASSERT: REF(data.flag) == TRUE\n"
        ));
    }
    survives(&task_document(&body));
}

#[test]
fn a_long_chain_of_ordered_steps_is_total() {
    let mut steps = String::new();
    for i in 0..100 {
        steps.push_str(&format!(
            "    STEP:\n        ID: step.s{i}\n        ACTION:\n            ID: action.a{i}\n            OPERATION: core.inspect\n            TARGET: REF(data.subject)\n"
        ));
    }
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.one\n{steps}\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    survives(&source);
}

#[test]
fn planning_is_total_over_arbitrary_supplied_values() {
    use lcl_semantics::Value;
    let source = task_document(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in\")\n\nVALIDATE:\n    ID: validate.one\n    ASSERT: REF(input.one) > 0\n\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n",
    );
    let (resolved, checked) = check(&source);
    for value in [
        Value::Missing,
        Value::Unknown,
        Value::Null,
        Value::Boolean(true),
        Value::Text(String::new()),
        Value::List(Vec::new()),
        Value::Set(Vec::new()),
        Value::Identifier("unit.second".to_string()),
        Value::Reference("input.one".to_string()),
    ] {
        let invocation = Invocation::new().with("input.one", value);
        let _ = preflight().plan(&checked, &resolved, &invocation);
    }
}

#[test]
fn results_are_identical_across_repeated_planning() {
    let source =
        std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID/12_SET_SORTING.lcl"))
            .expect("readable");
    let (resolved, checked) = check(&source);
    let first = preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .expect("static stage succeeded");
    for _ in 0..25 {
        let again = preflight()
            .plan(&checked, &resolved, &Invocation::new())
            .expect("static stage succeeded");
        assert_eq!(
            first.partial_plan().serialize(),
            again.partial_plan().serialize()
        );
    }
}
