//! Object `FIELD` value constraints: `MINIMUM`, `MAXIMUM` and `PATTERN`.
//!
//! `types_v0.1.0.json#/object_type_contract`: "Defaults and constraints govern
//! construction/validation, not object type identity". `03_TYPES_AND_VALUES/07`:
//! "MINIMUM and MAXIMUM are inclusive. ... PATTERN is GLOB or REGEX".
//!
//! A statically known field value is judged here; any other is recorded as a
//! demand obligation for the layer that constructs the object.
//!
//! Regressions, PRETEST-01 F01.

mod common;

use common::{check, ids, HEADER};
use lcl_checker::{DemandKind, Outcome};

const BOUNDED: &str = "\nDEFINE:\n    ID: type.bounded\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: ratio\n        TYPE: DECIMAL\n        REQUIRED: TRUE\n        MINIMUM: 0.5\n        MAXIMUM: 1\n";

fn bounded_data(ty: &str, ratio: &str) -> String {
    format!(
        "{HEADER}{BOUNDED}\nDATA:\n    ID: data.bounded\n    TYPE: {ty}\n    VALUE:\n        ratio: {ratio}\n"
    )
}

#[test]
fn a_named_object_type_bounds_its_literal_field_values() {
    let ty = "OBJECT[REF(type.bounded)]";
    assert_eq!(
        ids(&check(&bounded_data(ty, "1.5"))),
        vec!["error.value.out_of_range"]
    );
    assert_eq!(
        ids(&check(&bounded_data(ty, "0.25"))),
        vec!["error.value.out_of_range"]
    );
    // Inclusive at both ends.
    assert_eq!(check(&bounded_data(ty, "1.0")).outcome(), Outcome::Checked);
    assert_eq!(check(&bounded_data(ty, "0.5")).outcome(), Outcome::Checked);
}

/// The pinned conformance source: an exact quotient is statically known.
#[test]
fn a_statically_known_quotient_is_bounded_too() {
    assert_eq!(
        ids(&check(&bounded_data("OBJECT[REF(type.bounded)]", "3 / 2"))),
        vec!["error.value.out_of_range"]
    );
}

#[test]
fn a_transparent_alias_preserves_the_constraints() {
    let source = format!(
        "{HEADER}{BOUNDED}\nDEFINE:\n    ID: type.alias\n    KIND: kind.type\n    BASE: REF(type.bounded)\n\n\
         DATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF(type.alias)]\n    VALUE:\n        ratio: 1.5\n"
    );
    assert_eq!(ids(&check(&source)), vec!["error.value.out_of_range"]);
}

/// Constraints are not identity, so two same-shaped types are the same object
/// type. The constraints applied are still the ones the declaration named.
#[test]
fn constraints_come_from_the_named_type_not_a_same_shaped_one() {
    let loose = "\nDEFINE:\n    ID: type.loose\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: ratio\n        TYPE: DECIMAL\n        REQUIRED: TRUE\n        MAXIMUM: 10\n";
    let data = |ty: &str| {
        format!(
            "{HEADER}{loose}{BOUNDED}\nDATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF({ty})]\n    VALUE:\n        ratio: 5.0\n"
        )
    };
    assert_eq!(check(&data("type.loose")).outcome(), Outcome::Checked);
    assert_eq!(
        ids(&check(&data("type.bounded"))),
        vec!["error.value.out_of_range"]
    );
}

#[test]
fn a_local_schema_bounds_its_fields() {
    let data = |count: &str| {
        format!(
            "{HEADER}\nDATA:\n    ID: data.local\n    TYPE: OBJECT\n    SCHEMA:\n        FIELD:\n            NAME: count\n            TYPE: INTEGER\n            REQUIRED: TRUE\n            MINIMUM: 5\n    VALUE:\n        count: {count}\n"
        )
    };
    assert_eq!(ids(&check(&data("2"))), vec!["error.value.out_of_range"]);
    assert_eq!(check(&data("7")).outcome(), Outcome::Checked);
}

#[test]
fn a_nested_object_field_uses_its_own_types_constraints() {
    let data = |ratio: &str| {
        format!(
            "{HEADER}{BOUNDED}\nDEFINE:\n    ID: type.outer\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: inner\n        TYPE: OBJECT[REF(type.bounded)]\n        REQUIRED: TRUE\n\n\
             DATA:\n    ID: data.outer\n    TYPE: OBJECT[REF(type.outer)]\n    VALUE:\n        inner:\n            ratio: {ratio}\n"
        )
    };
    assert_eq!(ids(&check(&data("2.0"))), vec!["error.value.out_of_range"]);
    assert_eq!(check(&data("0.75")).outcome(), Outcome::Checked);
}

#[test]
fn a_field_pattern_constrains_its_value() {
    let data = |code: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: type.coded\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: code\n        TYPE: STRING\n        REQUIRED: TRUE\n        PATTERN: REGEX(\"[a-z]+\")\n\n\
             DATA:\n    ID: data.coded\n    TYPE: OBJECT[REF(type.coded)]\n    VALUE:\n        code: {code}\n"
        )
    };
    assert_eq!(
        ids(&check(&data("\"ABC\""))),
        vec!["error.pattern.mismatch"]
    );
    assert_eq!(check(&data("\"abc\"")).outcome(), Outcome::Checked);
}

/// A field value the static layer cannot know is the constructing layer's.
#[test]
fn a_field_value_that_is_not_statically_known_is_deferred() {
    let source = format!(
        "{HEADER}{BOUNDED}\nDATA:\n    ID: data.ratio\n    TYPE: DECIMAL\n    VALUE: 1.5\n\n\
         DATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF(type.bounded)]\n    VALUE:\n        ratio: REF(data.ratio)\n"
    );
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Checked);
    let at = source.rfind("REF(data.ratio)").expect("written");
    assert!(
        checked
            .deferred()
            .iter()
            .any(|o| o.kind == DemandKind::DeclaredBound && o.span.start == at),
        "{:?}",
        checked.deferred()
    );
    assert!(checked.object_schema(0).is_some() || checked.object_schema(1).is_some());
}

/// `object_type_contract/combined_schema`: "an additional SCHEMA must have an
/// identical field/type/requiredness map and identical defaults and
/// constraints after alias resolution".
///
/// Regression, PRETEST-01 F01. Defaults were compared with their source spans,
/// so an identical SCHEMA written anywhere else was never identical.
#[test]
fn a_combined_schema_compares_written_defaults_and_constraints() {
    let data = |maximum: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: type.pair\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: count\n        TYPE: INTEGER\n        REQUIRED: FALSE\n        DEFAULT: 1\n        MAXIMUM: 9\n\n\
             DATA:\n    ID: data.pair\n    TYPE: OBJECT[REF(type.pair)]\n    SCHEMA:\n        FIELD:\n            NAME: count\n            TYPE: INTEGER\n            REQUIRED: FALSE\n            DEFAULT: 1\n            MAXIMUM: {maximum}\n    VALUE:\n        count: 2\n"
        )
    };
    assert_eq!(check(&data("9")).outcome(), Outcome::Checked);
    assert_eq!(ids(&check(&data("8"))), vec!["error.object.schema"]);
}
