//! Machine output, and the equivalence that makes it worth having.
//!
//! Two acceptance criteria live here. "CLI and direct engine API produce
//! equivalent diagnostics/results for the same source/inputs" is tested by
//! running both over one document and comparing the records byte for byte —
//! not field by field, which would only compare the fields someone thought to
//! check. And "machine output includes exact source identity, byte spans,
//! diagnostic IDs/stages/statuses and execution/evidence records" is tested by
//! reading each of those back out of the emitted JSON with the trust root's own
//! strict reader.

mod common;

use common::{canonical_root, example, invalid_example, lcl_in, project, scratch, write};
use lcl_project::FileProvider;
use lcl_protocol::{Engine, Inputs};
use lcl_spec::json::{self, Json};

fn engine() -> Engine {
    Engine::open(canonical_root()).expect("the approved package assembles")
}

fn parse(text: &str) -> Json {
    json::parse(text).expect("machine output is valid JSON")
}

/// The CLI's record and the facade's record are the same bytes.
///
/// The comparison is deliberately whole-document. Comparing selected fields
/// would prove only that the fields someone remembered to compare agree.
#[test]
fn the_cli_and_the_engine_agree_on_a_valid_document() {
    let root = project("machine_agree", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let from_cli = lcl_in(&root, &["check", "--machine", "main.lcl"], &[]);
    assert_eq!(from_cli.code, 0, "{}{}", from_cli.stdout, from_cli.stderr);

    let provider = FileProvider::new(&root).expect("the project opens");
    let unit = provider.root_unit(root.join("main.lcl")).expect("loads");
    let direct = engine().check(&unit, &provider).to_json().pretty();

    assert_eq!(from_cli.stdout, direct);
}

/// The same, for a document that is rejected.
///
/// A diagnostic is where a second implementation would show first, so the
/// rejection path is compared as strictly as the accepting one.
#[test]
fn the_cli_and_the_engine_agree_on_a_rejected_document() {
    for name in [
        "02_TAB_INDENTATION.invalid.lcl",
        "06_UNRESOLVED_REFERENCE.invalid.lcl",
        "07_TYPE_MISMATCH.invalid.lcl",
        "08_HARD_CONFLICT.invalid.lcl",
    ] {
        let root = project(
            &format!("machine_agree_{}", name.replace('.', "_")),
            "main.lcl",
            &invalid_example(name),
        );
        let from_cli = lcl_in(&root, &["validate", "--machine", "main.lcl"], &[]);

        let provider = FileProvider::new(&root).expect("opens");
        let unit = provider.root_unit(root.join("main.lcl")).expect("loads");
        let direct = engine()
            .validate(&unit, &provider, &Inputs::new())
            .to_json()
            .pretty();

        assert_eq!(from_cli.stdout, direct, "{name}");
    }
}

/// The same, for a run that reaches completion.
#[test]
fn the_cli_and_the_engine_agree_on_a_run() {
    let root = project("machine_agree_run", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let from_cli = lcl_in(&root, &["run", "--machine", "main.lcl"], &[]);
    assert_eq!(from_cli.code, 0, "{}{}", from_cli.stdout, from_cli.stderr);

    let engine = engine();
    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider.root_unit(root.join("main.lcl")).expect("loads");
    // The same host the CLI builds when no capability was granted: the engine's
    // internal stores, the in-language verifier, and nothing else.
    let mut stdlib = engine
        .stdlib()
        .expect("assembles")
        .with_profiles(lcl_stdlib::checking_profiles())
        .with_grants(lcl_capabilities::Grants::internal());
    let mut host = lcl_stdlib::HostAdapter::new(lcl_capabilities::Grants::internal());
    let direct = engine
        .run(&unit, &provider, &Inputs::new(), &mut stdlib, &mut host)
        .to_json()
        .pretty();

    assert_eq!(from_cli.stdout, direct);
}

/// The same, with a supplied input on both sides.
#[test]
fn the_cli_and_the_engine_agree_on_supplied_inputs() {
    let source = example("01_MINIMAL_TASK.lcl").replace(
        "    TYPE: INTEGER\n    VALUE: 4\n",
        "    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 4\n",
    );
    let root = project("machine_agree_input", "main.lcl", &source);
    let from_cli = lcl_in(
        &root,
        &["validate", "--machine", "--input", "input.value=7", "main.lcl"],
        &[],
    );

    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider.root_unit(root.join("main.lcl")).expect("loads");
    let direct = engine()
        .validate(
            &unit,
            &provider,
            &Inputs::new().with_text("input.value", "7"),
        )
        .to_json()
        .pretty();

    assert_eq!(from_cli.stdout, direct);
}

/// Machine output carries every part the criterion names.
#[test]
fn machine_output_carries_identity_spans_ids_stages_and_statuses() {
    let root = project(
        "machine_fields",
        "main.lcl",
        &invalid_example("02_TAB_INDENTATION.invalid.lcl"),
    );
    let run = lcl_in(&root, &["check", "--machine", "main.lcl"], &[]);
    let value = parse(&run.stdout);

    assert_eq!(
        value.get("protocol").and_then(Json::as_str),
        Some("lcl.engine/1")
    );

    // Exact source identity: the root-relative name, and the digest of the
    // exact bytes that were read.
    let units = value.get("units").and_then(Json::as_array).expect("units");
    let root_unit = units
        .iter()
        .find(|u| u.get("root").and_then(Json::as_bool) == Some(true))
        .expect("a root unit");
    assert_eq!(root_unit.get("id").and_then(Json::as_str), Some("main.lcl"));
    let digest = root_unit
        .get("digest")
        .and_then(Json::as_str)
        .expect("a digest");
    assert_eq!(
        digest,
        format!(
            "sha256:{}",
            lcl_spec::sha256::hex_digest(invalid_example("02_TAB_INDENTATION.invalid.lcl").as_bytes())
        )
    );

    // Diagnostic identity, stage, status, and the byte span it was found at.
    let diagnostics = value
        .get("diagnostics")
        .and_then(Json::as_array)
        .expect("diagnostics");
    let primary = diagnostics
        .iter()
        .find(|d| d.get("primary").and_then(Json::as_bool) == Some(true))
        .expect("a primary diagnostic");
    assert!(primary
        .get("id")
        .and_then(Json::as_str)
        .is_some_and(|id| id.starts_with("error.")));
    assert_eq!(
        primary.get("stage").and_then(Json::as_str),
        Some("lexical"),
        "the registered stage name"
    );
    assert!(primary
        .get("default_status")
        .and_then(Json::as_str)
        .is_some_and(|s| s.starts_with("status.")));
    let span = primary.get("span").expect("a span");
    let start = span.get("start").and_then(Json::as_u64).expect("a start");
    let end = span.get("end").and_then(Json::as_u64).expect("an end");
    assert!(end >= start);
    let position = primary.get("position").expect("a position");
    assert!(position.get("line").is_some());
    assert!(position.get("column").is_some());
    assert_eq!(
        position.get("offset").and_then(Json::as_u64),
        Some(start),
        "the derived position points at the same byte the span starts at"
    );
    assert!(primary.get("specificity_rank").is_some());
    assert!(primary.get("meaning").and_then(Json::as_str).is_some());
}

/// A run's machine output carries the execution and completion records.
#[test]
fn machine_output_carries_execution_and_evidence_records() {
    let root = project(
        "machine_records",
        "main.lcl",
        &example("09_EXPLICIT_ASSUMPTION_AND_EVIDENCE.lcl"),
    );
    let run = lcl_in(&root, &["run", "--machine", "main.lcl"], &[]);
    let value = parse(&run.stdout);

    let execution = value.get("execution").expect("an execution record");
    assert!(execution
        .get("invocations")
        .and_then(Json::as_array)
        .is_some_and(|i| !i.is_empty()));
    assert!(execution.get("steps").and_then(Json::as_u64).is_some());

    let completion = value.get("completion").expect("a completion record");
    assert!(completion
        .get("terminal_status")
        .and_then(Json::as_str)
        .is_some_and(|s| s.starts_with("status.")));
    assert!(completion.get("checks").and_then(Json::as_array).is_some());
    assert!(completion
        .get("evidence")
        .and_then(Json::as_array)
        .is_some_and(|e| !e.is_empty()));
    assert!(completion.get("outputs").and_then(Json::as_array).is_some());
    assert!(completion.get("verdict").is_some());
    assert!(completion.get("reason").and_then(Json::as_str).is_some());
}

/// `inspect` carries the structural record and no execution.
#[test]
fn inspect_machine_output_carries_the_structure() {
    let root = scratch("machine_inspect");
    write(root.join("src/main.lcl"), example("03_IMPORTING_TASK.lcl"));
    write(
        root.join("src/02_IMPORT_LIBRARY.lcl"),
        example("02_IMPORT_LIBRARY.lcl"),
    );
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"entry\": \"src/main.lcl\"\n}}\n",
            canonical_root().display().to_string()
        ),
    );

    let run = lcl_in(&root, &["inspect", "--machine"], &[]);
    let value = parse(&run.stdout);

    let structure = value.get("structure").expect("a structure record");
    let imports = structure
        .get("imports")
        .and_then(Json::as_array)
        .expect("imports");
    assert_eq!(imports.len(), 1);
    assert_eq!(
        imports[0].get("outcome").and_then(Json::as_str),
        Some("loaded")
    );
    assert!(structure
        .get("declarations")
        .and_then(Json::as_array)
        .is_some_and(|d| !d.is_empty()));
    assert!(structure
        .get("plan")
        .and_then(Json::as_array)
        .is_some_and(|p| !p.is_empty()));
    assert!(
        value.get("execution").is_none(),
        "inspect performs no effect and produces no execution record"
    );

    let units = value.get("units").and_then(Json::as_array).expect("units");
    assert_eq!(units.len(), 2, "the root and its import");
}

