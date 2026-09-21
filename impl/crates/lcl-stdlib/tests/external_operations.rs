//! Phase D: process, network and human rows, and the bounds they run inside.

mod common;

use lcl_capabilities::{Bounds, Deadline, Grants, RealProcess};
use lcl_runtime::{Execution, Runtime, Value};
use lcl_stdlib::fixtures::{MemoryProcess, MemoryResponder, MemoryTransport};
use lcl_stdlib::{
    filesystem_profiles, process_profiles, transport_profiles, HostAdapter, MemoryFileSystem,
};

/// Everything this crate ships, installed at once.
fn all_profiles() -> Vec<lcl_capabilities::Profile> {
    let mut profiles = filesystem_profiles();
    profiles.extend(process_profiles());
    profiles.extend(transport_profiles());
    profiles
}

fn run_with_host(source: &str, host: &mut HostAdapter) -> Execution {
    let mut stdlib = common::stdlib().with_profiles(all_profiles());
    let fixture = common::fixture(source);
    Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            host,
        )
        .expect("the document planned")
}

// ---------------------------------------------------------------------------
// core.execute
// ---------------------------------------------------------------------------

const EXECUTE_ACTION: &str = "ID: action.execute\nOPERATION: core.execute\n\
                              TARGET: REF(data.command)";

fn command_document(command: &str) -> String {
    common::task(
        &common::data("data.command", "STRING", &format!("{command:?}")),
        &[EXECUTE_ACTION],
    )
}

#[test]
fn core_execute_reports_what_the_command_did() {
    let source = command_document("report --now");
    let mut host = HostAdapter::new(Grants::none().permit_program("report")).with_process(
        MemoryProcess::new().with_program("report", MemoryProcess::succeeded("all clear")),
    );
    let execution = run_with_host(&source, &mut host);

    let result = common::result_of(&execution, "action.execute");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("stdout"),
        Some(&Value::Text("all clear".to_string()))
    );
    assert_eq!(
        result.fields.get("mode"),
        Some(&Value::Identifier("non_graph".to_string()))
    );
    assert_eq!(
        result.observed_effects.first().map(|e| e.class),
        Some(lcl_runtime::EffectClass::Process)
    );
}

#[test]
fn a_nonzero_exit_code_is_a_domain_outcome_and_not_a_failure() {
    // "status.succeeded means the producer completed its contract; it may
    // accompany a FALSE domain outcome or a nonzero command exit_code."
    let source = command_document("report");
    let mut host = HostAdapter::new(Grants::none().permit_program("report"))
        .with_process(MemoryProcess::new().with_program("report", MemoryProcess::exited(1, "no")));
    let execution = run_with_host(&source, &mut host);

    let result = common::result_of(&execution, "action.execute");
    assert_eq!(result.status, "status.succeeded");
    assert!(result.execution_errors.is_empty());
    assert_eq!(
        result.fields.get("exit_code"),
        Some(&Value::Integer(
            lcl_checker::numeric::Decimal::parse_integer("1").unwrap()
        ))
    );
}

#[test]
fn an_ungranted_program_is_denied() {
    let source = command_document("rm -rf /");
    let mut host = HostAdapter::new(Grants::none().permit_program("report"))
        .with_process(MemoryProcess::new().with_program("rm", MemoryProcess::succeeded("")));
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.execute"),
        vec!["error.permission.denied".to_string()]
    );
}

#[test]
fn an_absent_process_capability_is_a_limitation() {
    let source = command_document("report");
    let mut host = HostAdapter::new(Grants::none().permit_program("report"));
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.execute"),
        vec!["error.host.constraint".to_string()]
    );
}

#[test]
fn a_legacy_bounded_error_without_observations_is_not_proof_of_no_effects() {
    struct LegacyBounded;
    impl lcl_capabilities::process::Process for LegacyBounded {
        fn run(
            &mut self,
            _command: &lcl_capabilities::process::Command,
            _bounds: &Bounds,
        ) -> Result<lcl_capabilities::process::Completion, lcl_capabilities::process::ProcessError>
        {
            Err(lcl_capabilities::process::ProcessError::Bounded(
                lcl_capabilities::bounds::Cancelled::new("effect observations unavailable"),
            ))
        }
    }
    let mut host =
        HostAdapter::new(Grants::none().permit_program("report")).with_process(LegacyBounded);
    let execution = run_with_host(&command_document("report"), &mut host);
    let result = common::result_of(&execution, "action.execute");
    assert_eq!(result.execution_errors, ["error.host.constraint"]);
    assert_eq!(
        result.failure_phase,
        lcl_runtime::result::FailurePhase::Indeterminate
    );
    assert_eq!(
        result.effect_state,
        lcl_runtime::result::EffectState::Indeterminate
    );
    assert!(result.observed_effects.is_empty());
    assert!(result.violations().is_empty());
}

// ---------------------------------------------------------------------------
// core.download and core.upload
// ---------------------------------------------------------------------------

