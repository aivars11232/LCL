//! Engine-level result-schema runs.
//!
//! The component runs beside these validate a [`ResultRecord`] against the
//! canonical schema directly. These execute a document through the whole
//! engine and observe the record the runtime actually built, the OUTPUT the
//! completion layer actually published, and what the engine does with a host
//! observation that breaks the closed contract.

use crate::{attempt_field, judge, ExecutedCase, Expectation, Runner};
use lcl_runtime::{CapabilityOutcome, MockHost, Observation, Value};
use lcl_spec::SpecPackage;

/// A `kind.task` document with declarations, one subject ACTION and an
/// optional OUTPUT the action publishes.
fn document(declarations: &str, action: &str, output: Option<(&str, &str)>) -> String {
    let (output_block, output_field) = match output {
        Some((id, ty)) => (
            format!("\nOUTPUT:\n    ID: {id}\n    TYPE: {ty}\n    FORMAT: format.plain_text\n"),
            format!("\n    OUTPUT: REF({id})"),
        ),
        None => (String::new(), String::new()),
    };
    let task_output = match output {
        Some((id, _)) => format!("\n    OUTPUT: REF({id})"),
        None => String::new(),
    };
    crate::fixtures::task_document(&format!(
        "{declarations}{output_block}\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.subject\n    {action}{output_field}\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.subject){task_output}\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n"
    ))
}

fn parameter(name: &str, ty: &str, required: &str, value: &str) -> String {
    format!("\n    PARAMETER:\n        NAME: {name}\n        TYPE: {ty}\n        REQUIRED: {required}\n        VALUE: {value}")
}

fn data(id: &str, ty: &str, value: &str) -> String {
    format!("\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n")
}

/// Run one document on a scripted host and judge it.
fn run(
    runner: &Runner,
    label: &str,
    clause: &str,
    source: &str,
    host: MockHost,
    expectation: Expectation,
) -> ExecutedCase {
    let mut host = host;
    let mut case = runner.execute_on(label, clause, source, expectation, &mut host);
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case
}

/// The exact field names the subject's result record carries.
fn closed_record(
    runner: &Runner,
    label: &str,
    source: &str,
    host: MockHost,
    expected: &[&str],
) -> ExecutedCase {
    let mut host = host;
    let mut case = runner.execute_on(
        label,
        "the closed result contract: the engine records exactly the registered schema-local fields",
        source,
        Expectation::Accepts,
        &mut host,
    );
    let fields: Vec<String> = case
        .observed
        .invocations
        .iter()
        .filter(|i| i.declaration.as_deref() == Some("action.subject"))
        .filter_map(|i| i.result.as_ref())
        .flat_map(|r| r.fields.keys().cloned())
        .collect();
    case.expectation = Expectation::All(vec![
        Expectation::Attempts {
            declaration: "action.subject".into(),
            statuses: vec!["status.succeeded".into()],
        },
        Expectation::Component(vec![("fields".into(), format!("{expected:?}"))]),
    ]);
    case.observed.component = vec![("fields".into(), format!("{fields:?}"))];
    case.observed.input_evidence.push(format!(
        "host: lcl-runtime MockHost; requests={:?}",
        host.requests()
    ));
    case.verdict = judge(&case.expectation, &case.observed);
    case
}

/// A completed observation with one extra field the schema does not register.
fn output(id: &str, value: &str) -> Expectation {
    Expectation::Output {
        id: id.into(),
        value: value.into(),
    }
}

fn succeeded() -> Expectation {
    Expectation::Attempts {
        declaration: "action.subject".into(),
        statuses: vec!["status.succeeded".into()],
    }
}

fn integer(n: &str) -> Value {
    Value::Integer(lcl_checker::numeric::Decimal::parse_integer(n).expect("an integer literal"))
}

