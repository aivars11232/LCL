//! Shared test helpers. Every test checks against the approved package only.

#![allow(dead_code)]

use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolved, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{
    Bindings, Contracts, Demand, Evaluator, Execution, IterationPath, MockHost, NotPlanned, Runtime,
};
use lcl_semantics::{
    Contracts as PreflightContracts, Invocation, Outcome, Planned, Preflight, Value,
};
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

pub fn contracts() -> &'static Contracts {
    static CONTRACTS: OnceLock<Contracts> = OnceLock::new();
    CONTRACTS.get_or_init(|| {
        Contracts::load(spec()).expect("runtime contracts load from the approved package")
    })
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

/// One document carried through every stage below the runtime.
pub struct Fixture {
    pub source: SourceId,
    pub resolved: Resolved,
    pub checked: Checked,
    pub planned: Planned,
    pub bindings: Bindings,
}

impl Fixture {
    /// An evaluator over this fixture at the root iteration context.
    pub fn evaluator(&self) -> Evaluator<'_> {
        Evaluator {
            contracts: contracts(),
            resolved: &self.resolved,
            checked: &self.checked,
            plan: self.planned.partial_plan(),
            bindings: &self.bindings,
            source: self.source.clone(),
            iteration: IterationPath::root(),
        }
    }

    /// Demand the inline `VALUE` expression of one declaration.
    pub fn demand(&self, declaration_id: &str) -> Demand {
        let index = self
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == declaration_id)
            .unwrap_or_else(|| panic!("{declaration_id} is declared"));
        let block = lcl_runtime::syntax::declaration_block(&self.resolved, index)
            .expect("the declaration has a block");
        let expr = lcl_runtime::syntax::field_expr(&block, "VALUE")
            .or_else(|| lcl_runtime::syntax::field_expr(&block, "ASSERT"))
            .unwrap_or_else(|| panic!("{declaration_id} has an inline VALUE or ASSERT"));
        self.evaluator().demand(expr)
    }

    pub fn bind_local(&mut self, name: &str, value: Value) {
        self.bindings
            .bind_local(name, &IterationPath::root(), value);
    }
}

/// Carry one document through lexing, parsing, resolution, checking and
/// preflight, asserting each earlier stage succeeded.
pub fn fixture(source: &str) -> Fixture {
    fixture_with(source, Invocation::new())
}

pub fn fixture_with(source: &str, invocation: Invocation) -> Fixture {
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
    Fixture {
        source: id,
        resolved,
        checked,
        planned,
        bindings: Bindings::new(),
    }
}

/// The same, but permitting a rejected preflight, for cases whose subject is a
/// runtime demand over a document preflight declines to plan.
pub fn fixture_allowing_rejection(source: &str, invocation: Invocation) -> Fixture {
    let id = SourceId::new("root.lcl");
    let unit = SourceUnit::new(id.clone(), source.as_bytes());
    let resolved = Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed");
    let checked = Checker::new(static_contracts())
        .check(&resolved)
        .expect("resolution succeeded");
    let planned = Preflight::new(preflight_contracts())
        .plan(&checked, &resolved, &invocation)
        .expect("static checking succeeded");
    Fixture {
        source: id,
        resolved,
        checked,
        planned,
        bindings: Bindings::new(),
    }
}

/// Resolve one document, without asserting the outcome.
pub fn resolve(source: &str) -> Resolved {
    let unit = SourceUnit::new(SourceId::new("root.lcl"), source.as_bytes());
    Resolver::new(rules(), grammar(), lexicon())
        .resolve(&unit, &MemoryProvider::new())
        .expect("lexing and parsing succeed")
}

/// Statically check one resolved document, without asserting the outcome.
pub fn check(resolved: &Resolved) -> Checked {
    Checker::new(static_contracts())
        .check(resolved)
        .expect("resolution succeeded")
}

pub fn is_planned(fixture: &Fixture) -> bool {
    fixture.planned.outcome() == Outcome::Planned
}

/// A `kind.data` document holding one `DATA` declaration per entry.
///
/// `(id, TYPE, VALUE)`. This is the smallest document shape that carries an
/// arbitrary expression through every earlier stage, so an evaluator test
/// exercises the real pipeline rather than a hand-built syntax tree.
pub fn data_document(entries: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.fixture\n    \
         NAME: \"Evaluator fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.data\n",
    );
    for (id, ty, value) in entries {
        out.push_str(&format!(
            "\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n"
        ));
    }
    out
}

/// Evaluate one expression of the given declared type, in a fresh document.
pub fn eval(ty: &str, expression: &str) -> Demand {
    let source = data_document(&[("data.subject", ty, expression)]);
    fixture(&source).demand("data.subject")
}

/// The value of one expression, asserting it produced no fault.
pub fn value(ty: &str, expression: &str) -> Value {
    match eval(ty, expression) {
        Ok(value) => value,
        Err(fault) => panic!("{expression} faulted with {}: {}", fault.id, fault.detail),
    }
}

pub fn boolean(expression: &str) -> Value {
    value("BOOLEAN", expression)
}

pub fn integer(n: i64) -> Value {
    let magnitude = lcl_checker::numeric::Decimal::from_integer(
        lcl_checker::numeric::Integer::from_u64(n.unsigned_abs()),
    );
    Value::Integer(if n < 0 {
        magnitude.negated()
    } else {
        magnitude
    })
}

