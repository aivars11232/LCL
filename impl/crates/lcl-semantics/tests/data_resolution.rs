//! Step 7: input, data, context, memory, state, default, assumption and
//! dependency resolution, and step 6's scope and workspace containment.
//!
//! Authority: `05_SEMANTICS/06`, `05_SEMANTICS/07` and `05_SEMANTICS/02`.

mod common;

use common::*;
use lcl_semantics::{Invocation, Origin, Outcome, Value};

fn resolution<'a>(planned: &'a lcl_semantics::Planned, id: &str) -> &'a lcl_semantics::Resolution {
    planned
        .partial_plan()
        .resolutions()
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("`{id}` must be resolved"))
}

// ---------------------------------------------------------------------------
// The resolution order
// ---------------------------------------------------------------------------

/// One `DATA` the scaffolding's ACTION can target, so a fixture only has to
/// declare what it is actually about.
const SUBJECT: &str = "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n";

#[test]
fn an_explicit_value_wins_over_everything() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    VALUE: 7\n    DEFAULT: 42\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &Invocation::new().with("input.one", Value::Integer(nine())),
    );
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::DeclaredValue);
    assert_eq!(resolved.value.to_string(), "7");
}

/// `03_TYPES_AND_VALUES/10`, OBJECT: "An object uses an indented VALUE block
/// containing unique lowercase property names."
///
/// Regression, `LCL-TASK-0020` defect 1. That body is `Body::Nested`, an inline
/// read cannot see it, and every object-valued declaration resolved to MISSING
/// while nothing in the document was missing. CLOSURE-005 failed on it, and
/// CLOSURE-021, CLOSURE-042 and CLOSURE-051 could not be written at all.
const RECORD: &str = "\nDATA:\n    ID: data.record\n    TYPE: OBJECT\n    VALUE:\n        name: \"a\"\n        count: 2\n";

#[test]
fn an_object_written_as_an_indented_value_block_resolves_to_an_object() {
    let source = task_document(&format!("{RECORD}{SUBJECT}"));
    let planned = plan_with(&source, &Invocation::new());
    let resolved = resolution(&planned, "data.record");
    assert_eq!(resolved.origin, Origin::DeclaredValue);
    let Value::Object(fields) = &resolved.value else {
        panic!(
            "an indented VALUE block declares an OBJECT, got {:?}",
            resolved.value
        );
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields.get("name"), Some(&Value::Text("a".to_string())));
    assert_eq!(
        fields.get("count").map(|v| v.to_string()),
        Some("2".to_string())
    );
}

/// "Property order has no semantic effect", and `03_TYPES_AND_VALUES/03` gives
/// OBJECT equality by field. Two objects written in different orders are one
/// value.
#[test]
fn property_order_has_no_semantic_effect() {
    let reversed = "\nDATA:\n    ID: data.record\n    TYPE: OBJECT\n    VALUE:\n        count: 2\n        name: \"a\"\n";
    let first = plan_with(
        &task_document(&format!("{RECORD}{SUBJECT}")),
        &Invocation::new(),
    );
    let second = plan_with(
        &task_document(&format!("{reversed}{SUBJECT}")),
        &Invocation::new(),
    );
    assert_eq!(
        resolution(&first, "data.record").value,
        resolution(&second, "data.record").value
    );
}

/// An object property may itself be an object: the same syntactic form nests,
/// and the depth bound is the evaluator's own.
#[test]
fn an_object_property_may_itself_be_an_object() {
    let nested = "\nDATA:\n    ID: data.record\n    TYPE: OBJECT\n    VALUE:\n        inner:\n            name: \"a\"\n";
    let planned = plan_with(
        &task_document(&format!("{nested}{SUBJECT}")),
        &Invocation::new(),
    );
    let Value::Object(outer) = &resolution(&planned, "data.record").value else {
        panic!("the outer value is an object");
    };
    let Some(Value::Object(inner)) = outer.get("inner") else {
        panic!("the inner value is an object, got {:?}", outer.get("inner"));
    };
    assert_eq!(inner.get("name"), Some(&Value::Text("a".to_string())));
}

