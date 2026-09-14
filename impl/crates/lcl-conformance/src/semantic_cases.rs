//! Concrete additional semantic obligations, shared by acceptance and reporting.
//! A row may require multiple runs; each exact input and observation is retained.

use crate::{judge, ExecutedCase, Expectation, Observed, Runner};
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use lcl_runtime::MockHost;
use lcl_semantics::{Invocation, Value};
use lcl_spec::SpecPackage;

fn group(id: &str, contract: &str, runs: Vec<ExecutedCase>) -> ExecutedCase {
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
        id: id.into(),
        contract: contract.into(),
        source,
        expectation,
        observed,
        verdict,
    }
}

fn run(runner: &Runner, label: &str, source: &str, expectation: Expectation) -> ExecutedCase {
    runner.execute(
        label,
        "concrete canonical semantic subcase",
        source,
        expectation,
    )
}

fn check(runner: &Runner, label: &str, declarations: &str, expression: &str) -> ExecutedCase {
    run(
        runner,
        label,
        &crate::fixtures::assertion_task(declarations, expression),
        Expectation::All(vec![
            Expectation::Accepts,
            Expectation::Check {
                id: "verify.case".into(),
                outcome: "TRUE".into(),
            },
        ]),
    )
}

fn data(id: &str, ty: &str, value: &str) -> String {
    format!(
        "\nDATA:\n    ID: data.{id}\n    TYPE: {ty}\n    VALUE:{}{value}\n",
        if value.starts_with('\n') { "" } else { " " }
    )
}

const ENUM: &str = "\nDEFINE:\n    ID: type.choice\n    KIND: kind.type\n    BASE: ENUM\n    ITEM: first\n    ITEM: second\n";
const REFERENCE: &str = "\nDEFINE:\n    ID: constant.original\n    KIND: kind.constant\n    TYPE: INTEGER\n    VALUE: 7\n";

