//! The two closed pattern profiles, executed against their canonical witnesses.

use lcl_runtime::{Flags, Glob, PatternFault, Regex};

fn regex(pattern: &str, flags: &str) -> Regex {
    Regex::compile(pattern, Flags::parse(flags).expect("admitted flags"))
        .expect("the pattern is admitted by the closed profile")
}

fn matches(pattern: &str, flags: &str, input: &str) -> bool {
    regex(pattern, flags).matches(input).expect("within limits")
}

// ---------------------------------------------------------------------------
// Decision witnesses
// ---------------------------------------------------------------------------

#[test]
fn closure_030_ascii_folding_only() {
    // "REGEX(\"[a-z]+\", \"i\") matched against \"ABC\" => TRUE under ASCII
    // folding; non-ASCII case equivalents are not silently added."
    assert!(matches("[a-z]+", "i", "ABC"));
    assert!(!matches("[a-z]+", "", "ABC"));
    // A non-ASCII scalar gains no case partner.
    assert!(!matches("[a-z]+", "i", "\u{C4}"));
}

#[test]
fn closure_031_lookaround_is_outside_the_closed_grammar() {
    // "REGEX(\"(?=a)\") => error.literal.invalid; lookaround is outside the
    // closed REGEX grammar."
    let compiled = Regex::compile("(?=a)", Flags::default());
    assert!(
        matches!(compiled, Err(PatternFault::Invalid(_))),
        "{compiled:?}"
    );
}

#[test]
fn closure_032_a_standalone_double_star_consumes_zero_segments() {
    // "GLOB(\"src/**/test?.lcl\") against src/test1.lcl => TRUE; standalone **
    // consumes zero or more complete path segments."
    let glob = Glob::compile("src/**/test?.lcl").expect("admitted");
    assert!(glob.matches("src/test1.lcl").expect("within limits"));
    assert!(glob.matches("src/a/b/test9.lcl").expect("within limits"));
    assert!(!glob.matches("src/test10.lcl").expect("within limits"));
}

// ---------------------------------------------------------------------------
// REGEX profile
// ---------------------------------------------------------------------------

#[test]
fn matching_is_full_string() {
    // "Success requires one derivation consuming the entire input from its
    // first boundary through its final boundary."
    assert!(matches("abc", "", "abc"));
    assert!(!matches("abc", "", "xabcx"));
    assert!(!matches("b", "", "abc"));
}

#[test]
fn every_forbidden_feature_is_refused() {
    for pattern in [
        "(?=a)",   // lookahead
        "(?<=a)",  // lookbehind
        "(a)\\1",  // backreference
        "(?<n>a)", // named group
        "(?i)a",   // inline flags
        "a*?",     // reluctant quantifier
        "a*+",     // possessive quantifier
        "\\p{L}",  // unicode property escape
        "\\b",     // unlisted escape
        "\\u0041", // unlisted escape
    ] {
        assert!(
            Regex::compile(pattern, Flags::default()).is_err(),
            "{pattern:?} must be refused by the closed grammar"
        );
    }
}

#[test]
fn only_the_three_registered_flags_are_admitted() {
    assert!(Flags::parse("ims").is_ok());
    assert!(Flags::parse("").is_ok());
    // "unknown_flags_allowed": false
    assert!(Flags::parse("g").is_err());
    assert!(Flags::parse("x").is_err());
    // "duplicate_flags_allowed": false
    assert!(Flags::parse("ii").is_err());
}

#[test]
fn dot_excludes_exactly_four_scalars_without_s() {
    // "Without s dot excludes exactly U+000A, U+000D, U+2028, and U+2029."
    for excluded in ['\u{A}', '\u{D}', '\u{2028}', '\u{2029}'] {
        assert!(
            !matches(".", "", &excluded.to_string()),
            "{excluded:?} must be excluded without s"
        );
        assert!(
            matches(".", "s", &excluded.to_string()),
            "{excluded:?} must be admitted with s"
        );
    }
    assert!(
        matches(".", "", "\u{85}"),
        "U+0085 is not in the excluded set"
    );
}

