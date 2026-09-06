//! The grammar productions this build names are the shipped grammar's.
//!
//! `block_schemas` and `field_signatures` are machine-readable, so the parser
//! reads them as data. The EBNF is not: the package ships it as text. M1 set
//! the precedent for that gap with `TYPE_ARGUMENT_WORDS` — name the production
//! in Rust, then assert here that it still equals the shipped grammar — and
//! this file does the same for every production the parser encodes.

mod common;

use common::*;
use std::collections::BTreeSet;

/// Extract the quoted alternatives of one EBNF production.
fn ebnf_alternatives(production: &str) -> BTreeSet<String> {
    let ebnf = std::fs::read_to_string(canonical_root().join("04_GRAMMAR/10_COMPLETE_EBNF.ebnf"))
        .expect("ebnf");
    let start = ebnf
        .find(&format!("\n{production} ="))
        .unwrap_or_else(|| panic!("{production} not in EBNF"));
    let body = &ebnf[start..];
    let end = body.find(';').expect("terminated");
    let body = &body[..end];
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        out.insert(after[..close].to_string());
        rest = &after[close + 1..];
    }
    out
}

#[test]
fn scalar_types_equal_the_scalar_type_production() {
    let from_build: BTreeSet<String> = grammar().scalar_types().map(str::to_string).collect();
    assert_eq!(from_build, ebnf_alternatives("SCALAR_TYPE"));
}

#[test]
fn literal_words_are_exactly_the_word_shaped_literal_alternatives() {
    // `LITERAL` also names STRING, MULTILINE_STRING, INTEGER_LITERAL and
    // DECIMAL_LITERAL, which are token kinds rather than quoted words. The
    // quoted alternatives are precisely the word-shaped ones.
    let from_build: BTreeSet<String> = grammar().literal_words().map(str::to_string).collect();
    assert_eq!(from_build, ebnf_alternatives("LITERAL"));
}

#[test]
fn bracket_types_are_the_four_bracketed_type_forms() {
    // The bracketed alternatives are exactly the words the production writes
    // immediately before a `"["`:
    //
    //     "LIST", "[", TYPE_EXPRESSION, "]" | "SET", "[", … |
    //     "OBJECT", "[", REFERENCE_CALL, "]" | "REFERENCE", "[", …
    //
    // `OBJECT` is also a SCALAR_TYPE, so a set difference would lose it; the
    // adjacency is what actually distinguishes a type argument from an index.
    let ebnf = std::fs::read_to_string(canonical_root().join("04_GRAMMAR/10_COMPLETE_EBNF.ebnf"))
        .expect("ebnf");
    let start = ebnf
        .find("\nNON_NULL_TYPE_EXPRESSION =")
        .expect("production present");
    let body = &ebnf[start..start + ebnf[start..].find(';').expect("terminated")];

    let mut from_ebnf = BTreeSet::new();
    let mut rest = body;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let word = &after[..close];
        let tail = after[close + 1..].trim_start();
        if tail.starts_with(", \"[\"") {
            from_ebnf.insert(word.to_string());
        }
        rest = &after[close + 1..];
    }

    let from_build: BTreeSet<String> = grammar().bracket_types().map(str::to_string).collect();
    assert_eq!(from_build, from_ebnf);
    assert_eq!(from_build.len(), 4);
}

#[test]
fn callables_equal_the_callable_production_plus_ref() {
    let from_registry: BTreeSet<String> = grammar().callables().map(str::to_string).collect();
    let mut from_ebnf = ebnf_alternatives("CALLABLE");
    // `REFERENCE_CALL = "REF", "(", IDENTIFIER, ")"` — the one other word that
    // may stand immediately before an opening parenthesis.
    from_ebnf.insert("REF".to_string());
    assert_eq!(from_registry, from_ebnf);
}

#[test]
fn block_words_equal_the_registered_block_set() {
    let from_ebnf = ebnf_alternatives("BLOCK_WORD");
    let from_registry: BTreeSet<String> = grammar().block_names().map(str::to_string).collect();
    assert_eq!(from_registry, from_ebnf);
}

#[test]
fn compare_operators_are_all_recognised() {
    // Every `COMPARE_OPERATOR` alternative must parse as a comparison; this is
    // the behavioural half of the same parity claim.
    for op in ebnf_alternatives("COMPARE_OPERATOR") {
        let src = data_doc(&format!(
            "DATA:\n    ID: data.x\n    TYPE: BOOLEAN\n    VALUE: 1 {op} 2\n"
        ));
        let parsed = parse(&src);
        assert!(
            !id_list(&parsed).contains(&"error.grammar.invalid".to_string()),
            "{op} must be a recognised comparison operator: {:?}",
            ids(&parsed)
        );
    }
}

#[test]
fn additive_and_multiplicative_operators_are_all_recognised() {
    for production in ["ADD_OPERATOR", "MULTIPLY_OPERATOR"] {
        for op in ebnf_alternatives(production) {
            let src = data_doc(&format!(
                "DATA:\n    ID: data.x\n    TYPE: INTEGER\n    VALUE: 4 {op} 2\n"
            ));
            let parsed = parse(&src);
            assert!(
                !id_list(&parsed).contains(&"error.grammar.invalid".to_string()),
                "{op} must be a recognised {production}: {:?}",
                ids(&parsed)
            );
        }
    }
}
