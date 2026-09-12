//! A3: checked collection families survive the preflight boundary.
//! Authority: 03_TYPES_AND_VALUES/03 and /10; collections evaluate all
//! members before strict-equal SET duplicates collapse.

mod common;

use common::*;
use lcl_resolver::MemoryProvider;
use lcl_semantics::{Invocation, Origin, Planned, Value};

const SUBJECT: &str = "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"subject\"\n";

fn value(planned: &Planned, id: &str) -> Value {
    planned
        .partial_plan()
        .resolutions()
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("missing resolution {id}"))
        .value
        .clone()
}

fn datum(ty: &str, body: &str) -> Value {
    let source = task_document(&format!(
        "{SUBJECT}\nDATA:\n    ID: data.collection\n    TYPE: {ty}\n    VALUE: {body}\n"
    ));
    value(&plan(&source), "data.collection")
}

fn integers(values: &[u64]) -> Vec<Value> {
    values
        .iter()
        .map(|v| {
            Value::Integer(lcl_checker::numeric::Decimal::from_integer(
                lcl_checker::numeric::Integer::from_u64(*v),
            ))
        })
        .collect()
}

#[test]
fn inline_and_multiline_sets_collapse_duplicates_but_lists_keep_them() {
    for body in ["[3, 1, 1]", "[\n        3,\n        1,\n        1\n    ]"] {
        assert_eq!(
            datum("LIST[INTEGER]", body),
            Value::List(integers(&[3, 1, 1]))
        );
        assert_eq!(datum("SET[INTEGER]", body), Value::Set(integers(&[3, 1])));
    }
}

#[test]
fn empty_collections_keep_the_exact_receiving_family() {
    assert_eq!(datum("LIST[INTEGER]", "[]"), Value::List(vec![]));
    assert_eq!(datum("SET[INTEGER]", "[]"), Value::Set(vec![]));
}

#[test]
fn nested_set_equality_ignores_member_order_while_nested_lists_keep_order() {
    assert_eq!(
        datum("SET[SET[INTEGER]]", "[[3, 1, 1], [1, 3]]"),
        Value::Set(vec![Value::Set(integers(&[3, 1]))])
    );
    assert_eq!(
        datum("SET[LIST[INTEGER]]", "[[3, 1], [1, 3], [3, 1]]"),
        Value::Set(vec![
            Value::List(integers(&[3, 1])),
            Value::List(integers(&[1, 3]))
        ])
    );
}

#[test]
fn constant_and_default_use_the_same_typed_collection_path() {
    for body in ["[3, 1, 1]", "[\n        3,\n        1,\n        1\n    ]"] {
        let source = task_document(&format!(
            "{SUBJECT}\nDEFINE:\n    ID: constant.collection\n    KIND: kind.constant\n    TYPE: SET[INTEGER]\n    VALUE: {body}\n\nINPUT:\n    ID: input.collection\n    TYPE: SET[INTEGER]\n    REQUIRED: FALSE\n    DEFAULT: {body}\n"
        ));
        let planned = plan(&source);
        for id in ["constant.collection", "input.collection"] {
            assert_eq!(value(&planned, id), Value::Set(integers(&[3, 1])), "{id}");
        }
        let supplied = plan_with(
            &source,
            &Invocation::new().with("input.collection", Value::Set(integers(&[9]))),
        );
        assert_eq!(
            value(&supplied, "input.collection"),
            Value::Set(integers(&[9]))
        );
        assert!(supplied
            .partial_plan()
            .resolutions()
            .iter()
            .any(|r| r.id == "input.collection" && r.origin == Origin::Supplied));
        let unknown = plan_with(
            &source,
            &Invocation::new().with("input.collection", Value::Unknown),
        );
        assert_eq!(value(&unknown, "input.collection"), Value::Unknown);
    }
}

#[test]
fn normalized_duration_and_temporal_duplicates_collapse() {
    for (ty, body) in [
        (
            "DURATION",
            "[DURATION(1, unit.minute), DURATION(60, unit.second)]",
        ),
        ("TIME", "[TIME(\"10:00:00+01:00\"), TIME(\"09:00:00Z\")]"),
        (
            "DATETIME",
            "[DATETIME(\"2026-09-12T10:00:00+01:00\"), DATETIME(\"2026-09-12T09:00:00Z\")]",
        ),
    ] {
        let Value::Set(members) = datum(&format!("SET[{ty}]"), body) else {
            panic!("{ty}: checked SET was not materialized as SET");
        };
        assert_eq!(members.len(), 1, "{ty}: canonical normalized equality");
        assert_eq!(
            datum(&format!("LIST[{ty}]"), body).members().unwrap().len(),
            2
        );
    }
}

