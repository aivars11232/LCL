//! The parser is total: it returns for every token stream and never panics.
//!
//! The parser's input is whatever M1 accepts, so every corpus here is lexed
//! first and parsed only when the lexical stage is clean — that is the exact
//! precondition `earliest_stage_rule` sets. Each parse runs under
//! `catch_unwind` so a failure names the bytes that caused it, and every result
//! is checked against the structural invariants in `common::assert_well_formed`.

mod common;

use common::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Lex, and parse when M1 accepts. Never panics.
fn check(bytes: &[u8], label: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let lexed = lex_bytes(bytes);
        if lexed.primary().is_some() {
            // The grammar stage is not evaluated; the API must say so rather
            // than produce a verdict.
            assert!(lcl_parser::Parser::new(grammar()).parse(&lexed).is_err());
            return;
        }
        let parsed = lcl_parser::Parser::new(grammar())
            .parse(&lexed)
            .expect("a clean lexical stage permits parsing");
        assert_well_formed(&parsed, lexed.source_len(), label);
    }));
    assert!(result.is_ok(), "{label}: panicked on {bytes:?}");
}

/// A small deterministic xorshift generator, so the random corpus is the same
/// on every run and every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn every_single_byte() {
    for b in 0..=255u8 {
        check(&[b], &format!("byte {b:#04x}"));
        check(&[b, b'\n'], &format!("byte {b:#04x} + LF"));
    }
}

#[test]
fn random_bytes_biased_to_the_lcl_alphabet() {
    const ALPHABET: &[u8] =
        b"LCLSPECIFICATIONDATAVERSIONIDTYPEVALUE: \n\"[](),.REF0123456789abc_-+*/=<>!";
    let mut rng = Rng(0x5eed_1234_9abc_def0);
    let mut buf = Vec::with_capacity(256);
    for case in 0..3000 {
        buf.clear();
        let len = rng.below(200);
        for _ in 0..len {
            buf.push(ALPHABET[rng.below(ALPHABET.len())]);
        }
        buf.push(b'\n');
        check(&buf, &format!("random {case}"));
    }
}

#[test]
fn random_structured_documents() {
    const KEYS: &[&str] = &[
        "ID", "TYPE", "VALUE", "NAME", "VERSION", "KIND", "ITEM", "MODE",
    ];
    const VALUES: &[&str] = &[
        "\"s\"",
        "1",
        "TRUE",
        "NULL",
        "a.b",
        "REF(a.b)",
        "[REF(a.b)]",
        "STRING",
        "LIST[STRING]",
        "[1, 2]",
        "1 + 2",
        "NOT TRUE",
        "PATH(\"/tmp/a\")",
    ];
    const BLOCKS: &[&str] = &[
        "DATA", "INPUT", "OUTPUT", "GOAL", "ACTION", "STEP", "DEFINE",
    ];
    let mut rng = Rng(0xfeed_face_1357_9bdf);
    for case in 0..2000 {
        let mut src = String::from("LCL:\n    VERSION: \"0.1.0\"\n\n");
        let blocks = 1 + rng.below(4);
        for _ in 0..blocks {
            src.push_str(BLOCKS[rng.below(BLOCKS.len())]);
            src.push_str(":\n");
            let fields = 1 + rng.below(4);
            for _ in 0..fields {
                let depth = rng.below(3);
                src.push_str(&" ".repeat(4 * (1 + depth)));
                src.push_str(KEYS[rng.below(KEYS.len())]);
                if rng.below(6) == 0 {
                    src.push_str(":\n");
                } else {
                    src.push_str(": ");
                    src.push_str(VALUES[rng.below(VALUES.len())]);
                    src.push('\n');
                }
            }
            src.push('\n');
        }
        check(src.as_bytes(), &format!("structured {case}"));
    }
}

#[test]
fn every_canonical_file_is_parsed_without_panic() {
    for sub in [
        "08_EXAMPLES/VALID",
        "08_EXAMPLES/INVALID",
        "09_CONFORMANCE/SOURCE_FIXTURES",
    ] {
        let dir = canonical_root().join(sub);
        for entry in std::fs::read_dir(dir).expect("dir").filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|x| x == "lcl") {
                let bytes = std::fs::read(&path).expect("read");
                check(&bytes, &path.display().to_string());
            }
        }
    }
}

#[test]
fn every_prefix_and_suffix_of_every_canonical_source() {
    // Truncation is the cheapest way to reach states a well-formed document
    // never produces: an unterminated block, a value cut mid-expression, a
    // dedent that never arrives.
    for sub in ["08_EXAMPLES/VALID", "08_EXAMPLES/INVALID"] {
        let dir = canonical_root().join(sub);
        for entry in std::fs::read_dir(dir).expect("dir").filter_map(Result::ok) {
            let path = entry.path();
            if !path.extension().is_some_and(|x| x == "lcl") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read");
            let name = path.display().to_string();
            for cut in 0..bytes.len() {
                check(&bytes[..cut], &format!("{name} prefix {cut}"));
                check(&bytes[cut..], &format!("{name} suffix {cut}"));
            }
        }
    }
}

#[test]
fn single_byte_mutations_of_a_valid_document() {
    let path = canonical_root().join("08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl");
    let original = std::fs::read(&path).expect("read");
    let interesting: &[u8] = b"\n \":[](),.-+*/=<>!ABCabc019";
    for index in 0..original.len() {
        for &byte in interesting {
            let mut mutated = original.clone();
            mutated[index] = byte;
            check(&mutated, &format!("mutation at {index} to {byte:#04x}"));
        }
    }
}

#[test]
fn deep_nesting_and_long_runs() {
    // Indentation depth, long collections and long operator chains must not
    // exhaust the stack or wedge the cursor.
    for depth in [1usize, 8, 32, 64] {
        let mut src = String::from("LCL:\n    VERSION: \"0.1.0\"\n\nDATA:\n");
        for level in 0..depth {
            src.push_str(&" ".repeat(4 * (level + 1)));
            src.push_str(&format!("k{level}:\n"));
        }
        src.push_str(&" ".repeat(4 * (depth + 1)));
        src.push_str("leaf: 1\n");
        check(src.as_bytes(), &format!("nesting depth {depth}"));
    }

    for count in [1usize, 50, 500] {
        let members: Vec<String> = (0..count).map(|i| i.to_string()).collect();
        let src = format!(
            "LCL:\n    VERSION: \"0.1.0\"\n\nDATA:\n    VALUE: [{}]\n",
            members.join(", ")
        );
        check(src.as_bytes(), &format!("collection of {count}"));
    }

    for count in [1usize, 50, 200] {
        let terms: Vec<String> = (0..count).map(|_| "1".to_string()).collect();
        let src = format!(
            "LCL:\n    VERSION: \"0.1.0\"\n\nDATA:\n    VALUE: {}\n",
            terms.join(" + ")
        );
        check(src.as_bytes(), &format!("chain of {count}"));
    }
}

#[test]
fn results_are_identical_across_repeated_parsing() {
    // Determinism: same bytes, same grammar, same result — no map iteration
    // order, no filesystem order, no clock.
    let path = canonical_root().join("08_EXAMPLES/INVALID/10_CONTEXT_WITHOUT_SCOPE.invalid.lcl");
    let bytes = std::fs::read(&path).expect("read");
    let first = parse_bytes(&bytes);
    for _ in 0..16 {
        let again = parse_bytes(&bytes);
        assert_eq!(first.document(), again.document());
        assert_eq!(first.diagnostics(), again.diagnostics());
    }
}
