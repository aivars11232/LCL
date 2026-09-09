//! Phase B: the rows that compute their result from declared values alone.

mod common;

use lcl_runtime::{MockHost, Value};
use lcl_stdlib::Stdlib;

/// The `items` of a `result.collection`, as a list of rendered members.
fn items(execution: &lcl_runtime::Execution, declaration: &str) -> Vec<String> {
    match common::field(execution, declaration, "items") {
        Value::List(members) => members.iter().map(|m| m.to_string()).collect(),
        other => panic!("items is not a LIST: {other:?}"),
    }
}

fn value_of(execution: &lcl_runtime::Execution, declaration: &str) -> String {
    common::field(execution, declaration, "value").to_string()
}

fn count_of(execution: &lcl_runtime::Execution, declaration: &str) -> String {
    common::field(execution, declaration, "count").to_string()
}

// ---------------------------------------------------------------------------
// core.return
// ---------------------------------------------------------------------------

#[test]
fn core_return_produces_the_target_value() {
    let source = common::task(
        &common::data("data.subject", "INTEGER", "42"),
        &["ID: action.return\nOPERATION: core.return\nTARGET: REF(data.subject)"],
    );
    let execution = common::run(&source);
    assert_eq!(value_of(&execution, "action.return"), "42");
    assert_eq!(
        common::result_of(&execution, "action.return").status,
        "status.succeeded"
    );
}

#[test]
fn core_return_reaches_no_host() {
    // A pure row has `possible_dependencies` exactly declared_state_only. It
    // must therefore ask the host nothing at all, which is stronger than
    // asking and being permitted.
    let source = common::task(
        &common::data("data.subject", "INTEGER", "42"),
        &["ID: action.return\nOPERATION: core.return\nTARGET: REF(data.subject)"],
    );
    let mut stdlib = common::stdlib();
    let mut host = MockHost::new();
    let fixture = common::fixture(&source);
    lcl_runtime::Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");
    assert_eq!(host.requests().len(), 0, "no request crossed the boundary");
}

// ---------------------------------------------------------------------------
// core.calculate
// ---------------------------------------------------------------------------

fn calculate(expression: &str) -> String {
    let action = format!(
        "ID: action.calculate\nOPERATION: core.calculate\nPARAMETER:\n    NAME: expression\n    \
         TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: {expression:?}"
    );
    let execution = common::run(&common::task("", &[&action]));
    value_of(&execution, "action.calculate")
}

#[test]
fn core_calculate_evaluates_one_expression_fragment() {
    assert_eq!(calculate("1 + 2"), "3");
    assert_eq!(calculate("2 * 3 + 1"), "7");
    assert_eq!(calculate("TRUE AND FALSE"), "FALSE");
}

#[test]
fn core_calculate_reads_the_reserved_target_binding() {
    // "The reserved target binding is the resolved TARGET value, or MISSING on
    // omission."
    let source = common::task(
        &common::data("data.base", "INTEGER", "10"),
        &[
            "ID: action.calculate\nOPERATION: core.calculate\nTARGET: REF(data.base)\n\
           PARAMETER:\n    NAME: expression\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
           VALUE: \"target + 5\"",
        ],
    );
    let execution = common::run(&source);
    assert_eq!(value_of(&execution, "action.calculate"), "15");
}

#[test]
fn core_calculate_reads_a_document_value_only_through_ref() {
    // "Bare names in fragments … never implicitly read a document declaration;
    // use REF for that value read."
    let source = common::task(
        &common::data("data.base", "INTEGER", "10"),
        &["ID: action.calculate\nOPERATION: core.calculate\n\
           PARAMETER:\n    NAME: expression\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
           VALUE: \"REF(data.base) + 5\""],
    );
    let execution = common::run(&source);
    assert_eq!(value_of(&execution, "action.calculate"), "15");
}

