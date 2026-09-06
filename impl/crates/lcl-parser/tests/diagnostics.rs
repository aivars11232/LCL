//! `diagnostic_selection` at the grammar stage: multiplicity, supersession,
//! duplicate suppression, `stable_order`, `location_rule` and recovery.

mod common;

use common::*;
use lcl_parser::Outcome;

#[test]
fn independent_defects_are_all_emitted() {
    // `multiplicity_rule`: "Emit every independent applicable diagnostic at the
    // selected stage. Distinct source loci … are independent even when their
    // identifiers match."
    // `ITEM` and `MODE` are registered words — the lexer accepts them — but
    // neither is a field of DATA, so each is a schema defect at its own locus.
    let src = data_doc(
        "DATA:\n    ID: data.a\n    TYPE: STRING\n    VALUE: \"v\"\n    ITEM: x\n\n\
         DATA:\n    ID: data.b\n    TYPE: STRING\n    VALUE: \"v\"\n    MODE: mode.read_only\n",
    );
    let parsed = parse(&src);
    assert_well_formed(&parsed, src.len(), "independent");
    let forbidden: Vec<_> = parsed
        .diagnostics()
        .iter()
        .filter(|d| d.id.to_string() == "error.field.forbidden")
        .collect();
    assert_eq!(
        forbidden.len(),
        2,
        "each unknown field is its own locus: {:?}",
        ids(&parsed)
    );
    assert!(forbidden[0].span.start < forbidden[1].span.start);
}

#[test]
fn diagnostics_are_ordered_by_byte_offset_then_specificity() {
    // `stable_order` ranks byte offset above specificity_rank, so an earlier
    // rank-100 diagnostic precedes a later rank-200 one.
    let src = data_doc("CONTEXT:\n    ID: context.c\n    TYPE: STRING\n    VALUE: \"a\"\n");
    let parsed = parse(&src);
    assert_well_formed(&parsed, src.len(), "ordering");
    let list = ids(&parsed);
    // CONTEXT is not legal for kind.data, and it omits SCOPE.
    assert!(
        list.iter().any(|(id, _)| id == "error.block.context"),
        "{list:?}"
    );
    assert!(
        list.iter().any(|(id, _)| id == "error.field.required"),
        "{list:?}"
    );
    let context_at = list
        .iter()
        .find(|(id, _)| id == "error.block.context")
        .unwrap()
        .1;
    let required_at = list
        .iter()
        .find(|(id, _)| id == "error.field.required")
        .unwrap()
        .1;
    assert!(
        context_at < required_at,
        "the block header precedes the end-of-block omission locus"
    );
    assert_eq!(
        parsed.primary().map(|d| d.id.to_string()),
        Some("error.block.context".to_string()),
        "the earliest locus is primary"
    );
}

#[test]
fn a_duplicate_field_supersedes_its_cardinality_diagnostic() {
    // `supersedes`: error.field.duplicate -> error.field.cardinality, and
    // duplicate carries the higher specificity_rank of 200.
    let src =
        data_doc("DATA:\n    ID: data.a\n    ID: data.b\n    TYPE: STRING\n    VALUE: \"v\"\n");
    let parsed = parse(&src);
    assert_well_formed(&parsed, src.len(), "supersession");
    let list = id_list(&parsed);
    assert!(
        list.contains(&"error.field.duplicate".to_string()),
        "{list:?}"
    );
    assert!(
        !list.contains(&"error.field.cardinality".to_string()),
        "the superseded identifier must not survive: {list:?}"
    );
    let duplicate = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.field.duplicate")
        .unwrap();
    assert_eq!(duplicate.specificity_rank, 200);
    // The locus is the repeated key, not the first occurrence.
    assert_eq!(duplicate.span.slice(&src), Some("ID"));
    assert!(duplicate.span.start > src.find("ID: data.a").unwrap());
}

#[test]
fn an_omitted_field_uses_the_location_rule_locus() {
    // "An omitted field or child block uses the zero-width byte position at the
    // first following nonblank line whose indentation is not greater than the
    // parent block header".
    let src =
        data_doc("DATA:\n    ID: data.a\n    VALUE: \"v\"\n\nCOMMENT:\n    CONTENT: \"after\"\n");
    let parsed = parse(&src);
    assert_well_formed(&parsed, src.len(), "omission");
    let required = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.field.required")
        .expect("DATA requires TYPE");
    assert!(
        required.span.is_empty(),
        "the locus is a position, not a lexeme"
    );
    let at = src.find("COMMENT:").expect("the following top-level line");
    assert_eq!(
        required.span.start, at,
        "the locus is the first following line at or below the parent indentation"
    );
}

#[test]
fn an_omission_with_no_following_line_uses_end_of_file() {
    let src = data_doc("DATA:\n    ID: data.a\n    VALUE: \"v\"\n");
    let parsed = parse(&src);
    let required = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.field.required")
        .expect("DATA requires TYPE");
    assert_eq!(required.span.start, src.len());
    assert!(required.span.is_empty());
}

#[test]
fn recovery_preserves_later_independent_evidence() {
    // A malformed statement must not swallow the rest of the block.
    let src = data_doc("DATA:\n    ID data.a\n    TYPE: STRING\n    VALUE: \"v\"\n    ITEM: x\n");
    let parsed = parse(&src);
    assert_well_formed(&parsed, src.len(), "recovery");
    let list = id_list(&parsed);
    assert!(
        list.contains(&"error.grammar.invalid".to_string()),
        "{list:?}"
    );
    assert!(
        list.contains(&"error.field.forbidden".to_string()),
        "the later unknown field is still reported: {list:?}"
    );
    let block = parsed.document().block("DATA").expect("DATA");
    assert!(block.field("TYPE").is_some(), "later statements survive");
    assert!(block.field("VALUE").is_some());
}

#[test]
fn recovery_never_fabricates_a_node() {
    // The malformed statement contributes nothing: no invented key, value or
    // identifier appears in the tree.
    let src = data_doc("DATA:\n    ID data.a\n    TYPE: STRING\n    VALUE: \"v\"\n");
    let parsed = parse(&src);
    let block = parsed.document().block("DATA").expect("DATA");
    assert!(
        block.field("ID").is_none(),
        "the unparsable `ID data.a` line must yield no field"
    );
    let keys: Vec<&str> = block
        .body
        .iter()
        .filter_map(|s| match s {
            lcl_parser::syntax::Statement::Field(f) => Some(f.key.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["TYPE", "VALUE"], "only real source appears");
}

#[test]
fn every_diagnostic_is_reproducible_and_stable() {
    let src = data_doc("CONTEXT:\n    ID: context.c\n    TYPE: STRING\n    VALUE: \"a\"\n");
    let first = parse(&src);
    for _ in 0..8 {
        assert_eq!(first.diagnostics(), parse(&src).diagnostics());
    }
}

#[test]
fn a_clean_parse_reports_no_status() {
    let parsed = parse(&data_doc(
        "DATA:\n    ID: data.a\n    TYPE: STRING\n    VALUE: \"v\"\n",
    ));
    assert_eq!(parsed.outcome(), Outcome::Parsed);
    assert!(parsed.primary().is_none());
    assert_eq!(parsed.terminal_status(), None);
}

#[test]
fn a_rejected_parse_carries_the_registered_default_status() {
    let parsed = parse(&data_doc("DATA:\n    ID: data.a\n    VALUE: \"v\"\n"));
    assert_eq!(parsed.outcome(), Outcome::Rejected);
    assert_eq!(parsed.terminal_status(), Some("status.invalid"));
}
