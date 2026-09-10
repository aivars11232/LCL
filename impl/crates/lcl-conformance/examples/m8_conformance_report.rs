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

use lcl_conformance::report::Implementation;
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

    let mut report = ConformanceReport::new(
        Implementation::under_test(
            lcl_spec::APPROVED_PACKAGE.identity_digest,
            spec.formal_version().to_string(),
        ),
        index.requirement_count(),
        index.witness_count(),
    );

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
                    let id = if probe.label.is_empty() {
                        case.id.to_string()
                    } else {
                        format!("{}/{}", case.id, probe.label)
                    };
                    // The example runs the host-free probes only; the
                    // filesystem-backed ones need the test fixture host, and the
                    // gate in `tests/decision_witnesses.rs` runs those.
                    if probe.needs_filesystem {
                        report.record_descriptive(
                            format!("{id} (needs the filesystem fixture)"),
                            "executed by the semantic_case_execution gate, which installs the \
                             in-memory filesystem this probe reads",
                        );
                        continue;
                    }
                    report.record(
                        runner.execute(&id, &contract, &probe.source, probe.expectation.clone()),
                        case.coverage,
                    );
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
