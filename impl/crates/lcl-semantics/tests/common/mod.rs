//! Shared test helpers. Every test checks against the approved package only.

#![allow(dead_code)]

use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_lexer::{Lexed, Lexer, Lexicon};
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
use lcl_semantics::{Contracts, Invocation, Planned, Preflight};
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

pub fn static_contracts() -> &'static StaticContracts {
    static CONTRACTS: OnceLock<StaticContracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        StaticContracts::load(spec()).expect("static contracts load from the approved package")
    })
}

pub fn contracts() -> &'static Contracts {
    static CONTRACTS: OnceLock<Contracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        Contracts::load(spec()).expect("preflight contracts load from the approved package")
    })
}

pub fn resolver() -> Resolver<'static> {
    Resolver::new(rules(), grammar(), lexicon())
}

pub fn checker() -> Checker<'static> {
    Checker::new(static_contracts())
}

pub fn preflight() -> Preflight<'static> {
    Preflight::new(contracts())
}

pub fn unit(id: &str, source: &str) -> SourceUnit {
    SourceUnit::new(SourceId::new(id), source.as_bytes())
}

pub fn lex(source: &str) -> Lexed {
    Lexer::new(lexicon()).lex_str(source)
}

/// Resolve one standalone document with an empty provider.
pub fn resolve(source: &str) -> Resolved {
    resolve_with(source, MemoryProvider::new())
}

/// Resolve one document against an explicit provider.
pub fn resolve_with(source: &str, provider: MemoryProvider) -> Resolved {
    resolver()
        .resolve(&unit("root.lcl", source), &provider)
        .expect("earlier stages pass")
}

/// A provider holding every canonical `VALID` example under its own file name.
///
/// The importing example names its library by exact path, so the provider must
/// be told about it — which is the source-provider contract working as
/// specified: "receives explicit identifiers/paths derived from source/project
/// configuration" and "never injects unreferenced chat/history/ambient files".
pub fn canonical_example_provider() -> MemoryProvider {
    let mut provider = MemoryProvider::new();
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .expect("the canonical examples are readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lcl"))
        .collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).expect("readable");
        provider.insert(name, bytes);
    }
    provider
}

/// Resolve, check and plan one canonical example by file name.
pub fn plan_example(name: &str) -> Planned {
    let source = std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the canonical example is readable");
    let resolved = resolver()
        .resolve(&unit(name, &source), &canonical_example_provider())
        .expect("earlier stages pass");
    assert!(
        resolved.diagnostics().is_empty(),
        "{name} must resolve cleanly: {:?}",
        resolved
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let checked = checker().check(&resolved).expect("resolution succeeded");
    assert!(
        checked.diagnostics().is_empty() && checked.earlier_stage_defects().is_empty(),
        "{name} must statically check cleanly: {:?}",
        checked
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    preflight()
        .plan(&checked, &resolved, &Invocation::new())
        .expect("the static stage succeeded")
}

/// Resolve and statically check one standalone document, asserting the test's
/// own source is clean through the static stage, so a failure is a defect in
/// the preflight layer rather than in the test fixture.
pub fn check(source: &str) -> (Resolved, Checked) {
    let resolved = resolve(source);
    assert!(
        resolved.diagnostics().is_empty(),
        "test source must resolve cleanly: {:?}",
        resolved
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    let checked = checker().check(&resolved).expect("resolution succeeded");
    assert!(
        checked.diagnostics().is_empty() && checked.earlier_stage_defects().is_empty(),
        "test source must statically check cleanly: {:?}",
        checked
            .diagnostics()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    (resolved, checked)
}

/// Statically check then preflight one standalone document with no supplied
/// invocation data.
pub fn plan(source: &str) -> Planned {
    plan_with(source, &Invocation::new())
}

/// Statically check then preflight one standalone document with explicit
/// invocation data.
pub fn plan_with(source: &str, invocation: &Invocation) -> Planned {
    let (resolved, checked) = check(source);
    preflight()
        .plan(&checked, &resolved, invocation)
        .expect("the static stage succeeded")
}

/// Every emitted preflight diagnostic identifier, in stable order.
pub fn ids(planned: &Planned) -> Vec<String> {
    planned
        .diagnostics()
        .iter()
        .map(|d| d.id.to_string())
        .collect()
}

/// The primary identifier and its byte offset, if any.
pub fn primary(planned: &Planned) -> Option<(String, usize)> {
    planned.primary().map(|d| (d.id.to_string(), d.span.start))
}

/// A `kind.task` document header.
pub const HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"Test\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n";

/// A `kind.library` document header, for rule-only documents that declare no
/// `EXECUTE` root.
pub const LIBRARY: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"Test\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n";

/// A `kind.data` document header, for fixtures that need no execution root.
pub const DATA_HEADER: &str = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: test.doc\n    NAME: \"Test\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n";

/// The smallest `kind.task` document that resolves, checks and plans: one
/// `ACTION` under one `TASK` under `EXECUTE`.
///
/// `body` is inserted before the execution blocks, so a test can add the
/// declarations it is about without restating the scaffolding.
pub fn task_document(body: &str) -> String {
    format!(
        "{HEADER}{body}\nGOAL:\n    ID: goal.one\n    ASSERT: TRUE\n\nACTION:\n    ID: action.one\n    OPERATION: core.inspect\n    TARGET: REF(data.subject)\n\nSUCCESS:\n    ID: success.one\n    ALL: [REF(goal.one)]\n\nTASK:\n    ID: task.one\n    GOAL: REF(goal.one)\n    ACTION: REF(action.one)\n    SUCCESS: REF(success.one)\n\nEXECUTE:\n    REFERENCE: REF(task.one)\n"
    )
}

/// One `DATA` declaration, for use as `task_document`'s body.
pub fn data_block(id: &str, ty: &str, value: &str) -> String {
    format!("\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n")
}
