//! Release hardening: the generator, the corpora, and the invariants.
//!
//! ## What this crate is for
//!
//! Every milestone tested what it built. This one tests what a hostile or
//! careless world does to all of it at once: bytes that are not LCL, bytes that
//! are almost LCL, documents that are valid but enormous, values that arrive
//! only at demand, requests that arrive over a socket, and paths that point
//! somewhere they should not.
//!
//! ## Why it is a crate and not a directory of tests
//!
//! Cargo runs an integration test from a package, and the workspace root is not
//! one. The generator and the invariant checks are shared by every suite here,
//! so they live in a library the suites import, rather than being copied into
//! each.
//!
//! ## What it deliberately does not do
//!
//! It decides no language rule, and nothing in the engine depends on it. Every
//! judgement it makes is read from an engine `Report`: the identifier, the
//! stage, the span and the status were all decided by the layer that emitted
//! them. A hardening suite that made its own ruling about what LCL means would
//! be a second implementation, and would drift.
//!
//! ## Determinism
//!
//! Nothing here is random. Every generated input is a pure function of a `u64`
//! seed, so a failure is reproducible from the seed the failure message prints,
//! and no corpus has to be stored to reproduce one.

pub mod corpus;
pub mod invariant;
pub mod rng;

pub use corpus::Corpus;
pub use invariant::{check_report, Violation};
pub use rng::Rng;

use lcl_protocol::Engine;
use lcl_resolver::{MemoryProvider, SourceId, SourceUnit};
use std::path::{Path, PathBuf};

/// The canonical package this build is judged against.
pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("the canonical package is present")
}

/// One engine, opened once against the verified package.
///
/// Opening verifies 176 files against the trust anchor, which is the dominant
/// cost of any single measurement, so a suite opens one engine and reuses it.
pub fn engine() -> Engine {
    Engine::open(canonical_root()).expect("the canonical package is authoritative")
}

/// One source unit under a fixed identity.
pub fn unit(source: &str) -> SourceUnit {
    SourceUnit::new(SourceId::new("fuzz.lcl"), source.as_bytes())
}

/// A provider that answers nothing.
///
/// `05_SEMANTICS/02`: "Ambient current directory and implied nearby files do
/// not exist in portable LCL." A generated document that names an import gets
/// an honest unresolved reference, never a file this suite happened to have.
pub fn empty_provider() -> MemoryProvider {
    MemoryProvider::new()
}
