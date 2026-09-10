//! Shared test helpers. Every test runs against the approved package only.

#![allow(dead_code)]

use lcl_protocol::Engine;
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

/// One engine, assembled once for the whole suite.
///
/// Assembly verifies the package and loads seven layers' contracts, so sharing
/// it is what keeps a suite of stage tests fast. A request holds no state, so
/// sharing changes no result.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| Engine::open(canonical_root()).expect("the approved package assembles"))
}

/// One source unit under an explicit identity.
pub fn unit(id: &str, source: &str) -> SourceUnit {
    SourceUnit::new(SourceId::new(id), source.as_bytes())
}

/// A provider holding every canonical valid example, keyed by file name.
///
/// The examples that import name their sibling by file name, so this is the
/// provider that makes `03_IMPORTING_TASK.lcl` resolvable without a filesystem.
pub fn example_provider() -> MemoryProvider {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut provider = MemoryProvider::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("the canonical examples are readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lcl"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        provider.insert(name, std::fs::read(&path).expect("readable"));
    }
    provider
}

/// The bytes of one canonical valid example.
pub fn example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the example is readable")
}

/// The bytes of one canonical invalid example.
pub fn invalid_example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/INVALID").join(name))
        .expect("the example is readable")
}

/// Every canonical valid example, in file-name order.
pub fn valid_examples() -> Vec<String> {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();
    names
}
