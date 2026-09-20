//! Phase E: comparison, checking, and the rows that coordinate execution.

mod common;

use lcl_runtime::{Execution, Runtime, Value};
use lcl_stdlib::{checking_profiles, MemoryFileSystem};

fn run_checked(source: &str) -> Execution {
    let mut stdlib = common::stdlib().with_profiles(checking_profiles());
    let mut host = lcl_runtime::MockHost::new();
    let fixture = common::fixture(source);
    Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned")
}

// ---------------------------------------------------------------------------
// core.compare
// ---------------------------------------------------------------------------

fn compare_document(
    left_type: &str,
    left: &str,
    right_type: &str,
    right: &str,
    criteria: Option<&str>,
) -> String {
    let declarations = format!(
        "{}{}",
        common::data("data.left", left_type, left),
        common::data("data.right", right_type, right)
    );
    let criteria = criteria
        .map(|token| {
            format!(
                "\nPARAMETER:\n    NAME: criteria\n    TYPE: STRING\n    REQUIRED: FALSE\n    \
                 VALUE: {token:?}"
            )
        })
        .unwrap_or_default();
    let action = format!(
        "ID: action.compare\nOPERATION: core.compare\nTARGET: REF(data.left)\n\
         PARAMETER:\n    NAME: against\n    TYPE: {right_type}\n    REQUIRED: TRUE\n    \
         VALUE: REF(data.right){criteria}"
    );
    common::task(&declarations, &[&action])
}

fn compared(source: &str) -> Value {
    common::field(&run_checked(source), "action.compare", "value").clone()
}

#[test]
fn an_omitted_criterion_selects_strict_equality() {
    // "Omitted criteria selects ==."
    let same = compare_document("INTEGER", "3", "INTEGER", "3", None);
    assert_eq!(compared(&same), Value::Boolean(true));

    let different = compare_document("INTEGER", "3", "INTEGER", "4", None);
    assert_eq!(compared(&different), Value::Boolean(false));
}

#[test]
fn a_false_comparison_is_a_successful_result() {
    // "result.value.value is exactly one BOOLEAN; FALSE is a successful
    // comparison result."
    let source = compare_document("INTEGER", "3", "INTEGER", "4", None);
    let execution = run_checked(&source);
    assert_eq!(
        common::result_of(&execution, "action.compare").status,
        "status.succeeded"
    );
    assert!(common::errors_of(&execution, "action.compare").is_empty());
}

#[test]
fn the_registered_ordering_operators_compare_by_the_total_order() {
    let less = compare_document("INTEGER", "3", "INTEGER", "4", Some("<"));
    assert_eq!(compared(&less), Value::Boolean(true));

    let greater = compare_document("INTEGER", "3", "INTEGER", "4", Some(">"));
    assert_eq!(compared(&greater), Value::Boolean(false));

    let at_least = compare_document("INTEGER", "4", "INTEGER", "4", Some(">="));
    assert_eq!(compared(&at_least), Value::Boolean(true));
}

#[test]
fn membership_reads_in_and_contains_from_opposite_sides() {
    let inside = compare_document("INTEGER", "2", "LIST[INTEGER]", "[1, 2, 3]", Some("IN"));
    assert_eq!(compared(&inside), Value::Boolean(true));

    let holds = compare_document(
        "LIST[INTEGER]",
        "[1, 2, 3]",
        "INTEGER",
        "2",
        Some("CONTAINS"),
    );
    assert_eq!(compared(&holds), Value::Boolean(true));

    let absent = compare_document("INTEGER", "9", "LIST[INTEGER]", "[1, 2, 3]", Some("IN"));
    assert_eq!(compared(&absent), Value::Boolean(false));
}

#[test]
fn matches_applies_the_closed_pattern_profiles() {
    let glob = compare_document(
        "STRING",
        "\"src/main.py\"",
        "GLOB",
        "GLOB(\"src/**/*.py\")",
        Some("MATCHES"),
    );
    assert_eq!(compared(&glob), Value::Boolean(true));

    let regex = compare_document(
        "STRING",
        "\"Alpha\"",
        "REGEX",
        "REGEX(\"[A-Za-z]+\")",
        Some("MATCHES"),
    );
    assert_eq!(compared(&regex), Value::Boolean(true));
}

