//! LCL Core 0.3 Task 01: the multi-file project model through the engine.
//!
//! Every project fixture the Core 0.3.0 package states in
//! `09_CONFORMANCE/PROJECT_FIXTURES/expected_results.json` is validated by the
//! 0.3.0 engine and compared with the package's own expectation. An accepted
//! project reports no diagnostic, is admitted and records the stated part
//! states and locales. A rejected project reports exactly its one diagnostic,
//! by identifier, stage, source and byte offset. The fixtures are read into
//! memory, so nothing is written into the package.

use lcl_protocol::{Engine, Inputs, Outcome, ProjectRecord, Report};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use lcl_spec::json::{self, Json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn canonical(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("canonical package present")
}

fn fixtures() -> PathBuf {
    canonical("0.3.0").join("09_CONFORMANCE/PROJECT_FIXTURES")
}

/// The Core 0.3.0 engine with exactly the named fixture profiles.
fn project_engine(locales: &[String]) -> Engine {
    let profiles: Vec<PathBuf> = locales
        .iter()
        .map(|locale| {
            canonical("0.3.0").join(format!(
                "09_CONFORMANCE/LOCALIZATION_FIXTURES/profiles/{locale}.json"
            ))
        })
        .collect();
    Engine::open_project(canonical("0.3.0"), &profiles).expect("the Core 0.3.0 engine opens")
}

/// Every file of one fixture, keyed by its `/`-separated relative path.
fn fixture_files(name: &str) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("fixture directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(root, &path, out);
                continue;
            }
            let key = path
                .strip_prefix(root)
                .expect("inside the fixture")
                .components()
                .map(|c| c.as_os_str().to_str().expect("UTF-8 file name"))
                .collect::<Vec<_>>()
                .join("/");
            out.insert(key, std::fs::read(&path).expect("fixture file"));
        }
    }
    let root = fixtures().join(name);
    let mut out = BTreeMap::new();
    walk(&root, &root, &mut out);
    out
}

fn validate(engine: &Engine, files: &BTreeMap<String, Vec<u8>>, entry: &str) -> Report {
    let mut provider = MemoryProvider::new();
    for (key, bytes) in files {
        provider.insert(key.clone(), bytes.clone());
    }
    let unit = SourceUnit::new(SourceId::new(entry), files[entry].clone());
    engine.validate(&unit, &provider, &Inputs::new())
}

fn expected_results() -> Json {
    let text = std::fs::read_to_string(fixtures().join("expected_results.json"))
        .expect("expected_results.json");
    json::parse(&text).expect("expected_results.json parses")
}

fn text(value: &Json) -> String {
    value.as_str().expect("a JSON string").to_string()
}

fn strings(value: Option<&Json>) -> Vec<String> {
    value
        .and_then(Json::as_array)
        .map(|items| items.iter().map(text).collect())
        .unwrap_or_default()
}

/// The package's expectation, `{error, stage, source, offset}`, as a tuple.
fn wanted_diagnostics(want: &Json) -> Vec<(String, String, String, usize)> {
    want.get("diagnostics")
        .and_then(Json::as_array)
        .unwrap_or_default()
        .iter()
        .map(|d| {
            let offset = match d.get("offset") {
                Some(Json::Number(n)) => *n as usize,
                other => panic!("offset is not a number: {other:?}"),
            };
            (
                text(d.get("error").expect("error")),
                text(d.get("stage").expect("stage")),
                text(d.get("source").expect("source")),
                offset,
            )
        })
        .collect()
}

fn reported_diagnostics(report: &Report) -> Vec<(String, String, String, usize)> {
    report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.id.clone(),
                d.stage.as_registry_str().to_string(),
                d.source.clone(),
                d.span.start,
            )
        })
        .collect()
}

/// The unit's selected locale, or `canonical` for canonical English.
fn unit_locale(report: &Report, unit: &str) -> Option<String> {
    let record = report.units.iter().find(|u| u.id == unit)?;
    Some(match &record.locale {
        Some(locale) if locale.method != "canonical" => locale.locale.clone()?,
        _ => "canonical".to_string(),
    })
}

/// Every mismatch between one fixture's report and the package's expectation.
fn mismatches(name: &str, case: &Json) -> Vec<String> {
    let entry = text(case.get("entry").expect("entry"));
    let want = case.get("expected").expect("expected");
    let engine = project_engine(&strings(want.get("profiles")));
    let report = validate(&engine, &fixture_files(name), &entry);
    let got = reported_diagnostics(&report);
    let mut problems = Vec::new();
    match text(want.get("outcome").expect("outcome")).as_str() {
        "reject" => {
            if got != wanted_diagnostics(want) {
                problems.push(format!(
                    "diagnostics {got:?}, wanted {:?}",
                    wanted_diagnostics(want)
                ));
            }
            if report.outcome != Outcome::Rejected {
                problems.push(format!("outcome {:?}", report.outcome));
            }
            if let Some(project) = &report.project {
                if project.admission != "rejected" {
                    problems.push(format!("admission {}", project.admission));
                }
            }
        }
        "accept" => {
            if !got.is_empty() || report.outcome != Outcome::Accepted {
                problems.push(format!("outcome {:?} with {got:?}", report.outcome));
            }
            let Some(project) = &report.project else {
                return vec![format!("{name}: no project record")];
            };
            if project.admission != "admitted" {
                problems.push(format!("admission {}", project.admission));
            }
            let parts: Vec<Vec<String>> = project
                .parts
                .iter()
                .map(|p| {
                    let required = if p.required { "required" } else { "optional" };
                    vec![
                        p.source.clone(),
                        p.kind.clone(),
                        required.to_string(),
                        p.state.clone(),
                    ]
                })
                .collect();
            let wanted: Vec<Vec<String>> = want
                .get("parts")
                .and_then(Json::as_array)
                .unwrap_or_default()
                .iter()
                .map(|p| strings(Some(p)))
                .collect();
            if parts != wanted {
                problems.push(format!("parts {parts:?}, wanted {wanted:?}"));
            }
            for (unit, locale) in want
                .get("locales")
                .and_then(Json::as_object)
                .unwrap_or_default()
            {
                let got = unit_locale(&report, unit);
                if got.as_deref() != Some(text(locale).as_str()) {
                    problems.push(format!("{unit} locale {got:?}, wanted {}", text(locale)));
                }
            }
        }
        other => problems.push(format!("unknown expected outcome {other}")),
    }
    problems
        .into_iter()
        .map(|p| format!("{name}: {p}"))
        .collect()
}

