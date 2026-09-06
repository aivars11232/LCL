//! Source identity, the provider contract, and stage monotonicity.

mod common;

use common::{resolver, unit, HEADER};
use lcl_lexer::Span;
use lcl_resolver::{
    LoadError, MemoryProvider, SourceId, SourceProvider, SourceRef, SourceRequest, SourceUnit,
};

fn request(origin: &str, reference: SourceRef) -> SourceRequest {
    SourceRequest {
        origin: SourceId::new(origin),
        reference,
        span: Span::new(0, 0),
    }
}

#[test]
fn a_unit_digest_is_the_sha256_of_its_exact_bytes() {
    let u = SourceUnit::new(SourceId::new("a.lcl"), b"abc".to_vec());
    assert_eq!(u.digest(), lcl_spec::sha256::hex_digest(b"abc"));
    assert_eq!(u.digest().len(), 64);
    assert!(u.digest().chars().all(|c| c.is_ascii_hexdigit()));
    // Distinct bytes, distinct digest: a checksum comparison is meaningful.
    let other = SourceUnit::new(SourceId::new("a.lcl"), b"abd".to_vec());
    assert_ne!(u.digest(), other.digest());
}

#[test]
fn source_ids_order_deterministically() {
    let mut ids = [
        SourceId::new("b.lcl"),
        SourceId::new("a.lcl"),
        SourceId::new("a/b.lcl"),
    ];
    ids.sort();
    assert_eq!(
        ids.iter().map(SourceId::as_str).collect::<Vec<_>>(),
        ["a.lcl", "a/b.lcl", "b.lcl"]
    );
}

#[test]
fn a_path_resolves_relative_to_the_importing_unit() {
    // 07_VERSIONING_AND_EXTENSIONS/02: "SOURCE PATH resolves relative only to
    // importing file or explicit WORKSPACE."
    let provider = MemoryProvider::new().with("lib/rules.lcl", b"x".to_vec());
    let loaded = provider
        .load(&request(
            "lib/task.lcl",
            SourceRef::Path("rules.lcl".into()),
        ))
        .expect("sibling resolves");
    assert_eq!(loaded.id().as_str(), "lib/rules.lcl");
    assert_eq!(loaded.bytes(), b"x");
}

#[test]
fn dot_segments_fold_and_cannot_escape_the_root() {
    let provider = MemoryProvider::new().with("rules.lcl", b"x".to_vec());
    let up = provider.load(&request(
        "lib/task.lcl",
        SourceRef::Path("../rules.lcl".into()),
    ));
    assert_eq!(up.expect("resolves").id().as_str(), "rules.lcl");

    // A `..` that would climb above the root resolves to nothing rather than
    // reaching outside the explicitly supplied set.
    let escape = provider.load(&request(
        "task.lcl",
        SourceRef::Path("../../secret.lcl".into()),
    ));
    assert!(escape.is_err(), "an escaping path must not resolve");
}

#[test]
fn an_unregistered_source_is_a_load_error_not_a_guess() {
    let provider = MemoryProvider::new().with("a.lcl", b"x".to_vec());
    let err = provider
        .load(&request("root.lcl", SourceRef::Path("b.lcl".into())))
        .expect_err("must not resolve");
    assert!(format!("{err}").contains("no source unit is registered"));
}

/// A provider that records every request it is asked for.
struct Recording {
    inner: MemoryProvider,
    seen: std::cell::RefCell<Vec<String>>,
}

impl SourceProvider for Recording {
    fn load(&self, request: &SourceRequest) -> Result<SourceUnit, LoadError> {
        self.seen
            .borrow_mut()
            .push(request.reference.text().to_string());
        self.inner.load(request)
    }
}

#[test]
fn a_document_that_imports_nothing_requests_nothing() {
    // The provider holds units the document never names. None may enter
    // resolution: 05_SEMANTICS/02, "implied nearby files do not exist".
    let provider = Recording {
        inner: MemoryProvider::new()
            .with("ambient.lcl", HEADER.as_bytes().to_vec())
            .with("root.lcl", b"decoy".to_vec()),
        seen: std::cell::RefCell::new(Vec::new()),
    };
    let resolved = resolver()
        .resolve(&unit("root.lcl", HEADER), &provider)
        .expect("earlier stages pass");
    assert!(
        provider.seen.borrow().is_empty(),
        "no source was named, so none may be requested"
    );
    assert_eq!(resolved.unit_count(), 1);
    assert_eq!(
        resolved.units().next().expect("root").id().as_str(),
        "root.lcl"
    );
    // The decoy under the root's own key was never consulted: the root's bytes
    // are the caller's, not the provider's.
    assert_eq!(resolved.units().next().expect("root").source(), HEADER);
}

#[test]
fn the_resolution_stage_is_skipped_after_a_lexical_failure() {
    // A TAB is error.source.tab. The resolution stage has no verdict at all.
    let source = "LCL:\n\tVERSION: \"0.1.0\"\n";
    let err = resolver()
        .resolve(&unit("root.lcl", source), &MemoryProvider::new())
        .expect_err("resolution must not be evaluated");
    assert_eq!(err.stage, lcl_diagnostics::Stage::Lexical);
    assert_eq!(err.primary, "error.source.tab");
    assert_eq!(err.source.as_str(), "root.lcl");
}

#[test]
fn the_resolution_stage_is_skipped_after_a_grammar_failure() {
    // A kind.data document may not carry EXECUTE: error.block.context.
    let source = concat!(
        "LCL:\n    VERSION: \"0.1.0\"\n\n",
        "SPECIFICATION:\n    ID: test.doc\n    NAME: \"T\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n\n",
        "EXECUTE:\n    REFERENCE: REF(task.t)\n",
    );
    let err = resolver()
        .resolve(&unit("root.lcl", source), &MemoryProvider::new())
        .expect_err("resolution must not be evaluated");
    assert_eq!(err.stage, lcl_diagnostics::Stage::GrammarOrSchema);
    assert_eq!(err.primary, "error.block.context");
}

#[test]
fn resolution_is_identical_across_repeated_runs() {
    let resolved_a = common::resolve(HEADER);
    let resolved_b = common::resolve(HEADER);
    assert_eq!(common::ids(&resolved_a), common::ids(&resolved_b));
    assert_eq!(resolved_a.outcome(), resolved_b.outcome());
    assert_eq!(resolved_a.unit_count(), resolved_b.unit_count());
}
