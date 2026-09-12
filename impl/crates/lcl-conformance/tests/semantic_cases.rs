//! Concrete additional semantic obligations, shared by acceptance and reporting.
//! A row may require multiple runs; each exact input and observation is retained.

mod common;

use lcl_conformance::{judge, ExecutedCase, Expectation, Observed, Runner, Verdict};
use lcl_spec::SpecPackage;

fn group(id: &str, contract: &str, runs: Vec<ExecutedCase>) -> ExecutedCase {
    let source = runs.iter().map(|r| format!("SUBCASE {}\n{}", r.id, r.source)).collect::<Vec<_>>().join("\n");
    let expectation = Expectation::Runs(runs.iter().map(|r| r.expectation.clone()).collect());
    let observed = Observed { runs: runs.into_iter().map(|r| r.observed).collect(), ..Observed::default() };
    let verdict = judge(&expectation, &observed);
    ExecutedCase { id: id.into(), contract: contract.into(), source, expectation, observed, verdict }
}

fn run(runner: &Runner, label: &str, source: &str, expectation: Expectation) -> ExecutedCase {
    runner.execute(label, "concrete canonical semantic subcase", source, expectation)
}

fn check(runner: &Runner, label: &str, declarations: &str, expression: &str) -> ExecutedCase {
    run(runner, label, &common::assertion_task(declarations, expression), Expectation::All(vec![
        Expectation::Accepts, Expectation::Check { id: "verify.case".into(), outcome: "TRUE".into() },
    ]))
}

fn data(id: &str, ty: &str, value: &str) -> String {
    format!("\nDATA:\n    ID: data.{id}\n    TYPE: {ty}\n    VALUE:{}{value}\n", if value.starts_with('\n') { "" } else { " " })
}

const ENUM: &str = "\nDEFINE:\n    ID: type.choice\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: first\n    ITEM: second\n";
const REFERENCE: &str = "\nDEFINE:\n    ID: constant.original\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 7\n";

pub fn execute_types(runner: &Runner) -> Vec<ExecutedCase> {
    let types: Vec<(&str, &str, &str, Vec<(&str, &str)>, &str)> = vec![
        ("STRING", "STRING", "", vec![("\"\"", "\"\""), ("\"é\\n\\\"x\\\"\"", "\"é\\n\\\"x\\\"\"")], "1"),
        ("INTEGER", "INTEGER", "", vec![("0", "0"), ("-123456789012345678901234567890", "-123456789012345678901234567890")], "TRUE"),
        ("DECIMAL", "DECIMAL", "", vec![("1.250", "1.25"), ("-0.001", "-0.001")], "TRUE"),
        ("BOOLEAN", "BOOLEAN", "", vec![("TRUE", "TRUE"), ("FALSE", "FALSE")], "1"),
        ("NULL", "NULL", "", vec![("NULL", "NULL")], "1"),
        ("LIST[T]", "LIST[INTEGER]", "", vec![("[]", "[]"), ("[3, 1, 1]", "[3, 1, 1]"), ("[\n        3,\n        1,\n        1\n    ]", "[3, 1, 1]")], "[TRUE]"),
        ("SET[T]", "SET[INTEGER]", "", vec![("[]", "[]"), ("[3, 1, 1]", "[1, 3]"), ("[\n        3,\n        1,\n        1\n    ]", "[1, 3]")], "[TRUE]"),
        ("OBJECT", "OBJECT", "", vec![("\n        key: 1", "\n        key: 1"), ("\n        name: \"value\"\n        flag: FALSE", "\n        flag: FALSE\n        name: \"value\"")], "1"),
        ("ENUM", "REF(type.choice)", ENUM, vec![("first", "first"), ("second", "second")], "1"),
        ("REFERENCE[T]", "REFERENCE[REF(constant.original)]", REFERENCE, vec![("REF(constant.original)", "REF(constant.original)")], "1"),
        ("PATH", "PATH", "", vec![("PATH(\"/case/a\")", "PATH(\"/case/a\")")], "1"),
        ("URI", "URI", "", vec![("URI(\"https://example.invalid/x\")", "URI(\"https://example.invalid/x\")")], "1"),
        ("GLOB", "GLOB", "", vec![("GLOB(\"*.txt\")", "GLOB(\"*.txt\")")], "1"),
        ("REGEX", "REGEX", "", vec![("REGEX(\"a+\")", "REGEX(\"a+\")")], "1"),
        ("DATE", "DATE", "", vec![("DATE(\"2024-02-29\")", "DATE(\"2024-02-29\")")], "1"),
        ("TIME", "TIME", "", vec![("TIME(\"12:00:00+01:00\")", "TIME(\"11:00:00Z\")"), ("TIME(\"11:00:00\")", "TIME(\"11:00:00Z\")")], "1"),
        ("DATETIME", "DATETIME", "", vec![("DATETIME(\"2026-01-01T00:00:00+01:00\")", "DATETIME(\"2025-12-31T23:00:00Z\")")], "1"),
        ("DURATION", "DURATION", "", vec![("DURATION(0, unit.second)", "DURATION(0, unit.second)"), ("DURATION(1, unit.minute)", "DURATION(60, unit.second)")], "1"),
        ("PERCENTAGE", "PERCENTAGE", "", vec![("PERCENTAGE(0)", "PERCENTAGE(0)"), ("PERCENTAGE(100)", "PERCENTAGE(100)")], "1"),
        ("BYTES", "BYTES", "", vec![("BYTES(0)", "BYTES(0)"), ("BYTES(123456789012345678901234567890)", "BYTES(123456789012345678901234567890)")], "1"),
        ("MEASURE", "MEASURE", "", vec![("MEASURE(-1.25, unit.second)", "MEASURE(-1.25, unit.second)")], "1"),
    ];
    let mut out = Vec::new();
    for (subject, ty, declarations, forms, invalid) in types {
        let mut runs = Vec::new();
        for (index, (input, expected)) in forms.iter().enumerate() {
            let decl = format!("{declarations}{}{}", data("actual", ty, input), data("expected", ty, expected));
            runs.push(check(runner, &format!("form/{index}"), &decl, "REF(data.actual) == REF(data.expected)"));
        }
        out.push(group(&format!("semantic/type_valid/{subject}"), "types: every admitted canonical value form", runs));
        let decl = format!("{declarations}{}", data("bad", ty, invalid));
        // The closed diagnostic names a member outside its receiving T as heterogeneous.
        let error = if matches!(subject, "LIST[T]" | "SET[T]") { "error.collection.heterogeneous" } else { "error.type.mismatch" };
        out.push(group(&format!("semantic/type_invalid/{subject}"), "types: incompatible receiving value", vec![run(runner, "incompatible", &common::assertion_task(&decl, "TRUE"), Expectation::Diagnostic(error.into()))]));
    }
    out
}

