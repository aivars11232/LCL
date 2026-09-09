//! Shared test helpers. Every test checks against the approved package only.
//!
//! Unlike the layers below, a completion fixture carries a document all the way
//! through *execution*: steps 11 to 13 have nothing to observe otherwise. So
//! `execute` here is the real pipeline — lexer, parser, resolver, checker,
//! preflight, runtime, mock host — and never a hand-built `Execution`.

#![allow(dead_code)]

use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_completion::Contracts as CompletionContracts;
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts as RuntimeContracts, Execution, MockHost, Runtime};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Outcome, Planned, Preflight};
use lcl_spec::SpecPackage;
use lcl_stdlib::Stdlib;
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

pub fn completion_contracts() -> &'static CompletionContracts {
    static CONTRACTS: OnceLock<CompletionContracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        CompletionContracts::load(spec()).expect("completion contracts load from the package")
    })
}

pub fn runtime_contracts() -> &'static RuntimeContracts {
    static CONTRACTS: OnceLock<RuntimeContracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| RuntimeContracts::load(spec()).expect("runtime contracts load"))
}

pub fn lexicon() -> &'static Lexicon {
    static LEXICON: OnceLock<Lexicon> = OnceLock::new();
    LEXICON.get_or_init(|| Lexicon::load(spec()).expect("lexicon loads"))
}

pub fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| Grammar::load(spec()).expect("grammar loads"))
}

pub fn rules() -> &'static Rules {
    static RULES: OnceLock<Rules> = OnceLock::new();
    RULES.get_or_init(|| Rules::load(spec(), grammar()).expect("rules load"))
}

pub fn static_contracts() -> &'static StaticContracts {
    static CONTRACTS: OnceLock<StaticContracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| StaticContracts::load(spec()).expect("static contracts load"))
}

pub fn preflight_contracts() -> &'static PreflightContracts {
    static CONTRACTS: OnceLock<PreflightContracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| PreflightContracts::load(spec()).expect("preflight contracts load"))
}

/// One document carried through every stage, including execution.
pub struct Fixture {
    pub source: SourceId,
    pub resolved: Resolved,
    pub checked: Checked,
    pub planned: Planned,
    pub execution: Execution,
}

/// Carry one document through every stage and execute it against a mock host.
///
/// Asserts each earlier stage succeeded, because a completion test whose
/// document failed at resolution would be testing the wrong thing.
pub fn execute(source: &str) -> Fixture {
    execute_with(source, Invocation::new())
}

pub fn execute_with(source: &str, invocation: Invocation) -> Fixture {
    let id = SourceId::new("root.lcl");
    let unit = SourceUnit::new(id.clone(), source.as_bytes());
    let resolved = Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed");
    assert!(
        resolved.primary().is_none(),
        "resolution must succeed: {:?}",
        resolved.primary()
    );
    let checked = Checker::new(static_contracts())
        .check(&resolved)
        .expect("resolution succeeded");
    assert!(
        checked.primary().is_none(),
        "static checking must succeed: {:?}",
        checked.primary()
    );
    let planned = Preflight::new(preflight_contracts())
        .plan(&checked, &resolved, &invocation)
        .expect("static checking succeeded");
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "preflight must accept: {:?}",
        planned.primary()
    );
    // The real Core operation surface, not a bare deferring runtime. A
    // completion test whose ACTION resolved to a host placeholder would be
    // asserting over a value the language never produced.
    let mut stdlib = Stdlib::load(spec()).expect("the standard library assembles");
    let mut host = MockHost::new();
    let execution = Runtime::new(runtime_contracts())
        .execute_with(&planned, &checked, &resolved, &mut stdlib, &mut host)
        .expect("preflight accepted, so a plan exists");
    Fixture {
        source: id,
        resolved,
        checked,
        planned,
        execution,
    }
}

/// A minimal `kind.task` document with the given top-level blocks appended.
///
/// The header is the smallest document that reaches execution at all, so a
/// completion test states only the blocks it is actually about.
pub fn task_document(blocks: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.completion\n    \
         NAME: \"Completion fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n{blocks}"
    )
}

/// A provider holding every canonical valid example, so an importing example
/// resolves its imports from the package rather than from anything ambient.
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

/// Every canonical valid example file name, in ascending order.
pub fn canonical_example_names() -> Vec<String> {
    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".lcl"))
        .collect();
    names.sort();
    names
}

/// Every canonical invalid example file name, in ascending order.
pub fn canonical_invalid_names() -> Vec<String> {
    let dir = canonical_root().join("08_EXAMPLES/INVALID");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".invalid.lcl"))
        .collect();
    names.sort();
    names
}

/// One canonical example, carried through every stage including execution.
///
/// Unlike [`execute`], this asserts nothing about the earlier stages: the
/// caller decides what the example is expected to do.
pub struct ExampleRun {
    pub source: SourceId,
    pub resolved: Resolved,
    pub checked: Checked,
    pub planned: Planned,
    pub execution: Option<Execution>,
}

/// Run one canonical example, or report the stage that refused it outright.
///
/// A document that fails lexing or parsing has no resolved model at all: the
/// resolver returns `StageSkipped` rather than an empty `Resolved`. That is
/// itself proof the document failed before this layer, so it is reported as
/// `Err(stage)` rather than treated as a completion result.
pub fn run_example(dir: &str, name: &str) -> Result<ExampleRun, String> {
    let bytes = std::fs::read(canonical_root().join(dir).join(name))
        .unwrap_or_else(|e| panic!("{name} is readable: {e}"));
    let id = SourceId::new(name);
    let unit = SourceUnit::new(id.clone(), bytes);
    let resolved = match Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &canonical_example_provider())
    {
        Ok(resolved) => resolved,
        Err(skipped) => return Err(format!("{skipped:?}")),
    };
    let checked = match Checker::new(static_contracts()).check(&resolved) {
        Ok(checked) => checked,
        Err(skipped) => return Err(format!("{skipped:?}")),
    };
    let planned =
        match Preflight::new(preflight_contracts()).plan(&checked, &resolved, &Invocation::new()) {
            Ok(planned) => planned,
            Err(skipped) => return Err(format!("{skipped:?}")),
        };
    let execution = if planned.outcome() == Outcome::Planned {
        let mut stdlib = Stdlib::load(spec()).expect("the standard library assembles");
        let mut host = MockHost::new();
        Runtime::new(runtime_contracts())
            .execute_with(&planned, &checked, &resolved, &mut stdlib, &mut host)
            .ok()
    } else {
        None
    };
    Ok(ExampleRun {
        source: id,
        resolved,
        checked,
        planned,
        execution,
    })
}
