//! `EXPRESSION` and its productions, in the precedence the EBNF declares.
//!
//! Several of these forms appear in no canonical example — multiline
//! collections and multiline strings among them — so they are exercised here
//! against the grammar directly rather than through a fixture.

mod common;

use common::*;
use lcl_parser::syntax::*;
use lcl_parser::Outcome;

/// The expression written as a `DATA` block's VALUE.
///
/// `DATA.VALUE` is `value_or_object_expression`, which imposes no grammar-stage
/// shape, so the value under test reaches the parser unfiltered.
fn value(src: &str) -> Expr {
    let text = data_value(src);
    let parsed = parse(&text);
    assert_eq!(
        parsed.outcome(),
        Outcome::Parsed,
        "{src:?} must parse: {:?}",
        ids(&parsed)
    );
    parsed
        .document()
        .block("DATA")
        .and_then(|b| b.field("VALUE"))
        .and_then(|f| f.body.as_inline())
        .and_then(|v| v.as_expression())
        .cloned()
        .unwrap_or_else(|| panic!("{src:?} produced no expression"))
}

/// The diagnostics of a document whose DATA VALUE is `src`.
fn value_errors(src: &str) -> Vec<String> {
    id_list(&parse(&data_value(src)))
}

fn binary(expr: &Expr) -> (&Expr, BinaryOp, &Expr) {
    match expr {
        Expr::Binary(b) => (&b.left, b.operator, &b.right),
        other => panic!("expected a binary expression, got {other:?}"),
    }
}

#[test]
fn literals_cover_every_alternative_of_the_literal_production() {
    for (src, kind) in [
        ("\"text\"", LiteralKind::String),
        ("42", LiteralKind::Integer),
        ("4.25", LiteralKind::Decimal),
        ("TRUE", LiteralKind::True),
        ("FALSE", LiteralKind::False),
        ("NULL", LiteralKind::Null),
        ("MISSING", LiteralKind::Missing),
        ("UNKNOWN", LiteralKind::Unknown),
    ] {
        match value(src) {
            Expr::Literal(l) => assert_eq!(l.kind, kind, "{src}"),
            other => panic!("{src}: expected a literal, got {other:?}"),
        }
    }
}

