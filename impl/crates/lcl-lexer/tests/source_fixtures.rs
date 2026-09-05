//! The concrete canonical proof: `09_CONFORMANCE/SOURCE_FIXTURES`.
//!
//! `expected_results.json` names exactly one expected result per fixture. Per
//! `primary_rule` that is the primary diagnostic, so these tests compare the
//! lexer's primary — not "some diagnostic somewhere" — against it, and the
//! valid fixtures must produce no diagnostic at all.

mod common;

use common::*;
use lcl_lexer::TokenKind;
use std::collections::BTreeMap;

fn fixtures() -> (std::path::PathBuf, BTreeMap<String, String>) {
    let dir = canonical_root().join("09_CONFORMANCE/SOURCE_FIXTURES");
    let text =
        std::fs::read_to_string(dir.join("expected_results.json")).expect("expected_results");
    let json = lcl_spec::json::parse(&text).expect("valid JSON");
    let map = json
        .as_object()
        .expect("object")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().expect("string").to_string()))
        .collect();
    (dir, map)
}

fn read(dir: &std::path::Path, name: &str) -> Vec<u8> {
    std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn fixture_inventory_matches_expected_results() {
    let (dir, expected) = fixtures();
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = expected.keys().cloned().collect();
    listed.sort();
    assert_eq!(on_disk, listed, "fixture inventory drift");
    assert_eq!(expected.len(), 15);
}

#[test]
fn every_fixture_primary_matches_expected_results() {
    let (dir, expected) = fixtures();
    let mut failures = Vec::new();
    for (name, want) in &expected {
        let lexed = lex_bytes(&read(&dir, name));
        assert_well_formed(&lexed, name);
        let got = lexed
            .primary()
            .map(|d| d.id.to_string())
            .unwrap_or_else(|| "accept".to_string());
        if &got != want {
            failures.push(format!(
                "{name}: expected {want}, got {got} ({:?})",
                ids(&lexed)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn rejected_fixtures_carry_status_invalid() {
    let (dir, expected) = fixtures();
    for (name, want) in &expected {
        let lexed = lex_bytes(&read(&dir, name));
        if want == "accept" {
            assert_eq!(lexed.terminal_status(), None, "{name}");
            assert_eq!(lexed.outcome(), lcl_lexer::Outcome::Tokenized, "{name}");
        } else {
            assert_eq!(lexed.terminal_status(), Some("status.invalid"), "{name}");
            assert_eq!(lexed.outcome(), lcl_lexer::Outcome::Rejected, "{name}");
        }
    }
}

/// The complete diagnostic list of every fixture, not only the primary. This
/// pins the recovery behaviour: a defect raises itself and any genuinely
/// independent defect, and nothing manufactured by the recovery.
#[test]
fn exact_diagnostic_lists() {
    let (dir, _) = fixtures();
    let find = |bytes: &[u8], needle: &[u8]| -> Vec<usize> {
        (0..bytes.len())
            .filter(|&i| bytes[i..].starts_with(needle))
            .collect()
    };
    let check = |name: &str, want: Vec<(&str, usize)>| {
        let bytes = read(&dir, name);
        let lexed = lex_bytes(&bytes);
        let got: Vec<(String, usize)> = ids(&lexed);
        let want: Vec<(String, usize)> =
            want.into_iter().map(|(i, o)| (i.to_string(), o)).collect();
        assert_eq!(got, want, "{name}");
        bytes
    };

    check("valid_minimum.lcl", vec![]);
    check("valid_unicode_string.lcl", vec![]);
    check("invalid_bom.lcl", vec![("error.source.bom", 0)]);
    check(
        "invalid_control.lcl",
        vec![("error.source.control_character", 4)],
    );
    check("invalid_tab.lcl", vec![("error.source.tab", 5)]);
    check(
        "invalid_indent_width.lcl",
        vec![("error.indentation.width", 5)],
    );
    check(
        "invalid_indent_jump.lcl",
        vec![("error.indentation.jump", 5)],
    );
    check(
        "invalid_trailing_space.lcl",
        vec![("error.source.trailing_space", 4)],
    );
    check("invalid_hash.lcl", vec![("error.symbol.invalid", 0)]);

    let bytes = read(&dir, "invalid_crlf.lcl");
    let crs = find(&bytes, b"\r");
    assert_eq!(crs.len(), 8);
    let want: Vec<(&str, usize)> = crs.iter().map(|&o| ("error.newline.invalid", o)).collect();
    check("invalid_crlf.lcl", want);

    let bytes = read(&dir, "invalid_no_final_lf.lcl");
    check(
        "invalid_no_final_lf.lcl",
        vec![("error.source.final_line_feed", bytes.len())],
    );

    let bytes = read(&dir, "invalid_semicolon.lcl");
    check(
        "invalid_semicolon.lcl",
        vec![("error.symbol.invalid", find(&bytes, b";")[0])],
    );

    // `'0.1.0'`: both quotes are excluded symbols, and the run between them is a
    // malformed numeric literal in its own right. No cascade beyond that.
    let bytes = read(&dir, "invalid_single_quote.lcl");
    let quotes = find(&bytes, b"'");
    check(
        "invalid_single_quote.lcl",
        vec![
            ("error.symbol.invalid", quotes[0]),
            ("error.literal.invalid", quotes[0] + 1),
            ("error.symbol.invalid", quotes[1]),
        ],
    );

    let bytes = read(&dir, "invalid_unclosed_string.lcl");
    check(
        "invalid_unclosed_string.lcl",
        vec![("error.literal.unclosed", find(&bytes, b"\"0.1.0")[0])],
    );

    let bytes = read(&dir, "invalid_typographic_quote.lcl");
    let open = find(&bytes, "\u{201C}".as_bytes())[0];
    let close = find(&bytes, "\u{201D}".as_bytes())[0];
    check(
        "invalid_typographic_quote.lcl",
        vec![
            ("error.source.non_ascii_outside_string", open),
            ("error.literal.invalid", open + 3),
            ("error.source.non_ascii_outside_string", close),
        ],
    );
}

#[test]
fn diagnostic_spans_cover_the_offending_bytes() {
    let (dir, _) = fixtures();
    let slice = |name: &str| {
        let bytes = read(&dir, name);
        let lexed = lex_bytes(&bytes);
        let d = lexed.primary().expect("primary").clone();
        (bytes[d.span.start..d.span.end].to_vec(), d)
    };
    assert_eq!(slice("invalid_bom.lcl").0, vec![0xEF, 0xBB, 0xBF]);
    assert_eq!(slice("invalid_tab.lcl").0, b"\t");
    assert_eq!(slice("invalid_crlf.lcl").0, b"\r");
    assert_eq!(slice("invalid_control.lcl").0, b"\x01");
    assert_eq!(slice("invalid_semicolon.lcl").0, b";");
    assert_eq!(slice("invalid_hash.lcl").0, b"#");
    assert_eq!(slice("invalid_single_quote.lcl").0, b"'");
    assert_eq!(slice("invalid_trailing_space.lcl").0, b" ");
    assert_eq!(
        slice("invalid_typographic_quote.lcl").0,
        "\u{201C}".as_bytes()
    );
    assert_eq!(slice("invalid_indent_width.lcl").0, b"  ");
    assert_eq!(slice("invalid_indent_jump.lcl").0, b"        ");
    assert_eq!(slice("invalid_unclosed_string.lcl").0, b"\"0.1.0");
    let (bytes, d) = slice("invalid_no_final_lf.lcl");
    assert!(bytes.is_empty());
    assert!(d.span.is_empty());
    assert_eq!(d.span.start, read(&dir, "invalid_no_final_lf.lcl").len());
}

#[test]
fn diagnostic_positions_are_derived_correctly() {
    let (dir, _) = fixtures();
    let lexed = lex_bytes(&read(&dir, "invalid_semicolon.lcl"));
    let d = lexed.primary().unwrap();
    assert_eq!((d.position.line, d.position.column), (2, 21));
    let lexed = lex_bytes(&read(&dir, "invalid_typographic_quote.lcl"));
    let d = lexed.primary().unwrap();
    assert_eq!((d.position.line, d.position.column), (2, 14));
    // Column is counted in scalars, so the closing quote is at 20 not 22.
    let d2 = &lexed.diagnostics()[2];
    assert_eq!((d2.position.line, d2.position.column), (2, 20));
}

/// `LCL_HEADER = "LCL", ":", NEWLINE, INDENT, "VERSION", ":", SPACE, STRING,
/// NEWLINE, DEDENT` — the token stream of the minimum fixture must be exactly
/// what the grammar's terminals spell.
#[test]
fn valid_minimum_token_stream_matches_the_grammar_terminals() {
    use TokenKind::*;
    let (dir, _) = fixtures();
    let lexed = lex_bytes(&read(&dir, "valid_minimum.lcl"));
    let got = shape(&lexed);
    let s = |k: TokenKind, t: &str| (k, t.to_string());
    let expected = vec![
        s(ReservedWord, "LCL"),
        s(Symbol, ":"),
        s(Newline, "\n"),
        s(Indent, ""),
        s(ReservedWord, "VERSION"),
        s(Symbol, ":"),
        s(Space, " "),
        s(String, "0.1.0"),
        s(Newline, "\n"),
        s(Dedent, ""),
        s(BlankLine, "\n"),
        s(ReservedWord, "SPECIFICATION"),
        s(Symbol, ":"),
        s(Newline, "\n"),
        s(Indent, ""),
        s(ReservedWord, "ID"),
        s(Symbol, ":"),
        s(Space, " "),
        s(QualifiedIdentifier, "conformance.minimum"),
        s(Newline, "\n"),
        s(ReservedWord, "NAME"),
        s(Symbol, ":"),
        s(Space, " "),
        s(String, "Minimum data document"),
        s(Newline, "\n"),
        s(ReservedWord, "VERSION"),
        s(Symbol, ":"),
        s(Space, " "),
        s(String, "1.0.0"),
        s(Newline, "\n"),
        s(ReservedWord, "KIND"),
        s(Symbol, ":"),
        s(Space, " "),
        s(QualifiedIdentifier, "kind.data"),
        s(Newline, "\n"),
        s(Dedent, ""),
        s(Eof, ""),
    ];
    assert_eq!(got, expected);
    // Exact spans: the first few are fixed by the bytes `LCL:\n    VERSION: "0.1.0"\n`.
    let spans: Vec<(usize, usize)> = lexed.tokens()[..10]
        .iter()
        .map(|t| (t.span.start, t.span.end))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0, 3),
            (3, 4),
            (4, 5),
            (9, 9),
            (9, 16),
            (16, 17),
            (17, 18),
            (18, 25),
            (25, 26),
            (26, 26)
        ]
    );
}

#[test]
fn unicode_string_value_is_decoded_exactly() {
    let (dir, _) = fixtures();
    let lexed = lex_bytes(&read(&dir, "valid_unicode_string.lcl"));
    let names: Vec<&str> = lexed
        .tokens_of(TokenKind::String)
        .filter_map(|t| t.value.as_deref())
        .collect();
    assert_eq!(names, vec!["0.1.0", "Minimum data document 日本", "1.0.0"]);
}

#[test]
fn lexing_is_deterministic_across_repetition_and_order() {
    let (dir, expected) = fixtures();
    let mut names: Vec<&String> = expected.keys().collect();
    let first: Vec<String> = names
        .iter()
        .map(|n| {
            let l = lex_bytes(&read(&dir, n));
            format!("{}\n{:?}", l.render_tokens(), ids(&l))
        })
        .collect();
    names.reverse();
    let mut second: Vec<String> = names
        .iter()
        .map(|n| {
            let l = lex_bytes(&read(&dir, n));
            format!("{}\n{:?}", l.render_tokens(), ids(&l))
        })
        .collect();
    second.reverse();
    assert_eq!(first, second);
}
