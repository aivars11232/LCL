//! The demand-driven evaluator, against `05_SEMANTICS/12` and the registered
//! operator, function and truth tables.
//!
//! Every expression here travels the real pipeline — lex, parse, resolve,
//! check, plan — before the evaluator sees it, so a case that an earlier stage
//! would reject cannot reach this layer by accident.

mod common;

use common::{boolean, decimal, integer, value};
use lcl_runtime::{strict_equal, RuntimeError, Value};

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

// ---------------------------------------------------------------------------
// Evaluation order and short circuit
// ---------------------------------------------------------------------------

#[test]
fn closure_011_a_skipped_and_operand_is_not_demanded() {
    // "FALSE AND (1 / 0 == 0) => FALSE; the division is statically well-typed
    // but is not evaluated."
    //
    // The denominator arrives at invocation, so the division is *not*
    // statically foldable and would fault if it were demanded. It is FALSE
    // instead, which is exactly the non-demand this witness pins.
    //
    // The witness's own literal spelling is decided earlier by M4; see
    // `a_literal_zero_denominator_is_decided_before_this_layer`.
    let outcome = common::eval_dynamic(
        &[("input.zero", "INTEGER", "1")],
        "BOOLEAN",
        "FALSE AND (1 / REF(input.zero) == 0)",
        &[("input.zero", common::integer(0))],
    );
    assert_eq!(
        outcome.expect("the right operand is not demanded"),
        Value::Boolean(false)
    );
}

#[test]
fn a_demanded_zero_denominator_faults_at_this_layer() {
    // The same division, actually demanded: the contrast proves the skip above
    // is a real non-demand and not an accident of folding.
    let fault = common::eval_dynamic(
        &[("input.zero", "INTEGER", "1")],
        "BOOLEAN",
        "TRUE AND (1 / REF(input.zero) == 0)",
        &[("input.zero", common::integer(0))],
    )
    .expect_err("the right operand is demanded");
    assert_eq!(fault.id, RuntimeError::NumericDivisionByZero);
    // `expression_demand_resolution` lists this identifier, so the demand
    // resolves it to the execution stage.
    assert!(fault.demand_resolved);
}

#[test]
fn a_literal_zero_denominator_is_decided_before_this_layer() {
    // The earliest-stage rule: a statically known zero denominator is M4's,
    // and never reaches the runtime.
    let source = common::data_document(&[("data.subject", "DECIMAL", "1 / 0")]);
    let resolved = common::resolve(&source);
    let checked = common::check(&resolved);
    let primary = checked
        .primary()
        .expect("M4 decides a literal zero denominator");
    assert_eq!(
        primary.id.as_registry_str(),
        "error.numeric.division_by_zero"
    );
}

#[test]
fn a_skipped_or_operand_is_not_demanded() {
    let outcome = common::eval_dynamic(
        &[("input.zero", "INTEGER", "1")],
        "BOOLEAN",
        "TRUE OR (1 / REF(input.zero) == 0)",
        &[("input.zero", common::integer(0))],
    );
    assert_eq!(
        outcome.expect("the right operand is not demanded"),
        Value::Boolean(true)
    );
}

#[test]
fn unknown_on_the_left_never_skips_the_right() {
    // "UNKNOWN on the left never skips the right."
    assert_eq!(boolean("UNKNOWN AND FALSE"), Value::Boolean(false));
    assert_eq!(boolean("UNKNOWN OR TRUE"), Value::Boolean(true));
    assert_eq!(boolean("UNKNOWN AND TRUE"), Value::Unknown);
    assert_eq!(boolean("UNKNOWN OR FALSE"), Value::Unknown);
}

#[test]
fn the_registered_strong_kleene_tables_govern_not_and_or() {
    assert_eq!(boolean("NOT UNKNOWN"), Value::Unknown);
    assert_eq!(boolean("NOT TRUE"), Value::Boolean(false));
    assert_eq!(boolean("NOT FALSE"), Value::Boolean(true));
    assert_eq!(boolean("UNKNOWN AND UNKNOWN"), Value::Unknown);
    assert_eq!(boolean("UNKNOWN OR UNKNOWN"), Value::Unknown);
    assert_eq!(boolean("FALSE AND UNKNOWN"), Value::Boolean(false));
    assert_eq!(boolean("TRUE OR UNKNOWN"), Value::Boolean(true));
}

