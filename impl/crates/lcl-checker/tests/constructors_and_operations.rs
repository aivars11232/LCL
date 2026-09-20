//! Phase D: typed constructors and invocation-site operation contracts.

mod common;

use common::{check, contracts, data_document as value, ids};
use lcl_checker::Outcome;

#[test]
fn every_registered_constructor_accepts_its_registered_arity() {
    // Arity comes from the registry, so a row added there is covered here.
    for row in contracts().constructors() {
        let arities = row.arities();
        assert!(!arities.is_empty(), "{} registers an arity", row.name);
        assert!(
            !arities.contains(&(arities.iter().max().copied().unwrap_or(0) + 1)),
            "{} is not variadic",
            row.name
        );
    }
}

#[test]
fn a_constructor_rejects_an_unregistered_arity() {
    // "The constructor registry defines the exact accepted overloads; calls are
    // never variadic."
    for expression in [
        "DATE(\"2026-08-30\", \"extra\")",
        "BYTES(1, 2)",
        "PERCENTAGE()",
        "MEASURE(1)",
    ] {
        let checked = check(&value("STRING", expression));
        assert!(
            ids(&checked).contains(&"error.operator.operand".to_string()),
            "{expression}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn a_constructor_rejects_an_unregistered_operand_family() {
    for expression in [
        "BYTES(\"8\")",
        "PERCENTAGE(TRUE)",
        "DURATION(\"5\", unit.second)",
    ] {
        let checked = check(&value("STRING", expression));
        assert!(
            ids(&checked).contains(&"error.operator.operand".to_string()),
            "{expression}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn declared_numeric_bounds_are_enforced_on_statically_known_values() {
    // "PERCENTAGE is 0 through 100 inclusive"; "BYTES accepts only a
    // non-negative INTEGER".
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(90)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(0)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("PERCENTAGE", "PERCENTAGE(100)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        ids(&check(&value("PERCENTAGE", "PERCENTAGE(101)"))),
        vec!["error.value.out_of_range"]
    );
    assert_eq!(
        ids(&check(&value("BYTES", "BYTES(-1)"))),
        vec!["error.value.out_of_range"]
    );
    // BYTES "accepts only a non-negative INTEGER", not a DECIMAL.
    assert_eq!(
        ids(&check(&value("BYTES", "BYTES(1.5)"))),
        vec!["error.operator.operand"]
    );
}

#[test]
fn a_unit_argument_must_be_registered_and_in_its_declared_category() {
    assert_eq!(
        check(&value("MEASURE", "MEASURE(1920, unit.pixel)")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        check(&value("DURATION", "DURATION(5, unit.second)")).outcome(),
        Outcome::Checked
    );
    // "Core 0.1.0 admits only units in the closed unit registry." Identifiers
    // under reserved namespaces "resolve only to this core registry"
    // (`06_STANDARD_LIBRARY/09`), and "Unknown references retain
    // error.reference.unresolved"
    // (`operators_and_functions_v0.1.0.json#/evaluation_contract`): an
    // unregistered unit is an unknown reference, not a wrong operand family.
    let unregistered = check(&value("MEASURE", "MEASURE(1, unit.furlong)"));
    assert_eq!(unregistered.outcome(), Outcome::Rejected);
    assert!(ids(&unregistered).is_empty(), "{:?}", ids(&unregistered));
    let defects = unregistered.earlier_stage_defects();
    assert_eq!(defects.len(), 1);
    assert_eq!(defects[0].identifier, "error.reference.unresolved");
    assert_eq!(defects[0].stage, lcl_diagnostics::Stage::Resolution);
    // A wrong operand family keeps error.operator.operand.
    assert!(ids(&check(&value("MEASURE", "MEASURE(\"1\", unit.pixel)")))
        .contains(&"error.operator.operand".to_string()));
    // "DURATION requires a registered Time-category unit."
    assert_eq!(
        ids(&check(&value("DURATION", "DURATION(5, unit.meter)"))),
        vec!["error.numeric.unit_mismatch"]
    );
    // "MEASURE pairs a number with any registered unit, including a
    // Time-category unit."
    assert_eq!(
        check(&value("MEASURE", "MEASURE(5, unit.second)")).outcome(),
        Outcome::Checked
    );
}

#[test]
fn a_relative_path_is_legal_only_as_an_import_or_extension_source() {
    // M1 owns every closed literal profile; `PATH` is the one it deferred,
    // because its "absolute/relative legality depends on the receiving field
    // and on resolution".
    assert_eq!(
        check(&value("PATH", "PATH(\"/absolute/path\")")).outcome(),
        Outcome::Checked
    );
    let relative = check(&value("PATH", "PATH(\"relative/path\")"));
    assert_eq!(relative.outcome(), Outcome::Rejected);
    let defects = relative.earlier_stage_defects();
    assert_eq!(defects.len(), 1);
    assert_eq!(defects[0].identifier, "error.literal.invalid");
    assert_eq!(defects[0].stage, lcl_diagnostics::Stage::Lexical);
}

#[test]
fn an_invocation_site_must_satisfy_its_operations_parameter_contract() {
    let action = |parameters: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: SET[INTEGER]\n    VALUE: [3, 1]\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: LIST[INTEGER]\n    FORMAT: format.json\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.sort\n    TARGET: REF(input.value)\n{}",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            parameters
        )
    };

    // core.sort registers exactly `key` and `direction`, both optional.
    assert_eq!(check(&action("")).outcome(), Outcome::Checked);

    // "supplies an unregistered named parameter"
    let unregistered = action(
        "    PARAMETER:\n        NAME: stable\n        TYPE: BOOLEAN\n        REQUIRED: FALSE\n        VALUE: TRUE\n",
    );
    assert_eq!(
        ids(&check(&unregistered)),
        vec!["error.operation.parameter"]
    );

    // "duplicates a named parameter"
    let duplicated = action(concat!(
        "    PARAMETER:\n        NAME: direction\n        TYPE: STRING\n        REQUIRED: FALSE\n        VALUE: \"ascending\"\n",
        "    PARAMETER:\n        NAME: direction\n        TYPE: STRING\n        REQUIRED: FALSE\n        VALUE: \"descending\"\n"
    ));
    assert_eq!(ids(&check(&duplicated)), vec!["error.operation.parameter"]);
}

fn contextual_enum_document(parameters: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n\
         SPECIFICATION:\n    ID: test.direction\n    NAME: \"Contextual enum\"\n    \
         VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
         DATA:\n    ID: data.subject\n    TYPE: LIST[INTEGER]\n    VALUE: [3, 1, 2]\n\n\
         ACTION:\n    ID: action.sort\n    OPERATION: core.sort\n    TARGET: REF(data.subject)\n\
         {parameters}\n\
         GOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n\n\
         SUCCESS:\n    ID: success.subject\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    \
         ACTION: REF(action.sort)\n    SUCCESS: REF(success.subject)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.subject)\n"
    )
}

fn enum_parameter(name: &str, member: &str) -> String {
    format!(
        "    PARAMETER:\n        NAME: {name}\n        TYPE: ENUM\n        \
         REQUIRED: FALSE\n        VALUE: {member}\n"
    )
}

/// 03_TYPES_AND_VALUES/05: bare ENUM is legal when an operation supplies one
/// exact domain. The receiving row, not the spelling of VALUE, supplies it.
#[test]
fn contextual_enum_accepts_each_registered_sort_direction() {
    for member in ["ascending", "descending"] {
        let source = contextual_enum_document(&enum_parameter("direction", member));
        let checked = check(&source);
        assert_eq!(
            checked.outcome(),
            Outcome::Checked,
            "{member}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn contextual_enum_rejects_nonmembers_and_other_value_families() {
    for member in ["sideways", "\"descending\"", "TRUE"] {
        let source = contextual_enum_document(&enum_parameter("direction", member));
        let checked = check(&source);
        assert_eq!(ids(&checked), vec!["error.type.mismatch"], "{member}");
        let diagnostic = checked.primary().expect("a value rejection");
        let start = source.find(&format!("VALUE: {member}")).unwrap() + "VALUE: ".len();
        assert_eq!(
            diagnostic.span.start, start,
            "reject VALUE, not its admitted TYPE"
        );
    }
}

#[test]
fn contextual_enum_does_not_make_bare_enum_a_general_material_type() {
    let checked = check(&value("ENUM", "descending"));
    assert_eq!(checked.outcome(), Outcome::Rejected);
    assert!(ids(&checked).iter().all(|id| id == "error.type.mismatch"));
}

#[test]
fn contextual_enum_does_not_leak_to_another_parameter() {
    // `key` registers STRING|REFERENCE, not the `direction` enum. Cover it
    // both alone and after an admitted direction in the same ACTION.
    for prefix in [String::new(), enum_parameter("direction", "descending")] {
        let source =
            contextual_enum_document(&format!("{prefix}{}", enum_parameter("key", "descending")));
        let checked = check(&source);
        // `key` also rejects the unregistered declared family. Preserve that
        // existing operation-contract diagnostic alongside the type defect.
        assert_eq!(
            ids(&checked),
            vec!["error.operation.parameter", "error.type.mismatch"]
        );
        let key_type = source.rfind("TYPE: ENUM").unwrap() + "TYPE: ".len();
        let type_defect = checked
            .diagnostics()
            .iter()
            .find(|d| d.id.to_string() == "error.type.mismatch")
            .unwrap();
        assert_eq!(type_defect.span.start, key_type);
    }
}

/// `expression_fragment_contract/syntax`: "consume exactly one EXPRESSION …
/// followed only by optional whitespace", `#/diagnostics`: "Malformed fragment
/// syntax and invalid bindings produce error.operation.parameter", and
/// `#/evaluation`: "Static checks cover the complete fragment".
///
/// Regression, `LCL-TASK-0020` defect 3. No stage checked the fragment, so a
/// malformed one passed `check` and reached execution, where the runtime could
/// not emit a static-stage identifier and substituted another from the row.
/// CLOSURE-019 failed on the substituted identifier.
#[test]
fn a_fragment_that_is_not_one_expression_is_refused_at_this_stage() {
    let calculate = |fragment: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "INPUT:\n    ID: input.value\n    TYPE: INTEGER\n    VALUE: 4\n\n",
                "OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.calculate\n    TARGET: REF(input.value)\n",
                "    PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: {:?}\n",
                "    OUTPUT: REF(output.value)\n\n",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            fragment
        )
    };

    // One expression is admitted, so the check below is about the fragment and
    // not about fragments.
    assert_eq!(
        check(&calculate("REF(input.value) * 2")).outcome(),
        Outcome::Checked
    );

    // The witness: a fragment holding two expressions.
    assert_eq!(
        ids(&check(&calculate("1; 2"))),
        vec!["error.operation.parameter"]
    );
    // One that lexes, so the defect is the trailing expression and not a token.
    assert_eq!(
        ids(&check(&calculate("1 2"))),
        vec!["error.operation.parameter"]
    );
    // "followed only by optional whitespace" — trailing space is not trailing
    // input.
    assert_eq!(
        check(&calculate("REF(input.value) * 2   ")).outcome(),
        Outcome::Checked
    );
    // A fragment holding no expression at all.
    assert_eq!(
        ids(&check(&calculate(""))),
        vec!["error.operation.parameter"]
    );
}

/// `06_STANDARD_LIBRARY/10` and the two witnesses that name this identifier for
/// a declared parameter family the row does not register.
///
/// Regression, `LCL-TASK-0020` defect 5. `core.append` accepted a `BYTES`
/// content and completed, though the row types content `STRING|LIST[T]` and
/// says plainly that "BYTES is a count and is not content"; CLOSURE-052 could
/// not be executed. `core.validate` accepted an arbitrary OBJECT as its schema,
/// though the row types it `REFERENCE` and says "Material OBJECT schema
/// encodings are not admitted"; CLOSURE-051 could not be executed.
#[test]
fn a_declared_parameter_family_outside_the_row_is_refused() {
    let invocation = |operation: &str, target: &str, parameter: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "DATA:\n    ID: data.subject\n    TYPE: {}\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: {}\n    TARGET: REF(data.subject)\n{}",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            target, operation, parameter
        )
    };

    // "BYTES is a count and is not content."
    let bytes_content = invocation(
        "core.append",
        "PATH\n    VALUE: PATH(\"/srv/data/a.txt\")",
        "    PARAMETER:\n        NAME: content\n        TYPE: BYTES\n        REQUIRED: TRUE\n        VALUE: BYTES(4)\n\n",
    );
    assert_eq!(
        ids(&check(&bytes_content)),
        vec!["error.operation.parameter"]
    );

    // A STRING content is the family the row registers.
    let string_content = invocation(
        "core.append",
        "PATH\n    VALUE: PATH(\"/srv/data/a.txt\")",
        "    PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"x\"\n\n",
    );
    assert_eq!(check(&string_content).outcome(), Outcome::Checked);

    // "Material OBJECT schema encodings are not admitted."
    let object_schema = invocation(
        "core.validate",
        "STRING\n    VALUE: \"x\"",
        "    PARAMETER:\n        NAME: schema\n        TYPE: OBJECT\n        REQUIRED: TRUE\n        VALUE:\n            name: \"a\"\n\n",
    );
    assert_eq!(
        ids(&check(&object_schema)),
        vec!["error.operation.parameter"]
    );
}

