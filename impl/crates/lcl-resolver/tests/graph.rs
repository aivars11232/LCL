//! The structural candidate graph: membership, child order and templates.

mod common;

use common::{assert_stages_clean, canonical_root, ids, resolve};
use lcl_resolver::{NodeKind, Resolved};

fn task(body: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n{body}"
    )
}

/// The scaffolding every `kind.task` document needs, so a test can concentrate
/// on the shape under test.
const SUPPORT: &str = concat!(
    "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n",
    "VERIFY:\n    ID: verify.v\n    ASSERT: TRUE\n\n",
    "SUCCESS:\n    ID: success.s\n    ALL: [REF(verify.v)]\n\n",
);

/// Every activated declaration, in graph order.
fn activated(resolved: &Resolved) -> Vec<String> {
    resolved
        .graph()
        .nodes()
        .iter()
        .filter_map(|n| n.declaration)
        .filter_map(|i| resolved.declarations().get(i))
        .map(|d| d.id.local.clone())
        .collect()
}

#[test]
fn a_document_without_execute_has_no_graph() {
    let source = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: lib.doc\n    NAME: \"L\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 1\n";
    assert_stages_clean(source);
    let resolved = resolve(source);
    assert!(resolved.graph().is_empty());
    assert_eq!(ids(&resolved), Vec::<String>::new());
}

