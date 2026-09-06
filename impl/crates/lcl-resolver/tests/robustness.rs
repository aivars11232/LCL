//! Determinism and totality.
//!
//! The acceptance criterion is that the same explicit source set yields the
//! same bindings and graph regardless of enumeration or hash iteration order,
//! and that no input — however malformed — panics.

mod common;

use common::{canonical_root, resolver, unit};
use lcl_resolver::{
    LoadError, MemoryProvider, Resolved, SourceId, SourceProvider, SourceRequest, SourceUnit,
};

/// A provider that answers from a reversed, deliberately differently ordered
/// backing store, to prove enumeration order cannot reach the result.
struct Reversed {
    units: Vec<(String, Vec<u8>)>,
}

impl SourceProvider for Reversed {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError> {
        let text = request.reference.text();
        // Search from the end, and accept either an exact key or a sibling of
        // the requesting unit.
        let base = match request.origin.as_str().rfind('/') {
            Some(i) => format!("{}/{text}", &request.origin.as_str()[..i]),
            None => text.to_string(),
        };
        for (key, bytes) in self.units.iter().rev() {
            if key == text || *key == base {
                return Ok(SourceUnit::new(SourceId::new(key.clone()), bytes.clone()));
            }
        }
        Err(LoadError::new(format!("no unit for {text}")))
    }
}

fn fingerprint(resolved: &Resolved) -> String {
    let mut out = String::new();
    out.push_str("units:");
    for u in resolved.units() {
        out.push_str(&format!(" {}@{}", u.id(), &u.digest()[..8]));
    }
    out.push_str("\npaths:");
    for p in resolved.paths() {
        out.push_str(&format!(" {}/{}", p.unit, p.prefixes.join(".")));
    }
    out.push_str("\ndecls:");
    for d in resolved.declarations().all() {
        out.push_str(&format!(" {}:{}", d.block, d.id.qualified()));
    }
    out.push_str("\nbindings:");
    for b in resolved.bindings() {
        out.push_str(&format!(
            " {}@{}->{:?}",
            b.text,
            b.span.start,
            b.resolved_id.as_ref().map(|i| i.qualified())
        ));
    }
    out.push_str("\ngraph:");
    for n in resolved.graph().nodes() {
        out.push_str(&format!(" {:?}:{}@{}", n.kind, n.block, n.span.start));
    }
    out.push_str("\ndiagnostics:");
    for d in resolved.diagnostics() {
        out.push_str(&format!(" {}@{}:{}", d.id, d.source, d.span.start));
    }
    out
}

fn example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn provider_enumeration_order_cannot_reach_the_result() {
    let root = example("03_IMPORTING_TASK.lcl");
    let lib = example("02_IMPORT_LIBRARY.lcl");
    let root_unit = unit("03_IMPORTING_TASK.lcl", &root);

    let mut forward = MemoryProvider::new();
    forward.insert("02_IMPORT_LIBRARY.lcl", lib.as_bytes());
    forward.insert("zz_decoy.lcl", b"decoy".to_vec());
    let a = resolver().resolve(&root_unit, &forward).expect("resolves");

    let reversed = Reversed {
        units: vec![
            ("zz_decoy.lcl".to_string(), b"decoy".to_vec()),
            ("02_IMPORT_LIBRARY.lcl".to_string(), lib.into_bytes()),
        ],
    };
    let b = resolver().resolve(&root_unit, &reversed).expect("resolves");

    assert_eq!(fingerprint(&a), fingerprint(&b));
}

#[test]
fn repeated_resolution_is_byte_identical() {
    let root = example("04_AUTOMATED_CODING_TASK.lcl");
    let u = unit("root.lcl", &root);
    let provider = MemoryProvider::new();
    let first = fingerprint(&resolver().resolve(&u, &provider).expect("resolves"));
    for _ in 0..8 {
        assert_eq!(
            fingerprint(&resolver().resolve(&u, &provider).expect("resolves")),
            first
        );
    }
}