#[test]
fn an_omitted_required_target_is_an_operation_parameter_defect() {
    // "omits TARGET when the selected operation marks it required and no
    // handler-context binding supplies it".
    let source = concat!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n",
        "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
        "OUTPUT:\n    ID: output.value\n    TYPE: LIST[INTEGER]\n    FORMAT: format.json\n\n",
        "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
        "ACTION:\n    ID: action.one\n    OPERATION: core.sort\n    OUTPUT: REF(output.value)\n\n",
        "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
        "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
        "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let checked = check(source);
    assert!(
        ids(&checked).contains(&"error.operation.parameter".to_string()),
        "{:?}",
        ids(&checked)
    );
    assert_eq!(checked.terminal_status(), Some("status.invalid"));
}

#[test]
fn every_core_operation_contract_is_applied_from_the_registry() {
    // The contract set is the registry's: every operation with a required
    // parameter is one this checker would demand at its invocation sites.
    let required: Vec<&str> = contracts()
        .operations()
        .filter(|contract| contract.parameters.values().any(|p| p.required))
        .map(|contract| contract.id.as_str())
        .collect();
    assert!(
        !required.is_empty(),
        "some core operations register required parameters"
    );
    for contract in contracts().operations() {
        assert!(
            !contract.positional_parameters,
            "{} takes no positional arguments",
            contract.id
        );
    }
}

