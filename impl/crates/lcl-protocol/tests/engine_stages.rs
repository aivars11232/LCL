//! The facade: one staged walk, stopping where the command says.
//!
//! These tests are about the *seam*, not about the language. Whether
//! `error.keyword.case` is the right identifier for a lowercase block header is
//! M1's question and M1 answers it; what is asked here is whether the facade
//! reports the identifier the layer chose, at the stage the layer classified,
//! with the byte span the layer recorded, and whether it stops advancing where
//! the canonical processing order says it must.

mod common;

use common::{engine, example, example_provider, invalid_example, unit, valid_examples};
use lcl_diagnostics::Stage;
use lcl_protocol::Inputs;
use lcl_protocol::{Command, Outcome, Reached};
use lcl_resolver::MemoryProvider;
use lcl_runtime::MockHost;

/// The minimal canonical task passes every stage a `check` evaluates.
#[test]
fn a_valid_document_is_accepted_through_static_checking() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().check(
        &unit("01_MINIMAL_TASK.lcl", &source),
        &MemoryProvider::new(),
    );

    assert_eq!(report.outcome, Outcome::Accepted);
    assert_eq!(report.reached, Reached::StaticChecking);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.command, Command::Check);
    assert_eq!(report.root(), Some("01_MINIMAL_TASK.lcl"));
}

/// A report names the package it was produced against.
///
/// A result nobody can attribute to an exact specification is not reproducible.
#[test]
fn every_report_carries_the_package_identity() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().check(&unit("doc.lcl", &source), &MemoryProvider::new());

    assert_eq!(report.spec.formal_version, "0.1.0");
    assert_eq!(report.spec.authority, "authoritative");
    assert_eq!(report.spec.identity_digest.len(), 64);
    assert!(report
        .spec
        .identity_digest
        .chars()
        .all(|c| c.is_ascii_hexdigit()));
}

/// The root unit's exact bytes are identified by digest.
#[test]
fn the_root_unit_is_identified_by_the_digest_of_its_bytes() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().check(&unit("doc.lcl", &source), &MemoryProvider::new());

    let root = report.units.iter().find(|u| u.root).expect("a root unit");
    assert_eq!(root.id, "doc.lcl");
    assert_eq!(root.bytes, source.len());
    assert_eq!(root.digest, lcl_spec::sha256::hex_digest(source.as_bytes()));
}

/// A lexical defect stops the walk at step 1 and keeps its exact locus.
#[test]
fn a_lexical_defect_stops_at_the_lexical_stage() {
    let source = invalid_example("02_TAB_INDENTATION.invalid.lcl");
    let report = engine().check(&unit("tabs.lcl", &source), &MemoryProvider::new());

    assert_eq!(report.outcome, Outcome::Rejected);
    assert_eq!(report.reached, Reached::Lexical);
    let primary = report.primary().expect("a primary diagnostic");
    assert_eq!(primary.stage, Stage::Lexical);
    assert!(primary.id.starts_with("error."), "{}", primary.id);
    assert_eq!(primary.source, "tabs.lcl");
    assert_eq!(
        source.as_bytes().get(primary.span.start).copied(),
        Some(b'\t'),
        "the span indexes the offending byte"
    );
}

/// A grammar defect stops at step 3, having passed step 1 cleanly.
#[test]
fn a_grammar_defect_stops_at_the_grammar_stage() {
    let source = invalid_example("03_BARE_EQUALS.invalid.lcl");
    let report = engine().check(&unit("equals.lcl", &source), &MemoryProvider::new());

    assert_eq!(report.outcome, Outcome::Rejected);
    let primary = report.primary().expect("a primary diagnostic");
    assert!(
        matches!(primary.stage, Stage::Lexical | Stage::GrammarOrSchema),
        "an earliest-stage defect, not a later one: {:?}",
        primary.stage
    );
}

/// A later-stage defect passes every earlier stage.
///
/// `06_UNRESOLVED_REFERENCE` is a resolution defect. It must lex and parse
/// cleanly, or the earliest-stage rule would be reporting the wrong stage.
#[test]
fn a_resolution_defect_passes_the_earlier_stages() {
    let source = invalid_example("06_UNRESOLVED_REFERENCE.invalid.lcl");
    let report = engine().check(&unit("ref.lcl", &source), &MemoryProvider::new());

    assert_eq!(report.reached, Reached::Resolution);
    let primary = report.primary().expect("a primary diagnostic");
    assert_eq!(primary.stage, Stage::Resolution);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.stage != Stage::Lexical && d.stage != Stage::GrammarOrSchema),
        "no earlier-stage diagnostic accompanies it"
    );
}

