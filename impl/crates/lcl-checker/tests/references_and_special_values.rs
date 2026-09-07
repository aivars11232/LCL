//! Phase E: reference contexts, special values, and output reads.

mod common;

use common::{check, ids, HEADER};
use lcl_checker::Outcome;

/// A `kind.task` document with the given blocks spliced in.
fn task(blocks: &str) -> String {
    format!(
        concat!(
            "LCL:\n    VERSION: \"0.1.0\"\n\n",
            "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
            "INPUT:\n    ID: input.value\n    TYPE: INTEGER\n    VALUE: 4\n\n",
            "OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n",
            "{}",
            "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
            "ACTION:\n    ID: action.one\n    OPERATION: core.calculate\n    TARGET: REF(input.value)\n",
            "    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"REF(input.value) * 2\"\n",
            "    OUTPUT: REF(output.value)\n\n",
            "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
            "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
            "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.one)\n"
        ),
        blocks
    )
}

#[test]
fn a_value_context_reads_the_declarations_static_type() {
    // "REF reads exactly one bound value of INPUT, DATA, CONTEXT, MEMORY,
    // STATE, OUTPUT, DEFINE kind.constant, or a loop-local binding."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.count\n    TYPE: INTEGER\n    VALUE: 2\n\nDATA:\n    ID: data.doubled\n    TYPE: INTEGER\n    VALUE: REF(data.count) * 2\n"
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    // The read has the declaration's exact type, so a wrong receiving type is a
    // mismatch rather than an unchecked pass.
    let wrong = format!(
        "{HEADER}\nDATA:\n    ID: data.count\n    TYPE: INTEGER\n    VALUE: 2\n\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: REF(data.count)\n"
    );
    assert_eq!(ids(&check(&wrong)), vec!["error.type.mismatch"]);
}

#[test]
fn an_identity_context_retains_the_reference_rather_than_reading_it() {
    // "an explicitly REFERENCE-typed value slot … retains the referenced
    // identity", and "REFERENCE is a material type distinct from its referent's
    // value type".
    let source = format!(
        "{HEADER}\nDEFINE:\n    ID: constant.counter\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 2\n\nDEFINE:\n    ID: constant.reference\n    KIND: kind.constant\n    TYPE: REFERENCE[REF(constant.counter)]\n    VALUE: REF(constant.counter)\n"
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    // The same REF in an ordinary value slot reads the value instead.
    let value_context = format!(
        "{HEADER}\nDEFINE:\n    ID: constant.counter\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 2\n\nDEFINE:\n    ID: constant.doubled\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: REF(constant.counter) * 2\n"
    );
    assert_eq!(check(&value_context).outcome(), Outcome::Checked);
}

#[test]
fn an_output_reference_is_typed_without_being_demanded() {
    // "Static checking resolves a reference's declaration and static type; it
    // does not read an OUTPUT before its producer binds it. … An unbound OUTPUT
    // yields MISSING only at an actual bound-value read; its existence as a
    // reference during static checking does not emit error.required.missing."
    let source = task("");
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Checked);
    assert!(
        checked
            .diagnostics()
            .iter()
            .all(|d| d.id.to_string() != "error.required.missing"),
        "a static OUTPUT reference is not a demanded read"
    );
}

#[test]
fn a_validate_or_verify_reference_reads_a_boolean_check_result() {
    // "REF to VALIDATE or VERIFY reads its Boolean check result."
    let source = task("");
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));
}

#[test]
fn a_loop_local_binding_takes_the_collections_member_type() {
    // "Loop-local identifiers exist only in their FOR EACH body."
    let source = concat!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n",
        "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
        "INPUT:\n    ID: input.values\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2]\n\n",
        "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
        "SEQUENCE:\n    ID: sequence.one\n    MODE: mode.sequential\n",
        "    FOR EACH item IN REF(input.values):\n",
        "        STEP:\n            ID: step.one\n            ACTION:\n                ID: action.one\n                OPERATION: core.return\n                TARGET: REF(item)\n\n",
        "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
        "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.values)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let checked = check(source);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));
}

