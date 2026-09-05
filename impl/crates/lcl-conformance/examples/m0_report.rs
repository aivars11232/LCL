//! Prints the M0 foundation report for a canonical LCL package.
//!
//! Usage: cargo run --offline -p lcl-conformance --example m0_report [PATH]
//!
//! Read-only. Exits non-zero if the package fails to verify.

use lcl_conformance::ConformanceIndex;
use lcl_diagnostics::DiagnosticRegistry;
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../canonical/LCL_Core_0.1.0")
}

fn main() -> ExitCode {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(default_root);

    let pkg = match SpecPackage::open(&root) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("FAIL: {e}");
            if let lcl_spec::SpecError::IntegrityFailed(r) = &e {
                for d in &r.defects {
                    eprintln!("  {d}");
                }
            }
            return ExitCode::FAILURE;
        }
    };

    let r = pkg.integrity();
    println!("LCL M0 FOUNDATION REPORT");
    println!("========================\n");
    println!("Package root      : {}", r.package_root.display());
    println!(
        "Formal version    : {} (pinned {})",
        r.formal_version,
        lcl_spec::PINNED_FORMAL_VERSION
    );
    println!("Package status    : {}", r.status);
    println!("Release ready     : {}", r.release_ready);
    println!("Internal integrity: {}", r.summary());
    println!();
    println!("-- trust boundary --");
    println!("  authority        : {}", pkg.authority());
    println!("  approved package : {}", lcl_spec::APPROVED_PACKAGE.label);
    println!(
        "  anchor identity  : {}",
        lcl_spec::APPROVED_PACKAGE.identity_digest
    );
    println!("  actual identity  : {}", pkg.identity_digest());
    println!(
        "  anchor match     : {}",
        if pkg.identity_digest() == lcl_spec::APPROVED_PACKAGE.identity_digest {
            "yes"
        } else {
            "NO"
        }
    );
    println!();

    println!("-- component counts (declared vs observed) --");
    let declared = pkg.declared_component_counts();
    let observed = pkg.observed_component_counts();
    for (k, d) in &declared {
        match observed.get(k) {
            Some(o) => println!(
                "  {:<28} {:>5} {:>5}  {}",
                k,
                d,
                o,
                if d == o { "ok" } else { "MISMATCH" }
            ),
            None => println!("  {:<28} {:>5}     -  not a registry cardinality", k, d),
        }
    }

    let diags = match DiagnosticRegistry::load(&pkg) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("FAIL: diagnostics: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("\n-- diagnostics skeleton --");
    println!(
        "  statuses {}, errors {}",
        diags.status_count(),
        diags.error_count()
    );
    for (stage, n) in diags.stage_histogram() {
        println!("  {:<28} {:>5}", stage.to_string(), n);
    }
    println!(
        "  handler-recoverable errors   {:>5}",
        diags.recoverable_errors().len()
    );
    println!(
        "  terminal statuses            {:>5}",
        diags.terminal_statuses().len()
    );
    println!("  selection algorithm          NOT IMPLEMENTED (contract exposed as data)");

    let conf = match ConformanceIndex::load(&pkg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FAIL: conformance: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("\n-- conformance skeleton --");
    println!(
        "  descriptive requirements     {:>5}",
        conf.requirement_count()
    );
    println!("  decision witnesses           {:>5}", conf.witness_count());
    println!(
        "  categories                   {:>5}",
        conf.declared_category_counts().len()
    );
    println!("  executed                     {:>5}", 0);
    println!("\n  {}", conf.claim_blocked_reason());

    println!("\n-- scope boundary --");
    println!("  No lexer, parser, evaluator, or runtime is present at M0.");
    println!("  No conformance level is claimed.");

    ExitCode::SUCCESS
}
