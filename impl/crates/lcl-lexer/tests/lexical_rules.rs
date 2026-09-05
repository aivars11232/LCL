//! Rule-by-rule tests against the normative text of `02_LEXICAL`.
//!
//! Each test names the rule it exercises. Inputs are minimal and use only
//! registered words as keys, so a failure points at one rule rather than at a
//! document.

mod common;

use common::*;
use lcl_lexer::TokenKind::{self, *};
use std::string::String as StdString;

fn s(k: TokenKind, t: &str) -> (TokenKind, StdString) {
    (k, t.to_string())
}

/// Tokens of a one-line source, minus the trailing NEWLINE and EOF.
fn line(src: &str) -> Vec<(TokenKind, StdString)> {
    let lexed = lex(&format!("{src}\n"));
    assert_well_formed(&lexed, src);
    let mut v = shape(&lexed);
    assert_eq!(v.pop().map(|t| t.0), Some(Eof));
    assert_eq!(v.pop().map(|t| t.0), Some(Newline));
    v
}

fn errors(src: &str) -> Vec<StdString> {
    let lexed = lex(src);
    assert_well_formed(&lexed, src);
    id_list(&lexed)
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/01 — character encoding and source text
// ---------------------------------------------------------------------------

#[test]
fn empty_source_is_just_eof() {
    let l = lex("");
    assert!(l.diagnostics().is_empty());
    assert_eq!(shape(&l), vec![s(Eof, "")]);
    assert_eq!(l.tokens()[0].span, lcl_lexer::Span::empty(0));
}

#[test]
fn single_line_feed_is_one_blank_line() {
    let l = lex("\n");
    assert!(l.diagnostics().is_empty());
    assert_eq!(shape(&l), vec![s(BlankLine, "\n"), s(Eof, "")]);
}

#[test]
fn invalid_utf8_is_the_only_result() {
    let l = lex_bytes(b"LCL:\n    VERSION: \"\xff\"\n");
    assert_eq!(id_list(&l), vec!["error.encoding.invalid"]);
    assert_eq!(l.primary().unwrap().span.start, 19);
    assert!(l.tokens().is_empty());
    assert_eq!(l.source_len(), 22);
    assert_eq!(l.terminal_status(), Some("status.invalid"));
}

#[test]
fn bom_is_reported_once_and_then_skipped() {
    let l = lex_bytes(b"\xef\xbb\xbfID: 1\n");
    assert_eq!(ids(&l), vec![("error.source.bom".to_string(), 0)]);
    // U+FEFF later in the file is an ordinary non-ASCII scalar, not a BOM.
    let l = lex("LCL:\u{FEFF}\n");
    assert_eq!(id_list(&l), vec!["error.source.non_ascii_outside_string"]);
    let l = lex("ID: \"\u{FEFF}\"\n");
    assert!(l.diagnostics().is_empty());
}

#[test]
fn prohibited_controls_are_reported_wherever_they_occur() {
    assert_eq!(
        errors("ID: 1\x01\n"),
        vec!["error.source.control_character"]
    );
    assert_eq!(
        errors("ID: 1\x7f\n"),
        vec!["error.source.control_character"]
    );
    // C1 controls are non-ASCII and controls; the control identifier wins and
    // no second non-ASCII diagnostic is raised for the same byte.
    assert_eq!(
        errors("ID: 1\u{0085}\n"),
        vec!["error.source.control_character"]
    );
    assert_eq!(
        errors("ID: 1\u{009F}\n"),
        vec!["error.source.control_character"]
    );
    // Raw source rule: still prohibited inside a string literal.
    assert_eq!(
        errors("ID: \"a\x02b\"\n"),
        vec!["error.source.control_character"]
    );
    assert_eq!(errors("ID: \"a\tb\"\n"), vec!["error.source.tab"]);
    assert_eq!(errors("ID: \"a\rb\"\n"), vec!["error.newline.invalid"]);
}

#[test]
fn carriage_return_is_newline_invalid_not_control() {
    assert_eq!(errors("ID: 1\r\n"), vec!["error.newline.invalid"]);
    assert_eq!(
        errors("ID: 1\r"),
        vec!["error.newline.invalid", "error.source.final_line_feed"]
    );
}

#[test]
fn trailing_space_reports_the_first_trailing_byte() {
    let l = lex("ID: 1   \n");
    assert_eq!(
        ids(&l),
        vec![("error.source.trailing_space".to_string(), 5)]
    );
    assert_eq!(l.primary().unwrap().span, lcl_lexer::Span::new(5, 8));
    // A line of only spaces is a trailing-space line, not a blank line.
    let l = lex("ID: 1\n  \n");
    assert_eq!(
        ids(&l),
        vec![("error.source.trailing_space".to_string(), 6)]
    );
    // Applies inside multiline string content too: it is a raw source rule.
    let l = lex("ID: \"\"\"\n    text \n\"\"\"\n");
    assert_eq!(id_list(&l), vec!["error.source.trailing_space"]);
}

#[test]
fn missing_final_line_feed_uses_the_eof_offset() {
    let l = lex("ID: 1");
    assert_eq!(
        ids(&l),
        vec![("error.source.final_line_feed".to_string(), 5)]
    );
    assert!(l.primary().unwrap().span.is_empty());
    // Still fully tokenized: the diagnostic is zero-width and withdraws nothing.
    assert_eq!(
        shape(&l),
        vec![
            s(ReservedWord, "ID"),
            s(Symbol, ":"),
            s(Space, " "),
            s(IntegerLiteral, "1"),
            s(Eof, "")
        ]
    );
}

#[test]
fn non_ascii_outside_strings_is_rejected_per_scalar() {
    let l = lex("ID: é\n");
    assert_eq!(
        ids(&l),
        vec![("error.source.non_ascii_outside_string".to_string(), 4)]
    );
    assert_eq!(l.primary().unwrap().span.len(), 2);
    let l = lex("ID: \"é日本\"\n");
    assert!(l.diagnostics().is_empty());
    assert_eq!(
        l.tokens_of(TokenKind::String)
            .next()
            .unwrap()
            .value
            .as_deref(),
        Some("é日本")
    );
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/02 — whitespace, indentation, lines
// ---------------------------------------------------------------------------

#[test]
fn indentation_levels_emit_indent_and_dedent() {
    let l = lex("TASK:\n    STEP:\n        GOAL: 1\n    DATA: 2\nTEST: 3\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    let kinds: Vec<TokenKind> = l
        .tokens()
        .iter()
        .map(|t| t.kind)
        .filter(|k| matches!(k, Indent | Dedent | Eof))
        .collect();
    assert_eq!(kinds, vec![Indent, Indent, Dedent, Dedent, Eof]);
}

#[test]
fn dedent_closes_every_deeper_block_at_once() {
    let l = lex("TASK:\n    STEP:\n        GOAL:\n            DATA: 1\nTEST: 2\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    let dedents_before_test: usize = l
        .tokens()
        .iter()
        .take_while(|t| l.lexeme(t) != Some("TEST"))
        .filter(|t| t.kind == Dedent)
        .count();
    assert_eq!(dedents_before_test, 3);
    let eof_dedents = l
        .tokens()
        .iter()
        .rev()
        .skip(1)
        .take_while(|t| t.kind == Dedent)
        .count();
    assert_eq!(eof_dedents, 0);
}

#[test]
fn remaining_levels_close_at_end_of_input() {
    let l = lex("TASK:\n    STEP:\n        GOAL: 1\n");
    assert!(l.diagnostics().is_empty());
    let tail: Vec<TokenKind> = l.tokens().iter().rev().take(3).map(|t| t.kind).collect();
    assert_eq!(tail, vec![Eof, Dedent, Dedent]);
    assert!(l
        .tokens()
        .iter()
        .rev()
        .take(3)
        .all(|t| t.span.start == l.source_len()));
}

#[test]
fn indentation_width_must_be_a_multiple_of_four() {
    assert_eq!(
        errors("TASK:\n  STEP: 1\n"),
        vec!["error.indentation.width"]
    );
    assert_eq!(
        errors("TASK:\n     STEP: 1\n"),
        vec!["error.indentation.width"]
    );
    assert_eq!(
        errors("TASK:\n   STEP: 1\n"),
        vec!["error.indentation.width"]
    );
    // Recovery adopts the licensed level, so no empty-block cascade.
    assert_eq!(
        errors("TASK:\n  STEP: 1\nGOAL: 2\n"),
        vec!["error.indentation.width"]
    );
}

#[test]
fn indentation_may_increase_by_exactly_one_level() {
    assert_eq!(
        errors("TASK:\n        STEP: 1\n"),
        vec!["error.indentation.jump"]
    );
    assert_eq!(
        errors("TASK:\n            STEP: 1\n"),
        vec!["error.indentation.jump"]
    );
    // A jump reports itself once; the following dedent is then clean.
    assert_eq!(
        errors("TASK:\n        STEP: 1\nGOAL: 2\n"),
        vec!["error.indentation.jump"]
    );
}

#[test]
fn indentation_increase_needs_a_block_opener() {
    // `ID: 1` is an inline field; nothing licenses an INDENT on the next line.
    assert_eq!(
        errors("ID: 1\n    NAME: 2\n"),
        vec!["error.indentation.invalid"]
    );
    // But an increase by more than one level is the more specific jump.
    assert_eq!(
        errors("ID: 1\n        NAME: 2\n"),
        vec!["error.indentation.jump"]
    );
    // After `[` of a MULTILINE_COLLECTION the increase is licensed.
    assert!(errors("VALUE: [\n    1,\n    2\n]\n").is_empty());
}

#[test]
fn empty_blocks_are_invalid() {
    assert_eq!(
        errors("TASK:\nSTEP: 1\n"),
        vec!["error.indentation.empty_block"]
    );
    assert_eq!(errors("TASK:\n"), vec!["error.indentation.empty_block"]);
    assert_eq!(
        errors("TASK:\n    STEP:\n    GOAL: 1\n"),
        vec!["error.indentation.empty_block"]
    );
    // Locus is the first following non-blank line at or above the parent.
    let l = lex("TASK:\n\nSTEP: 1\n");
    assert_eq!(
        ids(&l),
        vec![("error.indentation.empty_block".to_string(), 7)]
    );
    let l = lex("TASK:\n");
    assert_eq!(
        ids(&l),
        vec![("error.indentation.empty_block".to_string(), 6)]
    );
}

#[test]
fn a_colon_followed_by_a_value_is_not_a_block_opener() {
    assert!(errors("ID: 1\nNAME: 2\n").is_empty());
    // Nor is a colon whose value was a rejected lexeme.
    assert_eq!(
        errors("ID: '1'\nNAME: 2\n"),
        vec!["error.symbol.invalid", "error.symbol.invalid"]
    );
}

#[test]
fn blank_lines_emit_one_token_and_no_structure() {
    let l = lex("TASK:\n    STEP: 1\n\n\nGOAL: 2\n");
    assert!(l.diagnostics().is_empty());
    let kinds: Vec<TokenKind> = l
        .tokens()
        .iter()
        .map(|t| t.kind)
        .filter(|k| matches!(k, Indent | Dedent | BlankLine))
        .collect();
    // The dedent closes the block before the blank lines, as the grammar's
    // `{ TOP_LEVEL_BLOCK, { BLANK_LINE } }` requires, at the end of the
    // block's last line.
    assert_eq!(kinds, vec![Indent, Dedent, BlankLine, BlankLine]);
    assert_eq!(
        l.tokens_of(Dedent).next().unwrap().span,
        lcl_lexer::Span::empty(18)
    );
    let blanks: Vec<lcl_lexer::Span> = l.tokens_of(BlankLine).map(|t| t.span).collect();
    assert_eq!(
        blanks,
        vec![lcl_lexer::Span::new(18, 19), lcl_lexer::Span::new(19, 20)]
    );
}

#[test]
fn tab_indentation_reports_only_the_tab() {
    // `error.source.tab` supersedes `error.indentation.invalid`, and recovery
    // adopts the licensed level so no empty block is manufactured either.
    assert_eq!(
        errors("TASK:\n\tSTEP: 1\nGOAL: 2\n"),
        vec!["error.source.tab"]
    );
    assert_eq!(errors("TASK:\n  \tSTEP: 1\n"), vec!["error.source.tab"]);
}

#[test]
fn each_space_is_one_space_token() {
    assert_eq!(
        line("ID:  1"),
        vec![
            s(ReservedWord, "ID"),
            s(Symbol, ":"),
            s(Space, " "),
            s(Space, " "),
            s(IntegerLiteral, "1")
        ]
    );
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/02, /03, /04 — words: reserved words and identifiers
// ---------------------------------------------------------------------------

#[test]
fn every_registered_word_lexes_as_a_reserved_word() {
    let lexicon = lexicon();
    let words: Vec<&str> = lexicon.reserved_words().collect();
    assert_eq!(words.len(), 141);
    for w in words {
        let l = lex(&format!("{w}\n"));
        assert!(l.diagnostics().is_empty(), "{w}: {:?}", ids(&l));
        assert_eq!(shape(&l)[0], s(ReservedWord, w), "{w}");
    }
}

#[test]
fn unregistered_uppercase_runs_are_one_unknown_keyword() {
    assert_eq!(errors("MUST: 1\n"), vec!["error.keyword.unknown"]);
    assert_eq!(
        errors("WHILE (TRUE):\n    ID: 1\n"),
        vec!["error.keyword.unknown"]
    );
    // Not split into `INCLUDE` + `X`, and not into `IN` + `CLUDEX`.
    let l = lex("INCLUDEX\n");
    assert_eq!(ids(&l), vec![("error.keyword.unknown".to_string(), 0)]);
    assert_eq!(
        l.primary().unwrap().span.slice(l.source()),
        Some("INCLUDEX")
    );
    assert!(l.tokens_of(ReservedWord).next().is_none());
}

#[test]
fn longest_reserved_word_wins_and_words_need_a_boundary() {
    assert_eq!(line("INCLUDE"), vec![s(ReservedWord, "INCLUDE")]);
    assert_eq!(line("IN"), vec![s(ReservedWord, "IN")]);
    assert_eq!(
        line("REF(a)"),
        vec![
            s(ReservedWord, "REF"),
            s(Symbol, "("),
            s(SimpleIdentifier, "a"),
            s(Symbol, ")")
        ]
    );
    // A digit or underscore adjacent to a word is part of the same run.
    assert_eq!(errors("ID_\n"), vec!["error.keyword.unknown"]);
    assert_eq!(errors("ID2\n"), vec!["error.keyword.unknown"]);
    assert_eq!(line("ITEM_TYPE"), vec![s(ReservedWord, "ITEM_TYPE")]);
}

#[test]
fn mixed_case_words_are_keyword_case_or_identifier_invalid() {
    let l = lex("Lcl:\n    ID: 1\n");
    assert_eq!(ids(&l), vec![("error.keyword.case".to_string(), 0)]);
    assert_eq!(errors("ID: Ref(a)\n"), vec!["error.keyword.case"]);
    assert_eq!(errors("ID: tRUE\n"), vec!["error.keyword.case"]);
    assert_eq!(errors("ID: myVar\n"), vec!["error.identifier.invalid"]);
    assert_eq!(errors("ID: Wrong\n"), vec!["error.identifier.invalid"]);
    assert_eq!(errors("ID: _x\n"), vec!["error.identifier.invalid"]);
    assert_eq!(errors("ID: a.bC\n"), vec!["error.identifier.invalid"]);
    assert_eq!(errors("ID: a1B\n"), vec!["error.identifier.invalid"]);
}

#[test]
fn lowercase_keyword_spellings_stay_identifiers_where_identifiers_are_permitted() {
    // `02_LEXICAL/03`: "count, status, and true do not become COUNT, STATUS, or
    // TRUE" where an identifier is permitted.
    let l = lex("VALUE:\n    status: true\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    let idents: Vec<(&str, Option<&str>)> = l
        .tokens_of(SimpleIdentifier)
        .map(|t| (l.lexeme(t).unwrap(), t.case_folds_to.as_deref()))
        .collect();
    assert_eq!(
        idents,
        vec![("status", Some("STATUS")), ("true", Some("TRUE"))]
    );
    // A non-keyword identifier carries no fold (`EXAMPLE` is registered;
    // `SAMPLE` is not).
    let l = lex("ID: sample\n");
    assert_eq!(
        l.tokens_of(SimpleIdentifier).next().unwrap().case_folds_to,
        None
    );
    // `VALUE: true` is an identifier expression, per the same rule.
    assert!(errors("VALUE: true\n").is_empty());
    assert!(errors("ID: example.count\n").is_empty());
}

#[test]
fn keyword_case_is_decided_lexically_in_two_positions_only() {
    // 1. A key at indentation level 0 can never be object data.
    assert_eq!(errors("lcl:\n    ID: 1\n"), vec!["error.keyword.case"]);
    assert_eq!(
        errors("specification:\n    ID: 1\n"),
        vec!["error.keyword.case"]
    );
    // A lowercase key deeper than level 0 may be an object property: no
    // lexical decision, the fold is recorded for the parser.
    let l = lex("VALUE:\n    version: 1\n");
    assert!(l.diagnostics().is_empty());
    assert_eq!(
        l.tokens_of(SimpleIdentifier)
            .next()
            .unwrap()
            .case_folds_to
            .as_deref(),
        Some("VERSION")
    );
    // An unregistered lowercase key at level 0 is a legal identifier lexically.
    assert!(errors("foo:\n    ID: 1\n").is_empty());
    // A level-0 identifier not followed by `:` is not a key.
    assert!(errors("lcl\n").is_empty());

    // 2. A registered callable immediately before `(`.
    assert_eq!(errors("ID: ref(a)\n"), vec!["error.keyword.case"]);
    assert_eq!(errors("ID: count(a)\n"), vec!["error.keyword.case"]);
    assert_eq!(errors("ID: path(\"/x\")\n"), vec!["error.keyword.case"]);
    // A registered non-callable word before `(` is not a callable position.
    assert!(errors("ID: version(a)\n").is_empty());
    // With a space before `(` it is not "immediately before".
    assert!(errors("ID: ref (a)\n").is_empty());
}

#[test]
fn identifiers_are_consumed_maximally() {
    assert_eq!(line("a"), vec![s(SimpleIdentifier, "a")]);
    assert_eq!(line("a_b9"), vec![s(SimpleIdentifier, "a_b9")]);
    assert_eq!(line("a.b"), vec![s(QualifiedIdentifier, "a.b")]);
    assert_eq!(line("kind.task"), vec![s(QualifiedIdentifier, "kind.task")]);
    assert_eq!(
        line("a.b.c_d.e1"),
        vec![s(QualifiedIdentifier, "a.b.c_d.e1")]
    );
    // Trailing or doubled dots do not join the identifier.
    assert_eq!(line("a."), vec![s(SimpleIdentifier, "a"), s(Symbol, ".")]);
    assert_eq!(
        line("a..b"),
        vec![
            s(SimpleIdentifier, "a"),
            s(Symbol, "."),
            s(Symbol, "."),
            s(SimpleIdentifier, "b")
        ]
    );
    // A dot before an uppercase run is the `.` symbol, then the run.
    assert_eq!(
        line("a.ID"),
        vec![
            s(SimpleIdentifier, "a"),
            s(Symbol, "."),
            s(ReservedWord, "ID")
        ]
    );
}

#[test]
fn property_access_after_a_completed_primary_is_the_dot_symbol() {
    assert_eq!(
        line("REF(a).b"),
        vec![
            s(ReservedWord, "REF"),
            s(Symbol, "("),
            s(SimpleIdentifier, "a"),
            s(Symbol, ")"),
            s(Symbol, "."),
            s(SimpleIdentifier, "b"),
        ]
    );
    assert_eq!(
        line("REF(output.test).exit_code == 0"),
        vec![
            s(ReservedWord, "REF"),
            s(Symbol, "("),
            s(QualifiedIdentifier, "output.test"),
            s(Symbol, ")"),
            s(Symbol, "."),
            s(SimpleIdentifier, "exit_code"),
            s(Space, " "),
            s(Symbol, "=="),
            s(Space, " "),
            s(IntegerLiteral, "0"),
        ]
    );
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/07 — numeric literals
// ---------------------------------------------------------------------------

#[test]
fn integer_and_decimal_literals() {
    assert_eq!(line("0"), vec![s(IntegerLiteral, "0")]);
    assert_eq!(line("42"), vec![s(IntegerLiteral, "42")]);
    assert_eq!(line("1.5"), vec![s(DecimalLiteral, "1.5")]);
    assert_eq!(line("0.25"), vec![s(DecimalLiteral, "0.25")]);
    assert_eq!(line("10.0"), vec![s(DecimalLiteral, "10.0")]);
}

#[test]
fn malformed_numeric_literals() {
    for bad in [
        "007", "01", "1.2.3", "0.1.0", "1e5", "1_000", "123abc", "0x1F", "1.5e3",
    ] {
        let l = lex(&format!("ID: {bad}\n"));
        assert_eq!(id_list(&l), vec!["error.literal.invalid"], "{bad}");
        assert_eq!(
            l.primary().unwrap().span.slice(l.source()),
            Some(bad),
            "{bad}"
        );
    }
}

#[test]
fn signs_are_separate_operator_tokens() {
    assert_eq!(line("-3"), vec![s(Symbol, "-"), s(IntegerLiteral, "3")]);
    assert_eq!(
        line("- -3"),
        vec![
            s(Symbol, "-"),
            s(Space, " "),
            s(Symbol, "-"),
            s(IntegerLiteral, "3")
        ]
    );
    // A leading plus is lexically the adopted `+`; its invalidity as a sign is
    // a grammar fact, not a token-formation one.
    assert_eq!(line("+3"), vec![s(Symbol, "+"), s(IntegerLiteral, "3")]);
}

#[test]
fn a_dot_after_a_number_only_joins_when_a_digit_follows() {
    assert_eq!(
        line("1.foo"),
        vec![
            s(IntegerLiteral, "1"),
            s(Symbol, "."),
            s(SimpleIdentifier, "foo")
        ]
    );
    assert_eq!(line("1."), vec![s(IntegerLiteral, "1"), s(Symbol, ".")]);
    assert_eq!(line(".5"), vec![s(Symbol, "."), s(IntegerLiteral, "5")]);
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/07 — strings and escapes
// ---------------------------------------------------------------------------

#[test]
fn single_line_strings_decode_exactly() {
    assert_eq!(line("\"\""), vec![s(String, "")]);
    assert_eq!(line("\"a b\""), vec![s(String, "a b")]);
    assert_eq!(line("\"/src/main.py\""), vec![s(String, "/src/main.py")]);
    // Text resembling LCL inside a string is data.
    assert_eq!(
        line("\"LCL: ; # ' = <tag>\""),
        vec![s(String, "LCL: ; # ' = <tag>")]
    );
    assert_eq!(
        line("\"a\" \"b\""),
        vec![s(String, "a"), s(Space, " "), s(String, "b")]
    );
}

#[test]
fn all_six_escapes_decode() {
    assert_eq!(line(r#""\"""#), vec![s(String, "\"")]);
    assert_eq!(line(r#""\\""#), vec![s(String, "\\")]);
    assert_eq!(line(r#""\n""#), vec![s(String, "\n")]);
    assert_eq!(line(r#""\r""#), vec![s(String, "\r")]);
    assert_eq!(line(r#""\t""#), vec![s(String, "\t")]);
    assert_eq!(line(r#""\u0041""#), vec![s(String, "A")]);
    assert_eq!(line(r#""\u00e9\u00E9""#), vec![s(String, "\u{e9}\u{e9}")]);
    assert_eq!(line(r#""\u0000""#), vec![s(String, "\u{0}")]);
    // Decoded once, left to right: a decoded backslash starts no second pass.
    assert_eq!(line(r#""\\n""#), vec![s(String, "\\n")]);
    assert_eq!(line(r#""\\u0041""#), vec![s(String, "\\u0041")]);
}

#[test]
fn surrogate_pairs_follow_the_normative_formula() {
    assert_eq!(line(r#""\uD83D\uDE00""#), vec![s(String, "\u{1F600}")]);
    assert_eq!(line(r#""\ud800\udc00""#), vec![s(String, "\u{10000}")]);
    assert_eq!(line(r#""\uDBFF\uDFFF""#), vec![s(String, "\u{10FFFF}")]);
}

#[test]
fn malformed_escapes() {
    for bad in [
        r#""\x""#,
        r#""\a""#,
        r#""\u""#,
        r#""\u12""#,
        r#""\u12G4""#,
        r#""\uD83D""#,
        r#""\uD83Dx""#,
        r#""\uD83DA""#,
        r#""\uDE00""#,
        r#""\U0041""#,
        r#""\ ""#,
    ] {
        let l = lex(&format!("ID: {bad}\n"));
        assert_eq!(
            id_list(&l),
            vec!["error.literal.escape"],
            "{bad}: {:?}",
            ids(&l)
        );
        // The locus is the backslash that begins the malformed escape.
        assert_eq!(l.primary().unwrap().span.start, 5, "{bad}");
        assert!(
            l.tokens_of(String).next().is_none(),
            "{bad}: no token for a rejected lexeme"
        );
    }
    // A backslash at the very end of the line is a bad escape and the string
    // is also unclosed: two causes, two diagnostics.
    assert_eq!(
        errors("ID: \"abc\\\n"),
        vec!["error.literal.unclosed", "error.literal.escape"]
    );
}

#[test]
fn unclosed_strings_end_at_the_line_feed() {
    let l = lex("ID: \"abc\nNAME: 1\n");
    assert_eq!(ids(&l), vec![("error.literal.unclosed".to_string(), 4)]);
    // The next line still lexes in NORMAL mode.
    assert!(l
        .tokens_of(ReservedWord)
        .any(|t| l.lexeme(t) == Some("NAME")));
    let l = lex("ID: \"abc");
    assert_eq!(
        id_list(&l),
        vec!["error.literal.unclosed", "error.source.final_line_feed"]
    );
}

#[test]
fn multiline_strings_strip_the_prefix_and_keep_the_rest() {
    let src = "    ID: \"\"\"\n        first\n          indented\n\n        last\n    \"\"\"\n";
    let l = lex(&format!("TASK:\n{src}"));
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    let t = l.tokens_of(MultilineString).next().unwrap();
    assert_eq!(t.value.as_deref(), Some("first\n  indented\n\nlast\n"));
    assert_eq!(
        l.lexeme(t),
        Some(
            src.trim_start_matches(' ')
                .trim_start_matches("ID: ")
                .trim_end_matches('\n')
        )
    );
    // Content lines emit no INDENT, DEDENT or BLANK_LINE.
    let structural: Vec<TokenKind> = l
        .tokens()
        .iter()
        .map(|t| t.kind)
        .filter(|k| matches!(k, Indent | Dedent | BlankLine))
        .collect();
    assert_eq!(structural, vec![Indent, Dedent]);
}

#[test]
fn multiline_string_at_level_zero_and_empty_content() {
    let l = lex("ID: \"\"\"\n\"\"\"\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    assert_eq!(
        l.tokens_of(MultilineString)
            .next()
            .unwrap()
            .value
            .as_deref(),
        Some("")
    );
    let l = lex("ID: \"\"\"\n    a\\tb\\\"\n\"\"\"\n");
    assert_eq!(
        l.tokens_of(MultilineString)
            .next()
            .unwrap()
            .value
            .as_deref(),
        Some("a\tb\"\n")
    );
}

#[test]
fn multiline_string_defects() {
    // Opening delimiter must be followed immediately by LINE FEED.
    assert_eq!(
        errors("ID: \"\"\" a\n\"\"\"\n"),
        vec!["error.literal.invalid", "error.literal.unclosed"]
    );
    // Missing content prefix.
    assert_eq!(
        errors("ID: \"\"\"\n  a\n\"\"\"\n"),
        vec!["error.literal.invalid"]
    );
    assert_eq!(
        errors("ID: \"\"\"\na\n\"\"\"\n"),
        vec!["error.literal.invalid"]
    );
    // Misaligned closing delimiter ends the literal, with one diagnostic.
    assert_eq!(
        errors("ID: \"\"\"\n    a\n    \"\"\"\n"),
        vec!["error.literal.invalid"]
    );
    // A triple quote inside content is a misplaced delimiter, never content;
    // the real closer that follows still closes the literal.
    assert_eq!(
        errors("ID: \"\"\"\n    a \"\"\" b\n\"\"\"\n"),
        vec!["error.literal.invalid"]
    );
    // But an escaped quote before two quotes is content.
    let l = lex("ID: \"\"\"\n    a \\\"\"\" b\n\"\"\"\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    assert_eq!(
        l.tokens_of(MultilineString)
            .next()
            .unwrap()
            .value
            .as_deref(),
        Some("a \"\"\" b\n")
    );
    // Unclosed at end of input.
    assert_eq!(
        errors("ID: \"\"\"\n    a\n"),
        vec!["error.literal.unclosed"]
    );
    let l = lex("ID: \"\"\"\n    a\n");
    assert_eq!(l.primary().unwrap().span.start, 4);
    // A rejected literal yields no token.
    assert!(l.tokens_of(MultilineString).next().is_none());
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/09, /10 — adopted and excluded symbols
// ---------------------------------------------------------------------------

#[test]
fn every_adopted_symbol_lexes_in_normal_mode_except_string_delimiters_and_backslash() {
    for sym in lexicon().adopted_symbols() {
        match sym {
            "\"" | "\"\"\"" => continue,
            "\\" => {
                assert_eq!(errors("ID: \\\n"), vec!["error.lexical.malformed_token"]);
                continue;
            }
            "_" => {
                // `_` is adopted as an identifier-segment separator; it never
                // starts a token on its own.
                assert_eq!(errors("ID: _\n"), vec!["error.identifier.invalid"]);
                continue;
            }
            _ => {}
        }
        let l = lex(&format!("ID: {sym}\n"));
        let syms: Vec<&str> = l.tokens_of(Symbol).filter_map(|t| l.lexeme(t)).collect();
        // An unpaired delimiter is the offending lexeme of its own diagnostic
        // and is withdrawn; every other symbol stands.
        let (expected_syms, expected_errors): (Vec<&str>, Vec<&str>) = match sym {
            // `ID: :` ends in a block-opening colon with no child block.
            ":" => (vec![":", ":"], vec!["error.indentation.empty_block"]),
            "(" => (vec![":"], vec!["error.delimiter.unclosed"]),
            // A trailing `[` opens a MULTILINE_COLLECTION, which then has no
            // member line, on top of never being closed.
            "[" => (
                vec![":"],
                vec!["error.delimiter.unclosed", "error.indentation.empty_block"],
            ),
            ")" | "]" => (vec![":"], vec!["error.delimiter.mismatch"]),
            _ => (vec![":", sym], vec![]),
        };
        assert_eq!(syms, expected_syms, "{sym}");
        assert_eq!(id_list(&l), expected_errors, "{sym}");
    }
}

#[test]
fn every_excluded_lexeme_is_symbol_invalid_with_its_exact_span() {
    let lexemes: Vec<&str> = lexicon().excluded_lexemes().collect();
    assert_eq!(lexemes.len(), 23);
    for lexeme in lexemes {
        let l = lex(&format!("ID: {lexeme}\n"));
        let d = l
            .primary()
            .unwrap_or_else(|| panic!("{lexeme}: no diagnostic"));
        assert_eq!(d.id.to_string(), "error.symbol.invalid", "{lexeme}");
        assert_eq!(d.span.slice(l.source()), Some(lexeme), "{lexeme}");
        assert_eq!(l.diagnostics().len(), 1, "{lexeme}: {:?}", ids(&l));
    }
}

#[test]
fn longest_lexeme_wins_across_adopted_and_excluded() {
    assert_eq!(
        line("a == b"),
        vec![
            s(SimpleIdentifier, "a"),
            s(Space, " "),
            s(Symbol, "=="),
            s(Space, " "),
            s(SimpleIdentifier, "b")
        ]
    );
    assert_eq!(line("a != b")[2], s(Symbol, "!="));
    assert_eq!(line("a <= b")[2], s(Symbol, "<="));
    assert_eq!(line("a >= b")[2], s(Symbol, ">="));
    assert_eq!(line("a < b")[2], s(Symbol, "<"));
    assert_eq!(line("a > b")[2], s(Symbol, ">"));
    assert_eq!(errors("a = b\n"), vec!["error.symbol.invalid"]);
    assert_eq!(errors("a ! b\n"), vec!["error.symbol.invalid"]);
    // Excluded multi-character forms are one invalid form, not split.
    for form in ["->", "...", "//", "/*", "*/", "&&", "||", "```"] {
        let l = lex(&format!("a {form} b\n"));
        assert_eq!(
            ids(&l),
            vec![("error.symbol.invalid".to_string(), 2)],
            "{form}"
        );
        assert_eq!(
            l.primary().unwrap().span.slice(l.source()),
            Some(form),
            "{form}"
        );
    }
    // `a === b` is `==` then an excluded `=`.
    let l = lex("a === b\n");
    assert_eq!(ids(&l), vec![("error.symbol.invalid".to_string(), 4)]);
}

#[test]
fn xml_tag_notation_is_the_excluded_pattern() {
    for tag in ["<tag>", "</tag>", "<a.b:c-d_e>", "<br/>", "<X>"] {
        let l = lex(&format!("ID: {tag}\n"));
        assert_eq!(
            ids(&l),
            vec![("error.symbol.invalid".to_string(), 4)],
            "{tag}"
        );
        assert_eq!(
            l.primary().unwrap().span.slice(l.source()),
            Some(tag),
            "{tag}"
        );
        assert_eq!(
            l.primary().unwrap().cause.as_str(),
            "excluded_notation_pattern"
        );
    }
    // A comparison is never a tag.
    assert!(errors("ID: a < b\n").is_empty());
    assert!(errors("ID: a <= b\n").is_empty());
    // `<` directly before a letter without a closing `>` is `<` then a word.
    assert_eq!(line("<b"), vec![s(Symbol, "<"), s(SimpleIdentifier, "b")]);
}

#[test]
fn punctuation_in_neither_inventory_is_unknown_symbol() {
    let l = lex("ID: @\n");
    assert_eq!(
        ids(&l),
        vec![("error.lexical.unknown_symbol".to_string(), 4)]
    );
    // `@` is the only ASCII punctuation in neither registry list, which is a
    // fact about the release worth pinning.
    let lexicon = lexicon();
    let uncovered: Vec<char> = (0x21u8..=0x7Eu8)
        .map(char::from)
        .filter(|c| c.is_ascii_punctuation())
        .filter(|c| {
            let s = c.to_string();
            !lexicon.adopted_symbols().any(|a| a == s)
                && !lexicon.excluded_lexemes().any(|e| e == s)
        })
        .collect();
    assert_eq!(uncovered, vec!['@']);
}

#[test]
fn delimiter_pairing() {
    assert_eq!(errors("ID: (1]\n"), vec!["error.delimiter.mismatch"]);
    assert_eq!(errors("ID: [1)\n"), vec!["error.delimiter.mismatch"]);
    assert_eq!(errors("ID: 1)\n"), vec!["error.delimiter.mismatch"]);
    let l = lex("ID: (1\n");
    assert_eq!(ids(&l), vec![("error.delimiter.unclosed".to_string(), 4)]);
    let l = lex("ID: [(1\n");
    assert_eq!(
        ids(&l),
        vec![
            ("error.delimiter.unclosed".to_string(), 4),
            ("error.delimiter.unclosed".to_string(), 5)
        ]
    );
    assert!(errors("ID: PATH(REF(a), \"b\")[0]\n").is_empty());
    // Pairing spans lines, as a MULTILINE_COLLECTION requires.
    assert!(errors("VALUE: [\n    1,\n    2\n]\n").is_empty());
    // Delimiters inside strings are data.
    assert!(errors("ID: \"(\"\n").is_empty());
}

// ---------------------------------------------------------------------------
// 02_LEXICAL/08, /12 — data forms and comment boundaries
// ---------------------------------------------------------------------------

#[test]
fn inline_collections_and_calls() {
    assert_eq!(
        line("[1, 2, 3]"),
        vec![
            s(Symbol, "["),
            s(IntegerLiteral, "1"),
            s(Symbol, ","),
            s(Space, " "),
            s(IntegerLiteral, "2"),
            s(Symbol, ","),
            s(Space, " "),
            s(IntegerLiteral, "3"),
            s(Symbol, "]"),
        ]
    );
    assert_eq!(
        line("LIST[STRING]"),
        vec![
            s(ReservedWord, "LIST"),
            s(Symbol, "["),
            s(ReservedWord, "STRING"),
            s(Symbol, "]")
        ]
    );
    assert_eq!(
        line("MEASURE(5, unit.second)"),
        vec![
            s(ReservedWord, "MEASURE"),
            s(Symbol, "("),
            s(IntegerLiteral, "5"),
            s(Symbol, ","),
            s(Space, " "),
            s(QualifiedIdentifier, "unit.second"),
            s(Symbol, ")"),
        ]
    );
}

#[test]
fn there_is_no_comment_syntax() {
    assert_eq!(errors("# c\n"), vec!["error.symbol.invalid"]);
    assert_eq!(errors("ID: 1 // c\n"), vec!["error.symbol.invalid"]);
    assert_eq!(
        errors("/* c */\n"),
        vec!["error.symbol.invalid", "error.symbol.invalid"]
    );
    // COMMENT is an ordinary block.
    assert!(errors("COMMENT:\n    CONTENT: \"c\"\n").is_empty());
}

#[test]
fn object_keys_are_lowercase_identifiers() {
    let l = lex("VALUE:\n    spelling: \"British English\"\n    maximum_characters: 300\n");
    assert!(l.diagnostics().is_empty(), "{:?}", ids(&l));
    let keys: Vec<&str> = l
        .tokens_of(SimpleIdentifier)
        .filter_map(|t| l.lexeme(t))
        .collect();
    assert_eq!(keys, vec!["spelling", "maximum_characters"]);
}

// ---------------------------------------------------------------------------
// diagnostic_selection — ordering and registry facts
// ---------------------------------------------------------------------------

#[test]
fn diagnostics_follow_stable_order_and_carry_registry_facts() {
    let l = lex("ID: @ ; 007 é\n");
    assert_eq!(
        id_list(&l),
        vec![
            "error.lexical.unknown_symbol",
            "error.symbol.invalid",
            "error.literal.invalid",
            "error.source.non_ascii_outside_string",
        ]
    );
    for d in l.diagnostics() {
        let reg = lexicon().error(d.id);
        assert_eq!(d.meaning, reg.meaning);
        assert_eq!(d.default_status, "status.invalid");
        assert_eq!(d.specificity_rank, reg.specificity_rank);
    }
}

#[test]
fn specificity_ranks_and_supersession_come_from_the_registry() {
    use lcl_lexer::LexicalError::*;
    let lexicon = lexicon();
    for id in [
        IndentationJump,
        IndentationWidth,
        LiteralEscape,
        LiteralUnclosed,
        SourceTab,
    ] {
        assert_eq!(lexicon.error(id).specificity_rank, 200, "{id}");
    }
    for id in [
        SymbolInvalid,
        SourceBom,
        KeywordCase,
        IndentationInvalid,
        LiteralInvalid,
    ] {
        assert_eq!(lexicon.error(id).specificity_rank, 100, "{id}");
    }
    assert_eq!(
        lexicon.error(IndentationJump).supersedes,
        [IndentationInvalid].into()
    );
    assert_eq!(
        lexicon.error(IndentationWidth).supersedes,
        [IndentationInvalid].into()
    );
    assert_eq!(
        lexicon.error(SourceTab).supersedes,
        [IndentationInvalid].into()
    );
    assert_eq!(
        lexicon.error(LiteralEscape).supersedes,
        [LiteralInvalid].into()
    );
    assert_eq!(
        lexicon.error(LiteralUnclosed).supersedes,
        [LiteralInvalid].into()
    );
    assert!(lexicon.error(SymbolInvalid).supersedes.is_empty());
}

#[test]
fn unclosed_string_precedes_its_trailing_bad_escape_by_offset() {
    let l = lex("ID: \"\\\n");
    assert_eq!(
        ids(&l),
        vec![
            ("error.literal.unclosed".to_string(), 4),
            ("error.literal.escape".to_string(), 5)
        ]
    );
}
