//! M2 report: parse every canonical example and print what the parser produced
//! against what the release expects.
//!
//! This is the executed form of the release validator's
//! `complete_example_parse_matrix` check. That check is classified
//! OUT_OF_SCOPE in `validate_release.py` because the bare-language package
//! ships no parser; the classification defers the matrix to an implementation,
//! and this is that implementation's result.
//!
//! Read-only with respect to `canonical/`. Prints; claims nothing beyond the
//! grammar-and-schema stage. Exits non-zero if any expectation is unmet.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_lexer::{Lexer, Lexicon};
use lcl_parser::{Grammar, Outcome, Parser};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn main() {
    let root = canonical_root();
    let spec = match SpecPackage::open(&root) {
        Ok(spec) => spec,
        Err(e) => {
            eprintln!("cannot open the approved package: {e}");
            std::process::exit(2);
        }
    };
    let lexicon = match Lexicon::load(&spec) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot load the lexicon: {e}");
            std::process::exit(2);
        }
    };
    let grammar = match Grammar::load(&spec) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("cannot load the grammar: {e}");
            std::process::exit(2);
        }
    };
    let registry = match DiagnosticRegistry::load(&spec) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot load the diagnostic registry: {e}");
            std::process::exit(2);
        }
    };
    let lexer = Lexer::new(&lexicon);
    let parser = Parser::new(&grammar);

    println!("LCL Core {} — M2 parser report", grammar.formal_version());
    println!("package authority   : {}", spec.authority());
    println!("identity digest     : {}", spec.identity_digest());
    println!("block schemas       : {}", grammar.block_count());
    println!("field signatures    : {}", grammar.field_count());
    println!("value kinds         : {}", grammar.value_kinds().len());
    println!("document kinds      : {}", grammar.document_kinds().count());
    println!(
        "grammar errors      : {}",
        registry.errors_by_stage(Stage::GrammarOrSchema).len()
    );
    println!(
        "enforced conditions : {}",
        lcl_parser::enforced_requirements().len()
    );
    println!();

    let mut failures = 0usize;

    // -- valid examples ----------------------------------------------------
    println!("== 08_EXAMPLES/VALID (must parse with no grammar diagnostic) ==");
    let valid = sorted_lcl(&root.join("08_EXAMPLES/VALID"));
    let mut valid_pass = 0usize;
    for path in &valid {
        let bytes = std::fs::read(path).expect("read");
        let lexed = lexer.lex(&bytes);
        let (ok, note) = match parser.parse(&lexed) {
            Err(e) => (false, format!("lexical stage failed: {e}")),
            Ok(parsed) => {
                let blocks = parsed.document().blocks().count();
                if parsed.outcome() == Outcome::Parsed {
                    (true, format!("blocks={blocks}"))
                } else {
                    (
                        false,
                        parsed
                            .diagnostics()
                            .iter()
                            .map(|d| d.id.to_string())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )
                }
            }
        };
        valid_pass += usize::from(ok);
        println!(
            "  {} {:<46} {note}",
            if ok { "PASS" } else { "FAIL" },
            file_name(path)
        );
    }
    println!("  {valid_pass}/{} valid examples parse", valid.len());
    failures += valid.len() - valid_pass;
    println!();

    // -- invalid examples --------------------------------------------------
    println!("== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!("   lexical expectation        -> M1 owns it; the grammar stage never runs");
    println!("   grammar/schema expectation -> the pinned identifier must be raised here");
    println!(
        "   later-stage expectation    -> the parser must raise nothing (earliest_stage_rule)"
    );
    let invalid = sorted_lcl(&root.join("08_EXAMPLES/INVALID"));
    let mut invalid_pass = 0usize;
    let (mut lexical_n, mut grammar_n, mut later_n) = (0usize, 0usize, 0usize);
    for path in &invalid {
        let expected = std::fs::read_to_string(format!("{}.expected.txt", path.display()))
            .expect("expectation");
        let want_error = field(&expected, "EXPECTED_ERROR").expect("EXPECTED_ERROR");
        let want_status = field(&expected, "EXPECTED_TERMINAL_STATUS").expect("status");
        let stage = registry.error(&want_error).expect("registered").stage;
        let bytes = std::fs::read(path).expect("read");
        let lexed = lexer.lex(&bytes);

        let (ok, note) = if stage == Stage::Lexical {
            lexical_n += 1;
            (
                lexed.primary().is_some(),
                format!("lexical stage owns it: {want_error}"),
            )
        } else if lexed.primary().is_some() {
            (
                false,
                format!(
                    "a {stage} expectation requires a clean lexical stage, got {}",
                    lexed
                        .primary()
                        .map(|d| d.id.to_string())
                        .unwrap_or_default()
                ),
            )
        } else {
            match parser.parse(&lexed) {
                Err(e) => (false, e.to_string()),
                Ok(parsed) => {
                    let raised: Vec<String> = parsed
                        .diagnostics()
                        .iter()
                        .map(|d| d.id.to_string())
                        .collect();
                    if stage == Stage::GrammarOrSchema {
                        grammar_n += 1;
                        let has = raised.contains(&want_error);
                        let status_ok = parsed.terminal_status() == Some(want_status.as_str());
                        let primary = parsed
                            .primary()
                            .map(|d| format!("{} at byte {}", d.id, d.span.start))
                            .unwrap_or_else(|| "none".into());
                        (has && status_ok, format!("primary={primary}"))
                    } else {
                        later_n += 1;
                        (
                            parsed.outcome() == Outcome::Parsed,
                            if raised.is_empty() {
                                format!("parses cleanly; {want_error} is {stage}-stage")
                            } else {
                                format!("unexpected: {}", raised.join(", "))
                            },
                        )
                    }
                }
            }
        };
        invalid_pass += usize::from(ok);
        println!(
            "  {} {:<46} expects {:<34} {note}",
            if ok { "PASS" } else { "FAIL" },
            file_name(path),
            want_error
        );
        if stage == Stage::GrammarOrSchema {
            if let Ok(parsed) = parser.parse(&lexed) {
                for d in parsed.diagnostics() {
                    println!("        - {d}");
                }
            }
        }
    }
    println!(
        "  {invalid_pass}/{} invalid examples consistent",
        invalid.len()
    );
    println!("  by stage: lexical={lexical_n} grammar_or_schema={grammar_n} later={later_n}");
    failures += invalid.len() - invalid_pass;
    println!();

    // -- conformance source fixtures --------------------------------------
    println!("== 09_CONFORMANCE/SOURCE_FIXTURES (total, no panic) ==");
    let fixtures = sorted_lcl(&root.join("09_CONFORMANCE/SOURCE_FIXTURES"));
    let mut parsed_n = 0usize;
    for path in &fixtures {
        let bytes = std::fs::read(path).expect("read");
        let lexed = lexer.lex(&bytes);
        if lexed.primary().is_some() {
            continue;
        }
        if parser.parse(&lexed).is_ok() {
            parsed_n += 1;
        }
    }
    println!(
        "  {parsed_n} of {} fixtures reached the grammar stage; the rest fail lexically",
        fixtures.len()
    );
    println!();

    println!("Deferred to a later milestone:");
    for (area, owner) in lcl_parser::deferred_requirements() {
        println!("  {area} -> {owner}");
    }
    println!();
    println!(
        "Scope: grammar and schema stage only. No resolution, type, validation or execution \
         result is implied."
    );
    println!("M3 (resolution) has not been started.");

    if failures != 0 {
        std::process::exit(1);
    }
}

fn sorted_lcl(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "lcl"))
        .collect();
    out.sort();
    out
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string()
}

fn field(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix(": ")))
        .map(|v| v.trim().to_string())
}
