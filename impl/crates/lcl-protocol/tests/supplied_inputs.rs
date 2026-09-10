//! Supplied invocation data: converted by the language, never by the tool.
//!
//! `05_SEMANTICS/02` rules out ambient data — "Ambient current directory and
//! implied nearby files do not exist in portable LCL" — so every datum has to
//! be named and supplied by a caller. These tests are about what happens to it
//! on the way in: that the value a caller wrote is the value the document
//! reads, that an expression the engine cannot evaluate is refused with a
//! reason rather than approximated, and that a refusal is reported as a refused
//! *request* rather than as a rejected document.

mod common;

use common::{engine, example, unit};
use lcl_protocol::{Inputs, Outcome, Reached};
use lcl_resolver::MemoryProvider;
use lcl_runtime::MockHost;
use lcl_semantics::Value;

/// `01_MINIMAL_TASK`, with its declared `VALUE` replaced by a `DEFAULT`.
///
/// Two canonical rules decide this fixture's shape. `05_SEMANTICS/06` orders
/// resolution "1. explicit VALUE; 2. resolved SOURCE/INPUT/STATE/MEMORY/
/// CONTEXT; 3. DEFAULT only for MISSING", so a declaration carrying its own
/// `VALUE` can never show whether a supplied datum was read: the declared value
/// wins, correctly. And `block_schemas_v0.1.0.json#/schemas/INPUT` requires
/// "Exactly one VALUE or SOURCE unless optional with DEFAULT", so simply
/// deleting the `VALUE` produces a document the grammar stage rejects.
///
/// An optional `INPUT` with a `DEFAULT` satisfies the schema and puts the
/// supplied datum on the resolution path at position 2, ahead of the default at
/// position 3. Running it with and without a supplied value therefore tests the
/// precedence as well as the plumbing.
fn awaiting_input() -> String {
    example("01_MINIMAL_TASK.lcl").replace(
        "    TYPE: INTEGER\n    VALUE: 4\n",
        "    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 4\n",
    )
}