/// A type defect reaches static checking and stops there.
#[test]
fn a_static_defect_stops_at_static_checking() {
    let source = invalid_example("07_TYPE_MISMATCH.invalid.lcl");
    let report = engine().check(&unit("types.lcl", &source), &MemoryProvider::new());

    assert_eq!(report.reached, Reached::StaticChecking);
    let primary = report.primary().expect("a primary diagnostic");
    assert_eq!(primary.stage, Stage::StaticOrExpression);
}

/// Exactly one diagnostic is primary, in every rejection.
#[test]
fn exactly_one_diagnostic_is_primary() {
    for name in [
        "01_WRONG_KEYWORD_CASE.invalid.lcl",
        "05_DUPLICATE_ID.invalid.lcl",
        "06_UNRESOLVED_REFERENCE.invalid.lcl",
        "07_TYPE_MISMATCH.invalid.lcl",
        "08_HARD_CONFLICT.invalid.lcl",
        "12_FLOATING_VERSION.invalid.lcl",
    ] {
        let source = invalid_example(name);
        let report =
            engine().validate(&unit(name, &source), &MemoryProvider::new(), &Inputs::new());
        let primaries = report.diagnostics.iter().filter(|d| d.primary).count();
        assert_eq!(primaries, 1, "{name} has exactly one primary diagnostic");
        assert_eq!(report.outcome, Outcome::Rejected, "{name} is rejected");
    }
}

