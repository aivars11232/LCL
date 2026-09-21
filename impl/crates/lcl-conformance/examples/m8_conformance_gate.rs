//! The testing-readiness gate over the production conformance verdict.
//!
//! `m8_conformance_report` prints what the engine did and exits 0 whenever it
//! could assemble a report — including when the claim is `source_conforming`.
//! This gate reads that same verdict and fails closed unless it establishes the
//! semantic half of the readiness contract: the claim is `semantics_conforming`,
//! no required obligation failed, is missing, is invalid or is counted twice,
//! the mapping digest is the reviewed inventory's, and the package identity is
//! the approved anchor's.
//!
//! Run it with:
//!
//! ```text
//! cargo run --offline -p lcl-conformance --example m8_conformance_gate
//! ```
//!
//! Exit codes: 0 accepted, 1 refused with every reason printed, 2 the report
//! itself could not be assembled.

use lcl_conformance::obligations::Obligations;
use lcl_conformance::{acceptance, fixtures, production};
use std::process::ExitCode;

fn main() -> ExitCode {
    let report = match production::report(fixtures::spec()) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("the production report could not be assembled: {error}");
            return ExitCode::from(2);
        }
    };
    // Loaded independently of the verdict being checked: the reviewed mapping
    // under its pinned digest, refused unless the approved package verifies.
    let inventory = match Obligations::load(fixtures::spec()) {
        Ok(inventory) => inventory,
        Err(error) => {
            eprintln!("the reviewed obligation inventory could not be loaded: {error}");
            return ExitCode::from(2);
        }
    };
    let verdict = report.render_verdict_json();
    match acceptance::accept(
        &verdict,
        &inventory,
        lcl_spec::APPROVED_PACKAGE.identity_digest,
    ) {
        Ok(accepted) => {
            println!("ACCEPTED: claim {}", accepted.claim);
            for (level, required) in &accepted.levels {
                println!("  {level}: {required} required by the reviewed inventory, all satisfied");
            }
            println!("  mapping digest {}", accepted.mapping_digest);
            println!("  package identity {}", accepted.package_identity);
            ExitCode::SUCCESS
        }
        Err(refusals) => {
            eprintln!("REFUSED: the verdict does not establish testing readiness");
            for refusal in &refusals {
                eprintln!("  - {refusal}");
            }
            ExitCode::from(1)
        }
    }
}