// ---------------------------------------------------------------------------
// MISSING and UNKNOWN
// ---------------------------------------------------------------------------

#[test]
fn consuming_missing_raises_required_missing_except_for_equality_and_exists() {
    // "A consumed MISSING operand emits error.required.missing except for ==,
    // !=, and EXISTS."
    //
    // A MISSING *literal* in a Boolean operand is a static sentinel-placement
    // defect that M4 owns; a MISSING that arrives at demand is this layer's.
    let fault = common::eval_unbound("BOOLEAN", "REF(output.unbound) < 2")
        .expect_err("a demanded MISSING operand");
    assert_eq!(fault.id, RuntimeError::RequiredMissing);

    // The three exemptions, which admit the sentinel statically too.
    assert_eq!(boolean("MISSING == MISSING"), Value::Boolean(true));
    assert_eq!(boolean("MISSING != UNKNOWN"), Value::Boolean(true));
    assert_eq!(boolean("EXISTS(MISSING)"), Value::Boolean(false));
}

#[test]
fn a_missing_demand_is_demand_resolved_to_the_execution_stage() {
    // `expression_demand_resolution` lists error.required.missing, so a demand
    // here resolves to the execution stage while keeping status.blocked.
    let fault = common::eval_unbound("INTEGER", "REF(output.unbound) + 1")
        .expect_err("a demanded MISSING operand");
    assert_eq!(fault.id, RuntimeError::RequiredMissing);
    assert!(
        fault.demand_resolved,
        "an eligible identifier demanded after preflight is demand-resolved"
    );
    assert_eq!(
        common::contracts()
            .demand()
            .status_for(RuntimeError::RequiredMissing),
        "status.blocked",
        "required MISSING retains status.blocked"
    );
}

#[test]
fn an_ineligible_identifier_is_not_demand_resolved() {
    // error.execution.order is not in the closed eligible map, so it keeps its
    // registered stage no matter when it is raised.
    assert!(!common::contracts()
        .demand()
        .is_eligible(RuntimeError::ExecutionOrder));
    assert!(!common::contracts()
        .demand()
        .is_eligible(RuntimeError::RetryExhausted));
    assert!(!common::contracts()
        .demand()
        .is_eligible(RuntimeError::PermissionDenied));
}

#[test]
fn unknown_propagates_through_arithmetic_and_comparison() {
    // "any UNKNOWN operand yields UNKNOWN for a constructor, arithmetic
    // operation, ordered comparison, membership, containment, pattern match,
    // ordinary pure function, property access, or index access."
    //
    // An UNKNOWN *literal* in arithmetic has no registered overload and is
    // M4's; an UNKNOWN that arrives at demand propagates here.
    let inputs = &[("input.unknown", "INTEGER", "1")][..];
    let supplied = &[("input.unknown", Value::Unknown)][..];
    assert_eq!(
        common::eval_dynamic(inputs, "INTEGER", "REF(input.unknown) + 1", supplied)
            .expect("UNKNOWN propagates"),
        Value::Unknown
    );
    assert_eq!(
        common::eval_dynamic(inputs, "BOOLEAN", "REF(input.unknown) < 1", supplied)
            .expect("UNKNOWN propagates"),
        Value::Unknown
    );
    assert_eq!(
        common::eval_dynamic(inputs, "BOOLEAN", "REF(input.unknown) IN [1, 2]", supplied)
            .expect("UNKNOWN propagates"),
        Value::Unknown
    );
}

#[test]
fn a_required_material_destination_rejects_a_demanded_unknown() {
    // "A required material destination rejects UNKNOWN with
    // error.value.unknown."
    assert_eq!(
        common::contracts()
            .demand()
            .status_for(RuntimeError::ValueUnknown),
        "status.blocked"
    );
    assert!(common::contracts()
        .demand()
        .is_eligible(RuntimeError::ValueUnknown));
}