pub fn decimal(text: &str) -> Value {
    let exact = lcl_checker::numeric::Decimal::parse_decimal(text)
        .or_else(|| lcl_checker::numeric::Decimal::parse_integer(text))
        .expect("an exact decimal");
    Value::Decimal(exact)
}

/// A `kind.task` document whose `INPUT`s are optional with a `DEFAULT`, so an
/// `Invocation` may supply their values at runtime.
///
/// `field_signatures#/blocks/INPUT`: "Exactly one VALUE or SOURCE unless
/// optional with DEFAULT." An optional `INPUT` with a `DEFAULT` and no `VALUE`
/// is therefore the canonical way to declare a datum whose value arrives at
/// invocation — which is what makes a demand *dynamic* and reaches this
/// milestone rather than being folded by M4.
pub fn dynamic_document(inputs: &[(&str, &str, &str)], data: &[(&str, &str, &str)]) -> String {
    let mut out = String::from(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.dynamic\n    \
         NAME: \"Dynamic fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n",
    );
    for (id, ty, default) in inputs {
        out.push_str(&format!(
            "\nINPUT:\n    ID: {id}\n    TYPE: {ty}\n    REQUIRED: FALSE\n    DEFAULT: {default}\n"
        ));
    }
    for (id, ty, value) in data {
        out.push_str(&format!(
            "\nDATA:\n    ID: {id}\n    TYPE: {ty}\n    VALUE: {value}\n"
        ));
    }
    out.push_str(
        "\nGOAL:\n    ID: goal.dynamic\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.dynamic\n    OPERATION: core.inspect\n    \
         TARGET: REF(goal.dynamic)\n\n\
         SUCCESS:\n    ID: success.dynamic\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.dynamic\n    GOAL: REF(goal.dynamic)\n    \
         ACTION: REF(action.dynamic)\n    SUCCESS: REF(success.dynamic)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.dynamic)\n",
    );
    out
}

/// A `kind.task` document with an `OUTPUT` no producer binds.
///
/// `05_SEMANTICS/05`: "Within a valid instance an output not yet bound yields
/// MISSING", and reading it "does not execute its producer". That is the
/// canonical way a demand meets MISSING at runtime — an optional `INPUT` with a
/// `DEFAULT` cannot express it, because "DEFAULT only for MISSING" means the
/// default is exactly what a supplied MISSING resolves to.
pub fn unbound_output_document(subject_type: &str, subject: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.unbound\n    \
         NAME: \"Unbound output fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
         OUTPUT:\n    ID: output.unbound\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n\
         OUTPUT:\n    ID: output.flag\n    TYPE: BOOLEAN\n    FORMAT: format.plain_text\n\n\
         DATA:\n    ID: data.subject\n    TYPE: {subject_type}\n    VALUE: {subject}\n\n\
         GOAL:\n    ID: goal.unbound\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.unbound\n    OPERATION: core.inspect\n    \
         TARGET: REF(goal.unbound)\n\n\
         SUCCESS:\n    ID: success.unbound\n    ALL: [TRUE]\n\n\
         TASK:\n    ID: task.unbound\n    GOAL: REF(goal.unbound)\n    \
         ACTION: REF(action.unbound)\n    SUCCESS: REF(success.unbound)\n\n\
         EXECUTE:\n    REFERENCE: REF(task.unbound)\n"
    )
}

/// Demand one expression that reads an unbound `OUTPUT`, which is MISSING.
pub fn eval_unbound(subject_type: &str, subject: &str) -> Demand {
    let source = unbound_output_document(subject_type, subject);
    fixture(&source).demand("data.subject")
}

/// Demand one expression over dynamically supplied inputs.
///
/// `inputs` are `(id, TYPE, DEFAULT)`; `supplied` are the values the invocation
/// provides, which outrank the declared `DEFAULT` under the canonical
/// resolution order.
pub fn eval_dynamic(
    inputs: &[(&str, &str, &str)],
    subject_type: &str,
    subject: &str,
    supplied: &[(&str, Value)],
) -> Demand {
    let source = dynamic_document(inputs, &[("data.subject", subject_type, subject)]);
    let mut invocation = Invocation::new();
    for (id, value) in supplied {
        invocation = invocation.with(*id, value.clone());
    }
    fixture_with(&source, invocation).demand("data.subject")
}

/// Execute one fixture against a host.
pub fn run(fixture: &Fixture, host: &mut MockHost) -> Result<Execution, NotPlanned> {
    Runtime::new(contracts()).execute(&fixture.planned, &fixture.checked, &fixture.resolved, host)
}

/// Execute one document against a permissive deterministic host.
pub fn execute(source: &str) -> (Execution, MockHost) {
    let fixture = fixture(source);
    let mut host = MockHost::new();
    let execution = run(&fixture, &mut host).expect("preflight planned it");
    (execution, host)
}

/// Execute one document against a supplied host.
pub fn execute_with(source: &str, mut host: MockHost) -> (Execution, MockHost) {
    let fixture = fixture(source);
    let execution = run(&fixture, &mut host).expect("preflight planned it");
    (execution, host)
}

/// Read one canonical valid example by file name.
pub fn canonical_example(name: &str) -> String {
    std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID").join(name))
        .expect("the canonical example is readable")
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

/// Carry one canonical example through every stage, by file name.
pub fn example_fixture(name: &str) -> Fixture {
    let source = canonical_example(name);
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
        bindings: Bindings::new(),
    }
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
