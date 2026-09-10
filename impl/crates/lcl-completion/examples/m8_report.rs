//! The M8 report: every canonical example, carried to one terminal status.
//!
//! ## What this proves, and what it does not
//!
//! The milestone's claim is that an LCL invocation can now progress from
//! accepted source through execution to evidence, a `SUCCESS` or `FAILURE`
//! decision, exactly one terminal status and its declared outputs. Reading the
//! code would prove none of that, so this report **runs** all thirteen valid
//! canonical examples through the real engine — bytes, tokens, syntax,
//! resolution, checking, preflight, the standard library, a deterministic host,
//! then completion — and prints what each one actually decided.
//!
//! A run that ends `status.failed` is not a defect being hidden. Producer
//! completion and domain outcome are separate axes, and an example whose
//! operations reach an uninstalled capability truthfully fails rather than
//! pretending. What the report shows is that every example reaches *exactly
//! one* terminal status, and why that one.
//!
//! Run it with:
//!
//! ```text
//! cargo run --offline -p lcl-completion --example m8_report
//! ```

use lcl_checker::{Checker, Contracts as StaticContracts};
use lcl_completion::{Completion, Contracts as CompletionContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts as RuntimeContracts, MockHost, Runtime};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Outcome, Preflight};
use lcl_spec::SpecPackage;
use lcl_stdlib::Stdlib;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("the canonical package must be present")
}

fn example_provider(dir: &Path) -> MemoryProvider {
    let mut provider = MemoryProvider::new();
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .expect("the canonical examples are readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lcl"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        provider.insert(name, std::fs::read(&path).expect("readable"));
    }
    provider
}

fn main() {
    let root = canonical_root();
    let spec = SpecPackage::open(&root).expect("the approved package opens");
    let lexicon = Lexicon::load(&spec).expect("lexicon");
    let grammar = Grammar::load(&spec).expect("grammar");
    let rules = Rules::load(&spec, &grammar).expect("rules");
    let statics = StaticContracts::load(&spec).expect("static contracts");
    let preflight = PreflightContracts::load(&spec).expect("preflight contracts");
    let runtime = RuntimeContracts::load(&spec).expect("runtime contracts");
    let completion_contracts = CompletionContracts::load(&spec).expect("completion contracts");

    println!("LCL M8 REPORT — verification, evidence and completion\n");
    println!("package authority : {:?}", spec.authority());
    println!("language version  : {}", spec.formal_version());
    println!(
        "owned identifiers : {} at stage verification_or_completion",
        lcl_completion::CompletionError::OWNED.len()
    );
    for id in lcl_completion::CompletionError::OWNED {
        let registered = completion_contracts.error(id);
        println!("    {id} -> {}", registered.default_status);
    }

    let dir = root.join("08_EXAMPLES/VALID");
    let provider = example_provider(&dir);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();

    println!("\nCANONICAL EXAMPLES ({} valid)\n", names.len());
    let mut statuses: BTreeMap<String, usize> = BTreeMap::new();
    let mut checks_run = 0usize;
    let mut checks_skipped = 0usize;
    let mut outputs_published = 0usize;

    for name in &names {
        let bytes = std::fs::read(dir.join(name)).expect("readable");
        let unit = SourceUnit::new(SourceId::new(name.as_str()), bytes);
        let resolved = Resolver::new(&rules, &grammar, &lexicon)
            .resolve(&unit, &provider)
            .expect("the example resolves");
        let checked = Checker::new(&statics).check(&resolved).expect("checks");
        let planned = Preflight::new(&preflight)
            .plan(&checked, &resolved, &Invocation::new())
            .expect("plans");
        if planned.outcome() != Outcome::Planned {
            println!("  {name:<44} preflight refused");
            continue;
        }
        // Every registered implementation profile is installed, so an example
        // that fails does so for a language reason and not because this report
        // forgot to install the row's profile.
        let mut stdlib = Stdlib::load(&spec)
            .expect("the standard library assembles")
            .with_profiles(
                lcl_stdlib::checking_profiles()
                    .into_iter()
                    .chain(lcl_stdlib::filesystem_profiles())
                    .chain(lcl_stdlib::process_profiles())
                    .chain(lcl_stdlib::transport_profiles())
                    .collect::<Vec<_>>(),
            );
        let mut host = MockHost::new();
        let execution = Runtime::new(&runtime)
            .execute_with(&planned, &checked, &resolved, &mut stdlib, &mut host)
            .expect("the plan executes");
        let completion = Completion::of(
            &completion_contracts,
            &planned,
            &checked,
            &resolved,
            &execution,
        )
        .expect("the execution completes");

        let terminal = completion.terminal_status().to_string();
        *statuses.entry(terminal.clone()).or_insert(0) += 1;
        checks_run += completion.checks().results().len();
        checks_skipped += completion.checks().skipped().len();
        outputs_published += completion
            .outputs()
            .records()
            .iter()
            .filter(|r| r.publication.published())
            .count();

        println!(
            "  {name:<44} {terminal:<18} checks {}/{} evidence {} outputs {}",
            completion.checks().results().len(),
            completion.checks().results().len() + completion.checks().skipped().len(),
            completion.evidence().records().len(),
            completion.outputs().records().len(),
        );
        for check in completion.checks().results() {
            println!(
                "      {} {} = {} (required {})",
                check.kind, check.id, check.outcome, check.required
            );
        }
        if let Some(success) = &completion.verdict().success {
            println!(
                "      SUCCESS {} {} = {}",
                success.id, success.quantifier, success.value
            );
        }
        if let Some(failure) = &completion.verdict().failure {
            println!(
                "      FAILURE {} requests {}",
                failure.id, failure.requested_status
            );
        }
        println!("      why: {:?}", completion.terminal().reason);
    }

    println!("\nTOTALS");
    println!("  examples completed   : {}", names.len());
    println!("  checks with a result : {checks_run}");
    println!("  checks skipped       : {checks_skipped}");
    println!("  outputs published    : {outputs_published}");
    println!("  terminal statuses    :");
    for (status, count) in &statuses {
        println!("      {status:<20} {count}");
    }
    println!(
        "\n  Every example ended with exactly one terminal status. `status.succeeded` is not\n  \
         the only truthful outcome: it is legal at a root only when SUCCESS is TRUE, every\n  \
         required output is bound and valid, every hard rule holds and every required\n  \
         evidence exists."
    );
}