pub fn execute(spec: &SpecPackage, runner: &Runner) -> Vec<ExecutedCase> {
    let mut out = execute_types(runner);
    out.extend(execute_functions(runner));
    out.extend(execute_operators(runner));
    out.extend(execute_statuses_and_errors(spec));
    out
}

#[test]
fn additional_semantic_cases_execute() {
    let runner = common::runner();
    let cases = execute(common::spec(), &runner);
    let mut failed = Vec::new();
    for case in &cases {
        if case.verdict != Verdict::Passed {
            eprintln!("{}", case.serialize());
            for (i, (expectation, observed)) in match &case.expectation { Expectation::Runs(runs) => runs, _ => unreachable!() }.iter().zip(&case.observed.runs).enumerate() {
                if judge(expectation, observed) != Verdict::Passed { eprintln!("FAIL {} subcase {i}: {} ; stage={} primary={:?}", case.id, expectation.serialize(), observed.reached, observed.primary); }
            }
            failed.push(case.id.clone());
        }
    }
    println!("additional semantic groups: {} ; sub-runs: {}", cases.len(), cases.iter().map(|c| c.observed.runs.len()).sum::<usize>());
    assert!(failed.is_empty(), "failed semantic obligations: {failed:?}");
}

fn ordered_values() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("INTEGER", "1", "2"), ("DECIMAL", "1.25", "2.5"), ("STRING", "\"Z\"", "\"a\""),
        ("DATE", "DATE(\"2024-01-01\")", "DATE(\"2024-01-02\")"),
        ("TIME", "TIME(\"10:00:00Z\")", "TIME(\"12:00:00+01:00\")"),
        ("DATETIME", "DATETIME(\"2024-01-01T00:00:00Z\")", "DATETIME(\"2024-01-01T02:00:00+01:00\")"),
        ("DURATION", "DURATION(30, unit.second)", "DURATION(1, unit.minute)"),
        ("PERCENTAGE", "PERCENTAGE(1)", "PERCENTAGE(2)"), ("BYTES", "BYTES(1)", "BYTES(2)"),
        ("MEASURE", "MEASURE(1, unit.meter)", "MEASURE(2, unit.meter)"),
    ]
}

