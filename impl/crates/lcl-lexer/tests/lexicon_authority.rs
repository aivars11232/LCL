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

#[test]
fn block_names_equal_the_grammar_block_word_production() {
    let from_registry: BTreeSet<String> = lexicon().block_names().map(str::to_string).collect();
    assert_eq!(from_registry, ebnf_alternatives("BLOCK_WORD"));
    assert_eq!(from_registry.len(), 41);
}

/// Independent re-derivation of the field contexts straight from the JSON,
/// so the lexicon's reading of `field_signatures` is checked, not trusted.
fn field_kinds() -> Vec<(String, String, String)> {
    let text = std::fs::read_to_string(
        canonical_root().join("10_REGISTRIES/field_signatures_v0.1.0.json"),
    )
    .unwrap();
    let json = lcl_spec::json::parse(&text).unwrap();
    let mut out = Vec::new();
    for (block, body) in json.get("blocks").unwrap().as_object().unwrap() {
        for (field, sig) in body.get("fields").unwrap().as_object().unwrap() {
            let kind = sig.get("value_kind").unwrap().as_str().unwrap();
            out.push((block.clone(), field.clone(), kind.to_string()));
        }
    }
    out
}

#[test]
fn object_data_fields_are_exactly_the_value_or_object_expression_signatures() {
    let expected: BTreeSet<(String, String)> = field_kinds()
        .into_iter()
        .filter(|(_, _, k)| k == "value_or_object_expression")
        .map(|(b, f, _)| (b, f))
        .collect();
    let actual: BTreeSet<(String, String)> = lexicon()
        .object_data_fields()
        .map(|(b, f)| (b.to_string(), f.to_string()))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 10);
    assert!(actual.contains(&("DATA".to_string(), "VALUE".to_string())));
    assert!(actual.contains(&("EXAMPLE".to_string(), "CONTENT".to_string())));
    assert!(!actual.contains(&("COMMENT".to_string(), "CONTENT".to_string())));
    assert!(lexicon().is_object_data_field(Some("MEMORY"), "VALUE"));
    assert!(!lexicon().is_object_data_field(None, "VALUE"));
    assert!(!lexicon().is_object_data_field(Some("INPUT"), "DEFAULT"));
}

#[test]
fn type_position_fields_are_exactly_the_type_signatures() {
    let expected: BTreeSet<String> = field_kinds()
        .into_iter()
        .filter(|(_, _, k)| k == "type_expression" || k == "type_or_format_base")
        .map(|(_, f, _)| f)
        .collect();
    let actual: BTreeSet<String> = lexicon()
        .type_position_fields()
        .map(str::to_string)
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual, ["BASE".to_string(), "TYPE".to_string()].into());
}

#[test]
fn literal_and_type_words_come_from_keyword_categories() {
    let literal: BTreeSet<&str> = lexicon().literal_words().collect();
    assert_eq!(
        literal,
        ["FALSE", "MISSING", "NULL", "TRUE", "UNKNOWN"].into()
    );
    let types: BTreeSet<&str> = lexicon().type_words().collect();
    for w in ["LIST", "SET", "OBJECT", "REFERENCE", "STRING", "BOOLEAN"] {
        assert!(types.contains(w), "{w}");
    }
    assert!(!types.contains("REF"));
}

/// The words a `"["` follows in `NON_NULL_TYPE_EXPRESSION`.
fn ebnf_type_argument_words() -> BTreeSet<String> {
    let ebnf = std::fs::read_to_string(canonical_root().join("04_GRAMMAR/10_COMPLETE_EBNF.ebnf"))
        .expect("ebnf");
    let start = ebnf
        .find("\nNON_NULL_TYPE_EXPRESSION =")
        .expect("NON_NULL_TYPE_EXPRESSION not in EBNF");
    let body = &ebnf[start..];
    let body = &body[..body.find(';').expect("terminated")];
    let mut out = BTreeSet::new();
    for alternative in body.split('|') {
        let mut quoted = alternative.split('"').skip(1).step_by(2);
        if let (Some(word), Some("[")) = (quoted.next(), quoted.next()) {
            out.insert(word.to_string());
        }
    }
    out
}

