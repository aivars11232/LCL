//! Phase B: documents that are hostile in shape rather than in bytes.
//!
//! The fuzz suite feeds the engine input that is wrong. This one feeds it input
//! that is *big* — deeply nested, enormously wide, pathologically repetitive,
//! self-referential — which is the shape that defeats a recursive descent
//! parser, a graph walk or a demand evaluator, and which random mutation almost
//! never produces.
//!
//! Every case here also runs the later stages: `validate` carries a document
//! through the no-effect preflight, and `run` carries it to a terminal status
//! against a host that has been granted nothing. The newest code in the engine
//! is behind those two calls, and a corpus that stops at `check` never reaches
//! it.

use lcl_hardening::{check_report, empty_provider, engine, unit};
use lcl_protocol::{Engine, Inputs, Report};
use lcl_runtime::MockHost;
use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

/// A ceiling that catches non-termination, not slowness.
///
/// This suite is about totality: every case must *end*. How fast the engine is
/// belongs to `performance.rs`, which measures ordinary documents against the
/// thresholds in `LCL_RELEASE_BASELINE.md` 5.2. Some fixtures here are
/// deliberately far outside anything a person writes -- fifty thousand nested
/// operators, a two-hundred-kilobyte literal -- and timing those would be
/// measuring the fixture rather than the engine.
const CASE_CEILING: std::time::Duration = std::time::Duration::from_secs(120);

fn sources_of(source: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    map.insert("fuzz.lcl".to_string(), source.to_string());
    map
}

/// Carry one source through every command, asserting totality and the
/// invariants, and return what `run` reported.
fn all_commands(engine: &Engine, label: &str, source: &str) -> Report {
    let registry = lcl_diagnostics::DiagnosticRegistry::load(engine.spec())
        .expect("the package is authoritative");
    let sources = sources_of(source);
    let started = std::time::Instant::now();

    for command in ["check", "validate", "inspect"] {
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| match command {
            "check" => engine.check(&unit(source), &empty_provider()),
            "validate" => engine.validate(&unit(source), &empty_provider(), &Inputs::new()),
            _ => engine.inspect(&unit(source), &empty_provider(), &Inputs::new()),
        }));
        let report = match outcome {
            Ok(report) => report,
            Err(_) => panic!("{label} panicked during {command}"),
        };
        let violations = check_report(&report, &sources, &registry);
        assert!(
            violations.is_empty(),
            "{label} broke an invariant during {command}: {violations:?}"
        );
    }

    let mut stdlib = engine.stdlib().expect("the standard library assembles");
    let mut host = MockHost::new();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        engine.run(
            &unit(source),
            &empty_provider(),
            &Inputs::new(),
            &mut stdlib,
            &mut host,
        )
    }));
    let report = match outcome {
        Ok(report) => report,
        Err(_) => panic!("{label} panicked during run"),
    };
    let violations = check_report(&report, &sources, &registry);
    assert!(
        violations.is_empty(),
        "{label} broke an invariant during run: {violations:?}"
    );

    let elapsed = started.elapsed();
    assert!(
        elapsed < CASE_CEILING,
        "{label} took {elapsed:?}, past the {CASE_CEILING:?} ceiling for one case"
    );
    report
}

const HEADER: &str =
    "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: adversarial.case\n    \
                      NAME: \"Adversarial\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n";

/// Nested indented bodies, up to the depth this build is declared to survive.
///
/// `LCL_RELEASE_BASELINE.md` records the bound and the two paths that set it.
/// Unlike an expression, a nested body costs four bytes of indentation per
/// level per line, so its depth and the document's size cannot be separated:
/// 2,000 levels is already a 7.7 MB document.
#[test]
fn a_deeply_nested_body_terminates() {
    let engine = engine();
    for depth in [64usize, 256, 512] {
        let mut source = String::from(HEADER);
        source.push_str("\nDATA:\n    ID: data.deep\n    TYPE: OBJECT\n    VALUE:\n");
        for level in 0..depth {
            source.push_str(&"    ".repeat(level + 2));
            source.push_str(&format!("k{level}:\n"));
        }
        source.push_str(&"    ".repeat(depth + 2));
        source.push_str("leaf: 1\n");
        all_commands(&engine, &format!("nesting depth {depth}"), &source);
    }
}

/// Expression nesting, at the depth M2 proved for the parser.
///
/// `04_GRAMMAR/10` states the shape and no limit, and no registered diagnostic
/// permits an implementation-defined nesting rejection, so depth is not this
/// implementation's to cap. Every one of these shapes ended the process by
/// `SIGABRT` before `LCL-TASK-0020` made the static checker's walk and the
/// syntax tree's `Clone` iterative.
#[test]
fn a_deeply_nested_expression_terminates() {
    let engine = engine();
    // Every shape that nests without bound, at the depth M2 proved for the
    // parser. Each of these ended the process by `SIGABRT` before this task.
    let shapes = [
        ("group", "INTEGER"),
        ("collection", "LIST[INTEGER]"),
        ("unary", "BOOLEAN"),
        ("binary", "INTEGER"),
        ("call", "INTEGER"),
        ("round", "INTEGER"),
    ];
    for (name, ty) in shapes {
        // `ROUND` is the one shape whose cost is quadratic in depth, because
        // the row materializes a direct quotient and re-reads its argument to
        // do it. `LCL_RELEASE_BASELINE.md` records the measurement. It is
        // bounded rather than unbounded, so this suite proves totality at a
        // depth that takes seconds rather than spending ten minutes proving
        // the same thing more slowly.
        let deepest = match name {
            "round" => 8_000usize,
            _ => 50_000,
        };
        for depth in [1_024usize, deepest] {
            let value = match name {
                "group" => format!("{}1{}", "(".repeat(depth), ")".repeat(depth)),
                "collection" => format!("{}{}", "[".repeat(depth), "]".repeat(depth)),
                "unary" => format!("{}TRUE", "NOT ".repeat(depth)),
                "binary" => format!("1{}", " + 1".repeat(depth)),
                "call" => format!("{}1{}", "ABS(".repeat(depth), ")".repeat(depth)),
                _ => format!("{}1{}", "ROUND(".repeat(depth), ")".repeat(depth)),
            };
            let source =
                format!("{HEADER}\nDATA:\n    ID: data.deep\n    TYPE: {ty}\n    VALUE: {value}\n");
            all_commands(&engine, &format!("{name} depth {depth}"), &source);
        }
    }
}

