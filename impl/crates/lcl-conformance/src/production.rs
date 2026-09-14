//! The production conformance report.
//!
//! One entry, shared by the report command and its tests: every executed
//! population this build has, recorded against the independently pinned
//! obligations of the verified approved package. No caller selects the
//! populations, so a report cannot be assembled from a convenient subset.

use crate::report::{ConformanceReport, Coverage};
use crate::witness_cases::Plan;
use crate::{
    operation_cases, result_cases, semantic_cases, source_cases, witness_cases, ConformanceIndex,
    Runner,
};
use lcl_spec::SpecPackage;

/// Execute every population and record it against the complete verified
/// inventory.
pub fn report(spec: &SpecPackage) -> Result<ConformanceReport, String> {
    let index = ConformanceIndex::load(spec).map_err(|error| error.to_string())?;
    let runner = Runner::new(spec).map_err(|error| error.to_string())?;
    let mut report = ConformanceReport::for_spec(spec)?;

    for case in source_cases::cases(spec)? {
        let coverage = if ["source/fixture/", "source/keyword/", "source/symbol/"]
            .iter()
            .any(|prefix| case.id.starts_with(prefix))
        {
            Coverage::Lexical
        } else {
            Coverage::Grammar
        };
        report.record(case.execute(&runner), coverage);
    }

    for case in witness_cases::cases() {
        let contract = index
            .witnesses()
            .iter()
            .find(|witness| witness.id == case.id)
            .map(|witness| witness.contract.clone())
            .ok_or_else(|| format!("{} is not in the canonical catalog", case.id))?;
        match &case.plan {
            Plan::Executable(probes) | Plan::NotImplemented { probes, .. } => {
                for probe in probes {
                    report.record(probe.execute(&runner, case.id, &contract), case.coverage);
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

    for case in semantic_cases::execute(spec, &runner) {
        let coverage = semantic_coverage(&case.id)?;
        report.record(case, coverage);
    }
    for case in operation_cases::execute(spec, &runner) {
        report.record(case, Coverage::StandardLibrary);
    }
    for case in result_cases::execute(spec, &runner) {
        report.record(case, Coverage::Runtime);
    }
    Ok(report)
}

/// The stage a semantic contract group exercises. Closed: a family without a
/// reviewed mapping is an error, never a guess.
fn semantic_coverage(id: &str) -> Result<Coverage, String> {
    match id.split('/').nth(1) {
        Some("type_valid" | "type_invalid" | "operator_invalid" | "function_invalid") => {
            Ok(Coverage::StaticOrType)
        }
        Some("operator_valid" | "function_valid" | "status_transition") => Ok(Coverage::Runtime),
        Some("error_contract") => Ok(Coverage::EndToEnd),
        _ => Err(format!("semantic group {id} has no reviewed coverage")),
    }
}
