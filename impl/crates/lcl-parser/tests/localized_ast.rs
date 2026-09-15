//! LCL-FEATURE-04 C3: localized LCL 0.2.0 source builds the canonical AST.
//!
//! Equivalent documents in canonical English, lv-LV, nl-NL, ru-RU and zh-CN are
//! lexed through their fixture profiles and parsed by the one parser. With every
//! span removed the documents are identical; diagnostics keep the canonical
//! identifier and point at the author's own bytes.

use lcl_lexer::{Lexed, Lexer, Lexicon, WordMap};
use lcl_parser::{Grammar, Parser};
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

fn code_point(text: &str) -> u32 {
    u32::from_str_radix(text.strip_prefix("U+").expect("U+ prefix"), 16).expect("hex")
}

fn letters(spec: &SpecPackage) -> Vec<(u32, u32)> {
    let mut ranges = Vec::new();
    for (_, entry) in spec
        .registry("locale_profile_schema")
        .and_then(|s| s.get("lexical_repertoires"))
        .and_then(Json::as_object)
        .expect("repertoires")
    {
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

fn profile(locale: &str) -> Json {
    let text = std::fs::read_to_string(fixtures().join("profiles").join(format!("{locale}.json")))
        .expect("profile");
    json::parse(&text).expect("profile parses")
}

fn word_map(spec: &SpecPackage, locale: &str, source: &[u8]) -> WordMap {
    let spellings: BTreeMap<String, String> = profile(locale)
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

/// `Debug` rendering with every `Span { start: _, end: _ }` removed.
fn without_spans(rendered: &str) -> String {
    const OPEN: &str = "Span { start: ";
    let mut out = String::new();
    let mut rest = rendered;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        out.push_str("Span");
        let after = &rest[at..];
        let close = after.find('}').expect("closed span");
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

struct Tools {
    spec: SpecPackage,
    lexicon: Lexicon,
    grammar: Grammar,
}

fn tools() -> Tools {
    let spec = SpecPackage::open_with_anchor(package_root(), &APPROVED_PACKAGE_0_2_0)
        .expect("the approved 0.2.0 package opens");
    let lexicon = Lexicon::load(&spec).expect("the 0.2.0 lexicon loads");
    let grammar = Grammar::load(&spec).expect("the 0.2.0 grammar loads");
    Tools {
        spec,
        lexicon,
        grammar,
    }
}

fn lex(tools: &Tools, locale: Option<&str>, source: &[u8]) -> Lexed {
    let lexer = Lexer::new(&tools.lexicon);
    match locale {
        None => lexer.lex(source),
        Some(locale) => lexer.lex_localized(source, &word_map(&tools.spec, locale, source)),
    }
}

const LOCALIZED: [(&str, &str); 8] = [
    ("auto_lv.lcl", "lv-LV"),
    ("auto_nl.lcl", "nl-NL"),
    ("auto_ru.lcl", "ru-RU"),
    ("auto_zh.lcl", "zh-CN"),
    ("explicit_lv.lcl", "lv-LV"),
    ("explicit_nl.lcl", "nl-NL"),
    ("explicit_ru.lcl", "ru-RU"),
    ("explicit_zh.lcl", "zh-CN"),
];

#[test]
fn equivalent_localized_documents_parse_to_the_canonical_document() {
    let tools = tools();
    let parser = Parser::new(&tools.grammar);
    let canonical_source =
        std::fs::read(fixtures().join("sources/canonical_en.lcl")).expect("read");
    let canonical = parser
        .parse(&lex(&tools, None, &canonical_source))
        .expect("canonical lexes");
    assert!(canonical.primary().is_none(), "{:?}", canonical.primary());
    let canonical_tree = without_spans(&format!("{:?}", canonical.document()));
    assert!(canonical_tree.contains("\"SPECIFICATION\""));

    for (name, locale) in LOCALIZED {
        let source = std::fs::read(fixtures().join("sources").join(name)).expect("read");
        let parsed = parser
            .parse(&lex(&tools, Some(locale), &source))
            .unwrap_or_else(|skipped| panic!("{name} lexes: {skipped:?}"));
        assert!(parsed.primary().is_none(), "{name}: {:?}", parsed.primary());
        assert_eq!(
            without_spans(&format!("{:?}", parsed.document())),
            canonical_tree,
            "{name}: canonical AST"
        );
    }
}

#[test]
fn localized_grammar_diagnostics_keep_their_identifier_and_original_bytes() {
    let tools = tools();
    let parser = Parser::new(&tools.grammar);
    let duplicate = |text: &str, type_word: &str, integer_word: &str| {
        let line = format!("    {type_word}: {integer_word}\n");
        assert_eq!(text.matches(&line).count(), 1, "one TYPE line");
        text.replacen(&line, &format!("{line}{line}"), 1)
    };

    let canonical_text =
        std::fs::read_to_string(fixtures().join("sources/canonical_en.lcl")).expect("read");
    let canonical_source = duplicate(&canonical_text, "TYPE", "INTEGER");
    let canonical = parser
        .parse(&lex(&tools, None, canonical_source.as_bytes()))
        .expect("canonical lexes");
    let canonical_primary = canonical.primary().expect("the duplicate is rejected");
    let canonical_id = canonical_primary.id.to_string();

    for (name, locale) in LOCALIZED {
        let preferred = profile(locale);
        let spelling = |word: &str| {
            preferred
                .get("preferred")
                .and_then(|p| p.get(word))
                .and_then(Json::as_str)
                .expect("preferred spelling")
                .to_string()
        };
        let (type_word, integer_word) = (spelling("TYPE"), spelling("INTEGER"));
        let text = std::fs::read_to_string(fixtures().join("sources").join(name)).expect("read");
        let source = duplicate(&text, &type_word, &integer_word);
        let parsed = parser
            .parse(&lex(&tools, Some(locale), source.as_bytes()))
            .unwrap_or_else(|skipped| panic!("{name} lexes: {skipped:?}"));
        let primary = parsed.primary().expect("the duplicate is rejected");
        assert_eq!(primary.id.to_string(), canonical_id, "{name}: identifier");
        let line = format!("    {type_word}: {integer_word}\n");
        let second = source
            .match_indices(&line)
            .nth(1)
            .expect("second TYPE line")
            .0
            + 4;
        assert_eq!(primary.span.start, second, "{name}: original byte offset");
        assert_eq!(
            primary.span.slice(&source),
            Some(type_word.as_str()),
            "{name}: the span covers the author's spelling"
        );
    }
}
