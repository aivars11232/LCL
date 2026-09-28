//! C03-AUDIT-02: every `qualified_identifier(DOMAIN)` field obeys its domain's
//! canonical contract, `field_signatures_vX.json#/qualified_identifier_domains`.
//!
//! `04_GRAMMAR/13`: "DOMAIN resolves through the exact source and JSON Pointer
//! in qualified_identifier_domains and, when defined_kind is present, also
//! admits an ID whose DEFINE.KIND is that exact registered kind."
//!
//! * A static domain (no `defined_kind`: definition kinds, encodings, modes,
//!   document and part kinds) admits its registered members only. Anything
//!   else is `error.field.type` ("A field value does not match its exact value
//!   kind") at the grammar_or_schema stage, as Task 01 already did for
//!   `SPECIFICATION.KIND` and `PART.KIND` (see `document_kinds.rs`).
//! * A domain with `defined_kind` (formats, errors, events, statuses, terminal
//!   non-success statuses) also admits the ID of a DEFINE of that kind. Alias
//!   IDs are ordinary user IDs and "must obey reserved-prefix … rules"
//!   (`07_VERSIONING_AND_EXTENSIONS/03`), so a value in a reserved namespace
//!   (`format.`, `event.`, `status.`, `error.` …) that is not a registered
//!   member can never be one: `error.field.type` at the grammar stage. A user
//!   ID is resolved like an alias `BASE`: no declaration is
//!   `error.reference.unresolved`, a declaration of another kind — or a status
//!   alias whose canonical status the domain's filter excludes — is
//!   `error.reference.kind`, both at the resolution stage.
//!
//! The contract is the same in Core 0.1.0, 0.2.0 and 0.3.0, so every case runs
//! in every version.

use lcl_diagnostics::Stage;
use lcl_protocol::{Engine, Report};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use lcl_spec::json::Json;
use std::path::{Path, PathBuf};

const VERSIONS: [&str; 3] = ["0.1.0", "0.2.0", "0.3.0"];

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

fn registry(version: &str, name: &str) -> Json {
    let path = canonical(version).join(format!("10_REGISTRIES/{name}_v{version}.json"));
    lcl_spec::json::parse(&std::fs::read_to_string(path).expect("registry")).expect("JSON")
}

/// The members of one enum group, or the keys of one object, of a registry.
fn members(version: &str, registry_name: &str, pointer: &[&str]) -> Vec<String> {
    let mut node = registry(version, registry_name);
    for key in pointer {
        node = node.get(key).expect("pointer resolves").clone();
    }
    if let Some(items) = node.as_array() {
        items
            .iter()
            .map(|m| m.as_str().unwrap().to_string())
            .collect()
    } else {
        node.as_object()
            .expect("an object")
            .iter()
            .map(|(k, _)| k.clone())
            .collect()
    }
}

fn example(version: &str, name: &str) -> String {
    std::fs::read_to_string(canonical("0.1.0").join("08_EXAMPLES/VALID").join(name))
        .expect("the example")
        .replacen("VERSION: \"0.1.0\"", &format!("VERSION: \"{version}\""), 1)
}

/// The canonical minimal task at `version`, with `edit` applied.
fn minimal(version: &str, edit: impl Fn(String) -> String) -> String {
    edit(example(version, "01_MINIMAL_TASK.lcl"))
}

/// A top-level block inserted before the minimal task's `TASK`.
fn with_block(source: String, block: &str) -> String {
    source.replacen("TASK:\n", &format!("{block}\nTASK:\n"), 1)
}

fn check(engine: &Engine, source: &str) -> Report {
    let unit = SourceUnit::new(SourceId::new("main.lcl"), source.as_bytes().to_vec());
    engine.check(&unit, &MemoryProvider::new())
}