#[test]
fn a_malformed_fragment_fails_before_any_effect() {
    let action = "ID: action.calculate\nOPERATION: core.calculate\n\
                  PARAMETER:\n    NAME: expression\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"1 + \"";
    let execution = common::run(&common::task("", &[action]));
    let result = common::result_of(&execution, "action.calculate");
    assert_eq!(result.status, "status.failed");
    assert_eq!(
        result.failure_phase,
        lcl_runtime::FailurePhase::PreEffect,
        "a fragment defect is decided before anything happens"
    );
    // core.calculate does not list error.operation.precondition, so the row's
    // own operand identifier is the one selected.
    assert_eq!(
        common::errors_of(&execution, "action.calculate"),
        vec!["error.operator.operand".to_string()]
    );
}

#[test]
fn a_fragment_holding_two_expressions_is_not_one_expression() {
    // "consume exactly one EXPRESSION … followed only by optional whitespace"
    let action = "ID: action.calculate\nOPERATION: core.calculate\n\
                  PARAMETER:\n    NAME: expression\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"1 + 2 3\"";
    let execution = common::run(&common::task("", &[action]));
    assert_eq!(
        common::result_of(&execution, "action.calculate").status,
        "status.failed"
    );
}

// ---------------------------------------------------------------------------
// core.filter and core.select
// ---------------------------------------------------------------------------

fn filter_document(target_type: &str, target_value: &str, predicate: &str) -> String {
    common::task(
        &common::data("data.numbers", target_type, target_value),
        &[&format!(
            "ID: action.filter\nOPERATION: core.filter\nTARGET: REF(data.numbers)\n\
             PARAMETER:\n    NAME: predicate\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
             VALUE: {predicate:?}"
        )],
    )
}

#[test]
fn core_filter_retains_every_true_member_in_source_order() {
    // "core.filter retains every TRUE member in exact LIST source order."
    let source = filter_document("LIST[INTEGER]", "[3, 1, 4, 1, 5]", "item > 2");
    let execution = common::run(&source);
    assert_eq!(items(&execution, "action.filter"), vec!["3", "4", "5"]);
    assert_eq!(count_of(&execution, "action.filter"), "3");
}

// `core.filter` refusing a SET is checked as a unit test in `src/pure.rs`.
// A `SET`-typed declaration does not reach the runtime as a `Value::Set` today:
// M5 builds `Value::List` for every inline collection regardless of the declared
// type, so no document fixture can present one here.

#[test]
fn core_select_retains_the_members_its_predicate_accepts() {
    let source = common::task(
        &common::data("data.numbers", "LIST[INTEGER]", "[3, 1, 4]"),
        &[
            "ID: action.select\nOPERATION: core.select\nTARGET: REF(data.numbers)\n\
           PARAMETER:\n    NAME: predicate\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
           VALUE: \"item > 2\"",
        ],
    );
    let execution = common::run(&source);
    assert_eq!(items(&execution, "action.select"), vec!["3", "4"]);
    assert_eq!(count_of(&execution, "action.select"), "2");
}

#[test]
fn an_empty_result_is_a_completed_outcome() {
    // "An empty items list is a valid completed outcome."
    let source = filter_document("LIST[INTEGER]", "[1, 2]", "item > 90");
    let execution = common::run(&source);
    assert_eq!(items(&execution, "action.filter"), Vec::<String>::new());
    assert_eq!(count_of(&execution, "action.filter"), "0");
    assert_eq!(
        common::result_of(&execution, "action.filter").status,
        "status.succeeded"
    );
}

#[test]
fn a_non_boolean_predicate_result_is_an_operand_defect() {
    let source = filter_document("LIST[INTEGER]", "[1, 2]", "item + 1");
    let execution = common::run(&source);
    assert_eq!(
        common::errors_of(&execution, "action.filter"),
        vec!["error.operator.operand".to_string()]
    );
}

// ---------------------------------------------------------------------------
// core.sort
// ---------------------------------------------------------------------------

fn sort_document(target_type: &str, target_value: &str, extra: &str) -> String {
    common::task(
        &common::data("data.subject", target_type, target_value),
        &[&format!(
            "ID: action.sort\nOPERATION: core.sort\nTARGET: REF(data.subject){extra}"
        )],
    )
}

