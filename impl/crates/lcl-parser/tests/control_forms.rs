//! `CONDITIONAL`, `FOR_EACH`, `STEP_BLOCK`, `COMMENT_BLOCK` and the
//! structurally decidable conditional requirements.
//!
//! No canonical valid example writes an `IF`/`ELSE` or a `COMMENT` block, so
//! those forms are exercised here against `04_GRAMMAR/04` and the EBNF
//! directly.

mod common;

use common::*;
use lcl_parser::syntax::*;
use lcl_parser::Outcome;

/// A `kind.task` document whose SEQUENCE body is `body`.
fn sequence(body: &str) -> String {
    task_doc(&format!(
        "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.s\n{body}\n\
         \nSUCCESS:\n    ID: success.s\n    ALL: [REF(goal.g)]\n\
         \nTASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    SEQUENCE: REF(sequence.s)\n    SUCCESS: REF(success.s)\n"
    ))
}

#[test]
fn a_conditional_parses_both_arms() {
    let src = sequence(
        "    IF (REF(input.flag) == TRUE) THEN:\n\
         \x20       STEP:\n            ID: step.a\n            ACTION: REF(action.a)\n\
         \x20   ELSE:\n\
         \x20       STEP:\n            ID: step.b\n            ACTION: REF(action.b)\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));

    let sequence_block = parsed.document().block("SEQUENCE").expect("SEQUENCE");
    let conditional = sequence_block
        .body
        .iter()
        .find_map(|s| match s {
            Statement::Conditional(c) => Some(c),
            _ => None,
        })
        .expect("the IF is a statement of the SEQUENCE body");

    assert_eq!(conditional.keyword_span.slice(&src), Some("IF"));
    assert!(matches!(*conditional.condition, Expr::Binary(_)));
    assert_eq!(conditional.then_body.len(), 1);
    let arm = conditional.else_body.as_ref().expect("ELSE is present");
    assert_eq!(arm.keyword_span.slice(&src), Some("ELSE"));
    assert_eq!(arm.body.len(), 1);
}

#[test]
fn else_is_optional() {
    let src = sequence(
        "    IF (TRUE) THEN:\n\
         \x20       STEP:\n            ID: step.a\n            ACTION: REF(action.a)\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let conditional = parsed
        .document()
        .block("SEQUENCE")
        .and_then(|b| {
            b.body.iter().find_map(|s| match s {
                Statement::Conditional(c) => Some(c.clone()),
                _ => None,
            })
        })
        .expect("conditional");
    assert!(conditional.else_body.is_none());
}

#[test]
fn a_for_each_binds_one_simple_identifier() {
    let src = sequence(
        "    FOR EACH item IN REF(input.items):\n\
         \x20       STEP:\n            ID: step.a\n            ACTION: REF(action.a)\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let for_each = parsed
        .document()
        .block("SEQUENCE")
        .and_then(|b| {
            b.body.iter().find_map(|s| match s {
                Statement::ForEach(f) => Some(f.clone()),
                _ => None,
            })
        })
        .expect("for each");
    assert_eq!(for_each.binding.text, "item");
    assert!(!for_each.binding.qualified);
    assert!(matches!(*for_each.collection, Expr::Call(_)));
    assert_eq!(for_each.body.len(), 1);
}

#[test]
fn for_each_has_no_key_or_comparator_syntax() {
    // "FOR EACH has no key or comparator syntax; those rules belong only to
    // core.sort." — 04_GRAMMAR/04
    let src = sequence(
        "    FOR EACH item, other IN REF(input.items):\n\
         \x20       STEP:\n            ID: step.a\n            ACTION: REF(action.a)\n",
    );
    let parsed = parse(&src);
    assert!(
        id_list(&parsed).contains(&"error.grammar.invalid".to_string()),
        "{:?}",
        ids(&parsed)
    );
}

#[test]
fn control_forms_nest_inside_one_another() {
    let src = sequence(
        "    FOR EACH item IN REF(input.items):\n\
         \x20       IF (TRUE) THEN:\n\
         \x20           STEP:\n                ID: step.a\n                ACTION: REF(action.a)\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let for_each = parsed
        .document()
        .block("SEQUENCE")
        .and_then(|b| {
            b.body.iter().find_map(|s| match s {
                Statement::ForEach(f) => Some(f.clone()),
                _ => None,
            })
        })
        .expect("for each");
    assert!(matches!(for_each.body[0], Executable::Conditional(_)));
}

#[test]
fn a_branch_body_admits_only_its_registered_blocks() {
    // "Branch bodies contain STEP, nested IF, nested FOR EACH, or COMMENT." —
    // 04_GRAMMAR/04. DATA declares no IF/FOR_EACH/ELSE parent, so it is not one.
    let src = sequence(
        "    IF (TRUE) THEN:\n\
         \x20       DATA:\n            ID: data.x\n            TYPE: STRING\n            VALUE: \"v\"\n",
    );
    let parsed = parse(&src);
    assert!(
        id_list(&parsed).contains(&"error.block.field".to_string()),
        "{:?}",
        ids(&parsed)
    );
}

#[test]
fn a_comment_block_is_legal_in_a_branch_body() {
    let src = sequence(
        "    IF (TRUE) THEN:\n\
         \x20       COMMENT:\n            CONTENT: \"why this branch exists\"\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
}

#[test]
fn if_and_for_each_are_legal_only_in_phase_sequence_or_a_branch() {
    // 04_GRAMMAR/11 rule 4. A DATA block is none of those.
    let parsed = parse(&data_doc(
        "DATA:\n    ID: data.x\n    TYPE: STRING\n    IF (TRUE) THEN:\n        STEP:\n            ID: step.a\n            ACTION: REF(action.a)\n",
    ));
    assert!(
        id_list(&parsed).contains(&"error.block.field".to_string()),
        "{:?}",
        ids(&parsed)
    );
}

// -- conditional requirements ---------------------------------------------

#[test]
fn exactly_one_of_a_required_group_is_enforced() {
    // CONTEXT: "Exactly one VALUE or SOURCE."
    let both = task_doc(
        "SCOPE:\n    ID: scope.s\n    INCLUDE: REF(task.t)\n\n\
         CONTEXT:\n    ID: context.c\n    TYPE: STRING\n    SCOPE: REF(scope.s)\n    VALUE: \"a\"\n    SOURCE: PATH(\"/tmp/a\")\n",
    );
    assert!(
        id_list(&parse(&both)).contains(&"error.block.conditional_requirement".to_string()),
        "two of an exactly-one group must be reported"
    );

    let neither = task_doc(
        "SCOPE:\n    ID: scope.s\n    INCLUDE: REF(task.t)\n\n\
         CONTEXT:\n    ID: context.c\n    TYPE: STRING\n    SCOPE: REF(scope.s)\n",
    );
    assert!(
        id_list(&parse(&neither)).contains(&"error.block.conditional_requirement".to_string()),
        "none of an exactly-one group must be reported"
    );
}

#[test]
fn success_selects_exactly_one_quantifier() {
    let src = task_doc(
        "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n\
         SUCCESS:\n    ID: success.s\n    ALL: [REF(goal.g)]\n    ANY: [REF(goal.g)]\n\n\
         TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    ACTION: REF(action.a)\n    SUCCESS: REF(success.s)\n",
    );
    assert!(
        id_list(&parse(&src)).contains(&"error.block.conditional_requirement".to_string()),
        "SUCCESS contains exactly ALL, ANY, or NONE"
    );
}

#[test]
fn reference_only_fields_reject_an_inline_block() {
    // TASK: "PHASE, SEQUENCE, and ACTION fields are reference/list-only;
    // inline child blocks are forbidden."
    let src = task_doc(
        "GOAL:\n    ID: goal.g\n    ASSERT: TRUE\n\n\
         SUCCESS:\n    ID: success.s\n    ALL: [REF(goal.g)]\n\n\
         TASK:\n    ID: task.t\n    GOAL: REF(goal.g)\n    SUCCESS: REF(success.s)\n\
         \x20   ACTION:\n        ID: action.inline\n        OPERATION: core.inspect\n",
    );
    let list = id_list(&parse(&src));
    assert!(
        list.contains(&"error.block.conditional_requirement".to_string())
            || list.contains(&"error.field.type".to_string()),
        "an inline ACTION under TASK must be reported: {list:?}"
    );
}

#[test]
fn a_uri_import_source_requires_a_checksum() {
    // IMPORT: "URI source requires CHECKSUM."
    let without = data_doc(
        "IMPORT:\n    ID: import.i\n    SOURCE: URI(\"https://example.invalid/lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n",
    );
    assert!(
        id_list(&parse(&without)).contains(&"error.block.conditional_requirement".to_string()),
        "a URI source without CHECKSUM must be reported"
    );

    let with = data_doc(
        "IMPORT:\n    ID: import.i\n    SOURCE: URI(\"https://example.invalid/lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n    CHECKSUM: \"sha256:0000000000000000000000000000000000000000000000000000000000000000\"\n",
    );
    let list = id_list(&parse(&with));
    assert!(
        !list.contains(&"error.block.conditional_requirement".to_string()),
        "a URI source with CHECKSUM is well formed: {list:?}"
    );

    // A PATH source needs none.
    let path = data_doc(
        "IMPORT:\n    ID: import.i\n    SOURCE: PATH(\"lib.lcl\")\n    NAMESPACE: lib\n    VERSION: \"1.0.0\"\n",
    );
    assert!(!id_list(&parse(&path)).contains(&"error.block.conditional_requirement".to_string()));
}

#[test]
fn item_is_legal_only_under_a_kind_type_enum_definition() {
    // DEFINE: "ITEM is legal only for KIND kind.type with BASE ENUM …"
    let legal = data_doc(
        "DEFINE:\n    ID: type.state\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: ready\n    ITEM: done\n",
    );
    let list = id_list(&parse(&legal));
    assert!(
        !list.contains(&"error.block.conditional_requirement".to_string()),
        "an enum definition may carry ITEM: {list:?}"
    );

    let illegal = data_doc(
        "DEFINE:\n    ID: constant.c\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 1\n    ITEM: ready\n",
    );
    assert!(
        id_list(&parse(&illegal)).contains(&"error.block.conditional_requirement".to_string()),
        "ITEM outside a kind.type ENUM definition must be reported"
    );
}

#[test]
fn every_enforced_requirement_string_is_still_registered() {
    // The enforcement table quotes registry prose verbatim. If a release
    // rewords or withdraws a requirement, the rule must stop matching rather
    // than enforce a string the specification no longer states.
    let g = grammar();
    for (block, text) in lcl_parser::enforced_requirements() {
        let schema = g.schema(block).unwrap_or_else(|| panic!("{block}"));
        assert!(
            schema.conditional_requirements.iter().any(|c| c == text),
            "{block} no longer declares {text:?}"
        );
    }
}

#[test]
fn the_deferred_boundary_is_published() {
    let deferred = lcl_parser::deferred_requirements();
    assert!(!deferred.is_empty());
    for (area, owner) in deferred {
        assert!(!area.is_empty() && !owner.is_empty());
    }
}