#[test]
fn a_deeply_parenthesised_expression_terminates() {
    let engine = engine();
    for depth in [64usize, 1_024, 10_000] {
        let mut value = String::new();
        value.push_str(&"(".repeat(depth));
        value.push('1');
        value.push_str(&")".repeat(depth));
        let source =
            format!("{HEADER}\nDATA:\n    ID: data.deep\n    TYPE: INTEGER\n    VALUE: {value}\n");
        all_commands(&engine, &format!("parenthesis depth {depth}"), &source);
    }
}

#[test]
fn an_enormous_collection_terminates() {
    let engine = engine();
    for width in [1_000usize, 20_000] {
        let members: Vec<String> = (0..width).map(|n| n.to_string()).collect();
        let source = format!(
            "{HEADER}\nDATA:\n    ID: data.wide\n    TYPE: LIST[INTEGER]\n    VALUE: [{}]\n",
            members.join(", ")
        );
        all_commands(&engine, &format!("collection width {width}"), &source);
    }
}

#[test]
fn an_enormous_number_of_declarations_terminates() {
    let engine = engine();
    let mut source = String::from(HEADER);
    for index in 0..2_000 {
        source.push_str(&format!(
            "\nDATA:\n    ID: data.item{index}\n    TYPE: INTEGER\n    VALUE: {index}\n"
        ));
    }
    all_commands(&engine, "2000 declarations", &source);
}

#[test]
fn a_very_long_literal_terminates() {
    let engine = engine();
    for length in [10_000usize, 200_000] {
        let source = format!(
            "{HEADER}\nDATA:\n    ID: data.long\n    TYPE: STRING\n    VALUE: \"{}\"\n",
            "x".repeat(length)
        );
        all_commands(&engine, &format!("literal length {length}"), &source);
    }
    // An unterminated one, which the lexer must not scan forever looking for a
    // close quote that is not there.
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.long\n    TYPE: STRING\n    VALUE: \"{}\n",
        "x".repeat(200_000)
    );
    all_commands(&engine, "unterminated 200000-byte literal", &source);
}

#[test]
fn a_very_long_identifier_terminates() {
    let engine = engine();
    let long = "a".repeat(100_000);
    let source = format!("{HEADER}\nDATA:\n    ID: data.{long}\n    TYPE: INTEGER\n    VALUE: 1\n");
    all_commands(&engine, "100000-byte identifier", &source);
}

#[test]
fn a_reference_cycle_terminates_rather_than_looping() {
    // `03_TYPES_AND_VALUES/01`: "cycles use error.reference.cycle". The point
    // here is termination, not the identifier: a walk that followed the cycle
    // would never return to make any claim at all.
    let engine = engine();
    let mut source = String::from(HEADER);
    for index in 0..200 {
        let next = (index + 1) % 200;
        source.push_str(&format!(
            "\nDATA:\n    ID: data.n{index}\n    TYPE: INTEGER\n    VALUE: REF(data.n{next})\n"
        ));
    }
    all_commands(&engine, "200-node reference cycle", &source);
}

#[test]
fn pathological_unicode_terminates() {
    let engine = engine();
    let cases = [
        (
            "combining marks",
            "a\u{0301}\u{0302}\u{0303}\u{0304}".repeat(2_000),
        ),
        ("right to left", "\u{202E}abc\u{202C}".repeat(2_000)),
        ("astral plane", "\u{1F600}\u{1F4A9}".repeat(2_000)),
        ("zero width", "\u{200B}\u{FEFF}\u{200D}".repeat(2_000)),
        ("lone surrogate escape", "\\uD800".repeat(2_000)),
    ];
    for (label, payload) in cases {
        let source = format!(
            "{HEADER}\nDATA:\n    ID: data.text\n    TYPE: STRING\n    VALUE: \"{payload}\"\n"
        );
        all_commands(&engine, label, &source);
    }
}

#[test]
fn every_canonical_example_runs_to_a_terminal_state_without_a_grant() {
    // A run against a host granted nothing is the ordinary hostile case: the
    // document asks for the world and the world says no. Every example must
    // still reach a stated end rather than a panic or a hang.
    let engine = engine();
    let corpus = lcl_hardening::corpus::canonical_examples(&lcl_hardening::canonical_root());
    for (name, source) in corpus.iter() {
        let report = all_commands(&engine, name, source);
        // "No false success claims": reaching completion is the only state that
        // may carry a terminal status, and the invariant checker has already
        // asserted that. Here the claim is weaker and more important: something
        // was decided.
        assert!(
            report.primary().is_some() || report.terminal_status().is_some(),
            "{name} ended with neither a diagnostic nor a terminal status"
        );
    }
}