#[test]
fn core_sort_orders_by_the_registered_total_order() {
    let execution = common::run(&sort_document("LIST[INTEGER]", "[3, 1, 2]", ""));
    assert_eq!(items(&execution, "action.sort"), vec!["1", "2", "3"]);
}

#[test]
fn core_sort_turns_a_set_into_an_ordered_list() {
    // A SET "must first be given an explicit deterministic LIST order through
    // core.sort", so core.sort is the one collection row that accepts one.
    let execution = common::run(&sort_document("SET[STRING]", "[\"beta\", \"alpha\"]", ""));
    assert_eq!(
        items(&execution, "action.sort"),
        vec!["\"alpha\"", "\"beta\""]
    );
}

/// The `direction` parameter, typed by a locally declared ENUM.
///
/// The registry writes the contract type as `ENUM[ascending|descending]`, which
/// is registry-only notation: "registry-only unions, metatypes, and
/// result-schema names are not source forms." A document names the type it
/// declares, and `TYPE_EXPRESSION` admits a `REFERENCE_CALL` for exactly that.
const DIRECTION_TYPE: &str = "\nDEFINE:\n    ID: type.direction\n    KIND: kind.type\n    \
                              BASE: ENUM\n    ITEM: ascending\n    ITEM: descending\n";

#[test]
fn descending_reverses_the_key_order() {
    let declarations = format!(
        "{}{DIRECTION_TYPE}",
        common::data("data.subject", "LIST[INTEGER]", "[3, 1, 2]")
    );
    let action = "ID: action.sort\nOPERATION: core.sort\nTARGET: REF(data.subject)\n\
                  PARAMETER:\n    NAME: direction\n    TYPE: REF(type.direction)\n    \
                  REQUIRED: FALSE\n    VALUE: descending";
    let execution = common::run(&common::task(&declarations, &[action]));
    assert_eq!(items(&execution, "action.sort"), vec!["3", "2", "1"]);
}

#[test]
fn the_default_direction_is_ascending() {
    // The registry declares `direction` default "ascending", and an omitted
    // optional parameter takes it.
    let execution = common::run(&sort_document("LIST[INTEGER]", "[2, 3, 1]", ""));
    assert_eq!(items(&execution, "action.sort"), vec!["1", "2", "3"]);
}

#[test]
fn an_unresolved_key_operation_is_a_precondition_failure() {
    // "a missing, ambiguous, incomplete, or out-of-bounds key-operation profile
    // use error.operation.precondition". A DEFINE kind.operation declares a
    // contract and no body, so with nothing installed there is no profile.
    let declarations = format!(
        "{}{}",
        common::data("data.names", "LIST[STRING]", "[\"beta\", \"alpha\"]"),
        "\nDEFINE:\n    ID: sort.identity_key\n    KIND: kind.operation\n    \
         MEANING: \"Return the STRING member as its ordered key.\"\n    \
         SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        \
         NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        \
         TYPE: STRING\n"
    );
    let action = "ID: action.sort\nOPERATION: core.sort\nTARGET: REF(data.names)\n\
                  PARAMETER:\n    NAME: key\n    TYPE: REFERENCE[REF(sort.identity_key)]\n    \
                  REQUIRED: FALSE\n    VALUE: REF(sort.identity_key)";
    let execution = common::run(&common::task(&declarations, &[action]));
    assert_eq!(
        common::errors_of(&execution, "action.sort"),
        vec!["error.operation.precondition".to_string()]
    );
}

