//! Output instance selection at the canonical static resolution stage.
mod common;

use common::{ids, resolve};

fn source(consumer: &str, extra: &str) -> String {
    format!(
        r#"LCL:
    VERSION: "0.1.0"

SPECIFICATION:
    ID: example.instances
    NAME: "Output instances"
    VERSION: "1.0.0"
    KIND: kind.task

INPUT:
    ID: input.members
    TYPE: LIST[INTEGER]
    VALUE: [1, 2]

OUTPUT:
    ID: output.item
    TYPE: INTEGER
    FORMAT: format.plain_text
    TARGET: PATH("/case/item.txt")

SEQUENCE:
    ID: sequence.case
    FOR EACH item IN REF(input.members):
        STEP:
            ID: step.producer
            ACTION:
                ID: action.producer
                OPERATION: core.return
                TARGET: REF(item)
                OUTPUT: REF(output.item)
{consumer}
{extra}
EXECUTE:
    REFERENCE: REF(sequence.case)
"#
    )
}

fn rejects(source: &str) {
    let resolved = resolve(source);
    assert_eq!(ids(&resolved), ["error.reference.unresolved"], "{source}");
    assert!(resolved.diagnostics().iter().all(|d| d.detail.as_deref()
        .is_some_and(|s| s.contains("loop-local producer"))));
}

fn accepts(source: &str) {
    let resolved = resolve(source);
    assert!(resolved.diagnostics().is_empty(), "{:?}\n{source}", ids(&resolved));
}

#[test]
fn outside_assertion_and_data_values_cannot_select_an_iteration() {
    for extra in [
        "VERIFY:\n    ID: verify.outside\n    ASSERT: REF(output.item) == 1\n",
        "DATA:\n    ID: data.outside\n    TYPE: INTEGER\n    VALUE: REF(output.item)\n",
        "INPUT:\n    ID: input.outside\n    TYPE: INTEGER\n    DEFAULT: REF(output.item)\n",
    ] { rejects(&source("", extra)); }
}

#[test]
fn metadata_and_explicit_reference_values_do_not_select_an_iteration() {
    for extra in [
        "VERIFY:\n    ID: verify.metadata\n    ASSERT: REF(output.item).TARGET == PATH(\"/case/item.txt\")\n",
        "DATA:\n    ID: data.identity\n    TYPE: REFERENCE[REF(output.item)]\n    VALUE: REF(output.item)\n",
        "DATA:\n    ID: data.identities\n    TYPE: LIST[REFERENCE[REF(output.item)]]\n    VALUE: [REF(output.item)]\n",
    ] { accepts(&source("", extra)); }
}

#[test]
fn an_operator_inside_an_identity_slot_still_reads_its_operands() {
    rejects(&source("", "ACTION:\n    ID: action.outside\n    OPERATION: core.inspect\n    TARGET: [REF(output.item) + 1]\n"));
}

#[test]
fn nested_parameter_value_cannot_escape_the_producer_loop() {
    rejects(&source("", "ACTION:\n    ID: action.outside\n    OPERATION: core.inspect\n    TARGET: REF(input.members)\n    PARAMETER:\n        NAME: depth\n        TYPE: INTEGER\n        REQUIRED: FALSE\n        VALUE: REF(output.item)\n"));
}

#[test]
fn execute_exports_cannot_select_a_loop_local_output() {
    rejects(&source("", "").replace(
        "    REFERENCE: REF(sequence.case)\n",
        "    REFERENCE: REF(sequence.case)\n    OUTPUT: REF(output.item)\n",
    ));
}

#[test]
fn conditions_and_nested_loop_collections_are_value_contexts() {
    let outside = "    IF (REF(output.item) == 1) THEN:\n        STEP:\n            ID: step.outside\n            ACTION:\n                ID: action.outside\n                OPERATION: core.return\n                TARGET: 1\n";
    rejects(&source(outside, ""));
    let inside = outside.lines().map(|line| format!("    {line}\n")).collect::<String>();
    accepts(&source(&inside, ""));
}

#[test]
fn a_later_action_in_the_same_iteration_can_read_the_value() {
    accepts(&source("        STEP:\n            ID: step.consumer\n            ACTION:\n                ID: action.consumer\n                OPERATION: core.return\n                WHEN: REF(output.item) == REF(item)\n                TARGET: REF(output.item)\n", ""));
}
