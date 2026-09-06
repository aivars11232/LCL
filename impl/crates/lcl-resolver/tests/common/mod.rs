//! Shared test helpers. Every test resolves against the approved package only.

#![allow(dead_code)]

use lcl_lexer::{Lexed, Lexer, Lexicon};
use lcl_parser::{Grammar, Parser};
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
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

pub fn lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| Lexicon::load(spec()).expect("lexicon loads from the approved package"))
}

pub fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| Grammar::load(spec()).expect("grammar loads from the approved package"))
}

pub fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| {
        Rules::load(spec(), grammar()).expect("rules load from the approved package")
    })
}

pub fn resolver() -> Resolver<'static> {
    Resolver::new(rules(), grammar(), lexicon())
}

pub fn lex(source: &str) -> Lexed {
    Lexer::new(lexicon()).lex_str(source)
}

/// Assert a test's own source is clean through the grammar stage, so a failure
/// here is a defect in the test rather than a resolver result.
pub fn assert_stages_clean(source: &str) {
    let lexed = lex(source);
    assert_eq!(
        lexed.primary().map(|d| d.id.to_string()),
        None,
        "test source must be lexically clean"
    );
    let parsed = Parser::new(grammar())
        .parse(&lexed)
        .expect("a clean lexical stage permits parsing");
    assert_eq!(
        parsed.primary().map(|d| d.id.to_string()),
        None,
        "test source must be grammatically clean: {:?}",
        parsed.primary().map(ToString::to_string)
    );
}

pub fn unit(id: &str, source: &str) -> SourceUnit {
    SourceUnit::new(SourceId::new(id), source.as_bytes())
}

/// Resolve one standalone document with an empty provider.
pub fn resolve(source: &str) -> Resolved {
    let provider = MemoryProvider::new();
    resolver()
        .resolve(&unit("root.lcl", source), &provider)
        .expect("earlier stages pass")
}

/// Resolve a root against a provider holding the given units.
pub fn resolve_with(root: &str, units: &[(&str, &str)]) -> Resolved {
    let mut provider = MemoryProvider::new();
    for (key, body) in units {
        provider.insert(*key, body.as_bytes());
    }
    resolver()
        .resolve(&unit("root.lcl", root), &provider)
        .expect("earlier stages pass")
}

/// Every emitted diagnostic identifier, in stable order.
pub fn ids(resolved: &Resolved) -> Vec<String> {
    resolved
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect()
}

/// A minimal valid task document header.
pub const HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"Test\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n";
