//! Shared test helpers. Every test parses against the approved package only.

#![allow(dead_code)]

use lcl_lexer::{Lexed, Lexer, Lexicon};
use lcl_parser::{Grammar, Parsed, Parser};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

pub fn spec() -> &'static SpecPackage {
    static SPEC: OnceLock<SpecPackage> = OnceLock::new();
    SPEC.get_or_init(|| SpecPackage::open(canonical_root()).expect("approved package opens"))
}

pub fn lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| Lexicon::load(spec()).expect("lexicon loads from the approved package"))
}

pub fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| Grammar::load(spec()).expect("grammar loads from the approved package"))
}

pub fn lex(source: &str) -> Lexed {
    Lexer::new(lexicon()).lex_str(source)
}

pub fn lex_bytes(source: &[u8]) -> Lexed {
    Lexer::new(lexicon()).lex(source)
}

/// Lex and parse, asserting the lexical stage is clean first.
///
/// A test that means to exercise the grammar stage must supply lexically valid
/// source; a lexical failure here is a defect in the test, not a parser result.
pub fn parse(source: &str) -> Parsed {
    let lexed = lex(source);
    assert_eq!(
        lexed.primary().map(|d| d.id.to_string()),
        None,
        "test source must be lexically clean: {source:?}"
    );
    Parser::new(grammar())
        .parse(&lexed)
        .expect("a clean lexical stage permits parsing")
}

pub fn parse_bytes(source: &[u8]) -> Parsed {
    let lexed = lex_bytes(source);
    Parser::new(grammar())
        .parse(&lexed)
        .expect("a clean lexical stage permits parsing")
}

/// `(identifier, span start)` of every diagnostic, in emitted order.
pub fn ids(parsed: &Parsed) -> Vec<(String, usize)> {
    parsed
        .diagnostics()
        .iter()
        .map(|d| (d.id.to_string(), d.span.start))
        .collect()
}

/// Identifiers only.
pub fn id_list(parsed: &Parsed) -> Vec<String> {
    parsed
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect()
}

/// Structural invariants every parse result must satisfy, whatever the input.
pub fn assert_well_formed(parsed: &Parsed, source_len: usize, label: &str) {
    let document = parsed.document();
    assert_eq!(
        document.span.start, 0,
        "{label}: a document starts at byte 0"
    );
    assert_eq!(
        document.span.end, source_len,
        "{label}: a document ends at the source length"
    );

    // Spans are inside the source, ordered, and nested within their parents.
    for item in &document.items {
        assert_span_tree(item.span(), document.span, source_len, label);
    }

    for d in parsed.diagnostics() {
        assert!(d.span.start <= d.span.end, "{label}: inverted span: {d}");
        assert!(d.span.end <= source_len, "{label}: span past end: {d}");
        assert_eq!(d.default_status, "status.invalid", "{label}: {d}");
        assert_eq!(
            d.stage(),
            lcl_diagnostics::Stage::GrammarOrSchema,
            "{label}"
        );
        assert!(d.specificity_rank >= 100, "{label}: rank from registry");
        assert!(!d.meaning.is_empty(), "{label}: meaning from registry");
    }

    // stable_order: offset ascending, specificity descending, identifier ascending.
    let keys: Vec<(usize, std::cmp::Reverse<u64>, String)> = parsed
        .diagnostics()
        .iter()
        .map(|d| {
            (
                d.span.start,
                std::cmp::Reverse(d.specificity_rank),
                d.id.to_string(),
            )
        })
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "{label}: diagnostics must be in stable_order");

    // duplicate_key: no two survivors share identifier, locus and cause.
    let mut seen = std::collections::BTreeSet::new();
    for d in parsed.diagnostics() {
        assert!(
            seen.insert((d.id, d.span, d.cause.clone())),
            "{label}: duplicate diagnostic survived suppression: {d}"
        );
    }

    // The primary is the first in stable_order, or none.
    assert_eq!(
        parsed.primary().map(|d| (d.id, d.span)),
        parsed.diagnostics().first().map(|d| (d.id, d.span)),
        "{label}"
    );
}

fn assert_span_tree(span: lcl_lexer::Span, parent: lcl_lexer::Span, len: usize, label: &str) {
    assert!(span.start <= span.end, "{label}: inverted node span");
    assert!(span.end <= len, "{label}: node span past end");
    assert!(
        span.start >= parent.start && span.end <= parent.end,
        "{label}: node escapes its parent"
    );
}

/// The minimal legal document header for one document kind, so a test can
/// focus on a single block.
pub fn header(kind: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"Test\"\n    VERSION: \"1.0.0\"\n    KIND: {kind}\n"
    )
}

/// A `kind.data` document with `body` appended. `body` must end with a newline.
pub fn data_doc(body: &str) -> String {
    format!("{}\n{body}", header("kind.data"))
}

/// A `kind.task` document with `body` appended, closed by the EXECUTE root a
/// task document requires.
pub fn task_doc(body: &str) -> String {
    format!(
        "{}\n{body}\nEXECUTE:\n    REFERENCE: REF(task.t)\n",
        header("kind.task")
    )
}

/// A single `DATA` block carrying `value` as its VALUE, for value-shape tests.
pub fn data_value(value: &str) -> String {
    data_doc(&format!(
        "DATA:\n    ID: data.x\n    TYPE: STRING\n    VALUE: {value}\n"
    ))
}