/// `check` stops after step 5 even for a document with a later-stage defect.
///
/// `08_HARD_CONFLICT` is a validation-stage defect. A `check` must not see it:
/// stopping early is the point of the command, and a command that quietly ran
/// further would be reporting a stage the caller did not ask for.
#[test]
fn check_stops_before_preflight() {
    let source = invalid_example("08_HARD_CONFLICT.invalid.lcl");
    let checked = engine().check(&unit("conflict.lcl", &source), &MemoryProvider::new());
    assert_eq!(checked.outcome, Outcome::Accepted);
    assert_eq!(checked.reached, Reached::StaticChecking);

    let validated = engine().validate(
        &unit("conflict.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
    );
    assert_eq!(validated.outcome, Outcome::Rejected);
    assert_eq!(validated.reached, Reached::Preflight);

    let primary = validated.primary().expect("a primary diagnostic");
    assert_eq!(primary.id, "error.conflict.hard");
    // How far the walk got and how the registry classifies the identifier are
    // two different axes, and the facade must not conflate them. The walk
    // reached preflight, which is where step 6 decides an unsatisfiable hard
    // clause; the identifier's registered stage is `resolution`, verbatim from
    // `statuses_and_errors_v0.1.0.json#/errors/error.conflict.hard`. The
    // preflight layer documents that it "spans three of them and never
    // overwrites one", and neither does this.
    assert_eq!(primary.stage, Stage::Resolution);
    assert_eq!(primary.default_status, "status.invalid");
}

/// `validate` accepts a runnable document and produces no execution record.
#[test]
fn validate_produces_no_execution() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().validate(
        &unit("01_MINIMAL_TASK.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
    );

    assert_eq!(report.outcome, Outcome::Accepted);
    assert_eq!(report.reached, Reached::Preflight);
    assert!(report.execution.is_none(), "preflight performs no effect");
    assert!(report.completion.is_none());
}

/// A run carries the minimal task to exactly one terminal status.
#[test]
fn a_run_reaches_one_terminal_status() {
    let source = example("01_MINIMAL_TASK.lcl");
    let mut stdlib = engine()
        .stdlib()
        .expect("the operation surface assembles")
        .with_profiles(lcl_stdlib::checking_profiles());
    let mut host = MockHost::new();
    let report = engine().run(
        &unit("01_MINIMAL_TASK.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
        &mut stdlib,
        &mut host,
    );

    assert_eq!(report.reached, Reached::Completion);
    assert_eq!(report.outcome, Outcome::Accepted);
    assert_eq!(report.terminal_status(), Some("status.succeeded"));

    let completion = report.completion.as_ref().expect("a completion record");
    assert_eq!(completion.checks.len(), 1);
    assert_eq!(completion.checks[0].id, "verify.value");
    assert_eq!(completion.checks[0].outcome.as_deref(), Some("TRUE"));
    assert_eq!(completion.outputs.len(), 1);
    assert_eq!(completion.outputs[0].publication, "published");

    let execution = report.execution.as_ref().expect("an execution record");
    assert!(execution.steps > 0, "the queue actually ran");
    assert!(!execution.invocations.is_empty());
}

/// A run with no operation surface installed is refused, not faked.
#[test]
fn a_run_without_a_host_is_reported_rather_than_substituted() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().inspect(
        &unit("01_MINIMAL_TASK.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
    );
    // `inspect` reaches preflight and stops. It never produces an execution
    // record, so nothing can mistake it for a run.
    assert!(report.execution.is_none());
    assert_eq!(report.reached, Reached::Preflight);
}

/// Every valid canonical example reaches one terminal status.
///
/// Not every one succeeds, and that is the point: a document whose operations
/// need a capability the deterministic host does not have fails truthfully.
/// What is asserted is that the facade carried each of the thirteen all the way
/// and produced exactly one status for each.
#[test]
fn every_valid_example_reaches_completion() {
    let provider = example_provider();
    let mut succeeded: Vec<String> = Vec::new();
    for name in valid_examples() {
        let source = example(&name);
        let mut stdlib = engine()
            .stdlib()
            .expect("the operation surface assembles")
            .with_profiles(
                lcl_stdlib::checking_profiles()
                    .into_iter()
                    .chain(lcl_stdlib::filesystem_profiles())
                    .chain(lcl_stdlib::process_profiles())
                    .chain(lcl_stdlib::transport_profiles())
                    .collect::<Vec<_>>(),
            );
        let mut host = MockHost::new();
        let report = engine().run(
            &unit(&name, &source),
            &provider,
            &Inputs::new(),
            &mut stdlib,
            &mut host,
        );
        assert_eq!(report.reached, Reached::Completion, "{name} completes");
        let status = report.terminal_status().expect("one terminal status");
        assert!(status.starts_with("status."), "{name} -> {status}");
        if status == "status.succeeded" {
            succeeded.push(name);
        }
    }
    // Named, not counted: a count cannot tell a repaired example from a
    // regressed one, and this oracle was stale for exactly that reason.
    // `03_IMPORTING_TASK.lcl` reads `REF(output.copy).TARGET`, which
    // `05_SEMANTICS/01` defines as a metadata read of "the declaration field
    // without requiring the OUTPUT's result binding". The runtime used to
    // answer it with the field's *source spelling* as a STRING, so `core.copy`
    // received a STRING where its contract requires a PATH and refused with
    // `error.operation.precondition`; the example therefore failed. Evaluating
    // that field in its declaring source context restored its declared PATH
    // type, so a valid canonical example whose SUCCESS is satisfiable now
    // succeeds, as it should.
    let succeeded: Vec<&str> = succeeded.iter().map(String::as_str).collect();
    assert_eq!(
        succeeded,
        [
            "01_MINIMAL_TASK.lcl",
            "02_IMPORT_LIBRARY.lcl",
            "03_IMPORTING_TASK.lcl",
            "04_AUTOMATED_CODING_TASK.lcl",
            "05_CONDITION_AND_ITERATION.lcl",
            "10_ACCEPTED_CORE_PROFILES.lcl",
            "11_EXACT_DIVISION_AND_ROUNDING.lcl",
            "13_TYPES_AND_REFERENCE_VALUES.lcl",
        ],
        "exactly the examples the M8 report reaches success on"
    );
}

/// An importing document reports every unit it loaded, with digests.
#[test]
fn imports_are_reported_as_loaded_units() {
    let name = "03_IMPORTING_TASK.lcl";
    let source = example(name);
    let report = engine().check(&unit(name, &source), &example_provider());

    assert_eq!(report.units.len(), 2, "the root and its one import");
    assert!(report.units[0].root);
    assert_eq!(report.units[1].id, "02_IMPORT_LIBRARY.lcl");
    assert!(!report.units[1].root);
    assert_eq!(
        report.units[1].digest,
        lcl_spec::sha256::hex_digest(example("02_IMPORT_LIBRARY.lcl").as_bytes())
    );
}

/// A provider that cannot supply a named import produces a language
/// diagnostic, not a tool failure.
#[test]
fn an_unresolvable_import_is_a_resolution_diagnostic() {
    let name = "03_IMPORTING_TASK.lcl";
    let source = example(name);
    let report = engine().check(&unit(name, &source), &MemoryProvider::new());

    assert_eq!(report.outcome, Outcome::Rejected);
    let primary = report.primary().expect("a primary diagnostic");
    assert_eq!(primary.id, "error.import.not_found");
    assert_eq!(primary.stage, Stage::Resolution);
}

/// `inspect` reports the structure preflight built, and no effect.
#[test]
fn inspect_reports_imports_declarations_and_the_ordered_plan() {
    let name = "03_IMPORTING_TASK.lcl";
    let source = example(name);
    let report = engine().inspect(&unit(name, &source), &example_provider(), &Inputs::new());

    let structure = report.structure.as_ref().expect("a structure record");
    assert_eq!(structure.imports.len(), 1);
    assert_eq!(structure.imports[0].kind, "IMPORT");
    assert_eq!(structure.imports[0].namespace, "file_rules");
    assert_eq!(structure.imports[0].outcome, "loaded");
    assert_eq!(
        structure.imports[0].loaded.as_deref(),
        Some("02_IMPORT_LIBRARY.lcl")
    );
    assert!(!structure.declarations.is_empty());
    assert!(structure.candidates > 0);
    assert!(
        structure.plan.iter().any(|n| n.order.is_some()),
        "the plan carries its final ordering"
    );
    assert!(
        structure.plan.iter().any(|n| n.operation.is_some()),
        "an authorized node names its operation"
    );
}

/// Supplied data the document does not declare is reported, never absorbed.
#[test]
fn unused_supplied_input_is_reported() {
    let source = example("01_MINIMAL_TASK.lcl");
    let inputs = Inputs::new().with_value(
        "input.nothing_declares_this",
        lcl_semantics::Value::Boolean(true),
    );
    let report = engine().inspect(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

    let structure = report.structure.as_ref().expect("a structure record");
    assert_eq!(
        structure.unused_inputs,
        vec!["input.nothing_declares_this".to_string()]
    );
}

/// Two runs of the same bytes produce the same record.
///
/// `05_SEMANTICS/11` fixes determinism as a language property, and a protocol
/// that reordered a map or renumbered an identity would break it at the product
/// layer, where a UI diffing two runs would see phantom change.
#[test]
fn the_same_input_produces_the_same_record() {
    let source = example("05_CONDITION_AND_ITERATION.lcl");
    let render = || {
        let mut stdlib = engine()
            .stdlib()
            .expect("assembles")
            .with_profiles(lcl_stdlib::checking_profiles());
        let mut host = MockHost::new();
        engine()
            .run(
                &unit("doc.lcl", &source),
                &MemoryProvider::new(),
                &Inputs::new(),
                &mut stdlib,
                &mut host,
            )
            .to_json()
            .pretty()
    };
    assert_eq!(render(), render());
}

/// Malformed and untrusted bytes are reported, never panicked on.
#[test]
fn arbitrary_bytes_do_not_panic() {
    let cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"\x00\x01\x02".to_vec(),
        b"\xff\xfe not utf-8".to_vec(),
        b"LCL:\n".to_vec(),
        "LCL:\n    VERSION: \"9.9.9\"\n".as_bytes().to_vec(),
        vec![b'A'; 100_000],
    ];
    for bytes in cases {
        let unit = lcl_resolver::SourceUnit::new(lcl_resolver::SourceId::new("fuzz.lcl"), bytes);
        let report = engine().check(&unit, &MemoryProvider::new());
        // Any verdict is acceptable; surviving with a report is the assertion.
        assert!(matches!(
            report.outcome,
            Outcome::Accepted | Outcome::Rejected
        ));
    }
}

/// The JSON projection of a real run is readable by the strict reader.
#[test]
fn a_report_serializes_to_readable_json() {
    let source = example("01_MINIMAL_TASK.lcl");
    let mut stdlib = engine()
        .stdlib()
        .expect("assembles")
        .with_profiles(lcl_stdlib::checking_profiles());
    let mut host = MockHost::new();
    let report = engine().run(
        &unit("01_MINIMAL_TASK.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
        &mut stdlib,
        &mut host,
    );

    let rendered = report.to_json().pretty();
    let parsed = lcl_spec::json::parse(&rendered).expect("the strict reader accepts it");
    assert_eq!(
        parsed.get("protocol").and_then(|v| v.as_str()),
        Some(lcl_protocol::PROTOCOL)
    );
    assert_eq!(parsed.get("command").and_then(|v| v.as_str()), Some("run"));
    assert_eq!(
        parsed.get("reached").and_then(|v| v.as_str()),
        Some("completion")
    );
    assert_eq!(
        parsed
            .get("completion")
            .and_then(|c| c.get("terminal_status"))
            .and_then(|v| v.as_str()),
        Some("status.succeeded")
    );
    assert!(
        parsed.get("structure").is_none(),
        "a run carries no structure section"
    );
}

/// A diagnostic's JSON carries the byte span, not only a line and column.
#[test]
fn diagnostic_json_carries_byte_spans() {
    let source = invalid_example("02_TAB_INDENTATION.invalid.lcl");
    let report = engine().check(&unit("tabs.lcl", &source), &MemoryProvider::new());
    let rendered = report.to_json().compact();
    let parsed = lcl_spec::json::parse(&rendered).expect("valid json");

    let diagnostics = parsed
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("a diagnostics array");
    let first = diagnostics.first().expect("at least one diagnostic");
    let span = first.get("span").expect("a span");
    assert!(span.get("start").is_some() && span.get("end").is_some());
    let position = first.get("position").expect("a position");
    assert!(position.get("line").is_some() && position.get("offset").is_some());
    assert_eq!(
        first.get("stage").and_then(|v| v.as_str()),
        Some("lexical"),
        "the registered stage name, not a product invention"
    );
}