/// One type row: the subject, its receiving type, the declarations it needs,
/// its value forms as `(clause label, input, expected)` and one incompatible value.
type TypeRow = (
    &'static str,
    &'static str,
    &'static str,
    Vec<(&'static str, &'static str, &'static str)>,
    &'static str,
);

pub fn execute_types(runner: &Runner) -> Vec<ExecutedCase> {
    let types: Vec<TypeRow> = vec![
        (
            "STRING",
            "STRING",
            "",
            vec![
                ("form/empty", "\"\"", "\"\""),
                ("form/escapes", "\"é\\n\\\"x\\\"\"", "\"é\\n\\\"x\\\"\""),
                ("form/unicode-escape", r#""\u00e9""#, "\"\u{e9}\""),
                (
                    "form/surrogate-pair-escape",
                    r#""\uD83D\uDE00""#,
                    "\"\u{1f600}\"",
                ),
                ("form/escaped-control-scalar", r#""\u0009x""#, r#""\tx""#),
                (
                    "form/multiline-indent",
                    "\"\"\"\n        first\n            second\n\n        third\n    \"\"\"",
                    r#""first\n    second\n\nthird\n""#,
                ),
            ],
            "1",
        ),
        (
            "INTEGER",
            "INTEGER",
            "",
            vec![
                ("form/zero", "0", "0"),
                (
                    "form/unbounded-negative",
                    "-123456789012345678901234567890",
                    "-123456789012345678901234567890",
                ),
            ],
            "TRUE",
        ),
        (
            "DECIMAL",
            "DECIMAL",
            "",
            vec![
                ("form/trailing-zeros-exact", "1.250", "1.25"),
                ("form/negative-fraction", "-0.001", "-0.001"),
            ],
            "TRUE",
        ),
        (
            "BOOLEAN",
            "BOOLEAN",
            "",
            vec![
                ("form/true", "TRUE", "TRUE"),
                ("form/false", "FALSE", "FALSE"),
            ],
            "1",
        ),
        ("NULL", "NULL", "", vec![("form/null", "NULL", "NULL")], "1"),
        (
            "LIST[T]",
            "LIST[INTEGER]",
            "",
            vec![
                ("form/empty", "[]", "[]"),
                ("form/inline-order-and-duplicates", "[3, 1, 1]", "[3, 1, 1]"),
                (
                    "form/multiline",
                    "[\n        3,\n        1,\n        1\n    ]",
                    "[3, 1, 1]",
                ),
            ],
            "[TRUE]",
        ),
        (
            "SET[T]",
            "SET[INTEGER]",
            "",
            vec![
                ("form/empty", "[]", "[]"),
                ("form/inline-duplicates-collapse", "[3, 1, 1]", "[1, 3]"),
                (
                    "form/multiline-duplicates-collapse",
                    "[\n        3,\n        1,\n        1\n    ]",
                    "[1, 3]",
                ),
            ],
            "[TRUE]",
        ),
        (
            "OBJECT",
            "OBJECT",
            "",
            vec![
                ("form/single-field", "\n        key: 1", "\n        key: 1"),
                (
                    "form/field-order-not-semantic",
                    "\n        name: \"value\"\n        flag: FALSE",
                    "\n        flag: FALSE\n        name: \"value\"",
                ),
                (
                    "form/nested",
                    "\n        outer:\n            inner: 1\n            label: \"x\"",
                    "\n        outer:\n            label: \"x\"\n            inner: 1",
                ),
            ],
            "1",
        ),
        (
            "ENUM",
            "REF(type.choice)",
            ENUM,
            vec![
                ("form/first-member", "first", "first"),
                ("form/second-member", "second", "second"),
            ],
            "1",
        ),
        (
            "REFERENCE[T]",
            "REFERENCE[REF(constant.original)]",
            REFERENCE,
            vec![(
                "form/reference",
                "REF(constant.original)",
                "REF(constant.original)",
            )],
            "1",
        ),
        (
            "PATH",
            "PATH",
            "",
            vec![("form/absolute", "PATH(\"/case/a\")", "PATH(\"/case/a\")")],
            "1",
        ),
        (
            "URI",
            "URI",
            "",
            vec![(
                "form/absolute-with-scheme",
                "URI(\"https://example.invalid/x\")",
                "URI(\"https://example.invalid/x\")",
            )],
            "1",
        ),
        (
            "GLOB",
            "GLOB",
            "",
            vec![("form/star", "GLOB(\"*.txt\")", "GLOB(\"*.txt\")")],
            "1",
        ),
        (
            "REGEX",
            "REGEX",
            "",
            vec![("form/plain-pattern", "REGEX(\"a+\")", "REGEX(\"a+\")")],
            "1",
        ),
        (
            "DATE",
            "DATE",
            "",
            vec![(
                "form/leap-day",
                "DATE(\"2024-02-29\")",
                "DATE(\"2024-02-29\")",
            )],
            "1",
        ),
        (
            "TIME",
            "TIME",
            "",
            vec![
                (
                    "form/offset-normalized",
                    "TIME(\"12:00:00+01:00\")",
                    "TIME(\"11:00:00Z\")",
                ),
                (
                    "form/omitted-offset-utc",
                    "TIME(\"11:00:00\")",
                    "TIME(\"11:00:00Z\")",
                ),
                (
                    "form/fraction",
                    "TIME(\"12:00:00.500\")",
                    "TIME(\"12:00:00.5Z\")",
                ),
                (
                    "form/plus-zero-offset",
                    "TIME(\"12:00:00+00:00\")",
                    "TIME(\"12:00:00Z\")",
                ),
            ],
            "1",
        ),
        (
            "DATETIME",
            "DATETIME",
            "",
            vec![
                (
                    "form/offset-normalized",
                    "DATETIME(\"2026-01-01T00:00:00+01:00\")",
                    "DATETIME(\"2025-12-31T23:00:00Z\")",
                ),
                (
                    "form/omitted-offset-utc",
                    "DATETIME(\"2026-01-01T00:00:00\")",
                    "DATETIME(\"2026-01-01T00:00:00Z\")",
                ),
            ],
            "1",
        ),
        (
            "DURATION",
            "DURATION",
            "",
            vec![
                (
                    "form/zero",
                    "DURATION(0, unit.second)",
                    "DURATION(0, unit.second)",
                ),
                (
                    "form/unit-normalization",
                    "DURATION(1, unit.minute)",
                    "DURATION(60, unit.second)",
                ),
                (
                    "form/decimal-magnitude",
                    "DURATION(1.5, unit.minute)",
                    "DURATION(90, unit.second)",
                ),
            ],
            "1",
        ),
        (
            "PERCENTAGE",
            "PERCENTAGE",
            "",
            vec![
                ("form/zero", "PERCENTAGE(0)", "PERCENTAGE(0)"),
                ("form/hundred", "PERCENTAGE(100)", "PERCENTAGE(100)"),
                ("form/decimal", "PERCENTAGE(12.50)", "PERCENTAGE(12.5)"),
            ],
            "1",
        ),
        (
            "BYTES",
            "BYTES",
            "",
            vec![
                ("form/zero", "BYTES(0)", "BYTES(0)"),
                (
                    "form/unbounded",
                    "BYTES(123456789012345678901234567890)",
                    "BYTES(123456789012345678901234567890)",
                ),
            ],
            "1",
        ),
        (
            "MEASURE",
            "MEASURE",
            "",
            vec![
                (
                    "form/negative-decimal-time-unit",
                    "MEASURE(-1.25, unit.second)",
                    "MEASURE(-1.25, unit.second)",
                ),
                (
                    "form/non-time-category-unit",
                    "MEASURE(1920, unit.pixel)",
                    "MEASURE(1920, unit.pixel)",
                ),
            ],
            "1",
        ),
    ];
    let mut out = Vec::new();
    for (subject, ty, declarations, forms, invalid) in types {
        let mut runs = Vec::new();
        for (label, input, expected) in forms {
            let decl = format!(
                "{declarations}{}{}",
                data("actual", ty, input),
                data("expected", ty, expected)
            );
            runs.push(check(
                runner,
                label,
                &decl,
                "REF(data.actual) == REF(data.expected) AND REF(data.actual) != UNKNOWN AND REF(data.actual) != MISSING",
            ));
        }
        runs.extend(valid_forms(runner, subject));
        out.push(group(
            &format!("semantic/type_valid/{subject}"),
            "types: every admitted canonical value form",
            runs,
        ));
        let decl = format!("{declarations}{}", data("bad", ty, invalid));
        // The closed diagnostic names a member outside its receiving T as heterogeneous.
        let error = if matches!(subject, "LIST[T]" | "SET[T]") {
            "error.collection.heterogeneous"
        } else {
            "error.type.mismatch"
        };
        let mut runs = vec![run(
            runner,
            "incompatible",
            &crate::fixtures::assertion_task(&decl, "TRUE"),
            Expectation::Diagnostic(error.into()),
        )];
        runs.extend(rejected_forms(runner, subject, ty, declarations));
        out.push(group(
            &format!("semantic/type_invalid/{subject}"),
            "types: incompatible, malformed or out-of-profile receiving value",
            runs,
        ));
    }
    out
}

const WORKSPACE: &str =
    "\nWORKSPACE:\n    ID: workspace.case\n    PATH: PATH(\"/case\")\n    MODE: mode.read_only\n";

/// A task that iterates one declared SET directly. Each iteration returns
/// whether its member equals `first`, so the first iteration of a two-member
/// SET fixes the whole order. With `before`, a producer reads the SET first.
fn iteration_task(ty: &str, members: &str, first: &str, before: bool) -> String {
    let before = if before {
        "    STEP:\n        ID: step.before\n        ACTION:\n            ID: action.before\n            OPERATION: core.return\n            TARGET: REF(data.members)\n"
    } else {
        ""
    };
    crate::fixtures::task_document(&format!(
        "{}\nGOAL:\n    ID: goal.case\n    ASSERT: TRUE\n\nSEQUENCE:\n    ID: sequence.case\n{before}    FOR EACH item IN REF(data.members):\n        STEP:\n            ID: step.item\n            ACTION:\n                ID: action.item\n                OPERATION: core.return\n                TARGET: REF(item) == {first}\n\nSUCCESS:\n    ID: success.case\n    ALL: TRUE\n\nTASK:\n    ID: task.case\n    GOAL: REF(goal.case)\n    SEQUENCE: REF(sequence.case)\n    SUCCESS: REF(success.case)\n\nEXECUTE:\n    REFERENCE: REF(task.case)\n",
        data("members", ty, members)
    ))
}

/// Direct FOR EACH over a two-member SET whose first iteration must visit `first`.
fn first_iteration(
    runner: &Runner,
    label: &str,
    ty: &str,
    members: &str,
    first: &str,
) -> ExecutedCase {
    run(
        runner,
        label,
        &iteration_task(ty, members, first, false),
        Expectation::All(vec![
            Expectation::Accepts,
            Expectation::AttemptField {
                declaration: "action.item".into(),
                attempt: 0,
                field: "value".into(),
                value: "TRUE".into(),
            },
        ]),
    )
}

/// Valid forms whose canonical property is more than a round trip: decoding
/// without normalization, unbounded exact numbers, direct SET iteration order,
/// contained WORKSPACE paths, and the closed pattern profiles' semantics.
fn valid_forms(runner: &Runner, subject: &str) -> Vec<ExecutedCase> {
    let checked = |label: &str, declarations: &str, assertion: &str| {
        check(runner, label, declarations, assertion)
    };
    let patterns = |ty: &str, forms: &[(&str, &str, &str)]| {
        forms
            .iter()
            .map(|&(label, value, assertion)| {
                checked(
                    label,
                    &data("actual", ty, value),
                    &format!("REF(data.actual) == {value} AND {assertion}"),
                )
            })
            .collect::<Vec<_>>()
    };
    match subject {
        "STRING" => vec![checked(
            "form/no-unicode-normalization",
            &data("actual", "STRING", "\"e\u{301}\""),
            r#"REF(data.actual) != "\u00e9" AND REF(data.actual) == "e\u0301""#,
        )],
        "INTEGER" => vec![checked(
            "form/unbounded-positive",
            &data("actual", "INTEGER", "123456789012345678901234567890"),
            "REF(data.actual) - 1 == 123456789012345678901234567889 AND REF(data.actual) > 9223372036854775807",
        )],
        "DECIMAL" => vec![checked(
            "form/long-fraction-without-rounding",
            &data("actual", "DECIMAL", "1.0000000000000000000000000000001"),
            "REF(data.actual) != 1 AND REF(data.actual) - 1 == 0.0000000000000000000000000000001",
        )],
        "SET[T]" => vec![
            first_iteration(runner, "form/for-each-canonical-order", "SET[INTEGER]", "[2, 1]", "1"),
            first_iteration(
                runner,
                "form/for-each-time-day-displacement",
                "SET[TIME]",
                r#"[TIME("23:30:00-01:00"), TIME("00:45:00Z")]"#,
                r#"TIME("00:45:00Z")"#,
            ),
            first_iteration(
                runner,
                "form/for-each-duration-normalization",
                "SET[DURATION]",
                "[DURATION(1, unit.minute), DURATION(59, unit.second)]",
                "DURATION(59, unit.second)",
            ),
        ],
        "PATH" => {
            let path = r#"PATH(REF(workspace.case), "src/a.txt")"#;
            vec![checked(
                "form/workspace-relative-contained",
                &format!(
                    "{WORKSPACE}{}{}",
                    data("actual", "PATH", path),
                    data("expected", "PATH", path)
                ),
                r#"REF(data.actual) == REF(data.expected) AND REF(data.actual) == PATH(REF(workspace.case), "src/a.txt") AND REF(data.actual) != PATH(REF(workspace.case), "src/b.txt")"#,
            )]
        }
        "GLOB" => patterns(
            "GLOB",
            &[
                ("form/double-star", r#"GLOB("src/**/*.py")"#, r#""src/main.py" MATCHES REF(data.actual) AND "src/a/b/main.py" MATCHES REF(data.actual) AND NOT ("lib/main.py" MATCHES REF(data.actual))"#),
                ("form/question-mark", r#"GLOB("a?.txt")"#, r#""ab.txt" MATCHES REF(data.actual) AND NOT ("a.txt" MATCHES REF(data.actual))"#),
                ("form/character-class", r#"GLOB("[a-c].txt")"#, r#""b.txt" MATCHES REF(data.actual) AND NOT ("d.txt" MATCHES REF(data.actual))"#),
                ("form/negated-class", r#"GLOB("[!a].txt")"#, r#""b.txt" MATCHES REF(data.actual) AND NOT ("a.txt" MATCHES REF(data.actual))"#),
                ("form/escape", r#"GLOB("\\*.txt")"#, r#""*.txt" MATCHES REF(data.actual) AND NOT ("a.txt" MATCHES REF(data.actual))"#),
            ],
        ),
        "REGEX" => patterns(
            "REGEX",
            &[
                ("form/flag-i", r#"REGEX("abc", "i")"#, r#""ABC" MATCHES REF(data.actual) AND NOT ("ABC" MATCHES REGEX("abc"))"#),
                ("form/flag-m", r#"REGEX("a$\n^b", "m")"#, r#""a\nb" MATCHES REF(data.actual) AND NOT ("a\nb" MATCHES REGEX("a$\n^b"))"#),
                ("form/flag-s", r#"REGEX("a.b", "s")"#, r#""a\nb" MATCHES REF(data.actual) AND NOT ("a\nb" MATCHES REGEX("a.b"))"#),
                ("form/flags-canonical-ims", r#"REGEX("a", "ims")"#, r#""A" MATCHES REF(data.actual) AND REGEX("a", "") == REGEX("a")"#),
                ("form/ascii-only-case-folding", r#"REGEX("[a-z]+", "i")"#, r#""ABC" MATCHES REF(data.actual) AND NOT ("É" MATCHES REGEX("é", "i"))"#),
            ],
        ),
        "DURATION" => vec![checked(
            "form/every-time-unit",
            "",
            "DURATION(1000, unit.nanosecond) == DURATION(1, unit.microsecond) AND DURATION(1000, unit.microsecond) == DURATION(1, unit.millisecond) AND DURATION(1000, unit.millisecond) == DURATION(1, unit.second) AND DURATION(60, unit.second) == DURATION(1, unit.minute) AND DURATION(60, unit.minute) == DURATION(1, unit.hour) AND DURATION(24, unit.hour) == DURATION(1, unit.day) AND DURATION(7, unit.day) == DURATION(1, unit.week) AND DURATION(1, unit.nanosecond) != DURATION(1, unit.microsecond)",
        )],
        _ => Vec::new(),
    }
}

/// Rejections of malformed, out-of-domain and out-of-profile values. Each run
/// expects the registered identifier its canonical contract names or, where
/// the contract names only schema-stage rejection, rejection at that stage.
fn rejected_forms(
    runner: &Runner,
    subject: &str,
    ty: &str,
    declarations: &str,
) -> Vec<ExecutedCase> {
    const LITERAL: &str = "error.literal.invalid";
    const OPERAND: &str = "error.operator.operand";
    const RANGE: &str = "error.value.out_of_range";
    let rejects = |id: &str| Expectation::Rejects(id.into());
    let literal = |forms: &[(&'static str, &'static str)]| {
        forms
            .iter()
            .map(|&(label, value)| (label, "", value, rejects(LITERAL)))
            .collect::<Vec<_>>()
    };
    let cases: Vec<(&str, &str, &str, Expectation)> = match subject {
        "STRING" => vec![("malformed", "", r#""\q""#, rejects("error.literal.escape"))],
        "INTEGER" => vec![("malformed", "", "+1", rejects("error.grammar.invalid"))],
        "DECIMAL" => vec![("malformed", "", "+0.5", rejects("error.grammar.invalid"))],
        "BOOLEAN" => vec![("malformed", "", "True", rejects("error.keyword.case"))],
        "NULL" => vec![("malformed", "", "Null", rejects("error.keyword.case"))],
        "OBJECT" => vec![(
            "malformed",
            "",
            "\n        key: 1\n        key: 2",
            rejects("error.field.duplicate"),
        )],
        "ENUM" => vec![(
            "malformed",
            "",
            "First",
            rejects("error.identifier.invalid"),
        )],
        "REFERENCE[T]" => vec![(
            "malformed",
            "",
            r#"REF("constant.original")"#,
            rejects("error.grammar.invalid"),
        )],
        "DURATION" => vec![
            ("malformed", "", "DURATION(5)", rejects(OPERAND)),
            (
                "out-of-domain/negative",
                "",
                "DURATION(-1, unit.second)",
                rejects(RANGE),
            ),
        ],
        "PERCENTAGE" => vec![
            ("malformed", "", r#"PERCENTAGE("90")"#, rejects(OPERAND)),
            (
                "out-of-domain/over-hundred",
                "",
                "PERCENTAGE(100.5)",
                rejects(RANGE),
            ),
        ],
        "BYTES" => vec![
            ("malformed", "", "BYTES(1.5)", rejects(OPERAND)),
            ("out-of-domain/negative", "", "BYTES(-1)", rejects(RANGE)),
        ],
        "LIST[T]" | "SET[T]" => vec![
            (
                "malformed",
                "",
                "[1, 2)",
                rejects("error.delimiter.mismatch"),
            ),
            (
                "item-block",
                "",
                "\n        ITEM: 1",
                Expectation::RejectsAtStage("grammar_or_schema".into()),
            ),
        ],
        "PATH" => vec![
            (
                "variadic",
                "",
                r#"PATH("/case", "a", "b")"#,
                rejects(OPERAND),
            ),
            (
                "relative-outside-import",
                "",
                r#"PATH("relative/a.txt")"#,
                rejects(LITERAL),
            ),
            (
                "workspace-escape",
                WORKSPACE,
                r#"PATH(REF(workspace.case), "../outside.txt")"#,
                rejects(RANGE),
            ),
        ],
        "URI" => literal(&[
            ("malformed", r#"URI("https://exa mple.invalid/")"#),
            ("relative-reference", r#"URI("/relative/x")"#),
        ]),
        "GLOB" => literal(&[
            ("malformed", r#"GLOB("a/[b")"#),
            ("absolute", r#"GLOB("/case/*.txt")"#),
            ("parent-escape", r#"GLOB("../*.txt")"#),
            ("brace-expansion", r#"GLOB("*.{txt,md}")"#),
        ]),
        "REGEX" => literal(&[
            ("lookahead", r#"REGEX("a(?=b)")"#),
            ("lookbehind", r#"REGEX("(?<=a)b")"#),
            ("backreference", r#"REGEX("(a)\\1")"#),
            ("unicode-property-escape", r#"REGEX("\\p{L}")"#),
            ("unlisted-escape", r#"REGEX("\\b")"#),
            ("unknown-flag", r#"REGEX("a", "x")"#),
            ("duplicate-flag", r#"REGEX("a", "ii")"#),
            ("stateful-flag", r#"REGEX("a", "g")"#),
            ("u-flag", r#"REGEX("a", "u")"#),
            ("out-of-order-flags", r#"REGEX("a", "mi")"#),
        ]),
        "DATE" => literal(&[
            ("outside-profile", r#"DATE("2023-02-29")"#),
            ("wrong-spelling", r#"DATE("2026/01/01")"#),
        ]),
        "TIME" => literal(&[
            ("outside-profile", r#"TIME("24:00:00")"#),
            ("leap-second", r#"TIME("23:59:60Z")"#),
            ("minus-zero-offset", r#"TIME("12:00:00-00:00")"#),
        ]),
        "DATETIME" => literal(&[
            ("missing-uppercase-t", r#"DATETIME("2026-01-01t00:00:00Z")"#),
            ("outside-profile", r#"DATETIME("2026-01-01 00:00:00Z")"#),
        ]),
        "MEASURE" => vec![
            (
                "malformed-pair",
                "",
                "MEASURE(unit.pixel, 1920)",
                rejects(OPERAND),
            ),
            (
                "unregistered-unit",
                "",
                "MEASURE(5, unit.furlong)",
                rejects("error.reference.unresolved"),
            ),
        ],
        _ => Vec::new(),
    };
    let mut out: Vec<ExecutedCase> = cases
        .into_iter()
        .map(|(label, extra, value, expectation)| {
            let decl = format!("{declarations}{extra}{}", data("bad", ty, value));
            run(
                runner,
                label,
                &crate::fixtures::assertion_task(&decl, "TRUE"),
                expectation,
            )
        })
        .collect();
    if subject == "SET[T]" {
        // The SET value is valid; only direct iteration over members without a
        // mutual total order is refused, before any iteration runs.
        out.push(run(
            runner,
            "for-each-without-total-order",
            &iteration_task(
                "SET[MEASURE]",
                "[MEASURE(1, unit.pixel), MEASURE(1, unit.second)]",
                "MEASURE(1, unit.pixel)",
                true,
            ),
            Expectation::All(vec![
                Expectation::Rejects("error.type.mismatch".into()),
                Expectation::Attempts {
                    declaration: "action.before".into(),
                    statuses: vec!["status.succeeded".into()],
                },
                Expectation::Attempts {
                    declaration: "action.item".into(),
                    statuses: Vec::new(),
                },
            ]),
        ));
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

fn ordered_values() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("INTEGER", "1", "2"),
        ("DECIMAL", "1.25", "2.5"),
        ("STRING", "\"Z\"", "\"a\""),
        ("DATE", "DATE(\"2024-01-01\")", "DATE(\"2024-01-02\")"),
        ("TIME", "TIME(\"10:00:00Z\")", "TIME(\"12:00:00+01:00\")"),
        (
            "DATETIME",
            "DATETIME(\"2024-01-01T00:00:00Z\")",
            "DATETIME(\"2024-01-01T02:00:00+01:00\")",
        ),
        (
            "DURATION",
            "DURATION(30, unit.second)",
            "DURATION(1, unit.minute)",
        ),
        ("PERCENTAGE", "PERCENTAGE(1)", "PERCENTAGE(2)"),
        ("BYTES", "BYTES(1)", "BYTES(2)"),
        (
            "MEASURE",
            "MEASURE(1, unit.meter)",
            "MEASURE(2, unit.meter)",
        ),
    ]
}

fn expression_group(
    runner: &Runner,
    category: &str,
    subject: &str,
    expressions: Vec<(String, String, String)>,
    specials: Vec<ExecutedCase>,
) -> ExecutedCase {
    let mut runs: Vec<ExecutedCase> = expressions
        .iter()
        .map(|(label, decl, expr)| check(runner, label, decl, expr))
        .collect();
    runs.extend(specials);
    group(
        &format!("semantic/{category}/{subject}"),
        "operators_and_functions: registered overloads and outcomes",
        runs,
    )
}

/// Statically typed MISSING operands: a selection past the end of a one-member
/// LIST is MISSING (`05_SEMANTICS/12`: "Every INTEGER below zero or at least the
/// member count yields MISSING").
fn missing_sources() -> String {
    [
        data("nums", "LIST[INTEGER]", "[1]"),
        data("decs", "LIST[DECIMAL]", "[1.5]"),
        data("flags", "LIST[BOOLEAN]", "[TRUE]"),
        data("strs", "LIST[STRING]", r#"["a"]"#),
        data("lists", "LIST[LIST[INTEGER]]", "[[1]]"),
    ]
    .concat()
}

/// Optional typed INPUTs a run supplies as UNKNOWN. "Reading an existing binding
/// whose value cannot be determined yields UNKNOWN" (`03_TYPES_AND_VALUES/04`).
fn unknown_inputs() -> String {
    // An optional INPUT needs a DEFAULT ("Exactly one VALUE or SOURCE unless
    // optional with DEFAULT"), and DEFAULT replaces only MISSING, so a supplied
    // UNKNOWN is still read as UNKNOWN.
    let inputs: String = [
        ("u", "INTEGER", "0"),
        ("ud", "DECIMAL", "0.5"),
        ("us", "STRING", "\"x\""),
        ("ul", "LIST[INTEGER]", "[0]"),
        ("uo", "OBJECT", "REF(data.template)"),
    ]
    .iter()
    .map(|(id, ty, default)| {
        format!("\nINPUT:\n    ID: input.{id}\n    TYPE: {ty}\n    REQUIRED: FALSE\n    DEFAULT: {default}\n")
    })
    .collect();
    format!("{}{inputs}", data("template", "OBJECT", "\n        a: 1"))
}

/// A closed object schema with one required and one optional field.
const RECORD: &str = "\nDEFINE:\n    ID: type.record\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: a\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    FIELD:\n        NAME: opt\n        TYPE: INTEGER\n        REQUIRED: FALSE\n\nDATA:\n    ID: data.record\n    TYPE: OBJECT[REF(type.record)]\n    VALUE:\n        a: 1\n";

/// A declared inclusive bound that receives a division quotient.
const BOUNDED: &str = "\nDEFINE:\n    ID: type.bounded\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: ratio\n        TYPE: DECIMAL\n        REQUIRED: TRUE\n        MAXIMUM: 1\n\nDATA:\n    ID: data.bounded\n    TYPE: OBJECT[REF(type.bounded)]\n    VALUE:\n        ratio: 3 / 2\n";

/// How one special-value or diagnostic sub-run is observed.
enum Special {
    /// The assertion records TRUE over these declarations.
    Holds(String),
    /// The source is rejected with exactly this registered identifier.
    Rejects(&'static str, String),
    /// The assertion records TRUE while this optional INPUT is supplied as UNKNOWN.
    Unknown(&'static str),
}

struct SpecialRun {
    label: &'static str,
    assertion: String,
    kind: Special,
}

fn special(label: &'static str, assertion: impl Into<String>, kind: Special) -> SpecialRun {
    SpecialRun {
        label,
        assertion: assertion.into(),
        kind,
    }
}

fn special_runs(runner: &Runner, specials: Vec<SpecialRun>) -> Vec<ExecutedCase> {
    specials
        .into_iter()
        .map(|item| match item.kind {
            Special::Holds(declarations) => {
                check(runner, item.label, &declarations, &item.assertion)
            }
            Special::Rejects(id, declarations) => run(
                runner,
                item.label,
                &crate::fixtures::assertion_task(&declarations, &item.assertion),
                Expectation::Rejects(id.into()),
            ),
            Special::Unknown(input) => {
                let source = crate::fixtures::assertion_task(&unknown_inputs(), &item.assertion);
                let observed = runner.run_input(
                    &SourceUnit::new(SourceId::new("case.lcl"), source.as_bytes()),
                    &MemoryProvider::new(),
                    &Invocation::new().with(input, Value::Unknown),
                    &mut MockHost::new(),
                );
                let expectation = Expectation::All(vec![
                    Expectation::Accepts,
                    Expectation::Check {
                        id: "verify.case".into(),
                        outcome: "TRUE".into(),
                    },
                ]);
                let verdict = judge(&expectation, &observed);
                ExecutedCase {
                    id: item.label.into(),
                    contract: "concrete canonical semantic subcase".into(),
                    source,
                    expectation,
                    observed,
                    verdict,
                }
            }
        })
        .collect()
}

/// Special-value, cross-type, unit, address-form and access sub-runs of one
/// operator row, from `operators_and_functions_v0.1.0.json#/evaluation_contract`
/// and `05_SEMANTICS/12`.
fn operator_specials(name: &str) -> Vec<SpecialRun> {
    let missing = |assertion: String| {
        special(
            "special/missing",
            assertion,
            Special::Rejects("error.required.missing", missing_sources()),
        )
    };
    let unknown = |assertion: String, input: &'static str| {
        special("special/unknown", assertion, Special::Unknown(input))
    };
    match name {
        "NOT" => vec![missing("NOT REF(data.flags)[5]".into())],
        "unary -" => vec![
            missing("-REF(data.nums)[5] == 1".into()),
            unknown("(-REF(input.u)) == UNKNOWN".into(), "input.u"),
        ],
        "*" | "/" | "+" | "-" => {
            let mut runs = vec![
                missing(format!("REF(data.nums)[5] {name} 2 == 1")),
                unknown(format!("(REF(input.u) {name} 2) == UNKNOWN"), "input.u"),
            ];
            if name == "-" {
                runs.push(special(
                    "constraint/negative-duration-result",
                    "DURATION(1, unit.second) - DURATION(2, unit.second) == DURATION(0, unit.second)",
                    Special::Rejects("error.value.out_of_range", String::new()),
                ));
            }
            runs
        }
        "==" | "!=" => {
            let (unequal, equal) = if name == "==" {
                ("FALSE", "TRUE")
            } else {
                ("TRUE", "FALSE")
            };
            vec![special(
                "equality/path-address-form-identity",
                format!(
                    r#"(PATH(REF(workspace.case), "a.txt") {name} PATH("/case/a.txt")) == {unequal} AND (PATH("/a/../b") {name} PATH("/b")) == {unequal} AND (PATH(REF(workspace.case), "a.txt") {name} PATH(REF(workspace.case), "a.txt")) == {equal}"#
                ),
                Special::Holds(WORKSPACE.to_string()),
            )]
        }
        "<" | "<=" | ">" | ">=" => {
            let cross = match name {
                "<" => "1 < 1.5 AND (2 < 1.5) == FALSE AND (1 < 1.0) == FALSE",
                "<=" => "1 <= 1.0 AND 1 <= 1.5 AND (2 <= 1.5) == FALSE",
                ">" => "2 > 1.5 AND (1 > 1.5) == FALSE AND (1 > 1.0) == FALSE",
                _ => "1 >= 1.0 AND 2 >= 1.5 AND (1 >= 1.5) == FALSE",
            };
            vec![
                special(
                    "order/integer-decimal-cross-type",
                    cross,
                    Special::Holds(String::new()),
                ),
                special(
                    "order/measure-unit-mismatch",
                    format!("MEASURE(1, unit.meter) {name} MEASURE(1, unit.kilometer)"),
                    Special::Rejects("error.numeric.unit_mismatch", String::new()),
                ),
                missing(format!("REF(data.nums)[5] {name} 2")),
                unknown(format!("(REF(input.u) {name} 2) == UNKNOWN"), "input.u"),
            ]
        }
        "IN" => vec![
            missing("REF(data.nums)[5] IN [1, 2]".into()),
            unknown("(REF(input.u) IN [1, 2]) == UNKNOWN".into(), "input.u"),
        ],
        "CONTAINS" => vec![
            missing(r#"REF(data.strs)[5] CONTAINS "a""#.into()),
            unknown(
                r#"(REF(input.us) CONTAINS "a") == UNKNOWN"#.into(),
                "input.us",
            ),
        ],
        "MATCHES" => vec![
            special(
                "match/glob-workspace-path",
                r#"PATH(REF(workspace.case), "src/a.py") MATCHES GLOB("src/*.py") AND NOT (PATH(REF(workspace.case), "lib/a.py") MATCHES GLOB("src/*.py"))"#,
                Special::Holds(WORKSPACE.to_string()),
            ),
            missing(r#"REF(data.strs)[5] MATCHES REGEX("a")"#.into()),
            unknown(
                r#"(REF(input.us) MATCHES REGEX("a")) == UNKNOWN"#.into(),
                "input.us",
            ),
        ],
        "AND" => vec![
            missing("REF(data.flags)[5] AND TRUE".into()),
            special(
                "special/skipped-operand-not-demanded",
                "(FALSE AND REF(data.flags)[5]) == FALSE",
                Special::Holds(missing_sources()),
            ),
        ],
        "OR" => vec![
            missing("REF(data.flags)[5] OR FALSE".into()),
            special(
                "special/skipped-operand-not-demanded",
                "(TRUE OR REF(data.flags)[5]) == TRUE",
                Special::Holds(missing_sources()),
            ),
        ],
        "property access" => vec![
            special(
                "property/closed-schema-absent-operand-error",
                "REF(data.record).absent == 1",
                Special::Rejects("error.operator.operand", RECORD.to_string()),
            ),
            special(
                "property/optional-field-absent-missing",
                "REF(data.record).opt == MISSING",
                Special::Holds(RECORD.to_string()),
            ),
            unknown("(REF(input.uo).a) == UNKNOWN".into(), "input.uo"),
        ],
        "index access" => vec![
            special(
                "index/closed-schema-absent-operand-error",
                r#"REF(data.record)["absent"] == 1"#,
                Special::Rejects("error.operator.operand", RECORD.to_string()),
            ),
            special(
                "index/runtime-varying-key-operand-error",
                "REF(data.record)[REF(input.us)] == 1",
                Special::Rejects(
                    "error.operator.operand",
                    format!("{RECORD}{}", unknown_inputs()),
                ),
            ),
            unknown("(REF(input.ul)[0]) == UNKNOWN".into(), "input.ul"),
        ],
        _ => Vec::new(),
    }
}

/// Registered division and pattern diagnostics of the operator_invalid rows.
fn operator_rejections(name: &str) -> Vec<SpecialRun> {
    let rejects = |label: &'static str, assertion: String, id: &'static str, declarations: &str| {
        special(
            label,
            assertion,
            Special::Rejects(id, declarations.to_string()),
        )
    };
    match name {
        "/" => vec![
            rejects(
                "division/zero-scalar-denominator",
                "1 / 0 == 1".into(),
                "error.numeric.division_by_zero",
                "",
            ),
            rejects(
                "division/zero-measure-denominator",
                "MEASURE(1, unit.meter) / MEASURE(0, unit.meter) == 1".into(),
                "error.numeric.division_by_zero",
                "",
            ),
            rejects(
                "division/non-terminating-quotient",
                "1 / 3 == 1".into(),
                "error.numeric.non_terminating",
                "",
            ),
            rejects(
                "division/measure-unit-mismatch",
                "MEASURE(1, unit.meter) / MEASURE(1, unit.second) == 1".into(),
                "error.numeric.unit_mismatch",
                "",
            ),
            rejects(
                "division/declared-bound",
                "TRUE".into(),
                "error.value.out_of_range",
                BOUNDED,
            ),
            // An exact quotient larger than the host materializes: "Host
            // limitations produce error.host.constraint".
            rejects(
                "division/host-capacity",
                format!("1 / 1.{}1 == 1", "0".repeat(4096)),
                "error.host.constraint",
                "",
            ),
        ],
        "MATCHES" => vec![rejects(
            "pattern/resource-limit",
            r#""a" MATCHES REGEX("a{200000}")"#.into(),
            "error.pattern.resource_limit",
            "",
        )],
        _ => Vec::new(),
    }
}

/// Special-value, zero-digit, optional-binding and quantifier-argument sub-runs of
/// one function row.
fn function_specials(name: &str) -> Vec<SpecialRun> {
    let missing = |assertion: String| {
        special(
            "special/missing",
            assertion,
            Special::Rejects("error.required.missing", missing_sources()),
        )
    };
    let unknown = |assertion: String, input: &'static str| {
        special("special/unknown", assertion, Special::Unknown(input))
    };
    match name {
        "ABS" => vec![
            missing("ABS(REF(data.nums)[5]) == 1".into()),
            unknown("ABS(REF(input.u)) == UNKNOWN".into(), "input.u"),
        ],
        "COUNT" => vec![
            missing("COUNT(REF(data.strs)[5]) == 1".into()),
            unknown("COUNT(REF(input.us)) == UNKNOWN".into(), "input.us"),
        ],
        "EMPTY" => vec![
            missing("EMPTY(REF(data.strs)[5])".into()),
            unknown("EMPTY(REF(input.us)) == UNKNOWN".into(), "input.us"),
        ],
        "SUM" | "MIN" | "MAX" => vec![
            missing(format!("{name}(REF(data.lists)[5]) == 1")),
            unknown(format!("{name}(REF(input.ul)) == UNKNOWN"), "input.ul"),
        ],
        "ROUND" => vec![
            special(
                "round/zero-digits",
                "ROUND(2.5, 0) == 2 AND ROUND(3.5, 0) == 4 AND ROUND(-2.5, 0) == -2",
                Special::Holds(String::new()),
            ),
            missing("ROUND(REF(data.decs)[5], 1) == 1".into()),
            unknown("ROUND(REF(input.ud), 1) == UNKNOWN".into(), "input.ud"),
        ],
        "EXISTS" => vec![special(
            "exists/unresolved-optional-binding-false",
            "EXISTS(REF(input.opt)) == FALSE",
            Special::Holds(
                "\nINPUT:\n    ID: input.opt\n    TYPE: STRING\n    SOURCE: PATH(\"/case/absent.txt\")\n    REQUIRED: FALSE\n".to_string(),
            ),
        )],
        "ALL" | "ANY" | "NONE" => {
            let material = match name {
                "ALL" => "ALL(REF(data.bools)) == FALSE AND ALL(REF(data.trues)) == TRUE",
                "ANY" => "ANY(REF(data.bools)) == TRUE AND ANY(REF(data.falses)) == FALSE",
                _ => "NONE(REF(data.bools)) == FALSE AND NONE(REF(data.falses)) == TRUE",
            };
            let sequences = [
                data("bools", "LIST[BOOLEAN]", "[TRUE, FALSE]"),
                data("trues", "LIST[BOOLEAN]", "[TRUE, TRUE]"),
                data("falses", "LIST[BOOLEAN]", "[FALSE, FALSE]"),
            ]
            .concat();
            vec![
                special(
                    "quantifier/material-list-argument",
                    material,
                    Special::Holds(sequences),
                ),
                special(
                    "quantifier/missing-member",
                    format!("{name}([TRUE, REF(data.flags)[5]]) == TRUE"),
                    Special::Rejects("error.required.missing", missing_sources()),
                ),
            ]
        }
        _ => Vec::new(),
    }
}

/// Registered function diagnostics of the function_invalid rows.
fn function_rejections(name: &str) -> Vec<SpecialRun> {
    let rejects = |label: &'static str, assertion: String, id: &'static str| {
        special(label, assertion, Special::Rejects(id, String::new()))
    };
    match name {
        "ALL" | "ANY" | "NONE" => vec![rejects(
            "quantifier/non-boolean-member",
            format!("{name}([TRUE, 1]) == TRUE"),
            "error.operator.operand",
        )],
        "EMPTY" => vec![rejects(
            "empty/null-operand",
            "EMPTY(NULL) == TRUE".into(),
            "error.operator.operand",
        )],
        "ROUND" => vec![
            rejects(
                "round/negative-digits",
                "ROUND(2.5, -1) == 1".into(),
                "error.value.out_of_range",
            ),
            rejects(
                "round/division-by-zero",
                "ROUND(1 / 0, 2) == 1".into(),
                "error.numeric.division_by_zero",
            ),
            rejects(
                "round/unit-mismatch",
                "ROUND(MEASURE(1, unit.meter) / MEASURE(1, unit.second), 1) == 1".into(),
                "error.numeric.unit_mismatch",
            ),
            rejects(
                "round/host-capacity",
                "ROUND(1 / 3, 5000) == 1".into(),
                "error.host.constraint",
            ),
        ],
        "SUM" => vec![special(
            "sum/unit-mismatch",
            "SUM(REF(data.mixed)) == MEASURE(2, unit.meter)",
            Special::Rejects(
                "error.numeric.unit_mismatch",
                data(
                    "mixed",
                    "LIST[MEASURE]",
                    "[MEASURE(1, unit.meter), MEASURE(1, unit.second)]",
                ),
            ),
        )],
        _ => Vec::new(),
    }
}

pub fn execute_functions(runner: &Runner) -> Vec<ExecutedCase> {
    let mut out = Vec::new();
    for name in [
        "ABS", "COUNT", "SUM", "MIN", "MAX", "ROUND", "EXISTS", "EMPTY", "ALL", "ANY", "NONE",
    ] {
        let mut expressions: Vec<(String, String, String)> = Vec::new();
        match name {
            "ABS" => {
                for (label, expr) in [
                    ("family/integer", "ABS(-3) == 3"),
                    ("family/decimal", "ABS(-1.25) == 1.25"),
                    (
                        "family/duration",
                        "ABS(DURATION(1, unit.minute)) == DURATION(60, unit.second)",
                    ),
                    (
                        "family/measure",
                        "ABS(MEASURE(-2, unit.meter)) == MEASURE(2, unit.meter)",
                    ),
                ] {
                    expressions.push((label.into(), String::new(), expr.into()));
                }
            }
            "ROUND" => {
                for (label, expr) in [
                    ("round/half-even-down", "ROUND(2.45, 1) == 2.4"),
                    ("round/half-even-up", "ROUND(2.55, 1) == 2.6"),
                    ("round/negative-half-even", "ROUND(-2.45, 1) == -2.4"),
                    (
                        "round/measure-preserves-unit",
                        "ROUND(MEASURE(1.25, unit.meter), 1) == MEASURE(1.2, unit.meter)",
                    ),
                    (
                        "round/direct-quotient-rounded-once",
                        "ROUND(10 / 3, 2) == 3.33",
                    ),
                ] {
                    expressions.push((label.into(), String::new(), expr.into()));
                }
            }
            "COUNT" | "EMPTY" => {
                for (count_clause, empty_clause, ty, value, count) in [
                    (
                        "string-scalar-values",
                        "string-nonempty",
                        "STRING",
                        "\"é😀\"",
                        2,
                    ),
                    ("string-empty", "string-empty", "STRING", "\"\"", 0),
                    (
                        "list-occurrences",
                        "list-nonempty",
                        "LIST[INTEGER]",
                        "[1, 1]",
                        2,
                    ),
                    ("list-empty", "list-empty", "LIST[INTEGER]", "[]", 0),
                    (
                        "set-unique-members",
                        "set-nonempty",
                        "SET[INTEGER]",
                        "[1, 1]",
                        1,
                    ),
                    ("set-empty", "set-empty", "SET[INTEGER]", "[]", 0),
                    (
                        "object-present-properties",
                        "object-nonempty",
                        "OBJECT",
                        "\n        a: FALSE\n        b: 0",
                        2,
                    ),
                    ("bytes-exact", "bytes-nonempty", "BYTES", "BYTES(3)", 3),
                    ("bytes-zero", "bytes-zero", "BYTES", "BYTES(0)", 0),
                ] {
                    let (label, expected) = if name == "COUNT" {
                        (format!("count/{count_clause}"), count.to_string())
                    } else {
                        let empty = if count == 0 { "TRUE" } else { "FALSE" };
                        (format!("empty/{empty_clause}"), empty.to_string())
                    };
                    expressions.push((
                        label,
                        data("arg", ty, value),
                        format!("{name}(REF(data.arg)) == {expected}"),
                    ));
                }
            }
            "SUM" => {
                for collection in ["LIST", "SET"] {
                    for (ty, values, total) in [
                        ("INTEGER", "[1, 2]", "3"),
                        ("DECIMAL", "[1.25, 2.5]", "3.75"),
                        (
                            "DURATION",
                            "[DURATION(30, unit.second), DURATION(1, unit.minute)]",
                            "DURATION(90, unit.second)",
                        ),
                        (
                            "MEASURE",
                            "[MEASURE(1, unit.meter), MEASURE(2, unit.meter)]",
                            "MEASURE(3, unit.meter)",
                        ),
                    ] {
                        expressions.push((
                            format!("sum/{collection}/{ty}"),
                            data("arg", &format!("{collection}[{ty}]"), values),
                            format!("SUM(REF(data.arg)) == {total}"),
                        ));
                    }
                }
            }
            "MIN" | "MAX" => {
                let prefix = if name == "MIN" { "min" } else { "max" };
                for collection in ["LIST", "SET"] {
                    for (ty, low, high) in ordered_values() {
                        expressions.push((
                            format!("{prefix}/{collection}/{ty}"),
                            data(
                                "arg",
                                &format!("{collection}[{ty}]"),
                                &format!("[{high}, {low}]"),
                            ),
                            format!(
                                "{name}(REF(data.arg)) == {}",
                                if name == "MIN" { low } else { high }
                            ),
                        ));
                    }
                }
            }
            "EXISTS" => {
                for (label, expr) in [
                    ("exists/missing-false", "EXISTS(MISSING) == FALSE"),
                    ("exists/unknown-true", "EXISTS(UNKNOWN) == TRUE"),
                    ("exists/null-true", "EXISTS(NULL) == TRUE"),
                    ("exists/false-true", "EXISTS(FALSE) == TRUE"),
                    ("exists/zero-true", "EXISTS(0) == TRUE"),
                    ("exists/empty-string-true", "EXISTS(\"\") == TRUE"),
                ] {
                    expressions.push((label.into(), String::new(), expr.into()));
                }
            }
            "ALL" | "ANY" | "NONE" => {
                for (clause, values, all, any, none) in [
                    ("empty", "[]", "TRUE", "FALSE", "TRUE"),
                    ("true", "[TRUE]", "TRUE", "TRUE", "FALSE"),
                    ("false", "[FALSE]", "FALSE", "FALSE", "TRUE"),
                    ("unknown", "[UNKNOWN]", "UNKNOWN", "UNKNOWN", "UNKNOWN"),
                    (
                        "true-unknown",
                        "[TRUE, UNKNOWN]",
                        "UNKNOWN",
                        "TRUE",
                        "FALSE",
                    ),
                    (
                        "false-unknown",
                        "[FALSE, UNKNOWN]",
                        "FALSE",
                        "UNKNOWN",
                        "UNKNOWN",
                    ),
                ] {
                    expressions.push((
                        format!("quantifier/{clause}"),
                        String::new(),
                        format!(
                            "{name}({values}) == {}",
                            match name {
                                "ALL" => all,
                                "ANY" => any,
                                _ => none,
                            }
                        ),
                    ));
                }
            }
            _ => unreachable!(),
        }
        out.push(expression_group(
            runner,
            "function_valid",
            name,
            expressions,
            special_runs(runner, function_specials(name)),
        ));
        let invalid = if name == "EXISTS" {
            "EXISTS(1, 2)".to_string()
        } else if name == "ROUND" {
            "ROUND(TRUE, 1)".into()
        } else {
            format!("{name}(1, 2)")
        };
        let mut negatives = vec![run(
            runner,
            "invalid-arity-or-family",
            &crate::fixtures::assertion_task("", &format!("EXISTS({invalid})")),
            Expectation::Diagnostic("error.operator.operand".into()),
        )];
        if matches!(name, "SUM" | "MIN" | "MAX") {
            negatives.push(run(
                runner,
                "empty-typed-collection",
                &crate::fixtures::assertion_task(
                    &data("empty", "LIST[INTEGER]", "[]"),
                    &format!("EXISTS({name}(REF(data.empty)))"),
                ),
                Expectation::Diagnostic("error.operator.operand".into()),
            ));
        }
        negatives.extend(special_runs(runner, function_rejections(name)));
        out.push(group(
            &format!("semantic/function_invalid/{name}"),
            "functions: reject unregistered signatures and empty reductions",
            negatives,
        ));
    }
    out
}

pub fn execute_operators(runner: &Runner) -> Vec<ExecutedCase> {
    let mut out = Vec::new();
    for name in [
        "NOT",
        "unary -",
        "*",
        "/",
        "+",
        "-",
        "==",
        "!=",
        "<",
        "<=",
        ">",
        ">=",
        "IN",
        "CONTAINS",
        "MATCHES",
        "AND",
        "OR",
        "property access",
        "index access",
    ] {
        let mut expressions: Vec<(String, String, String)> = Vec::new();
        let plain: Vec<(&str, &str)> = match name {
            "NOT" => vec![
                ("family/true", "NOT TRUE == FALSE"),
                ("family/false", "NOT FALSE == TRUE"),
                ("special/unknown", "NOT UNKNOWN == UNKNOWN"),
            ],
            "unary -" => vec![
                ("family/integer", "-3 == -3"),
                ("family/decimal", "-1.25 == -1.25"),
                (
                    "family/measure",
                    "-MEASURE(2, unit.meter) == MEASURE(-2, unit.meter)",
                ),
            ],
            "IN" => vec![
                ("membership/literal-list-present", "1 IN [1, 2]"),
                ("membership/literal-list-absent", "(3 IN [1, 2]) == FALSE"),
            ],
            "CONTAINS" => vec![
                (
                    "containment/string-substring",
                    "\"éclair\" CONTAINS \"clair\"",
                ),
                (
                    "containment/string-empty-substring",
                    "\"abc\" CONTAINS \"\"",
                ),
                ("containment/literal-list-member", "[1, 2] CONTAINS 1"),
                (
                    "containment/literal-list-absent",
                    "([1, 2] CONTAINS 3) == FALSE",
                ),
            ],
            "MATCHES" => vec![
                ("match/regex-full-match", "\"abc\" MATCHES REGEX(\"a.*\")"),
                (
                    "match/regex-partial-is-false",
                    "(\"xabc\" MATCHES REGEX(\"a.*\")) == FALSE",
                ),
                ("match/glob-string", "\"a.txt\" MATCHES GLOB(\"*.txt\")"),
                (
                    "match/glob-absolute-path-false",
                    "(PATH(\"/case/a.txt\") MATCHES GLOB(\"*.txt\")) == FALSE",
                ),
            ],
            "property access" | "index access" | "AND" | "OR" | "==" | "!=" | "*" | "/" | "+"
            | "-" | "<" | "<=" | ">" | ">=" => vec![],
            _ => unreachable!(),
        };
        expressions.extend(
            plain
                .into_iter()
                .map(|(label, e)| (label.into(), String::new(), e.into())),
        );
        let family = |value: &str| {
            if value.contains('.') {
                "decimal"
            } else {
                "integer"
            }
        };
        match name {
            "*" | "/" | "+" | "-" => {
                for left in ["6", "6.0"] {
                    for right in ["2", "2.0"] {
                        let expected = match name {
                            "*" => "12",
                            "/" => "3.0",
                            "+" => "8",
                            _ => "4",
                        };
                        expressions.push((
                            format!("family/{}-{}", family(left), family(right)),
                            String::new(),
                            format!("({left} {name} {right}) == {expected}"),
                        ));
                    }
                }
                for numeric in ["2", "2.0"] {
                    let expected = if name == "*" { "12" } else { "3" };
                    if matches!(name, "*" | "/") {
                        expressions.push((
                            format!("family/measure-{}", family(numeric)),
                            String::new(),
                            format!("(MEASURE(6, unit.meter) {name} {numeric}) == MEASURE({expected}, unit.meter)"),
                        ));
                    }
                    if name == "*" {
                        expressions.push((
                            format!("family/{}-measure", family(numeric)),
                            String::new(),
                            format!(
                                "({numeric} * MEASURE(6, unit.meter)) == MEASURE(12, unit.meter)"
                            ),
                        ));
                    }
                }
                if name == "/" {
                    expressions.push((
                        "family/measure-measure-same-unit".into(),
                        String::new(),
                        "(MEASURE(6, unit.meter) / MEASURE(2, unit.meter)) == 3.0".into(),
                    ));
                }
                if matches!(name, "+" | "-") {
                    for (label, ty) in [
                        ("family/duration-duration", "DURATION"),
                        ("family/measure-measure-same-unit", "MEASURE"),
                    ] {
                        expressions.push((
                            label.into(),
                            String::new(),
                            format!("({ty}(6, unit.second) {name} {ty}(2, unit.second)) == {ty}({}, unit.second)", if name == "+" { 8 } else { 4 }),
                        ));
                    }
                }
            }
            "<" | "<=" | ">" | ">=" => {
                for (ty, low, high) in ordered_values() {
                    let (left, right) = if name.starts_with('<') {
                        (low, high)
                    } else {
                        (high, low)
                    };
                    expressions.push((
                        format!("order/{ty}/distinct"),
                        String::new(),
                        format!("{left} {name} {right}"),
                    ));
                    expressions.push((
                        format!("order/{ty}/equal"),
                        String::new(),
                        format!(
                            "({low} {name} {low}) == {}",
                            if name.ends_with('=') { "TRUE" } else { "FALSE" }
                        ),
                    ));
                }
            }
            "==" | "!=" => {
                for (clause, left, right, equal) in [
                    ("integer-decimal-promotion", "1", "1.0", true),
                    ("different-material-families", "TRUE", "1", false),
                    ("null", "NULL", "NULL", true),
                    ("missing-self", "MISSING", "MISSING", true),
                    ("unknown-self", "UNKNOWN", "UNKNOWN", true),
                    ("missing-unknown-distinct", "MISSING", "UNKNOWN", false),
                    ("unknown-material", "UNKNOWN", "1", false),
                    (
                        "measure-different-units",
                        "MEASURE(1, unit.meter)",
                        "MEASURE(1, unit.kilometer)",
                        false,
                    ),
                    (
                        "measure-duration-families",
                        "MEASURE(1, unit.second)",
                        "DURATION(1, unit.second)",
                        false,
                    ),
                    ("pattern-identity", "REGEX(\"a\")", "REGEX(\"[a]\")", false),
                ] {
                    expressions.push((
                        format!("equality/{clause}"),
                        String::new(),
                        format!(
                            "({left} {name} {right}) == {}",
                            if equal == (name == "==") {
                                "TRUE"
                            } else {
                                "FALSE"
                            }
                        ),
                    ));
                }
            }
            "AND" | "OR" => {
                for left in ["TRUE", "FALSE", "UNKNOWN"] {
                    for right in ["TRUE", "FALSE", "UNKNOWN"] {
                        let expected = if name == "AND" {
                            if left == "FALSE" || right == "FALSE" {
                                "FALSE"
                            } else if left == "UNKNOWN" || right == "UNKNOWN" {
                                "UNKNOWN"
                            } else {
                                "TRUE"
                            }
                        } else if left == "TRUE" || right == "TRUE" {
                            "TRUE"
                        } else if left == "UNKNOWN" || right == "UNKNOWN" {
                            "UNKNOWN"
                        } else {
                            "FALSE"
                        };
                        expressions.push((
                            format!("logic/{}-{}", left.to_lowercase(), right.to_lowercase()),
                            String::new(),
                            format!("({left} {name} {right}) == {expected}"),
                        ));
                    }
                }
            }
            "IN" | "CONTAINS" => {
                let (prefix, present) = if name == "IN" {
                    ("membership", "present")
                } else {
                    ("containment", "member")
                };
                for collection in ["LIST", "SET"] {
                    for (values, expected) in [("[1, 1]", "TRUE"), ("[]", "FALSE")] {
                        let expr = if name == "IN" {
                            "1 IN REF(data.arg)"
                        } else {
                            "REF(data.arg) CONTAINS 1"
                        };
                        let clause = match (collection, values) {
                            ("LIST", "[1, 1]") => format!("list-duplicates-{present}"),
                            ("SET", "[1, 1]") => format!("set-{present}"),
                            _ => format!("{}-empty-absent", collection.to_lowercase()),
                        };
                        expressions.push((
                            format!("{prefix}/{clause}"),
                            data("arg", &format!("{collection}[INTEGER]"), values),
                            format!("({expr}) == {expected}"),
                        ));
                    }
                }
                if name == "CONTAINS" {
                    expressions.push((
                        "containment/object-key-presence".into(),
                        data("arg", "OBJECT", "\n        a: FALSE"),
                        "(REF(data.arg) CONTAINS \"a\") AND NOT (REF(data.arg) CONTAINS \"b\")"
                            .into(),
                    ));
                }
            }
            "property access" => {
                expressions.push((
                    "property/present-field".into(),
                    data("arg", "OBJECT", "\n        a: 3"),
                    "REF(data.arg).a == 3".into(),
                ));
                expressions.push((
                    "property/schema-free-absent-missing".into(),
                    data("arg", "OBJECT", "\n        a: 3"),
                    "REF(data.arg).absent == MISSING".into(),
                ));
                expressions.push((
                    "property/declaration-metadata".into(),
                    data("arg", "INTEGER", "3"),
                    "REF(data.arg).VALUE == 3".into(),
                ));
            }
            "index access" => {
                for (clause, index, expected) in [
                    ("list-first", "0", "3"),
                    ("list-second", "1", "4"),
                    ("list-negative-missing", "-1", "MISSING"),
                    ("list-beyond-missing", "2", "MISSING"),
                ] {
                    expressions.push((
                        format!("index/{clause}"),
                        data("arg", "LIST[INTEGER]", "[3, 4]"),
                        format!("REF(data.arg)[{index}] == {expected}"),
                    ));
                }
                expressions.push((
                    "index/object-key".into(),
                    data("arg", "OBJECT", "\n        a: 3"),
                    "REF(data.arg)[\"a\"] == 3".into(),
                ));
                expressions.push((
                    "index/object-absent-key-missing".into(),
                    data("arg", "OBJECT", "\n        a: 3"),
                    "REF(data.arg)[\"absent\"] == MISSING".into(),
                ));
            }
            _ => {}
        }
        if matches!(name, "property access" | "index access") {
            let (label, select) = if name == "property access" {
                (
                    "property/forwarded-reference-nested",
                    "(REF(data.forward).nested).answer",
                )
            } else {
                (
                    "index/forwarded-reference-nested",
                    "REF(data.forward)[\"nested\"][\"answer\"]",
                )
            };
            let decl = format!(
                "{}{}",
                data("forward", "OBJECT", "REF(data.original)"),
                data(
                    "original",
                    "OBJECT",
                    "\n        nested:\n            answer: 42"
                )
            );
            expressions.push((label.into(), decl, format!("{select} == 42")));
        }
        out.push(expression_group(
            runner,
            "operator_valid",
            name,
            expressions,
            special_runs(runner, operator_specials(name)),
        ));
        let invalid = match name {
            "NOT" => "NOT 1".into(),
            "unary -" => "-TRUE".into(),
            "IN" => "1 IN TRUE".into(),
            "CONTAINS" => "TRUE CONTAINS 1".into(),
            "MATCHES" => "1 MATCHES REGEX(\"a\")".into(),
            "property access" => "REF(data.arg).a".into(),
            "index access" => "\"abc\"[0]".into(),
            "==" | "!=" => format!("REF(data.arg).TYPE {name} 1"),
            _ => format!("TRUE {name} 1"),
        };
        let decl = format!("{ENUM}{}", data("arg", "INTEGER", "1"));
        let mut runs = vec![run(
            runner,
            "unregistered-operands",
            &crate::fixtures::assertion_task(&decl, &format!("EXISTS({invalid})")),
            Expectation::Diagnostic("error.operator.operand".into()),
        )];
        runs.extend(special_runs(runner, operator_rejections(name)));
        out.push(group(
            &format!("semantic/operator_invalid/{name}"),
            "operators: unregistered operand families and registered operator diagnostics",
            runs,
        ));
    }
    out
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
        contract: "production component contract".into(),
        source: input,
        expectation,
        observed,
        verdict,
    }
}

pub fn execute_statuses_and_errors(spec: &SpecPackage) -> Vec<ExecutedCase> {
    use lcl_diagnostics::DiagnosticRegistry;
    use lcl_spec::json::Json;
    use std::collections::{BTreeMap, VecDeque};
    let canonical = spec
        .registry("statuses_and_errors")
        .expect("verified status/error registry");
    let registry = DiagnosticRegistry::load(spec).expect("production diagnostic registry");
    let statuses = canonical.get("statuses").and_then(Json::as_object).unwrap();
    // Paths come from the independent canonical oracle. Every step is executed
    // through Lifecycle::transition; a loader or transition defect cannot make
    // an unreachable starting state look tested.
    let mut paths = BTreeMap::from([("status.not_started".to_string(), Vec::<String>::new())]);
    let mut pending = VecDeque::from(["status.not_started".to_string()]);
    while let Some(from) = pending.pop_front() {
        let row = canonical
            .get("statuses")
            .and_then(|r| r.get(&from))
            .unwrap();
        for to in row
            .get("allowed_next")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .map(|j| j.as_str().unwrap())
        {
            if !paths.contains_key(to) {
                let mut path = paths[&from].clone();
                path.push(to.to_string());
                paths.insert(to.to_string(), path);
                pending.push_back(to.to_string());
            }
        }
    }
    assert_eq!(
        paths.len(),
        statuses.len(),
        "every required status must have a concrete entry path"
    );
    let mut out = Vec::new();
    for (from, row) in statuses {
        let allowed: Vec<_> = row
            .get("allowed_next")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .map(|j| j.as_str().unwrap())
            .collect();
        let mut runs = Vec::new();
        for to in statuses
            .iter()
            .map(|(id, _)| id.as_str())
            .chain(std::iter::once("status.invented"))
        {
            for root in [false, true] {
                // A root cannot enter skipped. Its refusal is exercised from
                // each other state, while the child exercises that endpoint.
                if root && from == "status.skipped" {
                    continue;
                }
                let mut life = lcl_runtime::Lifecycle::new(root);
                let mut expected = Vec::new();
                let mut actual = Vec::new();
                for step in &paths[from] {
                    let got = life.transition(&registry, step);
                    expected.push((format!("enter/{step}"), "true".into()));
                    actual.push((format!("enter/{step}"), got.is_ok().to_string()));
                }
                let permits = allowed.contains(&to) && !(root && to == "status.skipped");
                expected.push(("permits".into(), permits.to_string()));
                actual.push(("permits".into(), life.permits(&registry, to).to_string()));
                let result = life.transition(&registry, to);
                expected.push(("transition".into(), permits.to_string()));
                actual.push(("transition".into(), result.is_ok().to_string()));
                expected.push((
                    "current".into(),
                    if permits { to } else { from }.to_string(),
                ));
                actual.push(("current".into(), life.current().into()));
                let final_row = canonical
                    .get("statuses")
                    .and_then(|r| r.get(if permits { to } else { from }))
                    .unwrap();
                expected.push((
                    "terminal".into(),
                    final_row
                        .get("terminal")
                        .and_then(Json::as_bool)
                        .unwrap()
                        .to_string(),
                ));
                actual.push(("terminal".into(), life.is_terminal(&registry).to_string()));
                runs.push(component(
                    &format!("{to}/root={root}"),
                    format!(
                        "Lifecycle::new({root}); path={:?}; transition({to:?})",
                        paths[from]
                    ),
                    expected,
                    actual,
                ));
            }
        }
        out.push(group(
            &format!("semantic/status_transition/{from}"),
            "every allowed and forbidden lifecycle transition; root skip exclusion",
            runs,
        ));
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
        let expected = vec![
            ("id".into(), id.clone()),
            ("stage".into(), text("stage")),
            ("status".into(), text("default_status")),
            (
                "recoverable".into(),
                row.get("recoverable_with_declared_handler")
                    .and_then(Json::as_bool)
                    .unwrap()
                    .to_string(),
            ),
            (
                "event".into(),
                format!("{:?}", row.get("event").and_then(Json::as_str)),
            ),
            ("meaning".into(), text("meaning")),
        ];
        let error = registry.error(id).unwrap();
        let actual = vec![
            ("id".into(), error.id.clone()),
            ("stage".into(), error.stage.to_string()),
            ("status".into(), error.default_status.clone()),
            (
                "recoverable".into(),
                error.recoverable_with_declared_handler.to_string(),
            ),
            ("event".into(), format!("{:?}", error.event.as_deref())),
            ("meaning".into(), error.meaning.clone()),
        ];
        let mut runs = vec![component(
            "registry-contract",
            format!(
                "DiagnosticRegistry::load(approved 0.1.0).error({id:?}); canonical oracle={row:?}"
            ),
            expected,
            actual,
        )];
        let mut mirrors = Vec::new();
        if let Some(error) = lcl_lexer::LexicalError::from_registry_str(id) {
            mirrors.push(("lexer", lexicon.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_parser::GrammarError::from_registry_str(id) {
            mirrors.push(("parser", grammar.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_resolver::ResolutionError::from_registry_str(id) {
            mirrors.push(("resolver", rules.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_checker::StaticError::from_registry_str(id) {
            mirrors.push(("checker", statics.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_semantics::PreflightError::from_registry_str(id) {
            mirrors.push(("preflight", preflight.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_runtime::RuntimeError::from_registry_str(id) {
            mirrors.push(("runtime", runtime.error(error).default_status.clone()));
        }
        if let Some(error) = lcl_completion::CompletionError::from_registry_str(id) {
            mirrors.push(("completion", completion.error(error).default_status.clone()));
        }
        assert!(!mirrors.is_empty(), "{id} has no implementing component");
        for (owner, status) in mirrors {
            runs.push(component(
                owner,
                format!("{owner}.error({id:?}).default_status"),
                vec![("default_status".into(), text("default_status"))],
                vec![("default_status".into(), status)],
            ));
        }
        out.push(group(&format!("semantic/error_contract/{id}"), "registered stage, recoverability, event and default status; implementing stage mirrors", runs));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Verdict;

    #[test]
    fn object_selection_keeps_closed_schemas_and_exact_receiving_types() {
        let runner = crate::fixtures::runner();
        let schema = "\nDEFINE:\n    ID: type.closed\n    KIND: kind.type\n    BASE: OBJECT\n    FIELD:\n        NAME: answer\n        TYPE: INTEGER\n        REQUIRED: TRUE\n";
        let decl = format!(
            "{schema}{}",
            data("arg", "OBJECT[REF(type.closed)]", "\n        answer: 42")
        );
        for select in ["REF(data.arg).missing", "REF(data.arg)[\"missing\"]"] {
            let case = run(
                &runner,
                "closed-absence",
                &crate::fixtures::assertion_task(&decl, &format!("{select} == MISSING")),
                Expectation::Diagnostic("error.operator.operand".into()),
            );
            assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
        }
        let decl = format!(
            "{}{}",
            data("arg", "OBJECT", "\n        answer: 42"),
            data("wrong", "STRING", "REF(data.arg).answer")
        );
        let case = run(
            &runner,
            "exact-inferred-type",
            &crate::fixtures::assertion_task(&decl, "TRUE"),
            Expectation::Diagnostic("error.type.mismatch".into()),
        );
        assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
        let decl = format!(
            "{}{}",
            data("arg", "OBJECT", "\n        answer: 42"),
            data("key", "STRING", "\"answer\"")
        );
        let case = run(
            &runner,
            "runtime-varying-key",
            &crate::fixtures::assertion_task(&decl, "REF(data.arg)[REF(data.key)] == 42"),
            Expectation::Diagnostic("error.operator.operand".into()),
        );
        assert_eq!(case.verdict, Verdict::Passed, "{}", case.serialize());
    }
}
