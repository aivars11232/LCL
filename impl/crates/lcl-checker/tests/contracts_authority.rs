//! The loaded contracts are the registry's, not this build's.
//!
//! Every assertion here re-derives its expectation from the canonical package
//! at test time. A test that hard-coded a count would pass against a drifted
//! registry, which is the failure mode these tests exist to catch.

mod common;

use lcl_checker::{Contracts, StaticError, DEFERRED};
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_spec::json::Json;

fn registry(name: &str) -> &'static Json {
    common::spec()
        .registry(name)
        .expect("registry is present in the approved package")
}

#[test]
fn the_mirrored_error_set_is_exactly_the_registered_static_stage_set() {
    let diagnostics = DiagnosticRegistry::load(common::spec()).expect("registry loads");
    let mut registered: Vec<String> = diagnostics
        .errors_by_stage(Stage::StaticOrExpression)
        .into_iter()
        .map(|e| e.id.clone())
        .collect();
    registered.sort();

    let mut mirrored: Vec<String> = StaticError::ALL
        .into_iter()
        .map(|e| e.as_registry_str().to_string())
        .collect();
    mirrored.sort();

    assert_eq!(
        mirrored, registered,
        "the build must mirror exactly the registered static_or_expression identifiers"
    );
}

#[test]
fn every_mirrored_identifier_round_trips_through_its_registry_spelling() {
    for id in StaticError::ALL {
        assert_eq!(
            StaticError::from_registry_str(id.as_registry_str()),
            Some(id)
        );
    }
    assert_eq!(StaticError::from_registry_str("error.required.missing"), None);
}

#[test]
fn every_identifier_carries_the_registrys_own_metadata() {
    let diagnostics = DiagnosticRegistry::load(common::spec()).expect("registry loads");
    for id in StaticError::ALL {
        let registered = diagnostics
            .error(id.as_registry_str())
            .expect("identifier is registered");
        let loaded = common::contracts().error(id);
        assert_eq!(loaded.meaning, registered.meaning);
        assert_eq!(loaded.default_status, registered.default_status);
        assert_eq!(registered.stage, Stage::StaticOrExpression);
        assert!(
            !loaded.default_status.is_empty(),
            "{id} must carry a registered default status"
        );
    }
}

#[test]
fn deferred_identifiers_are_named_with_the_layer_that_owns_them() {
    for (id, owner) in DEFERRED {
        assert!(
            StaticError::ALL.contains(id),
            "a deferred identifier must be a registered static identifier"
        );
        assert!(!owner.is_empty(), "{id} must name its owning layer");
        assert!(id.is_deferred());
    }
    let emitted: Vec<StaticError> = StaticError::emitted().collect();
    assert_eq!(
        emitted.len(),
        StaticError::ALL.len() - DEFERRED.len(),
        "emitted and deferred must partition the registered set"
    );
}

#[test]
fn an_unverified_package_is_refused() {
    let unverified =
        lcl_spec::SpecPackage::open_unverified(common::canonical_root()).expect("opens");
    assert!(!unverified.is_authoritative());
    let err = Contracts::load(&unverified).expect_err("must refuse");
    assert!(
        format!("{err}").contains("only the approved release is normative input"),
        "unexpected: {err}"
    );
}

#[test]
fn every_registered_operator_row_is_loaded_and_readable() {
    let registry_rows = registry("operators_and_functions")
        .get("operators")
        .and_then(Json::as_object)
        .expect("operators present");
    let contracts = common::contracts();
    assert_eq!(contracts.operators().count(), registry_rows.len());

    for (name, row) in registry_rows {
        let loaded = contracts
            .operator(name)
            .unwrap_or_else(|| panic!("operator {name} must be loaded"));
        assert_eq!(
            loaded.arity as u64,
            row.get("arity").and_then(Json::as_u64).expect("arity"),
            "operator {name} arity"
        );
        assert!(
            !loaded.overloads.is_empty(),
            "operator {name} must register at least one operand tuple"
        );
        for overload in &loaded.overloads {
            assert_eq!(
                overload.parameters.len(),
                loaded.arity,
                "operator {name}: every overload has exactly its registered arity"
            );
        }
    }
}

#[test]
fn every_registered_function_row_is_loaded_and_readable() {
    let registry_rows = registry("operators_and_functions")
        .get("functions")
        .and_then(Json::as_object)
        .expect("functions present");
    let contracts = common::contracts();
    assert_eq!(contracts.functions().count(), registry_rows.len());

    for (name, _) in registry_rows {
        let loaded = contracts
            .function(name)
            .unwrap_or_else(|| panic!("function {name} must be loaded"));
        assert!(
            !loaded.overloads.is_empty(),
            "function {name} must register at least one parameter list"
        );
        assert!(
            !loaded.arities().is_empty(),
            "function {name} must register at least one arity"
        );
    }
}

#[test]
fn every_registered_constructor_row_is_loaded_and_readable() {
    let registry_rows = registry("operators_and_functions")
        .get("constructors")
        .and_then(Json::as_object)
        .expect("constructors present");
    let contracts = common::contracts();
    assert_eq!(contracts.constructors().count(), registry_rows.len());

    for (name, row) in registry_rows {
        let loaded = contracts
            .constructor(name)
            .unwrap_or_else(|| panic!("constructor {name} must be loaded"));
        assert_eq!(
            loaded.result.family(),
            row.get("result").and_then(Json::as_str).expect("result"),
            "constructor {name} result family"
        );
        assert_eq!(
            loaded.overloads.len(),
            row.get("overloads")
                .and_then(Json::as_array)
                .expect("overloads")
                .len(),
            "constructor {name} overload count"
        );
    }
}

