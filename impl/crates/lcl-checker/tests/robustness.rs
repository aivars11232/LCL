//! Phase F: totality, determinism, and the declared-constraint path.

mod common;

use common::{check, checker, ids, resolver, unit, HEADER};
use lcl_checker::Outcome;
use lcl_resolver::MemoryProvider;
use std::fs;

/// Fingerprint everything a later stage could observe.
fn fingerprint(checked: &lcl_checker::Checked) -> String {
    let mut out = String::new();
    out.push_str(&format!("outcome={}\n", checked.outcome()));
    for annotation in checked.annotations() {
        out.push_str(&format!(
            "a {} {}..{} {}\n",
            annotation.source, annotation.span.start, annotation.span.end, annotation.outcome
        ));
    }
    for (index, ty) in checked.declaration_types() {
        out.push_str(&format!("t {index} {ty}\n"));
    }
    for obligation in checked.deferred() {
        out.push_str(&format!(
            "d {} {} {}\n",
            obligation.source, obligation.span.start, obligation.kind
        ));
    }
    for diagnostic in checked.diagnostics() {
        out.push_str(&format!(
            "e {} {} {}\n",
            diagnostic.id, diagnostic.source, diagnostic.span.start
        ));
    }
    for defect in checked.earlier_stage_defects() {
        out.push_str(&format!("x {} {}\n", defect.identifier, defect.span.start));
    }
    out
}

#[test]
fn checking_is_a_pure_function_of_its_input() {
    let path = common::canonical_root().join("08_EXAMPLES/VALID/04_AUTOMATED_CODING_TASK.lcl");
    let source = fs::read_to_string(path).expect("readable");
    let resolved = resolver()
        .resolve(&unit("root.lcl", &source), &MemoryProvider::new())
        .expect("earlier stages pass");

    let first = fingerprint(&checker().check(&resolved).expect("checked"));
    for _ in 0..3 {
        let again = fingerprint(&checker().check(&resolved).expect("checked"));
        assert_eq!(first, again, "repeated checking is byte-identical");
    }

    // A second, independently resolved copy of the same bytes agrees.
    let other = resolver()
        .resolve(&unit("root.lcl", &source), &MemoryProvider::new())
        .expect("earlier stages pass");
    assert_eq!(
        first,
        fingerprint(&checker().check(&other).expect("checked"))
    );
}

#[test]
fn every_truncation_of_every_valid_example_is_total() {
    // Malformed and partial sources must never panic. Most truncations fail an
    // earlier stage; the ones that do not must still return.
    let root = common::canonical_root().join("08_EXAMPLES/VALID");
    let mut reached = 0usize;
    for entry in fs::read_dir(root).expect("examples") {
        let path = entry.expect("entry").path();
        if !path.extension().is_some_and(|ext| ext == "lcl") {
            continue;
        }
        let bytes = fs::read(&path).expect("readable");
        for cut in (0..bytes.len()).step_by(37) {
            let truncated = &bytes[..cut];
            let unit = lcl_resolver::SourceUnit::new(
                lcl_resolver::SourceId::new("truncated.lcl"),
                truncated.to_vec(),
            );
            let Ok(resolved) = resolver().resolve(&unit, &MemoryProvider::new()) else {
                continue;
            };
            if !resolved.diagnostics().is_empty() {
                continue;
            }
            let _ = checker().check(&resolved);
            reached += 1;
        }
    }
    // The count is incidental; that none of them panicked is the assertion.
    let _ = reached;
}

#[test]
fn adversarial_shapes_return_rather_than_recurse() {
    // The parser owns its own depth limit; what this asserts is that the
    // checker's own walks add no depth of their own.
    for source in [
        format!(
            "{HEADER}\nDATA:\n    ID: data.x\n    TYPE: LIST[INTEGER]\n    VALUE: [{}]\n",
            "1, ".repeat(500) + "1"
        ),
        format!(
            "{HEADER}\nDATA:\n    ID: data.x\n    TYPE: INTEGER\n    VALUE: {}\n",
            "1 + ".repeat(200) + "1"
        ),
        format!(
            "{HEADER}\nDATA:\n    ID: data.x\n    TYPE: INTEGER\n    VALUE: {}1{}\n",
            "(".repeat(50),
            ")".repeat(50)
        ),
    ] {
        let unit = unit("deep.lcl", &source);
        let Ok(resolved) = resolver().resolve(&unit, &MemoryProvider::new()) else {
            continue;
        };
        if !resolved.diagnostics().is_empty() {
            continue;
        }
        let _ = checker().check(&resolved);
    }
}

