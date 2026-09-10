//! Shared test helpers.
//!
//! Every fixture is built under `target/test-tmp/`, which is the convention the
//! workspace already uses for tests that need real files. Nothing here writes
//! anywhere else, and `canonical/` is only ever read.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

/// A clean throwaway directory under `target/test-tmp/`.
pub fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-tmp")
        .join(name);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear scratch");
    }
    std::fs::create_dir_all(&dir).expect("create scratch");
    dir.canonicalize().expect("scratch canonicalizes")
}

/// Write one file, creating parent directories.
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

/// A minimal manifest naming the canonical package.
pub fn manifest_with(extra: &str) -> String {
    format!(
        "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}{extra}\n}}\n",
        canonical_root().display().to_string()
    )
}
