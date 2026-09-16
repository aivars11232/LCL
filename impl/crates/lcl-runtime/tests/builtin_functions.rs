//! `ABS` and `ROUND`: the two registered functions the evaluator did not apply.
//!
//! `06_STANDARD_LIBRARY/04_BUILT_IN_FUNCTIONS.txt` registers eleven pure
//! functions. Nine were applied; `ABS` and the general form of `ROUND` reached
//! the evaluator's unregistered-function arm and raised
//! `error.operator.operand` on a call the static checker had already accepted.
//! These are the regression tests for closing that gap.
//!
//! Every case here goes through the whole pipeline — bytes, tokens, syntax,
//! resolution, static checking, preflight, demand — so a case that the checker
//! would reject cannot appear: the unregistered-family arms of both functions
//! are totality, not reachable behavior.

mod common;

use lcl_runtime::Value;

#[test]
fn abs_returns_the_same_family_with_absolute_magnitude() {
    // "ABS(integer_decimal_duration_or_measure) -> same family with absolute
    // magnitude"
    assert_eq!(
        common::value("INTEGER", "ABS(-5)"),
        common::value("INTEGER", "5")
    );
    assert_eq!(
        common::value("INTEGER", "ABS(5)"),
        common::value("INTEGER", "5")
    );
    assert_eq!(
        common::value("DECIMAL", "ABS(-1.25)"),
        common::value("DECIMAL", "1.25")
    );
}

#[test]
fn abs_of_zero_is_zero_and_keeps_its_family() {
    assert_eq!(
        common::value("INTEGER", "ABS(0)"),
        common::value("INTEGER", "0")
    );
}

#[test]
fn abs_keeps_a_measures_exact_unit() {
    assert_eq!(
        common::value("MEASURE", "ABS(MEASURE(-3, unit.second))"),
        common::value("MEASURE", "MEASURE(3, unit.second)")
    );
}

#[test]
fn abs_keeps_a_durations_exact_unit() {
    // DURATION and MEASURE are one value shape with a registered unit, and a
    // magnitude changes neither the family nor the unit.
    //
    // A negative DURATION is not reachable either: the type "declares an
    // inclusive minimum of 0", and `03_TYPES_AND_VALUES/06` makes a negative
    // DURATION subtraction produce error.value.out_of_range (LCL-REPAIR-05,
    // S3). ABS still accepts a DURATION and keeps its unit.
    assert_eq!(
        common::value(
            "DURATION",
            "ABS(DURATION(90, unit.second) - DURATION(30, unit.second))"
        ),
        common::value("DURATION", "DURATION(60, unit.second)")
    );
}

#[test]
fn abs_propagates_unknown() {
    // An UNKNOWN operand has an undetermined magnitude, not an error.
    assert_eq!(common::boolean("EXISTS(ABS(-5))"), Value::Boolean(true));
}

#[test]
fn round_uses_half_to_even() {
    // "using half-even". The distinguishing cases are the exact halves: 2.45
    // rounds down to the even 2.4, and 2.55 rounds up to the even 2.6.
    assert_eq!(
        common::value("DECIMAL", "ROUND(2.45, 1)"),
        common::value("DECIMAL", "2.4")
    );
    assert_eq!(
        common::value("DECIMAL", "ROUND(2.55, 1)"),
        common::value("DECIMAL", "2.6")
    );
}

#[test]
fn round_keeps_a_value_that_is_already_short_enough() {
    assert_eq!(
        common::value("DECIMAL", "ROUND(1.5, 1)"),
        common::value("DECIMAL", "1.5")
    );
}

#[test]
fn round_keeps_a_measures_exact_unit() {
    // "a MEASURE keeps its exact UNIT"
    assert_eq!(
        common::value("MEASURE", "ROUND(MEASURE(1.25, unit.second), 1)"),
        common::value("MEASURE", "MEASURE(1.2, unit.second)")
    );
}

#[test]
fn the_direct_division_form_still_rounds_the_exact_quotient_once() {
    // The one context that materialises an otherwise non-terminating quotient.
    // Implementing the general form must not have displaced it.
    assert_eq!(
        common::value("DECIMAL", "ROUND(10 / 3, 2)"),
        common::value("DECIMAL", "3.33")
    );
    assert_eq!(
        common::value("DECIMAL", "ROUND(2 / 3, 4)"),
        common::value("DECIMAL", "0.6667")
    );
}

#[test]
fn round_of_a_negative_value_rounds_its_exact_magnitude() {
    assert_eq!(
        common::value("DECIMAL", "ROUND(-2.45, 1)"),
        common::value("DECIMAL", "-2.4")
    );
}

#[test]
fn round_to_more_digits_than_the_value_has_is_exact() {
    assert_eq!(
        common::value("DECIMAL", "ROUND(1.5, 3)"),
        common::value("DECIMAL", "1.500")
    );
}