fn expression_group(runner: &Runner, category: &str, subject: &str, expressions: Vec<(String, String)>) -> ExecutedCase {
    group(&format!("semantic/{category}/{subject}"), "operators_and_functions: registered overloads and outcomes",
        expressions.iter().enumerate().map(|(i, (decl, expr))| check(runner, &format!("overload/{i}"), decl, expr)).collect())
}

pub fn execute_functions(runner: &Runner) -> Vec<ExecutedCase> {
    let mut out = Vec::new();
    for name in ["ABS", "COUNT", "SUM", "MIN", "MAX", "ROUND", "EXISTS", "EMPTY", "ALL", "ANY", "NONE"] {
        let mut expressions: Vec<(String, String)> = Vec::new();
        match name {
            "ABS" => for expr in ["ABS(-3) == 3", "ABS(-1.25) == 1.25", "ABS(DURATION(1, unit.minute)) == DURATION(60, unit.second)", "ABS(MEASURE(-2, unit.meter)) == MEASURE(2, unit.meter)"] { expressions.push((String::new(), expr.into())); },
            "ROUND" => for expr in ["ROUND(2.45, 1) == 2.4", "ROUND(2.55, 1) == 2.6", "ROUND(-2.45, 1) == -2.4", "ROUND(MEASURE(1.25, unit.meter), 1) == MEASURE(1.2, unit.meter)", "ROUND(10 / 3, 2) == 3.33"] { expressions.push((String::new(), expr.into())); },
            "COUNT" | "EMPTY" => {
                for (ty, value, count) in [("STRING", "\"é😀\"", 2), ("STRING", "\"\"", 0), ("LIST[INTEGER]", "[1, 1]", 2), ("LIST[INTEGER]", "[]", 0), ("SET[INTEGER]", "[1, 1]", 1), ("SET[INTEGER]", "[]", 0), ("OBJECT", "\n        a: FALSE\n        b: 0", 2), ("BYTES", "BYTES(3)", 3), ("BYTES", "BYTES(0)", 0)] {
                    let expected = if name == "COUNT" { count.to_string() } else { if count == 0 { "TRUE" } else { "FALSE" }.into() };
                    expressions.push((data("arg", ty, value), format!("{name}(REF(data.arg)) == {expected}")));
                }
            }
            "SUM" => for collection in ["LIST", "SET"] {
                for (ty, values, total) in [("INTEGER", "[1, 2]", "3"), ("DECIMAL", "[1.25, 2.5]", "3.75"), ("DURATION", "[DURATION(30, unit.second), DURATION(1, unit.minute)]", "DURATION(90, unit.second)"), ("MEASURE", "[MEASURE(1, unit.meter), MEASURE(2, unit.meter)]", "MEASURE(3, unit.meter)")] {
                    expressions.push((data("arg", &format!("{collection}[{ty}]"), values), format!("SUM(REF(data.arg)) == {total}")));
                }
            },
            "MIN" | "MAX" => for collection in ["LIST", "SET"] { for (ty, low, high) in ordered_values() {
                expressions.push((data("arg", &format!("{collection}[{ty}]"), &format!("[{high}, {low}]")), format!("{name}(REF(data.arg)) == {}", if name == "MIN" { low } else { high })));
            } },
            "EXISTS" => for expr in ["EXISTS(MISSING) == FALSE", "EXISTS(UNKNOWN) == TRUE", "EXISTS(NULL) == TRUE", "EXISTS(FALSE) == TRUE", "EXISTS(0) == TRUE", "EXISTS(\"\") == TRUE"] { expressions.push((String::new(), expr.into())); },
            "ALL" | "ANY" | "NONE" => for (values, all, any, none) in [("[]", "TRUE", "FALSE", "TRUE"), ("[TRUE]", "TRUE", "TRUE", "FALSE"), ("[FALSE]", "FALSE", "FALSE", "TRUE"), ("[UNKNOWN]", "UNKNOWN", "UNKNOWN", "UNKNOWN"), ("[TRUE, UNKNOWN]", "UNKNOWN", "TRUE", "FALSE"), ("[FALSE, UNKNOWN]", "FALSE", "UNKNOWN", "UNKNOWN")] {
                expressions.push((String::new(), format!("{name}({values}) == {}", match name { "ALL" => all, "ANY" => any, _ => none })));
            },
            _ => unreachable!(),
        }
        out.push(expression_group(runner, "function_valid", name, expressions));
        let invalid = if name == "EXISTS" { "EXISTS(1, 2)".to_string() } else if name == "ROUND" { "ROUND(TRUE, 1)".into() } else { format!("{name}(1, 2)") };
        let mut negatives = vec![run(runner, "invalid-arity-or-family", &common::assertion_task("", &format!("EXISTS({invalid})")), Expectation::Diagnostic("error.operator.operand".into()))];
        if matches!(name, "SUM" | "MIN" | "MAX") {
            negatives.push(run(runner, "empty-typed-collection", &common::assertion_task(&data("empty", "LIST[INTEGER]", "[]"), &format!("EXISTS({name}(REF(data.empty)))")), Expectation::Diagnostic("error.operator.operand".into())));
        }
        out.push(group(&format!("semantic/function_invalid/{name}"), "functions: reject unregistered signatures and empty reductions", negatives));
    }
    out
}