#[test]
fn direct_iteration_of_an_unordered_set_is_rejected_before_it_begins() {
    // "Direct FOR EACH over such a SET produces error.type.mismatch before that
    // iteration begins or produces effects."
    let unordered = concat!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n",
        "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
        "INPUT:\n    ID: input.flags\n    TYPE: SET[BOOLEAN]\n    VALUE: [TRUE, FALSE]\n\n",
        "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
        "SEQUENCE:\n    ID: sequence.one\n    MODE: mode.sequential\n",
        "    FOR EACH flag IN REF(input.flags):\n",
        "        STEP:\n            ID: step.one\n            ACTION:\n                ID: action.one\n                OPERATION: core.return\n                TARGET: REF(flag)\n\n",
        "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
        "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.flags)\n    SEQUENCE: REF(sequence.one)\n    SUCCESS: REF(success.one)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    assert_eq!(ids(&check(unordered)), vec!["error.type.mismatch"]);

    // An ordered member type iterates directly; whether its *actual* members are
    // mutually order-compatible is a demanded question, recorded as one.
    let ordered = unordered
        .replace("SET[BOOLEAN]", "SET[INTEGER]")
        .replace("[TRUE, FALSE]", "[1, 2]");
    let checked = check(&ordered);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));
    assert!(
        checked
            .deferred()
            .iter()
            .any(|o| o.kind == lcl_checker::DemandKind::SetMemberOrder),
        "the actual-member order check is deferred to demand"
    );
}

#[test]
fn default_admits_the_missing_it_replaces_but_not_unknown() {
    // "MISSING … may appear literally only in an equality/inequality test,
    // DEFAULT, ASSUME condition, handler condition, or conformance case", and
    // "DEFAULT replaces MISSING only. DEFAULT never replaces NULL or UNKNOWN."
    let with = |default: &str| {
        format!(
            "{HEADER}\nINPUT:\n    ID: input.rate\n    TYPE: PERCENTAGE\n    REQUIRED: FALSE\n    DEFAULT: {default}\n"
        )
        .replace("KIND: kind.data", "KIND: kind.task")
    };
    // A `kind.task` document needs its task structure, so this uses kind.data
    // with a DATA block instead for the type-level assertion.
    let data = |default: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: type.rate\n    KIND: kind.type\n    BASE: PERCENTAGE\n\nDATA:\n    ID: data.rate\n    TYPE: REF(type.rate)\n    VALUE: PERCENTAGE(10)\n\nDEFINE:\n    ID: constant.rate\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: {default}\n"
        )
    };
    let _ = with;
    assert_eq!(ids(&check(&data("1"))), Vec::<String>::new());
    assert_eq!(ids(&check(&data("UNKNOWN"))), vec!["error.value.unknown"]);
    assert_eq!(ids(&check(&data("MISSING"))), vec!["error.type.mismatch"]);
}

#[test]
fn a_declaration_property_reads_the_declared_field_before_any_value() {
    // "A reserved uppercase property immediately following REF … reads the
    // declaration's registered field before reading its bound value.
    // REF(output.copy).TARGET therefore reads the declared TARGET even before
    // OUTPUT has a result."
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 2\n\nDATA:\n    ID: data.two\n    TYPE: INTEGER\n    VALUE: REF(data.one).VALUE\n"
    );
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));

    // "An unregistered declared field … uses error.operator.operand."
    let unregistered = format!(
        "{HEADER}\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: 2\n\nDATA:\n    ID: data.two\n    TYPE: INTEGER\n    VALUE: REF(data.one).LIMIT\n"
    );
    assert_eq!(ids(&check(&unregistered)), vec!["error.operator.operand"]);
}

#[test]
fn null_is_a_type_in_a_type_slot_and_a_value_elsewhere() {
    // "NULL denotes the NULL type in a type-required field or type argument and
    // denotes the material NULL value elsewhere."
    let source = format!(
        "{HEADER}\nDEFINE:\n    ID: constant.null\n    KIND: kind.constant\n    TYPE: NULL\n    VALUE: NULL\n"
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    // "It may be stored … only where the declared type is NULL."
    let wrong = format!("{HEADER}\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: NULL\n");
    assert_eq!(ids(&check(&wrong)), vec!["error.type.mismatch"]);
}
