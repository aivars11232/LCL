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
use lcl_stdlib::{HostAdapter, MemoryFileSystem};
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

/// The in-memory filesystem a filesystem-shaped witness runs against.
///
/// The same fixture `tests/decision_witnesses.rs` installs, so this report and
/// that gate execute the same probes against the same declared content rather
/// than two populations a reader has to reconcile.
fn filesystem_host() -> HostAdapter {
    let fs = MemoryFileSystem::new()
        .with_read_scope("/case")
        .with_scope("/case")
        .with_file("/case/a.txt", *b"abcd")
        .with_file("/case/lines.txt", *b"a\nb");
    let grants = fs.grants().clone();
    HostAdapter::new(grants).with_filesystem(fs)
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
        index.witnesses().iter().map(|w| w.id.clone()),
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
                    // Every probe runs here, including the filesystem-backed
                    // ones. They used to be recorded as descriptive entries
                    // with a note saying the gate ran them elsewhere, which was
                    // true and misleading at once: a reader comparing the
                    // executed count against the sixty-six indexed witnesses
                    // saw five behaviours apparently unexhibited, and the
                    // descriptive column counted probes rather than witnesses.
                    // The fixture is deterministic and contract-faithful, so
                    // there is no reason for this report to be the one place
                    // that cannot use it.
                    let executed = if probe.needs_filesystem {
                        let mut host = filesystem_host();
                        runner.execute_on(
                            &id,
                            &contract,
                            &probe.source,
                            probe.expectation.clone(),
                            &mut host,
                        )
                    } else {
                        runner.execute(&id, &contract, &probe.source, probe.expectation.clone())
                    };
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