#[test]
fn unreferenced_provider_units_never_enter_resolution() {
    // The provider holds four documents the root never names.
    let root = example("01_MINIMAL_TASK.lcl");
    let mut provider = MemoryProvider::new();
    for name in [
        "02_IMPORT_LIBRARY.lcl",
        "04_AUTOMATED_CODING_TASK.lcl",
        "05_CONDITION_AND_ITERATION.lcl",
        "ambient.lcl",
    ] {
        let body = if name == "ambient.lcl" {
            "LCL:\n    VERSION: \"0.1.0\"\n".to_string()
        } else {
            example(name)
        };
        provider.insert(name, body.into_bytes());
    }
    let resolved = resolver()
        .resolve(&unit("root.lcl", &root), &provider)
        .expect("resolves");
    assert_eq!(resolved.unit_count(), 1);
    assert_eq!(resolved.paths().len(), 1);
    assert!(resolved.imports().is_empty());
}

#[test]
fn malformed_sources_never_panic() {
    // Every single-byte truncation and a byte mutation of each valid example,
    // resolved against a provider that holds them all.
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("examples")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();

    let mut provider = MemoryProvider::new();
    for name in &names {
        provider.insert(name.clone(), std::fs::read(dir.join(name)).expect("read"));
    }

    for name in &names {
        let bytes = std::fs::read(dir.join(name)).expect("read");
        // Truncations, sampled so the test stays quick but covers every region.
        let step = (bytes.len() / 64).max(1);
        for end in (0..bytes.len()).step_by(step) {
            let u = SourceUnit::new(SourceId::new(name.clone()), bytes[..end].to_vec());
            let _ = resolver().resolve(&u, &provider);
        }
        // Byte mutations at the same sample points.
        for at in (0..bytes.len()).step_by(step) {
            for replacement in [0u8, b'\t', b'"', b'[', 0x80, 0xff] {
                let mut mutated = bytes.clone();
                mutated[at] = replacement;
                let u = SourceUnit::new(SourceId::new(name.clone()), mutated);
                let _ = resolver().resolve(&u, &provider);
            }
        }
    }
}

#[test]
fn adversarial_shapes_never_panic() {
    let provider = MemoryProvider::new();
    let cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"\n".to_vec(),
        b"LCL:\n".to_vec(),
        b"LCL:\n    VERSION: \"0.1.0\"\n".to_vec(),
        // A REF to itself.
        b"LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: a.b\n    NAME: \"x\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n    ID: d.one\n    TYPE: INTEGER\n    VALUE: REF(d.one)\n".to_vec(),
        // Deeply nested groups around a reference. This crate's expression
        // walk is iterative, so its own cost is depth-independent; the depth
        // here stays inside what `lcl_parser::Parser::parse` tolerates on a
        // libtest thread stack, because building the syntax tree is recursive
        // and overflows well before this stage is reached. That bound belongs
        // to M2 and is reported separately, not worked around here.
        format!(
            "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: a.b\n    NAME: \"x\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\nDATA:\n    ID: d.one\n    TYPE: INTEGER\n    VALUE: {}REF(d.one){}\n",
            "(".repeat(64),
            ")".repeat(64)
        )
        .into_bytes(),
        // Every byte value in one line.
        (0u8..=255).collect(),
    ];
    for (i, bytes) in cases.into_iter().enumerate() {
        let u = SourceUnit::new(SourceId::new(format!("case{i}.lcl")), bytes);
        let _ = resolver().resolve(&u, &provider);
    }
}

#[test]
fn a_self_importing_document_terminates() {
    // The provider maps the root's own key, so the import closes a cycle on the
    // first hop rather than recurring.
    let source = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: a.b\n    NAME: \"x\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\nIMPORT:\n    ID: import.self\n    SOURCE: PATH(\"root.lcl\")\n    NAMESPACE: me\n    VERSION: \"1.0.0\"\n";
    let mut provider = MemoryProvider::new();
    provider.insert("root.lcl", source.as_bytes());
    let resolved = resolver()
        .resolve(&unit("root.lcl", source), &provider)
        .expect("resolves");
    assert_eq!(
        resolved
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect::<Vec<_>>(),
        ["error.import.cycle"]
    );
}
