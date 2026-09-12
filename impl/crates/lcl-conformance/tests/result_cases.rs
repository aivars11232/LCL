//! Real runtime result-boundary probes; supplied observations are explicit input.
mod common;
use lcl_conformance::{judge, ExecutedCase, Expectation, Observed, Runner, Verdict};
use lcl_runtime::{CapabilityOutcome, Contracts, MockHost, Observation, ResultRecord, Value};
use lcl_spec::SpecPackage;

fn source() -> String {
    common::task_document("\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nACTION:\n    ID: action.read\n    OPERATION: core.read\n    TARGET: PATH(\"/case/data.txt\")\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    ACTION: REF(action.read)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n")
}

fn sample(runner: &Runner, label: &str, observation: Observation, valid: bool) -> ExecutedCase {
    let input = format!("host completes core.read with exact observation {observation:?}");
    let mut host = MockHost::new().script("core.read", vec![CapabilityOutcome::Completed(observation)]);
    let expectation = if valid { Expectation::Accepts } else { Expectation::Diagnostic("error.host.constraint".into()) };
    let mut case = runner.execute_on(label, "closed result.value boundary", &source(), expectation, &mut host);
    case.observed.input_evidence.push(input);
    case
}

fn boundary_runs(runner: &Runner) -> Vec<ExecutedCase> {
    let valid = Observation::none().with("value", Value::Text("data".into())).with("evidence", Value::List(Vec::new()));
    let mut missing = valid.clone(); missing.fields.remove("evidence");
    vec![
        sample(runner, "complete", valid.clone(), true),
        sample(runner, "unknown-field", valid.clone().with("invented", Value::Boolean(true)), false),
        sample(runner, "common-field-shadow", valid.clone().with("status", Value::Identifier("status.succeeded".into())), false),
        sample(runner, "required-evidence", missing, false),
        sample(runner, "wrong-evidence-type", valid.with("evidence", Value::Text("untyped".into())), false),
    ]
}

fn group(schema: &str, runs: Vec<ExecutedCase>) -> ExecutedCase {
    let source = runs.iter().map(|r| format!("SUBCASE {}\n{}",r.id,r.source)).collect::<Vec<_>>().join("\n");
    let expectation = Expectation::Runs(runs.iter().map(|r|r.expectation.clone()).collect());
    let observed = Observed { runs: runs.into_iter().map(|r|r.observed).collect(), ..Observed::default() };
    let verdict=judge(&expectation,&observed);
    ExecutedCase { id:format!("semantic/result_schemas/{schema}"),contract:format!("{schema} closure, cardinality, types and conditional outcomes"),source,expectation,observed,verdict }
}

fn integer(n: &str) -> Value {
    let magnitude=lcl_checker::numeric::Decimal::parse_integer(n.trim_start_matches('-')).unwrap();
    Value::Integer(if n.starts_with('-') { magnitude.negated() } else { magnitude })
}

fn record_case(contracts: &Contracts, label: &str, record: ResultRecord, accepted: bool) -> ExecutedCase {
    let source = format!("ResultRecord input to production schema validation: {}",record.serialize());
    let violations = record.schema_violations(contracts);
    let expectation = Expectation::Component(vec![("accepted".into(), accepted.to_string())]);
    let observed = Observed {
        component: vec![("accepted".into(),violations.is_empty().to_string())],
        input_evidence: vec![source.clone(),format!("violations={violations:?}")],
        ..Observed::default()
    };
    let verdict = judge(&expectation,&observed);
    ExecutedCase { id:label.into(),contract:"canonical result schema validation".into(),source,expectation,observed,verdict }
}

fn completed_records() -> Vec<ResultRecord> {
    let empty = || Value::List(Vec::new());
    let address = || Value::Constructed { constructor:"PATH".into(),text:"/case/target".into() };
    vec![
        ResultRecord::new("result.value","status.succeeded").with_field("value",Value::Null).with_field("evidence",empty()),
        ResultRecord::new("result.collection","status.succeeded").with_field("items",empty()).with_field("count",integer("0")),
        ResultRecord::new("result.operation","status.succeeded").with_field("changed",Value::Boolean(false)).with_field("target",address()),
        ResultRecord::new("result.command","status.succeeded").with_field("mode",Value::Identifier("non_graph".into()))
            .with_field("started",Value::Boolean(true)).with_field("completed",Value::Boolean(true)).with_field("exit_code",integer("7"))
            .with_field("stdout",Value::Text(String::new())).with_field("stderr",Value::Text(String::new())),
        ResultRecord::new("result.validation","status.succeeded").with_field("valid",Value::Boolean(true)).with_field("errors",empty()),
        ResultRecord::new("result.verification","status.succeeded").with_field("verified",Value::Boolean(false))
            .with_field("observed",Value::Object(Default::default())).with_field("errors",empty()).with_field("evidence",empty()),
        ResultRecord::new("result.test","status.succeeded").with_field("passed",Value::Boolean(false)).with_field("evidence",empty()),
        ResultRecord::new("result.message","status.succeeded").with_field("delivered",Value::Boolean(false)).with_field("recipient",address()).with_field("message_id",Value::Null),
        ResultRecord::new("result.transfer","status.succeeded").with_field("source",address()).with_field("destination",address())
            .with_field("bytes",Value::Bytes(lcl_checker::numeric::Decimal::parse_integer("0").unwrap())),
    ]
}

