//! Running the real binary, in a controlled environment.
//!
//! Every invocation here starts from an empty environment and is given exactly
//! the variables the case is about. That is not tidiness: the tool's whole
//! claim is that nothing is discovered, and a test that inherited the
//! developer's environment could not tell a resolved `--spec` from a leftover
//! `LCL_SPEC`.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary Cargo built for this test run.
pub fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lcl"))
}

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

/// What one invocation produced.
pub struct Run {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    fn of(output: Output) -> Run {
        Run {
            code: output.status.code().expect("the process exited normally"),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }
    }
}

/// Run `lcl` with an empty environment and no working-directory assumptions.
pub fn lcl(args: &[&str]) -> Run {
    lcl_in(&std::env::temp_dir(), args, &[])
}

/// Run `lcl` in `dir`, with an empty environment plus `env`.
pub fn lcl_in(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(binary());
    command.env_clear().current_dir(dir).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    Run::of(command.output().expect("the binary runs"))
}

/// A clean throwaway directory under Cargo's `CARGO_TARGET_TMPDIR`.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear scratch");
    }
    std::fs::create_dir_all(&dir).expect("create scratch");
    dir.canonicalize().expect("scratch canonicalizes")
}

pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(path, contents).expect("write fixture");
}

/// One canonical valid example's bytes.
pub fn example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the example is readable")
}

/// One canonical invalid example's bytes.
pub fn invalid_example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/INVALID").join(name))
        .expect("the example is readable")
}

/// A project holding one document, with a manifest naming the canonical
/// package.
pub fn project(name: &str, document: &str, source: &str) -> PathBuf {
    let root = scratch(name);
    write(root.join(document), source);
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"entry\": {:?}\n}}\n",
            canonical_root().display().to_string(),
            document
        ),
    );
    root
}
