//! Shared test helpers. Every test checks against the approved package only.

#![allow(dead_code)]

use lcl_runtime::Contracts;
use lcl_spec::SpecPackage;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub fn canonical_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.1.0")
        .canonicalize()
        .expect("canonical package must be present")
}

pub fn spec() -> &'static SpecPackage {
    static SPEC: OnceLock<SpecPackage> = OnceLock::new();
    SPEC.get_or_init(|| SpecPackage::open(canonical_root()).expect("approved package opens"))
}

pub fn contracts() -> &'static Contracts {
    static CONTRACTS: OnceLock<Contracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        Contracts::load(spec()).expect("runtime contracts load from the approved package")
    })
}
