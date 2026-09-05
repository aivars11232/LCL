//! Shared test helpers. Every test lexes against the approved package only.

#![allow(dead_code)]

use lcl_lexer::{Lexed, Lexer, Lexicon, TokenKind};
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

pub fn lex(source: &str) -> Lexed {
    Lexer::new(lexicon()).lex_str(source)
}

pub fn lex_bytes(source: &[u8]) -> Lexed {
    Lexer::new(lexicon()).lex(source)
}

/// `(identifier, span start)` of every diagnostic, in emitted order.
pub fn ids(lexed: &Lexed) -> Vec<(String, usize)> {
    lexed
        .diagnostics()
        .iter()
        .map(|d| (d.id.to_string(), d.span.start))
        .collect()
}

/// Identifiers only.
pub fn id_list(lexed: &Lexed) -> Vec<String> {
    lexed
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect()
}

/// `(kind, lexeme-or-value)` for every non-structural token.
pub fn shape(lexed: &Lexed) -> Vec<(TokenKind, String)> {
    lexed
        .tokens()
        .iter()
        .map(|t| {
            let text = match t.kind {
                TokenKind::String | TokenKind::MultilineString => {
                    t.value.clone().unwrap_or_default()
                }
                TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => String::new(),
                _ => lexed.lexeme(t).unwrap_or("<bad span>").to_string(),
            };
            (t.kind, text)
        })
        .collect()
}

/// Structural invariants every lexing result must satisfy, whatever the input.
pub fn assert_well_formed(lexed: &Lexed, label: &str) {
    let tokens = lexed.tokens();
    let len = lexed.source_len();

    if lexed.source().is_empty() && len > 0 {
        // Invalid UTF-8: no token stream, exactly one encoding diagnostic.
        assert!(tokens.is_empty(), "{label}: tokens with no text");
        let list = id_list(lexed);
        assert!(
            list == ["error.encoding.invalid"]
                || list == ["error.source.bom", "error.encoding.invalid"],
            "{label}: invalid UTF-8 must yield error.encoding.invalid (after a BOM at most), got {list:?}"
        );
        return;
    }

    assert_eq!(
        tokens.last().map(|t| t.kind),
        Some(TokenKind::Eof),
        "{label}: token stream must end with EOF"
    );
    assert_eq!(
        tokens.iter().filter(|t| t.kind == TokenKind::Eof).count(),
        1,
        "{label}: exactly one EOF"
    );

    let mut cursor = 0usize;
    for t in tokens {
        assert!(t.span.start <= t.span.end, "{label}: inverted span {:?}", t);
        assert!(t.span.end <= len, "{label}: span past end {:?}", t);
        assert!(
            t.span.start >= cursor,
            "{label}: overlapping or out-of-order token {:?} after byte {cursor}",
            t
        );
        cursor = t.span.end;
        assert!(
            lexed.lexeme(t).is_some(),
            "{label}: token span is not a character-boundary slice {:?}",
            t
        );
        if t.kind.is_zero_width() {
            assert!(
                t.span.is_empty(),
                "{label}: {:?} must be zero-width",
                t.kind
            );
        } else {
            assert!(
                !t.span.is_empty(),
                "{label}: {:?} must not be zero-width",
                t.kind
            );
        }
        assert_eq!(
            t.value.is_some(),
            matches!(t.kind, TokenKind::String | TokenKind::MultilineString),
            "{label}: only string tokens carry a decoded value {:?}",
            t
        );
        assert!(
            t.case_folds_to.is_none() || t.kind == TokenKind::SimpleIdentifier,
            "{label}: case_folds_to only on simple identifiers {:?}",
            t
        );
    }

    // A token never overlaps a diagnostic: a lexeme is one or the other.
    let mut covered = vec![false; len.saturating_add(1)];
    for d in lexed.diagnostics() {
        for slot in covered
            .iter_mut()
            .take(d.span.end.min(len))
            .skip(d.span.start)
        {
            *slot = true;
        }
    }
    for t in tokens {
        assert!(
            !covered[t.span.start..t.span.end.min(len)]
                .iter()
                .any(|c| *c),
            "{label}: token {:?} overlaps a diagnostic",
            t
        );
    }

    let indents = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Indent)
        .count();
    let dedents = tokens
        .iter()
        .filter(|t| t.kind == TokenKind::Dedent)
        .count();
    assert_eq!(indents, dedents, "{label}: INDENT/DEDENT must balance");

    for d in lexed.diagnostics() {
        assert!(
            d.span.start <= d.span.end,
            "{label}: inverted diagnostic span"
        );
        assert!(d.span.end <= len, "{label}: diagnostic span past end: {d}");
        assert_eq!(d.default_status, "status.invalid", "{label}: {d}");
        assert_eq!(d.stage(), lcl_diagnostics::Stage::Lexical);
        assert!(d.specificity_rank >= 100, "{label}: rank from registry");
    }

    // stable_order: offset ascending, specificity descending, identifier ascending.
    let keys: Vec<(usize, std::cmp::Reverse<u64>, String)> = lexed
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
}
