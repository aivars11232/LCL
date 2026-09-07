//! Step 6: effective authority, priority, override and hard conflicts.
//!
//! Authority: `05_SEMANTICS/04` and `05_SEMANTICS/03`.

mod common;

use common::*;
use lcl_semantics::{Outcome, RuleKind};
use std::fs;

fn records(source: &str) -> Vec<(String, u32, i32, RuleKind)> {
    let planned = plan(source);
    planned
        .partial_plan()
        .authorities()
        .iter()
        .map(|r| (r.id.clone(), r.authority, r.priority, r.kind))
        .collect()
}

// ---------------------------------------------------------------------------
// Effective authority
// ---------------------------------------------------------------------------

#[test]
fn a_clause_with_no_authority_takes_the_document_default_of_500() {
    // `field_signatures#/blocks/SPECIFICATION/fields/AUTHORITY/default` is 500,
    // and every clause's own AUTHORITY default is null, so an undeclared clause
    // authority is the document's — which here is itself undeclared, hence 500.
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n"
    );
    let found = records(&source);
    assert_eq!(
        found,
        vec![("rule.one".to_string(), 500, 0, RuleKind::Require)]
    );
}

#[test]
fn a_clause_with_no_authority_takes_its_documents_declared_authority() {
    let source = format!(
        "{LIBRARY}    AUTHORITY: 700\n\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n"
    );
    let found = records(&source);
    assert_eq!(found[0].1, 700);
}

#[test]
fn an_explicit_clause_authority_overrides_the_document_authority() {
    let source = format!(
        "{LIBRARY}    AUTHORITY: 700\n\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n    AUTHORITY: 200\n"
    );
    let found = records(&source);
    assert_eq!(found[0].1, 200);
}

// ---------------------------------------------------------------------------
// Priority
// ---------------------------------------------------------------------------

#[test]
fn an_absent_priority_is_zero_and_never_inherits() {
    // "When PRIORITY is optional and MISSING, its value is 0. PRIORITY never
    // inherits from an enclosing or referenced clause."
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n    PRIORITY: 400\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) != FALSE\n"
    );
    let found = records(&source);
    assert_eq!(found[0].2, 400, "the declaring clause keeps its priority");
    assert_eq!(found[1].2, 0, "the neighbouring clause inherits nothing");
}

#[test]
fn a_negative_priority_is_read_exactly() {
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nPREFER:\n    ID: rule.soft\n    ASSERT: REF(data.flag) == TRUE\n    PRIORITY: -250\n"
    );
    let found = records(&source);
    assert_eq!(found[0].2, -250);
}

// ---------------------------------------------------------------------------
// Hard conflicts
// ---------------------------------------------------------------------------

#[test]
fn the_canonical_hard_conflict_example_is_rejected_here() {
    // `08_HARD_CONFLICT.invalid.lcl` is the identifier M3 deferred to M5 by
    // name. This is the test that closes that hand-off.
    let path = canonical_root().join("08_EXAMPLES/INVALID/08_HARD_CONFLICT.invalid.lcl");
    let source = fs::read_to_string(&path).expect("the canonical example is readable");
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Rejected);
    assert_eq!(
        ids(&planned),
        vec!["error.conflict.hard".to_string()],
        "exactly the identifier the example's .expected.txt pins"
    );
    assert_eq!(planned.terminal_status(), Some("status.invalid"));
    assert!(
        planned.plan().is_none(),
        "a rejected preflight plans nothing"
    );
}

#[test]
fn two_hard_assertions_agreeing_are_not_a_conflict() {
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) == TRUE\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn contradictory_clauses_at_different_authority_are_resolved_not_reported() {
    // "higher authority wins only where clauses conflict; otherwise all remain."
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n    AUTHORITY: 700\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) == FALSE\n    AUTHORITY: 300\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_soft_clause_never_creates_a_hard_conflict() {
    // "PREFER is soft and cannot authorize an action or defeat hard rules."
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n\nPREFER:\n    ID: rule.soft\n    ASSERT: REF(data.flag) == FALSE\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn an_exact_override_resolves_a_hard_conflict() {
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) == FALSE\n\nOVERRIDE:\n    ID: override.flag\n    WINNER: REF(rule.one)\n    LOSER: REF(rule.two)\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn an_override_naming_an_unrelated_pair_does_not_resolve_the_conflict() {
    // "OVERRIDE never applies by similarity or broad category."
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nDATA:\n    ID: data.other\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) == FALSE\n\nREQUIRE:\n    ID: rule.three\n    ASSERT: REF(data.other) == TRUE\n\nOVERRIDE:\n    ID: override.other\n    WINNER: REF(rule.one)\n    LOSER: REF(rule.three)\n"
    );
    let planned = plan(&source);
    assert_eq!(ids(&planned), vec!["error.conflict.hard".to_string()]);
}

