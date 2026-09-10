//! Commands, exit codes and the refusals.
//!
//! The exit table is a promise to whatever script runs this tool, so it is
//! pinned here case by case. The distinctions it makes — a rejected document,
//! a completed run that did not succeed, a bad command line, an unusable
//! environment — are the ones a pipeline has to be able to tell apart.

mod common;

use common::{canonical_root, example, invalid_example, lcl, lcl_in, project, scratch, write};

const SUCCESS: i32 = 0;
const REJECTED: i32 = 1;
const NOT_SUCCEEDED: i32 = 2;
const USAGE: i32 = 3;
const ENVIRONMENT: i32 = 4;

fn spec() -> String {
    canonical_root().display().to_string()
}

#[test]
fn help_is_the_default_and_succeeds() {
    for args in [vec![], vec!["help"], vec!["--help"], vec!["-h"]] {
        let run = lcl(&args);
        assert_eq!(run.code, SUCCESS, "{args:?}");
        assert!(run.stdout.contains("USAGE"), "{args:?}");
        assert!(run.stdout.contains("EXIT CODES"), "{args:?}");
    }
}

#[test]
fn version_reports_the_protocol_and_the_language() {
    let run = lcl(&["version"]);
    assert_eq!(run.code, SUCCESS);
    assert!(run.stdout.contains("protocol lcl.engine/1"));
    assert!(run.stdout.contains("language 0.1.0"));
}