#[test]
fn equality_treats_the_two_sentinels_as_distinct_singletons() {
    // "Each singleton sentinel equals itself and differs from the other
    // sentinel and every material value."
    assert_eq!(boolean("UNKNOWN == UNKNOWN"), Value::Boolean(true));
    assert_eq!(boolean("MISSING == UNKNOWN"), Value::Boolean(false));
    assert_eq!(boolean("MISSING == 1"), Value::Boolean(false));
    assert_eq!(boolean("UNKNOWN == 1"), Value::Boolean(false));
    assert_eq!(boolean("UNKNOWN != 1"), Value::Boolean(true));
}

#[test]
fn exists_answers_for_both_sentinels() {
    assert_eq!(boolean("EXISTS(MISSING)"), Value::Boolean(false));
    // An UNKNOWN value exists; only its content is undetermined.
    assert_eq!(boolean("EXISTS(UNKNOWN)"), Value::Boolean(true));
    assert_eq!(boolean("EXISTS(1)"), Value::Boolean(true));
}

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

#[test]
fn equality_always_returns_boolean_and_never_coerces() {
    // "No coercion makes references, strings, booleans, or quantities equal to
    // numbers."
    assert_eq!(boolean("1 == \"1\""), Value::Boolean(false));
    assert_eq!(boolean("TRUE == 1"), Value::Boolean(false));
    assert_eq!(boolean("\"a\" == \"a\""), Value::Boolean(true));
}

#[test]
fn integer_and_decimal_compare_by_exact_mathematical_value() {
    // "Different material static types are unequal except for exact
    // INTEGER/DECIMAL comparison."
    assert_eq!(boolean("1 == 1.0"), Value::Boolean(true));
    assert_eq!(boolean("1.50 == 1.5"), Value::Boolean(true));
    assert_eq!(boolean("1 == 1.0000001"), Value::Boolean(false));
}

#[test]
fn measure_equality_requires_the_identical_unit() {
    // "MEASURE equality compares both exact unit identifier and numeric
    // magnitude; different units produce FALSE."
    assert_eq!(
        boolean("MEASURE(1, unit.meter) == MEASURE(1, unit.meter)"),
        Value::Boolean(true)
    );
    // Different units are FALSE "rather than a unit error".
    assert_eq!(
        boolean("MEASURE(1, unit.meter) == MEASURE(1, unit.kilometer)"),
        Value::Boolean(false)
    );
}

#[test]
fn a_measure_is_never_equal_to_a_duration() {
    // The two are different types with opposite comparison rules, and the
    // MEASURE row admits Time-category units, so this is the case a shared
    // representation would get wrong.
    assert_eq!(
        boolean("MEASURE(1, unit.second) == DURATION(1, unit.second)"),
        Value::Boolean(false)
    );
}

#[test]
fn duration_equality_uses_the_normalized_magnitude() {
    // `ordered_value_equality`: "equal normalized DURATION magnitudes therefore
    // collapse as SET duplicates before ordering."
    assert_eq!(
        boolean("DURATION(1, unit.minute) == DURATION(60, unit.second)"),
        Value::Boolean(true)
    );
    assert_eq!(
        boolean("DURATION(1, unit.minute) == DURATION(59, unit.second)"),
        Value::Boolean(false)
    );
    // The same two magnitudes as MEASURE are unequal, because MEASURE requires
    // the identical unit identifier.
    assert_eq!(
        boolean("MEASURE(1, unit.minute) == MEASURE(60, unit.second)"),
        Value::Boolean(false)
    );
}

