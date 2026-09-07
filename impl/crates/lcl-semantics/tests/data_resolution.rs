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
