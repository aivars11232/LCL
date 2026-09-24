//! The gate: what the workspace shows is what the CLI reports.
//!
//! ## Why this is a byte comparison
//!
//! The acceptance criterion is that no UI-only semantic path exists. Comparing
//! selected fields would prove that the fields someone thought to compare
//! agree. Comparing the whole record, byte for byte, against the output of a
//! separate process running the same document proves that there is nothing in
//! one that is not in the other — including a field nobody thought of.
//!
//! The CLI is run as a real subprocess with a cleared environment, so nothing
//! inherited from this test can carry a result into it.

mod common;

use common::{canonical_root, example, send, serve_examples, valid_examples, Running};
use std::path::PathBuf;
use std::process::Command;

/// The `lcl` binary this workspace builds.
///
/// Found from the test executable's own location rather than a fixed path, so
/// it is right for whichever profile the suite is running under.
fn lcl_binary() -> PathBuf {
    let mut path = std::env::current_exe().expect("the test executable has a path");
    path.pop(); // deps
    path.pop(); // debug or release
    path.push("lcl");
    assert!(
        path.is_file(),
        "the lcl binary is missing at {}. `cargo test --workspace --all-targets` \
         builds it; a single-crate test run may not.",
        path.display()
    );
    path
}

/// Run `lcl <command> --machine` over one document, with nothing inherited.
fn cli(command: &str, document: &PathBuf) -> String {
    let output = Command::new(lcl_binary())
        .env_clear()
        .arg(command)
        .arg(document)
        .arg("--machine")
        .arg("--spec")
        .arg(canonical_root())
        .output()
        .expect("the lcl binary runs");
    String::from_utf8(output.stdout).expect("machine output is UTF-8")
}

/// Ask the workspace for the same command over the same bytes.
fn workspace(running: &Running, command: &str, id: &str, body: &str) -> String {
    let reply = send(
        running.address,
        "POST",
        &format!("/api/{command}?t={}&id={id}", running.token),
        &[],
        body.as_bytes(),
    );
    assert_eq!(reply.status, 200, "{command} refused: {}", reply.body);
    reply.body
}

#[test]
fn check_agrees_with_the_cli_byte_for_byte_on_every_valid_example() {
    let (scratch, running) = serve_examples("equivalence-check");
    for name in valid_examples() {
        let source = example(&name);
        assert_eq!(
            workspace(&running, "check", &name, &source),
            cli("check", &scratch.join(&name)),
            "{name}: the workspace and the CLI disagree on check"
        );
    }
}

#[test]
fn inspect_agrees_with_the_cli_byte_for_byte_on_every_valid_example() {
    let (scratch, running) = serve_examples("equivalence-inspect");
    for name in valid_examples() {
        let source = example(&name);
        assert_eq!(
            workspace(&running, "inspect", &name, &source),
            cli("inspect", &scratch.join(&name)),
            "{name}: the workspace and the CLI disagree on inspect"
        );
    }
}

#[test]
fn validate_agrees_with_the_cli_byte_for_byte_on_every_valid_example() {
    let (scratch, running) = serve_examples("equivalence-validate");
    for name in valid_examples() {
        let source = example(&name);
        assert_eq!(
            workspace(&running, "validate", &name, &source),
            cli("validate", &scratch.join(&name)),
            "{name}: the workspace and the CLI disagree on validate"
        );
    }
}

#[test]
fn a_rejected_document_is_rejected_identically_by_both() {
    // Agreement on acceptance is the easy half. A UI that quietly softened a
    // refusal would still pass that, so this is the half that matters.
    let (scratch, running) = serve_examples("equivalence-rejected");

    let broken = [
        (
            "lexical.lcl",
            example("01_MINIMAL_TASK.lcl").replace("TASK:", "tAsK:"),
        ),
        (
            "reference.lcl",
            example("01_MINIMAL_TASK.lcl").replace(
                "TARGET: REF(input.value)",
                "TARGET: REF(input.nothing_declares_this)",
            ),
        ),
        (
            "grammar.lcl",
            example("01_MINIMAL_TASK.lcl").replace("    ID: input.value\n", ""),
        ),
    ];

    for (name, source) in broken {
        scratch.put(name, &source);
        for command in ["check", "inspect"] {
            assert_eq!(
                workspace(&running, command, name, &source),
                cli(command, &scratch.join(name)),
                "{name}: {command} disagrees on a rejected document"
            );
        }
    }
}

#[test]
fn the_navigation_the_ui_reads_is_in_the_cli_output_too() {
    // Navigation was added to the engine for the workspace, so the thing to
    // prove is that it went into the *engine* and not into the UI: the CLI,
    // which was not changed, emits it.
    let (scratch, running) = serve_examples("equivalence-navigation");
    let name = "01_MINIMAL_TASK.lcl";
    let from_cli = cli("inspect", &scratch.join(name));

    assert!(
        from_cli.contains("\"navigation\""),
        "navigation must live in the engine, reachable by any consumer"
    );
    assert_eq!(
        workspace(&running, "inspect", name, &example(name)),
        from_cli
    );
}

#[test]
fn no_route_reshapes_a_report() {
    // Every analysis route returns `Report::to_json` and nothing else. If one
    // of them ever grew a field of its own, the byte comparisons above would
    // fail; this states the rule they are checking.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes.rs"),
    )
    .expect("the route table is readable");

    let analyse = source
        .split("fn analyse(")
        .nth(1)
        .expect("the analyse handler exists");
    let body = &analyse[..analyse.find("\n    }").unwrap_or(analyse.len())];
    assert!(
        body.contains("report.to_json().pretty()"),
        "the analysis route must return the engine's own projection"
    );
}

/// `.lcl.txt` is the same document as `.lcl` in the workspace too.
///
/// For every canonical valid example, the same bytes are stored under both
/// endings. The workspace's record for the `.lcl.txt` document must equal its
/// record for the `.lcl` one, apart from the document's own name, and must
/// equal what the CLI reports for the `.lcl.txt` file byte for byte. A
/// malformed document is refused identically under either ending, and a
/// document of either ending locates the same project.
#[test]
fn both_endings_are_the_same_document_in_the_workspace() {
    let (scratch, running) = serve_examples("equivalence-endings");
    let mut documents: Vec<(String, String)> = valid_examples()
        .into_iter()
        .map(|name| {
            let source = example(&name);
            (name, source)
        })
        .collect();
    documents.push((
        "malformed.lcl".to_string(),
        example("01_MINIMAL_TASK.lcl").replace("TASK:", "tAsK:"),
    ));

    for (name, source) in documents {
        let text_name = format!("{name}.txt");
        scratch.put(&name, &source);
        scratch.put(&text_name, &source);
        for command in ["check", "validate", "inspect"] {
            let native = workspace(&running, command, &name, &source);
            let text = workspace(&running, command, &text_name, &source);
            assert_eq!(
                native.replace(&name, "<document>"),
                text.replace(&text_name, "<document>"),
                "{name}: the workspace's {command} differs between .lcl and .lcl.txt"
            );
            assert_eq!(
                text,
                cli(command, &scratch.join(&text_name)),
                "{text_name}: the workspace and the CLI disagree on {command}"
            );
        }

        let (native_root, native_id) =
            lcl_workspace::Workspace::locate_document(&scratch.join(&name)).expect("locates");
        let (text_root, text_id) =
            lcl_workspace::Workspace::locate_document(&scratch.join(&text_name)).expect("locates");
        assert_eq!(
            native_root, text_root,
            "{name}: both endings open one project"
        );
        assert_eq!(native_id, name);
        assert_eq!(text_id, text_name);
    }
}
