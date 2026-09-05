//! Closed literal profiles on literal constructor arguments.
//!
//! `03_TYPES_AND_VALUES/04` and `/07`, `types_v0.1.0.json#/pattern_profiles`
//! and `#/temporal_literal_contract`, and
//! `operators_and_functions_v0.1.0.json#/constructors` all name
//! `error.literal.invalid` for a malformed literal, and the registry stages that
//! error as lexical. A literal `STRING` argument's validity is a fact of the
//! token stream, so the lexer decides it; a dynamically supplied argument is
//! left to `expression_demand_resolution`.

mod common;

use common::*;
use lcl_lexer::TokenKind;

fn document(value: &str) -> String {
    format!("DATA:\n    VALUE: {value}\n")
}

fn accepted(value: &str) {
    let l = lex(&document(value));
    assert_well_formed(&l, value);
    assert!(l.diagnostics().is_empty(), "{value}: {:?}", ids(&l));
}

/// One `error.literal.invalid` whose span is exactly the offending quoted
/// literal, which then yields no token.
fn rejected(value: &str, offending: &str) {
    let l = lex(&document(value));
    assert_well_formed(&l, value);
    assert_eq!(
        id_list(&l),
        vec!["error.literal.invalid"],
        "{value}: {:?}",
        ids(&l)
    );
    let d = l.primary().unwrap();
    assert_eq!(d.span.slice(l.source()), Some(offending), "{value}");
    assert_eq!(
        d.span.start,
        document("").len() - 1 + value.find(offending).unwrap()
    );
    assert_eq!(l.terminal_status(), Some("status.invalid"));
    assert_eq!(d.cause.as_str(), "constructor_literal");
    let withdrawn = l
        .tokens_of(TokenKind::String)
        .all(|t| l.lexeme(t) != Some(offending));
    assert!(withdrawn, "{value}: rejected literal must yield no token");
}

// ---------------------------------------------------------------------------
// REGEX flags
// ---------------------------------------------------------------------------

#[test]
fn regex_flags_accept_unique_canonical_order_subsequences() {
    for flags in ["", "i", "m", "s", "im", "is", "ms", "ims"] {
        accepted(&format!("REGEX(\"a\", \"{flags}\")"));
    }
    // Omitted flags equal the empty flags STRING.
    accepted("REGEX(\"a\")");
}

#[test]
fn regex_flags_reject_out_of_order_duplicate_and_unknown() {
    for flags in [
        "mi", "si", "sm", "smi", "mis", "ii", "imm", "iss", "x", "u", "I", "M", "ims ", " i",
        "i,m", "ism",
    ] {
        rejected(
            &format!("REGEX(\"a\", \"{flags}\")"),
            &format!("\"{flags}\""),
        );
    }
}

#[test]
fn the_canonical_example_17_is_rejected_on_its_flags_literal() {
    let source =
        std::fs::read(canonical_root().join("08_EXAMPLES/INVALID/17_REGEX_FLAGS.invalid.lcl"))
            .unwrap();
    let l = lex_bytes(&source);
    assert_eq!(id_list(&l), vec!["error.literal.invalid"]);
    let d = l.primary().unwrap();
    assert_eq!(d.span.slice(l.source()), Some("\"mi\""));
    assert_eq!(l.terminal_status(), Some("status.invalid"));
    assert!(d
        .detail
        .as_deref()
        .unwrap()
        .contains("out of canonical `ims` order"));
}

// ---------------------------------------------------------------------------
// REGEX pattern grammar
// ---------------------------------------------------------------------------

