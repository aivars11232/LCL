//! The executed conformance report.
//!
//! Prints the production conformance report: every executed population this
//! build has, recorded against the independently pinned obligations of the
//! verified approved package, and the claim that evidence supports. `--json`
//! prints the same verdict as one structured document, for independent
//! reconciliation.
//!
//! Run it with:
//!
//! ```text
//! cargo run --offline -p lcl-conformance --example m8_conformance_report [-- --json]
//! ```

use lcl_conformance::{fixtures, production};
use std::process::ExitCode;

fn main() -> ExitCode {
    let json = match std::env::args().skip(1).collect::<Vec<_>>().as_slice() {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => {
            eprintln!("usage: m8_conformance_report [--json]");
            return ExitCode::from(2);
        }
    };
    match production::report(fixtures::spec()) {
        Ok(report) if json => print!("{}", report.render_verdict_json()),
        Ok(report) => println!("{}", report.render()),
        Err(error) => {
            eprintln!("conformance report unavailable: {error}");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}
