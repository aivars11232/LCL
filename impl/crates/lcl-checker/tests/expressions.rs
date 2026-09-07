//! Phase C: names, arity, operand families, results, and exact numerics.

mod common;

use common::{check, contracts, data_document, ids};
use lcl_checker::Outcome;

/// A `kind.data` document whose single `DATA` value is `expression`.
fn value(ty: &str, expression: &str) -> String {
    data_document(ty, expression)
}

#[test]
fn every_registered_operator_and_function_is_reachable_by_name() {
    // The vocabulary is the registry's: nothing is accepted that it does not
    // register, and nothing it registers is unknown here.
    for row in contracts().operators() {
        assert!(!row.overloads.is_empty(), "{} has overloads", row.name);
    }
    for row in contracts().functions() {
        assert!(
            contracts().function(&row.name).is_some(),
            "{} is reachable",
            row.name
        );
    }
}

#[test]
fn an_unregistered_arity_is_rejected() {
    // "An unregistered name, arity, or operand family is invalid."
    assert_eq!(
        ids(&check(&value("INTEGER", "COUNT(\"a\", \"b\")"))),
        vec!["error.operator.operand"]
    );
    assert_eq!(
        ids(&check(&value("DECIMAL", "ROUND(1.5)"))),
        vec!["error.operator.operand"]
    );
    assert_eq!(
        check(&value("INTEGER", "COUNT(\"ab\")")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn an_unregistered_operand_family_is_rejected() {
    for expression in [
        "\"a\" + 1",
        "TRUE + 1",
        "1 * \"a\"",
        "NOT 1",
        "-\"a\"",
        "\"a\" < 1",
    ] {
        let checked = check(&value("BOOLEAN", expression));
        assert!(
            ids(&checked).contains(&"error.operator.operand".to_string()),
            "{expression} must be rejected: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn numeric_promotion_follows_the_registry() {
    assert_eq!(
        check(&value("INTEGER", "1 + 2")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("DECIMAL", "1 + 2.5")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("DECIMAL", "1.5 + 2")).outcome(),
        Outcome::Checked
    );
    // INTEGER + INTEGER is INTEGER, not DECIMAL.
    assert_eq!(
        ids(&check(&value("DECIMAL", "1 + 2"))),
        vec!["error.type.mismatch"]
    );
    // INTEGER + DECIMAL is DECIMAL, not INTEGER.
    assert_eq!(
        ids(&check(&value("INTEGER", "1 + 2.5"))),
        vec!["error.type.mismatch"]
    );
}

#[test]
fn every_scalar_division_has_static_result_type_decimal() {
    // "Every scalar division overload … has static result type DECIMAL,
    // including an integral quotient such as 4 / 2."
    assert_eq!(
        check(&value("DECIMAL", "4 / 2")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("DECIMAL", "1 / 8")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        ids(&check(&value("INTEGER", "4 / 2"))),
        vec!["error.type.mismatch"]
    );
}

#[test]
fn an_exact_quotient_must_terminate_outside_round() {
    assert_eq!(
        ids(&check(&value("DECIMAL", "1 / 3"))),
        vec!["error.numeric.non_terminating"]
    );
    assert_eq!(
        ids(&check(&value("DECIMAL", "22 / 7"))),
        vec!["error.numeric.non_terminating"]
    );
    // "When its direct first argument is a division expression, ROUND evaluates
    // the exact mathematical quotient and rounds it once."
    assert_eq!(
        check(&value("DECIMAL", "ROUND(1 / 3, 2)")).outcome(),
        Outcome::Checked
    );
    // Not the direct first argument: the quotient must terminate on its own.
    assert_eq!(
        ids(&check(&value("DECIMAL", "ROUND(1 / 3 + 1, 2)"))),
        vec!["error.numeric.non_terminating"]
    );
}

#[test]
fn a_zero_denominator_is_invalid_even_inside_round() {
    assert_eq!(
        ids(&check(&value("DECIMAL", "1 / 0"))),
        vec!["error.numeric.division_by_zero"]
    );
    assert_eq!(
        ids(&check(&value("DECIMAL", "ROUND(1 / 0, 2)"))),
        vec!["error.numeric.division_by_zero"]
    );
}

#[test]
fn measure_arithmetic_keeps_its_exact_unit() {
    // "MEASURE divided by INTEGER or DECIMAL returns MEASURE … MEASURE divided
    // by a MEASURE with the same exact UNIT returns a dimensionless DECIMAL."
    assert_eq!(
        check(&value("MEASURE", "MEASURE(9, unit.meter) / 4")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value(
            "DECIMAL",
            "MEASURE(9, unit.meter) / MEASURE(4, unit.meter)"
        ))
        .outcome(),
        Outcome::Checked
    );
    // "Different units produce error.numeric.unit_mismatch."
    assert_eq!(
        ids(&check(&value(
            "DECIMAL",
            "MEASURE(9, unit.meter) / MEASURE(4, unit.second)"
        ))),
        vec!["error.numeric.unit_mismatch"]
    );
    assert_eq!(
        ids(&check(&value(
            "MEASURE",
            "MEASURE(9, unit.meter) + MEASURE(4, unit.second)"
        ))),
        vec!["error.numeric.unit_mismatch"]
    );
    assert_eq!(
        check(&value(
            "MEASURE",
            "MEASURE(9, unit.meter) + MEASURE(4, unit.meter)"
        ))
        .outcome(),
        Outcome::Checked
    );
}

#[test]
fn equality_accepts_any_two_values_and_always_returns_boolean() {
    // "Equality accepts any two material values and the permitted special
    // sentinels. … Different material static types compare FALSE."
    for expression in [
        "1 == 1",
        "1 == \"a\"",
        "TRUE == FALSE",
        "1 == MISSING",
        "UNKNOWN == UNKNOWN",
        "1 != 2.5",
    ] {
        let checked = check(&value("BOOLEAN", expression));
        assert_eq!(
            checked.outcome(),
            Outcome::Checked,
            "{expression}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn ordered_comparison_requires_a_registered_order_domain() {
    assert_eq!(
        check(&value("BOOLEAN", "1 < 2")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("BOOLEAN", "\"a\" < \"b\"")).outcome(),
        Outcome::Checked
    );
    // "Ordered comparison of enum members is forbidden"; BOOLEAN and NULL have
    // no registered total order either.
    assert!(!check(&value("BOOLEAN", "TRUE < FALSE"))
        .diagnostics()
        .is_empty());
}

#[test]
fn both_operands_of_a_short_circuit_are_still_checked() {
    // "Every expression is statically checked, including names and operand
    // families in a branch that will not be evaluated."
    let checked = check(&value("BOOLEAN", "FALSE AND (1 + \"a\") == 1"));
    assert!(
        ids(&checked).contains(&"error.operator.operand".to_string()),
        "a skipped operand is still checked: {:?}",
        ids(&checked)
    );
    let checked = check(&value("BOOLEAN", "TRUE OR NOT 1"));
    assert!(ids(&checked).contains(&"error.operator.operand".to_string()));
}

#[test]
fn quantifiers_supply_boolean_context_and_admit_unknown_members() {
    // "ALL, ANY, and NONE supply BOOLEAN context to their immediate bracket
    // argument; its UNKNOWN members are transient observations."
    for expression in ["ALL([TRUE, FALSE])", "ANY([UNKNOWN, TRUE])", "NONE([])"] {
        let checked = check(&value("BOOLEAN", expression));
        assert_eq!(
            checked.outcome(),
            Outcome::Checked,
            "{expression}: {:?}",
            ids(&checked)
        );
    }
    // "A member of another material family emits error.operator.operand."
    assert!(!check(&value("BOOLEAN", "ALL([1, 2])"))
        .diagnostics()
        .is_empty());
}

#[test]
fn an_empty_bracket_literal_needs_one_contextual_member_type() {
    // "An empty literal without one unique contextual member type produces
    // error.type.mismatch. Accordingly an unconstrained COUNT([]) is invalid."
    assert_eq!(
        ids(&check(&value("INTEGER", "COUNT([])"))),
        vec!["error.type.mismatch"]
    );
    // An already typed empty collection is a valid COUNT argument.
    assert_eq!(
        check(&value("LIST[INTEGER]", "[]")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn a_nonempty_reduction_rejects_a_typed_empty_collection() {
    // Two different rules, each with its own identifier: an *unconstrained*
    // empty literal "has no unique member type and uses error.type.mismatch",
    // while an already *typed* empty collection is the operand defect. Only the
    // first is statically decidable, so the second is deferred to demand.
    assert_eq!(
        ids(&check(&value("INTEGER", "SUM([])"))),
        vec!["error.type.mismatch"]
    );
    assert_eq!(
        check(&value("INTEGER", "SUM([1, 2])")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn list_indexing_is_zero_based_and_set_indexing_is_not_admitted() {
    assert_eq!(
        check(&value("INTEGER", "[10, 20][0]")).outcome(),
        Outcome::Checked
    );
    // "Neither SET nor STRING supports index access."
    assert!(!check(&value("STRING", "\"abc\"[0]"))
        .diagnostics()
        .is_empty());
    // "A non-INTEGER index uses error.operator.operand."
    assert!(!check(&value("INTEGER", "[10, 20][\"a\"]"))
        .diagnostics()
        .is_empty());
}

#[test]
fn count_and_empty_accept_their_registered_families() {
    assert_eq!(
        check(&value("INTEGER", "COUNT(BYTES(8))")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("INTEGER", "COUNT([1, 2])")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("BOOLEAN", "EMPTY(\"\")")).outcome(),
        Outcome::Checked
    );
    // "NULL is not an accepted family."
    assert!(!check(&value("BOOLEAN", "EMPTY(NULL)"))
        .diagnostics()
        .is_empty());
}

#[test]
fn matches_consumes_a_string_and_a_pattern() {
    assert_eq!(
        check(&value(
            "BOOLEAN",
            "\"Alpha\" MATCHES REGEX(\"[A-Za-z]+\", \"i\")"
        ))
        .outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value(
            "BOOLEAN",
            "\"src/main.py\" MATCHES GLOB(\"src/**/*.py\")"
        ))
        .outcome(),
        Outcome::Checked
    );
    // "MATCHES consumes STRING and REGEX, or PATH/STRING and GLOB."
    assert!(!check(&value("BOOLEAN", "1 MATCHES REGEX(\"a\")"))
        .diagnostics()
        .is_empty());
}

#[test]
fn a_membership_test_binds_one_member_type() {
    assert_eq!(
        check(&value("BOOLEAN", "1 IN [1, 2]")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("BOOLEAN", "[1, 2] CONTAINS 1")).outcome(),
        Outcome::Checked
    );
    // `T, LIST[T]` binds one identical type throughout.
    assert!(!check(&value("BOOLEAN", "\"a\" IN [1, 2]"))
        .diagnostics()
        .is_empty());
}

#[test]
fn a_literal_unknown_outside_its_permitted_positions_is_rejected() {
    // "It may appear literally only in equality/inequality tests, conditions,
    // handlers, assumptions, the immediate bracket argument consumed by ALL,
    // ANY, or NONE, or conformance cases."
    assert_eq!(
        ids(&check(&value("INTEGER", "UNKNOWN"))),
        vec!["error.value.unknown"]
    );
    assert_eq!(
        ids(&check(&value("INTEGER", "1 + UNKNOWN"))),
        vec!["error.value.unknown"]
    );
    // MISSING has no storable type at a material destination.
    assert_eq!(
        ids(&check(&value("INTEGER", "MISSING"))),
        vec!["error.type.mismatch"]
    );
}

#[test]
fn a_condition_slot_requires_boolean() {
    // `TASK` requires "At least PHASE, SEQUENCE, or ACTION", so the fixture is
    // a complete minimal task with only its GOAL assertion varying.
    let task = |assertion: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: INTEGER\n    VALUE: 4\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: {}\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.calculate\n    TARGET: REF(input.value)\n",
                "    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"REF(input.value) * 2\"\n",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            assertion
        )
    };
    assert_eq!(check(&task("TRUE")).outcome(), Outcome::Checked);
    assert_eq!(
        check(&task("REF(output.value) == 8")).outcome(),
        Outcome::Checked
    );
    // "boolean_expression: Expression statically producing BOOLEAN or UNKNOWN."
    assert_eq!(ids(&check(&task("1"))), vec!["error.type.mismatch"]);
}
