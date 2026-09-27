//! LCL Core 0.3 Task 01, owner decision 4 (2026-09-26): an unregistered
//! document kind is rejected in every Core version.
//!
//! `SPECIFICATION.KIND` names a member of the closed `document_kind` domain,
//! and Core 0.3.0's `PART.KIND` a member of the closed `part_kind` domain. A
//! name outside the domain is `error.field.type` at the grammar_or_schema
//! stage, at the name itself. Before the fix every version accepted any
//! qualified identifier there. The Core 0.3.0 kinds belong to 0.3.0 alone.

use lcl_diagnostics::Stage;
use lcl_protocol::{Engine, Outcome, Report};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use std::path::{Path, PathBuf};

fn canonical(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("canonical package present")
}

fn engine(version: &str) -> Engine {
    let root = canonical(version);
    match version {
        "0.1.0" => Engine::open(root),
        "0.2.0" => Engine::open_localized(root, &[]),
        "0.3.0" => Engine::open_project(root, &[]),
        other => panic!("no engine for {other}"),
    }
    .expect("the approved package opens")
}

/// The canonical minimal task, declared at `version`, with `kind` as its KIND.
fn minimal_task(version: &str, kind: &str) -> String {
    let source =
        std::fs::read_to_string(canonical("0.1.0").join("08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"))
            .expect("the minimal example");
    source
        .replacen("VERSION: \"0.1.0\"", &format!("VERSION: \"{version}\""), 1)
        .replacen("KIND: kind.task", &format!("KIND: {kind}"), 1)
}

fn check(engine: &Engine, source: &str) -> Report {
    let unit = SourceUnit::new(SourceId::new("main.lcl"), source.as_bytes().to_vec());
    engine.check(&unit, &MemoryProvider::new())
}

/// The one diagnostic `error.field.type`, at the grammar_or_schema stage, on
/// the first occurrence of `name` in `source`.
fn assert_field_type_at(report: &Report, source: &str, name: &str, context: &str) {
    assert_eq!(report.outcome, Outcome::Rejected, "{context}");
    let found: Vec<(&str, Stage, usize)> = report
        .diagnostics
        .iter()
        .map(|d| (d.id.as_str(), d.stage, d.span.start))
        .collect();
    let at = source.find(name).expect("the name is in the source");
    assert_eq!(
        found,
        [("error.field.type", Stage::GrammarOrSchema, at)],
        "{context}"
    );
}

#[test]
fn a_registered_kind_is_still_accepted_in_every_version() {
    for version in ["0.1.0", "0.2.0", "0.3.0"] {
        let report = check(&engine(version), &minimal_task(version, "kind.task"));
        assert!(
            report.diagnostics.is_empty(),
            "{version}: {:?}",
            report.diagnostics
        );
        assert_eq!(report.outcome, Outcome::Accepted, "{version}");
    }
}

#[test]
fn an_unregistered_kind_is_a_field_type_error_in_every_version() {
    for version in ["0.1.0", "0.2.0", "0.3.0"] {
        let source = minimal_task(version, "kind.bogus");
        let report = check(&engine(version), &source);
        assert_field_type_at(&report, &source, "kind.bogus", version);
    }
}

/// `kind.project` and the part kinds are Core 0.3.0 kinds: to Core 0.1.0 and
/// 0.2.0 they are as unregistered as any other name.
#[test]
fn the_0_3_0_kinds_do_not_exist_before_0_3_0() {
    for version in ["0.1.0", "0.2.0"] {
        for kind in ["kind.project", "kind.part.task", "kind.part.rules"] {
            let source = minimal_task(version, kind);
            let report = check(&engine(version), &source);
            assert_field_type_at(&report, &source, kind, &format!("{version} {kind}"));
        }
    }
}

/// `PART.KIND` names a part kind: a registered document kind that is no part
/// kind is as wrong there as an unregistered name.
#[test]
fn a_part_kind_must_be_a_part_kind() {
    for kind in ["kind.task", "kind.project", "kind.part.bogus"] {
        let source = format!(
            "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: project.kinds\n    \
             NAME: \"Kinds\"\n    VERSION: \"1.0.0\"\n    KIND: kind.project\n\n\
             PART:\n    ID: part.one\n    SOURCE: PATH(\"one.lcl\")\n    KIND: {kind}\n\n\
             EXECUTE:\n    REFERENCE: REF(task.one)\n"
        );
        let report = check(&engine("0.3.0"), &source);
        let at = source.rfind(kind).expect("the PART KIND");
        let found: Vec<(&str, usize)> = report
            .diagnostics
            .iter()
            .map(|d| (d.id.as_str(), d.span.start))
            .collect();
        assert_eq!(found, [("error.field.type", at)], "{kind}");
    }
}