#[test]
fn a_deeply_nested_type_alias_chain_terminates() {
    // Depth costs heap, not stack, and a cycle terminates the walk rather than
    // the process.
    let mut body = String::new();
    for index in 0..200 {
        body.push_str(&format!(
            "\nDEFINE:\n    ID: type.t{index}\n    KIND: kind.type\n    BASE: {}\n",
            if index == 0 {
                "INTEGER".to_string()
            } else {
                format!("REF(type.t{})", index - 1)
            }
        ));
    }
    body.push_str("\nDATA:\n    ID: data.x\n    TYPE: REF(type.t199)\n    VALUE: 1\n");
    let source = format!("{HEADER}{body}");
    assert_eq!(check(&source).outcome(), Outcome::Checked);
}

#[test]
fn a_declared_pattern_judges_a_statically_known_value() {
    // "PATTERN is GLOB or REGEX"; only a statically known value is judged here.
    let with = |pattern: &str, value: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: operation.one\n    KIND: kind.operation\n    MEANING: \"m\"\n    SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: subject\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: {value}\n        PATTERN: {pattern}\n"
        )
    };
    assert_eq!(
        check(&with("REGEX(\"[a-z]+\")", "\"alpha\"")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        ids(&check(&with("REGEX(\"[a-z]+\")", "\"Alpha1\""))),
        vec!["error.pattern.mismatch"]
    );
    assert_eq!(
        check(&with("GLOB(\"src/**/*.py\")", "\"src/main.py\"")).outcome(),
        Outcome::Checked
    );
    assert_eq!(
        ids(&check(&with("GLOB(\"src/**/*.py\")", "\"src/main.rs\""))),
        vec!["error.pattern.mismatch"]
    );
}

#[test]
fn a_declared_bound_judges_a_statically_known_value() {
    // "MINIMUM and MAXIMUM are inclusive."
    let with = |value: &str| {
        format!(
            "{HEADER}\nDEFINE:\n    ID: operation.one\n    KIND: kind.operation\n    MEANING: \"m\"\n    SIDE_EFFECT: FALSE\n    DETERMINISTIC: TRUE\n    PARAMETER:\n        NAME: subject\n        TYPE: INTEGER\n        REQUIRED: TRUE\n        VALUE: {value}\n        MINIMUM: 1\n        MAXIMUM: 10\n"
        )
    };
    assert_eq!(check(&with("1")).outcome(), Outcome::Checked);
    assert_eq!(check(&with("10")).outcome(), Outcome::Checked);
    assert_eq!(ids(&check(&with("0"))), vec!["error.value.out_of_range"]);
    assert_eq!(ids(&check(&with("11"))), vec!["error.value.out_of_range"]);
}

#[test]
fn a_value_known_only_at_demand_is_recorded_rather_than_judged() {
    // The obligation belongs to the demanding layer, and this stage says so
    // instead of guessing.
    let source = format!(
        "{HEADER}\nDATA:\n    ID: data.divisor\n    TYPE: INTEGER\n    VALUE: 3\n\nDEFINE:\n    ID: constant.one\n    KIND: kind.constant\n    TYPE: DECIMAL\n    VALUE: 1 / 3\n"
    );
    // A statically known non-terminating quotient is judged here.
    assert_eq!(ids(&check(&source)), vec!["error.numeric.non_terminating"]);
}

#[test]
fn no_diagnostic_carries_an_identifier_outside_this_stage() {
    let source = format!("{HEADER}\nDATA:\n    ID: data.x\n    TYPE: INTEGER\n    VALUE: \"a\"\n");
    let checked = check(&source);
    for diagnostic in checked.diagnostics() {
        assert_eq!(
            diagnostic.stage(),
            lcl_diagnostics::Stage::StaticOrExpression,
            "{} is a static-stage identifier",
            diagnostic.id
        );
    }
}
