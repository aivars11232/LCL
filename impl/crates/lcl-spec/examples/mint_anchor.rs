//! Computes the identity digest of a package, for minting a trust anchor.
//!
//! Read-only. The output must be reviewed and pasted into `anchor.rs` by hand:
//! approving a package is a deliberate source change, never an automatic one.
//!
//! Usage: cargo run --offline -p lcl-spec --example mint_anchor [PATH]

use std::path::{Path, PathBuf};

fn main() {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../canonical/LCL_Core_0.1.0")
        });

    match lcl_spec::compute_identity_digest(&root) {
        Ok((digest, count)) => {
            println!("root         : {}", root.display());
            println!("file count   : {count}");
            println!("identity     : {digest}");
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }
}