#[test]
fn reference_members_retain_identity_without_reading_referents() {
    let source = task_document(&format!(
        "{SUBJECT}\nDEFINE:\n    ID: type.item\n    KIND: kind.type\n    BASE: INTEGER\n\nDATA:\n    ID: data.collection\n    TYPE: SET[REFERENCE[REF(type.item)]]\n    VALUE: [REF(type.item), REF(type.item)]\n"
    ));
    assert_eq!(
        value(&plan(&source), "data.collection"),
        Value::Set(vec![Value::Reference("type.item".into())])
    );
}

#[test]
fn imported_collections_use_the_declaring_source_annotations() {
    let library = format!("{LIBRARY}\nDATA:\n    ID: data.collection\n    TYPE: SET[SET[INTEGER]]\n    VALUE: [[3, 1, 1], [1, 3]]\n");
    let root = task_document(&format!(
        "\nIMPORT:\n    ID: import.lib\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n{SUBJECT}"
    ));
    let mut provider = MemoryProvider::new();
    provider.insert("lib.lcl", library.into_bytes());
    let resolved = resolve_with(&root, provider);
    assert!(
        resolved.diagnostics().is_empty(),
        "{:?}",
        resolved.diagnostics()
    );
    let checked = checker().check(&resolved).unwrap();
    assert!(
        checked.diagnostics().is_empty(),
        "{:?}",
        checked.diagnostics()
    );
    assert!(checked.earlier_stage_defects().is_empty());
    let planned = preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .unwrap();
    assert_eq!(
        value(&planned, "lib.data.collection"),
        Value::Set(vec![Value::Set(integers(&[3, 1]))])
    );
}

#[test]
fn a_later_invalid_member_is_not_hidden_by_an_earlier_duplicate() {
    let source = task_document(&format!(
        "{SUBJECT}\nDATA:\n    ID: data.collection\n    TYPE: SET[DECIMAL]\n    VALUE: [1.0, 1.0, 1.0 / 0.0]\n"
    ));
    let resolved = resolve(&source);
    assert!(resolved.diagnostics().is_empty());
    let checked = checker().check(&resolved).unwrap();
    assert!(checked
        .diagnostics()
        .iter()
        .any(|d| d.id.to_string() == "error.numeric.division_by_zero"));
    assert!(preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .is_err());
}

#[test]
fn multiline_members_require_the_canonical_comma_separator() {
    let source = task_document(&format!(
        "{SUBJECT}\nDATA:\n    ID: data.collection\n    TYPE: SET[INTEGER]\n    VALUE: [\n        3\n        1\n    ]\n"
    ));
    let error = resolver()
        .resolve(&unit("root.lcl", &source), &MemoryProvider::new())
        .expect_err("MULTILINE_COLLECTION requires comma before the member newline");
    assert_eq!(error.primary, "error.grammar.invalid");
}

#[test]
fn a_value_reference_preserves_the_set_in_either_declaration_order() {
    let original = data_block("data.original", "SET[INTEGER]", "[3, 1, 1]");
    let copied = data_block("data.copied", "SET[INTEGER]", "REF(data.original)");
    for declarations in [format!("{original}{copied}"), format!("{copied}{original}")] {
        let planned = plan(&task_document(&format!("{SUBJECT}{declarations}")));
        assert_eq!(
            value(&planned, "data.copied"),
            Value::Set(integers(&[3, 1]))
        );
    }
}

#[test]
fn an_object_schema_supplies_the_exact_multiline_set_property_type() {
    let source = task_document(&format!(
        "{SUBJECT}\nDEFINE:\n    ID: type.record\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: items\n        TYPE: SET[INTEGER]\n        REQUIRED: TRUE\n\nDATA:\n    ID: data.record\n    TYPE: REF(type.record)\n    VALUE:\n        items: [\n            3,\n            1,\n            1\n        ]\n"
    ));
    let Value::Object(fields) = value(&plan(&source), "data.record") else {
        panic!("the declared record must be an object");
    };
    assert_eq!(fields.get("items"), Some(&Value::Set(integers(&[3, 1]))));
}