#[test]
fn regex_patterns_in_the_closed_grammar_are_accepted() {
    for pattern in [
        "",
        "a",
        "^[a-z]+$",
        "a|b",
        "a|",
        "|",
        "()",
        "(?:ab)*",
        "(a)(b)",
        "a{2}",
        "a{2,}",
        "a{2,5}",
        "a{0}",
        ".",
        ".*",
        "[^a-z0-9_-]",
        "[-a]",
        "[a-]",
        "[a-z-]",
        "[.^$|*+?(){}]",
        "\\\\d+\\\\w*\\\\s?\\\\D\\\\W\\\\S",
        "\\\\.\\\\^\\\\$\\\\|\\\\?\\\\*\\\\+\\\\(\\\\)\\\\[\\\\]\\\\{\\\\}\\\\\\\\\\\\/\\\\-",
        "\\\\n\\\\r\\\\t",
        "[\\\\d\\\\]\\\\[\\\\\\\\]",
        "[\\\\n-\\\\r]",
        "(a|b)+c?",
        "^$",
        "日本",
        "[日-本]",
    ] {
        accepted(&format!("REGEX(\"{pattern}\")"));
    }
}

#[test]
fn regex_patterns_outside_the_closed_grammar_are_rejected() {
    for pattern in [
        "a**",
        "a++",
        "a??",
        "a+?",
        "a*+",
        "*a",
        "+",
        "?",
        "a{",
        "a{}",
        "a{2,1}",
        "a{01}",
        "a{1,2}{3}",
        "^*",
        "$+",
        "(a",
        "a)",
        ")",
        "(?=a)",
        "(?!a)",
        "(?<=a)",
        "(?<!a)",
        "(?P<n>a)",
        "(?<n>a)",
        "(?i)a",
        "(?",
        "[a",
        "[",
        "[]",
        "[^]",
        "[z-a]",
        "[a-\\\\d]",
        "[\\\\d-z]",
        "[a-z-9]",
        "[a--b]",
        "[a[b]]",
        "a]",
        "a}",
        "{2}",
        "\\\\b",
        "\\\\u0041",
        "\\\\x41",
        "\\\\p{L}",
        "\\\\P{L}",
        "\\\\1",
        "\\\\a",
        "a\\\\",
    ] {
        rejected(&format!("REGEX(\"{pattern}\")"), &format!("\"{pattern}\""));
    }
}

#[test]
fn pattern_and_flags_are_independent_loci() {
    let l = lex(&document("REGEX(\"[a-z\", \"mi\")"));
    assert_eq!(
        id_list(&l),
        vec!["error.literal.invalid", "error.literal.invalid"]
    );
    assert_eq!(l.diagnostics()[0].span.slice(l.source()), Some("\"[a-z\""));
    assert_eq!(l.diagnostics()[1].span.slice(l.source()), Some("\"mi\""));
}

// ---------------------------------------------------------------------------
// GLOB
// ---------------------------------------------------------------------------

#[test]
fn glob_patterns_in_the_closed_profile_are_accepted() {
    for pattern in [
        "src/**/*.py",
        "**",
        "*",
        "?",
        "a",
        "a/b",
        "a/**/b",
        "*.py",
        ".hidden",
        "[!a-z]",
        "[a-z0-9_]",
        "[-a]",
        "[a-]",
        "\\\\*\\\\?\\\\[\\\\]\\\\\\\\\\\\{\\\\}\\\\!\\\\^\\\\-",
        "a?b*c",
        "[\\\\]\\\\-]",
    ] {
        accepted(&format!("GLOB(\"{pattern}\")"));
    }
}

#[test]
fn glob_patterns_outside_the_closed_profile_are_rejected() {
    for pattern in [
        "", "/a", "a/", "a//b", ".", "..", "./a", "a/../b", "a**", "**a", "a**b", "***", "a{b}",
        "a}", "a]", "[", "[]", "[!]", "[z-a]", "[a/b]", "[[]", "[a-z-9]", "\\\\x", "a\\\\", "\\\\",
    ] {
        rejected(&format!("GLOB(\"{pattern}\")"), &format!("\"{pattern}\""));
    }
}

// ---------------------------------------------------------------------------
// Temporal literals
// ---------------------------------------------------------------------------