#[test]
fn every_operation_contract_is_loaded_with_a_parsed_target_and_parameters() {
    let registry_rows = registry("operations")
        .get("contracts")
        .and_then(Json::as_object)
        .expect("contracts present");
    let contracts = common::contracts();
    assert_eq!(contracts.operations().count(), registry_rows.len());

    for (id, row) in registry_rows {
        let loaded = contracts
            .operation(id)
            .unwrap_or_else(|| panic!("operation {id} must be loaded"));
        assert_eq!(
            loaded.target.required,
            row.get("target")
                .and_then(|t| t.get("required"))
                .and_then(Json::as_bool)
                .unwrap_or(false),
            "operation {id} target requiredness"
        );
        assert_eq!(
            loaded.parameters.len(),
            row.get("parameters")
                .and_then(Json::as_object)
                .map(<[(String, Json)]>::len)
                .unwrap_or(0),
            "operation {id} parameter count"
        );
        assert!(
            !loaded.positional_parameters,
            "operation {id}: `No positional parameters exist.`"
        );
    }
}

#[test]
fn core_sort_registers_exactly_key_and_direction() {
    // `21_SORT_STABLE_PARAMETER.invalid.lcl` turns on this exact fact: "core.sort
    // has exactly key and direction parameters; stable and comparator are
    // unregistered".
    let sort = common::contracts()
        .operation("core.sort")
        .expect("core.sort is a core operation");
    let names: Vec<&str> = sort.parameters.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["direction", "key"]);
    assert!(sort.target.required);
}

#[test]
fn the_ordered_type_profile_is_the_registrys() {
    let rows = registry("operators_and_functions")
        .get("ordered_types")
        .and_then(Json::as_array)
        .expect("ordered_types present");
    let contracts = common::contracts();
    for row in rows {
        let family = row
            .as_str()
            .expect("ordered type row is a string")
            .split('[')
            .next()
            .expect("non-empty")
            .trim();
        assert!(
            contracts.is_ordered_family(family),
            "{family} is registered as ordered"
        );
    }
    // Families outside the profile must not acquire an order.
    for family in ["BOOLEAN", "NULL", "LIST", "SET", "OBJECT", "REFERENCE"] {
        assert!(
            !contracts.is_ordered_family(family),
            "{family} has no registered total order"
        );
    }
}

#[test]
fn units_formats_and_encodings_come_from_the_registry() {
    let reg = registry("formats_encodings_units");
    let contracts = common::contracts();

    let units = reg.get("units").and_then(Json::as_object).expect("units");
    assert_eq!(contracts.unit_count(), units.len());
    for (id, _) in units {
        assert!(contracts.is_unit(id), "{id} must be a registered unit");
    }
    assert!(!contracts.is_unit("unit.furlong"));

    for (id, _) in reg.get("formats").and_then(Json::as_object).expect("formats") {
        assert!(contracts.is_format(id));
    }
    for (id, _) in reg
        .get("encodings")
        .and_then(Json::as_object)
        .expect("encodings")
    {
        assert!(contracts.is_encoding(id));
    }

    // The Time category is what DURATION narrows to; it is read, not assumed.
    assert!(contracts.unit_in_category("unit.second", "Time"));
    assert!(!contracts.unit_in_category("unit.meter", "Time"));
}

#[test]
fn the_three_valued_logic_and_promotion_tables_are_loaded() {
    let reg = registry("operators_and_functions");
    let contracts = common::contracts();

    let logic = reg
        .get("unknown_logic")
        .and_then(Json::as_object)
        .expect("unknown_logic");
    assert_eq!(contracts.unknown_logic_rows(), logic.len());
    for (expression, expected) in logic {
        assert_eq!(
            contracts.unknown_logic(expression),
            expected.as_str(),
            "row {expression}"
        );
    }

    for (left, right) in [
        ("INTEGER", "INTEGER"),
        ("INTEGER", "DECIMAL"),
        ("DECIMAL", "INTEGER"),
        ("DECIMAL", "DECIMAL"),
    ] {
        assert!(
            contracts.numeric_promotion(left, right).is_some(),
            "{left}+{right} must be registered"
        );
    }
}

#[test]
fn every_block_field_value_kind_is_loaded_verbatim() {
    let blocks = registry("field_signatures")
        .get("blocks")
        .and_then(Json::as_object)
        .expect("blocks");
    let contracts = common::contracts();
    for (block, schema) in blocks {
        let Some(fields) = schema.get("fields").and_then(Json::as_object) else {
            continue;
        };
        for (field, signature) in fields {
            assert_eq!(
                contracts.value_kind(block, field),
                signature.get("value_kind").and_then(Json::as_str),
                "{block}.{field} value kind"
            );
            assert_eq!(
                contracts.field_is_required(block, field),
                signature
                    .get("required")
                    .and_then(Json::as_bool)
                    .unwrap_or(false),
                "{block}.{field} requiredness"
            );
        }
    }
}

#[test]
fn the_built_in_type_rows_are_the_registrys() {
    let rows = registry("types")
        .get("types")
        .and_then(Json::as_object)
        .expect("types");
    assert_eq!(common::contracts().type_row_count(), rows.len());
    for (row, _) in rows {
        assert!(common::contracts().is_type_row(row));
    }
}