pub fn execute_operators(runner: &Runner) -> Vec<ExecutedCase> {
    let mut out = Vec::new();
    for name in ["NOT", "unary -", "*", "/", "+", "-", "==", "!=", "<", "<=", ">", ">=", "IN", "CONTAINS", "MATCHES", "AND", "OR", "property access", "index access"] {
        let mut expressions: Vec<(String, String)> = Vec::new();
        let plain: Vec<&str> = match name {
            "NOT" => vec!["NOT TRUE == FALSE", "NOT FALSE == TRUE", "NOT UNKNOWN == UNKNOWN"],
            "unary -" => vec!["-3 == -3", "-1.25 == -1.25", "-MEASURE(2, unit.meter) == MEASURE(-2, unit.meter)"],
            "IN" => vec!["1 IN [1, 2]", "(3 IN [1, 2]) == FALSE"],
            "CONTAINS" => vec!["\"éclair\" CONTAINS \"clair\"", "\"abc\" CONTAINS \"\"", "[1, 2] CONTAINS 1", "([1, 2] CONTAINS 3) == FALSE"],
            "MATCHES" => vec!["\"abc\" MATCHES REGEX(\"a.*\")", "(\"xabc\" MATCHES REGEX(\"a.*\")) == FALSE", "\"a.txt\" MATCHES GLOB(\"*.txt\")", "(PATH(\"/case/a.txt\") MATCHES GLOB(\"*.txt\")) == FALSE"],
            "property access" | "index access" | "AND" | "OR" | "==" | "!=" | "*" | "/" | "+" | "-" | "<" | "<=" | ">" | ">=" => vec![],
            _ => unreachable!(),
        };
        expressions.extend(plain.into_iter().map(|e| (String::new(), e.into())));
        match name {
            "*" | "/" | "+" | "-" => {
                for left in ["6", "6.0"] { for right in ["2", "2.0"] {
                    let expected = match name { "*" => "12", "/" => "3.0", "+" => "8", _ => "4" };
                    expressions.push((String::new(), format!("({left} {name} {right}) == {expected}")));
                } }
                for numeric in ["2", "2.0"] {
                    let expected = if name == "*" { "12" } else { "3" };
                    if matches!(name, "*" | "/") { expressions.push((String::new(), format!("(MEASURE(6, unit.meter) {name} {numeric}) == MEASURE({expected}, unit.meter)"))); }
                    if name == "*" { expressions.push((String::new(), format!("({numeric} * MEASURE(6, unit.meter)) == MEASURE(12, unit.meter)"))); }
                }
                if name == "/" { expressions.push((String::new(), "(MEASURE(6, unit.meter) / MEASURE(2, unit.meter)) == 3.0".into())); }
                if matches!(name, "+" | "-") { for ty in ["DURATION", "MEASURE"] { expressions.push((String::new(), format!("({ty}(6, unit.second) {name} {ty}(2, unit.second)) == {ty}({}, unit.second)", if name == "+" { 8 } else { 4 }))); } }
            }
            "<" | "<=" | ">" | ">=" => for (_, low, high) in ordered_values() {
                let (left, right) = if name.starts_with('<') { (low, high) } else { (high, low) };
                expressions.push((String::new(), format!("{left} {name} {right}")));
                expressions.push((String::new(), format!("({low} {name} {low}) == {}", if name.ends_with('=') { "TRUE" } else { "FALSE" })));
            },
            "==" | "!=" => {
                for (left, right, equal) in [("1", "1.0", true), ("TRUE", "1", false), ("NULL", "NULL", true), ("MISSING", "MISSING", true), ("UNKNOWN", "UNKNOWN", true), ("MISSING", "UNKNOWN", false), ("UNKNOWN", "1", false), ("MEASURE(1, unit.meter)", "MEASURE(1, unit.kilometer)", false), ("MEASURE(1, unit.second)", "DURATION(1, unit.second)", false), ("REGEX(\"a\")", "REGEX(\"[a]\")", false)] {
                    expressions.push((String::new(), format!("({left} {name} {right}) == {}", if equal == (name == "==") { "TRUE" } else { "FALSE" })));
                }
            }
            "AND" | "OR" => for left in ["TRUE", "FALSE", "UNKNOWN"] { for right in ["TRUE", "FALSE", "UNKNOWN"] {
                let expected = if name == "AND" { if left == "FALSE" || right == "FALSE" { "FALSE" } else if left == "UNKNOWN" || right == "UNKNOWN" { "UNKNOWN" } else { "TRUE" } } else if left == "TRUE" || right == "TRUE" { "TRUE" } else if left == "UNKNOWN" || right == "UNKNOWN" { "UNKNOWN" } else { "FALSE" };
                expressions.push((String::new(), format!("({left} {name} {right}) == {expected}")));
            } },
            "IN" | "CONTAINS" => {
                for collection in ["LIST", "SET"] { for (values, expected) in [("[1, 1]", "TRUE"), ("[]", "FALSE")] {
                    let expr = if name == "IN" { "1 IN REF(data.arg)" } else { "REF(data.arg) CONTAINS 1" };
                    expressions.push((data("arg", &format!("{collection}[INTEGER]"), values), format!("({expr}) == {expected}")));
                } }
                if name == "CONTAINS" { expressions.push((data("arg", "OBJECT", "\n        a: FALSE"), "(REF(data.arg) CONTAINS \"a\") AND NOT (REF(data.arg) CONTAINS \"b\")".into())); }
            }
            "property access" => {
                expressions.push((data("arg", "OBJECT", "\n        a: 3"), "REF(data.arg).a == 3".into()));
                expressions.push((data("arg", "OBJECT", "\n        a: 3"), "REF(data.arg).absent == MISSING".into()));
                expressions.push((data("arg", "INTEGER", "3"), "REF(data.arg).VALUE == 3".into()));
            }
            "index access" => {
                for (index, expected) in [("0", "3"), ("1", "4"), ("-1", "MISSING"), ("2", "MISSING")] { expressions.push((data("arg", "LIST[INTEGER]", "[3, 4]"), format!("REF(data.arg)[{index}] == {expected}"))); }
                expressions.push((data("arg", "OBJECT", "\n        a: 3"), "REF(data.arg)[\"a\"] == 3".into()));
                expressions.push((data("arg", "OBJECT", "\n        a: 3"), "REF(data.arg)[\"absent\"] == MISSING".into()));
            }
            _ => {}
        }
        if matches!(name, "property access" | "index access") {
            let select = if name == "property access" { "(REF(data.forward).nested).answer" } else { "REF(data.forward)[\"nested\"][\"answer\"]" };
            let decl = format!("{}{}", data("forward", "OBJECT", "REF(data.original)"), data("original", "OBJECT", "\n        nested:\n            answer: 42"));
            expressions.push((decl, format!("{select} == 42")));
        }
        out.push(expression_group(runner, "operator_valid", name, expressions));
        let invalid = match name {
            "NOT" => "NOT 1".into(), "unary -" => "-TRUE".into(),
            "IN" => "1 IN TRUE".into(), "CONTAINS" => "TRUE CONTAINS 1".into(),
            "MATCHES" => "1 MATCHES REGEX(\"a\")".into(),
            "property access" => "REF(data.arg).a".into(), "index access" => "\"abc\"[0]".into(),
            "==" | "!=" => format!("REF(data.arg).TYPE {name} 1"),
            _ => format!("TRUE {name} 1"),
        };
        let decl = format!("{ENUM}{}", data("arg", "INTEGER", "1"));
        out.push(group(&format!("semantic/operator_invalid/{name}"), "operators: nonmaterial type designators and unregistered operand families", vec![run(runner, "unregistered-operands", &common::assertion_task(&decl, &format!("EXISTS({invalid})")), Expectation::Diagnostic("error.operator.operand".into()))]));
    }
    out
}