#[test]
fn temporal_literals_in_the_exact_profile_are_accepted() {
    for date in [
        "2026-08-30",
        "2024-02-29",
        "2000-02-29",
        "0001-01-01",
        "9999-12-31",
        "2026-04-30",
    ] {
        accepted(&format!("DATE(\"{date}\")"));
    }
    for time in [
        "14:30:00",
        "14:30:00Z",
        "14:30:00.5",
        "14:30:00.000",
        "14:30:00.123456+02:00",
        "00:00:00-05:30",
        "23:59:59",
        "23:59:59+23:59",
        "14:30:00+00:00",
    ] {
        accepted(&format!("TIME(\"{time}\")"));
    }
    for datetime in [
        "2026-08-30T14:30:00",
        "2026-08-30T14:30:00+02:00",
        "2026-08-30T14:30:00.25Z",
        "2024-02-29T23:59:59-11:00",
    ] {
        accepted(&format!("DATETIME(\"{datetime}\")"));
    }
}

#[test]
fn temporal_literals_outside_the_exact_profile_are_rejected() {
    for date in [
        "2026-8-30",
        "2026-13-01",
        "2026-00-10",
        "2026-02-30",
        "2023-02-29",
        "1900-02-29",
        "2026-04-31",
        "0000-01-01",
        "2026-08-00",
        "2026-08-30T00:00:00",
        "2026/08/30",
        "20260830",
        "2026-08-30 ",
        " 2026-08-30",
        "",
    ] {
        rejected(&format!("DATE(\"{date}\")"), &format!("\"{date}\""));
    }
    for time in [
        "24:00:00",
        "14:60:00",
        "14:30:60",
        "14:30",
        "14:30:00z",
        "14:30:00.",
        "14:30:00-00:00",
        "14:30:00+2:00",
        "14:30:00+24:00",
        "14:30:00+02:60",
        "14:30:00 Z",
        "1:30:00",
        "14:30:00ZZ",
        "14:30:00+02:00Z",
        "T14:30:00",
        "",
    ] {
        rejected(&format!("TIME(\"{time}\")"), &format!("\"{time}\""));
    }
    for datetime in [
        "2026-08-30 14:30:00",
        "2026-08-30t14:30:00",
        "2026-08-30T24:00:00",
        "2026-02-30T00:00:00",
        "2026-08-30",
        "14:30:00",
        "2026-08-30T14:30:00-00:00",
        "",
    ] {
        rejected(
            &format!("DATETIME(\"{datetime}\")"),
            &format!("\"{datetime}\""),
        );
    }
}

// ---------------------------------------------------------------------------
// URI
// ---------------------------------------------------------------------------

#[test]
fn absolute_uris_with_a_scheme_are_accepted() {
    for uri in [
        "https://example.invalid/resource",
        "http://example.invalid",
        "mailto:user@example.invalid",
        "urn:isbn:0451450523",
        "file:///tmp/x",
        "http://[2001:db8::1]:8080/p?q=1",
        "http://[::1]/",
        "http://[2001:db8:0:0:0:0:0:1]/",
        "http://[::ffff:192.0.2.1]/",
        "http://[v1.fe80::a+en1]/",
        "http://127.0.0.1/",
        "ftp://u:p@h.example/a%20b",
        "a+b-c.d:x",
        "s:",
        "s:?q",
        "s:/p",
        "s://h?q=a/b?c",
        "http://h/a//b",
        "http://h/;p=1,2",
        // RFC 3986 `reg-name` admits any unreserved run, so a dotted quad out
        // of IPv4 range is still a syntactically valid host; and an empty
        // authority is legal, as in `file:///`.
        "http://256.1.1.1/",
        "s:///a",
    ] {
        accepted(&format!("URI(\"{uri}\")"));
    }
}

#[test]
fn relative_references_and_malformed_uris_are_rejected() {
    for uri in [
        "/relative/path",
        "relative/path",
        "example.invalid/resource",
        "//host/path",
        "https://example.invalid/#frag",
        "s:#",
        "1http://x",
        "-s:x",
        "://x",
        ":x",
        "",
        "http://exa mple/",
        "http://x/a b",
        "http://x/%zz",
        "http://x/%2",
        "http://x?a%",
        "http://[::1",
        "http://[2001:db8:::1]/",
        "http://[gggg::1]/",
        "http://[1:2:3:4:5:6:7]/",
        "http://[1:2:3:4:5:6:7:8:9]/",
        "http://[::1]x/",
        "http://[v.1]/",
        "http://x:8a/",
        "http://[::256.1.1.1]/",
        "http://ex/\u{e9}",
        "http://x/\\\\",
        "http://x/<>",
        "http://x/\\\"",
    ] {
        rejected(&format!("URI(\"{uri}\")"), &format!("\"{uri}\""));
    }
}

