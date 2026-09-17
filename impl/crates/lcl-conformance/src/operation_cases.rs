//! Concrete operation binding, effect and error evidence, shared with reporting.
use crate::{judge, ExecutedCase, Expectation, Observed, Runner};
use lcl_runtime::Value;
use lcl_spec::{json::Json, SpecPackage};
use std::collections::BTreeMap;

mod clauses;

/// One row's fixture: the declarations it needs, and the action that invokes it.
struct Row {
    operation: &'static str,
    declarations: &'static str,
    action: &'static str,
}

/// A `PARAMETER` block, written once.
macro_rules! parameter {
    ($name:literal, $ty:literal, $required:literal, $value:literal) => {
        concat!(
            "\n    PARAMETER:\n        NAME: ",
            $name,
            "\n        TYPE: ",
            $ty,
            "\n        REQUIRED: ",
            $required,
            "\n        VALUE: ",
            $value
        )
    };
}

/// Declarations every fixture may draw on.
const SHARED: &str = concat!(
    "\nDEFINE:\n    ID: type.tagged\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: tag\n        TYPE: STRING\n        REQUIRED: TRUE\n",
    "\nDATA:\n    ID: data.tagged\n    TYPE: OBJECT[REF(type.tagged)]\n    VALUE:\n        tag: \"a\"\n",
    "\nDATA:\n    ID: data.objects\n    TYPE: LIST[OBJECT[REF(type.tagged)]]\n    VALUE: [REF(data.tagged)]\n",
    "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n",
    "\nDATA:\n    ID: data.number\n    TYPE: INTEGER\n    VALUE: 3\n",
    "\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: \"content\"\n",
    "\nDATA:\n    ID: data.list\n    TYPE: LIST[INTEGER]\n    VALUE: [3, 1, 2]\n",
    "\nDATA:\n    ID: data.path\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/report.txt\")\n",
    "\nDATA:\n    ID: data.other\n    TYPE: PATH\n    VALUE: PATH(\"/srv/data/other.txt\")\n",
    "\nDATA:\n    ID: data.uri\n    TYPE: URI\n    VALUE: URI(\"http://example.invalid/x\")\n",
    "\nDATA:\n    ID: data.command\n    TYPE: STRING\n    VALUE: \"report --now\"\n",
    "\nMEMORY:\n    ID: memory.notes\n    TYPE: STRING\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: \"kept\"\n",
    "\nSTATE:\n    ID: state.revision\n    TYPE: INTEGER\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: 2\n",
    "\nACTION:\n    ID: action.other\n    OPERATION: core.return\n    TARGET: REF(data.number)\n",
    // A pure key operation: a contract the document declares and the embedder
    // implements, which is the only way a key REFERENCE can resolve.
    "\nDEFINE:\n    ID: group.identity\n    KIND: kind.operation\n    \
     MEANING: \"Return the member as its own grouping key.\"\n    SIDE_EFFECT: FALSE\n    \
     DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: member\n        TYPE: INTEGER\n        \
     REQUIRED: TRUE\n    RESULT:\n        TYPE: INTEGER\n",
);

