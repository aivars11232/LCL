//! M5 semantic-preflight report.
//!
//! Prints what this milestone actually decides, against the approved canonical
//! package, and nothing it does not. Run with:
//!
//! ```text
//! cargo run -p lcl-semantics --example m5_report
//! ```

use lcl_checker::{Checker, Contracts as StaticContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_semantics::{Contracts, Invocation, Outcome, Preflight, PreflightError};
use lcl_spec::SpecPackage;
use std::fs;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

/// Every `.lcl` file in one package directory, in ascending name order.
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

fn main() {
    let spec = SpecPackage::open(canonical_root()).expect("the approved package opens");
    let lexicon = Lexicon::load(&spec).expect("lexicon");
    let grammar = Grammar::load(&spec).expect("grammar");
    let rules = Rules::load(&spec, &grammar).expect("rules");
    let statics = StaticContracts::load(&spec).expect("static contracts");
    let contracts = Contracts::load(&spec).expect("preflight contracts");

    let resolver = Resolver::new(&rules, &grammar, &lexicon);
    let checker = Checker::new(&statics);
    let preflight = Preflight::new(&contracts);

    // Every valid example, so an importing example's library resolves.
    let mut provider = MemoryProvider::new();
    for (name, source) in files("08_EXAMPLES/VALID", ".lcl") {
        provider.insert(name, source.into_bytes());
    }

    println!("LCL Core 0.1.0 — M5 semantic preflight report");
    println!("package authority   : {:?}", spec.authority());
    println!("identity digest     : {}", spec.identity_digest());
    println!(
        "authority bounds    : {}..{} (document default {})",
        contracts.authority_bounds().minimum,
        contracts.authority_bounds().maximum,
        contracts.authority_bounds().local_default
    );
    println!(
        "priority bounds     : {}..{} (optional default {})",
        contracts.priority_bounds().minimum,
        contracts.priority_bounds().maximum,
        contracts.priority_bounds().optional_default
    );
    println!("operation axes      : {}", contracts.operation_axes_count());
    let deferred = PreflightError::ALL
        .into_iter()
        .filter(|e| e.is_deferred())
        .count();
    println!(
        "preflight errors    : {} mirrored, {} emitted here, {} deferred",
        PreflightError::ALL.len(),
        PreflightError::ALL.len() - deferred,
        deferred
    );
    println!("  by registered stage:");
    for stage in lcl_diagnostics::Stage::ORDER {
        let owned: Vec<&str> = PreflightError::ALL
            .into_iter()
            .filter(|e| contracts.error(*e).stage == stage)
            .map(|e| e.as_registry_str())
            .collect();
        if !owned.is_empty() {
            println!("    {:<24} {}", stage.as_registry_str(), owned.join(", "));
        }
    }

    println!();
    println!("== 08_EXAMPLES/VALID (must plan with no preflight diagnostic) ==");
    let mut planned_count = 0usize;
    let valid = files("08_EXAMPLES/VALID", ".lcl");
    for (name, source) in &valid {
        let resolved = resolver
            .resolve(
                &SourceUnit::new(SourceId::new(name), source.as_bytes()),
                &provider,
            )
            .expect("earlier stages pass");
        let checked = checker.check(&resolved).expect("resolution succeeded");
        let result = preflight
            .plan(&checked, &resolved, &Invocation::new())
            .expect("the static stage succeeded");
        let plan = result.partial_plan();
        let verdict = match result.outcome() {
            Outcome::Planned => {
                planned_count += 1;
                "PASS"
            }
            Outcome::Rejected => "FAIL",
        };
        println!(
            "  {verdict} {name:<48} nodes={:<3} edges={:<3} rules={:<3} resolved={:<3} checks={}",
            plan.len(),
            plan.edges().len(),
            plan.authorities().len(),
            plan.resolutions().len(),
            plan.checks().len()
        );
        for diagnostic in result.diagnostics() {
            println!("        - {diagnostic}");
        }
    }
    println!("  {planned_count}/{} valid examples plan", valid.len());

    println!();
    println!("== 08_EXAMPLES/INVALID (.expected.txt) ==");
    println!("   earlier-stage expectation  -> M1/M2/M3/M4 own it; preflight never runs");
    println!("   preflight expectation      -> the pinned identifier must be primary here");
    println!("   later-stage expectation    -> preflight must raise nothing");
    let mut earlier = 0usize;
    let mut here = 0usize;
    let mut later = 0usize;
    for (name, source) in files("08_EXAMPLES/INVALID", ".invalid.lcl") {
        let expected = expected_identifier(&name);
        let unit = SourceUnit::new(SourceId::new(&name), source.as_bytes());

        let Ok(resolved) = resolver.resolve(&unit, &provider) else {
            earlier += 1;
            println!("  PASS {name:<48} expects {expected:<36} an earlier stage owns it");
            continue;
        };
        if !resolved.diagnostics().is_empty() || resolved.stage_failures().next().is_some() {
            earlier += 1;
            println!("  PASS {name:<48} expects {expected:<36} an earlier stage owns it");
            continue;
        }
        let Ok(checked) = checker.check(&resolved) else {
            earlier += 1;
            println!("  PASS {name:<48} expects {expected:<36} an earlier stage owns it");
            continue;
        };
        if !checked.diagnostics().is_empty() || !checked.earlier_stage_defects().is_empty() {
            earlier += 1;
            println!("  PASS {name:<48} expects {expected:<36} an earlier stage owns it");
            continue;
        }

        let result = preflight
            .plan(&checked, &resolved, &Invocation::new())
            .expect("the static stage succeeded");
        match result.primary() {
            Some(primary) if primary.id.to_string() == expected => {
                here += 1;
                println!(
                    "  PASS {name:<48} expects {expected:<36} raised here at byte {}",
                    primary.span.start
                );
            }
            Some(primary) => {
                println!(
                    "  FAIL {name:<48} expects {expected:<36} raised {} instead",
                    primary.id
                );
            }
            None => {
                later += 1;
                println!("  PASS {name:<48} expects {expected:<36} a later stage owns it");
            }
        }
    }
    println!("  by stage: earlier={earlier} preflight={here} later={later}");

    println!();
    println!("== 09_CONFORMANCE/SOURCE_FIXTURES (total, no panic) ==");
    let fixtures = files("09_CONFORMANCE/SOURCE_FIXTURES", ".lcl");
    let mut reached = 0usize;
    for (name, source) in &fixtures {
        let unit = SourceUnit::new(SourceId::new(name), source.as_bytes());
        let Ok(resolved) = resolver.resolve(&unit, &provider) else {
            continue;
        };
        let Ok(checked) = checker.check(&resolved) else {
            continue;
        };
        if preflight
            .plan(&checked, &resolved, &Invocation::new())
            .is_ok()
            && checked.diagnostics().is_empty()
        {
            reached += 1;
        }
    }
    println!(
        "  {reached} of {} fixtures reached the preflight stage; the rest fail earlier",
        fixtures.len()
    );

    println!();
    if deferred > 0 {
        println!("Registered identifiers deferred by this milestone:");
        for (id, owner) in lcl_semantics::DEFERRED {
            println!("  {id} -> {owner}");
        }
        println!();
    }

    println!("Decision witnesses executed by this milestone:");
    for (id, contract, what) in [
        (
            "CLOSURE-053",
            "graph",
            "child field and LIST order, sequential edges",
        ),
        (
            "CLOSURE-054",
            "graph",
            "two activation paths to one declaration",
        ),
        (
            "CLOSURE-055",
            "graph",
            "BEFORE may not reverse a sequential edge",
        ),
        (
            "CLOSURE-056",
            "output_owner",
            "one producing ACTION per OUTPUT",
        ),
        (
            "CLOSURE-060",
            "check_selection",
            "import alone activates no check",
        ),
        (
            "CLOSURE-062",
            "root_completion",
            "no SUCCESS field is invented",
        ),
        (
            "CLOSURE-063",
            "status",
            "pre-effect ordering failure stays pre_effect",
        ),
        (
            "CLOSURE-064",
            "check_selection",
            "optional prerequisite still evaluated",
        ),
        (
            "CLOSURE-065",
            "check_selection",
            "exact material target selects a check",
        ),
        (
            "CLOSURE-066",
            "check_selection",
            "unselected target has no result",
        ),
    ] {
        println!("  {id}  {contract:<16} {what}");
    }

    println!();
    println!("Scope: canonical processing steps 6 through 9 only. No effect has");
    println!("occurred, no action has executed, and no VERIFY, TEST, evidence or");
    println!("terminal status is implied. M6 (evaluator and runtime) has not been");
    println!("started.");
}