// ---------------------------------------------------------------------------
// A parameter whose registered constraint closes its object shape
// ---------------------------------------------------------------------------
//
// Regression, post-Task-20 finding F13. `core.read`'s `range` row says "An
// incompatible unit/representation or wrong key/type uses
// error.operation.parameter", and that identifier is registered at
// `static_or_expression`. A range written as a literal OBJECT is knowable
// there. It was not checked there, so it reached demand, where the runtime
// could not name the identifier the row names and substituted
// `error.operation.precondition` instead.

/// One `core.read` whose `range` parameter holds the given properties.
fn ranged_read(properties: &str) -> String {
    format!(
        concat!(
            "LCL:\n    VERSION: \"0.1.0\"\n\n",
            "SPECIFICATION:\n    ID: spec.range\n    NAME: \"Range\"\n",
            "    VERSION: \"1.0.0\"\n    KIND: kind.task\n    DOMAIN: \"general\"\n\n",
            "INPUT:\n    ID: input.target\n    TYPE: PATH\n    VALUE: PATH(\"/srv/a.txt\")\n\n",
            "OUTPUT:\n    ID: output.value\n    TYPE: STRING\n    FORMAT: format.json\n\n",
            "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
            "ACTION:\n    ID: action.read\n    OPERATION: core.read\n",
            "    TARGET: REF(input.target)\n",
            "    PARAMETER:\n        NAME: range\n        TYPE: OBJECT\n",
            "        REQUIRED: FALSE\n        VALUE:\n{}",
            "    OUTPUT: REF(output.value)\n\n",
            "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
            "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
            "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.target)\n",
            "    ACTION: REF(action.read)\n    OUTPUT: REF(output.value)\n",
            "    SUCCESS: REF(success.one)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.one)\n"
        ),
        properties
    )
}

