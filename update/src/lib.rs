//! LCL's updater, the PC side of Update System V1.
//!
//! One official source, the latest stable GitHub Release of `aivars11232/LCL`;
//! one trust root, the update signing keys compiled into this build; and one
//! statement of what a release holds, its signed manifest. An update is shown
//! only after its manifest's signature verifies, installed only after the
//! artifact matches the size and SHA-256 that manifest signed, and refused
//! whenever any of that cannot be established.

pub mod apply;
pub mod check;
pub mod http;
pub mod json;
pub mod manifest;
pub mod source;
pub mod stage;
pub mod state;
pub mod trust;
pub mod version;

/// The update protocol this updater implements. A manifest whose
/// `pc.required_updater_version` is newer asks for a manual installation.
pub const UPDATER_PROTOCOL: u64 = 1;

/// The LCL product version of this build: every binary of one release reports
/// the same one, which the release builder checks.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The PC architecture this build installs for, as manifests name it.
pub const ARCHITECTURE: &str = "x86_64-linux";
