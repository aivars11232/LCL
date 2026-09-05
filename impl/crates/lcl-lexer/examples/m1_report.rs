//! M1 report: lex every canonical source fixture and example, and print what
//! the lexer produced against what the release expects.
//!
//! Read-only with respect to `canonical/`. Prints; claims nothing beyond the
//! lexical stage.

use lcl_lexer::{Lexer, Lexicon, Outcome, TokenKind};
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
    let lexer = Lexer::new(&lexicon);

    println!("LCL Core {} — M1 lexer report", lexicon.formal_version());
    println!("package authority   : {}", spec.authority());
    println!("identity digest     : {}", spec.identity_digest());
    println!("reserved words      : {}", lexicon.reserved_words().count());
    println!("callables           : {}", lexicon.callables().count());
    println!(
        "adopted symbols     : {}",
        lexicon.adopted_symbols().count()
    );
    println!(
        "excluded lexemes    : {}",
        lexicon.excluded_lexemes().count()
    );
    println!(
        "lexical error ids   : {}",
        lcl_lexer::LexicalError::ALL.len()
    );
    println!();

    // ---- 09_CONFORMANCE/SOURCE_FIXTURES ------------------------------------
    println!("== SOURCE_FIXTURES (expected_results.json) ==");
    let fixture_dir = root.join("09_CONFORMANCE/SOURCE_FIXTURES");
    let expected_text = std::fs::read_to_string(fixture_dir.join("expected_results.json"))
        .expect("expected_results.json");
    let expected = lcl_spec::json::parse(&expected_text).expect("valid JSON");
    let Some(entries) = expected.as_object() else {
        eprintln!("expected_results.json is not an object");
        std::process::exit(2);
    };
    let mut fixture_pass = 0usize;
    for (name, want) in entries {
        let want = want.as_str().unwrap_or("?");
        let bytes = std::fs::read(fixture_dir.join(name)).expect("fixture bytes");
        let lexed = lexer.lex(&bytes);
        let got = match lexed.primary() {
            None => "accept".to_string(),
            Some(d) => d.id.to_string(),
        };
        let ok = got == want;
        fixture_pass += usize::from(ok);
        println!(
            "  {} {:<32} expected {:<40} got {:<40} tokens={} diagnostics={}",
            if ok { "PASS" } else { "FAIL" },
            name,
            want,
            got,
            lexed.tokens().len(),
            lexed.diagnostics().len()
        );
        for d in lexed.diagnostics() {
            println!("        - {d}");
        }
    }
    println!("  {fixture_pass}/{} fixtures agree", entries.len());
    println!();

    // ---- 08_EXAMPLES/VALID --------------------------------------------------
    println!("== 08_EXAMPLES/VALID (must tokenize with no lexical diagnostic) ==");
    let mut valid_pass = 0usize;
    let mut valid_total = 0usize;
    for path in sorted_lcl(&root.join("08_EXAMPLES/VALID")) {
        valid_total += 1;
        let bytes = std::fs::read(&path).expect("example bytes");
        let lexed = lexer.lex(&bytes);
        let ok = lexed.outcome() == Outcome::Tokenized;
        valid_pass += usize::from(ok);
        println!(
            "  {} {:<48} tokens={:<5} words={:<4} ids={:<4} strings={:<3} indents={:<3}",
            if ok { "PASS" } else { "FAIL" },
            file_name(&path),
            lexed.tokens().len(),
            lexed.tokens_of(TokenKind::ReservedWord).count(),
            lexed.tokens_of(TokenKind::SimpleIdentifier).count()
                + lexed.tokens_of(TokenKind::QualifiedIdentifier).count(),
            lexed.tokens_of(TokenKind::String).count(),
            lexed.tokens_of(TokenKind::Indent).count(),
        );
        for d in lexed.diagnostics() {
            println!("        - {d}");
        }
    }
    println!("  {valid_pass}/{valid_total} valid examples tokenize");
    println!();

    // ---- 08_EXAMPLES/INVALID ------------------------------------------------
    println!("== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!(
        "   lexical-stage expectation  -> lexer primary and status must equal it (no exceptions)"
    );
    println!("   later-stage expectation    -> lexer must raise nothing (earliest_stage_rule)");
    let diagnostics = lcl_diagnostics::DiagnosticRegistry::load(&spec).expect("diagnostics");
    let mut invalid_pass = 0usize;
    let mut invalid_total = 0usize;
    for path in sorted_lcl(&root.join("08_EXAMPLES/INVALID")) {
        invalid_total += 1;
        let expected_path = PathBuf::from(format!("{}.expected.txt", path.display()));
        let expectation = std::fs::read_to_string(&expected_path).expect("expected.txt");
        let want_error = field(&expectation, "EXPECTED_ERROR").unwrap_or_default();
        let want_status = field(&expectation, "EXPECTED_TERMINAL_STATUS").unwrap_or_default();
        let stage = diagnostics
            .error(&want_error)
            .map(|e| e.stage.to_string())
            .unwrap_or_else(|| "?".into());
        let bytes = std::fs::read(&path).expect("example bytes");
        let lexed = lexer.lex(&bytes);
        let (ok, note) = if stage == "lexical" {
            let got = lexed
                .primary()
                .map(|d| d.id.to_string())
                .unwrap_or_default();
            let status = lexed.terminal_status().unwrap_or_default();
            (
                got == want_error && status == want_status,
                format!("primary={got} status={status}"),
            )
        } else {
            (
                lexed.outcome() == Outcome::Tokenized,
                format!("lexically clean; {want_error} is {stage}-stage"),
            )
        };
        invalid_pass += usize::from(ok);
        println!(
            "  {} {:<44} expects {:<34} {note}",
            if ok { "PASS" } else { "FAIL" },
            file_name(&path),
            want_error
        );
        for d in lexed.diagnostics() {
            println!("        - {d}");
        }
    }
    println!("  {invalid_pass}/{invalid_total} invalid examples consistent");
    println!();

    println!(
        "Scope: lexical stage only. No parse, resolution, type or execution result is implied."
    );
    println!("M2 (parser) has not been started.");

    if fixture_pass != entries.len() || valid_pass != valid_total || invalid_pass != invalid_total {
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
