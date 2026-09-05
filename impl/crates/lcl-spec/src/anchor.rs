//! External immutable trust anchor for an approved LCL package.
//!
//! # The gap this closes
//!
//! `MANIFEST.json` and `SHA256SUMS.txt` live *inside* the package they describe.
//! Verifying a package against them proves only **internal self-consistency**:
//! anyone who alters a payload file and regenerates both records produces a
//! package that verifies perfectly and is not the approved release.
//!
//! An anchor breaks that circularity by holding the expected package identity
//! **outside** the package — compiled into this crate, where the package's own
//! metadata cannot reach it.
//!
//! # Identity digest
//!
//! The identity digest is a domain-separated digest over the package's complete
//! file inventory:
//!
//! ```text
//! SHA-256( "LCL-PACKAGE-IDENTITY-V1\n"
//!          || for each file, in ascending package-root-relative POSIX path order:
//!             "<lowercase sha256 hex>  <path>\n" )
//! ```
//!
//! Every regular file under the package root is included, `MANIFEST.json`,
//! `VALIDATION_REPORT.txt` and `SHA256SUMS.txt` among them. Nothing is
//! self-excluded, because the digest is not stored in the package. Any change to
//! any byte, any added or removed file, or any rename changes the digest.

use crate::sha256::{hex_digest, Sha256};

/// Domain separation tag. Changing this changes every digest, so it is versioned.
pub const IDENTITY_DOMAIN: &str = "LCL-PACKAGE-IDENTITY-V1\n";

/// An approved package identity, pinned outside the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustAnchor {
    /// Human-readable label for the approved release.
    pub label: &'static str,
    /// Formal specification version the anchor approves.
    pub formal_version: &'static str,
    /// Exact number of regular files in the approved package.
    pub package_file_count: usize,
    /// Expected identity digest, lowercase hex.
    pub identity_digest: &'static str,
}

/// The single approved package for this build.
///
/// This constant is the trust root. It is deliberately *not* read from disk,
/// not configurable at runtime, and not derived from the package. Approving a
/// different package is a source change subject to review.
///
/// Corresponds to `canonical/LCL_Core_0.1.0` as released on 2026-09-05, whose
/// external archive `LCL_Core_0.1.0_Bare_Language_2026-09-05.zip` has SHA-256
/// `1385ae06539d30b2bd710a70f10e3373c095e032cf1cfaa39686faf8dfd698c5`.
pub const APPROVED_PACKAGE: TrustAnchor = TrustAnchor {
    label: "LCL Core 0.1.0 Bare Language Specification Release (2026-09-05)",
    formal_version: "0.1.0",
    package_file_count: 176,
    identity_digest: "00d648b162939d06c44838481a67c39bc12c64bdd6d105035c24150148fe67ed",
};

/// Accumulates a package identity digest from `(path, content-hash)` pairs.
///
/// Callers must supply entries in ascending path order; [`IdentityBuilder::finish`]
/// is order-sensitive by design, because inventory order is part of identity.
pub struct IdentityBuilder {
    hasher: Sha256,
    count: usize,
    last_path: Option<String>,
    out_of_order: bool,
}

impl Default for IdentityBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityBuilder {
    pub fn new() -> Self {
        let mut hasher = Sha256::new();
        hasher.update(IDENTITY_DOMAIN.as_bytes());
        Self {
            hasher,
            count: 0,
            last_path: None,
            out_of_order: false,
        }
    }

    /// Add one file's package-root-relative path and its content digest.
    pub fn push(&mut self, path: &str, content_hex: &str) {
        if let Some(prev) = &self.last_path {
            if path <= prev.as_str() {
                self.out_of_order = true;
            }
        }
        self.last_path = Some(path.to_string());
        self.count += 1;
        self.hasher.update(content_hex.as_bytes());
        self.hasher.update(b"  ");
        self.hasher.update(path.as_bytes());
        self.hasher.update(b"\n");
    }

    pub fn file_count(&self) -> usize {
        self.count
    }

    /// True if `push` was ever called out of ascending order, which would make
    /// the digest meaningless.
    pub fn is_out_of_order(&self) -> bool {
        self.out_of_order
    }

    pub fn finish(self) -> String {
        crate::sha256::to_hex(&self.hasher.finalize())
    }
}

/// Convenience: digest an in-memory inventory. Sorts defensively.
pub fn identity_digest_of(entries: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut b = IdentityBuilder::new();
    for (path, hash) in sorted {
        b.push(path, hash);
    }
    b.finish()
}

/// Digest of the empty inventory, used in tests as a sanity value.
pub fn empty_identity() -> String {
    hex_digest(IDENTITY_DOMAIN.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_separation_applies() {
        // An empty inventory is not the digest of the empty string.
        assert_ne!(empty_identity(), hex_digest(b""));
        assert_eq!(empty_identity(), hex_digest(IDENTITY_DOMAIN.as_bytes()));
    }

    #[test]
    fn digest_is_order_independent_via_helper_but_order_sensitive_in_builder() {
        let a = ("a.txt".to_string(), "11".repeat(32));
        let b = ("b.txt".to_string(), "22".repeat(32));
        // Helper sorts, so argument order does not matter.
        assert_eq!(
            identity_digest_of(&[a.clone(), b.clone()]),
            identity_digest_of(&[b.clone(), a.clone()])
        );
        // Builder records misuse.
        let mut bad = IdentityBuilder::new();
        bad.push(&b.0, &b.1);
        bad.push(&a.0, &a.1);
        assert!(bad.is_out_of_order());
    }

    #[test]
    fn any_change_changes_the_digest() {
        let base = vec![
            ("x.txt".to_string(), "aa".repeat(32)),
            ("y.txt".to_string(), "bb".repeat(32)),
        ];
        let d0 = identity_digest_of(&base);

        // changed content
        let mut c = base.clone();
        c[0].1 = "ab".repeat(32);
        assert_ne!(identity_digest_of(&c), d0);

        // renamed path
        let mut r = base.clone();
        r[0].0 = "z.txt".to_string();
        assert_ne!(identity_digest_of(&r), d0);

        // added file
        let mut a = base.clone();
        a.push(("w.txt".to_string(), "cc".repeat(32)));
        assert_ne!(identity_digest_of(&a), d0);

        // removed file
        let rm = vec![base[0].clone()];
        assert_ne!(identity_digest_of(&rm), d0);
    }

    /// Swapping two files' contents must not collide with the original.
    #[test]
    fn path_binding_prevents_content_swap() {
        let base = vec![
            ("a.txt".to_string(), "11".repeat(32)),
            ("b.txt".to_string(), "22".repeat(32)),
        ];
        let swapped = vec![
            ("a.txt".to_string(), "22".repeat(32)),
            ("b.txt".to_string(), "11".repeat(32)),
        ];
        assert_ne!(identity_digest_of(&base), identity_digest_of(&swapped));
    }
}