#[test]
fn a_valid_document_checks_clean() {
    let root = project("cli_check_ok", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check", "main.lcl"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("passed every stage"));
}

#[test]
fn a_rejected_document_exits_one() {
    let root = project(
        "cli_check_bad",
        "main.lcl",
        &invalid_example("07_TYPE_MISMATCH.invalid.lcl"),
    );
    let run = lcl_in(&root, &["check", "main.lcl"], &[]);
    assert_eq!(run.code, REJECTED, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("was rejected"));
    assert!(
        run.stdout.contains("static_or_expression"),
        "the registered stage is named: {}",
        run.stdout
    );
}

/// A run that completes without succeeding is not an error.
///
/// `05_SEMANTICS/10` keeps producer completion and domain outcome apart. The
/// document below runs to completion and its declared `VERIFY` records FALSE,
/// which is an ordinary outcome and gets its own code rather than sharing one
/// with a rejection.
#[test]
fn a_run_that_does_not_succeed_exits_two() {
    let source = example("01_MINIMAL_TASK.lcl").replace("ASSERT: REF(output.value) == 8", "ASSERT: REF(output.value) == 9");
    let root = project("cli_run_failed", "main.lcl", &source);
    let run = lcl_in(&root, &["run", "main.lcl"], &[]);
    assert_eq!(run.code, NOT_SUCCEEDED, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("status."));
    assert!(!run.stdout.contains("status.succeeded"));
}

#[test]
fn a_succeeding_run_exits_zero() {
    let root = project("cli_run_ok", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["run", "main.lcl"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("status.succeeded"));
    assert!(run.stdout.contains("OUTPUT output.value published = 8"));
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let run = lcl(&["frobnicate"]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("unknown command"));
}

#[test]
fn an_unknown_option_is_a_usage_error() {
    let root = project("cli_bad_option", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check", "--wat", "main.lcl"], &[]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("unknown option"));
}

#[test]
fn an_option_without_its_value_is_a_usage_error() {
    let run = lcl(&["check", "--spec"]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("needs a value"));
}

#[test]
fn two_documents_are_a_usage_error() {
    let root = project("cli_two_docs", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check", "main.lcl", "other.lcl"], &[]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("at most one document"));
}

/// With no specification package named anywhere, the command stops.
///
/// It does not search for one. An engine that found its own authority by
/// looking around could not attribute a result to an exact specification.
#[test]
fn a_missing_specification_is_an_environment_error() {
    let root = scratch("cli_no_spec");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check", "main.lcl"], &[]);
    assert_eq!(run.code, ENVIRONMENT);
    assert!(run.stderr.contains("no specification package"));
}

#[test]
fn an_unreadable_document_is_an_environment_error() {
    let root = project("cli_no_doc", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check", "absent.lcl"], &[]);
    assert_eq!(run.code, ENVIRONMENT);
    assert!(run.stderr.contains("not readable"));
}

/// The specification is found from the flag, the variable, or the manifest.
#[test]
fn the_specification_comes_from_exactly_three_places() {
    let source = example("01_MINIMAL_TASK.lcl");

    // 1. the flag, with no manifest and no variable.
    let bare = scratch("cli_spec_flag");
    write(bare.join("main.lcl"), &source);
    let run = lcl_in(&bare, &["check", "--spec", &spec(), "main.lcl"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);

    // 2. the environment variable.
    let run = lcl_in(&bare, &["check", "main.lcl"], &[("LCL_SPEC", &spec())]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);

    // 3. the manifest.
    let root = project("cli_spec_manifest", "main.lcl", &source);
    let run = lcl_in(&root, &["check", "main.lcl"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
}

/// The flag wins over the variable.
#[test]
fn the_flag_outranks_the_environment() {
    let root = scratch("cli_spec_order");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(
        &root,
        &["check", "--spec", &spec(), "main.lcl"],
        &[("LCL_SPEC", "/nonexistent/package")],
    );
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
}

/// A package that is not the approved one is refused.
#[test]
fn an_unapproved_specification_package_is_refused() {
    let fake = scratch("cli_fake_spec");
    write(fake.join("MANIFEST.json"), "{}");
    let root = scratch("cli_fake_spec_project");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));

    let run = lcl_in(
        &root,
        &["check", "--spec", &fake.display().to_string(), "main.lcl"],
        &[],
    );
    assert_eq!(run.code, ENVIRONMENT);
    assert!(run.stderr.contains("specification package did not load"));
}

/// A document named by the manifest needs no argument.
#[test]
fn the_manifest_entry_is_used_when_no_document_is_named() {
    let root = project("cli_entry", "src/main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["check"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("src/main.lcl"));
}

/// With no entry and no argument, the command asks rather than guessing.
#[test]
fn no_document_and_no_entry_is_a_usage_error() {
    let root = scratch("cli_no_entry");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
            spec()
        ),
    );
    let run = lcl_in(&root, &["check"], &[]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("no document"));
}

/// Every command stops where its canonical boundary is.
#[test]
fn each_command_stops_at_its_own_stage() {
    // A validation-stage defect: `check` does not reach it, `validate` does.
    let root = project(
        "cli_stage_boundary",
        "main.lcl",
        &invalid_example("08_HARD_CONFLICT.invalid.lcl"),
    );
    assert_eq!(lcl_in(&root, &["check", "main.lcl"], &[]).code, SUCCESS);
    assert_eq!(lcl_in(&root, &["validate", "main.lcl"], &[]).code, REJECTED);
    assert_eq!(lcl_in(&root, &["run", "main.lcl"], &[]).code, REJECTED);
}

/// A supplied input that is not one expression refuses the request.
#[test]
fn a_bad_input_is_a_usage_error() {
    let root = project("cli_bad_input", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(
        &root,
        &["validate", "--input", "input.value=1 2", "main.lcl"],
        &[],
    );
    assert_eq!(run.code, USAGE, "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("not one LCL expression"));
    assert!(run.stdout.contains("was not judged"));
}

#[test]
fn an_input_without_an_equals_sign_is_a_usage_error() {
    let root = project("cli_input_shape", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let run = lcl_in(&root, &["validate", "--input", "novalue", "main.lcl"], &[]);
    assert_eq!(run.code, USAGE);
    assert!(run.stderr.contains("<id>=<expression>"));
}

/// A supplied datum reaches the document.
#[test]
fn a_supplied_input_reaches_the_run() {
    let source = example("01_MINIMAL_TASK.lcl").replace(
        "    TYPE: INTEGER\n    VALUE: 4\n",
        "    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 4\n",
    );
    let root = project("cli_input", "main.lcl", &source);

    let ok = lcl_in(&root, &["run", "--input", "input.value=4", "main.lcl"], &[]);
    assert_eq!(ok.code, SUCCESS, "{}{}", ok.stdout, ok.stderr);

    let not_ok = lcl_in(&root, &["run", "--input", "input.value=5", "main.lcl"], &[]);
    assert_eq!(not_ok.code, NOT_SUCCEEDED, "{}", not_ok.stdout);
}

/// `--input=id=expr` and `--input id=expr` are the same option.
#[test]
fn inline_and_separated_option_values_agree() {
    let root = project("cli_option_forms", "main.lcl", &example("01_MINIMAL_TASK.lcl"));
    let separated = lcl_in(&root, &["check", "--spec", &spec(), "main.lcl"], &[]);
    let inline = lcl_in(&root, &["check", &format!("--spec={}", spec()), "main.lcl"], &[]);
    assert_eq!(separated.code, inline.code);
    assert_eq!(separated.stdout, inline.stdout);
}

/// The exit code table in `help` is the table the tool uses.
#[test]
fn help_documents_the_codes_the_tool_returns() {
    let text = lcl(&["help"]).stdout;
    for code in [SUCCESS, REJECTED, NOT_SUCCEEDED, USAGE, ENVIRONMENT] {
        assert!(
            text.contains(&format!("    {code}    ")),
            "help documents exit code {code}"
        );
    }
}