#[test]
fn core_download_writes_the_retrieved_content_to_its_destination() {
    let declarations = format!(
        "{}{}",
        common::data(
            "data.endpoint",
            "URI",
            "URI(\"http://example.invalid/data.txt\")"
        ),
        common::data("data.destination", "PATH", "PATH(\"/srv/out/data.txt\")")
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    let source = common::task(&declarations, &[action]);

    let grants = Grants::none()
        .permit_write("/srv/out")
        .permit_network_host("example.invalid");
    let mut host = HostAdapter::new(grants)
        .with_filesystem(MemoryFileSystem::new().with_scope("/srv/out"))
        .with_transport(
            MemoryTransport::new()
                .with_resource("http://example.invalid/data.txt", "retrieved bytes"),
        );
    let execution = run_with_host(&source, &mut host);

    let result = common::result_of(&execution, "action.download");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    // Both sides resolved a concrete class, so both are observed.
    let classes: Vec<lcl_runtime::EffectClass> =
        result.observed_effects.iter().map(|e| e.class).collect();
    assert!(classes.contains(&lcl_runtime::EffectClass::Network));
    assert!(classes.contains(&lcl_runtime::EffectClass::Filesystem));
}

#[test]
fn an_ungranted_network_host_is_denied() {
    let declarations = format!(
        "{}{}",
        common::data(
            "data.endpoint",
            "URI",
            "URI(\"http://elsewhere.invalid/data.txt\")"
        ),
        common::data("data.destination", "PATH", "PATH(\"/srv/out/data.txt\")")
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    let source = common::task(&declarations, &[action]);

    let grants = Grants::none()
        .permit_write("/srv/out")
        .permit_network_host("example.invalid");
    let mut host = HostAdapter::new(grants)
        .with_filesystem(MemoryFileSystem::new().with_scope("/srv/out"))
        .with_transport(MemoryTransport::new());
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.download"),
        vec!["error.permission.denied".to_string()]
    );
}

#[test]
fn an_https_target_without_tls_is_a_limitation_not_a_downgrade() {
    // The shipped transport speaks no TLS. Reporting a limitation keeps the
    // meaning of the document intact; silently using cleartext would not.
    let declarations = format!(
        "{}{}",
        common::data(
            "data.endpoint",
            "URI",
            "URI(\"https://example.invalid/data.txt\")"
        ),
        common::data("data.destination", "PATH", "PATH(\"/srv/out/data.txt\")")
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    let source = common::task(&declarations, &[action]);

    let grants = Grants::none()
        .permit_write("/srv/out")
        .permit_network_host("example.invalid");
    let mut host = HostAdapter::new(grants)
        .with_filesystem(MemoryFileSystem::new().with_scope("/srv/out"))
        .with_transport(MemoryTransport::new());
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.download"),
        vec!["error.host.constraint".to_string()]
    );
}

// ---------------------------------------------------------------------------
// core.ask
// ---------------------------------------------------------------------------

#[test]
fn core_ask_returns_the_answer_a_responder_gave() {
    let action = "ID: action.ask\nOPERATION: core.ask\nTARGET: REF(data.responder)\n\
                  PARAMETER:\n    NAME: question\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"Which environment?\"\n\
                  PARAMETER:\n    NAME: expected_type\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"STRING\"";
    let source = common::task(
        &common::data("data.responder", "STRING", "\"the release owner\""),
        &[action],
    );
    let mut host = HostAdapter::new(Grants::none().permit_human())
        .with_responder(MemoryResponder::new().with_answer("Which environment?", "staging"));
    let execution = run_with_host(&source, &mut host);

    let result = common::result_of(&execution, "action.ask");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("value"),
        Some(&Value::Text("staging".to_string()))
    );
}

#[test]
fn an_absent_human_responder_is_a_limitation() {
    let action = "ID: action.ask\nOPERATION: core.ask\nTARGET: REF(data.responder)\n\
                  PARAMETER:\n    NAME: question\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"Which environment?\"\n\
                  PARAMETER:\n    NAME: expected_type\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"STRING\"";
    let source = common::task(
        &common::data("data.responder", "STRING", "\"the release owner\""),
        &[action],
    );
    let mut host = HostAdapter::new(Grants::none().permit_human());
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.ask"),
        vec!["error.host.constraint".to_string()]
    );
}

/// One `core.ask` of the installed responder, with its own expected type and
/// optional closed options.
fn ask(expected_type: &str, options: Option<&str>, answer: Option<&str>) -> Execution {
    let options = options
        .map(|options| {
            format!(
                "\nPARAMETER:\n    NAME: options\n    TYPE: LIST[STRING]\n    \
                 REQUIRED: FALSE\n    VALUE: {options}"
            )
        })
        .unwrap_or_default();
    let action = format!(
        "ID: action.ask\nOPERATION: core.ask\nTARGET: REF(data.responder)\n\
         PARAMETER:\n    NAME: question\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
         VALUE: \"Which environment?\"\n\
         PARAMETER:\n    NAME: expected_type\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
         VALUE: {expected_type:?}{options}"
    );
    let source = common::task(
        &common::data("data.responder", "STRING", "\"the release owner\""),
        &[&action],
    );
    let responder = match answer {
        Some(answer) => MemoryResponder::new().with_answer("Which environment?", answer),
        None => MemoryResponder::new(),
    };
    let mut host = HostAdapter::new(Grants::none().permit_human()).with_responder(responder);
    run_with_host(&source, &mut host)
}

#[test]
fn no_authorized_valid_answer_leaves_the_value_missing() {
    // "when no authorized valid answer is provided, the value remains MISSING
    // and error.required.missing applies": no answer at all, an answer that is
    // not compatible with expected_type, and an answer equal to no listed
    // option are the same absence.
    for (expected_type, options, answer) in [
        ("STRING", None, None),
        ("INTEGER", None, Some("staging")),
        ("STRING", Some("[\"production\"]"), Some("staging")),
    ] {
        let execution = ask(expected_type, options, answer);
        assert_eq!(
            common::errors_of(&execution, "action.ask"),
            vec!["error.required.missing".to_string()],
            "{expected_type} {options:?} {answer:?}"
        );
    }
}

#[test]
fn a_question_that_was_put_records_its_message_effect() {
    // The refusal follows the question, and "Absence of evidence never proves
    // absence of effects": the message effect began before the answer came
    // back absent.
    let execution = ask("STRING", None, None);
    let result = common::result_of(&execution, "action.ask");
    assert_eq!(result.failure_phase.as_registry_str(), "post_effect");
    assert_eq!(result.effect_state.as_registry_str(), "applied");
}

#[test]
fn an_option_incompatible_with_the_expected_type_refuses_before_the_question() {
    // "every option is compatible with expected_type" is a precondition.
    let action = "ID: action.ask\nOPERATION: core.ask\nTARGET: REF(data.responder)\n\
                  PARAMETER:\n    NAME: question\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"Which environment?\"\n\
                  PARAMETER:\n    NAME: expected_type\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"STRING\"\nPARAMETER:\n    NAME: options\n    TYPE: LIST[INTEGER]\n    \
                  REQUIRED: FALSE\n    VALUE: [1, 2]";
    let source = common::task(
        &common::data("data.responder", "STRING", "\"the release owner\""),
        &[action],
    );
    let mut host = HostAdapter::new(Grants::none().permit_human())
        .with_responder(MemoryResponder::new().with_answer("Which environment?", "staging"));
    let execution = run_with_host(&source, &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.ask"),
        vec!["error.type.mismatch".to_string()]
    );
    let result = common::result_of(&execution, "action.ask");
    assert_eq!(result.failure_phase.as_registry_str(), "pre_effect");
}

// ---------------------------------------------------------------------------
// core.read's range contract
// ---------------------------------------------------------------------------

/// `operations_v0.1.0.json#/contracts/core.read/parameters/range`.
///
/// Regression, `LCL-TASK-0020` defect 4. `core.read` returned the whole target
/// content and never applied the range, and an inverted range raised nothing.
/// CLOSURE-048, CLOSURE-049 and CLOSURE-050 could not be executed at all.
fn ranged_read(content: &str, unit: &str, start: i64, end: i64) -> Execution {
    let action = format!(
        "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)\n\
         PARAMETER:\n    NAME: range\n    TYPE: OBJECT\n    REQUIRED: TRUE\n    VALUE:\n        \
         unit: {unit:?}\n        start: {start}\n        end: {end}"
    );
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/a.txt\")"),
        &[&action],
    );
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", content.as_bytes());
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    run_with_host(&source, &mut host)
}

