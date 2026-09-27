//! Embed the Users Manual snapshot: every top-level `*.md` file of the
//! repository's `users_manual/`, in byte order of name, plus its manifest.
//! The desktop manual window serves exactly these bytes, offline.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let manual = manifest_dir.join("../../../users_manual");
    println!("cargo:rerun-if-changed={}", manual.display());
    let mut names: Vec<String> = std::fs::read_dir(&manual)
        .unwrap_or_else(|e| panic!("{}: {e}", manual.display()))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".md"))
        .collect();
    names.sort();
    let mut out = String::from("pub const FILES: &[(&str, &str)] = &[\n");
    for name in &names {
        let path = manual.join(name).canonicalize().unwrap();
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(
            out,
            "    ({name:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    out.push_str("];\n");
    let manifest = manual.join("MANIFEST.json").canonicalize().unwrap();
    println!("cargo:rerun-if-changed={}", manifest.display());
    writeln!(
        out,
        "pub const MANIFEST: &str = include_str!({:?});",
        manifest.display().to_string()
    )
    .unwrap();
    let dest = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("manual_files.rs");
    std::fs::write(dest, out).unwrap();
}
