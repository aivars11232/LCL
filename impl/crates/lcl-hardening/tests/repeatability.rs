//! Phase C: the same inputs produce the same result, every time and everywhere.
//!
//! `5.3 Determinism`: for the same canonical version, source bytes, explicit
//! inputs, explicit state and explicit capabilities, observable language
//! meaning must not depend on thread scheduling, hash-map iteration order,
//! filesystem enumeration order, wall-clock timing, discovery timing, ambient
//! chat or hidden provider defaults.
//!
//! Three of those are only visible across a process boundary, so this suite
//! runs the real binary as well as the library: a hash seeded per process, a
//! working directory the tool might have come to depend on, and an environment
//! it might read.

use lcl_hardening::{canonical_root, corpus, empty_provider, engine, unit};
use lcl_protocol::Inputs;
use lcl_runtime::MockHost;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The `lcl` binary this test run built.
///
/// `CARGO_BIN_EXE_lcl` is only defined inside the binary's own package, so the
/// path is derived from this test executable: `target/<profile>/deps/<test>`
/// puts the binary two directories up.
fn binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("the test executable has a path");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("lcl")
}

fn example(name: &str) -> PathBuf {
    canonical_root().join("08_EXAMPLES/VALID").join(name)
}

/// One `lcl --machine` invocation, with an empty environment.
fn machine(command: &str, document: &Path, working_directory: &Path) -> String {
    let output = Command::new(binary())
        .arg(command)
        .arg("--machine")
        .arg("--spec")
        .arg(canonical_root())
        .arg(document)
        .current_dir(working_directory)
        .env_clear()
        .output()
        .expect("the binary runs");
    String::from_utf8(output.stdout).expect("machine output is UTF-8")
}

#[test]
fn one_process_produces_one_answer_for_one_source() {
    let engine = engine();
    let corpus = corpus::canonical_examples(&canonical_root());
    for (name, source) in corpus.iter() {
        let first = engine
            .check(&unit(source), &empty_provider())
            .to_json()
            .pretty();
        let second = engine
            .check(&unit(source), &empty_provider())
            .to_json()
            .pretty();
        assert_eq!(first, second, "{name} was judged two different ways");
    }
}

#[test]
fn a_run_against_the_same_host_produces_the_same_record() {
    let engine = engine();
    let corpus = corpus::canonical_examples(&canonical_root());
    for (name, source) in corpus.iter() {
        let mut renders = Vec::new();
        for _ in 0..2 {
            let mut stdlib = engine.stdlib().expect("the standard library assembles");
            let mut host = MockHost::new();
            let report = engine.run(
                &unit(source),
                &empty_provider(),
                &Inputs::new(),
                &mut stdlib,
                &mut host,
            );
            renders.push(report.to_json().pretty());
        }
        assert_eq!(renders[0], renders[1], "{name} ran two different ways");
    }
}

#[test]
fn separate_processes_produce_byte_identical_output() {
    // A new process reseeds every hash map. Output that depended on iteration
    // order would differ here and nowhere else.
    let temporary = std::env::temp_dir();
    for name in ["01_MINIMAL_TASK.lcl", "05_CONDITION_AND_ITERATION.lcl"] {
        let document = example(name);
        for command in ["check", "validate", "inspect"] {
            let mut runs = Vec::new();
            for _ in 0..3 {
                runs.push(machine(command, &document, &temporary));
            }
            assert_eq!(
                runs[0], runs[1],
                "{name} {command} differed across processes"
            );
            assert_eq!(
                runs[1], runs[2],
                "{name} {command} differed across processes"
            );
            assert!(!runs[0].is_empty(), "{name} {command} produced nothing");
        }
    }
}

#[test]
fn the_working_directory_is_not_an_input() {
    // `05_SEMANTICS/02`: "Ambient current directory and implied nearby files do
    // not exist in portable LCL."
    let document = example("03_IMPORTING_TASK.lcl");
    let first = machine("check", &document, &std::env::temp_dir());
    let second = machine("check", &document, Path::new("/"));
    assert_eq!(first, second, "the working directory changed the answer");
}

#[test]
fn every_valid_example_agrees_with_itself_across_processes() {
    let temporary = std::env::temp_dir();
    let corpus = corpus::canonical_examples(&canonical_root());
    for (name, _) in corpus.iter() {
        if name.contains("invalid") {
            continue;
        }
        let document = example(name);
        let first = machine("check", &document, &temporary);
        let second = machine("check", &document, &temporary);
        assert_eq!(first, second, "{name} differed between two runs");
    }
}