/// With nothing supplied, the declared `DEFAULT` resolves it.
#[test]
fn the_default_resolves_an_unsupplied_datum() {
    let source = awaiting_input();
    let mut stdlib = engine()
        .stdlib()
        .expect("assembles")
        .with_profiles(lcl_stdlib::checking_profiles());
    let mut host = MockHost::new();
    let report = engine().run(
        &unit("doc.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
        &mut stdlib,
        &mut host,
    );

    assert!(report.inputs.is_empty());
    assert_eq!(report.terminal_status(), Some("status.succeeded"));
}

/// The document reads the supplied value, and the run reflects it.
///
/// The fixture doubles `input.value` and its `VERIFY` asserts the output is 8.
/// Supplying 4 makes that hold.
#[test]
fn a_supplied_value_reaches_the_document() {
    let source = awaiting_input();
    let inputs = Inputs::new().with_text("input.value", "4");

    let mut stdlib = engine()
        .stdlib()
        .expect("assembles")
        .with_profiles(lcl_stdlib::checking_profiles());
    let mut host = MockHost::new();
    let report = engine().run(
        &unit("doc.lcl", &source),
        &MemoryProvider::new(),
        &inputs,
        &mut stdlib,
        &mut host,
    );

    assert_eq!(report.inputs.len(), 1);
    assert!(report.inputs[0].accepted());
    assert_eq!(report.inputs[0].value.as_deref(), Some("4"));
    assert_eq!(report.terminal_status(), Some("status.succeeded"));
}

/// A different supplied value changes the outcome, which proves it was read.
///
/// A supplied datum that changed nothing would be indistinguishable from one
/// that was ignored. Supplying 5 makes the doubled output 10, the declared
/// `VERIFY` records FALSE, and the invocation no longer succeeds.
#[test]
fn a_supplied_value_actually_decides_the_result() {
    let source = awaiting_input();
    let inputs = Inputs::new().with_text("input.value", "5");

    let mut stdlib = engine()
        .stdlib()
        .expect("assembles")
        .with_profiles(lcl_stdlib::checking_profiles());
    let mut host = MockHost::new();
    let report = engine().run(
        &unit("doc.lcl", &source),
        &MemoryProvider::new(),
        &inputs,
        &mut stdlib,
        &mut host,
    );

    assert_eq!(report.inputs[0].value.as_deref(), Some("5"));
    assert_ne!(report.terminal_status(), Some("status.succeeded"));
    let completion = report.completion.as_ref().expect("a completion record");
    let verify = completion
        .checks
        .iter()
        .find(|c| c.id == "verify.value")
        .expect("the declared VERIFY ran");
    assert_eq!(verify.outcome.as_deref(), Some("FALSE"));
}

/// Text and value forms produce the same invocation.
#[test]
fn text_and_value_forms_agree() {
    let source = awaiting_input();
    let run = |inputs: Inputs| {
        let mut stdlib = engine()
            .stdlib()
            .expect("assembles")
            .with_profiles(lcl_stdlib::checking_profiles());
        let mut host = MockHost::new();
        engine()
            .run(
                &unit("doc.lcl", &source),
                &MemoryProvider::new(),
                &inputs,
                &mut stdlib,
                &mut host,
            )
            .terminal_status()
            .map(str::to_string)
    };

    let from_text = run(Inputs::new().with_text("input.value", "4"));
    let from_value = run(Inputs::new().with_value(
        "input.value",
        Value::Integer(lcl_checker::numeric::Decimal::parse_integer("4").expect("exact 4")),
    ));
    assert_eq!(from_text, from_value);
    assert_eq!(from_text.as_deref(), Some("status.succeeded"));
}

/// The whole literal surface converts, exactly.
#[test]
fn the_literal_surface_converts() {
    let source = example("01_MINIMAL_TASK.lcl");
    let cases = [
        ("TRUE", "TRUE"),
        ("FALSE", "FALSE"),
        ("NULL", "NULL"),
        ("MISSING", "MISSING"),
        ("UNKNOWN", "UNKNOWN"),
        ("42", "42"),
        ("-42", "-42"),
        (
            "0.1000000000000000000000000000001",
            "0.1000000000000000000000000000001",
        ),
        ("\"text\"", "\"text\""),
        ("PATH(\"/tmp/x\")", "PATH(\"/tmp/x\")"),
        ("[1, 2, 3]", "[1, 2, 3]"),
        ("unit.second", "unit.second"),
    ];
    for (expression, expected) in cases {
        let inputs = Inputs::new().with_text("input.value", expression);
        let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);
        let record = report.inputs.first().expect("one input record");
        assert!(
            record.accepted(),
            "{expression} converts: {:?}",
            record.reason
        );
        assert_eq!(record.value.as_deref(), Some(expected), "{expression}");
    }
}

/// An exact decimal is not rounded on the way in.
#[test]
fn an_exact_decimal_survives_conversion() {
    let source = example("01_MINIMAL_TASK.lcl");
    let exact = "3.14159265358979323846264338327950288419716939937510";
    let inputs = Inputs::new().with_text("input.value", exact);
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

    assert_eq!(report.inputs[0].value.as_deref(), Some(exact));
}

/// Text that is not one expression is refused with a reason.
#[test]
fn text_that_is_not_one_expression_is_refused() {
    let source = example("01_MINIMAL_TASK.lcl");
    for expression in ["", "1 2", "((", "ACTION:"] {
        let inputs = Inputs::new().with_text("input.value", expression);
        let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

        assert_eq!(
            report.outcome,
            Outcome::Refused,
            "{expression:?} refuses the request"
        );
        let record = report.inputs.first().expect("one input record");
        assert!(!record.accepted());
        assert!(record.reason.is_some(), "{expression:?} states a reason");
    }
}