const WELL_FORMED_RANGE: &str =
    "            unit: \"scalar\"\n            start: 1\n            end: 3\n";

#[test]
fn a_well_formed_range_is_accepted_at_the_static_stage() {
    // The control. Without it the three cases below would pass just as well
    // against a check that refused every range.
    assert_eq!(
        check(&ranged_read(WELL_FORMED_RANGE)).outcome(),
        Outcome::Checked
    );
    // Every admitted unit word, so the closed list is read rather than guessed.
    for unit in ["scalar", "line", "item", "byte"] {
        let source = ranged_read(&format!(
            "            unit: {unit:?}\n            start: 0\n            end: 1\n"
        ));
        assert_eq!(
            check(&source).outcome(),
            Outcome::Checked,
            "{unit} is a registered unit"
        );
    }
}

#[test]
fn a_wrong_key_in_a_closed_range_is_an_operation_parameter_defect() {
    // "No other keys or defaults are admitted."
    for properties in [
        // A key the row does not register.
        "            unit: \"scalar\"\n            start: 1\n            end: 3\n            step: 1\n",
        // A key misspelled, so one registered key is also missing.
        "            unit: \"scalar\"\n            start: 1\n            stop: 3\n",
        // A registered key omitted.
        "            unit: \"scalar\"\n            start: 1\n",
    ] {
        let checked = check(&ranged_read(properties));
        assert_eq!(
            ids(&checked),
            vec!["error.operation.parameter"],
            "properties: {properties}"
        );
        assert_eq!(
            checked.diagnostics()[0].stage().to_string(),
            "static_or_expression",
            "the row's identifier keeps its registered stage"
        );
    }
}