#[test]
fn a_string_literal_carries_its_decoded_value_and_its_raw_span() {
    let src = data_value("\"a\\nb\"");
    let parsed = parse(&src);
    let expr = parsed
        .document()
        .block("DATA")
        .and_then(|b| b.field("VALUE"))
        .and_then(|f| f.body.as_inline())
        .and_then(|v| v.as_expression())
        .expect("value");
    match expr {
        Expr::Literal(l) => {
            assert_eq!(l.text, "a\nb", "the decoded value resolves escapes");
            assert_eq!(
                l.span.slice(&src),
                Some("\"a\\nb\""),
                "the span still addresses the raw bytes"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_multiline_string_is_a_literal() {
    // "triple double quote on the value line, content on following lines, and
    // closing triple double quote aligned with the declaration." — 02_LEXICAL/07
    let text = data_doc(
        "DATA:\n    ID: data.x\n    TYPE: STRING\n    DESCRIPTION: \"\"\"\n        first\n        second\n    \"\"\"\n    VALUE: \"v\"\n",
    );
    let parsed = parse(&text);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let description = parsed
        .document()
        .block("DATA")
        .and_then(|b| b.field("DESCRIPTION"))
        .and_then(|f| f.body.as_inline())
        .and_then(|v| v.as_expression())
        .expect("DESCRIPTION");
    match description {
        Expr::Literal(l) => {
            assert_eq!(l.kind, LiteralKind::MultilineString);
            assert_eq!(l.text, "first\nsecond\n");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn precedence_follows_the_declared_chain() {
    // OR binds loosest, then AND, then comparison, then additive, then
    // multiplicative.
    let (left, op, _) = {
        let e = value("TRUE OR FALSE AND TRUE");
        let (l, o, r) = binary(&e);
        (l.clone(), o, r.clone())
    };
    assert_eq!(op, BinaryOp::Or);
    assert!(matches!(left, Expr::Literal(_)), "OR binds loosest");

    let e = value("1 + 2 * 3");
    let (l, o, r) = binary(&e);
    assert_eq!(o, BinaryOp::Add);
    assert!(matches!(l, Expr::Literal(_)));
    let (_, inner, _) = binary(r);
    assert_eq!(inner, BinaryOp::Multiply, "multiplication binds tighter");

    let e = value("1 + 2 == 3");
    let (l, o, _) = binary(&e);
    assert_eq!(o, BinaryOp::Equal, "comparison binds looser than additive");
    let (_, inner, _) = binary(l);
    assert_eq!(inner, BinaryOp::Add);
}

#[test]
fn additive_and_multiplicative_are_left_associative() {
    let e = value("1 - 2 - 3");
    let (left, op, right) = binary(&e);
    assert_eq!(op, BinaryOp::Subtract);
    assert!(matches!(right, Expr::Literal(_)), "the tail is the last term");
    let (_, inner, _) = binary(left);
    assert_eq!(inner, BinaryOp::Subtract, "grouping is left to right");
}

#[test]
fn comparison_admits_at_most_one_operator() {
    // `COMPARISON = ADDITIVE, [ SPACE, COMPARE_OPERATOR, SPACE, ADDITIVE ]` —
    // an optional single occurrence, not a repetition.
    assert!(
        value_errors("1 == 2 == 3").contains(&"error.grammar.invalid".to_string()),
        "a chained comparison is not a grammar form"
    );
    // One comparison is fine.
    assert!(matches!(value("1 == 2"), Expr::Binary(_)));
}

#[test]
fn every_compare_operator_alternative_parses() {
    for (src, op) in [
        ("1 == 2", BinaryOp::Equal),
        ("1 != 2", BinaryOp::NotEqual),
        ("1 < 2", BinaryOp::Less),
        ("1 <= 2", BinaryOp::LessOrEqual),
        ("1 > 2", BinaryOp::Greater),
        ("1 >= 2", BinaryOp::GreaterOrEqual),
        ("1 IN [1, 2]", BinaryOp::In),
        ("\"a\" CONTAINS \"b\"", BinaryOp::Contains),
        ("\"a\" MATCHES REGEX(\"a\")", BinaryOp::Matches),
    ] {
        let (_, found, _) = {
            let e = value(src);
            let (l, o, r) = binary(&e);
            (l.clone(), o, r.clone())
        };
        assert_eq!(found, op, "{src}");
    }
}

#[test]
fn a_binary_operator_requires_one_space_on_each_side() {
    // The EBNF spells `SPACE` around every operator, so `1+2` is not an
    // expression. The lexer accepts the bytes; the grammar rejects the shape.
    for src in ["1+2", "1 +2", "1+ 2"] {
        assert!(
            !value_errors(src).is_empty(),
            "{src} must not parse as an additive expression"
        );
    }
    assert!(value_errors("1 + 2").is_empty());
}

#[test]
fn unary_not_and_negation_nest() {
    match value("NOT TRUE") {
        Expr::Unary(u) => {
            assert_eq!(u.operator, UnaryOp::Not);
            assert!(matches!(*u.operand, Expr::Literal(_)));
        }
        other => panic!("{other:?}"),
    }
    match value("-4") {
        Expr::Unary(u) => assert_eq!(u.operator, UnaryOp::Negate),
        other => panic!("{other:?}"),
    }
    match value("NOT NOT FALSE") {
        Expr::Unary(u) => assert!(matches!(*u.operand, Expr::Unary(_)), "UNARY recurses"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn calls_are_positional_and_comma_separated_by_one_space() {
    match value("MEASURE(5, unit.second)") {
        Expr::Call(c) => {
            assert_eq!(c.callable.text, "MEASURE");
            assert_eq!(c.arguments.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    // Zero arguments is a legal shape; arity belongs to the standard library.
    assert!(matches!(value("EMPTY()"), Expr::Call(_)));
    // "Named, mixed positional and named, and variadic calls are invalid
    // syntax." — 04_GRAMMAR/03
    assert!(!value_errors("MEASURE(value: 5)").is_empty());
    // The grammar spells `",", SPACE`.
    assert!(!value_errors("MEASURE(5,unit.second)").is_empty());
}

#[test]
fn a_reference_call_carries_its_identifier() {
    match value("REF(input.value)") {
        Expr::Call(c) => {
            assert!(c.is_reference());
            assert_eq!(c.reference_target().map(|i| i.text.as_str()), Some("input.value"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn collection_literals_parse_inline_and_multiline() {
    match value("[1, 2, 3]") {
        Expr::Collection(c) => assert_eq!(c.members.len(), 3),
        other => panic!("{other:?}"),
    }
    match value("[]") {
        Expr::Collection(c) => assert!(c.members.is_empty()),
        other => panic!("{other:?}"),
    }

    // `MULTILINE_COLLECTION = "[", NEWLINE, INDENT, EXPRESSION,
    //  { ",", NEWLINE, EXPRESSION }, NEWLINE, DEDENT, "]"` — no canonical
    // example uses this form.
    let text = data_doc(
        "DATA:\n    ID: data.x\n    TYPE: LIST[INTEGER]\n    VALUE: [\n        1,\n        2\n    ]\n",
    );
    let parsed = parse(&text);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
    let value = parsed
        .document()
        .block("DATA")
        .and_then(|b| b.field("VALUE"))
        .and_then(|f| f.body.as_inline())
        .expect("VALUE");
    match value {
        Value::MultilineCollection(c) => assert_eq!(c.members.len(), 2),
        other => panic!("expected a multiline collection, got {other:?}"),
    }
}

#[test]
fn property_and_index_access_chain_on_a_postfix() {
    // A dotted identifier is one lexeme, so property access shows up after a
    // call or an index.
    match value("REF(a.b).NAME") {
        Expr::Property(p) => {
            assert_eq!(p.name, "NAME");
            assert!(p.reserved, "an uppercase property selects declaration metadata");
        }
        other => panic!("{other:?}"),
    }
    match value("REF(a.b).field") {
        Expr::Property(p) => {
            assert_eq!(p.name, "field");
            assert!(!p.reserved, "a lowercase property selects an OBJECT field");
        }
        other => panic!("{other:?}"),
    }
    match value("[10, 20][0]") {
        Expr::Index(i) => {
            assert!(matches!(*i.base, Expr::Collection(_)));
            assert!(matches!(*i.index, Expr::Literal(_)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn parentheses_are_retained_in_the_tree() {
    match value("(1 + 2) * 3") {
        Expr::Binary(b) => {
            assert_eq!(b.operator, BinaryOp::Multiply);
            assert!(
                matches!(*b.left, Expr::Group(_)),
                "the group node keeps the source shape"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn type_expressions_cover_every_alternative() {
    let type_of = |src: &str| -> Expr {
        let text = data_doc(&format!(
            "DATA:\n    ID: data.x\n    TYPE: {src}\n    VALUE: \"v\"\n"
        ));
        let parsed = parse(&text);
        assert_eq!(parsed.outcome(), Outcome::Parsed, "{src}: {:?}", ids(&parsed));
        parsed
            .document()
            .block("DATA")
            .and_then(|b| b.field("TYPE"))
            .and_then(|f| f.body.as_inline())
            .and_then(|v| v.as_expression())
            .cloned()
            .expect("TYPE")
    };

    // SCALAR_TYPE
    match type_of("INTEGER") {
        Expr::Type(TypeExpr::Scalar(w)) => assert_eq!(w.text, "INTEGER"),
        other => panic!("{other:?}"),
    }
    // The four bracketed forms.
    assert!(matches!(type_of("LIST[STRING]"), Expr::Type(TypeExpr::List(_))));
    assert!(matches!(type_of("SET[STRING]"), Expr::Type(TypeExpr::Set(_))));
    assert!(matches!(
        type_of("OBJECT[REF(type.t)]"),
        Expr::Type(TypeExpr::Object(_))
    ));
    assert!(matches!(
        type_of("REFERENCE[REF(type.t)]"),
        Expr::Type(TypeExpr::Reference(_))
    ));
    // Nesting.
    match type_of("LIST[SET[INTEGER]]") {
        Expr::Type(TypeExpr::List(b)) => {
            assert!(matches!(*b.argument, Expr::Type(TypeExpr::Set(_))));
        }
        other => panic!("{other:?}"),
    }
    // `TYPE_EXPRESSION = NON_NULL_TYPE_EXPRESSION | REFERENCE_CALL | "NULL"`.
    assert!(matches!(type_of("REF(type.t)"), Expr::Call(_)));
    match type_of("NULL") {
        Expr::Literal(l) => assert_eq!(l.kind, LiteralKind::Null),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_constructor_spelling_wins_over_the_scalar_type_spelling() {
    // PATH, REGEX, DATE and the rest are both SCALAR_TYPE words and CALLABLEs.
    // A following `(` selects the call.
    assert!(matches!(value("PATH(\"/tmp/a\")"), Expr::Call(_)));
    let text = data_doc("DATA:\n    ID: data.x\n    TYPE: PATH\n    VALUE: PATH(\"/tmp/a\")\n");
    let parsed = parse(&text);
    assert_eq!(parsed.outcome(), Outcome::Parsed, "{:?}", ids(&parsed));
}

#[test]
fn a_bare_identifier_is_never_a_type() {
    // 04_GRAMMAR/11 rule 8. `type_expression` accepts only a type expression,
    // a REFERENCE_CALL or NULL, so a bare identifier is a shape violation.
    let text = data_doc("DATA:\n    ID: data.x\n    TYPE: some_type\n    VALUE: \"v\"\n");
    let parsed = parse(&text);
    assert!(
        id_list(&parsed).contains(&"error.field.type".to_string()),
        "{:?}",
        ids(&parsed)
    );
}