/// Reviewed concrete fixtures, initially drawn from the M7 operation examples.
fn rows() -> Vec<Row> {
    vec![
        Row {
            operation: "core.analyze",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.analyze\n    TARGET: REF(data.text)",
                parameter!("criteria", "STRING", "TRUE", "\"tone\"")
            ),
        },
        Row {
            operation: "core.append",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.append\n    TARGET: REF(data.path)",
                parameter!("content", "STRING", "TRUE", "\"more\"")
            ),
        },
        Row {
            operation: "core.ask",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.ask\n    TARGET: REF(data.text)",
                parameter!("question", "STRING", "TRUE", "\"Which environment?\""),
                parameter!("expected_type", "STRING", "TRUE", "\"STRING\"")
            ),
        },
        Row {
            operation: "core.calculate",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.calculate",
                parameter!("expression", "STRING", "TRUE", "\"1 + 2\"")
            ),
        },
        Row {
            operation: "core.cancel",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.cancel\n    TARGET: REF(task.subject)",
                parameter!("reason", "STRING", "TRUE", "\"the owner cancelled it\"")
            ),
        },
        Row {
            operation: "core.compare",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.compare\n    TARGET: REF(data.number)",
                parameter!("against", "INTEGER", "TRUE", "3")
            ),
        },
        Row {
            operation: "core.continue",
            declarations: SHARED,
            action: "OPERATION: core.continue\n    TARGET: REF(action.other)",
        },
        Row {
            operation: "core.convert",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.convert\n    TARGET: REF(data.path)",
                parameter!("target_format", "STRING", "TRUE", "\"format.json\"")
            ),
        },
        Row {
            operation: "core.copy",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.copy\n    TARGET: REF(data.path)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.create",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.create\n    TARGET: REF(data.other)",
                parameter!("content", "STRING", "FALSE", "\"new\"")
            ),
        },
        Row {
            operation: "core.delete",
            declarations: SHARED,
            action: "OPERATION: core.delete\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.download",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.download\n    TARGET: REF(data.uri)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.execute",
            declarations: SHARED,
            action: "OPERATION: core.execute\n    TARGET: REF(data.command)",
        },
        Row {
            operation: "core.filter",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.filter\n    TARGET: REF(data.list)",
                parameter!("predicate", "STRING", "TRUE", "\"item > 1\"")
            ),
        },
        Row {
            operation: "core.generate",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.generate\n    TARGET: REF(data.other)",
                parameter!("specification", "STRING", "TRUE", "\"a short summary\"")
            ),
        },
        Row {
            operation: "core.group",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.group\n    TARGET: REF(data.objects)",
                parameter!("key", "STRING", "TRUE", "\"tag\"")
            ),
        },
        Row {
            operation: "core.inspect",
            declarations: SHARED,
            action: "OPERATION: core.inspect\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.install",
            declarations: SHARED,
            action: "OPERATION: core.install\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.memory_write",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.memory_write\n    TARGET: REF(memory.notes)",
                parameter!("value", "STRING", "TRUE", "\"written\"")
            ),
        },
        Row {
            operation: "core.modify",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.modify\n    TARGET: REF(data.path)",
                parameter!("change", "STRING", "TRUE", "\"changed\"")
            ),
        },
        Row {
            operation: "core.move",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.move\n    TARGET: REF(data.path)",
                parameter!("destination", "PATH", "TRUE", "REF(data.other)")
            ),
        },
        Row {
            operation: "core.publish",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.publish\n    TARGET: REF(data.path)",
                parameter!("destination", "URI", "TRUE", "REF(data.uri)"),
                parameter!("visibility", "STRING", "TRUE", "\"public\"")
            ),
        },
        Row {
            operation: "core.read",
            declarations: SHARED,
            action: "OPERATION: core.read\n    TARGET: REF(data.path)",
        },
        Row {
            operation: "core.rename",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.rename\n    TARGET: REF(data.path)",
                parameter!("new_name", "STRING", "TRUE", "\"renamed.txt\"")
            ),
        },
        Row {
            operation: "core.report",
            declarations: SHARED,
            action: "OPERATION: core.report\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.retry",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.retry\n    TARGET: REF(action.other)",
                parameter!("limit", "INTEGER", "TRUE", "1")
            ),
        },
        Row {
            operation: "core.return",
            declarations: SHARED,
            action: "OPERATION: core.return\n    TARGET: REF(data.number)",
        },
        Row {
            operation: "core.select",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.select\n    TARGET: REF(data.list)",
                parameter!("predicate", "STRING", "TRUE", "\"item > 1\"")
            ),
        },
        Row {
            operation: "core.send",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.send\n    TARGET: REF(data.text)",
                parameter!("recipient", "URI", "TRUE", "REF(data.uri)")
            ),
        },
        Row {
            operation: "core.sort",
            declarations: SHARED,
            action: "OPERATION: core.sort\n    TARGET: REF(data.list)",
        },
        Row {
            operation: "core.start",
            declarations: SHARED,
            action: "OPERATION: core.start\n    TARGET: REF(data.command)",
        },
        Row {
            operation: "core.state_update",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.state_update\n    TARGET: REF(state.revision)",
                parameter!("value", "INTEGER", "TRUE", "3")
            ),
        },
        Row {
            operation: "core.stop",
            declarations: SHARED,
            action: "OPERATION: core.stop\n    TARGET: REF(task.subject)",
        },
        Row {
            operation: "core.test",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.test",
                parameter!("expected", "INTEGER", "FALSE", "3"),
                parameter!("actual", "INTEGER", "FALSE", "REF(data.number)")
            ),
        },
        Row {
            operation: "core.uninstall",
            declarations: SHARED,
            action: "OPERATION: core.uninstall\n    TARGET: REF(data.text)",
        },
        Row {
            operation: "core.upload",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.upload\n    TARGET: REF(data.path)",
                parameter!("destination", "URI", "TRUE", "REF(data.uri)")
            ),
        },
        Row {
            operation: "core.validate",
            declarations: SHARED,
            action: "OPERATION: core.validate\n    TARGET: REF(data.number)",
        },
        Row {
            operation: "core.verify",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.verify\n    TARGET: REF(data.number)",
                parameter!("assertion", "BOOLEAN", "TRUE", "REF(data.number) == 3")
            ),
        },
        Row {
            operation: "core.write",
            declarations: SHARED,
            action: concat!(
                "OPERATION: core.write\n    TARGET: REF(data.path)",
                parameter!("content", "STRING", "TRUE", "\"content\"")
            ),
        },
    ]
}

