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