/// Two runs of the same project produce identical bytes.
#[test]
fn machine_output_is_reproducible() {
    let root = project(
        "machine_reproducible",
        "main.lcl",
        &example("05_CONDITION_AND_ITERATION.lcl"),
    );
    let first = lcl_in(&root, &["run", "--machine", "main.lcl"], &[]);
    let second = lcl_in(&root, &["run", "--machine", "main.lcl"], &[]);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.code, second.code);
}

/// Human and machine output report the same verdict.
#[test]
fn the_two_renderings_report_the_same_verdict() {
    let root = project(
        "machine_same_verdict",
        "main.lcl",
        &invalid_example("05_DUPLICATE_ID.invalid.lcl"),
    );
    let human = lcl_in(&root, &["check", "main.lcl"], &[]);
    let machine = lcl_in(&root, &["check", "--machine", "main.lcl"], &[]);

    assert_eq!(human.code, machine.code);
    let value = parse(&machine.stdout);
    let id = value
        .get("diagnostics")
        .and_then(Json::as_array)
        .and_then(|d| d.first())
        .and_then(|d| d.get("id"))
        .and_then(Json::as_str)
        .expect("an identifier");
    assert!(
        human.stdout.contains(id),
        "the human rendering names the same identifier: {}",
        human.stdout
    );
}