/// Every engine run of one result schema.
pub(super) fn runs(_spec: &SpecPackage, runner: &Runner, schema: &str) -> Vec<ExecutedCase> {
    let text = |s: &str| Value::Text(s.to_string());
    let mut out = Vec::new();
    match schema {
        "result.value" => {
            let read = "OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")";
            let source = |output: Option<(&str, &str)>| document("", read, output);
            let value = |v: Value| {
                CapabilityOutcome::Completed(
                    Observation::none()
                        .with("value", v)
                        .with("evidence", Value::List(Vec::new())),
                )
            };
            let host =
                |outcome: CapabilityOutcome| MockHost::new().script("core.read", vec![outcome]);
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &source(None),
                host(value(text("data"))),
                &["evidence", "value"],
            ));
            out.push(run(
                runner,
                "engine/value-default-projection",
                "the registered default_property `value` is published when no PROPERTY is selected",
                &source(Some(("output.read", "STRING"))),
                host(value(text("data"))),
                Expectation::All(vec![succeeded(), output("output.read", "\"data\"")]),
            ));
            out.push(run(
                runner,
                "engine/falsy-material-values-bind",
                "a falsy material value is still material and binds OUTPUT",
                &document("", read, Some(("output.read", "BOOLEAN"))),
                host(value(Value::Boolean(false))),
                Expectation::All(vec![succeeded(), output("output.read", "FALSE")]),
            ));
            out.push(run(
                runner,
                "engine/value-required-on-success",
                "status.succeeded requires value exactly once; a host that omits it breaks the closed contract",
                &source(None),
                host(CapabilityOutcome::Completed(Observation::none().with("evidence", Value::List(Vec::new())))),
                Expectation::Diagnostic("error.host.constraint".into()),
            ));
            out.push(run(
                runner,
                "engine/unknown-rejected",
                "a required material value of UNKNOWN is refused, never recorded",
                &source(None),
                host(value(Value::Unknown)),
                Expectation::Diagnostic("error.host.constraint".into()),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.value declares no partial OUTPUT: a failed producer leaves OUTPUT unbound",
                &source(Some(("output.read", "STRING"))),
                MockHost::new().script(
                    "core.read",
                    vec![MockHost::failed_before_effect(
                        "conformance: the read did not start",
                    )],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.execution.action".into()),
                    output("output.read", "UNBOUND"),
                ]),
            ));
        }
        "result.collection" => {
            let members = data("data.members", "LIST[INTEGER]", "[2, 1, 3]");
            let empty = data("data.empty", "LIST[INTEGER]", "[]");
            let filter = |target: &str, predicate: &str| {
                format!(
                    "OPERATION: core.filter\n    TARGET: REF({target}){}",
                    parameter("predicate", "STRING", "TRUE", predicate)
                )
            };
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&members, &filter("data.members", "\"item > 1\""), None),
                MockHost::new(),
                &["count", "items"],
            ));
            out.push(run(
                runner,
                "engine/items-and-count-on-success",
                "status.succeeded requires items and count, and count equals the member count",
                &document(&members, &filter("data.members", "\"item > 1\""), None),
                MockHost::new(),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("items", "[2, 3]"),
                    attempt_field("count", "2"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/empty-list-count-zero-binds",
                "an empty items list is a valid completed outcome with count 0, and it binds OUTPUT",
                &document(&empty, &filter("data.empty", "\"item > 1\""), Some(("output.items", "LIST[INTEGER]"))),
                MockHost::new(),
                Expectation::All(vec![succeeded(), attempt_field("count", "0"), output("output.items", "[]")]),
            ));
            out.push(run(
                runner,
                "engine/items-default-projection",
                "the registered default_property `items` is published when no PROPERTY is selected",
                &document(
                    &members,
                    &filter("data.members", "\"item > 1\""),
                    Some(("output.items", "LIST[INTEGER]")),
                ),
                MockHost::new(),
                Expectation::All(vec![succeeded(), output("output.items", "[2, 3]")]),
            ));
            out.push(run(
                runner,
                "engine/unknown-rejected",
                "an UNKNOWN member decision is refused, never recorded as items or count",
                &document(&members, &filter("data.members", "\"UNKNOWN\""), None),
                MockHost::new(),
                Expectation::Diagnostic("error.value.unknown".into()),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.collection declares no partial OUTPUT: a failed producer leaves OUTPUT unbound",
                &document(&members, &filter("data.members", "\"UNKNOWN\""), Some(("output.items", "LIST[INTEGER]"))),
                MockHost::new(),
                Expectation::All(vec![Expectation::Diagnostic("error.value.unknown".into()), output("output.items", "UNBOUND")]),
            ));
        }
        "result.operation" => {
            let write = format!(
                "OPERATION: core.write\n    TARGET: PATH(\"/case/out.txt\"){}",
                parameter("content", "STRING", "TRUE", "\"x\"")
            );
            let source = |output: Option<(&str, &str)>| document("", &write, output);
            let completed = |changed: Value, value: Option<Value>| {
                let mut observation = Observation::none().with("changed", changed).with(
                    "target",
                    Value::Constructed {
                        constructor: "PATH".into(),
                        text: "/case/out.txt".into(),
                    },
                );
                if let Some(value) = value {
                    observation = observation.with("value", value);
                }
                CapabilityOutcome::Completed(observation)
            };
            let host =
                |outcome: CapabilityOutcome| MockHost::new().script("core.write", vec![outcome]);
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &source(None),
                host(completed(Value::Boolean(true), None)),
                &["changed", "target"],
            ));
            out.push(run(
                runner,
                "engine/changed-default-projection",
                "the registered default_property `changed` is published when no PROPERTY is selected",
                &source(Some(("output.changed", "BOOLEAN"))),
                host(completed(Value::Boolean(true), None)),
                Expectation::All(vec![succeeded(), output("output.changed", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/changed-kept-after-failure-unbound",
                "changed remains present after failure and does not by itself bind OUTPUT",
                &source(Some(("output.changed", "BOOLEAN"))),
                MockHost::new().deny("core.write", "conformance: the host refuses the write"),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.permission.denied".into()),
                    attempt_field("changed", "FALSE"),
                    output("output.changed", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/unknown-changed-never-binds",
                "UNKNOWN changed never binds OUTPUT",
                &source(Some(("output.changed", "BOOLEAN"))),
                host(completed(Value::Unknown, None)),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("changed", "UNKNOWN"),
                    output("output.changed", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/value-only-when-exposed",
                "value is present only where the operation contract explicitly produces a material artifact",
                &source(None),
                host(completed(Value::Boolean(true), None)),
                Expectation::All(vec![succeeded(), Expectation::Component(vec![("value".into(), "absent".into())])]),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.operation declares no partial OUTPUT: a failed producer leaves OUTPUT unbound",
                &source(Some(("output.changed", "BOOLEAN"))),
                MockHost::new().script("core.write", vec![MockHost::failed_before_effect("conformance: the write did not start")]),
                Expectation::All(vec![Expectation::Diagnostic("error.execution.action".into()), output("output.changed", "UNBOUND")]),
            ));
        }
        "result.command" => {
            // Graph mode: "started, completed, exit_code, stdout, and stderr
            // are absent; graph completion is represented by status and no
            // command observation is synthesized", and "The default stdout
            // projection is unavailable in graph mode; a graph OUTPUT requires
            // an explicit value or mode property".
            let graph_document = |output: &str, property: &str| {
                let task_output = if output.is_empty() {
                    ""
                } else {
                    "\n    OUTPUT: REF(output.result)"
                };
                crate::fixtures::task_document(&format!(
                    "\nDATA:\n    ID: data.number\n    TYPE: INTEGER\n    VALUE: 3\n{output}\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.inner\n    OPERATION: core.return\n    TARGET: REF(data.number)\n\nACTION:\n    ID: action.subject\n    OPERATION: core.execute\n    TARGET: REF(action.inner){property}\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: [REF(action.subject), REF(action.inner)]{task_output}\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n"
                ))
            };
            out.push(closed_record(
                runner,
                "engine/graph-no-native-fields",
                &graph_document("", ""),
                MockHost::new(),
                &["mode", "value"],
            ));
            const OUTPUT_BLOCK: &str =
                "\nOUTPUT:\n    ID: output.result\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n";
            {
                // One run, two documents: the default projection binds nothing,
                // and the explicit `value` property binds the graph's one
                // material primary result.
                let mut host = MockHost::new();
                let default_binding = runner
                    .run_on(
                        &graph_document(OUTPUT_BLOCK, "\n    OUTPUT: REF(output.result)"),
                        &lcl_resolver::MemoryProvider::new(),
                        &mut host,
                    )
                    .invocations
                    .iter()
                    .find(|record| record.declaration.as_deref() == Some("action.subject"))
                    .and_then(|record| record.result.as_ref())
                    .map(|result| result.output_binding.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let mut host = MockHost::new();
                // "Every selected name must occur in the schema's
                // projectable_fields list": the selection is declared on the
                // OUTPUT, as canonical example 04 writes it.
                const SELECTING_VALUE: &str = "\nOUTPUT:\n    ID: output.result\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n    PROPERTY: value\n";
                let explicit = runner.run_on(
                    &graph_document(SELECTING_VALUE, "\n    OUTPUT: REF(output.result)"),
                    &lcl_resolver::MemoryProvider::new(),
                    &mut host,
                );
                let explicit_binding = explicit
                    .invocations
                    .iter()
                    .find(|record| record.declaration.as_deref() == Some("action.subject"))
                    .and_then(|record| record.result.as_ref())
                    .map(|result| result.output_binding.to_string())
                    .unwrap_or_else(|| "none".to_string());
                let published = explicit
                    .outputs
                    .iter()
                    .find(|(id, _)| id == "output.result")
                    .map(|(_, value)| value.clone())
                    .unwrap_or_else(|| "unpublished".to_string());
                let expectation = Expectation::Component(vec![
                    ("default projection".to_string(), "unbound".to_string()),
                    ("explicit value property".to_string(), "bound".to_string()),
                    ("published".to_string(), "3".to_string()),
                ]);
                let mut observed = explicit;
                observed.component = vec![
                    ("default projection".to_string(), default_binding),
                    ("explicit value property".to_string(), explicit_binding),
                    ("published".to_string(), published),
                ];
                observed.input_evidence.push(
                    "two graph-mode documents: one OUTPUT with no PROPERTY, one selecting value"
                        .to_string(),
                );
                let verdict = crate::judge(&expectation, &observed);
                out.push(ExecutedCase {
                    id: "engine/graph-output-requires-explicit-selection".into(),
                    contract: "the default stdout projection is unavailable in graph mode; a graph OUTPUT requires an explicit value or mode property".into(),
                    source: graph_document(OUTPUT_BLOCK, "\n    OUTPUT: REF(output.result)"),
                    expectation,
                    observed,
                    verdict,
                });
            }
            let execute = "OPERATION: core.execute\n    TARGET: \"run --now\"";
            let source = |output: Option<(&str, &str)>| document("", execute, output);
            let command =
                |started: bool, completed: bool, exit: Option<&str>, stdout: Option<&str>| {
                    let mut observation = Observation::none()
                        .with("mode", Value::Identifier("non_graph".into()))
                        .with("started", Value::Boolean(started))
                        .with("completed", Value::Boolean(completed));
                    if let Some(stdout) = stdout {
                        observation = observation
                            .with("stdout", text(stdout))
                            .with("stderr", text(""));
                    }
                    if let Some(code) = exit {
                        observation = observation.with("exit_code", integer(code));
                    }
                    observation
                };
            let host = |observation: Observation| {
                MockHost::new().script(
                    "core.execute",
                    vec![CapabilityOutcome::Completed(observation)],
                )
            };
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &source(None),
                host(command(true, true, Some("0"), Some("out"))),
                &[
                    "completed",
                    "exit_code",
                    "mode",
                    "started",
                    "stderr",
                    "stdout",
                ],
            ));
            out.push(run(
                runner,
                "engine/completed-exit-code",
                "exit_code is present exactly when completed is TRUE",
                &source(None),
                host(command(true, true, Some("0"), Some("out"))),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("completed", "TRUE"),
                    attempt_field("exit_code", "0"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/nonzero-exit-completed",
                "a nonzero exit_code is a completed domain outcome, not a failure to start",
                &source(None),
                host(command(true, true, Some("7"), Some(""))),
                Expectation::All(vec![succeeded(), attempt_field("exit_code", "7")]),
            ));
            out.push(run(
                runner,
                "engine/started-streams-present",
                "once a non_graph command starts, stdout and stderr are present, including as empty strings",
                &source(None),
                host(command(true, true, Some("0"), Some(""))),
                Expectation::All(vec![succeeded(), attempt_field("stdout", "\"\""), attempt_field("stderr", "\"\"")]),
            ));
            out.push(run(
                runner,
                "engine/stdout-default-projection",
                "the registered default_property `stdout` is published when no PROPERTY is selected",
                &source(Some(("output.stdout", "STRING"))),
                host(command(true, true, Some("0"), Some("printed"))),
                Expectation::All(vec![succeeded(), output("output.stdout", "\"printed\"")]),
            ));
            out.push(run(
                runner,
                "engine/failure-to-start-record",
                "a failure to start records started FALSE and completed FALSE with no exit_code, stdout or stderr",
                &source(Some(("output.stdout", "STRING"))),
                MockHost::new().script(
                    "core.execute",
                    vec![CapabilityOutcome::Failed {
                        detail: "conformance: the program never started".into(),
                        observation: command(false, false, None, None),
                    }],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.execution.action".into()),
                    attempt_field("started", "FALSE"),
                    attempt_field("completed", "FALSE"),
                    output("output.stdout", "UNBOUND"),
                ]),
            ));
            let mut interrupted = command(true, false, None, Some("partial output"));
            interrupted.host_limited = true;
            out.push(run(
                runner,
                "engine/partial-only-streams",
                "an interrupted command retains its streams as effect truth and binds no complete OUTPUT",
                &source(Some(("output.stdout", "STRING"))),
                MockHost::new().script(
                    "core.execute",
                    vec![CapabilityOutcome::Failed { detail: "conformance: interrupted".into(), observation: interrupted }],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.host.constraint".into()),
                    attempt_field("stdout", "\"partial output\""),
                    output("output.stdout", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/unknown-rejected",
                "an UNKNOWN in a closed ENUM field breaks the contract and is refused",
                &source(None),
                host(
                    Observation::none()
                        .with("mode", Value::Unknown)
                        .with("started", Value::Boolean(true))
                        .with("completed", Value::Boolean(true)),
                ),
                Expectation::Diagnostic("error.host.constraint".into()),
            ));
        }
        "result.validation" => {
            let validate = "OPERATION: core.validate\n    TARGET: REF(data.subject)";
            let subject = data("data.subject", "INTEGER", "3");
            let rule = |assertion: &str| {
                format!("\nVALIDATE:\n    ID: validate.case\n    ASSERT: {assertion}\n    REQUIRED: FALSE\n")
            };
            let with_rules = format!(
                "{validate}\n    PARAMETER:\n        NAME: rules\n        TYPE: LIST[REFERENCE[REF(validate.case)]]\n        REQUIRED: FALSE\n        VALUE: [REF(validate.case)]"
            );
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&subject, validate, None),
                MockHost::new(),
                &["errors", "valid"],
            ));
            out.push(run(
                runner,
                "engine/valid-default-projection",
                "the registered default_property `valid` is published when no PROPERTY is selected",
                &document(&subject, validate, Some(("output.valid", "BOOLEAN"))),
                MockHost::new(),
                Expectation::All(vec![succeeded(), output("output.valid", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/success-with-valid-false",
                "status.succeeded with valid FALSE is a valid completed validation result",
                &document(
                    &format!("{subject}{}", rule("REF(data.subject) < 2")),
                    &with_rules,
                    None,
                ),
                MockHost::new(),
                Expectation::All(vec![succeeded(), attempt_field("valid", "FALSE")]),
            ));
            out.push(run(
                runner,
                "engine/domain-errors-distinct-from-execution-errors",
                "domain findings live in errors and are distinct from execution_errors",
                &document(
                    &format!("{subject}{}", rule("REF(data.subject) < 2")),
                    &with_rules,
                    None,
                ),
                MockHost::new(),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("errors", "[error.validation.failed]"),
                    Expectation::NoDiagnostic("error.validation.failed".into()),
                ]),
            ));
            out.push(run(
                runner,
                "engine/unknown-rejected",
                "an UNKNOWN rule outcome is refused, never recorded as valid UNKNOWN",
                &document(
                    &format!("{subject}{}", rule("REF(data.subject) < UNKNOWN")),
                    &with_rules,
                    None,
                ),
                MockHost::new(),
                Expectation::Diagnostic("error.value.unknown".into()),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.validation declares no partial OUTPUT: a failed producer leaves OUTPUT unbound",
                &document(&subject, "OPERATION: core.validate\n    TARGET: REF(data.subject)\n    PARAMETER:\n        NAME: rules\n        TYPE: LIST[REFERENCE[REF(data.subject)]]\n        REQUIRED: FALSE\n        VALUE: [REF(data.subject)]", Some(("output.valid", "BOOLEAN"))),
                MockHost::new(),
                Expectation::All(vec![Expectation::Diagnostic("error.reference.kind".into()), output("output.valid", "UNBOUND")]),
            ));
        }
        "result.verification" => {
            let subject = data("data.subject", "INTEGER", "3");
            let verify = |assertion: &str| {
                format!(
                    "OPERATION: core.verify\n    TARGET: REF(data.subject){}",
                    parameter("assertion", "BOOLEAN", "TRUE", assertion)
                )
            };
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&subject, &verify("REF(data.subject) == 3"), None),
                MockHost::new(),
                &["errors", "evidence", "observed", "verified"],
            ));
            out.push(run(
                runner,
                "engine/verified-and-observed-after-run",
                "verified and observed are present exactly once after verification runs",
                &document(&subject, &verify("REF(data.subject) == 3"), None),
                MockHost::new(),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("verified", "TRUE"),
                    attempt_field("observed", "{target: 3}"),
                ]),
            ));
            // "UNKNOWN verified never binds OUTPUT." `core.verify` "resolve[s]
            // observation dependencies ... from the target", so an addressable
            // target is observed by the host that owns it, and a host that
            // cannot establish the assertion answers UNKNOWN through the
            // production boundary rather than inventing a verdict.
            out.push(run(
                runner,
                "engine/unknown-never-binds",
                "an UNKNOWN verified result never binds the selected OUTPUT",
                &document(
                    &data("data.path", "PATH", "PATH(\"/srv/data/report.txt\")"),
                    &format!(
                        "OPERATION: core.verify\n    TARGET: REF(data.path){}",
                        parameter("assertion", "BOOLEAN", "TRUE", "TRUE")
                    ),
                    Some(("output.verified", "BOOLEAN")),
                ),
                MockHost::new().script(
                    "core.verify",
                    vec![CapabilityOutcome::Completed(
                        Observation::none()
                            .with("verified", Value::Unknown)
                            .with(
                                "observed",
                                Value::Object(
                                    [("target".to_string(), Value::Unknown)]
                                        .into_iter()
                                        .collect(),
                                ),
                            )
                            .with("errors", Value::List(Vec::new()))
                            .with("evidence", Value::List(Vec::new())),
                    )],
                ),
                Expectation::All(vec![
                    attempt_field("verified", "UNKNOWN"),
                    attempt_field("output_binding", "unbound"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/verified-default-projection",
                "the registered default_property `verified` is published when no PROPERTY is selected",
                &document(&subject, &verify("REF(data.subject) == 3"), Some(("output.verified", "BOOLEAN"))),
                MockHost::new(),
                Expectation::All(vec![succeeded(), output("output.verified", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/success-with-false-or-unknown",
                "verified records FALSE independently of producer status",
                &document(&subject, &verify("REF(data.subject) == 9"), None),
                MockHost::new(),
                Expectation::All(vec![succeeded(), attempt_field("verified", "FALSE")]),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.verification declares no partial OUTPUT: a producer that failed before effects leaves OUTPUT unbound",
                &document(&subject, &verify("REF(data.subject) == 3"), Some(("output.verified", "BOOLEAN"))),
                MockHost::new(),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.operation.precondition".into()),
                    output("output.verified", "UNBOUND"),
                ]),
            ));
        }
        "result.test" => {
            let subject = data("data.subject", "INTEGER", "3");
            let test = |extra: String| format!("OPERATION: core.test{extra}");
            let assertion = |value: &str| test(parameter("assertion", "BOOLEAN", "FALSE", value));
            let comparison = |expected: &str, actual: &str| {
                test(format!(
                    "{}{}",
                    parameter("expected", "INTEGER", "FALSE", expected),
                    parameter("actual", "INTEGER", "FALSE", actual)
                ))
            };
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&subject, &assertion("REF(data.subject) == 3"), None),
                MockHost::new(),
                &["evidence", "passed"],
            ));
            out.push(run(
                runner,
                "engine/assertion-form-without-comparison",
                "assertion form omits expected and actual",
                &document(&subject, &assertion("REF(data.subject) == 3"), None),
                MockHost::new(),
                Expectation::All(vec![succeeded(), attempt_field("passed", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/comparison-form-with-both",
                "expected-and-actual form requires each exactly once",
                &document(&subject, &comparison("3", "REF(data.subject)"), None),
                MockHost::new(),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("expected", "3"),
                    attempt_field("actual", "3"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/tested-null-is-material",
                "a tested NULL is a present material value, not an absent field",
                &document(
                    &subject,
                    &test(format!(
                        "{}{}",
                        parameter("expected", "NULL", "FALSE", "NULL"),
                        parameter("actual", "NULL", "FALSE", "NULL")
                    )),
                    None,
                ),
                MockHost::new(),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("expected", "NULL"),
                    attempt_field("passed", "TRUE"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/passed-default-projection",
                "the registered default_property `passed` is published when no PROPERTY is selected",
                &document(&subject, &assertion("REF(data.subject) == 3"), Some(("output.passed", "BOOLEAN"))),
                MockHost::new(),
                Expectation::All(vec![succeeded(), output("output.passed", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/success-with-false-or-unknown",
                "passed records FALSE independently of producer status",
                &document(&subject, &comparison("9", "REF(data.subject)"), None),
                MockHost::new(),
                Expectation::All(vec![succeeded(), attempt_field("passed", "FALSE")]),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.test declares no partial OUTPUT: a producer that failed before effects leaves OUTPUT unbound",
                &document(
                    &format!("{subject}\nOUTPUT:\n    ID: output.pending\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n"),
                    &test(format!(
                        "{}\n    PARAMETER:\n        NAME: actual\n        TYPE: REFERENCE[REF(output.pending)]\n        REQUIRED: FALSE\n        VALUE: REF(output.pending)",
                        parameter("expected", "INTEGER", "FALSE", "3")
                    )),
                    Some(("output.passed", "BOOLEAN")),
                ),
                MockHost::new(),
                Expectation::All(vec![output("output.passed", "UNBOUND")]),
            ));
        }
        "result.message" => {
            let endpoint = data(
                "data.endpoint",
                "URI",
                "URI(\"http://example.invalid/inbox\")",
            );
            let body = data("data.body", "STRING", "\"hello\"");
            let send = format!(
                "OPERATION: core.send\n    TARGET: REF(data.body){}",
                parameter("recipient", "URI", "TRUE", "REF(data.endpoint)")
            );
            let message = |delivered: Value, id: Value| {
                CapabilityOutcome::Completed(
                    Observation::none()
                        .with("delivered", delivered)
                        .with(
                            "recipient",
                            Value::Constructed {
                                constructor: "URI".into(),
                                text: "http://example.invalid/inbox".into(),
                            },
                        )
                        .with("message_id", id),
                )
            };
            let host =
                |outcome: CapabilityOutcome| MockHost::new().script("core.send", vec![outcome]);
            let declarations = format!("{endpoint}{body}");
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&declarations, &send, None),
                host(message(Value::Boolean(true), Value::Null)),
                &["delivered", "message_id", "recipient"],
            ));
            out.push(run(
                runner,
                "engine/delivered-default-projection",
                "the registered default_property `delivered` is published when no PROPERTY is selected",
                &document(&declarations, &send, Some(("output.delivered", "BOOLEAN"))),
                host(message(Value::Boolean(true), Value::Null)),
                Expectation::All(vec![succeeded(), output("output.delivered", "TRUE")]),
            ));
            out.push(run(
                runner,
                "engine/null-message-id-means-unassigned",
                "message_id is NULL when the completed interaction assigned no identifier",
                &document(&declarations, &send, None),
                host(message(Value::Boolean(true), Value::Null)),
                Expectation::All(vec![succeeded(), attempt_field("message_id", "NULL")]),
            ));
            out.push(run(
                runner,
                "engine/success-with-false-or-unknown",
                "status.succeeded records completed dispatch and may accompany delivered FALSE",
                &document(&declarations, &send, None),
                host(message(Value::Boolean(false), Value::Null)),
                Expectation::All(vec![succeeded(), attempt_field("delivered", "FALSE")]),
            ));
            out.push(run(
                runner,
                "engine/unknown-never-binds",
                "UNKNOWN delivered never binds OUTPUT",
                &document(&declarations, &send, Some(("output.delivered", "BOOLEAN"))),
                host(message(Value::Unknown, Value::Null)),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("delivered", "UNKNOWN"),
                    output("output.delivered", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.message declares no partial OUTPUT: a failed dispatch leaves OUTPUT unbound",
                &document(&declarations, &send, Some(("output.delivered", "BOOLEAN"))),
                MockHost::new().script("core.send", vec![MockHost::failed_before_effect("conformance: the dispatch did not start")]),
                Expectation::All(vec![Expectation::Diagnostic("error.execution.action".into()), output("output.delivered", "UNBOUND")]),
            ));
        }
        "result.transfer" => {
            let endpoint = data(
                "data.endpoint",
                "URI",
                "URI(\"http://example.invalid/upload\")",
            );
            let upload = format!(
                "OPERATION: core.upload\n    TARGET: PATH(\"/case/data.txt\"){}",
                parameter("destination", "URI", "TRUE", "REF(data.endpoint)")
            );
            let transfer = |bytes: Option<Value>, value: Option<Value>| {
                let mut observation = Observation::none()
                    .with(
                        "source",
                        Value::Constructed {
                            constructor: "PATH".into(),
                            text: "/case/data.txt".into(),
                        },
                    )
                    .with(
                        "destination",
                        Value::Constructed {
                            constructor: "URI".into(),
                            text: "http://example.invalid/upload".into(),
                        },
                    );
                if let Some(bytes) = bytes {
                    observation = observation.with("bytes", bytes);
                }
                if let Some(value) = value {
                    observation = observation.with("value", value);
                }
                observation
            };
            let bytes = |n: &str| {
                Value::Bytes(lcl_checker::numeric::Decimal::parse_integer(n).expect("a byte count"))
            };
            let host = |observation: Observation| {
                MockHost::new().script(
                    "core.upload",
                    vec![CapabilityOutcome::Completed(observation)],
                )
            };
            out.push(closed_record(
                runner,
                "engine/closed-record",
                &document(&endpoint, &upload, None),
                host(transfer(Some(bytes("5")), None)),
                &["bytes", "destination", "source"],
            ));
            out.push(run(
                runner,
                "engine/bytes-default-projection",
                "the registered default_property `bytes` is published when no PROPERTY is selected",
                &document(&endpoint, &upload, Some(("output.bytes", "BYTES"))),
                host(transfer(Some(bytes("5")), None)),
                Expectation::All(vec![succeeded(), output("output.bytes", "BYTES(5)")]),
            ));
            out.push(run(
                runner,
                "engine/bytes-zero-valid",
                "a completed zero-byte transfer is valid and records BYTES(0)",
                &document(&endpoint, &upload, None),
                host(transfer(Some(bytes("0")), None)),
                Expectation::All(vec![succeeded(), attempt_field("bytes", "BYTES(0)")]),
            ));
            out.push(run(
                runner,
                "engine/bytes-absent-before-transfer",
                "bytes is absent before the transfer begins",
                &document(&endpoint, &upload, None),
                MockHost::new().script(
                    "core.upload",
                    vec![CapabilityOutcome::Failed {
                        detail: "conformance: refused before the transfer began".into(),
                        observation: transfer(None, None),
                    }],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.execution.action".into()),
                    Expectation::Component(vec![("bytes".into(), "absent".into())]),
                ]),
            ));
            let mut interrupted = transfer(Some(bytes("3")), None);
            interrupted.host_limited = true;
            out.push(run(
                runner,
                "engine/interrupted-count-retained-without-output",
                "a known interrupted-transfer byte count is effect truth and binds no partial OUTPUT",
                &document(&endpoint, &upload, Some(("output.bytes", "BYTES"))),
                MockHost::new().script(
                    "core.upload",
                    vec![CapabilityOutcome::Failed { detail: "conformance: interrupted after three bytes".into(), observation: interrupted }],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.host.constraint".into()),
                    attempt_field("bytes", "BYTES(3)"),
                    output("output.bytes", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/unknown-never-binds",
                "UNKNOWN bytes never binds OUTPUT",
                &document(&endpoint, &upload, Some(("output.bytes", "BYTES"))),
                host(transfer(Some(Value::Unknown), None)),
                Expectation::All(vec![
                    succeeded(),
                    attempt_field("bytes", "UNKNOWN"),
                    output("output.bytes", "UNBOUND"),
                ]),
            ));
            out.push(run(
                runner,
                "engine/value-only-when-content-supplied",
                "value is present only when the transfer operation explicitly exposes material content",
                &document(&endpoint, &upload, None),
                host(transfer(Some(bytes("5")), None)),
                Expectation::All(vec![succeeded(), Expectation::Component(vec![("value".into(), "absent".into())])]),
            ));
            out.push(run(
                runner,
                "engine/partial-output-unsupported",
                "result.transfer declares no partial OUTPUT",
                &document(&endpoint, &upload, Some(("output.bytes", "BYTES"))),
                MockHost::new().script(
                    "core.upload",
                    vec![MockHost::failed_before_effect(
                        "conformance: the transfer did not start",
                    )],
                ),
                Expectation::All(vec![
                    Expectation::Diagnostic("error.execution.action".into()),
                    output("output.bytes", "UNBOUND"),
                ]),
            ));
        }
        _ => {}
    }
    // Component observations for the two runs that assert an absent field.
    for case in out.iter_mut() {
        if !matches!(&case.expectation, Expectation::All(parts) if parts.iter().any(|p| matches!(p, Expectation::Component(values) if values.iter().any(|(_, v)| v == "absent"))))
        {
            continue;
        }
        let names: Vec<String> = match &case.expectation {
            Expectation::All(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    Expectation::Component(values) => Some(
                        values
                            .iter()
                            .map(|(name, _)| name.clone())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => Vec::new(),
        };
        case.observed.component = names
            .iter()
            .map(|name| {
                let present = case
                    .observed
                    .invocations
                    .iter()
                    .filter(|i| i.declaration.as_deref() == Some("action.subject"))
                    .filter_map(|i| i.result.as_ref())
                    .any(|r| r.field(name).is_some());
                (
                    name.clone(),
                    if present {
                        "present".to_string()
                    } else {
                        "absent".to_string()
                    },
                )
            })
            .collect();
        case.verdict = judge(&case.expectation, &case.observed);
    }
    out
}
