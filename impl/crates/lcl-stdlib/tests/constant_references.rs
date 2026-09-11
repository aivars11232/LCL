//! A `REF` to a `DEFINE kind.constant` in an operation parameter.
//!
//! Regression, post-Task-20 finding F14. `05_SEMANTICS/12` lists exactly what a
//! reference reads in a value context:
//!
//! > REF reads exactly one bound value of INPUT, DATA, CONTEXT, MEMORY, STATE,
//! > OUTPUT, DEFINE kind.constant, or a loop-local binding.
//!
//! Every stage honoured that except one. M4 records a constant's statically
//! known value and judges the reference as a value context; M6 reads a
//! declaration's value from the plan's resolutions; and M5 resolved only
//! `INPUT`, `DATA`, `CONTEXT`, `MEMORY` and `STATE` into them. So the read fell
//! through to `MISSING`, the parameter quietly took its registered default, and
//! nothing said so.
//!
//! The defect was invisible from the outside precisely because it was silent,
//! which is why these cases compare a reference against the literal it stands
//! for rather than only asserting that it is accepted.

mod common;

use common::run;

const SUBJECT: &str = "\nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"abc\"\n";

/// One `core.inspect` whose `depth` parameter is written as `value`.
///
/// `depth` is registered `INTEGER` with the declared bound `0..100` and the
/// default `1`. The bound is what makes a silent failure visible: a value past
/// it is refused, and `params::check_bounds` only sees a value that actually
/// materialised as an INTEGER.
fn inspect_depth(declarations: &str, value: &str) -> Vec<String> {
    let action = format!(
        "ID: action.inspect\nOPERATION: core.inspect\nTARGET: REF(data.subject)\n\
         PARAMETER:\n    NAME: depth\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    VALUE: {value}"
    );
    let execution = run(&common::task(declarations, &[&action]));
    common::errors_of(&execution, "action.inspect")
}

/// A document declaring one `kind.constant` of the given type and value.
fn with_constant(ty: &str, value: &str) -> String {
    format!(
        "{SUBJECT}\nDEFINE:\n    ID: constant.depth\n    KIND: kind.constant\n    \
         TYPE: {ty}\n    VALUE: {value}\n"
    )
}

#[test]
fn a_constant_reference_and_its_literal_agree() {
    // Both admitted, and neither refused.
    assert_eq!(inspect_depth(SUBJECT, "2"), Vec::<String>::new());
    assert_eq!(
        inspect_depth(&with_constant("INTEGER", "2"), "REF(constant.depth)"),
        Vec::<String>::new(),
        "a constant reference must be admitted wherever its literal is"
    );

    // Both past the registered bound, and both refused for it. This is the
    // half that was silently wrong: the reference resolved to MISSING, the
    // bound check had no INTEGER to judge, and the operation ran with its
    // default depth instead.
    let expected = vec!["error.value.out_of_range".to_string()];
    assert_eq!(inspect_depth(SUBJECT, "500"), expected);
    assert_eq!(
        inspect_depth(&with_constant("INTEGER", "500"), "REF(constant.depth)"),
        expected,
        "a constant reference must be judged by the value it reads, not skipped"
    );
}

#[test]
fn a_constant_reference_carries_the_exact_declared_value() {
    // Each bound edge, through a reference. `0..100` admits both ends, so an
    // off-by-one in the read would show here rather than only at 500.
    for (literal, refused) in [("0", false), ("100", false), ("101", true)] {
        let through_reference =
            inspect_depth(&with_constant("INTEGER", literal), "REF(constant.depth)");
        let through_literal = inspect_depth(SUBJECT, literal);
        assert_eq!(
            through_reference, through_literal,
            "depth {literal} disagrees between a reference and its literal"
        );
        assert_eq!(
            !through_reference.is_empty(),
            refused,
            "depth {literal} is {}",
            match refused {
                true => "outside the declared bound",
                false => "inside the declared bound",
            }
        );
    }
}

#[test]
fn a_reference_to_nothing_is_still_rejected_at_the_resolution_stage() {
    // The repair reads a declared value; it does not invent one. An unresolved
    // reference is a resolution-stage defect and stays one, which is asserted
    // through the resolver directly because the shared fixture deliberately
    // refuses to build a pipeline over a document that does not resolve.
    use lcl_resolver::{MemoryProvider, Resolver, SourceId, SourceUnit};

    let source = common::task(
        SUBJECT,
        &["ID: action.inspect\nOPERATION: core.inspect\nTARGET: REF(constant.absent)"],
    );
    let unit = SourceUnit::new(SourceId::new("root.lcl"), source.as_bytes());
    let resolved = Resolver::new(common::rules(), common::grammar(), common::lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed");
    let ids: Vec<String> = resolved
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect();
    assert!(
        ids.iter().any(|id| id == "error.reference.unresolved"),
        "an unresolved reference must still be refused: {ids:?}"
    );
}

/// Every static diagnostic one document produces, by identifier.
fn static_ids(source: &str) -> Vec<String> {
    use lcl_checker::Checker;
    use lcl_resolver::{MemoryProvider, Resolver, SourceId, SourceUnit};

    let unit = SourceUnit::new(SourceId::new("root.lcl"), source.as_bytes());
    let resolved = Resolver::new(common::rules(), common::grammar(), common::lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed");
    assert!(
        resolved.primary().is_none(),
        "this fixture is meant to resolve: {:?}",
        resolved.primary().map(|d| d.id.to_string())
    );
    Checker::new(common::static_contracts())
        .check(&resolved)
        .expect("resolution succeeded")
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect()
}

#[test]
fn a_constant_of_the_wrong_family_is_still_refused_at_the_static_stage() {
    // Type-family validation is unchanged, and it is stricter than the read:
    // a STRING constant standing where an INTEGER is registered never reaches
    // execution at all. Reading a constant's value does not mean accepting
    // whatever the value turned out to be.
    let source = common::task(
        &with_constant("STRING", "\"deep\""),
        &[
            "ID: action.inspect\nOPERATION: core.inspect\nTARGET: REF(data.subject)\n\
           PARAMETER:\n    NAME: depth\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    \
           VALUE: REF(constant.depth)",
        ],
    );
    let ids = static_ids(&source);
    assert!(
        ids.iter().any(|id| id == "error.type.mismatch"),
        "a STRING constant is not an INTEGER depth: {ids:?}"
    );

    // And the equivalent literal is refused the same way, so the reference is
    // not being judged by a different rule from the value it stands for.
    let literal = common::task(
        SUBJECT,
        &[
            "ID: action.inspect\nOPERATION: core.inspect\nTARGET: REF(data.subject)\n\
           PARAMETER:\n    NAME: depth\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    \
           VALUE: \"deep\"",
        ],
    );
    assert_eq!(
        static_ids(&literal),
        ids,
        "a constant reference and its literal must be refused identically"
    );
}
