//! `08_EXAMPLES`: the 13 valid and 21 invalid release examples.
//!
//! Valid examples must tokenize with no lexical diagnostic. For invalid
//! examples the release pins one primary error each. Where that error is a
//! token-formation defect the lexer's primary and terminal status must equal
//! the pinned pair; where it belongs to a later stage, `earliest_stage_rule`
//! requires the lexical stage to be clean, so the lexer must raise nothing.

mod common;

use common::*;
use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_lexer::{Outcome, TokenKind};
use std::path::{Path, PathBuf};

fn lcl_files(sub: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(canonical_root().join(sub))
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lcl"))
        .collect();
    v.sort();
    v
}

fn name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().to_string()
}

fn expectation(p: &Path, key: &str) -> String {
    let text = std::fs::read_to_string(format!("{}.expected.txt", p.display())).expect("expected");
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(": ")))
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|| panic!("{key} missing in {}", p.display()))
}

#[test]
fn all_thirteen_valid_examples_tokenize_cleanly() {
    let files = lcl_files("08_EXAMPLES/VALID");
    assert_eq!(files.len(), 13);
    for p in &files {
        let lexed = lex_bytes(&std::fs::read(p).unwrap());
        assert_well_formed(&lexed, &name(p));
        assert_eq!(
            lexed.outcome(),
            Outcome::Tokenized,
            "{}: {:?}",
            name(p),
            ids(&lexed)
        );
        // Every example begins with the mandatory header.
        let s = shape(&lexed);
        assert_eq!(
            s[0],
            (TokenKind::ReservedWord, "LCL".to_string()),
            "{}",
            name(p)
        );
        assert_eq!(s[1], (TokenKind::Symbol, ":".to_string()), "{}", name(p));
        assert_eq!(s[3].0, TokenKind::Indent, "{}", name(p));
        assert_eq!(
            s[4],
            (TokenKind::ReservedWord, "VERSION".to_string()),
            "{}",
            name(p)
        );
        assert_eq!(
            s[7],
            (TokenKind::String, "0.1.0".to_string()),
            "{}",
            name(p)
        );
    }
}

#[test]
fn valid_examples_only_use_registered_words_and_adopted_symbols() {
    let lexicon = lexicon();
    for p in lcl_files("08_EXAMPLES/VALID") {
        let lexed = lex_bytes(&std::fs::read(&p).unwrap());
        for t in lexed.tokens_of(TokenKind::ReservedWord) {
            assert!(
                lexicon.is_reserved_word(lexed.lexeme(t).unwrap()),
                "{}",
                name(&p)
            );
        }
        for t in lexed.tokens_of(TokenKind::Symbol) {
            assert!(
                lexicon.is_adopted_symbol(lexed.lexeme(t).unwrap()),
                "{}",
                name(&p)
            );
        }
    }
}

#[test]
fn invalid_examples_are_consistent_with_their_pinned_primary() {
    let registry = DiagnosticRegistry::load(spec()).expect("diagnostics");
    let files = lcl_files("08_EXAMPLES/INVALID");
    assert_eq!(files.len(), 21);

    let mut lexical_checked = Vec::new();
    let mut clean_checked = Vec::new();
    for p in &files {
        let want_error = expectation(p, "EXPECTED_ERROR");
        let want_status = expectation(p, "EXPECTED_TERMINAL_STATUS");
        let stage = registry.error(&want_error).expect("registered").stage;
        let lexed = lex_bytes(&std::fs::read(p).unwrap());
        assert_well_formed(&lexed, &name(p));

        // 17_REGEX_FLAGS pins error.literal.invalid for `REGEX("[a-z]+", "mi")`:
        // a registered-constructor value-domain rule (canonical `ims` order)
        // that is lexical-stage in the registry but not token formation. Its
        // tokens are well-formed; the check belongs to the constructor layer.
        let token_formation = stage == Stage::Lexical && !name(p).starts_with("17_");

        if token_formation {
            let got = lexed.primary().map(|d| d.id.to_string());
            assert_eq!(got.as_deref(), Some(want_error.as_str()), "{}", name(p));
            assert_eq!(
                lexed.terminal_status(),
                Some(want_status.as_str()),
                "{}",
                name(p)
            );
            lexical_checked.push(name(p));
        } else {
            assert_eq!(
                lexed.outcome(),
                Outcome::Tokenized,
                "{}: a {stage} expectation requires a clean lexical stage, got {:?}",
                name(p),
                ids(&lexed)
            );
            clean_checked.push(name(p));
        }
    }
    assert_eq!(
        lexical_checked,
        vec![
            "01_WRONG_KEYWORD_CASE.invalid.lcl",
            "02_TAB_INDENTATION.invalid.lcl",
            "03_BARE_EQUALS.invalid.lcl",
            "04_UNKNOWN_MODAL.invalid.lcl",
            "09_UNBOUNDED_WHILE.invalid.lcl",
            "13_PERCENT_SYMBOL.invalid.lcl",
            "14_SINGLE_QUOTED_STRING.invalid.lcl",
        ]
    );
    assert_eq!(clean_checked.len(), 14);
}

#[test]
fn lexical_invalid_examples_exact_diagnostics() {
    let dir = canonical_root().join("08_EXAMPLES/INVALID");
    let get = |n: &str| lex_bytes(&std::fs::read(dir.join(n)).unwrap());

    let l = get("01_WRONG_KEYWORD_CASE.invalid.lcl");
    assert_eq!(ids(&l), vec![("error.keyword.case".to_string(), 0)]);
    assert!(l
        .primary()
        .unwrap()
        .detail
        .as_deref()
        .unwrap()
        .contains("`LCL`"));

    let l = get("02_TAB_INDENTATION.invalid.lcl");
    assert_eq!(ids(&l), vec![("error.source.tab".to_string(), 5)]);

    let l = get("03_BARE_EQUALS.invalid.lcl");
    assert_eq!(ids(&l), vec![("error.symbol.invalid".to_string(), 17)]);

    let l = get("04_UNKNOWN_MODAL.invalid.lcl");
    assert_eq!(id_list(&l), vec!["error.keyword.unknown"]);
    assert_eq!(l.primary().unwrap().span.slice(l.source()), Some("MUST"));

    let l = get("09_UNBOUNDED_WHILE.invalid.lcl");
    assert_eq!(id_list(&l), vec!["error.keyword.unknown"]);
    assert_eq!(l.primary().unwrap().span.slice(l.source()), Some("WHILE"));

    let l = get("13_PERCENT_SYMBOL.invalid.lcl");
    assert_eq!(id_list(&l), vec!["error.symbol.invalid"]);
    assert_eq!(l.primary().unwrap().span.slice(l.source()), Some("%"));
    // `50` before the `%` is still a well-formed INTEGER.
    assert!(l
        .tokens_of(TokenKind::IntegerLiteral)
        .any(|t| l.lexeme(t) == Some("50")));

    let l = get("14_SINGLE_QUOTED_STRING.invalid.lcl");
    assert_eq!(
        id_list(&l),
        vec![
            "error.symbol.invalid",
            "error.identifier.invalid",
            "error.symbol.invalid"
        ]
    );
    assert_eq!(l.diagnostics()[1].span.slice(l.source()), Some("Wrong"));
}