/// The domain-relevant diagnostics: identifier, stage and start byte.
fn domain_diagnostics(report: &Report) -> Vec<(String, Stage, usize)> {
    report
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.id.as_str(),
                "error.field.type" | "error.reference.unresolved" | "error.reference.kind"
            )
        })
        .map(|d| (d.id.clone(), d.stage, d.span.start))
        .collect()
}

/// Exactly one domain diagnostic, `id` at `stage`, on `value` where the
/// field `field` uses it (`FIELD: value`), not where a DEFINE declares it.
fn assert_one(
    report: &Report,
    source: &str,
    field: &str,
    value: &str,
    id: &str,
    stage: Stage,
    context: &str,
) {
    let site = format!("{field}: {value}");
    let at = source.find(&site).expect("the use site is in the source") + field.len() + 2;
    assert_eq!(
        domain_diagnostics(report),
        [(id.to_string(), stage, at)],
        "{context}\n{:?}",
        report.diagnostics
    );
}

fn assert_none(report: &Report, context: &str) {
    assert!(
        domain_diagnostics(report).is_empty(),
        "{context}: {:?}",
        report.diagnostics
    );
}

// ------------------------------------------------------------- static domains

#[test]
fn every_registered_mode_encoding_format_and_definition_kind_is_accepted() {
    for version in VERSIONS {
        let engine = engine(version);
        for mode in members(
            version,
            "built_in_groups_and_results",
            &["enum_groups", "modes"],
        ) {
            let source = minimal(version, |s| {
                s.replacen(
                    "    REFERENCE: REF(task.double)\n",
                    &format!("    REFERENCE: REF(task.double)\n    MODE: {mode}\n"),
                    1,
                )
            });
            assert_none(&check(&engine, &source), &format!("{version} {mode}"));
        }
        for encoding in members(version, "formats_encodings_units", &["encodings"]) {
            let source = minimal(version, |s| {
                s.replacen(
                    "    FORMAT: format.plain_text\n",
                    &format!("    FORMAT: format.plain_text\n    ENCODING: {encoding}\n"),
                    1,
                )
            });
            assert_none(&check(&engine, &source), &format!("{version} {encoding}"));
        }
        for format in members(version, "formats_encodings_units", &["formats"]) {
            let source = minimal(version, |s| s.replacen("format.plain_text", &format, 1));
            assert_none(&check(&engine, &source), &format!("{version} {format}"));
        }
        for kind in members(
            version,
            "built_in_groups_and_results",
            &["enum_groups", "definition_kinds"],
        ) {
            let source = minimal(version, |s| {
                with_block(
                    s,
                    &format!(
                        "DEFINE:\n    ID: shop.thing\n    KIND: {kind}\n    MEANING: \"A thing\"\n"
                    ),
                )
            });
            let report = check(&engine, &source);
            let at = source.find(&kind).unwrap();
            assert!(
                !report
                    .diagnostics
                    .iter()
                    .any(|d| d.id == "error.field.type" && d.span.start == at),
                "{version} {kind}: {:?}",
                report.diagnostics
            );
        }
    }
}

#[test]
fn an_unregistered_mode_encoding_or_definition_kind_is_a_field_type_error() {
    for version in VERSIONS {
        let engine = engine(version);
        let cases = [
            (
                "MODE",
                "mode.sideways",
                minimal(version, |s| {
                    s.replacen(
                        "    REFERENCE: REF(task.double)\n",
                        "    REFERENCE: REF(task.double)\n    MODE: mode.sideways\n",
                        1,
                    )
                }),
            ),
            (
                "ENCODING",
                "encoding.ebcdic",
                minimal(version, |s| {
                    s.replacen(
                        "    FORMAT: format.plain_text\n",
                        "    FORMAT: format.plain_text\n    ENCODING: encoding.ebcdic\n",
                        1,
                    )
                }),
            ),
            (
                "KIND",
                "kind.gadget",
                minimal(version, |s| {
                    with_block(
                        s,
                        "DEFINE:\n    ID: shop.thing\n    KIND: kind.gadget\n    MEANING: \"A thing\"\n",
                    )
                }),
            ),
            (
                "KIND",
                "kind.gadget",
                minimal(version, |s| {
                    s.replacen("KIND: kind.task", "KIND: kind.gadget", 1)
                }),
            ),
        ];
        for (field, value, source) in cases {
            assert_one(
                &check(&engine, &source),
                &source,
                field,
                value,
                "error.field.type",
                Stage::GrammarOrSchema,
                &format!("{version} {value}"),
            );
        }
    }
}