// ---------------------------------------------------------------------------
// Scope of the pass
// ---------------------------------------------------------------------------

#[test]
fn dynamically_supplied_arguments_and_other_overloads_are_not_judged_here() {
    // A reference or expression argument is execution-stage material.
    accepted("REGEX(REF(pattern.x), \"mi\")");
    accepted("REGEX(\"a\", REF(flags.x))");
    accepted("DATE(REF(input.day))");
    accepted("GLOB(REF(a) + \"x\")");
    // A wrong arity is error.operator.operand at the static stage.
    accepted("REGEX(\"[a-z\", \"mi\", \"extra\")");
    accepted("DATE(\"not-a-date\", \"mi\")");
    accepted("URI()");
    // Constructors without a closed literal profile are never validated.
    accepted("PATH(\"relative/no/rule\")");
    accepted("BYTES(-1)");
    accepted("PERCENTAGE(101)");
    // The word as a type designator is not a call.
    accepted("REGEX");
    assert!(errors_of("DATA:\n    TYPE: REGEX\n").is_empty());
    // A call that does not close on its line is left to the grammar.
    assert_eq!(
        errors_of("DATA:\n    VALUE: REGEX(\"a\", \"mi\"\n"),
        vec!["error.delimiter.unclosed"]
    );
}

fn errors_of(src: &str) -> Vec<String> {
    let l = lex(src);
    assert_well_formed(&l, src);
    id_list(&l)
}

#[test]
fn every_position_that_holds_a_literal_constructor_is_checked() {
    // FIELD.PATTERN
    let l = lex("DEFINE:\n    FIELD:\n        PATTERN: REGEX(\"^a\", \"mi\")\n");
    assert_eq!(id_list(&l), vec!["error.literal.invalid"]);
    assert_eq!(l.primary().unwrap().span.slice(l.source()), Some("\"mi\""));
    // SCOPE.INCLUDE
    let l = lex("SCOPE:\n    INCLUDE: GLOB(\"/abs/**\")\n");
    assert_eq!(id_list(&l), vec!["error.literal.invalid"]);
    // An operand of MATCHES inside an expression.
    let l = lex("DATA:\n    VALUE: \"x\" MATCHES REGEX(\"a\", \"sm\")\n");
    assert_eq!(id_list(&l), vec!["error.literal.invalid"]);
    // A member of an inline collection.
    let l = lex("DATA:\n    VALUE: [DATE(\"2026-08-30\"), DATE(\"2026-02-30\")]\n");
    assert_eq!(ids(&l).len(), 1);
    assert_eq!(
        l.primary().unwrap().span.slice(l.source()),
        Some("\"2026-02-30\"")
    );
    // A member line of a multiline collection.
    let l = lex("DATA:\n    VALUE: [\n        URI(\"https://ok.invalid/\"),\n        URI(\"nope\")\n    ]\n");
    assert_eq!(id_list(&l), vec!["error.literal.invalid"]);
    assert_eq!(
        l.primary().unwrap().span.slice(l.source()),
        Some("\"nope\"")
    );
}

#[test]
fn the_thirteen_valid_examples_use_only_valid_literals() {
    // Every REGEX, GLOB, DATE, TIME, DATETIME and URI literal the release ships
    // in its valid examples passes the profiles: a silent reminder that the
    // validators reject nothing the release accepts.
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut constructor_calls = 0usize;
    for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
        let bytes = std::fs::read(entry.path()).unwrap();
        let l = lex_bytes(&bytes);
        assert!(l.diagnostics().is_empty(), "{:?}", entry.path());
        let text = String::from_utf8(bytes).unwrap();
        for name in ["REGEX(", "GLOB(", "DATE(", "TIME(", "DATETIME(", "URI("] {
            constructor_calls += text.matches(name).count();
        }
    }
    assert!(
        constructor_calls >= 6,
        "the examples exercise the constructors"
    );
}
