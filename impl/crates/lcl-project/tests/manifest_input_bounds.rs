//! Q-JSON: what a project manifest can do to the process that reads it.
//!
//! `lcl.project.json` is read from the user's own project directory, so its
//! bytes are ordinary input rather than trusted content. The JSON reader that
//! parses it descends recursively for each nested array or object, which is the
//! obvious shape for a reader of closed registry files and a different
//! proposition for a file anyone can write.
//!
//! The question this asks is narrow and deliberately answerable: does a
//! manifest a person could actually write take this process down, or does it
//! produce a diagnostic like any other malformed file? An answer of "it is
//! fine" closes the question as well as an answer of "it is not"; what would
//! not close it is replacing the parser because recursion exists.
//!
//! Each depth runs in a **child process** with its own stack, so a crash is
//! observed as a crash rather than taking the test runner with it, and the
//! depths are small enough to be quick.

use std::process::{Command, Stdio};

/// A manifest whose `spec` value is `depth` nested arrays.
fn nested_manifest(depth: usize) -> String {
    format!(
        "{{\"format\":\"lcl.project/1\",\"spec\":{}{}{}}}",
        "[".repeat(depth),
        "\"x\"",
        "]".repeat(depth)
    )
}

/// Parse one manifest in a child process and report how that child ended.
///
/// Returns `Ok(())` when the child exited normally, whatever it decided about
/// the manifest, and `Err` with a description when it did not.
fn parse_in_child(depth: usize) -> Result<String, String> {
    let exe = std::env::args().next().expect("this test binary");
    let output = Command::new(exe)
        .arg("--exact")
        .arg("child_parses_nested_manifest")
        .arg("--nocapture")
        .arg("--ignored")
        .env("LCL_Q_JSON_DEPTH", depth.to_string())
        .stdin(Stdio::null())
        .output()
        .expect("the child test process starts");

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = output.status.signal() {
            return Err(format!(
                "the child was killed by signal {signal} at depth {depth}; \
                 stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    Err(format!(
        "the child exited {:?} at depth {depth}; stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// The child half: parse one manifest at the depth the environment names.
///
/// Ignored, so it runs only when the parent invokes it by name.
#[test]
#[ignore = "driven by its parent, which supplies the depth"]
fn child_parses_nested_manifest() {
    let depth: usize = std::env::var("LCL_Q_JSON_DEPTH")
        .expect("the parent supplies a depth")
        .parse()
        .expect("a depth");
    let directory = std::env::temp_dir().join(format!("lcl-qjson-{}-{depth}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("owned scratch");
    let path = directory.join(lcl_project::MANIFEST_FILE);
    std::fs::write(&path, nested_manifest(depth)).expect("manifest written");

    // Whatever it decides is fine. Surviving the decision is the question.
    let outcome = lcl_project::Project::open(&directory);
    println!(
        "depth {depth}: {}",
        match &outcome {
            Ok(_) => "accepted".to_string(),
            Err(error) => format!("rejected: {error}"),
        }
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// An ordinary manifest, and ordinary nesting, are read without incident.
#[test]
fn a_manifest_with_ordinary_nesting_is_read_normally() {
    for depth in [0usize, 1, 8, 64] {
        let report = parse_in_child(depth)
            .unwrap_or_else(|failure| panic!("ordinary nesting must be survivable: {failure}"));
        assert!(
            report.contains(&format!("depth {depth}:")),
            "the child reported: {report}"
        );
    }
}

/// Deep nesting a person could write into their own project file.
///
/// This is the probe the question turns on. If the reader descends without a
/// bound, a file of this shape is not a diagnostic; it is the end of the
/// process, and a project shell that dies on `lcl.project.json` cannot report
/// anything about it.
#[test]
fn deeply_nested_manifest_input_does_not_end_the_process() {
    for depth in [1_000usize, 10_000, 100_000] {
        if let Err(failure) = parse_in_child(depth) {
            panic!(
                "a project manifest must be rejected or accepted, never fatal. \
                 {failure}"
            );
        }
    }
}

/// The paths a manifest actually carries are still checked, whatever the
/// nesting question turns out to be.
#[test]
fn a_manifest_that_is_not_an_object_is_refused() {
    let directory = std::env::temp_dir().join(format!("lcl-qjson-shape-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("owned scratch");
    std::fs::write(directory.join(lcl_project::MANIFEST_FILE), "[1,2,3]").expect("written");
    assert!(
        lcl_project::Project::open(&directory).is_err(),
        "a manifest that is not an object is not a manifest"
    );
    let _ = std::fs::remove_dir_all(&directory);
}