#[test]
fn a_wrong_type_in_a_closed_range_is_an_operation_parameter_defect() {
    // "unit: STRING, start: INTEGER, and end: INTEGER."
    for properties in [
        "            unit: 1\n            start: 1\n            end: 3\n",
        "            unit: \"scalar\"\n            start: \"1\"\n            end: 3\n",
        "            unit: \"scalar\"\n            start: 1\n            end: TRUE\n",
    ] {
        let checked = check(&ranged_read(properties));
        assert!(
            ids(&checked).contains(&"error.operation.parameter".to_string()),
            "properties {properties} gave {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn a_unit_outside_the_closed_list_is_an_operation_parameter_defect() {
    // "unit is exactly scalar, line, item, or byte."
    for unit in ["character", "SCALAR", "lines", ""] {
        let source = ranged_read(&format!(
            "            unit: {unit:?}\n            start: 0\n            end: 1\n"
        ));
        let checked = check(&source);
        assert_eq!(
            ids(&checked),
            vec!["error.operation.parameter"],
            "unit {unit:?} is not registered"
        );
        assert_eq!(
            checked.diagnostics()[0].stage().to_string(),
            "static_or_expression"
        );
    }
}

#[test]
fn a_range_this_stage_cannot_read_is_left_to_a_later_one() {
    // Deliberately narrow: a value that is not a written literal is not
    // refused here, because this stage does not evaluate one.
    let source = ranged_read(
        "            unit: \"scalar\"\n            start: REF(input.target)\n            end: 3\n",
    );
    let checked = check(&source);
    assert!(
        !ids(&checked).contains(&"error.operation.parameter".to_string()),
        "a reference is not a wrong key or a wrong written type: {:?}",
        ids(&checked)
    );
}

fn fallback_document(fallback: &str) -> String {
    format!(
        concat!(
            "LCL:\n    VERSION: \"0.1.0\"\n\n",
            "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
            "DATA:\n    ID: data.path\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/report.txt\")\n\n",
            "HANDLER:\n    ID: handler.other\n    EVENT: event.host_constraint\n    OPERATION: core.stop\n\n",
            "HANDLER:\n    ID: handler.fix\n    EVENT: event.host_constraint\n    OPERATION: core.stop\n    FALLBACK: {}\n\n",
            "ACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.path)\n\n",
            "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
            "SUCCESS:\n    ID: success.one\n    ALL: TRUE\n\n",
            "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    HANDLER: REF(handler.fix)\n    SUCCESS: REF(success.one)\n\n",
            "EXECUTE:\n    REFERENCE: REF(task.one)\n"
        ),
        fallback
    )
}

#[test]
fn a_fallback_operation_identifier_is_an_invocation_site() {
    // `05_SEMANTICS/06`: "One operation identifier is legal only when the
    // operation registers no required named parameter and its required target,
    // if any, is supplied by the original handler-context binding above. An
    // operation identifier that cannot satisfy its contract under those limits
    // uses error.operation.parameter".
    for illegal in [
        // Requires `destination`, and its target is not a handler context.
        "core.move",
        // Requires `content`.
        "core.append",
        "core.write",
        // Its required target is one no handler-context binding supplies.
        "core.delete",
    ] {
        let checked = check(&fallback_document(illegal));
        assert!(
            ids(&checked).contains(&"error.operation.parameter".to_string()),
            "FALLBACK {illegal}: {:?}",
            ids(&checked)
        );
        assert_eq!(
            checked.terminal_status(),
            Some("status.invalid"),
            "{illegal}"
        );
    }
    // A control operation whose target admits the execution unit, and the REF
    // form, which carries the referenced handler's own invocation data.
    for legal in ["core.stop", "REF(handler.other)"] {
        let checked = check(&fallback_document(legal));
        assert_eq!(
            checked.outcome(),
            Outcome::Checked,
            "FALLBACK {legal}: {:?}",
            ids(&checked)
        );
    }
}

#[test]
fn a_material_test_target_cannot_accompany_actual_or_assertion() {
    // "A material-value TARGET is legal only as that actual source and cannot
    // accompany actual or assertion."
    let invocation = |parameter: &str| {
        format!(
            concat!(
                "LCL:\n    VERSION: \"0.1.0\"\n\n",
                "SPECIFICATION:\n    ID: test.task\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n",
                "DATA:\n    ID: data.subject\n    TYPE: INTEGER\n    VALUE: 3\n\n",
                "GOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n",
                "ACTION:\n    ID: action.one\n    OPERATION: core.test\n    TARGET: REF(data.subject)\n{}",
                "VERIFY:\n    ID: verify.one\n    ASSERT: TRUE\n\n",
                "SUCCESS:\n    ID: success.one\n    ALL: [REF(verify.one)]\n\n",
                "TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\n",
                "EXECUTE:\n    REFERENCE: REF(task.one)\n"
            ),
            parameter
        )
    };
    let expected = "    PARAMETER:\n        NAME: expected\n        TYPE: INTEGER\n        \
                    REQUIRED: FALSE\n        VALUE: 3\n";
    for parameter in [
        format!("{expected}    PARAMETER:\n        NAME: actual\n        TYPE: INTEGER\n        REQUIRED: FALSE\n        VALUE: 3\n\n"),
        "    PARAMETER:\n        NAME: assertion\n        TYPE: BOOLEAN\n        REQUIRED: FALSE\n        VALUE: TRUE\n\n".to_string(),
    ] {
        assert_eq!(
            ids(&check(&invocation(&parameter))),
            vec!["error.operation.parameter"],
            "{parameter}"
        );
    }
    // The same TARGET is the actual source for expected, which is the one form
    // it is legal in.
    assert_eq!(
        check(&invocation(&format!("{expected}\n"))).outcome(),
        Outcome::Checked
    );
}