#[test]
fn a_supplied_value_resolves_a_declaration_with_no_declared_value() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in.txt\")\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &Invocation::new().with("input.one", Value::Integer(nine())),
    );
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Supplied);
    assert_eq!(resolved.value.to_string(), "9");
}

#[test]
fn a_declaration_with_neither_a_value_nor_a_supplied_datum_is_missing() {
    // Nothing is invented to fill the gap: "MISSING: no value/source exists."
    // A declared SOURCE names where a value would come from; reading it is an
    // effect, and preflight performs none.
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in.txt\")\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.value, Value::Missing);
    assert_eq!(resolved.origin, Origin::Absent);
}

#[test]
fn a_default_replaces_missing() {
    // "3. DEFAULT only for MISSING."
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 42\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Default);
    assert_eq!(resolved.value.to_string(), "42");
}

/// Regression, PRETEST-01 F02. A written DEFAULT this layer cannot fold is
/// undecided, not MISSING: it exists, and the demanding layer evaluates it.
#[test]
fn an_undecided_default_is_recorded_as_undecided_not_missing() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: DECIMAL\n    REQUIRED: FALSE\n    DEFAULT: 3 / 2\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Default);
    assert!(resolved.undecided);
    assert_eq!(resolved.value, Value::Unknown);
}

#[test]
fn a_default_never_replaces_null() {
    // "DEFAULT never replaces NULL/UNKNOWN unless a rule explicitly maps them
    // first." NULL is a known explicit absence, not a gap, so step 3 is never
    // reached and the declared NULL stands.
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: NULL\n    VALUE: NULL\n    DEFAULT: NULL\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.value, Value::Null);
    assert_eq!(
        resolved.origin,
        Origin::DeclaredValue,
        "the DEFAULT must never have been consulted"
    );
}

#[test]
fn a_default_never_replaces_unknown() {
    // The same rule for the other non-material sentinel. A supplied UNKNOWN is
    // "a value exists but cannot be determined" — not an absence a DEFAULT may
    // fill.
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 42\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &Invocation::new().with("input.one", Value::Unknown),
    );
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.value, Value::Unknown);
    assert_eq!(resolved.origin, Origin::Supplied);
}

#[test]
fn a_supplied_value_is_preferred_over_a_default() {
    // Step 2 precedes step 3.
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 42\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &Invocation::new().with("input.one", Value::Integer(nine())),
    );
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Supplied);
    assert_eq!(resolved.value.to_string(), "9");
}

// ---------------------------------------------------------------------------
// Assumptions
// ---------------------------------------------------------------------------

#[test]
fn an_applicable_assumption_resolves_a_missing_value_and_is_recorded_as_evidence() {
    // "ASSUME is conditional, cannot override explicit data, and must be
    // recorded as evidence."
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in.txt\")\n\nASSUME:\n    ID: assume.one\n    TARGET: REF(input.one)\n    VALUE: 5\n    WHEN: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Assumption);
    assert_eq!(resolved.value.to_string(), "5");

    let evidence = planned.partial_plan().evidence();
    assert_eq!(evidence.len(), 1, "an applied assumption is evidence");
    assert_eq!(evidence[0].id, "assume.one");
    assert!(evidence[0].detail.contains("input.one"));
}

#[test]
fn an_assumption_never_overrides_explicit_data() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    VALUE: 1\n\nASSUME:\n    ID: assume.one\n    TARGET: REF(input.one)\n    VALUE: 5\n    WHEN: TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::DeclaredValue);
    assert_eq!(resolved.value.to_string(), "1");
    assert!(
        planned.partial_plan().evidence().is_empty(),
        "an assumption that did not apply produces no evidence"
    );
}