fn read_value(execution: &Execution) -> String {
    match common::field(execution, "action.read", "value") {
        lcl_runtime::Value::Text(text) => text.clone(),
        other => panic!("core.read returns a value, got {other}"),
    }
}

#[test]
fn a_scalar_range_selects_exactly_the_positions_it_names() {
    // The witness: "range {unit: scalar, start: 1, end: 3} on STRING abcd" is
    // "Returned STRING bc. End is exclusive and Unicode scalars are the
    // indexing unit."
    assert_eq!(read_value(&ranged_read("abcd", "scalar", 1, 3)), "bc");
    // "equal bounds produce the corresponding empty value"
    assert_eq!(read_value(&ranged_read("abcd", "scalar", 2, 2)), "");
    // "indexes Unicode scalars", not bytes: each of these is three bytes.
    assert_eq!(read_value(&ranged_read("日本語", "scalar", 1, 3)), "本語");
}

#[test]
fn a_line_range_retains_each_selected_terminator() {
    // "line requires STRING and indexes LF-terminated lines, retaining each
    // selected line terminator and any final unterminated line."
    assert_eq!(read_value(&ranged_read("a\nb", "line", 0, 1)), "a\n");
    assert_eq!(read_value(&ranged_read("a\nb", "line", 1, 2)), "b");
    assert_eq!(read_value(&ranged_read("a\nb", "line", 0, 2)), "a\nb");
    // "Empty STRING has zero lines."
    assert_eq!(read_value(&ranged_read("", "line", 0, 0)), "");
}

#[test]
fn bounds_outside_the_sequence_are_refused_and_never_clipped() {
    // "negative, inverted, or excessive bounds are never clipped."
    for (start, end) in [(2, 1), (-1, 2), (0, 9)] {
        let execution = ranged_read("abcd", "scalar", start, end);
        assert_eq!(
            common::errors_of(&execution, "action.read"),
            vec!["error.value.out_of_range".to_string()],
            "range {start}..{end} is outside 0 <= start <= end <= length"
        );
    }
}

#[test]
fn a_unit_that_does_not_index_this_representation_is_refused() {
    // "item requires LIST[T]", and a file's representation is a STRING.
    //
    // Corrected expectation, post-Task-20 finding F13. This asserted
    // `error.operation.precondition`, which is not the identifier the row
    // names. `operations_v0.1.0.json#/contracts/core.read/parameters/range`
    // says "An incompatible unit/representation or wrong key/type uses
    // error.operation.parameter", and `core.read`'s own `errors` list admits
    // it. The substitution was made because that identifier is registered
    // `static_or_expression` and the demand map does not re-stage it, but
    // `diagnostic_selection.expression_demand_resolution.exclusion_rule` is
    // explicit that "Discovery time alone never changes classification": a
    // source-stage identifier found at execution keeps its registered stage,
    // which is not the same as being forbidden to appear.
    //
    // So the expectation is corrected to the row's identifier rather than
    // weakened, and the stage it is classified at is asserted too.
    let execution = ranged_read("abcd", "item", 0, 1);
    assert_eq!(
        common::errors_of(&execution, "action.read"),
        vec!["error.operation.parameter".to_string()]
    );

    // Whichever stage discovers it, the classification is the registry's.
    let diagnostic = execution
        .diagnostics()
        .iter()
        .find(|d| d.id == lcl_runtime::RuntimeError::OperationParameter)
        .expect("the diagnostic was emitted");
    assert_eq!(
        diagnostic.registered_stage.as_registry_str(),
        "static_or_expression",
        "the registered stage is never overwritten"
    );
    assert_eq!(
        diagnostic.resolved_stage, None,
        "this identifier is not in the demand map, so no stage is resolved for it"
    );
    assert_eq!(
        diagnostic.default_status, "status.invalid",
        "the registered default status travels with the identifier"
    );

    // A refused range reads nothing and changes nothing. `05_SEMANTICS/09`
    // makes the pre-effect phase mean an "empty observed_effects list", and a
    // parameter defect is decided before the target is touched.
    for invocation in execution.invocations() {
        let effects = invocation
            .result
            .as_ref()
            .map(|r| r.observed_effects.as_slice())
            .unwrap_or_default();
        assert!(
            effects.is_empty(),
            "{} left an effect behind for a parameter defect: {effects:?}",
            invocation.block
        );
    }
}

#[test]
fn a_read_without_a_range_is_unchanged() {
    let action = "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)";
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/a.txt\")"),
        &[action],
    );
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", *b"abcd");
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    assert_eq!(read_value(&run_with_host(&source, &mut host)), "abcd");
}

// ---------------------------------------------------------------------------
// Bounds
// ---------------------------------------------------------------------------

#[test]
fn a_read_past_its_byte_cap_is_a_limitation() {
    let action = "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)";
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/big.txt\")"),
        &[action],
    );
    let filesystem = MemoryFileSystem::new()
        .with_scope("/srv/data")
        .with_file("/srv/data/big.txt", "0123456789");
    let mut host = HostAdapter::new(Grants::none().permit_read("/srv/data"))
        .with_filesystem(filesystem)
        .with_bounds(Bounds::new().with_max_bytes(4));
    let execution = run_with_host(&source, &mut host);

    assert_eq!(
        common::errors_of(&execution, "action.read"),
        vec!["error.host.constraint".to_string()],
        "a bound is a host limitation and never changes LCL meaning"
    );
}

#[test]
fn the_real_process_adapter_runs_a_program_and_reports_its_output() {
    // Runtime evidence for the real adapter. `true` and `false` are the two
    // programs every POSIX host has, and their only contract is their exit code.
    if !std::path::Path::new("/bin/true").exists() {
        return;
    }
    let source = command_document("/bin/true");
    let grants = Grants::none().permit_program("/bin/true");
    let mut host = HostAdapter::new(grants.clone()).with_process(RealProcess::new(grants));
    let execution = run_with_host(&source, &mut host);

    let result = common::result_of(&execution, "action.execute");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("exit_code"),
        Some(&Value::Integer(
            lcl_checker::numeric::Decimal::parse_integer("0").unwrap()
        ))
    );
}