/// The document one row's fixture becomes.
fn document(row: &Row) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.coverage\n    \
         NAME: \"Coverage\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n{}\
         \nACTION:\n    ID: action.subject\n    {}\n\
         \nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n\
         \nSUCCESS:\n    ID: success.subject\n    ALL: TRUE\n\
         \nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    \
         ACTION: [REF(action.subject), REF(action.other)]\n    SUCCESS: REF(success.subject)\n\
         \nEXECUTE:\n    REFERENCE: REF(task.subject)\n",
        row.declarations, row.action
    )
}

fn group(id: String, contract: &str, runs: Vec<ExecutedCase>) -> ExecutedCase {
    let source = runs
        .iter()
        .map(|r| format!("SUBCASE {}\n{}", r.id, r.source))
        .collect::<Vec<_>>()
        .join("\n");
    let expectation = Expectation::Runs(runs.iter().map(|r| r.expectation.clone()).collect());
    let observed = Observed {
        run_labels: runs.iter().map(|r| r.id.clone()).collect(),
        runs: runs.into_iter().map(|r| r.observed).collect(),
        ..Observed::default()
    };
    let verdict = judge(&expectation, &observed);
    ExecutedCase {
        id,
        contract: contract.into(),
        source,
        expectation,
        observed,
        verdict,
    }
}
fn component(
    label: &str,
    input: String,
    expected: Vec<(String, String)>,
    actual: Vec<(String, String)>,
) -> ExecutedCase {
    let expectation = Expectation::Component(expected);
    let observed = Observed {
        component: actual,
        input_evidence: vec![input.clone()],
        ..Observed::default()
    };
    let verdict = judge(&expectation, &observed);
    ExecutedCase {
        id: label.into(),
        contract: "production operation component".into(),
        source: input,
        expectation,
        observed,
        verdict,
    }
}
fn host() -> lcl_stdlib::HostAdapter {
    use lcl_stdlib::fixtures::{MemoryProcess, MemoryResponder, MemoryTransport};
    let grants = lcl_capabilities::Grants::internal()
        .permit_write("/srv/data")
        .permit_program("report")
        .permit_network_host("example.invalid")
        .permit_human()
        .permit_packages();
    lcl_stdlib::HostAdapter::new(grants)
        .with_filesystem(
            lcl_stdlib::MemoryFileSystem::new()
                .with_scope("/srv/data")
                .with_file("/srv/data/report.txt", "content"),
        )
        .with_process(
            MemoryProcess::new().with_program("report", MemoryProcess::succeeded("all clear")),
        )
        .with_transport(MemoryTransport::new().with_resource("http://example.invalid/x", "remote"))
        .with_responder(MemoryResponder::new().with_answer("Which environment?", "staging"))
}
fn attempt(field: &str, value: String) -> Expectation {
    Expectation::AttemptField {
        declaration: "action.subject".into(),
        attempt: 0,
        field: field.into(),
        value,
    }
}
fn baseline(runner: &Runner, row: &Row, schema: &str) -> ExecutedCase {
    let precondition = matches!(
        row.operation,
        "core.analyze"
            | "core.convert"
            | "core.generate"
            | "core.install"
            | "core.uninstall"
            | "core.report"
            | "core.retry"
            | "core.continue"
    );
    let mut expected = vec![
        Expectation::Attempts {
            declaration: "action.subject".into(),
            statuses: vec![if precondition {
                "status.failed"
            } else {
                "status.succeeded"
            }
            .into()],
        },
        attempt("schema", schema.into()),
        Expectation::NoDiagnostic("error.operation.parameter".into()),
    ];
    if precondition {
        expected.push(Expectation::Diagnostic(
            "error.operation.precondition".into(),
        ));
        expected.push(attempt("failure_phase", "pre_effect".into()));
        expected.push(attempt("effect_state", "none".into()));
    }
    let source = document(row);
    let mut host = host();
    let mut case = runner.execute_on(
        "exact-binding",
        "real stdlib over bounded in-memory capabilities; absent profiles are explicit",
        &source,
        Expectation::All(expected),
        &mut host,
    );
    case.observed.input_evidence.push("fixture: /srv/data/report.txt=content; program report=all clear; URI http://example.invalid/x=remote; question Which environment?=staging; explicit grants; fresh host for each run".into());
    case.observed.input_evidence.push(format!(
        "actual resolved host requests: {:?}",
        host.requests()
    ));
    case
}
fn canonical_default(json: &Json) -> Option<Value> {
    match json {
        Json::Null => None,
        Json::Bool(v) => Some(Value::Boolean(*v)),
        Json::Number(n) => {
            assert!(
                n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0,
                "canonical numeric default must be an exactly represented integer"
            );
            Some(Value::Integer(
                lcl_checker::numeric::Decimal::parse_integer(&n.to_string())
                    .expect("integer default"),
            ))
        }
        Json::String(s) => Some(Value::Text(s.clone())),
        Json::Array(a) if a.is_empty() => Some(Value::List(Vec::new())),
        Json::Object(o) if o.is_empty() => Some(Value::Object(BTreeMap::new())),
        _ => panic!("unhandled canonical default must be implemented explicitly: {json:?}"),
    }
}
fn defaults(spec: &SpecPackage, operation: &str) -> Vec<ExecutedCase> {
    let contracts = lcl_stdlib::Contracts::load(spec).unwrap();
    let contract = contracts.operation(operation).unwrap();
    let registry = spec
        .registry("operations")
        .and_then(|r| r.get("contracts"))
        .and_then(|r| r.get(operation))
        .unwrap();
    let mut runs = Vec::new();
    for (name, row) in registry
        .get("parameters")
        .and_then(Json::as_object)
        .unwrap()
    {
        let default = canonical_default(row.get("default").unwrap()).map(|value| {
            if row
                .get("type")
                .and_then(Json::as_str)
                .unwrap()
                .starts_with("qualified_identifier(")
            {
                if let Value::Text(text) = value {
                    return Value::Identifier(text);
                }
            }
            value
        });
        let required = row.get("required").and_then(Json::as_bool).unwrap();
        for supplied in [
            None,
            Some(Value::Missing),
            Some(Value::Unknown),
            Some(Value::Null),
            Some(Value::Boolean(false)),
            Some(Value::Text(String::new())),
        ] {
            let mut input = BTreeMap::new();
            if let Some(v) = supplied.clone() {
                input.insert(name.clone(), v);
            }
            let actual = lcl_stdlib::params::with_defaults(contract, &input)
                .get(name)
                .cloned();
            let expected = if !required && (supplied.is_none() || supplied == Some(Value::Missing))
            {
                default.clone().or(supplied.clone())
            } else {
                supplied.clone()
            };
            runs.push(component(
                &format!("default/{name}/{supplied:?}"),
                format!("params::with_defaults({operation}, {input:?}); registered row={row:?}"),
                vec![("resolved".into(), format!("{expected:?}"))],
                vec![("resolved".into(), format!("{actual:?}"))],
            ));
        }
    }
    runs
}