// ------------------------------------------------ domains that admit DEFINEs

/// Example 08 (a HANDLER on `event.host_constraint`) at `version`.
fn handler_example(version: &str, event: &str, define: Option<&str>) -> String {
    let mut source = example(version, "08_AUTHORITY_OVERRIDE_HANDLER_AND_RETRY.lcl").replacen(
        "EVENT: event.host_constraint",
        &format!("EVENT: {event}"),
        1,
    );
    if let Some(define) = define {
        source = source.replacen("HANDLER:\n", &format!("{define}\nHANDLER:\n"), 1);
    }
    source
}

const FORMAT_ALIAS: &str =
    "DEFINE:\n    ID: shop.table\n    KIND: kind.format\n    MEANING: \"Shop tables\"\n    BASE: format.csv\n";
const EVENT_ALIAS: &str =
    "DEFINE:\n    ID: shop.host_trouble\n    KIND: kind.event\n    MEANING: \"Host trouble\"\n    BASE: event.host_constraint\n";
const ERROR_ALIAS: &str =
    "DEFINE:\n    ID: shop.wrong_value\n    KIND: kind.error\n    MEANING: \"Wrong value\"\n    BASE: error.required.missing\n";

fn failure(status: &str) -> String {
    format!(
        "FAILURE:\n    ID: failure.value\n    WHEN: REF(output.value) != 8\n    STATUS: {status}\n"
    )
}

fn status_alias(id: &str, base: &str) -> String {
    format!("DEFINE:\n    ID: {id}\n    KIND: kind.status\n    MEANING: \"A status\"\n    BASE: {base}\n")
}

#[test]
fn a_defined_alias_is_accepted_where_its_kind_is_admitted() {
    for version in VERSIONS {
        let engine = engine(version);
        // FORMAT through a kind.format DEFINE: the whole source is accepted.
        let source = minimal(version, |s| {
            with_block(
                s.replacen("format.plain_text", "shop.table", 1),
                FORMAT_ALIAS,
            )
        });
        let report = check(&engine, &source);
        assert!(
            report.diagnostics.is_empty(),
            "{version}: {:?}",
            report.diagnostics
        );

        // HANDLER.EVENT through a kind.event DEFINE.
        let source = handler_example(version, "shop.host_trouble", Some(EVENT_ALIAS));
        assert_none(&check(&engine, &source), &format!("{version} event alias"));

        // VERIFY.ERROR through a kind.error DEFINE.
        let source = minimal(version, |s| {
            with_block(
                s.replacen(
                    "    ASSERT: REF(output.value) == 8\n\nSUCCESS:",
                    "    ASSERT: REF(output.value) == 8\n    ERROR: shop.wrong_value\n\nSUCCESS:",
                    1,
                ),
                ERROR_ALIAS,
            )
        });
        assert_none(&check(&engine, &source), &format!("{version} error alias"));

        // FAILURE.STATUS through a kind.status DEFINE whose canonical status,
        // reached through a second alias, is terminal and non-success.
        let source = minimal(version, |s| {
            let s = with_block(s, &status_alias("shop.gave_up", "status.failed"));
            let s = with_block(s, &status_alias("shop.gave_up_again", "shop.gave_up"));
            with_block(s, &failure("shop.gave_up_again"))
        });
        assert_none(&check(&engine, &source), &format!("{version} status alias"));
    }
}