#[test]
fn the_real_process_adapter_stops_a_program_at_its_declared_timeout() {
    use lcl_capabilities::process::{Command, Completion, Process, ProcessError, ProcessFailure};
    use std::sync::{Arc, Mutex};
    struct CaptureBounds {
        real: RealProcess,
        seen: Arc<Mutex<Option<Bounds>>>,
    }
    impl Process for CaptureBounds {
        fn run(&mut self, command: &Command, bounds: &Bounds) -> Result<Completion, ProcessError> {
            self.run_observed(command, bounds)
                .map_err(|failure| failure.error)
        }
        fn run_observed(
            &mut self,
            command: &Command,
            bounds: &Bounds,
        ) -> Result<Completion, ProcessFailure> {
            *self.seen.lock().unwrap() = Some(bounds.clone());
            self.real.run_observed(command, bounds)
        }
    }
    // The declared DURATION is an explicit input, so enforcing it changes no
    // language meaning: the outcome is `error.host.constraint`.
    if !std::path::Path::new("/bin/sleep").exists() {
        return;
    }
    let source = common::task(
        &common::data("data.command", "STRING", "\"/bin/sleep 30\""),
        &[
            "ID: action.execute\nOPERATION: core.execute\nTARGET: REF(data.command)\n\
           PARAMETER:\n    NAME: timeout\n    TYPE: DURATION\n    REQUIRED: FALSE\n    \
           VALUE: DURATION(1, unit.second)",
        ],
    );
    let grants = Grants::none().permit_program("/bin/sleep");
    let seen = Arc::new(Mutex::new(None));
    let mut host = HostAdapter::new(grants.clone())
        .with_process(CaptureBounds {
            real: RealProcess::new(grants),
            seen: seen.clone(),
        })
        .with_bounds(Bounds::new().with_deadline(Some(Deadline::from_nanos(200_000_000))));
    let execution = run_with_host(&source, &mut host);

    assert_eq!(
        common::errors_of(&execution, "action.execute"),
        vec!["error.host.constraint".to_string()]
    );
    assert_eq!(
        host.requests()[0]
            .parameters
            .get("timeout")
            .map(ToString::to_string),
        // 03_TYPES_AND_VALUES/07: one second is exactly 10^9 nanoseconds.
        // The runtime's existing order_profile::DURATION_UNIT is its private
        // normalized representation, not a new source-level unit.
        Some("1000000000 duration.normalized_nanoseconds".to_string()),
        "the declared timeout must preserve its exact magnitude: {:?}",
        host.requests()
    );
    assert_eq!(
        seen.lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .deadline
            .map(|deadline| deadline.nanos()),
        Some(1_000_000_000),
        "the actual process must receive the declared second, not the fallback 200 ms"
    );
    let result = common::result_of(&execution, "action.execute");
    assert_eq!(
        result.failure_phase,
        lcl_runtime::result::FailurePhase::PostEffect
    );
    assert_eq!(
        result.effect_state,
        lcl_runtime::result::EffectState::Indeterminate
    );
    assert_eq!(result.fields.get("started"), Some(&Value::Boolean(true)));
    assert_eq!(result.fields.get("completed"), Some(&Value::Boolean(false)));
    assert_eq!(result.observed_effects.len(), 1);
    assert!(result.violations().is_empty(), "{:?}", result.violations());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn a_real_process_timeout_retains_partial_stdout_and_effect_uncertainty() {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    struct Script(std::path::PathBuf);
    impl Drop for Script {
        fn drop(&mut self) {
            std::fs::remove_file(&self.0).expect("remove owned process fixture");
        }
    }
    let path = std::env::temp_dir().join(format!("lcl-partial-command-{}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    let script = Script(path);
    file.write_all(b"#!/bin/sh\nprintf 'observed-prefix'\nexec /bin/sleep 3\n")
        .unwrap();
    file.set_permissions(std::fs::Permissions::from_mode(0o700))
        .unwrap();
    drop(file);
    let program = script.0.to_str().unwrap();
    let source = command_document(program);
    let grants = Grants::none().permit_program(program);
    let mut host = HostAdapter::new(grants.clone())
        .with_process(RealProcess::new(grants))
        .with_bounds(Bounds::new().with_deadline(Some(Deadline::from_nanos(200_000_000))));
    let execution = run_with_host(&source, &mut host);
    let result = common::result_of(&execution, "action.execute");
    assert_eq!(result.execution_errors, ["error.host.constraint"]);
    assert_eq!(
        result.failure_phase,
        lcl_runtime::result::FailurePhase::PostEffect
    );
    assert_eq!(
        result.effect_state,
        lcl_runtime::result::EffectState::Indeterminate
    );
    assert_eq!(
        result.fields.get("stdout"),
        Some(&Value::Text("observed-prefix".into()))
    );
    assert_eq!(result.fields.get("started"), Some(&Value::Boolean(true)));
    assert_eq!(result.fields.get("completed"), Some(&Value::Boolean(false)));
    assert_eq!(result.observed_effects.len(), 1);
    assert!(result.violations().is_empty(), "{:?}", result.violations());
}

#[test]
fn the_real_transport_retrieves_content_over_a_loopback_socket() {
    // Runtime evidence for the real transport: an actual TCP listener, an
    // actual HTTP/1.1 exchange, and the retrieved bytes read back off the disk
    // rather than out of the engine.
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is available");
    let port = listener.local_addr().expect("bound").port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("one connection");
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request);
        let body = "served over loopback";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();
    });

    let declarations = format!(
        "{}{}",
        common::data(
            "data.endpoint",
            "URI",
            &format!("URI(\"http://127.0.0.1:{port}/data.txt\")")
        ),
        common::data("data.destination", "PATH", "PATH(\"/srv/out/data.txt\")")
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    let source = common::task(&declarations, &[action]);

    let grants = Grants::none()
        .permit_write("/srv/out")
        .permit_network_host("127.0.0.1");
    let filesystem = MemoryFileSystem::new().with_scope("/srv/out");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(filesystem)
        .with_transport(lcl_capabilities::TcpTransport::new(grants));
    let execution = run_with_host(&source, &mut host);

    let _ = server.join();
    let result = common::result_of(&execution, "action.download");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("bytes"),
        Some(&Value::Bytes(
            lcl_checker::numeric::Decimal::parse_integer("20").unwrap()
        )),
        "the retrieved body is exactly the twenty bytes the server served"
    );
}

// ---------------------------------------------------------------------------
// EF-01 — a failure after the target was opened is not proof of no effect
// ---------------------------------------------------------------------------

