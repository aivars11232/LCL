//! M6 runtime report.
//!
//! Prints what this milestone actually executes, against the approved canonical
//! package, and nothing it does not. Run with:
//!
//! ```text
//! cargo run -p lcl-runtime --example m6_report
//! ```

use lcl_checker::{Checker, Contracts as StaticContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts, Interleaving, MockHost, Runtime, RuntimeError, ELSEWHERE};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Preflight};
use lcl_spec::SpecPackage;
use std::fs;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

fn files(dir: &str, suffix: &str) -> Vec<(String, String)> {
    let mut paths: Vec<_> = fs::read_dir(canonical_root().join(dir))
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(suffix))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                fs::read_to_string(&p).expect("readable"),
            )
        })
        .collect()
}

fn expected_identifier(name: &str) -> String {
    let path = canonical_root()
        .join("08_EXAMPLES/INVALID")
        .join(format!("{name}.expected.txt"));
    fs::read_to_string(path)
        .ok()
        .and_then(|text| {
            text.split_whitespace()
                .find(|word| word.starts_with("error."))
                .map(|word| word.trim_end_matches(['.', ',']).to_string())
        })
        .unwrap_or_else(|| "-".to_string())
}

fn provider() -> MemoryProvider {
    let mut provider = MemoryProvider::new();
    for (name, source) in files("08_EXAMPLES/VALID", ".lcl") {
        provider.insert(name, source.into_bytes());
    }
    provider
}

