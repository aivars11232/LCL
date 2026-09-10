//! The JSON writer: escaping, exactness, determinism, and readability by the
//! one reader in this workspace that is allowed to be strict.
//!
//! `lcl_spec::json::parse` refuses trailing commas, comments, NaN, and
//! duplicate keys, because anything it rejects in the canonical package would
//! be a drift signal. Running every emitted document back through it is
//! therefore a real gate rather than a formality: if the writer can produce
//! something the trust root refuses to read, the protocol is not
//! interoperable with the one JSON dialect this project has committed to.

use lcl_protocol::json::{Node, Object};
use lcl_spec::json::{self, Json};

#[test]
fn scalars_render_as_json() {
    assert_eq!(Node::Null.compact(), "null");
    assert_eq!(Node::Bool(true).compact(), "true");
    assert_eq!(Node::Bool(false).compact(), "false");
    assert_eq!(Node::usize(0).compact(), "0");
    assert_eq!(
        Node::u64(18_446_744_073_709_551_615).compact(),
        "18446744073709551615"
    );
    assert_eq!(Node::string("plain").compact(), "\"plain\"");
}

/// An exact base-10 quantity survives the writer unchanged.
///
/// The engine's numbers are exact decimals, byte offsets and counts. A writer
/// holding them as `f64` would round `18446744073709551615` and any decimal
/// with more precision than a double carries, which would make a report of an
/// exact language quantity inexact.
#[test]
fn numbers_are_not_rounded() {
    let exact = "0.1000000000000000000000000000001";
    let rendered = Node::Number(exact.to_string()).compact();
    assert_eq!(rendered, exact);

    let big = "18446744073709551615";
    assert_eq!(Node::Number(big.to_string()).compact(), big);
}

#[test]
fn strings_escape_exactly_what_json_requires() {
    let cases = [
        ("a\"b", "\"a\\\"b\""),
        ("a\\b", "\"a\\\\b\""),
        ("a\nb", "\"a\\nb\""),
        ("a\tb", "\"a\\tb\""),
        ("a\rb", "\"a\\rb\""),
        ("a\u{08}b", "\"a\\bb\""),
        ("a\u{0c}b", "\"a\\fb\""),
        ("a\u{01}b", "\"a\\u0001b\""),
    ];
    for (input, expected) in cases {
        assert_eq!(Node::string(input).compact(), expected, "input {input:?}");
    }
}

/// A non-ASCII scalar is emitted as itself.
///
/// `02_LEXICAL/01` allows any Unicode scalar inside a STRING literal, and the
/// lexer preserves it exactly. Escaping it here would change the bytes of a
/// value the language kept, for no gain: the output is UTF-8, where the scalar
/// is already legal JSON.
#[test]
fn unicode_is_emitted_as_itself() {
    let rendered = Node::string("naïve — 日本語").compact();
    assert_eq!(rendered, "\"naïve — 日本語\"");
    let Ok(Json::String(back)) = json::parse(&rendered) else {
        panic!("the trust root reads it back as a string");
    };
    assert_eq!(back, "naïve — 日本語");
}

#[test]
fn objects_keep_insertion_order() {
    let node: Node = Object::new()
        .with("zeta", Node::usize(1))
        .with("alpha", Node::usize(2))
        .with("mid", Node::usize(3))
        .into();
    assert_eq!(node.compact(), r#"{"zeta":1,"alpha":2,"mid":3}"#);
}

/// A key set twice replaces in place.
///
/// A duplicate key is not a style question here: the strict reader refuses the
/// document outright, so a writer that could append a second member could
/// produce output no consumer in this workspace can read.
#[test]
fn a_repeated_key_replaces_rather_than_duplicating() {
    let node: Node = Object::new()
        .with("id", Node::string("first"))
        .with("other", Node::Null)
        .with("id", Node::string("second"))
        .into();
    assert_eq!(node.compact(), r#"{"id":"second","other":null}"#);
    assert!(json::parse(&node.compact()).is_ok(), "no duplicate key");
}

#[test]
fn empty_containers_stay_compact_in_both_forms() {
    let node: Node = Object::new()
        .with("array", Node::Array(Vec::new()))
        .with("object", Object::new().into())
        .into();
    assert_eq!(node.compact(), r#"{"array":[],"object":{}}"#);
    assert_eq!(node.pretty(), "{\n  \"array\": [],\n  \"object\": {}\n}\n");
}

#[test]
fn pretty_output_ends_with_one_line_feed() {
    let rendered = Node::string("x").pretty();
    assert!(rendered.ends_with('\n'));
    assert!(!rendered.ends_with("\n\n"));
}

/// Both renderings parse to the same value.
#[test]
fn compact_and_pretty_carry_the_same_document() {
    let node: Node = Object::new()
        .with("protocol", Node::string("lcl.engine/1"))
        .with(
            "span",
            Object::new()
                .with("start", Node::usize(12))
                .with("end", Node::usize(19))
                .into(),
        )
        .with(
            "ids",
            Node::array([Node::string("error.id.duplicate"), Node::Null]),
        )
        .into();
    let compact = json::parse(&node.compact()).expect("compact parses");
    let pretty = json::parse(&node.pretty()).expect("pretty parses");
    assert_eq!(compact, pretty);
}

#[test]
fn nesting_renders_and_reparses() {
    let mut node = Node::string("leaf");
    for _ in 0..24 {
        node = Node::array([node]);
    }
    let rendered = node.compact();
    assert!(json::parse(&rendered).is_ok(), "deep nesting reparses");
}

#[test]
fn absent_and_null_are_different_claims() {
    let absent: Node = Object::new().with_some("section", None).into();
    let present: Node = Object::new().with("section", Node::Null).into();
    assert_eq!(absent.compact(), "{}");
    assert_eq!(present.compact(), r#"{"section":null}"#);
}

/// Rendering is a pure function of the tree.
#[test]
fn rendering_is_deterministic() {
    let build = || -> Node {
        Object::new()
            .with("b", Node::usize(2))
            .with("a", Node::array([Node::string("x"), Node::string("y")]))
            .into()
    };
    assert_eq!(build().compact(), build().compact());
    assert_eq!(build().pretty(), build().pretty());
}