/// A document that writes `content` to `target`, authorized by its own ALLOW.
fn writing_document(target: &str) -> String {
    let action = format!(
        "ID: action.write\nOPERATION: core.write\nTARGET: PATH({target:?})\n\
         PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
         VALUE: \"written by lcl\"\n\
         PARAMETER:\n    NAME: create_if_missing\n    TYPE: BOOLEAN\n    \
         REQUIRED: FALSE\n    VALUE: TRUE"
    );
    let allow = format!(
        "\nALLOW:\n    ID: allow.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    AUTHORITY: 900\n"
    );
    common::task(&allow, &[&action])
}

/// `/dev/full` accepts an open and refuses every write with `ENOSPC`.
///
/// That is exactly the shape of failure this case is about, and the reason it
/// is used rather than filling a real filesystem: the adapter has already
/// opened the target for modification — for a replacing write, already
/// truncated it — when the write fails. It cannot then prove that nothing
/// began, and `05_SEMANTICS/09` only permits the effect-free claim for a
/// failure that is *positively established* as pre-effect.
///
/// Linux only, because the device is.
#[cfg(target_os = "linux")]
#[test]
fn a_write_failing_after_the_target_was_opened_is_not_proven_effect_free() {
    let grants = Grants::none().permit_write("/dev/full");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants));
    let execution = run_with_host(&writing_document("/dev/full"), &mut host);
    let result = common::result_of(&execution, "action.write");

    assert!(
        !result.execution_errors.is_empty(),
        "the write did not succeed and the record must say so"
    );
    assert_ne!(
        result.failure_phase,
        lcl_runtime::result::FailurePhase::PreEffect,
        "a write that failed after its target was opened is not a pre-effect \
         failure: {result:?}"
    );
    assert_ne!(
        result.effect_state,
        lcl_runtime::result::EffectState::None,
        "and its effect state is not `none`: {result:?}"
    );
    assert!(
        !result.observed_effects.is_empty(),
        "the adapter records what it could not rule out, rather than an empty \
         list that reads as proof of nothing: {result:?}"
    );
}