#[test]
fn an_assumption_whose_condition_is_false_does_not_apply() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in.txt\")\n\nASSUME:\n    ID: assume.one\n    TARGET: REF(input.one)\n    VALUE: 5\n    WHEN: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(resolution(&planned, "input.one").value, Value::Missing);
    assert!(planned.partial_plan().evidence().is_empty());
}

// ---------------------------------------------------------------------------
// The explicit-data boundary
// ---------------------------------------------------------------------------

#[test]
fn supplied_data_for_an_undeclared_id_is_reported_and_never_read() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    SOURCE: PATH(\"/ws/in.txt\")\n{SUBJECT}"
    ));
    let planned = plan_with(
        &source,
        &Invocation::new().with("input.typo", Value::Integer(nine())),
    );
    assert_eq!(planned.unused_invocation_data(), ["input.typo".to_string()]);
    assert_eq!(
        resolution(&planned, "input.one").value,
        Value::Missing,
        "a misspelled id supplies nothing to anything"
    );
}

#[test]
fn an_empty_invocation_is_the_honest_default() {
    let invocation = Invocation::new();
    assert!(invocation.is_empty());
    assert_eq!(invocation.get("anything"), None);
}

// ---------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------

#[test]
fn a_required_dependency_asserting_false_blocks_before_effects() {
    // `error.dependency.unsatisfied` is `status.blocked` and pre_effect only.
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.ready\n    TYPE: BOOLEAN\n    VALUE: FALSE\n\nDEPENDENCY:\n    ID: dependency.one\n    REFERENCE: REF(data.ready)\n    ASSERT: REF(data.ready) == TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(
        ids(&planned),
        vec!["error.dependency.unsatisfied".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(planned.terminal_status(), Some("status.blocked"));
    assert_eq!(
        planned.primary().map(|d| d.failure_phase.to_string()),
        Some("pre_effect".to_string())
    );
}

/// Regression, PRETEST-01 F02. A pre-effect reader of a declaration whose
/// value preflight left undecided must not read the UNKNOWN placeholder as a
/// decided UNKNOWN and invent `error.dependency.unsatisfied`.
#[test]
fn a_dependency_reading_an_undecided_value_invents_no_failure() {
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.ready\n    TYPE: BOOLEAN\n    VALUE: 1 / 2 == 0.5\n\nDEPENDENCY:\n    ID: dependency.one\n    REFERENCE: REF(data.ready)\n    ASSERT: REF(data.ready)\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert!(resolution(&planned, "data.ready").undecided);
    assert!(ids(&planned).is_empty(), "{:?}", ids(&planned));
}

#[test]
fn an_optional_dependency_asserting_false_does_not_block() {
    // "REQUIRED defaults TRUE" — so an explicit FALSE makes it optional.
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.ready\n    TYPE: BOOLEAN\n    VALUE: FALSE\n\nDEPENDENCY:\n    ID: dependency.one\n    REFERENCE: REF(data.ready)\n    ASSERT: REF(data.ready) == TRUE\n    REQUIRED: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_satisfied_required_dependency_does_not_block() {
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.ready\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nDEPENDENCY:\n    ID: dependency.one\n    REFERENCE: REF(data.ready)\n    ASSERT: REF(data.ready) == TRUE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_dependency_gated_off_by_a_false_when_does_not_block() {
    let source = task_document(&format!(
        "\nDATA:\n    ID: data.ready\n    TYPE: BOOLEAN\n    VALUE: FALSE\n\nDEPENDENCY:\n    ID: dependency.one\n    REFERENCE: REF(data.ready)\n    ASSERT: REF(data.ready) == TRUE\n    WHEN: FALSE\n{SUBJECT}"
    ));
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

// ---------------------------------------------------------------------------
// Workspace containment
// ---------------------------------------------------------------------------

fn workspace_scope(include: &str, exclude: Option<&str>) -> String {
    let mut body = format!(
        "\nWORKSPACE:\n    ID: workspace.one\n    PATH: PATH(\"/ws\")\n    MODE: mode.read_write\n\nSCOPE:\n    ID: scope.one\n    INCLUDE: [{include}]\n"
    );
    if let Some(exclude) = exclude {
        body.push_str(&format!("    EXCLUDE: [{exclude}]\n"));
    }
    body.push_str(SUBJECT);
    task_document(&body)
}

#[test]
fn a_scope_selecting_outside_the_workspace_is_out_of_range() {
    // "A resolved escape produces error.value.out_of_range."
    let planned = plan(&workspace_scope("PATH(\"/etc/passwd\")", None));
    assert_eq!(
        ids(&planned),
        vec!["error.value.out_of_range".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_scope_inside_the_workspace_is_accepted() {
    let planned = plan(&workspace_scope("PATH(\"/ws/src\")", None));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_textual_prefix_does_not_establish_containment() {
    // "textual prefix alone does not establish containment."
    let planned = plan(&workspace_scope("PATH(\"/ws-other/src\")", None));
    assert_eq!(ids(&planned), vec!["error.value.out_of_range".to_string()]);
}

#[test]
fn a_resolved_escape_is_caught_even_when_it_starts_inside() {
    let planned = plan(&workspace_scope("PATH(\"/ws/a/../../etc\")", None));
    assert_eq!(ids(&planned), vec!["error.value.out_of_range".to_string()]);
}

#[test]
fn a_glob_cannot_escape_the_workspace_because_the_lexer_forbids_it() {
    // "A GLOB is evaluated only against workspace-relative paths under its
    // closed profile; it cannot select an absolute path or escape the
    // WORKSPACE."
    //
    // That rule is enforced *lexically*: `02_LEXICAL`'s closed GLOB profile
    // rejects a leading slash and any `.` or `..` segment, so an escaping GLOB
    // never reaches preflight at all. This layer therefore does not re-check
    // it — the earliest-stage rule owns it — and this test pins where the rule
    // actually lives, so a later refactor cannot quietly move it.
    for pattern in ["GLOB(\"/etc/*\")", "GLOB(\"../*\")"] {
        let source = workspace_scope(pattern, None);
        let lexed = lex(&source);
        assert_eq!(
            lexed.primary().map(|d| d.id.to_string()),
            Some("error.literal.invalid".to_string()),
            "{pattern} must be refused by the closed lexical GLOB profile"
        );
    }
}

#[test]
fn a_relative_exact_path_is_decided_by_an_earlier_stage() {
    // A relative `PATH` is admissible only where the receiving field permits
    // one, which M4 decides and reports with `error.literal.invalid` under its
    // registered earlier stage. Preflight therefore only ever compares
    // *absolute* declared paths against the workspace root, and this test pins
    // that boundary so neither layer drifts into the other's rule.
    let source = workspace_scope("PATH(\"../outside\")", None);
    let resolved = resolve(&source);
    assert!(resolved.diagnostics().is_empty());
    let checked = checker().check(&resolved).expect("resolution succeeded");
    assert_eq!(
        checked
            .earlier_stage_defects()
            .iter()
            .map(|d| d.identifier.clone())
            .collect::<Vec<_>>(),
        vec!["error.literal.invalid".to_string()]
    );
}

#[test]
fn a_relative_glob_inside_the_workspace_is_accepted() {
    let planned = plan(&workspace_scope(
        "GLOB(\"src/*.rs\")",
        Some("GLOB(\"src/generated/*\")"),
    ));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

/// The exact integer `9`, built through the same arithmetic the checker uses.
fn nine() -> lcl_checker::numeric::Decimal {
    lcl_checker::numeric::Decimal::parse_integer("9").expect("9 parses")
}

// ---------------------------------------------------------------------------
// SEM-01 — an expression this layer cannot decide is not MISSING
// ---------------------------------------------------------------------------

/// `05_SEMANTICS/06` defines MISSING as "no value/source exists", and step 3
/// applies a DEFAULT only to MISSING. The preflight evaluator's own contract
/// says the same thing from the other side: it returns `None` for "an
/// expression whose value this layer cannot establish", and that `None` is
/// "**not** `MISSING`", because the caller "must leave the obligation to the
/// layer that demands it rather than inventing an outcome".
///
/// So a declaration with an explicit VALUE has a value and a source, whatever
/// this layer manages to work out about it. Turning "I could not decide this"
/// into "there is nothing here" both loses the explicit value and hands the
/// declaration to DEFAULT, which the canonical order reserves for MISSING.
///
/// The probes change only the grouping around one explicit input, so every one
/// of them is the same value written differently. The evaluator's budget is
/// read from its own behaviour rather than assumed: whichever of these it can
/// and cannot fold, the answer must never be the DEFAULT.
fn grouped_input(parentheses: usize) -> String {
    let value = format!("{}7{}", "(".repeat(parentheses), ")".repeat(parentheses));
    task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    VALUE: {value}\n    DEFAULT: 42\n{SUBJECT}"
    ))
}

#[test]
fn grouping_an_explicit_value_never_turns_it_into_the_default() {
    for parentheses in [0usize, 2, 128, 129] {
        let planned = plan_with(&grouped_input(parentheses), &Invocation::new());
        let resolved = resolution(&planned, "input.one");
        assert_ne!(
            resolved.origin,
            Origin::Default,
            "{parentheses} parentheses around an explicit 7 took the DEFAULT; a \
             declaration with an explicit VALUE is not MISSING, and DEFAULT \
             applies only to MISSING"
        );
        assert_ne!(
            resolved.value,
            Value::Missing,
            "{parentheses} parentheses around an explicit 7 resolved to MISSING"
        );
        // Either the value was established, or it is explicitly undecided.
        // There is no third answer, and in particular no quietly substituted
        // one: `05_SEMANTICS/06` gives "cannot be determined" its own name.
        assert!(
            resolved.value.to_string() == "7" || resolved.value == Value::Unknown,
            "{parentheses} parentheses around an explicit 7 resolved to {:?}",
            resolved.value
        );
    }
}

/// The same probes from the other direction: within the capacity this host
/// actually has, every spelling produces the same observable value.
#[test]
fn equivalent_groupings_that_resolve_at_all_resolve_to_the_same_value() {
    for parentheses in [0usize, 2, 128, 129] {
        let planned = plan_with(&grouped_input(parentheses), &Invocation::new());
        let resolved = resolution(&planned, "input.one");
        // Neither sentinel is a resolved value: MISSING says there is nothing
        // here and UNKNOWN says it could not be established. The claim is about
        // the groupings that did produce one.
        let decided = resolved.value != Value::Missing && resolved.value != Value::Unknown;
        if resolved.origin == Origin::DeclaredValue && decided {
            assert_eq!(
                resolved.value.to_string(),
                "7",
                "{parentheses} parentheses around 7 is still 7"
            );
        }
    }
}

/// The control the repair must not break: a declaration with genuinely no
/// value and no supplied datum is MISSING, and DEFAULT does apply to it.
#[test]
fn an_absent_optional_input_still_takes_its_default() {
    let source = task_document(&format!(
        "\nINPUT:\n    ID: input.one\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 42\n{SUBJECT}"
    ));
    let planned = plan_with(&source, &Invocation::new());
    let resolved = resolution(&planned, "input.one");
    assert_eq!(resolved.origin, Origin::Default);
    assert_eq!(resolved.value.to_string(), "42");
}

// ---------------------------------------------------------------------------
// MEASURE-01 — preflight keeps the family and the exact unit
// ---------------------------------------------------------------------------

/// `03_TYPES_AND_VALUES`: a MEASURE is "a number paired with one registered
/// unit identifier". Arithmetic that preserves the unit must preserve it
/// exactly; a result that is a bare DECIMAL is a different value family, and a
/// preflight that produces one disagrees with the runtime about what the
/// document says — which is the cross-stage agreement the value-fidelity
/// contract requires.
fn measure_value(expression: &str) -> lcl_semantics::Value {
    let source = task_document(&format!(
        "{}{SUBJECT}",
        data_block("data.measured", "MEASURE", expression)
    ));
    let planned = plan_with(&source, &Invocation::new());
    resolution(&planned, "data.measured").value.clone()
}

fn assert_quantity(value: &Value, magnitude: &str, unit: &str, what: &str) {
    let Value::Quantity(actual, actual_unit) = value else {
        panic!("{what} must stay a MEASURE, got {value:?}");
    };
    assert_eq!(actual.to_string(), magnitude, "{what}: magnitude");
    assert_eq!(actual_unit.0, unit, "{what}: exact unit identifier");
}

#[test]
fn same_unit_measure_addition_keeps_the_unit() {
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) + MEASURE(3, unit.meter)"),
        "8",
        "unit.meter",
        "5 m + 3 m",
    );
}

#[test]
fn same_unit_measure_subtraction_keeps_the_unit() {
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) - MEASURE(3, unit.meter)"),
        "2",
        "unit.meter",
        "5 m - 3 m",
    );
    assert_quantity(
        &measure_value("MEASURE(3, unit.meter) - MEASURE(5, unit.meter)"),
        "-2",
        "unit.meter",
        "a negative difference is still a measure",
    );
}

#[test]
fn a_measure_multiplied_by_a_scalar_keeps_the_unit_either_way_round() {
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) * 3"),
        "15",
        "unit.meter",
        "5 m times an INTEGER",
    );
    assert_quantity(
        &measure_value("3 * MEASURE(5, unit.meter)"),
        "15",
        "unit.meter",
        "an INTEGER times 5 m",
    );
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) * 1.5"),
        "7.5",
        "unit.meter",
        "5 m times a DECIMAL",
    );
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) * 0"),
        "0",
        "unit.meter",
        "zero metres is still metres",
    );
    assert_quantity(
        &measure_value("MEASURE(5, unit.meter) * -2"),
        "-10",
        "unit.meter",
        "a negative multiple is still a measure",
    );
}

