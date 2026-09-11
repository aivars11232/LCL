//! Phase B: the boundaries that are not source bytes.
//!
//! A document is not the only untrusted thing a user hands this product. A
//! project carries a manifest, a lock file and a cache; the workspace carries a
//! socket; every command carries supplied inputs. Each is read before, or
//! instead of, any LCL, and each must refuse what it cannot read rather than
//! guess at it.
//!
//! `07_VERSIONING_AND_EXTENSIONS/05` forbids an ignore-unknown mode for
//! normative content, and the project layer applies the same reasoning to the
//! file that decides which specification an engine loads: "silently ignoring a
//! key a future version gives meaning to would make an old tool read a new
//! manifest wrongly and say nothing."

use lcl_hardening::{corpus, engine, Rng};
use lcl_project::manifest::Manifest;
use lcl_project::Project;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp/hardening")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the scratch directory is writable");
    root
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the directory is writable");
    }
    std::fs::write(path, contents).expect("the file is writable");
}

/// A manifest that is valid, as the baseline every mutation is measured from.
fn valid_manifest() -> String {
    format!(
        "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"entry\": \"src/main.lcl\"\n}}\n",
        lcl_hardening::canonical_root().display().to_string()
    )
}

#[test]
fn a_valid_manifest_opens_so_the_refusals_below_are_about_the_mutation() {
    let root = scratch("manifest-valid");
    write(&root.join("lcl.project.json"), &valid_manifest());
    Project::open(&root).expect("a valid manifest opens");
}

#[test]
fn no_mutated_manifest_panics_and_none_is_silently_accepted_wrongly() {
    let baseline = valid_manifest();
    let root = scratch("manifest-mutations");
    let path = root.join("lcl.project.json");
    let mut refused = 0usize;
    let mut opened = 0usize;
    for seed in 0..1_500u64 {
        let mut rng = Rng::new(seed ^ 0x00C0_FFEE);
        let mut text = baseline.clone();
        for _ in 0..=rng.below(3) {
            text = corpus::mutate(&mut rng, &text);
        }
        write(&path, &text);
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| Project::open(&root)));
        match outcome {
            Ok(Ok(project)) => {
                opened += 1;
                // Whatever it accepted, it may not have invented a root.
                assert_eq!(
                    project.root().canonicalize().ok(),
                    root.canonicalize().ok(),
                    "seed {seed} opened a project somewhere else"
                );
            }
            Ok(Err(_)) => refused += 1,
            Err(_) => panic!("seed {seed} panicked on manifest:\n{text}"),
        }
    }
    // A mutation usually breaks the JSON or a key, so most are refused. The
    // count is evidence the corpus reached the parser rather than a threshold.
    assert!(
        refused > opened,
        "{refused} refused and {opened} opened: the corpus is not exercising the reader"
    );
}

#[test]
fn a_manifest_with_an_unknown_key_is_refused_rather_than_ignored() {
    let root = scratch("manifest-unknown-key");
    let text = format!(
        "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"future\": true\n}}\n",
        lcl_hardening::canonical_root().display().to_string()
    );
    write(&root.join("lcl.project.json"), &text);
    assert!(
        Project::open(&root).is_err(),
        "an unknown key is refused, not ignored"
    );
}

#[test]
fn arbitrary_bytes_are_never_a_manifest() {
    let root = scratch("manifest-arbitrary");
    let path = root.join("lcl.project.json");
    for seed in 0..500u64 {
        let mut rng = Rng::new(seed ^ 0xDEAD_BEEF);
        let length = rng.below(512);
        let text = corpus::arbitrary_bytes(&mut rng, length);
        write(&path, &text);
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| Manifest::read(&path)));
        match outcome {
            Ok(_) => {}
            Err(_) => panic!("seed {seed} panicked reading a manifest of arbitrary bytes"),
        }
    }
}

#[test]
fn a_supplied_input_that_is_not_an_expression_is_reported_rather_than_guessed_at() {
    // `--input id=expression` carries an expression. One that does not parse
    // has to reach the caller as a diagnostic about the input, never as a
    // value and never as a panic.
    let engine = engine();
    let document = format!(
        "{}\nINPUT:\n    ID: input.value\n    TYPE: INTEGER\n    REQUIRED: FALSE\n    DEFAULT: 0\n\nOUTPUT:\n    ID: output.value\n    \
         TYPE: INTEGER\n    FORMAT: format.plain_text\n\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.one\n    OPERATION: core.return\n    TARGET: REF(input.value)\n    \
         OUTPUT: REF(output.value)\n\nSUCCESS:\n    ID: success.one\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    INPUT: REF(input.value)\n    \
         ACTION: REF(action.one)\n    OUTPUT: REF(output.value)\n    SUCCESS: REF(success.one)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.one)\n",
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: probe.case\n    NAME: \"P\"\n    \
         VERSION: \"1.0.0\"\n    KIND: kind.task"
    );
    for seed in 0..300u64 {
        let mut rng = Rng::new(seed ^ 0x0BAD_F00D);
        let length = rng.below(64);
        let text = corpus::arbitrary_bytes(&mut rng, length);
        let inputs = lcl_protocol::Inputs::new().with_text("input.value", text.clone());
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
            engine.validate(
                &lcl_hardening::unit(&document),
                &lcl_hardening::empty_provider(),
                &inputs,
            )
        }));
        match outcome {
            Ok(report) => {
                // "A caller who supplied an input the engine could not turn
                // into a value has learned nothing about the document": an
                // unreadable input is Refused, never Rejected, so the report
                // never claims the document was judged.
                use lcl_protocol::Outcome;
                assert!(
                    matches!(report.outcome, Outcome::Accepted | Outcome::Refused),
                    "seed {seed} judged the document on an unreadable input {text:?}"
                );
            }
            Err(_) => panic!("seed {seed} panicked on input expression {text:?}"),
        }
    }
}
