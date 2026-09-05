//! The vocabulary is derived from the verified release, and only from it.

mod common;

use common::*;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_lexer::{LexicalError, Lexicon, LexiconError};
use lcl_spec::SpecPackage;
use std::collections::BTreeSet;

#[test]
fn loads_from_the_approved_package_with_the_declared_cardinalities() {
    let lexicon = lexicon();
    assert_eq!(lexicon.formal_version(), "0.1.0");
    assert_eq!(lexicon.reserved_words().count(), 141);
    assert_eq!(lexicon.callables().count(), 23);
    assert_eq!(lexicon.adopted_symbols().count(), 21);
    assert_eq!(lexicon.excluded_lexemes().count(), 23);
    assert_eq!(LexicalError::ALL.len(), 23);
}

#[test]
fn refuses_an_unverified_package() {
    // `open_unverified` never establishes authority, whatever the bytes.
    let pkg = SpecPackage::open_unverified(canonical_root()).expect("loads");
    assert!(!pkg.is_authoritative());
    match Lexicon::load(&pkg) {
        Err(LexiconError::UnverifiedPackage(a)) => assert_eq!(a, lcl_spec::Authority::Unverified),
        other => panic!("expected UnverifiedPackage, got {:?}", other.map(|_| ())),
    }
}

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
fn reserved_words_equal_the_grammar_reserved_word_production() {
    let from_registry: BTreeSet<String> = lexicon().reserved_words().map(str::to_string).collect();
    let from_ebnf = ebnf_alternatives("RESERVED_WORD");
    assert_eq!(from_registry, from_ebnf);
    assert_eq!(from_ebnf.len(), 141);
}

#[test]
fn callables_equal_the_grammar_callable_production_plus_ref() {
    let from_registry: BTreeSet<String> = lexicon().callables().map(str::to_string).collect();
    let mut from_ebnf = ebnf_alternatives("CALLABLE");
    assert_eq!(from_ebnf.len(), 22);
    // REFERENCE_CALL = "REF", "(", IDENTIFIER, ")" — the one other word that
    // may stand immediately before an opening parenthesis.
    from_ebnf.insert("REF".to_string());
    assert_eq!(from_registry, from_ebnf);
}

#[test]
fn block_words_are_all_reserved_words() {
    let lexicon = lexicon();
    let block_words = ebnf_alternatives("BLOCK_WORD");
    assert_eq!(block_words.len(), 41);
    for w in &block_words {
        assert!(lexicon.is_reserved_word(w), "{w}");
    }
}

#[test]
fn lexical_error_enum_equals_the_registry_lexical_stage_set() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics");
    let registered: BTreeSet<&str> = registry
        .errors_by_stage(Stage::Lexical)
        .into_iter()
        .map(|e| e.id.as_str())
        .collect();
    let implemented: BTreeSet<&str> = LexicalError::ALL
        .iter()
        .map(|e| e.as_registry_str())
        .collect();
    assert_eq!(registered, implemented);
    for id in LexicalError::ALL {
        assert_eq!(
            LexicalError::from_registry_str(id.as_registry_str()),
            Some(id)
        );
        let def = registry.error(id.as_registry_str()).expect("registered");
        assert_eq!(def.stage, Stage::Lexical);
        assert_eq!(def.default_status, "status.invalid");
        assert!(!def.recoverable_with_declared_handler);
        assert_eq!(def.event, None);
        assert_eq!(lexicon().error(id).meaning, def.meaning);
    }
    assert_eq!(LexicalError::from_registry_str("error.block.order"), None);
}

#[test]
fn enum_order_is_registry_identifier_order() {
    let ids: Vec<&str> = LexicalError::ALL
        .iter()
        .map(|e| e.as_registry_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "stable_order tiebreak relies on this");
    let mut by_enum = LexicalError::ALL.to_vec();
    by_enum.sort();
    assert_eq!(by_enum.as_slice(), &LexicalError::ALL);
}

#[test]
fn adopted_symbols_equal_the_registry_and_the_prose() {
    let lexicon = lexicon();
    let adopted: BTreeSet<&str> = lexicon.adopted_symbols().collect();
    let expected: BTreeSet<&str> = [
        ":", "\"", "\"\"\"", "\\", "(", ")", "[", "]", ",", ".", "_", "+", "-", "*", "/", "==",
        "!=", "<", "<=", ">", ">=",
    ]
    .into_iter()
    .collect();
    // This list is the test's own reading of 02_LEXICAL/09; the lexer itself
    // never holds it.
    assert_eq!(adopted, expected);
    // Longest-first ordering is what the selection rule needs.
    let lens: Vec<usize> = lexicon.adopted_symbols().map(str::len).collect();
    assert!(lens.windows(2).all(|w| w[0] >= w[1]));
    let lens: Vec<usize> = lexicon.excluded_lexemes().map(str::len).collect();
    assert!(lens.windows(2).all(|w| w[0] >= w[1]));
}

#[test]
fn case_folding_is_total_over_the_registry() {
    let lexicon = lexicon();
    for w in lexicon.reserved_words() {
        assert_eq!(lexicon.case_folded_word(&w.to_lowercase()), Some(w));
        assert_eq!(lexicon.case_folded_word(w), Some(w));
    }
    assert_eq!(lexicon.case_folded_word("must"), None);
}