/// The control, and the other half of the contract: a failure the adapter
/// *can* prove is pre-effect still reports exactly that.
#[test]
fn a_refused_write_is_still_proven_effect_free() {
    let scratch = std::env::temp_dir().join(format!("lcl-ef01-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let outside = scratch.join("ungranted.txt");
    let granted = scratch.join("granted");
    std::fs::create_dir_all(&granted).expect("granted scope");

    let grants = Grants::none().permit_write(&granted);
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants));
    let execution = run_with_host(&writing_document(&outside.display().to_string()), &mut host);
    let result = common::result_of(&execution, "action.write");

    assert_eq!(
        result.failure_phase,
        lcl_runtime::result::FailurePhase::PreEffect,
        "a grant refusal happens before any byte moves: {result:?}"
    );
    assert_eq!(
        result.effect_state,
        lcl_runtime::result::EffectState::None,
        "and it is proven effect-free: {result:?}"
    );
    assert!(!outside.exists(), "and nothing was written");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The second control: an ordinary write into a granted scope still completes
/// and still reports an applied effect.
#[test]
fn an_ordinary_write_still_completes_with_an_applied_effect() {
    let scratch = std::env::temp_dir().join(format!("lcl-ef01-ok-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let target = scratch.join("written.txt");

    let grants = Grants::none().permit_write(&scratch);
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants));
    let execution = run_with_host(&writing_document(&target.display().to_string()), &mut host);
    let result = common::result_of(&execution, "action.write");

    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("readable"),
        "written by lcl"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

// ---------------------------------------------------------------------------
// Q-NETDOMAIN — an HTTP status is not automatically an operation outcome, but
// it is not nothing either
// ---------------------------------------------------------------------------

/// Serve exactly one response and close. Bounded; the thread ends with it.
fn one_response(response: &'static str) -> (u16, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("bound").port();
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, server)
}

fn download_document(port: u16, destination: &std::path::Path) -> String {
    let declarations = format!(
        "{}{}",
        common::data(
            "data.endpoint",
            "URI",
            &format!("URI(\"http://127.0.0.1:{port}/missing.txt\")")
        ),
        common::data(
            "data.destination",
            "PATH",
            &format!("PATH({:?})", destination.display().to_string())
        )
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    common::task(&declarations, &[action])
}

/// A `404` means the source was not accessible.
///
/// `operations_v0.1.0.json#/contracts/core.download` requires the precondition
/// "source is accessible" and the postcondition "destination bytes equal
/// received source". An error page is not the source, so writing it to the
/// destination would satisfy that postcondition with the wrong file and report
/// a transfer that did not happen.
#[test]
fn a_download_of_a_missing_resource_does_not_write_the_error_page() {
    let scratch = std::env::temp_dir().join(format!("lcl-qnet-404-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let destination = scratch.join("downloaded.txt");
    let (port, server) = one_response(
        "HTTP/1.1 404 Not Found\r\nContent-Length: 23\r\nConnection: close\r\n\r\n\
         <html>not here</html>\r\n",
    );

    let grants = Grants::none()
        .permit_write(&scratch)
        .permit_network_host("127.0.0.1");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants.clone()))
        .with_transport(lcl_capabilities::TcpTransport::new(grants));
    let execution = run_with_host(&download_document(port, &destination), &mut host);
    let _ = server.join();

    let result = common::result_of(&execution, "action.download");
    assert_ne!(
        result.status, "status.succeeded",
        "a 404 is not a completed transfer: {result:?}"
    );
    assert!(
        !destination.exists(),
        "and no error page was written to the destination"
    );
    assert_eq!(
        common::errors_of(&execution, "action.download"),
        vec!["error.operation.precondition".to_string()],
        "the registered identifier for an unmet precondition, and the row lists \
         it: {result:?}"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A `500` is the same question with a different number.
#[test]
fn a_download_of_a_failing_resource_does_not_write_the_error_page() {
    let scratch = std::env::temp_dir().join(format!("lcl-qnet-500-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let destination = scratch.join("downloaded.txt");
    let (port, server) = one_response(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 5\r\nConnection: close\r\n\r\nboom!",
    );

    let grants = Grants::none()
        .permit_write(&scratch)
        .permit_network_host("127.0.0.1");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants.clone()))
        .with_transport(lcl_capabilities::TcpTransport::new(grants));
    let execution = run_with_host(&download_document(port, &destination), &mut host);
    let _ = server.join();

    assert_ne!(
        common::result_of(&execution, "action.download").status,
        "status.succeeded"
    );
    assert!(!destination.exists(), "no error page was written");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// A redirect is not content either. This transport does not follow one, and
/// the body of a `302` is certainly not the source.
#[test]
fn a_download_of_a_redirect_does_not_write_the_redirect_body() {
    let scratch = std::env::temp_dir().join(format!("lcl-qnet-302-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let destination = scratch.join("downloaded.txt");
    let (port, server) = one_response(
        "HTTP/1.1 302 Found\r\nLocation: /elsewhere.txt\r\nContent-Length: 9\r\n\
         Connection: close\r\n\r\nmoved on.",
    );

    let grants = Grants::none()
        .permit_write(&scratch)
        .permit_network_host("127.0.0.1");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants.clone()))
        .with_transport(lcl_capabilities::TcpTransport::new(grants));
    let execution = run_with_host(&download_document(port, &destination), &mut host);
    let _ = server.join();

    assert_ne!(
        common::result_of(&execution, "action.download").status,
        "status.succeeded"
    );
    assert!(!destination.exists(), "no redirect body was written");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// The control: a `200` still downloads, and downloads exactly the body.
#[test]
fn a_successful_download_still_writes_exactly_the_received_content() {
    let scratch = std::env::temp_dir().join(format!("lcl-qnet-200-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("owned scratch");
    let destination = scratch.join("downloaded.txt");
    let (port, server) = one_response(
        "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nreal content!",
    );

    let grants = Grants::none()
        .permit_write(&scratch)
        .permit_network_host("127.0.0.1");
    let mut host = HostAdapter::new(grants.clone())
        .with_filesystem(lcl_capabilities::RealFileSystem::new(grants.clone()))
        .with_transport(lcl_capabilities::TcpTransport::new(grants));
    let execution = run_with_host(&download_document(port, &destination), &mut host);
    let _ = server.join();

    let result = common::result_of(&execution, "action.download");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        std::fs::read_to_string(&destination).expect("readable"),
        "real content!"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

// ---------------------------------------------------------------------------
// Q-READ — exact content, and a bound that is too large to be a position
// ---------------------------------------------------------------------------

/// Read `/srv/data/raw.bin` holding exactly `bytes`, with `extra` appended to
/// the ACTION body (a `PARAMETER` block, or nothing).
fn read_bytes(bytes: &[u8], extra: &str) -> Execution {
    let action = format!("ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target){extra}");
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/raw.bin\")"),
        &[action.as_str()],
    );
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/raw.bin", bytes);
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    run_with_host(&source, &mut host)
}

/// A `format` PARAMETER, written the way the checker admits it today.
fn format_parameter(value: &str) -> String {
    format!(
        "\nPARAMETER:\n    NAME: format\n    TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: {value}"
    )
}

/// Q-READ: bytes that are not UTF-8 are refused, never substituted.
///
/// This replaces the EXPECTED_REPRODUCTION
/// `expected_reproduction_qread_a_non_utf8_read_substitutes_silently`, which
/// recorded the baseline: `61 FF 62` read back as `"a\u{FFFD}b"` with
/// `status.succeeded` and no diagnostic, because the adapter decoded with
/// `String::from_utf8_lossy`.
///
/// `core.read` means "Retrieve accessible **exact content** without changing
/// its source", its postcondition is that "result is the **exact** requested
/// representation", and its range contract adds that "No clipping or ambient
/// encoding conversion occurs". STRING is "Unicode text", so bytes that are
/// not UTF-8 have no exact STRING representation.
///
/// The owner chose the repair (LCL-CLOSE-02, decision D1): the row's
/// registered `error.host.constraint`, "A host/provider limitation outside
/// portable LCL prevents execution", before any value is bound.
/// `error.operation.postcondition` would describe it more directly, but the
/// row does not register it.
#[test]
fn q_read_content_that_is_not_utf8_is_refused_not_substituted() {
    // 0xFF is not a legal UTF-8 byte in any position.
    let execution = read_bytes(&[b'a', 0xFF, b'b'], "");
    let result = common::result_of(&execution, "action.read");
    assert_eq!(
        result.execution_errors,
        vec!["error.host.constraint".to_string()],
        "content this host cannot represent exactly is its limitation, not a success"
    );
    assert_eq!(result.status, "status.blocked");
    assert_eq!(result.failure_phase.to_string(), "pre_effect");
    assert_eq!(result.effect_state.to_string(), "none");
    assert_eq!(
        result.fields.get("value"),
        None,
        "no substituted content is bound as the file's content"
    );
}

/// The control: UTF-8 content is returned exactly, multi-byte scalars and line
/// terminators included.
#[test]
fn q_read_utf8_content_is_returned_exactly() {
    let text = "é😀\r\nline two\n";
    let execution = read_bytes(text.as_bytes(), "");
    let result = common::result_of(&execution, "action.read");
    assert!(
        result.execution_errors.is_empty(),
        "{:?}",
        result.execution_errors
    );
    assert_eq!(result.status, "status.succeeded");
    assert_eq!(
        result.fields.get("value"),
        Some(&Value::Text(text.to_string()))
    );
}

/// "Resolve the exact target representation and any requested format before
/// selecting the range." This host produces one representation, Unicode text
/// (`format.plain_text`, "Unicode plain text."), and implements no other
/// registered format. A request for another format is therefore refused rather
/// than answered with plain text that is not what was asked for.
#[test]
fn q_read_a_format_this_host_does_not_implement_is_refused() {
    let execution = read_bytes(b"{\"a\": 1}", &format_parameter("\"format.json\""));
    let result = common::result_of(&execution, "action.read");
    assert_eq!(
        result.execution_errors,
        vec!["error.host.constraint".to_string()]
    );
    assert_eq!(result.status, "status.blocked");
    assert_eq!(result.fields.get("value"), None);
}

/// The control: `format.plain_text` is the representation this host produces.
#[test]
fn q_read_the_plain_text_format_reads_utf8_content() {
    let execution = read_bytes(b"plain", &format_parameter("\"format.plain_text\""));
    let result = common::result_of(&execution, "action.read");
    assert!(
        result.execution_errors.is_empty(),
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        result.fields.get("value"),
        Some(&Value::Text("plain".to_string()))
    );
}

/// A `core.read` request for one path, as the runtime would send it.
fn read_request(path: &str) -> lcl_runtime::CapabilityRequest {
    lcl_runtime::CapabilityRequest {
        operation: "core.read".to_string(),
        target: Some(Value::Constructed {
            constructor: "PATH".to_string(),
            text: path.to_string(),
        }),
        parameters: std::collections::BTreeMap::new(),
        authorization: lcl_runtime::Authorized {
            operation: "core.read".to_string(),
            target: None,
            scope: None,
            permitted_by: Vec::new(),
            overridden: Vec::new(),
        },
        category: "read_only".to_string(),
        possible_effects: Default::default(),
        possible_dependencies: Default::default(),
        result_schema: "result.value".to_string(),
        invocation: lcl_runtime::InvocationId::first(0, lcl_runtime::IterationPath::root()),
        source: lcl_resolver::SourceId::new("root.lcl"),
        span: lcl_lexer::Span::empty(0),
    }
}

/// The same rule at the host boundary, for every spelling a request can carry.
///
/// The row types `format` as `qualified_identifier(format)`, and the checker
/// leaves the source spelling of such a parameter to its value, so a request
/// may carry the registered name as an identifier or as text. Only an absent or
/// MISSING format, or exactly `format.plain_text`, reads. Every other value is
/// refused before the target is read, and non-UTF-8 content is refused however
/// plain text was requested.
#[test]
fn q_read_format_is_decided_exactly_at_the_host_boundary() {
    use lcl_runtime::{CapabilityOutcome, Host};

    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/text.txt", b"plain")
        .with_file("/srv/data/raw.bin", vec![b'a', 0xFF, b'b']);
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    let identifier = |name: &str| Value::Identifier(name.to_string());
    let text = |name: &str| Value::Text(name.to_string());
    let cases: Vec<(&str, Option<Value>, bool)> = vec![
        ("/srv/data/text.txt", None, true),
        ("/srv/data/text.txt", Some(Value::Missing), true),
        (
            "/srv/data/text.txt",
            Some(identifier("format.plain_text")),
            true,
        ),
        ("/srv/data/text.txt", Some(text("format.plain_text")), true),
        ("/srv/data/text.txt", Some(identifier("format.json")), false),
        (
            "/srv/data/text.txt",
            Some(identifier("format.binary")),
            false,
        ),
        ("/srv/data/text.txt", Some(text("format.markdown")), false),
        ("/srv/data/text.txt", Some(Value::Null), false),
        ("/srv/data/raw.bin", None, false),
        (
            "/srv/data/raw.bin",
            Some(identifier("format.plain_text")),
            false,
        ),
    ];
    for (path, format, reads) in cases {
        let mut request = read_request(path);
        if let Some(format) = &format {
            request
                .parameters
                .insert("format".to_string(), format.clone());
        }
        let outcome = host.invoke(&request);
        match (&outcome, reads) {
            (CapabilityOutcome::Completed(observation), true) => assert_eq!(
                observation.fields.get("value"),
                Some(&Value::Text("plain".to_string())),
                "{path} with format {format:?}"
            ),
            (CapabilityOutcome::Unavailable(_), false) => {}
            _ => panic!("{path} with format {format:?}: expected reads={reads}, got {outcome:?}"),
        }
    }
}

/// A range bound larger than any position can be.
///
/// The row says "negative, inverted, or excessive bounds are never clipped",
/// and `error.value.out_of_range` is registered for "bounds outside
/// 0 <= start <= end <= length". A bound of 2^70 is excessive, not unreadable:
/// it is a perfectly well typed INTEGER, since `INTEGER` is an "unbounded
/// signed whole number". Reporting it as a malformed *parameter* would answer a
/// different question from the one the document asked.
#[test]
fn q_read_a_range_bound_beyond_machine_size_is_out_of_range_not_malformed() {
    let action = "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: range\n    TYPE: OBJECT\n    REQUIRED: TRUE\n    \
                  VALUE:\n        unit: \"scalar\"\n        start: 0\n        \
                  end: 1180591620717411303424";
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/a.txt\")"),
        &[action],
    );
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", *b"abcd");
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    let execution = run_with_host(&source, &mut host);

    let errors = common::errors_of(&execution, "action.read");
    println!("Q-READ excessive bound: errors={errors:?}");
    assert_eq!(
        errors,
        vec!["error.value.out_of_range".to_string()],
        "an excessive but well-typed INTEGER bound is out of range; the row \
         says such bounds are never clipped, and it is not a malformed parameter"
    );
}

/// The control: an ordinary range still selects, and an ordinary refusal is
/// still the refusal it was.
#[test]
fn q_read_ordinary_bounds_are_unchanged() {
    let action = "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: range\n    TYPE: OBJECT\n    REQUIRED: TRUE\n    \
                  VALUE:\n        unit: \"scalar\"\n        start: 1\n        end: 3";
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/a.txt\")"),
        &[action],
    );
    let filesystem = MemoryFileSystem::new()
        .with_read_scope("/srv/data")
        .with_file("/srv/data/a.txt", *b"abcd");
    let mut host = HostAdapter::new(filesystem.grants().clone()).with_filesystem(filesystem);
    let execution = run_with_host(&source, &mut host);
    assert!(
        common::errors_of(&execution, "action.read").is_empty(),
        "an ordinary range is not affected"
    );
}

// ---------------------------------------------------------------------------
// PRETEST-04 F25: valid URI forms through the host adapter
// ---------------------------------------------------------------------------

/// A download from `uri`, with `host` granted and the memory transport
/// answering for `bound` only.
fn download_from(uri: &str, host: &str, bound: &str) -> Execution {
    let declarations = format!(
        "{}{}",
        common::data("data.endpoint", "URI", &format!("URI({uri:?})")),
        common::data("data.destination", "PATH", "PATH(\"/srv/out/data.txt\")")
    );
    let action = "ID: action.download\nOPERATION: core.download\nTARGET: REF(data.endpoint)\n\
                  PARAMETER:\n    NAME: destination\n    TYPE: PATH\n    REQUIRED: TRUE\n    \
                  VALUE: REF(data.destination)";
    let source = common::task(&declarations, &[action]);
    let grants = Grants::none()
        .permit_write("/srv/out")
        .permit_network_host(host);
    let mut adapter = HostAdapter::new(grants)
        .with_filesystem(MemoryFileSystem::new().with_scope("/srv/out"))
        .with_transport(MemoryTransport::new().with_resource(bound, "retrieved bytes"));
    run_with_host(&source, &mut adapter)
}

/// A valid URI is reached at the host it names when this host can address
/// it, and is a host limitation when it cannot. It is never read as another
/// host, silently sent in cleartext, or reported as a failed operation.
#[test]
fn valid_uri_forms_are_reached_or_are_host_limitations() {
    for (uri, host, bound) in [
        (
            "http://[2001:db8::1]:8080/data.txt",
            "[2001:db8::1]",
            "http://[2001:db8::1]/data.txt",
        ),
        (
            "http://example.invalid?name=data",
            "example.invalid",
            "http://example.invalid/?name=data",
        ),
        (
            "HTTP://example.invalid/data.txt",
            "example.invalid",
            "http://example.invalid/data.txt",
        ),
    ] {
        let execution = download_from(uri, host, bound);
        let result = common::result_of(&execution, "action.download");
        assert_eq!(
            result.status, "status.succeeded",
            "{uri}: {:?}",
            result.execution_errors
        );
    }
    for uri in [
        "ftp://example.invalid/data.txt",
        "mailto:someone@example.invalid",
        "HTTPS://example.invalid:443/data.txt",
        "http://example.invalid:65536/data.txt",
    ] {
        let execution = download_from(uri, "example.invalid", "http://example.invalid/data.txt");
        assert_eq!(
            common::errors_of(&execution, "action.download"),
            vec!["error.host.constraint".to_string()],
            "{uri}"
        );
    }
}

/// `core.send`: "Resolve host and optional network from the recipient or
/// endpoint and resolve the message effect from the selected transport
/// profile." A URI recipient is a network endpoint, so the invocation resolves
/// the network dependency beside host, and still exactly the message effect.
#[test]
fn a_uri_recipient_adds_the_network_dependency_to_a_send() {
    let source = common::task(
        &format!(
            "{}{}",
            common::data("data.body", "STRING", "\"hello\""),
            common::data("data.endpoint", "URI", "URI(\"http://example.invalid/inbox\")")
        ),
        &["ID: action.send\nOPERATION: core.send\nTARGET: REF(data.body)\n\
           PARAMETER:\n    NAME: recipient\n    TYPE: URI\n    REQUIRED: TRUE\n    VALUE: REF(data.endpoint)"],
    );
    let mut stdlib = common::stdlib().with_profiles(lcl_stdlib::transport_profiles());
    let mut host = lcl_runtime::MockHost::new();
    let fixture = common::fixture(&source);
    let execution = lcl_runtime::Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned");
    assert!(common::errors_of(&execution, "action.send").is_empty());
    let request = host
        .requests()
        .first()
        .expect("the send crossed the boundary");
    assert_eq!(
        request.possible_dependencies.iter().collect::<Vec<_>>(),
        vec!["host", "network"]
    );
    assert_eq!(
        request.possible_effects.iter().collect::<Vec<_>>(),
        vec!["message"]
    );
}

// ---------------------------------------------------------------------------
// RO-01: a delegated graph target is executed, never defaulted to success
// ---------------------------------------------------------------------------

/// A `kind.task` document whose TASK schedules exactly the actions named in
/// `scheduled`, with both actions declared.
///
/// `action.wrapper` is a `core.execute` in graph mode over `action.inner`;
/// `action.inner` computes `1 + 2`, so its material primary result is
/// distinguishable from any default.
fn delegating_document(scheduled: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.stdlib\n    \
         NAME: \"Standard library fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\
         \nACTION:\n    ID: action.inner\n    OPERATION: core.calculate\n    \
         PARAMETER:\n        NAME: expression\n        TYPE: STRING\n        \
         REQUIRED: TRUE\n        VALUE: \"1 + 2\"\n\
         \nACTION:\n    ID: action.wrapper\n    OPERATION: core.execute\n    \
         TARGET: REF(action.inner)\n\
         \nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n\
         \nSUCCESS:\n    ID: success.subject\n    ALL: [TRUE]\n\
         \nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    \
         ACTION: {scheduled}\n    SUCCESS: REF(success.subject)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.subject)\n"
    )
}

/// Every result record one declaration produced, in execution order.
fn results_for<'a>(
    execution: &'a Execution,
    declaration: &str,
) -> Vec<&'a lcl_runtime::ResultRecord> {
    execution
        .invocations()
        .iter()
        .filter(|r| r.declaration.as_deref() == Some(declaration))
        .filter_map(|r| r.result.as_ref())
        .collect()
}