#[test]
fn an_unregistered_value_in_a_reserved_namespace_is_a_field_type_error() {
    for version in VERSIONS {
        let engine = engine(version);
        let cases = [
            (
                "FORMAT",
                "format.not_registered",
                minimal(version, |s| {
                    s.replacen("format.plain_text", "format.not_registered", 1)
                }),
            ),
            // A registered member of another domain is not in this one.
            (
                "FORMAT",
                "event.missing",
                minimal(version, |s| {
                    s.replacen("format.plain_text", "event.missing", 1)
                }),
            ),
            (
                "EVENT",
                "event.not_registered",
                handler_example(version, "event.not_registered", None),
            ),
            (
                "ERROR",
                "error.not_registered",
                minimal(version, |s| {
                    s.replacen(
                        "    ASSERT: REF(output.value) == 8\n\nSUCCESS:",
                        "    ASSERT: REF(output.value) == 8\n    ERROR: error.not_registered\n\nSUCCESS:",
                        1,
                    )
                }),
            ),
            // FAILURE.STATUS admits terminal statuses other than succeeded.
            (
                "STATUS",
                "status.running",
                minimal(version, |s| with_block(s, &failure("status.running"))),
            ),
            (
                "STATUS",
                "status.succeeded",
                minimal(version, |s| with_block(s, &failure("status.succeeded"))),
            ),
            (
                "STATUS",
                "status.not_registered",
                minimal(version, |s| {
                    with_block(s, &failure("status.not_registered"))
                }),
            ),
        ];
        for (field, value, source) in cases {
            assert_one(
                &check(&engine, &source),
                &source,
                field,
                value,
                "error.field.type",
                Stage::GrammarOrSchema,
                &format!("{version} {value}"),
            );
        }
    }
}

#[test]
fn an_alias_that_resolves_to_nothing_is_unresolved() {
    for version in VERSIONS {
        let engine = engine(version);
        let source = minimal(version, |s| {
            s.replacen("format.plain_text", "shop.missing", 1)
        });
        assert_one(
            &check(&engine, &source),
            &source,
            "FORMAT",
            "shop.missing",
            "error.reference.unresolved",
            Stage::Resolution,
            &format!("{version} FORMAT"),
        );
        let source = handler_example(version, "shop.nothing", None);
        assert_one(
            &check(&engine, &source),
            &source,
            "EVENT",
            "shop.nothing",
            "error.reference.unresolved",
            Stage::Resolution,
            &format!("{version} EVENT"),
        );
    }
}

#[test]
fn an_alias_of_the_wrong_kind_or_domain_is_a_reference_kind_error() {
    for version in VERSIONS {
        let engine = engine(version);
        let cases = [
            // A kind.event DEFINE used as a format.
            (
                "FORMAT",
                "shop.host_trouble",
                minimal(version, |s| {
                    with_block(
                        s.replacen("format.plain_text", "shop.host_trouble", 1),
                        EVENT_ALIAS,
                    )
                }),
            ),
            // A declaration that is not a DEFINE at all.
            (
                "FORMAT",
                "input.value",
                minimal(version, |s| {
                    s.replacen("FORMAT: format.plain_text", "FORMAT: input.value", 1)
                }),
            ),
            // A kind.format DEFINE used as an event.
            (
                "EVENT",
                "shop.table",
                handler_example(version, "shop.table", Some(FORMAT_ALIAS)),
            ),
            // A status alias whose canonical status is not terminal
            // non-success: the domain's filter applies after resolution.
            (
                "STATUS",
                "shop.done",
                minimal(version, |s| {
                    let s = with_block(s, &status_alias("shop.done", "status.succeeded"));
                    with_block(s, &failure("shop.done"))
                }),
            ),
        ];
        for (field, value, source) in cases {
            assert_one(
                &check(&engine, &source),
                &source,
                field,
                value,
                "error.reference.kind",
                Stage::Resolution,
                &format!("{version} {value}"),
            );
        }
    }
}
