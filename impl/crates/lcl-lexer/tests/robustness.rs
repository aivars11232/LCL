//! The lexer is total: it returns for every input and never panics.
//!
//! Every input here is lexed under `catch_unwind` so a failure names the bytes
//! that caused it, and every result is checked against the structural
//! invariants in `common::assert_well_formed`.

mod common;

use common::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn check(bytes: &[u8], label: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let lexed = lex_bytes(bytes);
        assert_well_formed(&lexed, label);
        lexed
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
        (self.next() % n as u64) as usize
    }
}

#[test]
fn every_single_byte() {
    for b in 0..=255u8 {
        check(&[b], &format!("byte {b:#04x}"));
        check(&[b, b'\n'], &format!("byte {b:#04x} + LF"));
        check(&[b'"', b, b'"', b'\n'], &format!("quoted byte {b:#04x}"));
    }
}

#[test]
fn every_byte_pair() {
    let mut buf = [0u8; 3];
    buf[2] = b'\n';
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            buf[0] = a;
            buf[1] = b;
            check(&buf, &format!("pair {a:#04x} {b:#04x}"));
        }
    }
}

#[test]
fn random_bytes_biased_to_lcl_alphabet() {
    const ALPHABET: &[u8] = b"LCLVERSIONREFabcxyz_0123456789 \n\t\r\"\\.:,()[]<>=!+-*/#;'%@{}`~^?$|&\xc3\xa9\xef\xbb\xbf\xe2\x80\x9c\xff\x01\x7f";
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for i in 0..4000 {
        let len = rng.below(if i % 10 == 0 { 2048 } else { 64 });
        let bytes: Vec<u8> = (0..len)
            .map(|_| {
                if rng.below(8) == 0 {
                    rng.below(256) as u8
                } else {
                    ALPHABET[rng.below(ALPHABET.len())]
                }
            })
            .collect();
        check(&bytes, &format!("random #{i}"));
    }
}

#[test]
fn random_structured_lines() {
    const PIECES: &[&str] = &[
        "LCL:", "VERSION:", "ID:", "VALUE:", "X:", "\"\"\"", "\"", "\\u", "\\uD83D", "\\uDE00",
        "REF(", ")", "[", "]", ",", " ", "  ", "    ", "        ", "\n", "\n\n", "a.b", "a", "0",
        "1.5", "007", "TRUE", "true", "Lcl", "MUST", "==", "=", "->", "<tag>", "@", "\\", "é",
        "\u{201C}", "\t", "\r", "\u{0085}", "...", "```", "//", "/*",
    ];
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for i in 0..3000 {
        let n = rng.below(40);
        let text: String = (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect();
        check(text.as_bytes(), &format!("structured #{i}"));
    }
}

#[test]
fn every_canonical_file_is_lexed_without_panic() {
    // The whole package, not just LCL sources: registries, prose, EBNF, Python.
    let root = canonical_root();
    let mut count = 0usize;
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("dir").filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("file");
                check(&bytes, &path.display().to_string());
                count += 1;
            }
        }
    }
    assert_eq!(count, 176, "the approved package has 176 files");
}

#[test]
fn every_prefix_and_suffix_of_every_lcl_source() {
    let root = canonical_root();
    let dirs = [
        "09_CONFORMANCE/SOURCE_FIXTURES",
        "08_EXAMPLES/VALID",
        "08_EXAMPLES/INVALID",
    ];
    for dir in dirs {
        for entry in std::fs::read_dir(root.join(dir))
            .expect("dir")
            .filter_map(Result::ok)
        {
            let path = entry.path();
            if !path.extension().is_some_and(|x| x == "lcl") {
                continue;
            }
            let bytes = std::fs::read(&path).expect("file");
            let name = path.display().to_string();
            for cut in 0..=bytes.len() {
                check(&bytes[..cut], &format!("{name} prefix {cut}"));
                check(&bytes[cut..], &format!("{name} suffix {cut}"));
            }
        }
    }
}

#[test]
fn single_byte_mutations_of_every_fixture() {
    const REPLACEMENTS: &[u8] = b"\"\\\n\t\r [](){}.:,'#=<>0aA_\xff\xef\xe2\x80\x9c";
    let root = canonical_root().join("09_CONFORMANCE/SOURCE_FIXTURES");
    for entry in std::fs::read_dir(&root)
        .expect("dir")
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !path.extension().is_some_and(|x| x == "lcl") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("file");
        let name = path.display().to_string();
        for i in 0..bytes.len() {
            for &r in REPLACEMENTS {
                let mut m = bytes.clone();
                m[i] = r;
                check(&m, &format!("{name} byte {i} -> {r:#04x}"));
                let mut ins = bytes.clone();
                ins.insert(i, r);
                check(&ins, &format!("{name} insert {r:#04x} at {i}"));
            }
            let mut del = bytes.clone();
            del.remove(i);
            check(&del, &format!("{name} delete {i}"));
        }
    }
}

#[test]
fn deep_nesting_and_long_runs() {
    let mut deep = String::new();
    for level in 0..200 {
        deep.push_str(&"    ".repeat(level));
        deep.push_str("X:\n");
    }
    deep.push_str(&"    ".repeat(200));
    deep.push_str("Y: 1\n");
    check(deep.as_bytes(), "200 levels");

    check(&"(".repeat(10_000).into_bytes(), "10k open parens");
    check(&"\"".repeat(10_001).into_bytes(), "10k quotes");
    check(&"a.".repeat(10_000).into_bytes(), "10k dotted");
    check(&"\\u".repeat(10_000).into_bytes(), "10k escapes");
    check(&" ".repeat(100_000).into_bytes(), "100k spaces");
    check(&"\n".repeat(100_000).into_bytes(), "100k blank lines");
    check(&"9".repeat(100_000).into_bytes(), "100k digits");
    check(&"A".repeat(100_000).into_bytes(), "100k uppercase");
    check("X: \"\"\"\n".as_bytes(), "unclosed multiline");
    check(&[0xEF, 0xBB], "truncated BOM");
    check(&[0xEF, 0xBB, 0xBF], "BOM only");
    check(&[0xF0, 0x9F, 0x98], "truncated 4-byte scalar");
    check(&[0xED, 0xA0, 0x80], "encoded surrogate");
}

#[test]
fn results_are_identical_across_repeated_lexing() {
    let root = canonical_root().join("08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl");
    let bytes = std::fs::read(root).unwrap();
    let a = lex_bytes(&bytes);
    let b = lex_bytes(&bytes);
    assert_eq!(a.tokens(), b.tokens());
    assert_eq!(a.diagnostics(), b.diagnostics());
    assert_eq!(a.render_tokens(), b.render_tokens());
}