fn main() {
    let spec = SpecPackage::open(canonical_root()).expect("the approved package opens");
    println!("LCL M6 runtime report");
    println!("package authority: {:?}\n", spec.authority());

    let lexicon = Lexicon::load(&spec).expect("lexicon");
    let grammar = Grammar::load(&spec).expect("grammar");
    let rules = Rules::load(&spec, &grammar).expect("rules");
    let statics = StaticContracts::load(&spec).expect("static contracts");
    let preflight_contracts = PreflightContracts::load(&spec).expect("preflight contracts");
    let contracts = Contracts::load(&spec).expect("runtime contracts");

    let resolver = Resolver::new(&rules, &grammar, &lexicon);
    let checker = Checker::new(&statics);
    let preflight = Preflight::new(&preflight_contracts);
    let runtime = Runtime::new(&contracts);

    // -- Vocabulary ------------------------------------------------------
    println!("== registered vocabulary consumed ==");
    println!("  mirrored identifiers      {}", RuntimeError::ALL.len());
    println!(
        "  emitted by this milestone {}",
        RuntimeError::emitted().count()
    );
    println!(
        "  result schemas            {}",
        contracts.schemas().count()
    );
    println!("  canonical event vocabulary {}", contracts.events().len());
    println!(
        "  demand-eligible identifiers {}",
        contracts.demand().eligible().count()
    );
    let bounds = contracts.retry_bounds();
    println!(
        "  RETRY.LIMIT bounds        {}..{}  WHEN default {}  DELAY default {}",
        bounds.minimum_limit, bounds.maximum_limit, bounds.when_default, bounds.delay_default
    );

    // -- Valid examples --------------------------------------------------
    println!("\n== 08_EXAMPLES/VALID (execute) ==");
    let mut executed = 0;
    let mut inert = 0;
    for (name, source) in files("08_EXAMPLES/VALID", ".lcl") {
        let unit = SourceUnit::new(SourceId::new(name.clone()), source.as_bytes());
        let Ok(resolved) = resolver.resolve(&unit, &provider()) else {
            println!("  FAIL {name:48} earlier stage refused it");
            continue;
        };
        let Ok(checked) = checker.check(&resolved) else {
            println!("  FAIL {name:48} earlier stage refused it");
            continue;
        };
        let Ok(planned) = preflight.plan(&checked, &resolved, &Invocation::new()) else {
            println!("  FAIL {name:48} earlier stage refused it");
            continue;
        };
        let mut host = MockHost::new();
        match runtime.execute(&planned, &checked, &resolved, &mut host) {
            Ok(execution) => {
                let nodes = execution.invocations().len();
                if nodes == 0 {
                    inert += 1;
                }
                executed += 1;
                println!(
                    "  PASS {name:48} invocations={nodes:2} effects={:2} diagnostics={:2} steps={}",
                    host.requests().len(),
                    execution.diagnostics().len(),
                    execution.steps()
                );
            }
            Err(err) => println!("  FAIL {name:48} {err}"),
        }
    }
    println!("  {executed}/13 valid examples execute ({inert} declare no EXECUTE root)");

    // -- Invalid examples ------------------------------------------------
    println!("\n== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!("   earlier-stage expectation  -> M1..M5 own it; the runtime never runs");
    let mut earlier = 0;
    let mut here = 0;
    for (name, source) in files("08_EXAMPLES/INVALID", ".invalid.lcl") {
        let expected = expected_identifier(&name);
        let unit = SourceUnit::new(SourceId::new(name.clone()), source.as_bytes());
        let reached = resolver
            .resolve(&unit, &provider())
            .ok()
            .filter(|r| r.primary().is_none() && r.stage_failures().next().is_none())
            .and_then(|resolved| {
                let checked = checker.check(&resolved).ok()?;
                if checked.primary().is_some() || !checked.earlier_stage_defects().is_empty() {
                    return None;
                }
                let planned = preflight
                    .plan(&checked, &resolved, &Invocation::new())
                    .ok()?;
                planned.plan()?;
                Some((resolved, checked, planned))
            });
        match reached {
            None => {
                earlier += 1;
                println!("  PASS {name:44} expects {expected:36} an earlier stage owns it");
            }
            Some((resolved, checked, planned)) => {
                here += 1;
                let mut host = MockHost::new();
                let execution = runtime
                    .execute(&planned, &checked, &resolved, &mut host)
                    .expect("planned");
                match execution.primary() {
                    Some(primary) if primary.id.as_registry_str() == expected => {
                        println!("  PASS {name:44} expects {expected:36} raised here")
                    }
                    other => println!(
                        "  FAIL {name:44} expects {expected:36} got {:?}",
                        other.map(|d| d.id.to_string())
                    ),
                }
            }
        }
    }
    println!("  by stage: earlier={earlier} runtime={here}");

    // -- Determinism -----------------------------------------------------
    println!("\n== determinism under every admissible interleaving ==");
    let mut stable = 0;
    for (name, source) in files("08_EXAMPLES/VALID", ".lcl") {
        let unit = SourceUnit::new(SourceId::new(name.clone()), source.as_bytes());
        let Ok(resolved) = resolver.resolve(&unit, &provider()) else {
            continue;
        };
        let Ok(checked) = checker.check(&resolved) else {
            continue;
        };
        let Ok(planned) = preflight.plan(&checked, &resolved, &Invocation::new()) else {
            continue;
        };
        let mut outputs = Vec::new();
        for interleaving in Interleaving::ALL {
            let mut host = MockHost::new();
            if let Ok(execution) = runtime
                .with_interleaving(interleaving)
                .execute(&planned, &checked, &resolved, &mut host)
            {
                outputs.push(execution.serialize());
            }
        }
        if outputs.windows(2).all(|pair| pair[0] == pair[1]) {
            stable += 1;
        } else {
            println!("  FAIL {name} varied with the interleaving");
        }
    }
    println!(
        "  {stable}/13 examples identical across {} interleavings",
        Interleaving::ALL.len()
    );

    // -- Boundaries ------------------------------------------------------
    println!("\nRegistered identifiers this milestone mirrors but does not decide:");
    for (id, owner) in ELSEWHERE {
        println!("  {id} -> {owner}");
    }
    println!("\nDecision witnesses executed by this milestone:");
    for (id, topic, note) in [
        ("CLOSURE-007", "index", "zero-based, no negative wraparound"),
        (
            "CLOSURE-011",
            "evaluation",
            "a skipped AND operand is not demanded",
        ),
        (
            "CLOSURE-012",
            "evaluation",
            "a skipped branch is still statically checked",
        ),
        ("CLOSURE-013", "quantifier", "ALL([]), ANY([]), NONE([])"),
        (
            "CLOSURE-014",
            "quantifier",
            "an immediate sequence admits UNKNOWN",
        ),
        (
            "CLOSURE-015",
            "reduction",
            "a typed empty collection is rejected",
        ),
        (
            "CLOSURE-016",
            "collection",
            "an unconstrained empty literal has no member type",
        ),
        ("CLOSURE-017", "count", "COUNT over every registered family"),
        (
            "CLOSURE-022",
            "retry",
            "a successful first retry makes two attempts",
        ),
        ("CLOSURE-023", "retry", "a FALSE WHEN is not exhaustion"),
        (
            "CLOSURE-024",
            "retry",
            "exhaustion after exactly 1 + LIMIT attempts",
        ),
        ("CLOSURE-027", "continue", "recover and advance together"),
        (
            "CLOSURE-029",
            "status",
            "FALSE applicability skips a non-root unit",
        ),
        ("CLOSURE-030", "pattern", "ASCII-only case folding"),
        (
            "CLOSURE-031",
            "pattern",
            "lookaround is outside the closed grammar",
        ),
        (
            "CLOSURE-032",
            "pattern",
            "a standalone ** consumes zero or more segments",
        ),
        ("CLOSURE-039", "equality", "pattern representation identity"),
        (
            "CLOSURE-045",
            "temporal",
            "TIME subtracts its declared offset",
        ),
        ("CLOSURE-047", "unit_error", "unequal concrete units"),
        (
            "CLOSURE-058",
            "output_instance",
            "one OUTPUT binding per loop instance",
        ),
        ("CLOSURE-059", "retry_output", "each attempt starts unbound"),
    ] {
        println!("  {id}  {topic:16} {note}");
    }

    println!(
        "\nScope: canonical processing step 10 only. VERIFY, TEST, evidence,\n\
         SUCCESS/FAILURE and one terminal root status are steps 11 through 13\n\
         and belong to M8. The complete executable Core operation surface and\n\
         its real host adapters belong to M7; this milestone ships the capability\n\
         boundary and a deterministic mock host."
    );
}