/// Every machine-emitting command produces JSON the strict reader accepts.
#[test]
fn every_machine_command_emits_readable_json() {
    let root = project(
        "machine_all_commands",
        "main.lcl",
        &example("01_MINIMAL_TASK.lcl"),
    );
    for args in [
        vec!["check", "--machine", "main.lcl"],
        vec!["validate", "--machine", "main.lcl"],
        vec!["inspect", "--machine", "main.lcl"],
        vec!["run", "--machine", "main.lcl"],
        vec!["spec", "--machine"],
        vec!["syntax", "--machine"],
    ] {
        let run = lcl_in(&root, &args, &[]);
        assert!(
            json::parse(&run.stdout).is_ok(),
            "{args:?} emits readable JSON: {}",
            run.stdout
        );
    }
}

/// The syntax metadata is the registry's own data.
#[test]
fn syntax_metadata_matches_the_lexicon() {
    let root = project("machine_syntax", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["syntax", "--machine"], &[]);
    let value = parse(&run.stdout);

    let lexicon = lcl_lexer::Lexicon::load(engine().spec()).expect("the lexicon loads");
    let count = |key: &str| {
        value
            .get(key)
            .and_then(Json::as_array)
            .map(|a| a.len())
            .unwrap_or_default()
    };
    assert_eq!(count("reserved_words"), lexicon.reserved_words().count());
    assert_eq!(count("callables"), lexicon.callables().count());
    assert_eq!(count("adopted_symbols"), lexicon.adopted_symbols().count());
    assert_eq!(count("excluded_lexemes"), lexicon.excluded_lexemes().count());
    assert_eq!(
        value.get("language_version").and_then(Json::as_str),
        Some("0.1.0")
    );
    assert_eq!(
        value.get("media_type").and_then(Json::as_str),
        Some("text/x-lcl")
    );
}
