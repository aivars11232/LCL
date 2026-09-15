//! LCL-FEATURE-04 C2: lexing localized LCL 0.2.0 source through a [`WordMap`].
//!
//! Every map is built here from the authoritative 0.2.0 package: the letter
//! ranges from `locale_profile_schema_v0.2.0.json` and the spellings from the
//! package's fixture profiles. Equivalent documents in canonical English,
//! lv-LV, nl-NL, ru-RU and zh-CN must lex to the same token kinds and the same
//! canonical words, while every span still covers the author's own spelling.

use lcl_lexer::{Lexed, Lexer, Lexicon, Outcome, TokenKind, WordMap};
use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::json::{self, Json};
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn package_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.2.0")
        .canonicalize()
        .expect("the 0.2.0 package is present")
}

fn fixtures() -> PathBuf {
    package_root().join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
}

fn package() -> SpecPackage {
    SpecPackage::open_with_anchor(package_root(), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens")
}

fn code_point(text: &str) -> u32 {
    u32::from_str_radix(text.strip_prefix("U+").expect("U+ prefix"), 16).expect("hex")
}

fn letters(spec: &SpecPackage) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    let repertoires = spec
        .registry("locale_profile_schema")
        .and_then(|s| s.get("lexical_repertoires"))
        .and_then(Json::as_object)
        .expect("repertoires");
    for (_, entry) in repertoires {
        let Some(items) = entry.get("letters").and_then(Json::as_array) else {
            continue;
        };
        for item in items {
            let text = item.as_str().expect("range");
            match text.split_once("..") {
                Some((a, b)) => ranges.push((code_point(a), code_point(b))),
                None => ranges.push((code_point(text), code_point(text))),
            }
        }
    }
    ranges
}

fn word_map(spec: &SpecPackage, locale: &str, source: &[u8]) -> WordMap {
    let text = std::fs::read_to_string(fixtures().join("profiles").join(format!("{locale}.json")))
        .expect("profile");
    let profile = json::parse(&text).expect("profile parses");
    let spellings: BTreeMap<String, String> = profile
        .get("spellings")
        .and_then(Json::as_object)
        .expect("spellings")
        .iter()
        .map(|(s, w)| (s.clone(), w.as_str().expect("word").to_string()))
        .collect();
    let directive_len = if source.starts_with(b"@locale ") {
        source.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1)
    } else {
        0
    };
    WordMap::new(directive_len, letters(spec), spellings)
}

fn source(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join("sources").join(name)).expect("source")
}

fn kinds(lexed: &Lexed) -> Vec<TokenKind> {
    lexed.tokens().iter().map(|t| t.kind).collect()
}

fn reserved_words(lexed: &Lexed) -> Vec<String> {
    lexed
        .tokens()
        .iter()
        .filter(|t| t.kind == TokenKind::ReservedWord)
        .map(|t| t.word(lexed.source()).expect("word").to_string())
        .collect()
}

#[test]
fn equivalent_localized_documents_lex_to_the_canonical_token_sequence() {
    let spec = package();
    let lexicon = Lexicon::load(&spec).expect("the 0.2.0 lexicon loads");
    let lexer = Lexer::new(&lexicon);
    let canonical = lexer.lex(&source("canonical_en.lcl"));
    assert_eq!(
        canonical.outcome(),
        Outcome::Tokenized,
        "{:?}",
        canonical.primary()
    );
    let canonical_kinds = kinds(&canonical);
    let canonical_words = reserved_words(&canonical);
    assert!(canonical_words.len() > 10);

    for (name, locale) in [
        ("auto_lv.lcl", "lv-LV"),
        ("auto_nl.lcl", "nl-NL"),
        ("auto_ru.lcl", "ru-RU"),
        ("auto_zh.lcl", "zh-CN"),
        ("explicit_lv.lcl", "lv-LV"),
        ("explicit_nl.lcl", "nl-NL"),
        ("explicit_ru.lcl", "ru-RU"),
        ("explicit_zh.lcl", "zh-CN"),
    ] {
        let bytes = source(name);
        let map = word_map(&spec, locale, &bytes);
        let lexed = lexer.lex_localized(&bytes, &map);
        assert_eq!(
            lexed.outcome(),
            Outcome::Tokenized,
            "{name}: {:?}",
            lexed.primary()
        );
        assert_eq!(kinds(&lexed), canonical_kinds, "{name}: token kinds");
        assert_eq!(
            reserved_words(&lexed),
            canonical_words,
            "{name}: canonical words"
        );
        for token in lexed.tokens() {
            assert!(
                token.span.start >= map.directive_len(),
                "{name}: token inside the directive"
            );
            if token.kind == TokenKind::ReservedWord {
                let spelling = token.span.slice(lexed.source()).expect("span slices");
                assert_eq!(
                    map.canonical(spelling),
                    token.canonical.as_deref(),
                    "{name}: the span covers the profile spelling"
                );
            }
        }
    }
}

#[test]
fn unmapped_words_are_unknown_at_their_original_byte_offsets() {
    let spec = package();
    let lexicon = Lexicon::load(&spec).expect("the 0.2.0 lexicon loads");
    let lexer = Lexer::new(&lexicon);
    for (name, locale, offset) in [
        ("multibyte_offset_unknown_word.lcl", "zh-CN", 220usize),
        ("unknown_word_under_profile.lcl", "lv-LV", 226usize),
    ] {
        let bytes = source(name);
        let lexed = lexer.lex_localized(&bytes, &word_map(&spec, locale, &bytes));
        let primary = lexed.primary().expect("rejected");
        assert_eq!(primary.id.to_string(), "error.keyword.unknown", "{name}");
        assert_eq!(primary.span.start, offset, "{name}");
    }
}

#[test]
fn without_a_map_localized_source_keeps_core_0_1_0_rejection() {
    let spec = package();
    let lexicon = Lexicon::load(&spec).expect("the 0.2.0 lexicon loads");
    let lexer = Lexer::new(&lexicon);
    let lexed = lexer.lex(&source("auto_ru.lcl"));
    let primary = lexed.primary().expect("rejected without a profile");
    assert_eq!(
        primary.id.to_string(),
        "error.source.non_ascii_outside_string"
    );
}

#[test]
fn a_run_with_lowercase_ascii_is_not_a_candidate_word() {
    let spec = package();
    let lexicon = Lexicon::load(&spec).expect("the 0.2.0 lexicon loads");
    let lexer = Lexer::new(&lexicon);
    let ru = String::from_utf8(source("auto_ru.lcl")).expect("utf-8");
    let value_word: String = [0x0417u32, 0x0410, 0x0414, 0x0410, 0x0427, 0x0410]
        .into_iter()
        .map(|v| char::from_u32(v).expect("scalar"))
        .collect();
    let mutated = ru.replacen(": 3\n", &format!(": {value_word}x\n"), 1);
    assert_ne!(mutated, ru);
    let offset = mutated.find(&value_word).expect("inserted");
    let bytes = mutated.into_bytes();
    let lexed = lexer.lex_localized(&bytes, &word_map(&spec, "ru-RU", &bytes));
    let primary = lexed.primary().expect("rejected");
    assert_eq!(
        primary.id.to_string(),
        "error.source.non_ascii_outside_string"
    );
    assert_eq!(primary.span.start, offset);
}
