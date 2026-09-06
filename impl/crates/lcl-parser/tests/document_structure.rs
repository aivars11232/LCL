//! `DOCUMENT`, `CORE_BLOCK`, `FIELD_LINE`, `NESTED_FIELD` and
//! `OBJECT_PROPERTY`: the shapes of `04_GRAMMAR/01` and `/02`.

mod common;

use common::*;
use lcl_parser::syntax::*;
use lcl_parser::Outcome;

#[test]
fn a_minimal_document_parses_to_its_two_headers() {
    let parsed = parse(&data_doc(""));
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let items = &parsed.document().items;
    assert_eq!(items.len(), 2);
    let names: Vec<&str> = parsed.document().blocks().map(|b| b.key.text.as_str()).collect();
    assert_eq!(names, ["LCL", "SPECIFICATION"]);
}

#[test]
fn a_colon_and_space_opens_an_inline_value_and_a_colon_and_newline_opens_a_block() {
    // "A colon followed by NEWLINE opens one indented block. A colon followed
    // by one space and value is inline." — 04_GRAMMAR/02
    let parsed = parse(&data_doc(
        "DATA:\n    ID: data.x\n    TYPE: OBJECT\n    VALUE:\n        k: 1\n",
    ));
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let block = parsed.document().block("DATA").expect("DATA");
    assert!(matches!(
        block.field("ID").map(|f| &f.body),
        Some(Body::Inline(_))
    ));
    let value = block.field("VALUE").expect("VALUE");
    let nested = value.body.as_nested().expect("VALUE opens a block");
    assert_eq!(nested.statements.len(), 1);
    assert!(matches!(nested.statements[0], Statement::Property(_)));
}

#[test]
fn every_node_carries_the_exact_bytes_it_came_from() {
    let src = data_doc("DATA:\n    ID: data.x\n    TYPE: STRING\n    VALUE: \"v\"\n");
    let parsed = parse(&src);
    let block = parsed.document().block("DATA").expect("DATA");

    assert_eq!(block.key.span.slice(&src), Some("DATA"));
    // The block spans from its word through the last byte of its body.
    let text = block.span.slice(&src).expect("block slices");
    assert!(text.starts_with("DATA:"));
    assert!(text.trim_end().ends_with("\"v\""));

    let id = block.field("ID").expect("ID");
    assert_eq!(id.key.span.slice(&src), Some("ID"));
    assert_eq!(id.body.span().slice(&src), Some("data.x"));

    // A child span is always inside its parent's.
    for statement in &block.body {
        assert!(statement.span().start >= block.span.start);
        assert!(statement.span().end <= block.span.end);
    }
}

#[test]
fn nested_object_properties_keep_their_own_spans_and_nesting() {
    let src = data_doc(
        "DATA:\n    ID: data.x\n    TYPE: OBJECT\n    VALUE:\n        outer:\n            inner: 2\n",
    );
    let parsed = parse(&src);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let value = parsed
        .document()
        .block("DATA")
        .and_then(|b| b.field("VALUE"))
        .expect("VALUE");
    let outer = match &value.body.as_nested().expect("nested").statements[0] {
        Statement::Property(p) => p,
        other => panic!("expected a property, got {other:?}"),
    };
    assert_eq!(outer.key.text, "outer");
    assert_eq!(outer.key.span.slice(&src), Some("outer"));
    let inner = match &outer.body.as_nested().expect("nested").statements[0] {
        Statement::Property(p) => p,
        other => panic!("expected a property, got {other:?}"),
    };
    assert_eq!(inner.key.text, "inner");
}

#[test]
fn the_first_two_blocks_are_position_pinned() {
    // SPECIFICATION before LCL: the registries pin `top_level_first` and
    // `top_level_second`, so both positions are wrong.
    let src = "SPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nLCL:\n    VERSION: \"0.1.0\"\n";
    let parsed = parse(src);
    assert!(
        id_list(&parsed).contains(&"error.block.context".to_string()),
        "{:?}",
        ids(&parsed)
    );
}

#[test]
fn a_document_without_its_lcl_header_reports_the_omission_at_end_of_file() {
    let src = "DATA:\n    ID: data.x\n    TYPE: STRING\n    VALUE: \"v\"\n";
    let parsed = parse(src);
    let list = id_list(&parsed);
    assert!(
        list.contains(&"error.block.required".to_string()),
        "{:?}",
        ids(&parsed)
    );
    // `location_rule`: an omitted required top-level block uses the
    // end-of-file offset equal to the source byte length.
    let required = parsed
        .diagnostics()
        .iter()
        .find(|d| d.id.to_string() == "error.block.required")
        .expect("required");
    assert_eq!(required.span.start, src.len());
    assert!(required.span.is_empty());
}

#[test]
fn a_blank_line_is_a_document_separator_not_a_statement() {
    // No body production admits BLANK_LINE, and 04_GRAMMAR/11 rule 12 makes
    // undefined syntax invalid rather than implementation-defined.
    let parsed = parse(&data_doc(
        "DATA:\n    ID: data.x\n\n    TYPE: STRING\n    VALUE: \"v\"\n",
    ));
    assert!(
        id_list(&parsed).contains(&"error.grammar.invalid".to_string()),
        "{:?}",
        ids(&parsed)
    );
    // The statements on both sides of it survive: recovery keeps independent
    // evidence.
    let block = parsed.document().block("DATA").expect("DATA");
    assert!(block.field("ID").is_some());
    assert!(block.field("TYPE").is_some());
}

#[test]
fn a_field_without_its_colon_is_a_grammar_defect() {
    let parsed = parse(&data_doc("DATA:\n    ID data.x\n    TYPE: STRING\n"));
    assert!(
        id_list(&parsed).contains(&"error.grammar.invalid".to_string()),
        "{:?}",
        ids(&parsed)
    );
}

#[test]
fn parsing_is_deterministic_across_repeated_runs() {
    let src = data_doc("DATA:\n    ID: data.x\n    TYPE: STRING\n    VALUE: \"v\"\n");
    let first = parse(&src);
    for _ in 0..8 {
        let again = parse(&src);
        assert_eq!(first.document(), again.document());
        assert_eq!(first.diagnostics(), again.diagnostics());
    }
}

#[test]
fn the_grammar_stage_is_not_evaluated_after_a_lexical_failure() {
    // `earliest_stage_rule`: a source that failed the lexical stage has no
    // grammar verdict at all.
    let lexed = lex("LCL:\n\tVERSION: \"0.1.0\"\n");
    assert!(lexed.primary().is_some(), "the tab must fail lexically");
    let err = lcl_parser::Parser::new(grammar())
        .parse(&lexed)
        .expect_err("a failed lexical stage must not yield a grammar verdict");
    assert_eq!(err.lexical_primary, "error.source.tab");
}