/// Only `LIST`, `SET`, `OBJECT` and `REFERENCE` take a bracketed type
/// argument. The lexer names them because the package ships the grammar as
/// EBNF text rather than as registry data, so this test is what keeps the
/// named set equal to the shipped production.
#[test]
fn type_argument_words_equal_the_grammar_bracketed_type_forms() {
    let from_lexicon: BTreeSet<String> = lexicon()
        .type_argument_words()
        .map(str::to_string)
        .collect();
    let from_ebnf = ebnf_type_argument_words();
    assert_eq!(
        from_ebnf,
        ["LIST", "OBJECT", "REFERENCE", "SET"]
            .map(str::to_string)
            .into_iter()
            .collect::<BTreeSet<String>>()
    );
    assert_eq!(from_lexicon, from_ebnf);

    // A strict subset of the type-keyword category: every other type word
    // takes `INDEX_ACCESS` after `[`, not a type argument.
    let types: BTreeSet<String> = lexicon().type_words().map(str::to_string).collect();
    assert!(from_lexicon.is_subset(&types));
    assert!(from_lexicon.len() < types.len());
    for word in ["NULL", "STRING", "BOOLEAN", "ENUM", "DATE"] {
        assert!(types.contains(word), "{word}");
        assert!(!lexicon().is_type_argument_word(word), "{word}");
    }
}

#[test]
fn literal_constructor_contracts_come_from_the_registries() {
    let lexicon = lexicon();
    let names: BTreeSet<&str> = lexicon
        .literal_constructors()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["DATE", "DATETIME", "GLOB", "REGEX", "TIME", "URI"].into()
    );
    let regex = lexicon.literal_constructor("REGEX").unwrap();
    assert_eq!(regex.profile, lcl_lexer::LiteralProfile::Regex);
    assert_eq!(regex.literal_arities, [1usize, 2].into());
    for name in ["DATE", "DATETIME", "GLOB", "TIME", "URI"] {
        assert_eq!(
            lexicon.literal_constructor(name).unwrap().literal_arities,
            [1usize].into(),
            "{name}"
        );
    }
    // PATH has no closed literal profile; BYTES/PERCENTAGE/DURATION/MEASURE
    // register range and unit errors, not error.literal.invalid.
    for name in ["PATH", "BYTES", "PERCENTAGE", "DURATION", "MEASURE", "REF"] {
        assert!(lexicon.literal_constructor(name).is_none(), "{name}");
    }
    let flags = lexicon.regex_flags();
    assert_eq!(flags.allowed, vec!['i', 'm', 's']);
    assert_eq!(flags.canonical_order, vec!['i', 'm', 's']);
    assert!(!flags.duplicates_allowed);
    assert!(!flags.unknown_allowed);
}

#[test]
fn the_registry_regex_grammar_is_the_one_implemented() {
    // The validator implements these thirteen productions; pin them so a
    // grammar change in a future release fails here rather than drifting.
    let text =
        std::fs::read_to_string(canonical_root().join("10_REGISTRIES/types_v0.1.0.json")).unwrap();
    let json = lcl_spec::json::parse(&text).unwrap();
    let grammar = json
        .get("pattern_profiles")
        .and_then(|p| p.get("REGEX"))
        .and_then(|r| r.get("grammar"))
        .unwrap();
    assert_eq!(
        grammar.get("start_symbol").unwrap().as_str(),
        Some("REGEX_PATTERN")
    );
    let productions: Vec<&str> = grammar
        .get("productions")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert_eq!(productions.len(), 13);
    assert_eq!(productions[0], "REGEX_PATTERN = ALTERNATION ;");
    assert_eq!(
        productions[3],
        "PIECE = ASSERTION | ATOM , [ QUANTIFIER ] ;"
    );
    assert_eq!(
        productions[6],
        "QUANTIFIER = \"*\" | \"+\" | \"?\" | \"{\" , COUNT , \"}\" | \"{\" , COUNT , \",\" , [ COUNT ] , \"}\" ;"
    );
    let glob = json
        .get("pattern_profiles")
        .and_then(|p| p.get("GLOB"))
        .unwrap();
    let tokens: BTreeSet<String> = glob
        .get("tokens")
        .unwrap()
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(tokens, ["*", "**", "?", "[...]"].map(str::to_string).into());
}
