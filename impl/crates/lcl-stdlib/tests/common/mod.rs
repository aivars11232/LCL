//! Shared test helpers. Every test runs the real pipeline against the approved
//! package: bytes in, execution out, with no hand-built syntax trees.

#![allow(dead_code)]

use lcl_capabilities::ProfileCatalog;
use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts, Execution, MockHost, Runtime};
use lcl_semantics::{
    Contracts as PreflightContracts, Invocation, Outcome, Planned, Preflight, Value,
};
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

pub fn contracts() -> &'static Contracts {
    static CONTRACTS: OnceLock<Contracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| Contracts::load(spec()).expect("runtime contracts load"))
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

pub fn catalog() -> ProfileCatalog {
    ProfileCatalog::load(spec()).expect("the profile catalog loads")
}

/// The standard library, assembled from the approved package.
pub fn stdlib() -> Stdlib {
    Stdlib::load(spec()).expect("the standard library assembles")
}

/// One document carried through every stage below the runtime.
pub struct Fixture {
    pub source: SourceId,
    pub resolved: Resolved,
    pub checked: Checked,
    pub planned: Planned,
}

/// Carry one document through lexing, parsing, resolution, checking and
/// preflight, asserting each earlier stage succeeded.
pub fn fixture(source: &str) -> Fixture {
    let id = SourceId::new("root.lcl");
    let unit = SourceUnit::new(id.clone(), source.as_bytes());
    let resolved = Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed");
    assert!(
        resolved.primary().is_none(),
        "resolution must succeed: {:?}",
        resolved.primary().map(|d| d.id.to_string())
    );
    let checked = Checker::new(static_contracts())
        .check(&resolved)
        .expect("resolution succeeded");
    assert!(
        checked.primary().is_none(),
        "static checking must succeed: {:?}",
        checked.primary().map(|d| d.id.to_string())
    );
    let planned = Preflight::new(preflight_contracts())
        .plan(&checked, &resolved, &Invocation::new())
        .expect("static checking succeeded");
    assert_eq!(
        planned.outcome(),
        Outcome::Planned,
        "preflight must accept the document: {:?}",
        planned.primary().map(|d| d.id.to_string())
    );
    Fixture {
        source: id,
        resolved,
        checked,
        planned,
    }
}

/// Execute one document through the standard library and a mock host.
pub fn run(source: &str) -> Execution {
    run_with(source, stdlib(), MockHost::new())
}

/// Execute one document through a supplied surface and host.
pub fn run_with(source: &str, mut stdlib: Stdlib, mut host: MockHost) -> Execution {
    let fixture = fixture(source);
    Runtime::new(contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned")
}

/// The result record of one ACTION declaration, by qualified id.
pub fn result_of<'a>(execution: &'a Execution, declaration: &str) -> &'a lcl_runtime::ResultRecord {
    execution
        .invocations()
        .iter()
        .find(|r| r.declaration.as_deref() == Some(declaration))
        .unwrap_or_else(|| panic!("{declaration} was invoked"))
        .result
        .as_ref()
        .unwrap_or_else(|| panic!("{declaration} produced a result"))
}

/// One schema-local field of one ACTION's result.
pub fn field<'a>(execution: &'a Execution, declaration: &str, name: &str) -> &'a Value {
    result_of(execution, declaration)
        .fields
        .get(name)
        .unwrap_or_else(|| {
            panic!(
                "{declaration} result has no field {name}; it has {:?}",
                result_of(execution, declaration)
                    .fields
                    .keys()
                    .collect::<Vec<_>>()
            )
        })
}

/// The registered execution errors one ACTION's result carries.
pub fn errors_of(execution: &Execution, declaration: &str) -> Vec<String> {
    result_of(execution, declaration).execution_errors.clone()
}

/// A `kind.task` document: free declarations, then one ACTION per entry.
///
/// Each action is the indented body of an `ACTION:` block, beginning with its
/// `ID:` line. The builder supplies the GOAL, SUCCESS, TASK and EXECUTE
/// scaffolding every runnable document needs.
pub fn task(declarations: &str, actions: &[&str]) -> String {
    let mut out = String::from(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.stdlib\n    \
         NAME: \"Standard library fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n",
    );
    out.push_str(declarations);
    let mut ids = Vec::new();
    for action in actions {
        out.push_str("\nACTION:\n");
        for line in action.trim_end().lines() {
            out.push_str("    ");
            out.push_str(line.trim_end());
            out.push('\n');
            if let Some(id) = line.trim().strip_prefix("ID: ") {
                ids.push(id.trim().to_string());
            }
        }
    }
    out.push_str("\nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n");
    out.push_str("\nSUCCESS:\n    ID: success.subject\n    ALL: [TRUE]\n");
    out.push_str("\nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n");
    // A repeated field is `error.field.duplicate`; several actions are one
    // field holding a list, exactly as canonical example 12 writes it.
    match ids.as_slice() {
        [] => {}
        [only] => out.push_str(&format!("    ACTION: REF({only})\n")),
        many => {
            let refs: Vec<String> = many.iter().map(|id| format!("REF({id})")).collect();
            out.push_str(&format!("    ACTION: [{}]\n", refs.join(", ")));
        }
    }
    out.push_str("    SUCCESS: REF(success.subject)\n");
    out.push_str("\nEXECUTE:\n    REFERENCE: REF(task.subject)\n");
    out
}

/// One `DATA` declaration.
pub fn data(id: &str, ty: &str, value: &str) -> String {
    format!("\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n")
}

/// A provider holding every canonical `VALID` example under its own file name.
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

/// Carry one canonical example through every stage, by file name.
pub fn example_fixture(name: &str) -> Fixture {
    let source = std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the canonical example is readable");
    let id = SourceId::new(name);
    let unit = SourceUnit::new(id.clone(), source.as_bytes());
    let resolved = Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &canonical_example_provider())
        .expect("lexing and parsing succeed");
    let checked = Checker::new(static_contracts())
        .check(&resolved)
        .expect("resolution succeeded");
    let planned = Preflight::new(preflight_contracts())
        .plan(&checked, &resolved, &Invocation::new())
        .expect("static checking succeeded");
    Fixture {
        source: id,
        resolved,
        checked,
        planned,
    }
}

/// A compact, comparable summary of one execution.
///
/// Statuses, registered errors and bound outputs — everything an observer of
/// the engine can see — with nothing derived from timing or address.
pub fn summary(execution: &Execution) -> Vec<String> {
    let mut lines: Vec<String> = execution
        .invocations()
        .iter()
        .map(|record| {
            let result = record
                .result
                .as_ref()
                .map(|r| {
                    format!(
                        "{}|{:?}|{:?}|{}",
                        r.status,
                        r.failure_phase,
                        r.effect_state,
                        r.execution_errors.join(",")
                    )
                })
                .unwrap_or_else(|| "-".to_string());
            format!(
                "{}|{}|{}|{}",
                record.block,
                record.declaration.as_deref().unwrap_or("-"),
                record.status(),
                result
            )
        })
        .collect();
    for diagnostic in execution.diagnostics() {
        lines.push(format!("diagnostic {}", diagnostic.id));
    }
    lines
}