/// A refused request is not a rejected document.
///
/// The distinction is the whole reason `Outcome::Refused` exists: a caller who
/// mistyped an input has learned nothing about their document, and a report
/// claiming otherwise would be a false negative they might act on.
#[test]
fn a_refused_request_reports_no_verdict_on_the_document() {
    let source = example("01_MINIMAL_TASK.lcl");
    let inputs = Inputs::new().with_text("input.value", "1 2");
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

    assert_eq!(report.outcome, Outcome::Refused);
    assert!(
        report.diagnostics.is_empty(),
        "no language diagnostic is invented for a malformed request"
    );
    assert_eq!(
        report.reached,
        Reached::StaticChecking,
        "the walk stopped where conversion happens, before step 7"
    );
}

/// A supplied datum may not read the document it is supplied to.
#[test]
fn a_supplied_reference_is_refused() {
    let source = example("01_MINIMAL_TASK.lcl");
    for expression in [
        "REF(output.value)",
        "[1, REF(input.value)]",
        "-REF(input.value)",
    ] {
        let inputs = Inputs::new().with_text("input.value", expression);
        let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

        assert_eq!(report.outcome, Outcome::Refused, "{expression}");
        let reason = report.inputs[0].reason.as_deref().expect("a reason");
        assert!(
            reason.contains("may not read the document"),
            "{expression} -> {reason}"
        );
    }
}

/// The supplied expression is retained verbatim in the record.
#[test]
fn the_record_keeps_the_expression_as_written() {
    let source = example("01_MINIMAL_TASK.lcl");
    let inputs = Inputs::new().with_text("input.value", "PATH(\"a b\")");
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

    assert_eq!(
        report.inputs[0].expression.as_deref(),
        Some("PATH(\"a b\")")
    );
    assert_eq!(report.inputs[0].value.as_deref(), Some("PATH(\"a b\")"));
}

/// Leading whitespace is refused rather than trimmed away.
///
/// `02_LEXICAL/01` is categorical: "An implementation must not silently repair,
/// normalize, re-indent, re-quote, or otherwise rewrite source before
/// validation." A supplied expression is source. The fragment contract admits
/// "exactly one EXPRESSION ... followed only by optional whitespace", which is
/// why a trailing space is fine and a leading one is not, and a tool that
/// quietly trimmed the leading space would be repairing source on the author's
/// behalf.
#[test]
fn leading_whitespace_is_refused_rather_than_repaired() {
    let source = example("01_MINIMAL_TASK.lcl");

    let trailing = Inputs::new().with_text("input.value", "4  ");
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &trailing);
    assert!(report.inputs[0].accepted(), "trailing space is admitted");

    let leading = Inputs::new().with_text("input.value", "  4");
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &leading);
    assert!(!report.inputs[0].accepted());
    assert_eq!(report.outcome, Outcome::Refused);
}

/// A repeated id keeps the last value and reports one record.
#[test]
fn a_repeated_id_resolves_to_one_datum() {
    let source = example("01_MINIMAL_TASK.lcl");
    let inputs = Inputs::new()
        .with_text("input.value", "1")
        .with_text("input.value", "4");
    let report = engine().validate(&unit("doc.lcl", &source), &MemoryProvider::new(), &inputs);

    assert_eq!(report.inputs.len(), 1);
    assert_eq!(report.inputs[0].value.as_deref(), Some("4"));
}

/// Supplying nothing is a legal, and honest, invocation.
#[test]
fn supplying_nothing_is_legal() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().validate(
        &unit("doc.lcl", &source),
        &MemoryProvider::new(),
        &Inputs::new(),
    );

    assert!(report.inputs.is_empty());
    assert_eq!(report.outcome, Outcome::Accepted);
}

/// `check` reads no supplied data, because nothing before step 7 does.
#[test]
fn check_takes_no_supplied_data() {
    let source = example("01_MINIMAL_TASK.lcl");
    let report = engine().check(&unit("doc.lcl", &source), &MemoryProvider::new());
    assert!(report.inputs.is_empty());
    assert_eq!(report.outcome, Outcome::Accepted);
}