#[test]
fn an_installed_key_operation_orders_the_members() {
    let declarations = format!(
        "{}{}",
        common::data("data.names", "LIST[STRING]", "[\"beta\", \"alpha\"]"),
        "\nDEFINE:\n    ID: sort.identity_key\n    KIND: kind.operation\n    \
         MEANING: \"Return the STRING member as its ordered key.\"\n    \
         SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        \
         NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        \
         TYPE: STRING\n"
    );
    let action = "ID: action.sort\nOPERATION: core.sort\nTARGET: REF(data.names)\n\
                  PARAMETER:\n    NAME: key\n    TYPE: REFERENCE[REF(sort.identity_key)]\n    \
                  REQUIRED: FALSE\n    VALUE: REF(sort.identity_key)";
    let stdlib: Stdlib = common::stdlib()
        .with_pure_operation("sort.identity_key", Box::new(|member| Ok(member.clone())));
    let execution = common::run_with(
        &common::task(&declarations, &[action]),
        stdlib,
        MockHost::new(),
    );
    assert_eq!(
        items(&execution, "action.sort"),
        vec!["\"alpha\"", "\"beta\""]
    );
}

// ---------------------------------------------------------------------------
// core.group
// ---------------------------------------------------------------------------

/// A document declaring a pure key operation over STRING members.
fn group_document(members: &str, key_reference: &str) -> String {
    let declarations = format!(
        "{}{}",
        common::data("data.words", "LIST[STRING]", members),
        "\nDEFINE:\n    ID: group.initial\n    KIND: kind.operation\n    \
         MEANING: \"Return the first character of the member as its group key.\"\n    \
         SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        \
         NAME: member\n        TYPE: STRING\n        REQUIRED: TRUE\n    RESULT:\n        \
         TYPE: STRING\n"
    );
    let action = format!(
        "ID: action.group\nOPERATION: core.group\nTARGET: REF(data.words)\n\
         PARAMETER:\n    NAME: key\n    TYPE: {key_reference}\n    REQUIRED: TRUE\n    \
         VALUE: REF(group.initial)"
    );
    common::task(&declarations, &[&action])
}

/// The initial-letter key implementation the document declares and does not
/// define.
fn initial_letter() -> lcl_stdlib::PureOperation {
    Box::new(|member| match member {
        Value::Text(text) => Ok(Value::Text(
            text.chars().next().map(String::from).unwrap_or_default(),
        )),
        other => Err(format!(
            "expected a STRING member, found {}",
            other.family()
        )),
    })
}

#[test]
fn core_group_orders_groups_by_first_occurrence_and_keeps_source_order() {
    // "Strict == groups keys, first key occurrence orders the groups, and
    // source order is preserved within each group."
    let source = group_document(
        "[\"beta\", \"alpha\", \"blue\"]",
        "REFERENCE[REF(group.initial)]",
    );
    let stdlib = common::stdlib().with_pure_operation("group.initial", initial_letter());
    let execution = common::run_with(&source, stdlib, MockHost::new());

    let rendered = value_of(&execution, "action.group");
    let first_b = rendered.find("\"b\"").expect("the b group exists");
    let first_a = rendered.find("\"a\"").expect("the a group exists");
    assert!(
        first_b < first_a,
        "the first key occurrence orders the groups: {rendered}"
    );
    assert!(
        rendered.find("\"beta\"").unwrap() < rendered.find("\"blue\"").unwrap(),
        "source order is preserved within a group: {rendered}"
    );
}

#[test]
fn core_group_over_no_members_returns_an_empty_list() {
    // "Empty input returns an empty LIST of that record schema."
    let source = group_document("[]", "REFERENCE[REF(group.initial)]");
    let stdlib = common::stdlib().with_pure_operation("group.initial", initial_letter());
    let execution = common::run_with(&source, stdlib, MockHost::new());
    assert_eq!(value_of(&execution, "action.group"), "[]");
}

#[test]
fn an_undefined_property_path_is_a_precondition_failure() {
    // "A malformed or unregistered path uses error.operation.precondition." A
    // STRING member defines no property, so every path is undefined for it.
    let source = common::task(
        &common::data("data.words", "LIST[STRING]", "[\"alpha\"]"),
        &[
            "ID: action.group\nOPERATION: core.group\nTARGET: REF(data.words)\n\
           PARAMETER:\n    NAME: key\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
           VALUE: \"team\"",
        ],
    );
    let execution = common::run(&source);
    assert_eq!(
        common::errors_of(&execution, "action.group"),
        vec!["error.operation.precondition".to_string()]
    );
}