#[test]
fn a_lower_authority_winner_is_invalid() {
    // "A lower-authority winner is invalid."
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.flag\n    TYPE: BOOLEAN\n    VALUE: TRUE\n\nREQUIRE:\n    ID: rule.one\n    ASSERT: REF(data.flag) == TRUE\n    AUTHORITY: 200\n\nREQUIRE:\n    ID: rule.two\n    ASSERT: REF(data.flag) == FALSE\n    AUTHORITY: 800\n\nOVERRIDE:\n    ID: override.flag\n    WINNER: REF(rule.one)\n    LOSER: REF(rule.two)\n"
    );
    let planned = plan(&source);
    assert!(
        ids(&planned).contains(&"error.override.invalid".to_string()),
        "{:?}",
        ids(&planned)
    );
    assert_eq!(planned.terminal_status(), Some("status.invalid"));
}

// ---------------------------------------------------------------------------
// Permission versus prohibition
// ---------------------------------------------------------------------------

#[test]
fn the_canonical_authority_override_example_plans() {
    // `08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl` declares an ALLOW and a
    // FORBID over the same operation and target at equal authority 500, and an
    // exact OVERRIDE naming the winner and loser. It must plan.
    let path =
        canonical_root().join("08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl");
    let source = fs::read_to_string(&path).expect("the canonical example is readable");
    let planned = plan(&source);
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "the canonical valid example must plan: {:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );

    let authorities = planned.partial_plan().authorities();
    let allow = authorities
        .iter()
        .find(|r| r.id == "permission.download")
        .expect("the ALLOW is recorded");
    let forbid = authorities
        .iter()
        .find(|r| r.id == "rule.default_no_download")
        .expect("the FORBID is recorded");
    assert_eq!((allow.authority, allow.priority), (500, 0));
    assert_eq!((forbid.authority, forbid.priority), (500, 0));
    assert_eq!(allow.kind, RuleKind::Allow);
    assert_eq!(forbid.kind, RuleKind::Forbid);
}

#[test]
fn an_allow_does_not_defeat_a_forbid_but_is_not_itself_a_hard_conflict() {
    // "ALLOW ... Never defeats FORBID by itself." An unexercised permission is
    // not a contradiction; whether the action can run is decided where the
    // action is authorized.
    let source = format!(
        "{LIBRARY}\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\nALLOW:\n    ID: permission.inspect\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nFORBID:\n    ID: rule.no_inspect\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}

#[test]
fn a_forbid_over_a_different_target_is_not_matched() {
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.one\n    TYPE: STRING\n    VALUE: \"x\"\n\nDATA:\n    ID: data.two\n    TYPE: STRING\n    VALUE: \"y\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.one)\n\nREQUIRE:\n    ID: rule.act\n    ACTION: REF(action.one)\n\nFORBID:\n    ID: rule.no\n    OPERATION: core.inspect\n    TARGET: REF(data.two)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert!(
        !ids(&planned).contains(&"error.conflict.hard".to_string()),
        "a FORBID must never be widened onto a target it does not name: {:?}",
        ids(&planned)
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn authority_records_are_identical_across_repeated_runs() {
    let path =
        canonical_root().join("08_EXAMPLES/VALID/08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl");
    let source = fs::read_to_string(&path).expect("readable");
    let first = plan(&source);
    let second = plan(&source);
    assert_eq!(
        first.partial_plan().authorities(),
        second.partial_plan().authorities()
    );
}

#[test]
fn a_required_action_forbidden_at_equal_authority_is_a_hard_conflict() {
    // "FORBID blocks matching action even when an ACTION requires it unless a
    // valid OVERRIDE resolves the exact conflict." Two hard clauses, equal
    // authority, no override: `error.conflict.hard`.
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.one\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.one)\n\nREQUIRE:\n    ID: rule.act\n    ACTION: REF(action.one)\n\nFORBID:\n    ID: rule.no\n    OPERATION: core.inspect\n    TARGET: REF(data.one)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(
        ids(&planned),
        vec!["error.conflict.hard".to_string()],
        "{:?}",
        planned
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_exact_override_lets_a_required_forbidden_action_proceed() {
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.one\n    TYPE: STRING\n    VALUE: \"x\"\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.one)\n\nREQUIRE:\n    ID: rule.act\n    ACTION: REF(action.one)\n\nFORBID:\n    ID: rule.no\n    OPERATION: core.inspect\n    TARGET: REF(data.one)\n\nOVERRIDE:\n    ID: override.act\n    WINNER: REF(rule.act)\n    LOSER: REF(rule.no)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    );
    let planned = plan(&source);
    assert_eq!(planned.outcome(), Outcome::Planned, "{:?}", ids(&planned));
}
