//! Phase B: the type model, defined types, and object schemas.

mod common;

use common::{check, data_document, ids, HEADER};
use lcl_checker::Outcome;

fn define(body: &str) -> String {
    format!("{HEADER}{body}")
}

#[test]
fn a_value_must_match_its_declared_type() {
    let checked = check(&data_document("INTEGER", "\"three\""));
    assert_eq!(ids(&checked), vec!["error.type.mismatch"]);
    assert_eq!(checked.terminal_status(), Some("status.invalid"));

    assert_eq!(
        check(&data_document("INTEGER", "3")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&data_document("DECIMAL", "3.5")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&data_document("BOOLEAN", "TRUE")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&data_document("NULL", "NULL")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&data_document("STRING", "\"x\"")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn no_implicit_coercion_exists_between_families() {
    // "No implicit coercion occurs except exact INTEGER-to-DECIMAL promotion in
    // mixed numeric arithmetic/comparison."
    for (declared, value) in [
        ("INTEGER", "\"3\""),
        ("STRING", "3"),
        ("BOOLEAN", "1"),
        ("INTEGER", "TRUE"),
        ("DECIMAL", "\"3.5\""),
    ] {
        let checked = check(&data_document(declared, value));
        assert_eq!(
            ids(&checked),
            vec!["error.type.mismatch"],
            "{declared} must reject {value}"
        );
    }
}

#[test]
fn an_integer_value_does_not_satisfy_a_decimal_declaration() {
    // Promotion is an arithmetic and comparison rule, not an assignment rule:
    // "Every value has exactly one static type. Assignment does not exist."
    let checked = check(&data_document("DECIMAL", "3"));
    assert_eq!(ids(&checked), vec!["error.type.mismatch"]);
}

#[test]
fn a_transparent_alias_is_its_resolved_type() {
    let source = define(
        "\nDEFINE:\n    ID: type.counter\n    KIND: kind.type\n    BASE: INTEGER\n\nDATA:\n    ID: data.count\n    TYPE: REF(type.counter)\n    VALUE: 2\n",
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    // "It creates no nominal subtype": the alias and its base are one type, and
    // a wrong value is still wrong.
    let bad = define(
        "\nDEFINE:\n    ID: type.counter\n    KIND: kind.type\n    BASE: INTEGER\n\nDATA:\n    ID: data.count\n    TYPE: REF(type.counter)\n    VALUE: \"two\"\n",
    );
    assert_eq!(ids(&check(&bad)), vec!["error.type.mismatch"]);
}

#[test]
fn an_enum_item_resolves_only_against_one_expected_domain() {
    let source = define(
        "\nDEFINE:\n    ID: type.state\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: ready\n    ITEM: done\n\nDATA:\n    ID: data.state\n    TYPE: REF(type.state)\n    VALUE: ready\n",
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    // "A nonmember or ambiguous enum value uses error.type.mismatch."
    let nonmember = define(
        "\nDEFINE:\n    ID: type.state\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: ready\n    ITEM: done\n\nDATA:\n    ID: data.state\n    TYPE: REF(type.state)\n    VALUE: pending\n",
    );
    assert_eq!(ids(&check(&nonmember)), vec!["error.type.mismatch"]);
}

#[test]
fn an_alias_of_an_enum_preserves_that_exact_domain() {
    // "An alias of a defined enum preserves that enum's exact domain."
    let source = define(
        "\nDEFINE:\n    ID: type.state\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: ready\n\nDEFINE:\n    ID: type.alias\n    KIND: kind.type\n    BASE: REF(type.state)\n\nDATA:\n    ID: data.state\n    TYPE: REF(type.alias)\n    VALUE: ready\n",
    );
    assert_eq!(check(&source).outcome(), Outcome::Checked);

    let nonmember = define(
        "\nDEFINE:\n    ID: type.state\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: ready\n\nDEFINE:\n    ID: type.alias\n    KIND: kind.type\n    BASE: REF(type.state)\n\nDATA:\n    ID: data.state\n    TYPE: REF(type.alias)\n    VALUE: elsewhere\n",
    );
    assert_eq!(ids(&check(&nonmember)), vec!["error.type.mismatch"]);
}

#[test]
fn a_type_alias_cycle_is_reported_with_its_own_registered_identifier() {
    // `03_TYPES_AND_VALUES/01`: "Type references resolve acyclically before
    // value checking … cycles use error.reference.cycle", which the registry
    // stages at resolution. M3 checks only the alias domains that resolve to a
    // core identifier, so a kind.type chain reaches this stage.
    let source = define(
        "\nDEFINE:\n    ID: type.a\n    KIND: kind.type\n    BASE: REF(type.b)\n\nDEFINE:\n    ID: type.b\n    KIND: kind.type\n    BASE: REF(type.a)\n\nDATA:\n    ID: data.x\n    TYPE: REF(type.a)\n    VALUE: 1\n",
    );
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Rejected);
    let defects = checked.earlier_stage_defects();
    assert!(!defects.is_empty(), "the cycle must be reported");
    for defect in defects {
        assert_eq!(defect.identifier, "error.reference.cycle");
        assert_eq!(defect.stage, lcl_diagnostics::Stage::Resolution);
        assert_eq!(defect.default_status, "status.invalid");
    }
    // It is a resolution identifier, so it never appears in the static list.
    assert!(checked
        .diagnostics()
        .iter()
        .all(|d| d.id.to_string() != "error.reference.cycle"));
    assert_eq!(checked.terminal_status(), Some("status.invalid"));
}

#[test]
fn collection_member_types_are_invariant() {
    assert_eq!(
        check(&data_document("LIST[INTEGER]", "[1, 2, 3]")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&data_document("SET[STRING]", "[\"a\", \"b\"]")).outcome(),
        Outcome::Checked
    );
    // "Collection element types are invariant." A member incompatible with the
    // *declared* item type is `error.collection.heterogeneous`; each incompatible
    // member is an independent locus under `multiplicity_rule`.
    assert_eq!(
        ids(&check(&data_document("LIST[INTEGER]", "[1, \"two\"]"))),
        vec!["error.collection.heterogeneous"]
    );
    assert_eq!(
        ids(&check(&data_document("LIST[DECIMAL]", "[1, 2]"))),
        vec![
            "error.collection.heterogeneous",
            "error.collection.heterogeneous"
        ]
    );
}

#[test]
fn an_untyped_bracket_literal_needs_one_identical_member_type() {
    // "otherwise every member must have one identical static type; no
    // INTEGER-to-DECIMAL promotion occurs merely to make a collection
    // homogeneous."
    let mixed = format!(
        "{HEADER}\nDEFINE:\n    ID: constant.mixed\n    KIND: kind.constant\n    TYPE: LIST[INTEGER]\n    VALUE: [1, 2.5]\n"
    );
    assert!(!check(&mixed).diagnostics().is_empty());
}

#[test]
fn a_sentinel_is_never_a_material_collection_member() {
    // "MISSING and UNKNOWN are never material collection members." Each keeps
    // its own registered identifier: MISSING "has no storable user type", while
    // "A required material receiving site rejects UNKNOWN with
    // error.value.unknown".
    assert_eq!(
        ids(&check(&data_document("LIST[INTEGER]", "[1, MISSING]"))),
        vec!["error.type.mismatch"]
    );
    assert_eq!(
        ids(&check(&data_document("LIST[INTEGER]", "[1, UNKNOWN]"))),
        vec!["error.value.unknown"]
    );
}

#[test]
fn bare_object_is_a_receiving_family_and_bare_enum_is_not_a_type() {
    // "Bare OBJECT denotes an object family at a receiving contract, not an
    // additional nominal identity", and a schema-free object "infers each
    // present field's exact static type from its value".
    let object = format!(
        "{HEADER}\nDATA:\n    ID: data.record\n    TYPE: OBJECT\n    VALUE:\n        level: \"general\"\n        count: 3\n"
    );
    let checked = check(&object);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));

    // "Bare ENUM … cannot declare an unconstrained material value."
    assert_eq!(
        ids(&check(&data_document("ENUM", "ready"))),
        vec!["error.type.mismatch"]
    );
}

#[test]
fn an_object_schema_rejects_an_undeclared_field_and_a_missing_required_one() {
    let with_schema = |value: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: type.record\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: title\n        TYPE: STRING\n        REQUIRED: TRUE\n    FIELD:\n        NAME: count\n        TYPE: INTEGER\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.record\n    TYPE: OBJECT[REF(type.record)]\n    VALUE:\n{value}"
        )
    };
    assert_eq!(
        check(&with_schema("        title: \"a\"\n")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&with_schema("        title: \"a\"\n        count: 2\n")).outcome(),
        Outcome::Checked
    );
    // "no undeclared field is legal"
    assert_eq!(
        ids(&check(&with_schema(
            "        title: \"a\"\n        extra: 1\n"
        ))),
        vec!["error.object.schema"]
    );
    // "every required schema field must occur exactly once"
    assert_eq!(
        ids(&check(&with_schema("        count: 2\n"))),
        vec!["error.object.schema"]
    );
    // A declared field still has an exact type.
    assert_eq!(
        ids(&check(&with_schema("        title: 5\n"))),
        vec!["error.type.mismatch"]
    );
}

#[test]
fn object_types_are_structural() {
    // "recursively equal field-name/type/requiredness maps identify the same
    // object type regardless of defining ID or declaration order."
    let source = format!(
        "{HEADER}\nDEFINE:\n    ID: type.one\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: a\n        TYPE: STRING\n        REQUIRED: TRUE\n\nDEFINE:\n    ID: type.two\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: a\n        TYPE: STRING\n        REQUIRED: TRUE\n\nDATA:\n    ID: data.one\n    TYPE: OBJECT[REF(type.one)]\n    VALUE:\n        a: \"x\"\n"
    );
    let checked = check(&source);
    assert_eq!(checked.outcome(), Outcome::Checked, "{:?}", ids(&checked));

    let one = checked
        .declaration_types()
        .find(|(_, ty)| matches!(ty, lcl_checker::Type::Object(_)))
        .map(|(_, ty)| ty.clone());
    let types: Vec<_> = checked
        .declaration_types()
        .filter(|(_, ty)| matches!(ty, lcl_checker::Type::Object(_)))
        .map(|(_, ty)| ty.clone())
        .collect();
    assert!(one.is_some());
    assert!(
        types.windows(2).all(|pair| pair[0] == pair[1]),
        "two identical field maps are the same object type"
    );
}