pub fn execute(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    let contracts = Contracts::load(spec).unwrap();
    let mut out = Vec::new();
    for base in completed_records() {
        let mut runs = vec![record_case(&contracts,"complete",base.clone(),true)];
        for name in base.fields.keys() {
            let mut absent=base.clone(); absent.fields.remove(name);
            runs.push(record_case(&contracts,&format!("required/{name}"),absent,false));
            runs.push(record_case(&contracts,&format!("missing-type/{name}"),base.clone().with_field(name,Value::Missing),false));
        }
        for name in ["invented","status","output_binding","execution_errors","failure_phase","effect_state","observed_effects"] {
            runs.push(record_case(&contracts,&format!("forbidden/{name}"),base.clone().with_field(name,Value::Null),false));
        }
        let mut bad_status=base.clone();bad_status.status="status.invented".into();
        runs.push(record_case(&contracts,"unregistered-status",bad_status,false));
        let mut bad_error=base.clone();bad_error.execution_errors.push("error.invented".into());bad_error.failure_phase=lcl_runtime::FailurePhase::PreEffect;
        runs.push(record_case(&contracts,"unregistered-execution-error",bad_error,false));
        match base.schema.as_str() {
            "result.value" => {
                runs.extend(boundary_runs(runner));
                for (label,value) in [("false",Value::Boolean(false)),("zero",integer("0")),("empty-string",Value::Text(String::new())),("empty-list",Value::List(vec![])),("null",Value::Null)] {
                    runs.push(record_case(&contracts,label,base.clone().with_field("value",value),true));
                }
                runs.push(record_case(&contracts,"unknown-value",base.clone().with_field("value",Value::Unknown),false));
                runs.push(record_case(&contracts,"evidence-reference",base.clone().with_field("evidence",Value::List(vec![Value::Reference("evidence.sample".into())])),true));
                runs.push(record_case(&contracts,"evidence-wrong-domain",base.clone().with_field("evidence",Value::List(vec![Value::Reference("data.sample".into())])),false));
            }
            "result.collection" => {
                runs.push(record_case(&contracts,"wrong-count",base.clone().with_field("count",integer("1")),false));
                runs.push(record_case(&contracts,"negative-count",base.clone().with_field("count",integer("-1")),false));
                runs.push(record_case(&contracts,"nonempty-count",base.clone().with_field("items",Value::List(vec![integer("1"),integer("1")])).with_field("count",integer("2")),true));
                let mut absent=failed(base.clone());absent.fields.clear();
                runs.push(record_case(&contracts,"failed-before-computation",absent,true));
            }
            "result.operation" => runs.push(record_case(&contracts,"unknown-change",base.clone().with_field("changed",Value::Unknown),true)),
            "result.command" => {
                let graph=ResultRecord::new("result.command","status.succeeded").with_field("mode",Value::Identifier("graph".into()));
                runs.push(record_case(&contracts,"graph-no-primary",graph.clone(),true));
                runs.push(record_case(&contracts,"graph-primary-null",graph.clone().with_field("value",Value::Null),true));
                for (name,value) in &base.fields {
                    if name!="mode" { runs.push(record_case(&contracts,&format!("graph-forbids/{name}"),graph.clone().with_field(name,value.clone()),false)); }
                }
                let not_started=failed(ResultRecord::new("result.command","status.failed").with_field("mode",Value::Identifier("non_graph".into()))
                    .with_field("started",Value::Boolean(false)).with_field("completed",Value::Boolean(false)));
                runs.push(record_case(&contracts,"failure-to-start",not_started.clone(),true));
                for (name,value) in [("exit_code",integer("0")),("stdout",Value::Text(String::new())),("stderr",Value::Text(String::new())),("completed",Value::Boolean(true))] {
                    runs.push(record_case(&contracts,&format!("not-started-forbids/{name}"),not_started.clone().with_field(name,value),false));
                }
                let mut interrupted=base.clone();interrupted.fields.remove("exit_code");interrupted.fields.insert("completed".into(),Value::Boolean(false));
                interrupted.status="status.blocked".into();interrupted.failure_phase=lcl_runtime::FailurePhase::PostEffect;
                interrupted.effect_state=lcl_runtime::EffectState::Partial;
                interrupted.execution_errors.push("error.host.constraint".into());
                interrupted.observed_effects.push(lcl_runtime::ObservedEffect {class:lcl_runtime::EffectClass::Process,state:lcl_runtime::RecordState::Partial,target:None,evidence:vec!["captured stdout and stopped process".into()]});
                runs.push(record_case(&contracts,"interrupted-streams",interrupted.clone(),true));
                runs.push(record_case(&contracts,"interrupted-no-exit-code",interrupted.with_field("exit_code",integer("0")),false));
            }
            "result.validation" => {
                let findings=Value::List(vec![Value::Identifier("error.validation.failed".into())]);
                runs.push(record_case(&contracts,"false-with-finding",base.clone().with_field("valid",Value::Boolean(false)).with_field("errors",findings.clone()),true));
                runs.push(record_case(&contracts,"false-without-finding",base.clone().with_field("valid",Value::Boolean(false)),false));
                runs.push(record_case(&contracts,"true-with-finding",base.clone().with_field("errors",findings),false));
                runs.push(record_case(&contracts,"unknown-not-admitted",base.clone().with_field("valid",Value::Unknown),false));
            }
            "result.verification" => {
                runs.push(record_case(&contracts,"unknown-verification",base.clone().with_field("verified",Value::Unknown),true));
                runs.push(record_case(&contracts,"scalar-observation-forbidden",base.clone().with_field("observed",integer("3")),false));
                runs.push(record_case(&contracts,"registered-domain-finding",base.clone().with_field("errors",Value::List(vec![Value::Identifier("error.verification.failed".into())])),true));
                runs.push(record_case(&contracts,"unregistered-domain-finding",base.clone().with_field("errors",Value::List(vec![Value::Identifier("error.value.constraint".into())])),false));
            }
            "result.test" => {
                runs.push(record_case(&contracts,"unknown-test",base.clone().with_field("passed",Value::Unknown),true));
                runs.push(record_case(&contracts,"comparison-null",base.clone().with_field("expected",Value::Null).with_field("actual",Value::Null),true));
                for name in ["expected","actual"] { runs.push(record_case(&contracts,&format!("comparison-missing-pair/{name}"),base.clone().with_field(name,Value::Null),false)); }
            }
            "result.message" => {
                runs.push(record_case(&contracts,"unknown-delivery",base.clone().with_field("delivered",Value::Unknown),true));
                runs.push(record_case(&contracts,"known-id",base.clone().with_field("message_id",Value::Text("message-7".into())),true));
                runs.push(record_case(&contracts,"unknown-id-forbidden",base.clone().with_field("message_id",Value::Unknown),false));
            }
            "result.transfer" => {
                runs.push(record_case(&contracts,"unknown-bytes",base.clone().with_field("bytes",Value::Unknown),true));
                runs.push(record_case(&contracts,"negative-bytes",base.clone().with_field("bytes",Value::Bytes(lcl_checker::numeric::Decimal::parse_integer("1").unwrap().negated())),false));
                runs.push(record_case(&contracts,"checksum-known-absent",base.clone().with_field("checksum",Value::Null),true));
                runs.push(record_case(&contracts,"checksum-text",base.clone().with_field("checksum",Value::Text("sha256:sample".into())),true));
                let mut no_transfer=failed(base.clone());no_transfer.fields.remove("bytes");
                runs.push(record_case(&contracts,"before-transfer",no_transfer.clone(),true));
                runs.push(record_case(&contracts,"before-transfer-no-bytes",no_transfer.with_field("bytes",base.fields["bytes"].clone()),false));
            }
            _ => unreachable!(),
        }
        out.push(group(&base.schema,runs));
    }
    out
}

fn failed(mut record: ResultRecord) -> ResultRecord {
    record.status="status.failed".into();
    record.execution_errors=vec!["error.execution.action".into()];
    record.failure_phase=lcl_runtime::FailurePhase::PreEffect;
    record
}

#[test]
fn result_schema_obligations_execute() {
    let runner=common::runner();
    let cases=execute(common::spec(),&runner);
    let mut failed=Vec::new();
    for case in &cases {
        if let Expectation::Runs(expected)=&case.expectation {
            for (i,(expected,actual)) in expected.iter().zip(&case.observed.runs).enumerate() {
                if judge(expected,actual)==Verdict::Failed { eprintln!("FAIL {} subcase {i}: {} ; primary={:?}",case.id,expected.serialize(),actual.primary); }
            }
        }
        if case.verdict!=Verdict::Passed { failed.push(case.id.clone()); }
    }
    println!("result schema groups: {}; sub-runs: {}",cases.len(),cases.iter().map(|c|c.observed.runs.len()).sum::<usize>());
    assert!(failed.is_empty(),"failed schemas: {failed:?}");
}
