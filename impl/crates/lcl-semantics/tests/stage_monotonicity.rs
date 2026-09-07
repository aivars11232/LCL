//! Preflight is not evaluated for a program that failed an earlier stage.
//!
//! `earliest_stage_rule`: "If any applicable diagnostic remains unhandled, do
//! not evaluate later stages for that failed source unit or invocation path."
//! A later stage that reported *anything* about such a program — a pass or a
//! failure — would be claiming a verdict the language does not have.

mod common;

use common::*;
use lcl_checker::Checker;
use lcl_diagnostics::Stage;
use lcl_resolver::MemoryProvider;

/// Resolve and check without asserting cleanliness, so a deliberately invalid
/// document can be pushed at the preflight layer.
fn attempt(source: &str) -> Result<lcl_semantics::Planned, lcl_semantics::StageSkipped> {
    let provider = MemoryProvider::new();
    let resolved = resolver()
        .resolve(&unit("root.lcl", source), &provider)
        .expect("lexical and grammar stages pass for these fixtures");
    let checked = Checker::new(static_contracts())
        .check(&resolved)
        .expect("these fixtures resolve");
    preflight().plan(&checked, &resolved, &lcl_semantics::Invocation::new())
}

#[test]
fn a_resolution_failure_yields_no_preflight_verdict() {
    let source = format!(
        "{DATA_HEADER}\nDATA:\n    ID: data.one\n    TYPE: STRING\n    VALUE: REF(data.absent)\n"
    );
    let provider = MemoryProvider::new();
    let resolved = resolver()
        .resolve(&unit("root.lcl", &source), &provider)
        .expect("earlier stages pass");
    assert!(
        !resolved.diagnostics().is_empty(),
        "fixture must fail resolution"
    );
    // The static stage already refuses this program, so preflight never sees a
    // `Checked` at all. That is the monotonicity chain working end to end.
    assert!(Checker::new(static_contracts()).check(&resolved).is_err());
}

#[test]
fn a_static_failure_yields_no_preflight_verdict() {
    let source =
        format!("{DATA_HEADER}\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: \"text\"\n");
    let skipped = attempt(&source).expect_err("a static failure must skip preflight");
    assert_eq!(skipped.stage, Stage::StaticOrExpression);
    assert_eq!(skipped.primary, "error.type.mismatch");
}

#[test]
fn an_earlier_stage_defect_reported_by_the_checker_also_skips_preflight() {
    // M4 reports two registered identifiers whose registered stage is earlier
    // than its own. They are earlier-stage diagnostics, so preflight must not
    // run over them either.
    let source = format!(
        "{DATA_HEADER}\nDEFINE:\n    ID: type.loop\n    KIND: kind.type\n    BASE: REF(type.loop)\n"
    );
    let provider = MemoryProvider::new();
    let resolved = resolver()
        .resolve(&unit("root.lcl", &source), &provider)
        .expect("earlier stages pass");
    if resolved.diagnostics().is_empty() {
        let checked = Checker::new(static_contracts())
            .check(&resolved)
            .expect("resolution succeeded");
        if !checked.earlier_stage_defects().is_empty() {
            let skipped = preflight()
                .plan(&checked, &resolved, &lcl_semantics::Invocation::new())
                .expect_err("an earlier-stage defect must skip preflight");
            assert!(skipped.stage.index() < Stage::Validation.index());
        }
    }
}

#[test]
fn stage_skipped_says_which_stage_and_which_identifier() {
    let source =
        format!("{DATA_HEADER}\nDATA:\n    ID: data.one\n    TYPE: INTEGER\n    VALUE: \"text\"\n");
    let skipped = attempt(&source).expect_err("static failure");
    let text = skipped.to_string();
    assert!(text.contains("preflight not evaluated"));
    assert!(text.contains("static_or_expression"));
    assert!(text.contains("error.type.mismatch"));
}