/// Rules 1 to 9 of `05_SEMANTICS/13` over the package's own fixtures: the
/// valid projects (`valid_*`) are admitted, and every rejecting fixture names
/// its single diagnostic exactly.
#[test]
fn every_canonical_project_fixture_behaves_as_the_package_states() {
    let expected = expected_results();
    let cases = expected.as_object().expect("an object of fixtures");
    assert_eq!(cases.len(), 26, "the package states 26 project fixtures");
    let problems: Vec<String> = cases
        .iter()
        .flat_map(|(name, case)| mismatches(name, case))
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

fn project_of(report: &Report) -> &ProjectRecord {
    report.project.as_ref().expect("a project record")
}

fn file_statuses(project: &ProjectRecord) -> Vec<(String, String)> {
    std::iter::once((project.entry.clone(), project.entry_status.clone()))
        .chain(
            project
                .parts
                .iter()
                .map(|p| (p.source.clone(), p.status.clone())),
        )
        .collect()
}

fn check_fixture(name: &str) -> Report {
    validate(&project_engine(&[]), &fixture_files(name), "main.lcl")
}

/// The usage contract's project readiness: the workspace shows each file's
/// state exactly, from the engine. A missing required part is `missing`, not a
/// defect of the entry that lists it, and the project is rejected.
#[test]
fn a_missing_required_part_is_missing_and_blocks_the_project() {
    let report = check_fixture("missing_required_part");
    let project = project_of(&report);
    assert_eq!(project.admission, "rejected");
    assert!(!project.complete);
    assert_eq!(
        file_statuses(project),
        [
            ("main.lcl".to_string(), "ready".to_string()),
            ("description.lcl".to_string(), "missing".to_string()),
            ("task.lcl".to_string(), "ready".to_string()),
        ]
    );
}

/// An admitted project: every file is `ready`, in PART order, and the report
/// says so only because the whole project was admitted.
#[test]
fn an_admitted_project_has_every_file_ready_in_part_order() {
    let report = check_fixture("valid_all_roles");
    let project = project_of(&report);
    assert_eq!(project.admission, "admitted");
    assert!(project.complete);
    let statuses = file_statuses(project);
    assert_eq!(statuses.len(), 9);
    assert!(
        statuses.iter().all(|(_, status)| status == "ready"),
        "{statuses:?}"
    );
    assert_eq!(
        project.order,
        [
            "main.lcl",
            "description.lcl",
            "definitions.lcl",
            "data.lcl",
            "context.lcl",
            "rules.lcl",
            "output.lcl",
            "checks.lcl",
            "task/build.lcl"
        ]
    );
}

/// A file with a defect of its own is `invalid`; the others stay `ready`.
#[test]
fn a_role_violation_marks_only_its_own_file_invalid() {
    let report = check_fixture("role_violation");
    let project = project_of(&report);
    assert_eq!(project.admission, "rejected");
    let statuses = file_statuses(project);
    assert!(
        statuses.contains(&("rules.lcl".to_string(), "invalid".to_string())),
        "{statuses:?}"
    );
    assert_eq!(
        statuses.iter().filter(|(_, s)| s == "invalid").count(),
        1,
        "{statuses:?}"
    );
}

/// The second PART naming the same source is its own `duplicate` row.
#[test]
fn a_duplicate_part_is_its_own_row() {
    let report = check_fixture("part_duplicate");
    let project = project_of(&report);
    let states: Vec<&str> = project.parts.iter().map(|p| p.status.as_str()).collect();
    assert!(states.contains(&"duplicate"), "{states:?}");
    assert_eq!(project.entry_status, "ready");
    assert_eq!(project.admission, "rejected");
}

/// The same project, validated twice by independently opened engines,
/// produces byte-identical machine output.
#[test]
fn project_reports_are_byte_identical_across_engines() {
    for name in [
        "valid_all_roles",
        "missing_required_part",
        "reference_cycle_across_parts",
    ] {
        let first = check_fixture(name).to_json().pretty();
        let second = check_fixture(name).to_json().pretty();
        assert_eq!(first, second, "{name}");
        assert!(first.contains("\"project\""), "{name}");
    }
}

/// Every unit of a Core 0.3.0 project, canonical or localized, records the
/// language version it was judged under: 0.3.0.
#[test]
fn a_localized_project_records_the_0_3_0_language_version() {
    let engine = project_engine(&["lv-LV".to_string()]);
    let report = validate(&engine, &fixture_files("valid_localized_part"), "main.lcl");
    assert_eq!(
        report.outcome,
        Outcome::Accepted,
        "{:?}",
        report.diagnostics
    );
    assert_eq!(report.units.len(), 2);
    for unit in &report.units {
        let locale = unit.locale.as_ref().expect("a locale record");
        assert_eq!(locale.lcl_version, "0.3.0", "{}", unit.id);
    }
}