/// The controls: ordinary numbers are still ordinary numbers.
#[test]
fn scalar_arithmetic_is_unchanged() {
    let source = task_document(&format!(
        "{}{}{SUBJECT}",
        data_block("data.whole", "INTEGER", "5 + 3"),
        data_block("data.fraction", "DECIMAL", "5 + 1.5")
    ));
    let planned = plan_with(&source, &Invocation::new());
    assert_eq!(
        resolution(&planned, "data.whole").value,
        Value::Integer(lcl_checker::numeric::Decimal::parse_integer("8").unwrap()),
        "INTEGER + INTEGER is an INTEGER"
    );
    assert_eq!(
        resolution(&planned, "data.fraction").value.to_string(),
        "6.5",
        "a DECIMAL operand still gives a DECIMAL"
    );
}

// ---------------------------------------------------------------------------
// Effective scope against the target
// ---------------------------------------------------------------------------

/// A task whose subject action names `scope.one` and targets `data.subject`.
///
/// `05_SEMANTICS/02`: "SCOPE is computed as INCLUDE minus EXCLUDE. Exact
/// references/paths identify one entity ... EXCLUDE wins within the same
/// SCOPE." `statuses_and_errors_v0.1.0.json` registers
/// `error.scope.violation` as "An action targets an entity outside applicable
/// SCOPE", pre_effect only, "effective scope resolves at processing step 6
/// before the first authorized effect".
fn scoped_action(include: &str, exclude: Option<&str>, operation: Option<&str>) -> String {
    let mut scope = format!("\nSCOPE:\n    ID: scope.one\n    INCLUDE: [{include}]\n");
    if let Some(exclude) = exclude {
        scope.push_str(&format!("    EXCLUDE: [{exclude}]\n"));
    }
    if let Some(operation) = operation {
        scope.push_str(&format!("    OPERATION: {operation}\n"));
    }
    format!(
        "{HEADER}{SUBJECT}\nDATA:\n    ID: data.other\n    TYPE: STRING\n    VALUE: \"y\"\n{scope}\
         \nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\
         \nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n    SCOPE: REF(scope.one)\n\
         \nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\
         \nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    )
}

#[test]
fn an_action_targeting_an_entity_its_scope_excludes_is_a_scope_violation() {
    // INCLUDE names only the other datum, so the subject is outside it.
    let planned = plan(&scoped_action("REF(data.other)", None, None));
    assert_eq!(
        ids(&planned),
        vec!["error.scope.violation".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn exclude_wins_over_include_within_the_same_scope() {
    let planned = plan(&scoped_action(
        "REF(data.subject), REF(data.other)",
        Some("REF(data.subject)"),
        None,
    ));
    assert_eq!(ids(&planned), vec!["error.scope.violation".to_string()]);
}

#[test]
fn an_action_targeting_an_included_entity_is_in_scope() {
    // The positive control: the same shape, with the subject included.
    let planned = plan(&scoped_action("REF(data.subject)", None, None));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_scope_restricted_to_another_operation_does_not_apply() {
    // `SCOPE.OPERATION` restricts which invocations the scope governs, so a
    // scope for `core.read` is not the applicable scope of a `core.inspect`
    // action, and must not refuse it.
    let planned = plan(&scoped_action("REF(data.other)", None, Some("core.read")));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

/// A workspace, a scope over it, and one action targeting a relative path.
///
/// `05_SEMANTICS/02`: "GLOB/REGEX select a finite set resolved before affected
/// execution. A GLOB is evaluated only against workspace-relative paths under
/// its closed profile", and `types_v0.1.0.json#/pattern_profiles/GLOB` states
/// `workspace_relative: true` with `match_semantics: full_workspace_relative_path`.
fn workspace_scoped(include: &str, exclude: Option<&str>, target: &str) -> String {
    let mut scope = format!("\nSCOPE:\n    ID: scope.one\n    INCLUDE: [{include}]\n");
    if let Some(exclude) = exclude {
        scope.push_str(&format!("    EXCLUDE: [{exclude}]\n"));
    }
    format!(
        "{HEADER}{SUBJECT}\nWORKSPACE:\n    ID: workspace.one\n    PATH: PATH(\"/ws\")\n    MODE: mode.read_write\n{scope}\
         \nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\
         \nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: {target}\n    SCOPE: REF(scope.one)\n\
         \nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\
         \nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    WORKSPACE: REF(workspace.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    )
}

/// The path form a workspace-relative target is written in.
fn relative(path: &str) -> String {
    format!("PATH(REF(workspace.one), \"{path}\")")
}

#[test]
fn an_excluding_glob_that_matches_the_target_is_a_scope_violation() {
    // "EXCLUDE wins within the same SCOPE": an action may not gain permission
    // because its restriction is written as a pattern.
    let planned = plan(&workspace_scoped(
        "GLOB(\"src/**\")",
        Some("GLOB(\"src/secret/*\")"),
        &relative("src/secret/key.txt"),
    ));
    assert_eq!(
        ids(&planned),
        vec!["error.scope.violation".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_including_glob_that_matches_the_target_admits_it() {
    let planned = plan(&workspace_scoped(
        "GLOB(\"src/**\")",
        Some("GLOB(\"src/secret/*\")"),
        &relative("src/main.py"),
    ));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_target_no_including_selector_matches_is_a_scope_violation() {
    let planned = plan(&workspace_scoped(
        "GLOB(\"src/*.py\")",
        None,
        &relative("docs/readme.md"),
    ));
    assert_eq!(ids(&planned), vec!["error.scope.violation".to_string()]);
}

#[test]
fn an_exact_include_does_not_survive_a_matching_exclude_pattern() {
    let planned = plan(&workspace_scoped(
        &relative("src/main.py"),
        Some("GLOB(\"src/*\")"),
        &relative("src/main.py"),
    ));
    assert_eq!(ids(&planned), vec!["error.scope.violation".to_string()]);
}

#[test]
fn a_glob_consumes_the_targets_normalized_relative_segments() {
    // `types_v0.1.0.json#/pattern_profiles/GLOB/input`: a PATH operand "is
    // compared using its normalized relative segment sequence".
    let planned = plan(&workspace_scoped(
        "GLOB(\"src/main.py\")",
        None,
        &relative("./src/main.py"),
    ));
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_glob_selects_no_absolute_path_because_no_root_is_inferred() {
    // "it cannot select an absolute path"; `pattern_profiles/GLOB/input`: "An
    // input that cannot supply this form ... no root is inferred." The target
    // is therefore outside a scope whose only selector is a GLOB.
    let planned = plan(&workspace_scoped(
        "GLOB(\"**\")",
        None,
        "PATH(\"/ws/src/main.py\")",
    ));
    assert_eq!(ids(&planned), vec!["error.scope.violation".to_string()]);
}

#[test]
fn a_regex_selector_matches_the_entity_it_names() {
    // `pattern_profiles/REGEX`: `match_semantics: full_string` over the text
    // that identifies the entity.
    let admitted = plan(&workspace_scoped("REGEX(\"src/.*[.]py\")", None, &relative("src/main.py")));
    assert_eq!(admitted.outcome(), Outcome::Planned, "{:?}", ids(&admitted));
    let refused = plan(&workspace_scoped(
        "REGEX(\"src/.*[.]py\")",
        Some("REGEX(\"src/secret/.*\")"),
        &relative("src/secret/key.py"),
    ));
    assert_eq!(ids(&refused), vec!["error.scope.violation".to_string()]);
}

/// Which scopes are *applicable* to an action, and why this build reads it the
/// way it does.
///
/// `statuses_and_errors_v0.1.0.json` gives `error.scope.violation` the meaning
/// "An action targets an entity outside **applicable** SCOPE" without defining
/// which scopes are applicable. `field_signatures_v0.1.0.json` — the highest
/// authority — declares `SCOPE` as an optional `reference(SCOPE)` on TASK,
/// ACTION, ALLOW and FORBID with `"default": null`, and states no propagation
/// from an enclosing block to its members. `05_SEMANTICS/02` says "Nested
/// scopes intersect unless a higher-authority rule explicitly replaces a
/// referenced scope", which says how two scopes combine *when both are in
/// force*, not which ones are.
///
/// This build therefore applies the scope an ACTION itself names. The open
/// question — whether an enclosing TASK, PHASE or SEQUENCE scope intersects
/// into its members — is recorded for the owner rather than decided here. The
/// test below is the discriminator: under the wider reading, canonical valid
/// example 04 would be refused, and `canonical/LCL_Core_0.1.0` is immutable.
#[test]
fn an_enclosing_task_scope_is_not_applied_to_a_member_action_that_declares_none() {
    let source = format!(
        "{HEADER}{SUBJECT}\nDATA:\n    ID: data.other\n    TYPE: STRING\n    VALUE: \"y\"\n\
         \nSCOPE:\n    ID: scope.narrow\n    INCLUDE: [REF(data.other)]\n\
         \nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\
         \nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\
         \nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\
         \nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    SCOPE: REF(scope.narrow)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "the action declares no SCOPE of its own: {:?}",
        ids(&planned)
    );

    // The discriminator, from the canonical package itself: example 04 declares
    // TASK SCOPE scope.source (one source file) and reaches action.test, whose
    // TARGET is PATH("/usr/bin/python3"). It must still plan.
    let example = plan_example("04_AUTOMATED_CODING_TASK.lcl");
    assert_eq!(
        example.outcome(),
        Outcome::Planned,
        "canonical valid example 04 must plan: {:?}",
        ids(&example)
    );
}
