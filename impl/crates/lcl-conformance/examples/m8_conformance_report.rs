//! The executed conformance report.
//!
//! Prints what `semantic_case_execution` actually ran: every decision witness
//! this build executes, the exact source-and-observation evidence behind each,
//! every witness that stays descriptive and why, and the conformance level the
//! evidence supports.
//!
//! Run it with:
//!
//! ```text
//! cargo run --offline -p lcl-conformance --example m8_conformance_report
//! ```

use lcl_conformance::{ConformanceIndex, ConformanceReport, Runner};
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};

#[path = "../tests/witness_cases/mod.rs"]
mod witness_cases;

use witness_cases::Plan;

fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("the canonical package must be present")
}

fn main() {
    let spec = SpecPackage::open(canonical_root()).expect("the approved package opens");
    let index = ConformanceIndex::load(&spec).expect("the catalogs load");
    let runner = Runner::new(&spec).expect("the engine assembles");

    let mut report = ConformanceReport::for_spec(&spec).expect("complete verified inventory");

    for case in lcl_conformance::source_cases::cases(&spec).expect("complete concrete source cases") {
        let coverage = if ["source/fixture/", "source/keyword/", "source/symbol/"].iter()
            .any(|prefix| case.id.starts_with(prefix)) {
            lcl_conformance::Coverage::Lexical
        } else { lcl_conformance::Coverage::Grammar };
        report.record(case.execute(&runner), coverage);
    }

    for case in witness_cases::cases() {
        let contract = index
            .witnesses()
            .iter()
            .find(|w| w.id == case.id)
            .map(|w| w.contract.clone())
            .unwrap_or_default();
        match &case.plan {
            Plan::Executable(probes) | Plan::NotImplemented { probes, .. } => {
                for probe in probes {
                    let executed = probe.execute(&runner, case.id, &contract);
                    report.record(executed, case.coverage);
                }
            }
            Plan::Descriptive { reason } => report.record_descriptive(case.id, *reason),
        }
        if let Plan::NotImplemented { missing, owner, .. } = &case.plan {
            report.record_descriptive(
                format!("{} (not implemented)", case.id),
                format!("{missing}. Owner: {owner}."),
            );
        }
    }

    println!("{}", report.render());
}