#[test]
fn execute_identifies_one_root() {
    let source = task(&format!(
        "{SUPPORT}ACTION:\n    ID: action.a\n    OPERATION: core.inspect\n\nTASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n\nEXECUTE:\n    REFERENCE: REF(task.t)\n"
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let root = resolved.graph().root().expect("a root");
    assert_eq!(root.kind, NodeKind::Root);
    assert_eq!(root.block, "TASK");
    assert_eq!(activated(&resolved), ["task.t", "action.a"]);
}

#[test]
fn task_children_follow_source_field_order_then_list_order() {
    // CLOSURE-053: "TASK has ACTION field then PHASE field, each with a
    // reference LIST" -> "Children expand in that field order and
    // left-to-right LIST order."
    let source = task(&format!(
        "{SUPPORT}{}",
        concat!(
            "ACTION:\n    ID: action.one\n    OPERATION: core.inspect\n\n",
            "ACTION:\n    ID: action.two\n    OPERATION: core.inspect\n\n",
            "SEQUENCE:\n    ID: sequence.one\n",
            "    STEP:\n        ID: step.one\n",
            "        ACTION:\n            ID: action.instep\n            OPERATION: core.inspect\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n",
            "    ACTION: [REF(action.one), REF(action.two)]\n",
            "    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        )
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    // The TASK writes ACTION before SEQUENCE, and the ACTION list expands left
    // to right; the SEQUENCE's own children follow it.
    assert_eq!(
        activated(&resolved),
        [
            "task.t",
            "action.one",
            "action.two",
            "sequence.one",
            "step.one",
            "action.instep"
        ]
    );
}

#[test]
fn ordinary_references_never_activate_a_producer() {
    // "Ordinary REF, TARGET, check references, OUTPUT reads, BEFORE and AFTER
    // never activate a producer."
    let source = task(&format!(
        "{SUPPORT}{}",
        concat!(
            "OUTPUT:\n    ID: output.o\n    TYPE: INTEGER\n    FORMAT: format.json\n\n",
            "ACTION:\n    ID: action.other\n    OPERATION: core.inspect\n\n",
            "ACTION:\n    ID: action.a\n    OPERATION: core.inspect\n",
            "    TARGET: REF(output.o)\n    OUTPUT: REF(output.o)\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n",
            "    OUTPUT: REF(output.o)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        )
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let names = activated(&resolved);
    // TARGET and OUTPUT reads add nothing; action.other is never referenced by
    // an activating field, so it is not in the graph at all.
    assert_eq!(names, ["task.t", "action.a"]);
    assert!(!names.contains(&"output.o".to_string()));
    assert!(!names.contains(&"action.other".to_string()));
    // ... though every one of those references is still bound.
    assert!(resolved.bindings().iter().any(|b| b.text == "output.o"));
}

#[test]
fn both_branches_of_a_conditional_are_templates() {
    let source = task(&format!(
        "{SUPPORT}{}",
        concat!(
            "SEQUENCE:\n    ID: sequence.s\n",
            "    IF (TRUE) THEN:\n",
            "        STEP:\n            ID: step.then\n",
            "            ACTION:\n                ID: action.then\n",
            "                OPERATION: core.inspect\n",
            "    ELSE:\n",
            "        STEP:\n            ID: step.else\n",
            "            ACTION:\n                ID: action.else\n",
            "                OPERATION: core.inspect\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n",
            "    SEQUENCE: REF(sequence.s)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        )
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let kinds: Vec<NodeKind> = resolved.graph().nodes().iter().map(|n| n.kind).collect();
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == NodeKind::BranchTemplate)
            .count(),
        2,
        "both arms are templates"
    );
    let names = activated(&resolved);
    for expected in ["step.then", "action.then", "step.else", "action.else"] {
        assert!(
            names.contains(&expected.to_string()),
            "{expected} in {names:?}"
        );
    }
}

#[test]
fn a_loop_body_is_one_template_and_is_not_unrolled() {
    let source = task(&format!(
        "{SUPPORT}{}",
        concat!(
            "INPUT:\n    ID: input.files\n    TYPE: LIST[PATH]\n",
            "    VALUE: [PATH(\"/a\"), PATH(\"/b\"), PATH(\"/c\")]\n\n",
            "SEQUENCE:\n    ID: sequence.s\n",
            "    FOR EACH file IN REF(input.files):\n",
            "        STEP:\n            ID: step.one\n",
            "            ACTION:\n                ID: action.one\n",
            "                OPERATION: core.inspect\n                TARGET: REF(file)\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n",
            "    SEQUENCE: REF(sequence.s)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        )
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let loops = resolved
        .graph()
        .nodes()
        .iter()
        .filter(|n| n.kind == NodeKind::LoopTemplate)
        .count();
    assert_eq!(loops, 1, "one template, not one node per member");
    let names = activated(&resolved);
    assert_eq!(names.iter().filter(|n| *n == "step.one").count(), 1);
}

#[test]
fn a_structural_cycle_is_rejected() {
    // "Structural cycles use error.reference.cycle."
    let source = task(&format!(
        "{SUPPORT}{}",
        concat!(
            "SEQUENCE:\n    ID: sequence.a\n",
            "    STEP:\n        ID: step.a\n        SEQUENCE: REF(sequence.b)\n\n",
            "SEQUENCE:\n    ID: sequence.b\n",
            "    STEP:\n        ID: step.b\n        SEQUENCE: REF(sequence.a)\n\n",
            "TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n",
            "    SEQUENCE: REF(sequence.a)\n    SUCCESS: REF(success.s)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.t)\n",
        )
    ));
    assert_stages_clean(&source);
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), ["error.reference.cycle"]);
    assert_eq!(resolved.terminal_status(), Some("status.invalid"));
}

#[test]
fn the_canonical_iteration_example_builds_its_graph() {
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/05_CONDITION_AND_ITERATION.lcl"),
    )
    .expect("example present");
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let names = activated(&resolved);
    assert_eq!(
        names,
        [
            "task.inspect",
            "sequence.inspect",
            "step.inspect_file",
            "action.inspect_file"
        ]
    );
    assert_eq!(
        resolved
            .graph()
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::LoopTemplate)
            .count(),
        1
    );
}

#[test]
fn the_canonical_coding_example_builds_its_graph_in_order() {
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl"),
    )
    .expect("example present");
    let resolved = resolve(&source);
    assert_eq!(ids(&resolved), Vec::<String>::new());
    let names = activated(&resolved);
    // TASK -> SEQUENCE -> three STEPs, each with its referenced ACTION, in
    // lexical then reference order.
    assert_eq!(
        names.first().map(String::as_str),
        Some("task.implementation")
    );
    let steps: Vec<&String> = names.iter().filter(|n| n.starts_with("step.")).collect();
    assert_eq!(steps.len(), 3, "three steps, in lexical order: {names:?}");
}

#[test]
fn graph_construction_is_identical_across_repeated_runs() {
    let source = std::fs::read_to_string(
        canonical_root().join("08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl"),
    )
    .expect("example present");
    let a = resolve(&source);
    let b = resolve(&source);
    assert_eq!(activated(&a), activated(&b));
    assert_eq!(a.graph().len(), b.graph().len());
}