#[test]
fn registered_measure_products_execute() {
    let runner = common::runner();
    let case = execute_operators(&runner).into_iter().find(|c| c.id == "semantic/operator_valid/*").unwrap();
    assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
}

#[test]
fn object_selection_keeps_closed_schemas_and_exact_receiving_types() {
    let runner = common::runner();
    let schema = "\nDEFINE:\n    ID: type.closed\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: answer\n        TYPE: INTEGER\n        REQUIRED: TRUE\n";
    let decl = format!("{schema}{}", data("arg", "OBJECT[REF(type.closed)]", "\n        answer: 42"));
    for select in ["REF(data.arg).missing", "REF(data.arg)[\"missing\"]"] {
        let case = run(&runner, "closed-absence", &common::assertion_task(&decl, &format!("{select} == MISSING")), Expectation::Diagnostic("error.operator.operand".into()));
        assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
    }
    let decl = format!("{}{}", data("arg", "OBJECT", "\n        answer: 42"), data("wrong", "STRING", "REF(data.arg).answer"));
    let case = run(&runner, "exact-inferred-type", &common::assertion_task(&decl, "TRUE"), Expectation::Diagnostic("error.type.mismatch".into()));
    assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
    let decl = format!("{}{}", data("arg", "OBJECT", "\n        answer: 42"), data("key", "STRING", "\"answer\""));
    let case = run(&runner, "runtime-varying-key", &common::assertion_task(&decl, "REF(data.arg)[REF(data.key)] == 42"), Expectation::Diagnostic("error.operator.operand".into()));
    assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
}