pub fn execute(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    let contracts = lcl_stdlib::Contracts::load(spec).unwrap();
    let runners = clauses::Runners {
        shipped: runner,
        fixture: Runner::with_profiles(
            spec,
            Runner::shipped_profiles()
                .into_iter()
                .chain(clauses::fixture_profiles())
                .collect(),
        )
        .expect("the engine assembles with the D3 fixture profiles"),
        spec,
    };
    let mut out = Vec::new();
    for row in rows() {
        let contract = contracts.operation(row.operation).unwrap();
        let mut runs = vec![baseline(runner, &row, &contract.result_schema)];
        runs.extend(defaults(spec, row.operation));
        runs.extend(clauses::binding(&runners, &row));
        out.push(group(format!("semantic/operation_binding/{}", row.operation), "exact target/parameter binding, per-invocation defaults and completed or explicitly refused registered operation", runs));
        let mut errors = error_cases(runner, &row, contract);
        errors.extend(clauses::errors(&runners, &row));
        errors.extend(clauses::transfer_source_preconditions(&runners, &row));
        errors.extend(clauses::specific(&runners, &row, "errors"));
        errors.extend(clauses::lifecycle(&runners, &row, "errors"));
        out.push(group(
            format!("semantic/operation_errors/{}", row.operation),
            "all named binding failures and concrete operation-specific error paths",
            errors,
        ));
        let mut effects = effect_cases(spec, runner, &row, contract);
        effects.extend(clauses::effects(&runners, &row));
        effects.extend(clauses::specific(&runners, &row, "effects"));
        effects.extend(clauses::lifecycle(&runners, &row, "effects"));
        out.push(group(format!("semantic/operation_effects/{}", row.operation), "observable post-state, resolved effects and complete registered determinism/profile constraints", effects));
    }
    out
}

