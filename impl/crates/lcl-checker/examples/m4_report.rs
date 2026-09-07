//! M4 report: statically check every canonical example and print what the
//! checker produced against what the release expects.
//!
//! Read-only with respect to `canonical/`. Prints; claims nothing beyond the
//! static stage. Exits non-zero if any expectation is unmet.

use lcl_checker::{Checker, Contracts, Outcome, StaticError, DEFERRED};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_spec::SpecPackage;
use std::fs;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn fatal(what: &str, error: &dyn std::fmt::Display) -> ! {
    eprintln!("cannot load {what}: {error}");
    std::process::exit(2);
}

fn sorted_lcl(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| fatal("examples", &e))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "lcl"))
        .collect();
    out.sort();
    out
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn expectation(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path.with_extension("lcl.expected.txt")).ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix(key).map(str::trim).map(str::to_string))
}

fn main() {
    let root = canonical_root();
    let spec = match SpecPackage::open(&root) {
        Ok(spec) => spec,
        Err(e) => fatal("the approved package", &e),
    };
    let lexicon = Lexicon::load(&spec).unwrap_or_else(|e| fatal("lexicon", &e));
    let grammar = Grammar::load(&spec).unwrap_or_else(|e| fatal("grammar", &e));
    let rules = Rules::load(&spec, &grammar).unwrap_or_else(|e| fatal("resolution rules", &e));
    let contracts = Contracts::load(&spec).unwrap_or_else(|e| fatal("static contracts", &e));
    let resolver = Resolver::new(&rules, &grammar, &lexicon);
    let checker = Checker::new(&contracts);

    println!(
        "LCL Core {} — M4 static and type checker report",
        rules.lcl_version()
    );
    println!("package authority   : {}", spec.authority());
    println!("identity digest     : {}", spec.identity_digest());
    println!("registered types    : {}", contracts.type_row_count());
    println!(
        "ordered families    : {}",
        contracts.ordered_families().count()
    );
    println!("registered units    : {}", contracts.unit_count());
    println!("operators           : {}", contracts.operators().count());
    println!("pure functions      : {}", contracts.functions().count());
    println!("typed constructors  : {}", contracts.constructors().count());
    println!("operation contracts : {}", contracts.operations().count());
    println!(
        "static errors       : {} registered, {} emitted here, {} deferred",
        StaticError::ALL.len(),
        StaticError::emitted().count(),
        DEFERRED.len()
    );

    let mut failures = 0usize;

    // A provider holding every valid example, so an example that imports another
    // finds exactly what it named and nothing else.
    let valid_dir = root.join("08_EXAMPLES/VALID");
    let mut provider = MemoryProvider::new();
    for path in sorted_lcl(&valid_dir) {
        provider.insert(name(&path), fs::read(&path).unwrap_or_default());
    }

    println!("\n== 08_EXAMPLES/VALID (must check with no static diagnostic) ==");
    for path in sorted_lcl(&valid_dir) {
        let file = name(&path);
        let source = fs::read(&path).unwrap_or_default();
        let unit = SourceUnit::new(SourceId::new(&file), source);
        let Ok(resolved) = resolver.resolve(&unit, &provider) else {
            println!("  FAIL {file:<48} an earlier stage rejected it");
            failures += 1;
            continue;
        };
        let Ok(checked) = checker.check(&resolved) else {
            println!("  FAIL {file:<48} resolution did not succeed");
            failures += 1;
            continue;
        };
        match checked.outcome() {
            Outcome::Checked => println!(
                "  PASS {file:<48} annotations={:<4} types={:<3} deferred={}",
                checked.annotation_count(),
                checked.declaration_types().count(),
                checked.deferred().len()
            ),
            Outcome::Rejected => {
                failures += 1;
                println!("  FAIL {file:<48} {:?}", first_line(&checked));
            }
        }
    }

    println!("\n== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!("   earlier-stage expectation  -> M1/M2/M3 own it; static checking never runs");
    println!("   static expectation         -> the pinned identifier must be primary here");
    println!("   later-stage expectation    -> the checker must raise nothing");
    let invalid_dir = root.join("08_EXAMPLES/INVALID");
    let (mut earlier, mut here, mut later) = (0usize, 0usize, 0usize);
    for path in sorted_lcl(&invalid_dir) {
        let file = name(&path);
        let want = expectation(&path, "EXPECTED_ERROR:").unwrap_or_default();
        let want_status = expectation(&path, "EXPECTED_TERMINAL_STATUS:").unwrap_or_default();
        let source = fs::read(&path).unwrap_or_default();
        let unit = SourceUnit::new(SourceId::new(&file), source);

        let Ok(resolved) = resolver.resolve(&unit, &MemoryProvider::new()) else {
            earlier += 1;
            println!("  PASS {file:<48} expects {want:<34} an earlier stage owns it");
            continue;
        };
        if let Some(primary) = resolved.primary() {
            earlier += 1;
            println!(
                "  PASS {file:<48} expects {want:<34} resolution owns it: {}",
                primary.id
            );
            continue;
        }
        let Ok(checked) = checker.check(&resolved) else {
            failures += 1;
            println!("  FAIL {file:<48} resolution did not succeed");
            continue;
        };
        let raised: Vec<String> = checked
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect();
        if raised.contains(&want) {
            let status = checked.terminal_status().unwrap_or_default();
            if status == want_status {
                here += 1;
                println!(
                    "  PASS {file:<48} expects {want:<34} raised here at byte {}",
                    checked.primary().map(|d| d.span.start).unwrap_or(0)
                );
            } else {
                failures += 1;
                println!("  FAIL {file:<48} expects {want_status}, produced {status}");
            }
        } else if checked.outcome() == Outcome::Checked {
            later += 1;
            println!("  PASS {file:<48} expects {want:<34} a later stage owns it");
        } else {
            failures += 1;
            println!("  FAIL {file:<48} expects {want:<34} raised {raised:?}");
        }
    }
    println!("  by stage: earlier={earlier} static={here} later={later}");

    println!("\n== 09_CONFORMANCE/SOURCE_FIXTURES (total, no panic) ==");
    let mut reached = 0usize;
    let mut total = 0usize;
    for path in sorted_lcl(&root.join("09_CONFORMANCE/SOURCE_FIXTURES")) {
        total += 1;
        let unit = SourceUnit::new(
            SourceId::new(name(&path)),
            fs::read(&path).unwrap_or_default(),
        );
        let Ok(resolved) = resolver.resolve(&unit, &MemoryProvider::new()) else {
            continue;
        };
        if resolved.primary().is_some() {
            continue;
        }
        if checker.check(&resolved).is_ok() {
            reached += 1;
        }
    }
    println!("  {reached} of {total} fixtures reached the static stage; the rest fail earlier");

    if DEFERRED.is_empty() {
        println!("\nNo registered static identifier is deferred by this milestone.");
    } else {
        println!("\nRegistered static identifiers deferred by this milestone:");
        for (id, owner) in DEFERRED {
            println!("  {id} -> {owner}");
        }
    }

    println!("\nAlso decided here, with their own registered earlier stage:");
    println!("  error.reference.cycle  -> a kind.type BASE chain that resolves to itself");
    println!("  error.literal.invalid  -> a PATH form M1 deferred to the receiving field");

    println!("\nScope: static stage only. No semantic preflight, validation or");
    println!("execution result is implied. M5 (semantic preflight) has not been started.");

    if failures > 0 {
        eprintln!("\n{failures} expectation(s) unmet");
        std::process::exit(1);
    }
}

fn first_line(checked: &lcl_checker::Checked) -> String {
    checked
        .primary()
        .map(ToString::to_string)
        .or_else(|| {
            checked
                .earlier_stage_defects()
                .first()
                .map(ToString::to_string)
        })
        .unwrap_or_default()
}
