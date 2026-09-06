//! M3 report: resolve every canonical example and print what the resolver
//! produced against what the release expects.
//!
//! Read-only with respect to `canonical/`. Prints; claims nothing beyond the
//! resolution stage. Exits non-zero if any expectation is unmet.

use lcl_diagnostics::{DiagnosticRegistry, Stage};
use lcl_lexer::{Lexer, Lexicon};
use lcl_parser::Grammar;
use lcl_resolver::{
    MemoryProvider, Outcome, ResolutionError, Resolver, Rules, SourceId, SourceUnit, DEFERRED,
};
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
    let lexicon = Lexicon::load(&spec).unwrap_or_else(|e| fatal("lexicon", &e));
    let grammar = Grammar::load(&spec).unwrap_or_else(|e| fatal("grammar", &e));
    let rules = Rules::load(&spec, &grammar).unwrap_or_else(|e| fatal("resolution rules", &e));
    let registry = DiagnosticRegistry::load(&spec).unwrap_or_else(|e| fatal("diagnostics", &e));
    let resolver = Resolver::new(&rules, &grammar, &lexicon);
    let _ = Lexer::new(&lexicon);

    println!("LCL Core {} — M3 resolver report", rules.lcl_version());
    println!("package authority   : {}", spec.authority());
    println!("identity digest     : {}", spec.identity_digest());
    println!("exact LCL version   : {}", rules.lcl_version());
    println!(
        "reserved namespaces : {}",
        rules.reserved_namespaces().count()
    );
    println!("core operations     : {}", rules.core_operation_count());
    println!("reference slots     : {}", rules.reference_slot_count());
    println!("declaring blocks    : {}", rules.declaring_blocks().count());
    println!(
        "resolution errors   : {} registered, {} emitted here, {} deferred",
        registry.errors_by_stage(Stage::Resolution).len(),
        ResolutionError::emitted().count(),
        DEFERRED.len()
    );
    println!();

    let mut failures = 0usize;

    // Every valid example is offered to the provider under its own filename, so
    // the one example that imports a sibling resolves exactly as written.
    let valid = sorted_lcl(&root.join("08_EXAMPLES/VALID"));
    let mut provider = MemoryProvider::new();
    for path in &valid {
        provider.insert(file_name(path), std::fs::read(path).expect("read"));
    }

    println!("== 08_EXAMPLES/VALID (must resolve with no resolution diagnostic) ==");
    let mut valid_pass = 0usize;
    for path in &valid {
        let bytes = std::fs::read(path).expect("read");
        let unit = SourceUnit::new(SourceId::new(file_name(path)), bytes);
        let (ok, note) = match resolver.resolve(&unit, &provider) {
            Err(e) => (false, format!("earlier stage failed: {e}")),
            Ok(resolved) => {
                if resolved.outcome() == Outcome::Resolved {
                    (
                        true,
                        format!(
                            "units={} decls={} refs={} graph={}",
                            resolved.unit_count(),
                            resolved.declarations().len(),
                            resolved.bindings().len(),
                            resolved.graph().len()
                        ),
                    )
                } else {
                    (
                        false,
                        resolved
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
    println!("  {valid_pass}/{} valid examples resolve", valid.len());
    failures += valid.len() - valid_pass;
    println!();

    println!("== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!("   earlier-stage expectation  -> M1/M2 own it; resolution never runs");
    println!("   resolution expectation     -> the pinned identifier must be raised here,");
    println!("                                 unless this milestone defers that identifier");
    println!("   later-stage expectation    -> the resolver must raise nothing");
    let invalid = sorted_lcl(&root.join("08_EXAMPLES/INVALID"));
    let mut invalid_pass = 0usize;
    let (mut earlier_n, mut resolution_n, mut deferred_n, mut later_n) = (0, 0, 0, 0);
    for path in &invalid {
        let expected =
            std::fs::read_to_string(format!("{}.expected.txt", path.display())).expect("expected");
        let want_error = field(&expected, "EXPECTED_ERROR").expect("EXPECTED_ERROR");
        let want_status = field(&expected, "EXPECTED_TERMINAL_STATUS").expect("status");
        let stage = registry.error(&want_error).expect("registered").stage;
        let bytes = std::fs::read(path).expect("read");
        let unit = SourceUnit::new(SourceId::new(file_name(path)), bytes);
        let deferred = ResolutionError::from_registry_str(&want_error)
            .is_some_and(ResolutionError::is_deferred);

        let (ok, note) = match resolver.resolve(&unit, &provider) {
            Err(skipped) => {
                earlier_n += 1;
                (
                    stage == Stage::Lexical || stage == Stage::GrammarOrSchema,
                    format!("{} stage owns it: {}", skipped.stage, skipped.primary),
                )
            }
            Ok(resolved) => {
                let raised: Vec<String> = resolved
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect();
                if stage == Stage::Resolution && deferred {
                    deferred_n += 1;
                    let owner = DEFERRED
                        .iter()
                        .find(|(e, _)| e.as_registry_str() == want_error)
                        .map(|(_, o)| *o)
                        .unwrap_or("a later milestone");
                    (
                        raised.is_empty(),
                        format!("DEFERRED to {owner}; not decided here"),
                    )
                } else if stage == Stage::Resolution {
                    resolution_n += 1;
                    let has = raised.contains(&want_error);
                    let status_ok = resolved.terminal_status() == Some(want_status.as_str());
                    let primary = resolved
                        .primary()
                        .map(|d| format!("{} at byte {}", d.id, d.span.start))
                        .unwrap_or_else(|| "none".into());
                    (has && status_ok, format!("primary={primary}"))
                } else if stage == Stage::Lexical || stage == Stage::GrammarOrSchema {
                    earlier_n += 1;
                    (
                        false,
                        format!("an earlier-stage expectation reached resolution: {want_error}"),
                    )
                } else {
                    later_n += 1;
                    (
                        raised.is_empty(),
                        if raised.is_empty() {
                            format!("resolves cleanly; {want_error} is {stage}-stage")
                        } else {
                            format!("unexpected: {}", raised.join(", "))
                        },
                    )
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
        if stage == Stage::Resolution && !deferred {
            if let Ok(resolved) = resolver.resolve(&unit, &provider) {
                for d in resolved.diagnostics() {
                    println!("        - {d}");
                }
            }
        }
    }
    println!(
        "  {invalid_pass}/{} invalid examples consistent",
        invalid.len()
    );
    println!(
        "  by stage: earlier={earlier_n} resolution={resolution_n} deferred={deferred_n} later={later_n}"
    );
    failures += invalid.len() - invalid_pass;
    println!();

    println!("== 09_CONFORMANCE/SOURCE_FIXTURES (total, no panic) ==");
    let fixtures = sorted_lcl(&root.join("09_CONFORMANCE/SOURCE_FIXTURES"));
    let mut reached = 0usize;
    for path in &fixtures {
        let bytes = std::fs::read(path).expect("read");
        let unit = SourceUnit::new(SourceId::new(file_name(path)), bytes);
        if resolver.resolve(&unit, &provider).is_ok() {
            reached += 1;
        }
    }
    println!(
        "  {reached} of {} fixtures reached the resolution stage; the rest fail earlier",
        fixtures.len()
    );
    println!();

    println!("Registered resolution identifiers deferred by this milestone:");
    for (id, owner) in DEFERRED {
        println!("  {id} -> {owner}");
    }
    println!();
    println!("Owned by a later milestone, though decided before effects:");
    println!("  error.execution.order -> M5 ordering graph (registered stage: execution)");
    println!();
    println!("Scope: resolution stage only. No type, validation or execution result is implied.");
    println!("M4 (static and type checking) has not been started.");

    if failures != 0 {
        std::process::exit(1);
    }
}

fn fatal<E: std::fmt::Display>(what: &str, e: &E) -> ! {
    eprintln!("cannot load the {what}: {e}");
    std::process::exit(2);
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