fn with_action(row: &Row, action: &str) -> String {
    document(row).replacen(
        &format!("ID: action.subject\n    {}", row.action),
        &format!("ID: action.subject\n    {action}"),
        1,
    )
}
fn parameter(name: &str, ty: &str, value: &str) -> String {
    format!("\n    PARAMETER:\n        NAME: {name}\n        TYPE: {ty}\n        REQUIRED: TRUE\n        VALUE: {value}")
}
fn error_cases(
    runner: &Runner,
    row: &Row,
    contract: &lcl_stdlib::OperationContract,
) -> Vec<ExecutedCase> {
    let chunks: Vec<_> = row.action.split("\n    PARAMETER:").collect();
    let mut variants: Vec<(String, String, &str)> = Vec::new();
    let unknown = format!(
        "{}{}",
        row.action,
        parameter("unregistered", "INTEGER", "1")
    );
    variants.push((
        "unregistered-parameter".into(),
        unknown,
        "error.operation.parameter",
    ));
    if contract
        .target
        .as_ref()
        .is_some_and(|target| target.required)
    {
        let action = row
            .action
            .lines()
            .filter(|line| !line.trim_start().starts_with("TARGET:"))
            .collect::<Vec<_>>()
            .join("\n");
        variants.push((
            "required-target".into(),
            action,
            "error.operation.parameter",
        ));
    }
    for (name, parameter) in &contract.parameters {
        if parameter.required {
            let action = chunks
                .iter()
                .enumerate()
                .filter(|(i, chunk)| {
                    *i == 0
                        || !chunk
                            .lines()
                            .any(|line| line.trim() == format!("NAME: {name}"))
                })
                .map(|(i, chunk)| {
                    if i == 0 {
                        chunk.to_string()
                    } else {
                        format!("\n    PARAMETER:{chunk}")
                    }
                })
                .collect::<String>();
            assert_ne!(action, row.action, "fixture must supply required {name}");
            variants.push((
                format!("required-parameter/{name}"),
                action,
                "error.operation.parameter",
            ));
        }
    }
    if chunks.len() > 1 {
        variants.push((
            "duplicate-parameter".into(),
            format!("{}\n    PARAMETER:{}", row.action, chunks[1]),
            "error.operation.parameter",
        ));
        let positional = format!(
            "{}\n    PARAMETER:{}",
            chunks[0],
            chunks[1]
                .lines()
                .filter(|line| !line.trim_start().starts_with("NAME:"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        // NAME is required by the higher-authority grammar/field registry;
        // unnamed parameter syntax therefore stops before operation binding.
        variants.push((
            "positional-earliest-stage".into(),
            positional,
            "error.field.required",
        ));
    } else {
        let duplicate = contract
            .parameters
            .values()
            .next()
            .map(|p| parameter(&p.name, "STRING", "\"x\""));
        if let Some(p) = duplicate {
            variants.push((
                "duplicate-parameter".into(),
                format!("{}{p}{p}", row.action),
                "error.operation.parameter",
            ));
        }
    }
    if row
        .action
        .lines()
        .any(|line| line.trim_start().starts_with("TARGET:"))
    {
        let action = row
            .action
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("TARGET:") {
                    "    TARGET: REF(data.absent)"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        variants.push((
            "unresolved-target".into(),
            action,
            "error.reference.unresolved",
        ));
    }
    for (name, bound) in contract
        .parameters
        .iter()
        .filter_map(|(name, p)| p.bound.as_ref().map(|b| (name, b)))
    {
        for value in [bound.minimum - 1, bound.maximum + 1] {
            let base = chunks
                .iter()
                .enumerate()
                .filter(|(i, c)| {
                    *i == 0 || !c.lines().any(|line| line.trim() == format!("NAME: {name}"))
                })
                .map(|(i, c)| {
                    if i == 0 {
                        c.to_string()
                    } else {
                        format!("\n    PARAMETER:{c}")
                    }
                })
                .collect::<String>();
            variants.push((
                format!("bound/{name}/{value}"),
                format!("{base}{}", parameter(name, "INTEGER", &value.to_string())),
                "error.value.out_of_range",
            ));
        }
    }
    let mut runs: Vec<_> = variants
        .into_iter()
        .map(|(label, action, error)| {
            runner.execute(
                &label,
                "exact canonical operation/source error",
                &with_action(row, &action),
                Expectation::Diagnostic(error.into()),
            )
        })
        .collect();
    if matches!(
        row.operation,
        "core.create"
            | "core.write"
            | "core.append"
            | "core.modify"
            | "core.move"
            | "core.rename"
            | "core.delete"
            | "core.generate"
    ) {
        let action = row
            .action
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("TARGET:") {
                    "    TARGET: REF(memory.notes)"
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        runs.push(runner.execute(
            "forbidden-memory-target",
            "memory writes use core.memory_write",
            &with_action(row, &action),
            Expectation::Diagnostic("error.operation.precondition".into()),
        ));
    }
    runs
}

#[derive(Clone)]
struct SharedFs(std::rc::Rc<std::cell::RefCell<lcl_stdlib::MemoryFileSystem>>);
impl lcl_capabilities::FileSystem for SharedFs {
    fn metadata(
        &mut self,
        p: lcl_capabilities::Location<'_>,
    ) -> Result<lcl_capabilities::Metadata, lcl_capabilities::FsError> {
        self.0.borrow_mut().metadata(p)
    }
    fn read(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        b: &lcl_capabilities::Bounds,
    ) -> Result<Vec<u8>, lcl_capabilities::FsError> {
        self.0.borrow_mut().read(p, b)
    }
    fn write(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        c: &[u8],
        m: lcl_capabilities::WriteMode,
    ) -> Result<(), lcl_capabilities::FsError> {
        self.0.borrow_mut().write(p, c, m)
    }
    fn append(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        c: &[u8],
    ) -> Result<(), lcl_capabilities::FsError> {
        self.0.borrow_mut().append(p, c)
    }
    fn delete(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        r: bool,
    ) -> Result<bool, lcl_capabilities::FsError> {
        self.0.borrow_mut().delete(p, r)
    }
    fn rename(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        q: lcl_capabilities::Location<'_>,
        o: bool,
    ) -> Result<(), lcl_capabilities::FsError> {
        self.0.borrow_mut().rename(p, q, o)
    }
    fn copy(
        &mut self,
        p: lcl_capabilities::Location<'_>,
        q: lcl_capabilities::Location<'_>,
        o: bool,
    ) -> Result<u64, lcl_capabilities::FsError> {
        self.0.borrow_mut().copy(p, q, o)
    }
}
fn profile(
    operation: &str,
    role: lcl_capabilities::Role,
    deterministic: bool,
) -> lcl_capabilities::Profile {
    use lcl_capabilities::{Axes, Determinism, Profile};
    Profile::builder(operation, role, "conformance.fixture", "1")
        .determinism(
            if deterministic {
                Determinism::Deterministic
            } else {
                Determinism::Nondeterministic
            },
            "fixed fixture snapshot; variation category is explicit",
        )
        .axes(Axes::inert())
        .resolving("fixture selects no external dependency or observable effect")
}
fn effect_cases(
    spec: &SpecPackage,
    runner: &Runner,
    row: &Row,
    contract: &lcl_stdlib::OperationContract,
) -> Vec<ExecutedCase> {
    use lcl_capabilities::{
        AddressClass, Axes, Dependency, Determinism, Effect, ProfileCatalog, ProfileFault, Role,
        Selection,
    };
    let filesystem = SharedFs(std::rc::Rc::new(std::cell::RefCell::new(
        lcl_stdlib::MemoryFileSystem::new()
            .with_scope("/srv/data")
            .with_file("/srv/data/report.txt", "content"),
    )));
    let mut host = host().with_filesystem(filesystem.clone());
    let source = document(row);
    let observed = runner.run_on(&source, &lcl_resolver::MemoryProvider::new(), &mut host);
    let actual = observed
        .invocations
        .iter()
        .find(|i| i.declaration.as_deref() == Some("action.subject"))
        .and_then(|i| i.result.as_ref())
        .expect("concrete fixture must enter action.subject");
    let mut expected_files =
        BTreeMap::from([("/srv/data/report.txt".to_string(), b"content".to_vec())]);
    match row.operation {
        "core.append" => {
            expected_files.insert("/srv/data/report.txt".into(), b"contentmore".to_vec());
        }
        "core.modify" => {
            expected_files.insert("/srv/data/report.txt".into(), b"changed".to_vec());
        }
        "core.create" => {
            expected_files.insert("/srv/data/other.txt".into(), b"new".to_vec());
        }
        "core.copy" => {
            expected_files.insert("/srv/data/other.txt".into(), b"content".to_vec());
        }
        "core.delete" => {
            expected_files.clear();
        }
        "core.move" => {
            expected_files.clear();
            expected_files.insert("/srv/data/other.txt".into(), b"content".to_vec());
        }
        "core.rename" => {
            expected_files.clear();
            expected_files.insert("/srv/data/renamed.txt".into(), b"content".to_vec());
        }
        "core.download" => {
            expected_files.insert("/srv/data/other.txt".into(), b"remote".to_vec());
        }
        _ => {}
    }
    let fs = filesystem.0.borrow();
    let actual_files: BTreeMap<_, _> = fs
        .paths()
        .iter()
        .map(|p| {
            (
                p.to_string_lossy().to_string(),
                fs.file(p).unwrap().to_vec(),
            )
        })
        .collect();
    let effects_inside = actual.observed_effects.iter().all(|effect| {
        contract
            .maximum
            .effect_names()
            .iter()
            .any(|name| name == effect.class.as_registry_str())
    });
    let requests_inside = host.requests().iter().all(|request| {
        Axes::from_registry(
            &request
                .possible_dependencies
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            &request.possible_effects.iter().cloned().collect::<Vec<_>>(),
        )
        .within(&contract.maximum)
    });
    let result_contracts = lcl_runtime::Contracts::load(spec).unwrap();
    let values = vec![
        ("filesystem".into(), format!("{actual_files:?}")),
        ("effects_within_contract".into(), effects_inside.to_string()),
        (
            "requests_within_contract".into(),
            requests_inside.to_string(),
        ),
        (
            "result_invariants".into(),
            format!("{:?}", actual.schema_violations(&result_contracts)),
        ),
    ];
    let expected = vec![
        ("filesystem".into(), format!("{expected_files:?}")),
        ("effects_within_contract".into(), "true".into()),
        ("requests_within_contract".into(), "true".into()),
        ("result_invariants".into(), "[]".into()),
    ];
    let mut runs=vec![component("actual-post-state",format!("exact source:\n{source}\ninitial filesystem report.txt=content; fresh bounded fixture host; engine observation={}\nresolved host requests={:?}",observed.serialize(),host.requests()),expected,values)];
    let catalog = ProfileCatalog::load(spec).unwrap();
    let registry = spec
        .registry("operations")
        .and_then(|r| r.get("contracts"))
        .and_then(|r| r.get(row.operation))
        .unwrap();
    let category = registry
        .get("determinism")
        .and_then(|r| r.get("category"))
        .and_then(Json::as_str)
        .unwrap();
    let role = Role::new("conformance_category_input");
    let yes = profile(row.operation, role.clone(), true);
    let no = profile(row.operation, role, false);
    for (label, selected, all_deterministic) in [
        ("none", vec![], true),
        ("fixed", vec![&yes], true),
        ("variable", vec![&no], false),
        ("two-fixed", vec![&yes, &yes], true),
        ("mixed", vec![&yes, &no], false),
    ] {
        for graph in [
            None,
            Some(Determinism::Deterministic),
            Some(Determinism::Nondeterministic),
        ] {
            let graph_fixed = graph == Some(Determinism::Deterministic);
            let expected = match category {
                "deterministic" => true,
                "nondeterministic" => !selected.is_empty() && all_deterministic,
                "inherited" => graph_fixed,
                "derived" => match row.operation {
                    "core.sort" => true,
                    "core.verify" | "core.publish" => !selected.is_empty() && all_deterministic,
                    "core.download" => selected.len() == 2 && all_deterministic,
                    "core.test" => graph.is_none() || graph_fixed,
                    "core.execute" => {
                        if graph.is_some() {
                            graph_fixed
                        } else {
                            !selected.is_empty() && all_deterministic
                        }
                    }
                    _ => panic!("unmapped derived operation {}", row.operation),
                },
                _ => panic!("unmapped category {category}"),
            };
            let actual = catalog
                .resolve_determinism(row.operation, &selected, graph)
                .is_deterministic();
            runs.push(component(&format!("determinism/{label}/{graph:?}"),format!("ProfileCatalog::resolve_determinism({}, {selected:?}, {graph:?}); canonical category={category}",row.operation),vec![("deterministic".into(),expected.to_string())],vec![("deterministic".into(),actual.to_string())]));
        }
    }
    let mut roles = catalog.required_roles(row.operation, Some("non_graph"));
    roles.extend(catalog.required_roles(row.operation, Some("graph")));
    roles.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    roles.dedup();
    for role in roles {
        let valid = profile(row.operation, role.clone(), true);
        let selection = Selection {
            operation: row.operation,
            role: role.clone(),
            target_class: AddressClass::Path,
            implementation: None,
        };
        let scenarios = [
            ("missing", vec![], "missing"),
            ("complete", vec![valid.clone()], "selected"),
            ("ambiguous", vec![valid.clone(), valid.clone()], "ambiguous"),
        ];
        for (label, profiles, expected) in scenarios {
            let candidate = catalog.clone().with_all(profiles.clone());
            let answer = match candidate.select(&selection) {
                Ok(_) => "selected",
                Err(ProfileFault::Missing { .. }) => "missing",
                Err(ProfileFault::Ambiguous { .. }) => "ambiguous",
                Err(ProfileFault::Incomplete { .. }) => "incomplete",
                Err(ProfileFault::OutOfBounds { .. }) => "out_of_bounds",
            };
            runs.push(component(
                &format!("profile/{}/{label}", role.as_str()),
                format!("ProfileCatalog::select({selection:?}); profiles={profiles:?}"),
                vec![("selection".into(), expected.into())],
                vec![("selection".into(), answer.into())],
            ));
        }
        for missing in ["implementation", "version", "source", "resolution"] {
            let mut candidate = valid.clone();
            match missing {
                "implementation" => candidate.implementation_id.clear(),
                "version" => candidate.implementation_version.clear(),
                "source" => candidate.determinism_source.clear(),
                _ => candidate.invocation_resolution.clear(),
            };
            let answer = matches!(
                catalog.clone().with(candidate.clone()).select(&selection),
                Err(ProfileFault::Incomplete { .. })
            );
            runs.push(component(
                &format!("profile/{}/incomplete/{missing}", role.as_str()),
                format!("ProfileCatalog::select({selection:?}); profile={candidate:?}"),
                vec![("incomplete".into(), "true".into())],
                vec![("incomplete".into(), answer.to_string())],
            ));
        }
        if category != "inherited" {
            for effect in Effect::ALL {
                if !contract.maximum.effects.contains(&effect) {
                    let candidate = valid.clone().axes(Axes::inert().with_effect(effect));
                    let answer = matches!(
                        catalog.clone().with(candidate.clone()).select(&selection),
                        Err(ProfileFault::OutOfBounds { .. })
                    );
                    runs.push(component(
                        &format!("profile/{}/forbidden-effect/{effect}", role.as_str()),
                        format!("ProfileCatalog::select({selection:?}); profile={candidate:?}"),
                        vec![("out_of_bounds".into(), "true".into())],
                        vec![("out_of_bounds".into(), answer.to_string())],
                    ));
                }
            }
            for dependency in Dependency::ALL {
                if !contract.maximum.dependencies.contains(&dependency) {
                    let candidate = valid
                        .clone()
                        .axes(Axes::inert().with_dependency(dependency));
                    let answer = matches!(
                        catalog.clone().with(candidate.clone()).select(&selection),
                        Err(ProfileFault::OutOfBounds { .. })
                    );
                    runs.push(component(
                        &format!(
                            "profile/{}/forbidden-dependency/{dependency}",
                            role.as_str()
                        ),
                        format!("ProfileCatalog::select({selection:?}); profile={candidate:?}"),
                        vec![("out_of_bounds".into(), "true".into())],
                        vec![("out_of_bounds".into(), answer.to_string())],
                    ));
                }
            }
        }
        if category == "deterministic" {
            let candidate = valid
                .clone()
                .determinism(Determinism::Nondeterministic, "explicit variation");
            let answer = matches!(
                catalog.clone().with(candidate.clone()).select(&selection),
                Err(ProfileFault::OutOfBounds { .. })
            );
            runs.push(component(
                &format!("profile/{}/nondeterministic-under-fixed", role.as_str()),
                format!("ProfileCatalog::select({selection:?}); profile={candidate:?}"),
                vec![("out_of_bounds".into(), "true".into())],
                vec![("out_of_bounds".into(), answer.to_string())],
            ));
        }
    }
    runs
}