/// PRETEST-02 F07: `core.compare` MATCHES consumes the same subjects as the
/// expression operator, through the one shared rule.
fn workspace_compare(left_type: &str, left: &str, right: &str) -> String {
    compare_document(left_type, left, "GLOB", right, Some("MATCHES")).replacen(
        "KIND: kind.task\n",
        "KIND: kind.task\n\nWORKSPACE:\n    ID: workspace.case\n    PATH: PATH(\"/case\")\n    \
         MODE: mode.read_only\n",
        1,
    )
}

#[test]
fn matches_consumes_a_workspace_path_by_its_relative_segments() {
    let inside = workspace_compare(
        "PATH",
        r#"PATH(REF(workspace.case), "src/a.py")"#,
        r#"GLOB("src/*.py")"#,
    );
    let execution = run_checked(&inside);
    assert!(common::errors_of(&execution, "action.compare").is_empty());
    assert_eq!(compared(&inside), Value::Boolean(true));

    let outside = workspace_compare(
        "PATH",
        r#"PATH(REF(workspace.case), "lib/a.py")"#,
        r#"GLOB("src/*.py")"#,
    );
    assert_eq!(compared(&outside), Value::Boolean(false));
}

#[test]
fn matches_refuses_an_absolute_path_and_a_malformed_string_subject() {
    for (ty, subject) in [
        ("PATH", r#"PATH("/case/src/a.py")"#),
        ("STRING", r#""/src/a.py""#),
    ] {
        let source = workspace_compare(ty, subject, r#"GLOB("**")"#);
        assert_eq!(
            common::errors_of(&run_checked(&source), "action.compare"),
            vec!["error.operator.operand".to_string()],
            "{subject}"
        );
    }
}

#[test]
fn matches_reads_regex_flags_as_the_expression_operator_does() {
    let flagged = compare_document(
        "STRING",
        "\"ABC\"",
        "REGEX",
        "REGEX(\"[a-z]+\", \"i\")",
        Some("MATCHES"),
    );
    assert_eq!(compared(&flagged), Value::Boolean(true));
}

#[test]
fn matches_keeps_the_missing_projection_rule() {
    // "A supplied non-==/!= criterion that encounters MISSING uses
    // error.required.missing."
    let declarations = format!(
        "\nDATA:\n    ID: data.left\n    TYPE: OBJECT\n    VALUE:\n        name: \"src/a.py\"\n{}\
         \nDATA:\n    ID: data.criteria\n    TYPE: OBJECT\n    VALUE:\n        \
         operator: \"MATCHES\"\n        left: \"absent\"\n",
        common::data("data.right", "GLOB", r#"GLOB("**")"#)
    );
    let action = "ID: action.compare\nOPERATION: core.compare\nTARGET: REF(data.left)\n\
                  PARAMETER:\n    NAME: against\n    TYPE: GLOB\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.right)\nPARAMETER:\n    NAME: criteria\n    TYPE: OBJECT\n    \
                  REQUIRED: FALSE\n    VALUE: REF(data.criteria)";
    let source = common::task(&declarations, &[action]);
    assert_eq!(
        common::errors_of(&run_checked(&source), "action.compare"),
        vec!["error.required.missing".to_string()]
    );
}

#[test]
fn an_unregistered_criterion_is_refused() {
    // "Unknown keys, malformed paths, absent operator, and an unregistered
    // token produce error.operation.parameter."
    let source = compare_document("INTEGER", "3", "INTEGER", "4", Some("~="));
    let execution = run_checked(&source);
    assert_eq!(
        common::errors_of(&execution, "action.compare"),
        vec!["error.operation.parameter".to_string()]
    );
}

#[test]
fn a_malformed_criteria_object_is_a_parameter_defect() {
    // The same sentence covers an absent operator and an unknown key; both are
    // the criteria OBJECT failing its own closed form.
    let malformed = |fields: &str| {
        let declarations = format!(
            "{}{}\nDATA:\n    ID: data.criteria\n    TYPE: OBJECT\n    VALUE:\n{fields}",
            common::data("data.left", "INTEGER", "3"),
            common::data("data.right", "INTEGER", "4")
        );
        let action = "ID: action.compare\nOPERATION: core.compare\nTARGET: REF(data.left)\n\
                      PARAMETER:\n    NAME: against\n    TYPE: INTEGER\n    REQUIRED: TRUE\n    \
                      VALUE: REF(data.right)\nPARAMETER:\n    NAME: criteria\n    TYPE: OBJECT\n    \
                      REQUIRED: FALSE\n    VALUE: REF(data.criteria)";
        common::task(&declarations, &[action])
    };
    for fields in [
        "        left: \"name\"\n",
        "        operator: \"==\"\n        middle: \"name\"\n",
    ] {
        let execution = run_checked(&malformed(fields));
        assert_eq!(
            common::errors_of(&execution, "action.compare"),
            vec!["error.operation.parameter".to_string()],
            "criteria fields {fields:?}"
        );
    }
}

#[test]
fn a_criteria_reference_to_neither_admitted_form_is_a_type_defect() {
    // "One REFERENCE resolves once to a declared STRING or OBJECT value
    // satisfying the same contract." A declared LIST is neither form, and the
    // row's precondition is that "criteria are type-valid".
    let declarations = format!(
        "{}{}",
        common::data("data.left", "INTEGER", "3"),
        common::data("data.numbers", "LIST[INTEGER]", "[1, 2]")
    );
    let action = "ID: action.compare\nOPERATION: core.compare\nTARGET: REF(data.left)\n\
                  PARAMETER:\n    NAME: against\n    TYPE: INTEGER\n    REQUIRED: TRUE\n    \
                  VALUE: 3\nPARAMETER:\n    NAME: criteria\n    \
                  TYPE: REFERENCE[REF(data.numbers)]\n    REQUIRED: FALSE\n    \
                  VALUE: REF(data.numbers)";
    let execution = run_checked(&common::task(&declarations, &[action]));
    assert_eq!(
        common::errors_of(&execution, "action.compare"),
        vec!["error.type.mismatch".to_string()]
    );
}

#[test]
fn incompatible_operands_under_an_ordering_operator_are_an_operand_defect() {
    let source = compare_document("INTEGER", "3", "STRING", "\"three\"", Some("<"));
    let execution = run_checked(&source);
    assert_eq!(
        common::errors_of(&execution, "action.compare"),
        vec!["error.operator.operand".to_string()]
    );
}

#[test]
fn core_compare_reaches_no_host_for_two_material_operands() {
    // The row's *maximum* admits host and network, because its operands may be
    // addressable. An invocation over two material values resolves neither.
    let source = compare_document("INTEGER", "3", "INTEGER", "3", None);
    let mut stdlib = common::stdlib();
    let mut host = lcl_runtime::MockHost::new();
    let fixture = common::fixture(&source);
    Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");
    assert_eq!(host.requests().len(), 0);
}

// ---------------------------------------------------------------------------
// core.verify
// ---------------------------------------------------------------------------

fn verify_document(assertion: &str) -> String {
    let action = format!(
        "ID: action.verify\nOPERATION: core.verify\nTARGET: REF(data.subject)\n\
         PARAMETER:\n    NAME: assertion\n    TYPE: BOOLEAN\n    REQUIRED: TRUE\n    \
         VALUE: {assertion}"
    );
    common::task(&common::data("data.subject", "INTEGER", "3"), &[&action])
}

#[test]
fn core_verify_reports_whether_its_assertion_held() {
    let execution = run_checked(&verify_document("REF(data.subject) == 3"));
    let result = common::result_of(&execution, "action.verify");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(result.fields.get("verified"), Some(&Value::Boolean(true)));
    assert_eq!(result.fields.get("errors"), Some(&Value::List(Vec::new())));
}

#[test]
fn a_false_assertion_is_a_completed_verification_with_a_finding() {
    // "verified TRUE requires an empty errors list"; a FALSE verification is a
    // domain outcome carrying a finding, not a producer failure.
    let execution = run_checked(&verify_document("REF(data.subject) == 4"));
    let result = common::result_of(&execution, "action.verify");
    assert_eq!(result.status, "status.succeeded");
    assert_eq!(result.fields.get("verified"), Some(&Value::Boolean(false)));
    match result.fields.get("errors") {
        Some(Value::List(errors)) => assert_eq!(errors.len(), 1),
        other => panic!("expected one finding, got {other:?}"),
    }
}

#[test]
fn core_verify_without_an_installed_verifier_fails_its_precondition() {
    // The `verification` role is required by the row itself, so an engine with
    // no verifier installed refuses before it decides anything.
    let source = verify_document("REF(data.subject) == 3");
    let mut stdlib = common::stdlib(); // no profiles
    let mut host = lcl_runtime::MockHost::new();
    let fixture = common::fixture(&source);
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");
    assert_eq!(
        common::errors_of(&execution, "action.verify"),
        vec!["error.operation.precondition".to_string()]
    );
}

/// One `core.verify` whose evidence parameter names `evidence.item`.
fn verify_with_evidence(evidence: &str) -> lcl_runtime::Execution {
    let declarations = format!("{}{evidence}", common::data("data.number", "INTEGER", "3"));
    let action = "ID: action.verify\nOPERATION: core.verify\nTARGET: REF(data.number)\n\
                  PARAMETER:\n    NAME: assertion\n    TYPE: BOOLEAN\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.number) == 3\nPARAMETER:\n    NAME: evidence\n    \
                  TYPE: LIST[REFERENCE[REF(evidence.item)]]\n    REQUIRED: FALSE\n    \
                  VALUE: [REF(evidence.item)]";
    run_checked(&common::task(&declarations, &[action]))
}

#[test]
fn core_verify_records_the_evidence_it_required() {
    // "result records TRUE, FALSE, or UNKNOWN and evidence."
    let execution = verify_with_evidence(
        "\nEVIDENCE:\n    ID: evidence.item\n    TYPE: STRING\n    VALUE: \"observed\"\n",
    );
    let result = common::result_of(&execution, "action.verify");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("evidence"),
        Some(&Value::List(vec![Value::Reference(
            "evidence.item".to_string()
        )]))
    );
}

// ---------------------------------------------------------------------------
// core.cancel
// ---------------------------------------------------------------------------

#[test]
fn a_cancellation_reason_travels_with_the_state_effect_it_justified() {
    // "reason is evidenced": the recorded reason is evidence of the transition,
    // and "Every known begun effect is retained in observed_effects".
    let action = "ID: action.cancel\nOPERATION: core.cancel\nTARGET: REF(task.subject)\n\
                  PARAMETER:\n    NAME: reason\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"the owner cancelled it\"";
    let execution = common::run(&common::task("", &[action]));
    let result = common::result_of(&execution, "action.cancel");
    let evidence: Vec<&String> = result
        .observed_effects
        .iter()
        .flat_map(|effect| effect.evidence.iter())
        .collect();
    assert_eq!(evidence, vec!["the owner cancelled it"]);
}

// ---------------------------------------------------------------------------
// core.validate
// ---------------------------------------------------------------------------

#[test]
fn core_validate_with_no_declared_rules_is_valid() {
    // "valid TRUE requires an empty errors list."
    let action = "ID: action.validate\nOPERATION: core.validate\nTARGET: REF(data.subject)";
    let source = common::task(&common::data("data.subject", "INTEGER", "3"), &[action]);
    let execution = run_checked(&source);
    let result = common::result_of(&execution, "action.validate");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(result.fields.get("valid"), Some(&Value::Boolean(true)));
    assert_eq!(result.fields.get("errors"), Some(&Value::List(Vec::new())));
}

/// One `core.validate` over `data.subject` with a schema REFERENCE.
fn validate_against_schema(declarations: &str, schema: &str, subject: &str) -> Vec<String> {
    let action = format!(
        "ID: action.validate\nOPERATION: core.validate\nTARGET: REF(data.subject)\n\
         PARAMETER:\n    NAME: schema\n    TYPE: REFERENCE[REF({schema})]\n    \
         REQUIRED: FALSE\n    VALUE: REF({schema})"
    );
    let source = common::task(&format!("{declarations}{subject}"), &[&action]);
    let execution = run_checked(&source);
    let result = common::result_of(&execution, "action.validate");
    if !result.execution_errors.is_empty() {
        return common::errors_of(&execution, "action.validate");
    }
    match result.fields.get("errors") {
        Some(Value::List(errors)) => errors.iter().map(|error| error.to_string()).collect(),
        other => panic!("result.validation records an errors list, found {other:?}"),
    }
}

const PERSON_SCHEMA: &str = "\nDEFINE:\n    ID: type.person\n    KIND: kind.type\n    \
     BASE: OBJECT\n    FIELD:\n        NAME: name\n        TYPE: STRING\n        \
     REQUIRED: TRUE\n";

#[test]
fn a_schema_reference_of_the_wrong_kind_is_a_reference_defect() {
    // "The REFERENCE resolves exactly once, following transparent aliases, to a
    // kind.type whose resolved type is OBJECT." A DATA declaration is not one.
    assert_eq!(
        validate_against_schema(
            &common::data("data.number", "INTEGER", "3"),
            "data.number",
            &common::data("data.subject", "INTEGER", "3")
        ),
        vec!["error.reference.kind".to_string()]
    );
}

#[test]
fn a_declared_schema_is_applied_to_the_target() {
    // The same sentence requires a schema "whose schema applies to the target",
    // and "all detected failures use registered error identifiers".
    let conforming = "\nDATA:\n    ID: data.subject\n    TYPE: OBJECT[REF(type.person)]\n    \
                      VALUE:\n        name: \"ada\"\n";
    assert_eq!(
        validate_against_schema(PERSON_SCHEMA, "type.person", conforming),
        Vec::<String>::new()
    );

    let other = common::data("data.subject", "INTEGER", "3");
    assert_eq!(
        validate_against_schema(PERSON_SCHEMA, "type.person", &other),
        vec!["error.validation.failed".to_string()]
    );
}

// ---------------------------------------------------------------------------
// core.test
// ---------------------------------------------------------------------------

#[test]
fn core_test_compares_an_expected_value_against_an_actual_one() {
    // "Expected-and-actual form always uses the registered == strict-equality
    // operator."
    let action = "ID: action.test\nOPERATION: core.test\n\
                  PARAMETER:\n    NAME: expected\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    \
                  VALUE: 3\n\
                  PARAMETER:\n    NAME: actual\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    \
                  VALUE: REF(data.subject)";
    let source = common::task(&common::data("data.subject", "INTEGER", "3"), &[action]);
    let execution = run_checked(&source);

    let result = common::result_of(&execution, "action.test");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(result.fields.get("passed"), Some(&Value::Boolean(true)));
}

#[test]
fn core_test_evaluates_a_declared_assertion() {
    let action = "ID: action.test\nOPERATION: core.test\n\
                  PARAMETER:\n    NAME: assertion\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    \
                  VALUE: REF(data.subject) == 3";
    let source = common::task(&common::data("data.subject", "INTEGER", "3"), &[action]);
    let execution = run_checked(&source);
    assert_eq!(
        common::result_of(&execution, "action.test")
            .fields
            .get("passed"),
        Some(&Value::Boolean(true))
    );
}

#[test]
fn core_test_requires_exactly_one_comparison_form() {
    // "TARGET alone is not a complete test." `error.operation.precondition` is
    // not one of the identifiers this row's closed `errors` list admits: an
    // unsatisfied comparison form is its registered
    // `error.block.conditional_requirement`, at the stage that identifier is
    // registered to.
    let action = "ID: action.test\nOPERATION: core.test\nTARGET: REF(data.subject)";
    let source = common::task(&common::data("data.subject", "INTEGER", "3"), &[action]);
    assert_eq!(
        refused_before_execution(&source),
        "error.block.conditional_requirement"
    );
}

#[test]
fn an_assertion_may_not_accompany_an_actual_source() {
    // "A material-value TARGET is legal only as that actual source and cannot
    // accompany actual or assertion." Whether the TARGET is material is a type
    // question, so this half is the row's `error.operation.parameter`.
    let action = "ID: action.test\nOPERATION: core.test\nTARGET: REF(data.subject)\n\
                  PARAMETER:\n    NAME: assertion\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    \
                  VALUE: TRUE";
    let source = common::task(&common::data("data.subject", "INTEGER", "3"), &[action]);
    assert_eq!(
        refused_before_execution(&source),
        "error.operation.parameter"
    );
}

/// The identifier of the first stage that refuses one source before execution.
fn refused_before_execution(source: &str) -> String {
    let unit =
        lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new("root.lcl"), source.as_bytes());
    match lcl_resolver::Resolver::new(common::rules(), common::grammar(), common::lexicon())
        .resolve(&unit, &lcl_resolver::MemoryProvider::new())
    {
        Err(skipped) => skipped.primary,
        Ok(resolved) => match resolved.primary() {
            Some(primary) => primary.id.to_string(),
            None => lcl_checker::Checker::new(common::static_contracts())
                .check(&resolved)
                .expect("resolution succeeded")
                .primary()
                .map(|primary| primary.id.to_string())
                .unwrap_or_default(),
        },
    }
}

// ---------------------------------------------------------------------------
// The handler-context rows
// ---------------------------------------------------------------------------

#[test]
fn core_retry_outside_a_handler_fails_its_precondition() {
    // "core.retry ... is invalid outside its selected handler context."
    let declarations = format!(
        "{}{}",
        common::data("data.subject", "INTEGER", "3"),
        "\nACTION:\n    ID: action.subject\n    OPERATION: core.return\n    \
         TARGET: REF(data.subject)\n"
    );
    let action = "ID: action.retry\nOPERATION: core.retry\nTARGET: REF(action.subject)\n\
                  PARAMETER:\n    NAME: limit\n    TYPE: INTEGER\n    REQUIRED: TRUE\n    \
                  VALUE: 1";
    let source = common::task(&declarations, &[action]);
    let execution = run_checked(&source);
    assert_eq!(
        common::errors_of(&execution, "action.retry"),
        vec!["error.operation.precondition".to_string()]
    );
}

#[test]
fn every_control_row_that_reaches_no_capability_says_so() {
    // `core.stop` on a path needs a process capability; an engine without one
    // reports a limitation rather than claiming the process stopped.
    let action = "ID: action.stop\nOPERATION: core.stop\nTARGET: REF(data.service)";
    let source = common::task(
        &common::data("data.service", "PATH", "PATH(\"/srv/run/service\")"),
        &[action],
    );
    let mut stdlib = common::stdlib().with_profiles(lcl_stdlib::process_profiles());
    let mut host =
        lcl_stdlib::HostAdapter::new(lcl_capabilities::Grants::none().permit_any_program())
            .with_filesystem(MemoryFileSystem::new().with_scope("/srv/run"));
    let fixture = common::fixture(&source);
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");
    assert_eq!(
        common::errors_of(&execution, "action.stop"),
        vec!["error.host.constraint".to_string()]
    );
}

/// `core.stop=stop`: "Each listed role applies to every invocation except
/// core.execute", and "a missing, ambiguous, incomplete, or out-of-bounds
/// required profile role emits error.operation.precondition before effects".
/// A process stop therefore selects its stop profile before its request
/// crosses the boundary.
#[test]
fn a_process_stop_without_its_stop_profile_fails_its_precondition_before_effects() {
    let action = "ID: action.stop\nOPERATION: core.stop\nTARGET: REF(data.service)";
    let source = common::task(
        &common::data("data.service", "STRING", "\"report\""),
        &[action],
    );
    let run = |mut stdlib: lcl_stdlib::Stdlib| {
        let mut host = lcl_runtime::MockHost::new();
        let fixture = common::fixture(&source);
        let execution = Runtime::new(common::contracts())
            .execute_with(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut stdlib,
                &mut host,
            )
            .expect("planned");
        (execution, host.count("core.stop"))
    };

    let (execution, crossed) = run(common::stdlib());
    assert_eq!(
        common::errors_of(&execution, "action.stop"),
        vec!["error.operation.precondition".to_string()]
    );
    let result = common::result_of(&execution, "action.stop");
    assert_eq!(result.failure_phase, lcl_runtime::FailurePhase::PreEffect);
    assert_eq!(result.effect_state, lcl_runtime::EffectState::None);
    assert_eq!(crossed, 0, "no request crosses without the stop profile");

    // Control: with the shipped stop profile the request reaches the host.
    let (execution, crossed) = run(common::stdlib().with_profiles(lcl_stdlib::process_profiles()));
    assert!(common::errors_of(&execution, "action.stop").is_empty());
    assert_eq!(crossed, 1);
}