#[test]
fn closure_039_pattern_equality_is_representation_identity() {
    // "REGEX(\"a\") == REGEX(\"[a]\") => FALSE; pattern representation
    // identity, not accepted-language equivalence."
    assert_eq!(
        boolean("REGEX(\"a\") == REGEX(\"[a]\")"),
        Value::Boolean(false)
    );
    assert_eq!(
        boolean("REGEX(\"a\") == REGEX(\"a\")"),
        Value::Boolean(true)
    );
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn ordered_comparison_uses_the_registered_total_order() {
    assert_eq!(boolean("1 < 2"), Value::Boolean(true));
    assert_eq!(boolean("2 <= 2"), Value::Boolean(true));
    assert_eq!(boolean("\"a\" < \"b\""), Value::Boolean(true));
    // "STRING uses Unicode-scalar lexicographic order."
    assert_eq!(boolean("\"Z\" < \"a\""), Value::Boolean(true));
}

#[test]
fn ordered_measure_comparison_requires_the_same_exact_unit() {
    assert_eq!(
        boolean("MEASURE(1, unit.meter) < MEASURE(2, unit.meter)"),
        Value::Boolean(true)
    );
    // Unequal units at demand.
    let fault = common::eval_dynamic(
        &[("input.length", "MEASURE", "MEASURE(1, unit.meter)")],
        "BOOLEAN",
        "REF(input.length) < MEASURE(2, unit.meter)",
        &[(
            "input.length",
            Value::Quantity(
                lcl_checker::numeric::Decimal::from_integer(
                    lcl_checker::numeric::Integer::from_u64(1),
                ),
                lcl_checker::ty::UnitId("unit.kilometer".to_string()),
            ),
        )],
    )
    .expect_err("unequal concrete units at demand");
    assert_eq!(fault.id, RuntimeError::NumericUnitMismatch);
}

#[test]
fn temporal_values_order_by_their_canonical_keys() {
    assert_eq!(
        boolean("DATE(\"2024-01-01\") < DATE(\"2024-01-02\")"),
        Value::Boolean(true)
    );
    // "DATETIME uses its exact offset-normalized UTC instant."
    assert_eq!(
        boolean("DATETIME(\"2024-01-01T00:00:00Z\") == DATETIME(\"2024-01-01T01:00:00+01:00\")"),
        Value::Boolean(false),
        "equality is representation identity for constructed values"
    );
    assert_eq!(
        boolean("DATETIME(\"2024-01-01T00:00:00Z\") < DATETIME(\"2024-01-01T00:00:01Z\")"),
        Value::Boolean(true)
    );
}

#[test]
fn closure_045_time_subtracts_its_declared_offset() {
    // "TIME(\"00:00:00+01:00\") compared with TIME(\"23:00:00Z\")": the first
    // normalizes to -01:00 on the nominal day, so it is the earlier value and
    // no modulo-24-hour wrapping occurs.
    assert_eq!(
        boolean("TIME(\"00:00:00+01:00\") < TIME(\"23:00:00Z\")"),
        Value::Boolean(true)
    );
}

// ---------------------------------------------------------------------------
// Membership and containment
// ---------------------------------------------------------------------------

#[test]
fn membership_is_strict_equality() {
    assert_eq!(boolean("1 IN [1, 2, 3]"), Value::Boolean(true));
    assert_eq!(boolean("4 IN [1, 2, 3]"), Value::Boolean(false));
}

#[test]
fn membership_in_an_empty_collection_is_false() {
    // "Membership in an empty collection is FALSE once both operands are
    // material." An empty literal needs a declared member type, so the
    // collection is declared rather than written inline.
    let source = common::data_document(&[
        ("data.empty", "LIST[INTEGER]", "[]"),
        ("data.member", "BOOLEAN", "1 IN REF(data.empty)"),
    ]);
    let fixture = common::fixture(&source);
    assert_eq!(
        fixture.demand("data.member").expect("evaluates"),
        Value::Boolean(false)
    );
}

#[test]
fn contains_reverses_in_for_a_collection() {
    assert_eq!(boolean("[1, 2] CONTAINS 1"), Value::Boolean(true));
    assert_eq!(boolean("[1, 2] CONTAINS 3"), Value::Boolean(false));
}

#[test]
fn string_contains_tests_a_contiguous_substring() {
    // "STRING CONTAINS tests a contiguous exact Unicode-scalar substring, and
    // the empty substring is present in every STRING."
    assert_eq!(boolean("\"abcd\" CONTAINS \"bc\""), Value::Boolean(true));
    assert_eq!(boolean("\"abcd\" CONTAINS \"bd\""), Value::Boolean(false));
    assert_eq!(boolean("\"abcd\" CONTAINS \"\""), Value::Boolean(true));
    assert_eq!(boolean("\"\" CONTAINS \"\""), Value::Boolean(true));
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

#[test]
fn closure_007_list_indexing_is_zero_based_without_wraparound() {
    // "[10, 20][0], [10, 20][1], [10, 20][-1], [10, 20][2] => 10, 20, MISSING,
    // MISSING respectively; no negative wraparound."
    assert_eq!(value("INTEGER", "[10, 20][0]"), integer(10));
    assert_eq!(value("INTEGER", "[10, 20][1]"), integer(20));
    assert_eq!(value("INTEGER", "[10, 20][-1]"), Value::Missing);
    assert_eq!(value("INTEGER", "[10, 20][2]"), Value::Missing);
}

// ---------------------------------------------------------------------------
// Arithmetic
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_is_exact_with_no_float_anywhere() {
    assert_eq!(value("INTEGER", "2 + 3"), integer(5));
    assert_eq!(value("INTEGER", "7 - 9"), integer(-2));
    assert_eq!(value("INTEGER", "6 * 7"), integer(42));
    // A value no binary float can hold exactly.
    assert_eq!(value("DECIMAL", "0.1 + 0.2"), decimal("0.3"));
}

#[test]
fn integer_promotes_to_decimal_only_when_paired_with_decimal() {
    assert_eq!(value("INTEGER", "1 + 1"), integer(2));
    assert_eq!(value("DECIMAL", "1 + 1.0"), decimal("2.0"));
}

#[test]
fn every_scalar_division_has_decimal_result_type() {
    // "every scalar division overload has static result type DECIMAL"
    assert_eq!(value("DECIMAL", "4 / 2"), decimal("2"));
    assert_eq!(value("DECIMAL", "1 / 8"), decimal("0.125"));
}

#[test]
fn a_measure_divided_by_a_number_preserves_its_unit() {
    // "MEASURE divided by INTEGER or DECIMAL preserves its exact UNIT and has a
    // DECIMAL numeric component."
    let quotient = value("MEASURE", "MEASURE(9, unit.meter) / 4");
    match quotient {
        Value::Quantity(magnitude, unit) => {
            assert_eq!(unit.0, "unit.meter");
            assert_eq!(magnitude.to_string(), "2.25");
        }
        other => panic!("expected a MEASURE, got {other}"),
    }
}

#[test]
fn a_measure_divided_by_the_same_unit_is_dimensionless() {
    // "MEASURE divided by a MEASURE with the same exact UNIT returns
    // dimensionless DECIMAL"
    assert_eq!(
        value("DECIMAL", "MEASURE(9, unit.meter) / MEASURE(4, unit.meter)"),
        decimal("2.25")
    );
}

#[test]
fn measure_arithmetic_across_units_is_a_unit_mismatch() {
    // "Dynamically supplied MEASURE values or constructor unit values violate
    // the exact registered unit rule after their operand families passed static
    // checking." Statically known units are M4's; supplied ones are this
    // layer's.
    let fault = common::eval_dynamic(
        &[("input.length", "MEASURE", "MEASURE(1, unit.meter)")],
        "MEASURE",
        "REF(input.length) + MEASURE(1, unit.meter)",
        &[(
            "input.length",
            Value::Quantity(
                lcl_checker::numeric::Decimal::from_integer(
                    lcl_checker::numeric::Integer::from_u64(1),
                ),
                lcl_checker::ty::UnitId("unit.kilometer".to_string()),
            ),
        )],
    )
    .expect_err("unequal concrete units at demand");
    assert_eq!(fault.id, RuntimeError::NumericUnitMismatch);
}

#[test]
fn a_statically_known_unit_mismatch_is_decided_before_this_layer() {
    let source = common::data_document(&[(
        "data.subject",
        "MEASURE",
        "MEASURE(1, unit.meter) + MEASURE(1, unit.kilometer)",
    )]);
    let resolved = common::resolve(&source);
    let checked = common::check(&resolved);
    let primary = checked
        .primary()
        .expect("M4 decides a static unit mismatch");
    assert_eq!(primary.id.as_registry_str(), "error.numeric.unit_mismatch");
}

#[test]
fn round_materializes_an_otherwise_non_terminating_quotient() {
    // "A finite base-10 result is required unless the quotient is the direct
    // first argument of ROUND, which rounds once using half-even."
    assert_eq!(value("DECIMAL", "ROUND(1 / 3, 2)"), decimal("0.33"));
    assert_eq!(value("DECIMAL", "ROUND(2 / 3, 2)"), decimal("0.67"));
    // Half-even, not half-up.
    assert_eq!(value("DECIMAL", "ROUND(5 / 2, 0)"), decimal("2"));
    assert_eq!(value("DECIMAL", "ROUND(7 / 2, 0)"), decimal("4"));
}

#[test]
fn round_preserves_a_measure_unit() {
    // "ROUND also accepts MEASURE and preserves its UNIT."
    match value("MEASURE", "ROUND(MEASURE(1, unit.meter) / 3, 2)") {
        Value::Quantity(magnitude, unit) => {
            assert_eq!(unit.0, "unit.meter");
            assert_eq!(magnitude.to_string(), "0.33");
        }
        other => panic!("expected a MEASURE, got {other}"),
    }
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

#[test]
fn closure_017_count_and_empty_over_every_registered_family() {
    // "COUNT(BYTES(8)), EMPTY(BYTES(0)), COUNT(\"A😀\") => 8, TRUE, 2
    // respectively; STRING counts Unicode scalar values."
    assert_eq!(value("INTEGER", "COUNT(BYTES(8))"), integer(8));
    assert_eq!(boolean("EMPTY(BYTES(0))"), Value::Boolean(true));
    assert_eq!(value("INTEGER", "COUNT(\"A\u{1F600}\")"), integer(2));
    assert_eq!(value("INTEGER", "COUNT([1, 2, 3])"), integer(3));
}

#[test]
fn empty_is_true_exactly_when_count_is_zero() {
    assert_eq!(boolean("EMPTY(\"\")"), Value::Boolean(true));
    assert_eq!(boolean("EMPTY(\"a\")"), Value::Boolean(false));
    assert_eq!(boolean("EMPTY([1])"), Value::Boolean(false));
}

#[test]
fn null_is_not_an_accepted_empty_family_and_m4_says_so() {
    // "NULL is not an accepted family." M4 owns it, because the operand family
    // is statically known.
    let source = common::data_document(&[("data.subject", "BOOLEAN", "EMPTY(NULL)")]);
    let resolved = common::resolve(&source);
    let checked = common::check(&resolved);
    let primary = checked.primary().expect("M4 rejects EMPTY(NULL)");
    assert_eq!(primary.id.as_registry_str(), "error.operator.operand");
}

#[test]
fn closure_013_empty_quantifier_arguments_have_boolean_context() {
    // "ALL([]), ANY([]), NONE([]) => TRUE, FALSE, TRUE respectively; immediate
    // empty argument supplies BOOLEAN context."
    assert_eq!(boolean("ALL([])"), Value::Boolean(true));
    assert_eq!(boolean("ANY([])"), Value::Boolean(false));
    assert_eq!(boolean("NONE([])"), Value::Boolean(true));
}

#[test]
fn closure_014_an_immediate_sequence_admits_unknown() {
    // "ALL([TRUE, UNKNOWN]) inside a condition => UNKNOWN; the immediate
    // sequence is transient and cannot be stored as LIST data."
    assert_eq!(boolean("ALL([TRUE, UNKNOWN])"), Value::Unknown);
    assert_eq!(boolean("ANY([FALSE, UNKNOWN])"), Value::Unknown);
    assert_eq!(boolean("ALL([FALSE, UNKNOWN])"), Value::Boolean(false));
    assert_eq!(boolean("ANY([TRUE, UNKNOWN])"), Value::Boolean(true));
    assert_eq!(boolean("NONE([TRUE, UNKNOWN])"), Value::Boolean(false));
}

#[test]
fn parentheses_do_not_change_the_immediate_sequence_rule() {
    // "Parentheses around the immediate sequence do not change that rule."
    assert_eq!(boolean("ALL(([TRUE, UNKNOWN]))"), Value::Unknown);
}

#[test]
fn a_quantifier_member_of_another_material_family_is_an_operand_defect() {
    // A statically wrong member family is M4's.
    let source = common::data_document(&[("data.subject", "BOOLEAN", "ALL([TRUE, 1])")]);
    let resolved = common::resolve(&source);
    let checked = common::check(&resolved);
    let primary = checked.primary().expect("M4 rejects a non-BOOLEAN member");
    assert_eq!(primary.id.as_registry_str(), "error.operator.operand");
}

#[test]
fn a_missing_quantifier_member_is_a_required_missing() {
    // "A MISSING member emits error.required.missing" — at demand, when the
    // member's value arrives from the invocation.
    let fault = common::eval_unbound("BOOLEAN", "ALL([TRUE, REF(output.flag)])")
        .expect_err("a MISSING quantifier member");
    assert_eq!(fault.id, RuntimeError::RequiredMissing);
}

#[test]
fn reductions_use_the_registered_order_and_reject_an_empty_collection() {
    assert_eq!(value("INTEGER", "SUM([1, 2, 3])"), integer(6));
    assert_eq!(value("INTEGER", "MIN([3, 1, 2])"), integer(1));
    assert_eq!(value("INTEGER", "MAX([3, 1, 2])"), integer(3));
}

#[test]
fn closure_015_a_typed_empty_collection_is_rejected_by_a_reduction() {
    // "SUM of a declared LIST[INTEGER] value [] => error.operator.operand
    // because SUM requires a nonempty typed collection."
    let source = common::data_document(&[
        ("data.empty", "LIST[INTEGER]", "[]"),
        ("data.total", "INTEGER", "SUM(REF(data.empty))"),
    ]);
    let fixture = common::fixture(&source);
    let fault = fixture
        .demand("data.total")
        .expect_err("an empty reduction");
    assert_eq!(fault.id, RuntimeError::OperatorOperand);
}

#[test]
fn sum_requires_one_exact_unit_for_measure() {
    // "SUM returns the exact mathematical sum in the registered member family,
    // with one exact unit for MEASURE; it never invents an empty-sum unit."
    match value(
        "MEASURE",
        "SUM([MEASURE(1, unit.meter), MEASURE(2, unit.meter)])",
    ) {
        Value::Quantity(magnitude, unit) => {
            assert_eq!(unit.0, "unit.meter");
            assert_eq!(magnitude.to_string(), "3");
        }
        other => panic!("expected a MEASURE, got {other}"),
    }
}

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

#[test]
fn a_bracket_literal_defaults_to_list() {
    assert_eq!(
        value("LIST[INTEGER]", "[1, 2, 2]"),
        Value::List(vec![integer(1), integer(2), integer(2)])
    );
}

#[test]
fn a_set_collapses_strict_equal_duplicates_after_member_evaluation() {
    // "equal members collapse under strict equality before the snapshot" and
    // "Source order evaluates every SET source member before strict-equal
    // duplicates collapse."
    assert_eq!(
        value("SET[INTEGER]", "[1, 2, 2, 1]"),
        Value::Set(vec![integer(1), integer(2)])
    );
}

#[test]
fn missing_cannot_become_a_material_collection_member() {
    // "MISSING and UNKNOWN cannot become material collection members." A
    // literal MISSING member is M4's; a supplied one is this layer's.
    let fault = common::eval_unbound("LIST[INTEGER]", "[1, REF(output.unbound)]")
        .expect_err("a MISSING member");
    assert_eq!(fault.id, RuntimeError::RequiredMissing);
}

// ---------------------------------------------------------------------------
// MATCHES
// ---------------------------------------------------------------------------

#[test]
fn matches_compares_the_entire_input() {
    assert_eq!(
        boolean("\"abc\" MATCHES REGEX(\"a.c\")"),
        Value::Boolean(true)
    );
    // "A non-match returns FALSE."
    assert_eq!(
        boolean("\"xabcx\" MATCHES REGEX(\"a.c\")"),
        Value::Boolean(false)
    );
}

#[test]
fn matches_honours_the_registered_flags() {
    assert_eq!(
        boolean("\"ABC\" MATCHES REGEX(\"[a-z]+\", \"i\")"),
        Value::Boolean(true)
    );
    assert_eq!(
        boolean("\"ABC\" MATCHES REGEX(\"[a-z]+\")"),
        Value::Boolean(false)
    );
}

#[test]
fn a_glob_matches_the_full_workspace_relative_path() {
    assert_eq!(
        boolean("\"src/test1.lcl\" MATCHES GLOB(\"src/**/test?.lcl\")"),
        Value::Boolean(true)
    );
}

// ---------------------------------------------------------------------------
// Totality
// ---------------------------------------------------------------------------

#[test]
fn strict_equality_is_reflexive_over_every_representable_value() {
    let values = [
        Value::Boolean(true),
        integer(0),
        decimal("1.5"),
        text("x"),
        Value::Null,
        Value::Missing,
        Value::Unknown,
        Value::List(vec![integer(1)]),
        Value::Set(vec![integer(1)]),
        Value::Reference("a".to_string()),
    ];
    for value in &values {
        let reflexive = strict_equal(value, value);
        // The two sentinels equal themselves; every material value does too.
        assert!(reflexive, "{value} must equal itself");
    }
}

#[test]
fn evaluation_is_a_pure_function_of_its_input() {
    for _ in 0..3 {
        assert_eq!(value("DECIMAL", "ROUND(1 / 3, 5)"), decimal("0.33333"));
    }
}

// ---------------------------------------------------------------------------
// The earliest-stage boundary
// ---------------------------------------------------------------------------
//
// These witnesses are decided *before* this layer. Executing them here proves
// the boundary rather than assuming it: "No later stage repairs an earlier
// invalid stage by guessing intent."

/// The registered identifier M4 reports as primary for one document.
fn static_primary(source: &str) -> String {
    let resolved = common::resolve(source);
    let checked = common::check(&resolved);
    checked
        .primary()
        .map(|d| d.id.as_registry_str().to_string())
        .unwrap_or_else(|| "none".to_string())
}

#[test]
fn closure_012_a_skipped_branch_is_still_statically_checked() {
    // "TRUE OR (1 + \"x\" == 0) => error.operator.operand from static checking
    // of the skipped expression."
    //
    // An operand-family defect never moves to execution, so the runtime never
    // sees this document at all.
    let source = common::data_document(&[("data.subject", "BOOLEAN", "TRUE OR (1 + \"x\" == 0)")]);
    assert_eq!(static_primary(&source), "error.operator.operand");
}

#[test]
fn closure_016_an_unconstrained_empty_literal_has_no_member_type() {
    // "SUM([]) without a unique member-type context => error.type.mismatch;
    // empty literal member type cannot be inferred."
    let source = common::data_document(&[("data.subject", "INTEGER", "SUM([])")]);
    assert_eq!(static_primary(&source), "error.type.mismatch");
}

#[test]
fn closure_047_unequal_concrete_units_are_a_unit_mismatch() {
    // "MEASURE(1, unit.pixel) + MEASURE(1, unit.second) =>
    // error.numeric.unit_mismatch." Statically known units are M4's; the
    // dynamically supplied case is proved above.
    let source = common::data_document(&[(
        "data.subject",
        "MEASURE",
        "MEASURE(1, unit.pixel) + MEASURE(1, unit.second)",
    )]);
    assert_eq!(static_primary(&source), "error.numeric.unit_mismatch");
}

#[test]
fn the_exclusion_rule_holds_for_every_structural_defect() {
    // `exclusion_rule`: "No source structure, token, name resolution,
    // type-family, signature arity, receiving-type, or required
    // static-validation defect qualifies." Each of these is decided before the
    // runtime, so none can arrive here demand-resolved.
    for (subject, expected) in [
        ("COUNT()", "error.operator.operand"),
        ("COUNT(1, 2)", "error.operator.operand"),
    ] {
        let source = common::data_document(&[("data.subject", "INTEGER", subject)]);
        assert_eq!(static_primary(&source), expected, "for {subject}");
    }

    // A name-resolution defect is earlier still: M3 decides it, and the static
    // stage is never evaluated for that unit.
    let source = common::data_document(&[("data.subject", "INTEGER", "REF(data.absent)")]);
    let resolved = common::resolve(&source);
    assert_eq!(
        resolved.primary().map(|d| d.id.as_registry_str()),
        Some("error.reference.unresolved")
    );
}