fn component(label: &str, input: String, expected: Vec<(String, String)>, actual: Vec<(String, String)>) -> ExecutedCase {
    let expectation = Expectation::Component(expected);
    let observed = Observed { component: actual, input_evidence: vec![input.clone()], ..Observed::default() };
    let verdict = judge(&expectation, &observed);
    ExecutedCase { id: label.into(), contract: "production component contract".into(), source: input, expectation, observed, verdict }
}

pub fn execute_statuses_and_errors(spec: &SpecPackage) -> Vec<ExecutedCase> {
    use lcl_diagnostics::DiagnosticRegistry;
    use lcl_spec::json::Json;
    use std::collections::{BTreeMap, VecDeque};
    let canonical = spec.registry("statuses_and_errors").expect("verified status/error registry");
    let registry = DiagnosticRegistry::load(spec).expect("production diagnostic registry");
    let statuses = canonical.get("statuses").and_then(Json::as_object).unwrap();
    // Paths come from the independent canonical oracle. Every step is executed
    // through Lifecycle::transition; a loader or transition defect cannot make
    // an unreachable starting state look tested.
    let mut paths = BTreeMap::from([("status.not_started".to_string(), Vec::<String>::new())]);
    let mut pending = VecDeque::from(["status.not_started".to_string()]);
    while let Some(from) = pending.pop_front() {
        let row = canonical.get("statuses").and_then(|r| r.get(&from)).unwrap();
        for to in row.get("allowed_next").and_then(Json::as_array).unwrap().iter().map(|j| j.as_str().unwrap()) {
            if !paths.contains_key(to) {
                let mut path = paths[&from].clone(); path.push(to.to_string());
                paths.insert(to.to_string(), path); pending.push_back(to.to_string());
            }
        }
    }
    assert_eq!(paths.len(), statuses.len(), "every required status must have a concrete entry path");
    let mut out = Vec::new();
    for (from, row) in statuses {
        let allowed: Vec<_> = row.get("allowed_next").and_then(Json::as_array).unwrap().iter().map(|j| j.as_str().unwrap()).collect();
        let mut runs = Vec::new();
        for to in statuses.iter().map(|(id, _)| id.as_str()).chain(std::iter::once("status.invented")) {
            for root in [false, true] {
                // A root cannot enter skipped. Its refusal is exercised from
                // each other state, while the child exercises that endpoint.
                if root && from == "status.skipped" { continue; }
                let mut life = lcl_runtime::Lifecycle::new(root);
                let mut expected = Vec::new(); let mut actual = Vec::new();
                for step in &paths[from] {
                    let got = life.transition(&registry, step);
                    expected.push((format!("enter/{step}"), "true".into()));
                    actual.push((format!("enter/{step}"), got.is_ok().to_string()));
                }
                let permits = allowed.contains(&to) && !(root && to == "status.skipped");
                expected.push(("permits".into(), permits.to_string()));
                actual.push(("permits".into(), life.permits(&registry, to).to_string()));
                let result = life.transition(&registry, to);
                expected.push(("transition".into(), permits.to_string())); actual.push(("transition".into(), result.is_ok().to_string()));
                expected.push(("current".into(), if permits { to } else { from }.to_string())); actual.push(("current".into(), life.current().into()));
                let final_row = canonical.get("statuses").and_then(|r| r.get(if permits { to } else { from })).unwrap();
                expected.push(("terminal".into(), final_row.get("terminal").and_then(Json::as_bool).unwrap().to_string())); actual.push(("terminal".into(), life.is_terminal(&registry).to_string()));
                runs.push(component(&format!("{to}/root={root}"), format!("Lifecycle::new({root}); path={:?}; transition({to:?})", paths[from]), expected, actual));
            }
        }
        out.push(group(&format!("semantic/status_transition/{from}"), "every allowed and forbidden lifecycle transition; root skip exclusion", runs));
    }
    let lexicon = lcl_lexer::Lexicon::load(spec).unwrap();
    let grammar = lcl_parser::Grammar::load(spec).unwrap();
    let rules = lcl_resolver::Rules::load(spec, &grammar).unwrap();
    let statics = lcl_checker::Contracts::load(spec).unwrap();
    let preflight = lcl_semantics::Contracts::load(spec).unwrap();
    let runtime = lcl_runtime::Contracts::load(spec).unwrap();
    let completion = lcl_completion::Contracts::load(spec).unwrap();
    for (id, row) in canonical.get("errors").and_then(Json::as_object).unwrap() {
        let text = |key| row.get(key).and_then(Json::as_str).unwrap().to_string();
        let expected = vec![("id".into(), id.clone()), ("stage".into(), text("stage")), ("status".into(), text("default_status")),
            ("recoverable".into(), row.get("recoverable_with_declared_handler").and_then(Json::as_bool).unwrap().to_string()),
            ("event".into(), format!("{:?}", row.get("event").and_then(Json::as_str))), ("meaning".into(), text("meaning"))];
        let error = registry.error(id).unwrap();
        let actual = vec![("id".into(), error.id.clone()), ("stage".into(), error.stage.to_string()), ("status".into(), error.default_status.clone()),
            ("recoverable".into(), error.recoverable_with_declared_handler.to_string()), ("event".into(), format!("{:?}", error.event.as_deref())), ("meaning".into(), error.meaning.clone())];
        let mut runs = vec![component("registry-contract", format!("DiagnosticRegistry::load(approved 0.1.0).error({id:?}); canonical oracle={row:?}"), expected, actual)];
        let mut mirrors = Vec::new();
        if let Some(error) = lcl_lexer::LexicalError::from_registry_str(id) { mirrors.push(("lexer", lexicon.error(error).default_status.clone())); }
        if let Some(error) = lcl_parser::GrammarError::from_registry_str(id) { mirrors.push(("parser", grammar.error(error).default_status.clone())); }
        if let Some(error) = lcl_resolver::ResolutionError::from_registry_str(id) { mirrors.push(("resolver", rules.error(error).default_status.clone())); }
        if let Some(error) = lcl_checker::StaticError::from_registry_str(id) { mirrors.push(("checker", statics.error(error).default_status.clone())); }
        if let Some(error) = lcl_semantics::PreflightError::from_registry_str(id) { mirrors.push(("preflight", preflight.error(error).default_status.clone())); }
        if let Some(error) = lcl_runtime::RuntimeError::from_registry_str(id) { mirrors.push(("runtime", runtime.error(error).default_status.clone())); }
        if let Some(error) = lcl_completion::CompletionError::from_registry_str(id) { mirrors.push(("completion", completion.error(error).default_status.clone())); }
        assert!(!mirrors.is_empty(), "{id} has no implementing component");
        for (owner, status) in mirrors { runs.push(component(owner, format!("{owner}.error({id:?}).default_status"), vec![("default_status".into(), text("default_status"))], vec![("default_status".into(), status)])); }
        out.push(group(&format!("semantic/error_contract/{id}"), "registered stage, recoverability, event and default status; implementing stage mirrors", runs));
    }
    out
}