#[test]
fn anchors_accept_only_the_boundaries_without_m() {
    // "Without m they accept only the start and end respectively."
    assert!(matches("^a$", "", "a"));
    assert!(!matches("^a$", "", "b\na"));
    // "m: ^ also accepts a position immediately after U+000A; $ also accepts a
    // position immediately before U+000A."
    assert!(matches("a\\n^b", "m", "a\nb"));
    assert!(!matches("a\\n^b", "", "a\nb"));
}

#[test]
fn quantifiers_have_their_finite_concatenation_meaning() {
    assert!(matches("a*", "", ""));
    assert!(matches("a*", "", "aaaa"));
    assert!(!matches("a+", "", ""));
    assert!(matches("a?", "", ""));
    assert!(matches("a{3}", "", "aaa"));
    assert!(!matches("a{3}", "", "aa"));
    assert!(matches("a{2,}", "", "aaaaa"));
    assert!(!matches("a{2,}", "", "a"));
    assert!(matches("a{2,4}", "", "aaa"));
    assert!(!matches("a{2,4}", "", "aaaaa"));
}

#[test]
fn a_greedy_preference_cannot_change_acceptance() {
    // "no implementation strategy, capture state, or greedy/reluctant
    // preference changes Boolean acceptance." A backtracker that committed to
    // the longest `a*` would fail this; a state-set simulation cannot.
    assert!(matches("a*a", "", "aaa"));
    assert!(matches("(?:a|ab)c", "", "abc"));
    assert!(matches("(?:a*)*b", "", "aaab"));
}

#[test]
fn repetition_of_an_empty_matching_atom_terminates() {
    // "Repetition has the mathematical finite-concatenation meaning even when
    // its atom accepts empty text." A naive loop would not terminate here.
    assert!(matches("(?:a?)*", "", "aa"));
    assert!(matches("(?:)*", "", ""));
    assert!(!matches("(?:a?)*b", "", "aac"));
}

#[test]
fn descending_and_malformed_classes_are_invalid() {
    assert!(Regex::compile("[z-a]", Flags::default()).is_err());
    assert!(Regex::compile("[]", Flags::default()).is_err());
    assert!(Regex::compile("[a", Flags::default()).is_err());
    assert!(Regex::compile("[[a]]", Flags::default()).is_err());
    // A class escape cannot be a range endpoint.
    assert!(Regex::compile("[\\d-z]", Flags::default()).is_err());
}

#[test]
fn class_negation_applies_after_case_closure() {
    // "For classes, first close the positive member set under these pairs, then
    // apply any class negation."
    assert!(!matches("[^a]", "i", "A"));
    assert!(matches("[^a]", "", "A"));
}

#[test]
fn admitted_escapes_denote_their_scalars_and_classes() {
    assert!(matches("\\.", "", "."));
    assert!(!matches("\\.", "", "x"));
    assert!(matches("\\n", "", "\n"));
    assert!(matches("\\t", "", "\t"));
    assert!(matches("\\d+", "", "0123456789"));
    assert!(matches("\\w+", "", "aZ_9"));
    assert!(matches("\\s", "", " "));
    assert!(matches("\\S", "", "x"));
    assert!(!matches("\\d", "", "x"));
    assert!(matches("\\D", "", "x"));
}

#[test]
fn a_leading_zero_count_is_invalid() {
    // "Counts are exact nonnegative integers without leading zeroes."
    assert!(Regex::compile("a{01}", Flags::default()).is_err());
    assert!(Regex::compile("a{1}", Flags::default()).is_ok());
    // "{n,m} ... with n <= m"
    assert!(Regex::compile("a{3,2}", Flags::default()).is_err());
}

#[test]
fn an_assertion_cannot_be_quantified() {
    assert!(Regex::compile("^*", Flags::default()).is_err());
    assert!(Regex::compile("a**", Flags::default()).is_err());
}

#[test]
fn an_adversarial_pattern_is_refused_rather_than_run_forever() {
    // The classic catastrophic-backtracking shape. A state-set simulation
    // answers it directly; either way the runtime stays total.
    let compiled = Regex::compile("(?:a+)+b", Flags::default()).expect("admitted");
    let input = "a".repeat(40);
    match compiled.matches(&input) {
        Ok(accepted) => assert!(!accepted, "no derivation reaches b"),
        Err(PatternFault::ResourceLimit(_)) => {}
        Err(other) => panic!("unexpected fault: {other:?}"),
    }
}