/// The delegated unit actually runs, and the row reports what it did.
///
/// `05_SEMANTICS/01`: "Explicit graph-valued operation invocations create their
/// own child invocation under the same rules", and `core.execute`'s completion
/// contract: graph mode "records value exactly when the completed graph exposes
/// one material primary result". A target the engine never entered exposes no
/// result, so reporting `status.succeeded` for it would claim an execution that
/// did not happen.
#[test]
fn core_execute_runs_a_delegated_target_that_nothing_else_schedules() {
    let execution = common::run(&delegating_document("REF(action.wrapper)"));

    // The inner action executed exactly once, on the delegation, and left its
    // own evidence.
    let inner = results_for(&execution, "action.inner");
    assert_eq!(inner.len(), 1, "the delegated unit ran exactly once");
    assert_eq!(inner[0].status, "status.succeeded");
    assert_eq!(
        inner[0].fields.get("value").map(ToString::to_string).as_deref(),
        Some("3"),
        "the delegated unit computed its own result"
    );

    // The wrapper reports graph mode and the graph's one material primary.
    let wrapper = common::result_of(&execution, "action.wrapper");
    assert_eq!(wrapper.status, "status.succeeded");
    assert_eq!(
        wrapper.fields.get("mode"),
        Some(&Value::Identifier("graph".to_string()))
    );
    assert_eq!(
        wrapper.fields.get("value").map(ToString::to_string).as_deref(),
        Some("3"),
        "the row reports the completed graph's material primary result"
    );
}

/// The already-planned control: the same delegation while the inner action is
/// also scheduled in its own right.
///
/// The structural activation and the delegated child invocation are different
/// candidate invocations — "Explicit graph-valued operation invocations create
/// their own child invocation" — so both run and each keeps its own record,
/// rather than one being refused as a duplicate activation path or silently
/// replacing the other.
#[test]
fn a_delegated_target_that_is_also_scheduled_runs_as_both() {
    let execution = common::run(&delegating_document(
        "[REF(action.inner), REF(action.wrapper)]",
    ));

    let inner = results_for(&execution, "action.inner");
    assert_eq!(
        inner.len(),
        2,
        "the scheduled activation and the delegated child invocation are distinct"
    );
    for record in &inner {
        assert_eq!(record.status, "status.succeeded");
        assert_eq!(
            record.fields.get("value").map(ToString::to_string).as_deref(),
            Some("3")
        );
    }

    let wrapper = common::result_of(&execution, "action.wrapper");
    assert_eq!(wrapper.status, "status.succeeded");
    assert_eq!(
        wrapper.fields.get("value").map(ToString::to_string).as_deref(),
        Some("3"),
        "delegation reports its own child invocation, not the scheduled one"
    );
}