#[test]
fn an_oversized_repetition_hits_the_declared_state_limit() {
    let compiled = Regex::compile("(?:abcdefghij){100000}", Flags::default());
    assert!(
        matches!(compiled, Err(PatternFault::ResourceLimit(_))),
        "{compiled:?}"
    );
}

#[test]
fn unicode_scalars_are_matched_exactly() {
    // "unicode_text_handling": "always_enabled_independently_of_user_flags"
    assert!(matches("A\u{1F600}", "", "A\u{1F600}"));
    assert!(matches("..", "", "A\u{1F600}"), "an emoji is one scalar");
}

// ---------------------------------------------------------------------------
// GLOB profile
// ---------------------------------------------------------------------------

#[test]
fn glob_matches_the_full_workspace_relative_path() {
    let glob = Glob::compile("a/b.txt").expect("admitted");
    assert!(glob.matches("a/b.txt").expect("within limits"));
    assert!(!glob.matches("x/a/b.txt").expect("within limits"));
    assert!(!glob.matches("a/b.txt/c").expect("within limits"));
}

#[test]
fn a_star_never_crosses_a_separator() {
    // "*": "zero_or_more_non_separator_characters"
    let glob = Glob::compile("a/*.txt").expect("admitted");
    assert!(glob.matches("a/b.txt").expect("within limits"));
    assert!(!glob.matches("a/b/c.txt").expect("within limits"));
}

#[test]
fn a_question_mark_consumes_exactly_one_non_separator() {
    let glob = Glob::compile("?.lcl").expect("admitted");
    assert!(glob.matches("a.lcl").expect("within limits"));
    assert!(!glob.matches("ab.lcl").expect("within limits"));
    assert!(!glob.matches(".lcl").expect("within limits"));
}

#[test]
fn glob_classes_exclude_the_separator_and_admit_ranges() {
    let glob = Glob::compile("[a-c].lcl").expect("admitted");
    assert!(glob.matches("b.lcl").expect("within limits"));
    assert!(!glob.matches("d.lcl").expect("within limits"));
    let negated = Glob::compile("[!a].lcl").expect("admitted");
    assert!(negated.matches("b.lcl").expect("within limits"));
    assert!(!negated.matches("a.lcl").expect("within limits"));
}

#[test]
fn invalid_glob_shapes_are_refused() {
    for pattern in [
        "/a",     // absolute
        "a/",     // trailing slash
        "a//b",   // empty segment
        "./a",    // literal . segment
        "../a",   // literal .. segment
        "a**b",   // ** not a whole segment
        "a**",    // ** not a whole segment
        "a{b,c}", // brace expansion is false
        "**a/b",  // ** not a whole segment
        "",       // empty
    ] {
        assert!(
            Glob::compile(pattern).is_err(),
            "{pattern:?} must be refused by the closed profile"
        );
    }
}

#[test]
fn adjacent_stars_in_an_ordinary_segment_are_invalid() {
    // "adjacent stars in an ordinary segment are invalid"
    assert!(
        Glob::compile("a/**").is_ok(),
        "** as a whole segment is legal"
    );
    assert!(Glob::compile("a/x**y").is_err());
}

#[test]
fn a_leading_dot_has_no_special_treatment() {
    // "leading dot has no special treatment"
    let glob = Glob::compile("*").expect("admitted");
    assert!(glob.matches(".hidden").expect("within limits"));
}

#[test]
fn glob_escapes_denote_literal_scalars() {
    let glob = Glob::compile("a\\*b").expect("admitted");
    assert!(glob.matches("a*b").expect("within limits"));
    assert!(!glob.matches("axb").expect("within limits"));
    assert!(
        Glob::compile("a\\zb").is_err(),
        "an unlisted escape is invalid"
    );
}

#[test]
fn glob_matching_is_case_sensitive() {
    // "Literal comparison is scalar-exact and case-sensitive"
    let glob = Glob::compile("A.lcl").expect("admitted");
    assert!(!glob.matches("a.lcl").expect("within limits"));
}
