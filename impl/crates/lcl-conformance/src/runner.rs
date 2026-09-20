//! The executable case runner: concrete source in, observed engine result out.
//!
//! `09_CONFORMANCE/01_CONFORMANCE_REQUIREMENTS.txt` draws the line this module
//! exists to cross:
//!
//! > catalog entries without concrete input and an implementation result are
//! > not executed conformance cases
//!
//! An [`ExecutedCase`] therefore cannot be constructed without both. It holds
//! the exact source that was run and the [`Observed`] result the engine
//! produced, and its verdict is computed from comparing them — never asserted.
//! That is the whole reason a descriptive [`crate::Requirement`] and an
//! executed case are different types in this crate: no arithmetic can
//! accidentally add an indexed requirement to an executed one.
//!
//! ## What "the engine" means here
//!
//! Every implemented layer, in canonical order, with no stage skipped and no
//! stub substituted: lexer, parser, resolver, checker, semantic preflight,
//! runtime over the real Core operation surface, then completion. The host is
//! deterministic and in-memory, so a case's result depends on the source bytes
//! and nothing else.
//!
//! ## Why the runner never asserts
//!
//! [`Runner::run`] reports what happened. It has no notion of what should have
//! happened, and no way to signal failure. Judgement lives in [`Expectation`],
//! which is data supplied by a case. A runner that could decide a case had
//! passed would be a runner that could decide it in the absence of evidence.

use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_completion::{Completion, Contracts as CompletionContracts};
use lcl_lexer::{Lexer, Lexicon};
use lcl_parser::{Grammar, Parser};
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts as RuntimeContracts, Host, MockHost, Runtime};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Outcome, Preflight};
use lcl_spec::SpecPackage;
use lcl_stdlib::Stdlib;
use std::cell::RefCell;
use std::fmt;

/// The furthest canonical stage a source reached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reached {
    #[default]
    Lexical,
    Grammar,
    Resolution,
    StaticChecking,
    Preflight,
    Execution,
    Completion,
}

impl Reached {
    pub fn as_str(self) -> &'static str {
        match self {
            Reached::Lexical => "lexical",
            Reached::Grammar => "grammar",
            Reached::Resolution => "resolution",
            Reached::StaticChecking => "static_checking",
            Reached::Preflight => "preflight",
            Reached::Execution => "execution",
            Reached::Completion => "completion",
        }
    }
}

impl fmt::Display for Reached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the engine actually did with one concrete source.
///
/// Every field is an observation. Nothing here is a judgement.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    /// Ordered independent sub-runs, each retaining its own exact inputs.
    pub runs: Vec<Observed>,
    /// The label of each entry in `runs`, in the same order. A report checks a
    /// grouped probe's exact required sub-run membership against these labels.
    pub run_labels: Vec<String>,
    /// Exact return values of a named production component API invocation.
    pub component: Vec<(String, String)>,
    /// The furthest stage the source reached.
    pub reached: Reached,
    /// The registered identifier of the first unhandled diagnostic, if any.
    pub primary: Option<String>,
    /// That diagnostic's registered stage.
    pub primary_stage: Option<String>,
    /// Every registered identifier any stage produced, in stage order.
    pub diagnostics: Vec<String>,
    /// Exact source-stage byte loci, retained independently of diagnostic IDs.
    pub diagnostic_loci: Vec<(String, usize, usize)>,
    /// The one terminal status, when the invocation completed.
    pub terminal_status: Option<String>,
    /// Each post-execution check and the Boolean domain outcome it recorded.
    pub checks: Vec<(String, String)>,
    /// Each declared root output and its publication, rendered.
    pub outputs: Vec<(String, String)>,
    /// Actual entered invocations, including ordered retry attempt results.
    pub invocations: Vec<lcl_runtime::InvocationRecord>,
    /// Actual diagnostic-driven event selection and recovery records.
    pub events: Vec<lcl_runtime::EventRecord>,
    /// Exact accepted host retry evidence, separate from unchanged attempt history.
    pub retry_proofs: Vec<lcl_runtime::capability::RetryProof>,
    /// Exact input bytes and fixture data, with deterministic explicit encoding.
    pub input_evidence: Vec<String>,
}

impl Observed {
    /// A canonical, order-stable rendering, for evidence in a report.
    pub fn serialize(&self) -> String {
        let mut out = format!("reached={}", self.reached);
        for (index, run) in self.runs.iter().enumerate() {
            match self.run_labels.get(index) {
                Some(label) => {
                    out.push_str(&format!(" run[{index}:{label}]{{{}}}", run.serialize()))
                }
                None => out.push_str(&format!(" run[{index}]{{{}}}", run.serialize())),
            }
        }
        for (key, value) in &self.component {
            out.push_str(&format!(" component[{key}]={value:?}"));
        }
        if let Some(primary) = &self.primary {
            out.push_str(&format!(" primary={primary}"));
        }
        if let Some(status) = &self.terminal_status {
            out.push_str(&format!(" terminal={status}"));
        }
        for (id, start, end) in &self.diagnostic_loci {
            out.push_str(&format!(" diagnostic[{id}@{start}..{end}]"));
        }
        for (id, outcome) in &self.checks {
            out.push_str(&format!(" check[{id}]={outcome}"));
        }
        for (id, publication) in &self.outputs {
            out.push_str(&format!(" output[{id}]={publication}"));
        }
        for invocation in &self.invocations {
            out.push_str(&format!(
                " invocation[{}:{}]={}",
                invocation
                    .declaration
                    .as_deref()
                    .unwrap_or(&invocation.block),
                invocation.id,
                invocation.status()
            ));
            if let Some(initial) = &invocation.initial_output {
                out.push_str(&format!(" initial_output={initial}"));
            }
            if let Some(result) = &invocation.result {
                out.push_str(&format!(" {{{}}}", result.serialize()));
            }
        }
        for event in &self.events {
            out.push_str(&format!(" event[{event}]"));
        }
        for proof in &self.retry_proofs {
            out.push_str(&format!(" retry_proof[{proof:?}]"));
        }
        for input in &self.input_evidence {
            out.push_str(&format!(" input[{input}]"));
        }
        out
    }
}

/// What a case requires of the engine.
///
/// Data, not code: a case states its expectation and the runner compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expectation {
    /// Exact ordered population: every sub-run must meet its own expectation.
    Runs(Vec<Expectation>),
    /// Exact complete component return values, with concrete input evidence.
    Component(Vec<(String, String)>),
    /// Every independent assertion is required; none substitutes for another.
    All(Vec<Expectation>),
    /// The lexical or grammar stage accepts; this asserts no execution result.
    SourcePass(Reached),
    /// Exact ordered actual attempt statuses for one declaration.
    Attempts {
        declaration: String,
        statuses: Vec<String>,
    },
    /// A diagnostic must be absent from all retained evidence, not only primary.
    NoDiagnostic(String),
    /// Exact number of accepted, retained host retry proofs.
    RetryProofs(usize),
    /// An exact diagnostic must appear in the retained engine evidence.
    Diagnostic(String),
    DiagnosticAt {
        id: String,
        start: usize,
        end: usize,
    },
    /// An event from this producer must have a successful selected handler.
    Recovered(String),
    /// Exact selected root publication, including UNBOUND.
    Output { id: String, value: String },
    /// Exact field of a particular actual attempt. Common result axes use
    /// their registered names, schema-local fields use their value rendering.
    AttemptField {
        declaration: String,
        attempt: usize,
        field: String,
        value: String,
    },
    /// The source must be rejected with exactly this registered identifier.
    Rejects(String),
    /// The source must pass every implemented stage without a primary
    /// diagnostic and reach completion.
    Accepts,
    /// The source must complete and its named check must record this outcome.
    ///
    /// `TRUE`, `FALSE`, `MISSING` or `UNKNOWN`, as the value renders.
    Check { id: String, outcome: String },
    /// The source must complete with exactly this terminal status.
    Terminal(String),
    /// The source must be rejected, and no stage may accept it.
    ///
    /// Used where the canonical expectation names a rejection but the exact
    /// identifier is a choice among several registered ones that the witness
    /// does not pin.
    RejectsAtStage(String),
}

impl Expectation {
    /// Render the expectation for a report, in the catalog's own vocabulary.
    pub fn serialize(&self) -> String {
        match self {
            Expectation::Runs(runs) => format!(
                "all ordered runs {:?}",
                runs.iter().map(Self::serialize).collect::<Vec<_>>()
            ),
            Expectation::Component(values) => format!("exact component {values:?}"),
            Expectation::SourcePass(stage) => format!("passes source stage {stage}"),
            Expectation::All(assertions) => assertions
                .iter()
                .map(Self::serialize)
                .collect::<Vec<_>>()
                .join("; "),
            Expectation::Attempts {
                declaration,
                statuses,
            } => format!("{declaration} attempts {statuses:?}"),
            Expectation::RetryProofs(count) => format!("{count} accepted retry proofs"),
            Expectation::NoDiagnostic(id) => format!("no {id} in retained diagnostics"),
            Expectation::DiagnosticAt { id, start, end } => {
                format!("{id} at source bytes {start}..{end}")
            }
            Expectation::Diagnostic(id) => format!("{id} in retained diagnostics"),
            Expectation::Recovered(id) => format!("event from {id} recovered"),
            Expectation::Output { id, value } => format!("output {id} publishes {value}"),
            Expectation::AttemptField {
                declaration,
                attempt,
                field,
                value,
            } => format!("{declaration} attempt {attempt} {field}={value}"),
            Expectation::Rejects(id) => format!("rejects with {id}"),
            Expectation::Accepts => "accepts".to_string(),
            Expectation::Check { id, outcome } => format!("check {id} records {outcome}"),
            Expectation::Terminal(status) => format!("terminates {status}"),
            Expectation::RejectsAtStage(stage) => format!("rejects at stage {stage}"),
        }
    }
}

/// The verdict of one executed case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    Passed,
    Failed,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Passed => "passed",
            Verdict::Failed => "failed",
        }
    }
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One case that was actually executed.
///
/// Construction requires concrete input and an implementation result, which is
/// exactly the conformance requirement's threshold for calling something an
/// executed case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedCase {
    /// The catalog entry this case witnesses, e.g. `CLOSURE-007`.
    pub id: String,
    /// The catalog's own `contract` grouping.
    pub contract: String,
    /// The exact source that was run. Evidence, not a description of one.
    pub source: String,
    pub expectation: Expectation,
    pub observed: Observed,
    pub verdict: Verdict,
}

impl ExecutedCase {
    pub fn passed(&self) -> bool {
        self.verdict == Verdict::Passed
    }

    /// A one-line record for a report: what was expected, what was observed.
    pub fn serialize(&self) -> String {
        format!(
            "{} [{}] {} | expected: {} | observed: {} | source: {:?}",
            self.id,
            self.contract,
            self.verdict,
            self.expectation.serialize(),
            self.observed.serialize(),
            self.source
        )
    }
}

/// Why the runner could not be assembled.
#[derive(Debug)]
pub enum RunnerError {
    Spec(String),
}

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunnerError::Spec(detail) => write!(f, "the engine did not assemble: {detail}"),
        }
    }
}

impl std::error::Error for RunnerError {}

/// The whole engine, assembled once and reused.
pub struct Runner {
    lexicon: Lexicon,
    grammar: Grammar,
    rules: Rules,
    statics: StaticContracts,
    preflight: PreflightContracts,
    runtime: RuntimeContracts,
    completion: CompletionContracts,
    /// One assembled operation surface, lent mutably to each run.
    ///
    /// `Operations::invoke` takes `&mut self`, but `Stdlib` accumulates nothing
    /// across invocations: every mutating method on it is a builder that an
    /// embedder calls before running, and none is reachable from `invoke`. So
    /// reusing one surface leaves runs independent, which a conformance runner
    /// needs more than most callers.
    stdlib: RefCell<Stdlib>,
}

impl Runner {
    /// Assemble every layer from one verified package.
    pub fn new(spec: &SpecPackage) -> Result<Runner, RunnerError> {
        Runner::with_profiles(spec, Runner::shipped_profiles())
    }

    /// Every implementation profile the shipped adapters declare.
    ///
    /// A `Stdlib` starts with none, which makes every profile-requiring row
    /// fail its precondition before effects — correct for an engine with
    /// nothing installed, and useless for a conformance runner, which must be
    /// able to reach the rows the registry actually defines.
    pub fn shipped_profiles() -> Vec<lcl_capabilities::Profile> {
        lcl_stdlib::checking_profiles()
            .into_iter()
            .chain(lcl_stdlib::filesystem_profiles())
            .chain(lcl_stdlib::process_profiles())
            .chain(lcl_stdlib::transport_profiles())
            .chain(lcl_stdlib::store_profiles())
            .collect()
    }

    /// Assemble every layer with exactly `profiles` installed.
    ///
    /// A case about profile selection states the catalog it selects from — a
    /// missing, ambiguous, incomplete or out-of-bounds role, or a fixture
    /// profile for a row the shipped adapters cannot perform — and the rest of
    /// the engine is the same one [`Runner::new`] assembles.
    pub fn with_profiles(
        spec: &SpecPackage,
        profiles: Vec<lcl_capabilities::Profile>,
    ) -> Result<Runner, RunnerError> {
        Runner::with_surface(spec, |stdlib| stdlib.with_profiles(profiles))
    }

    /// Assemble every layer with the operation surface an embedder configured:
    /// its installed profiles and pure custom-operation implementations.
    pub fn with_surface(
        spec: &SpecPackage,
        configure: impl FnOnce(Stdlib) -> Stdlib,
    ) -> Result<Runner, RunnerError> {
        let e = |d: String| RunnerError::Spec(d);
        let lexicon = Lexicon::load(spec).map_err(|x| e(format!("{x}")))?;
        let grammar = Grammar::load(spec).map_err(|x| e(format!("{x}")))?;
        let rules = Rules::load(spec, &grammar).map_err(|x| e(format!("{x}")))?;
        let statics = StaticContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let preflight = PreflightContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let runtime = RuntimeContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let completion = CompletionContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let stdlib = configure(Stdlib::load(spec).map_err(|x| e(format!("{x}")))?);
        Ok(Runner {
            lexicon,
            grammar,
            rules,
            statics,
            preflight,
            runtime,
            completion,
            stdlib: RefCell::new(stdlib),
        })
    }

    /// The profile catalog the assembled operation surface selects from.
    pub fn stdlib_catalog(&self) -> lcl_capabilities::ProfileCatalog {
        self.stdlib.borrow().catalog().clone()
    }

    /// Execute exact bytes through the real lexer and parser, retaining all
    /// source diagnostics. No resolver, checker or host effects are invoked.
    pub fn run_source(&self, bytes: &[u8]) -> Observed {
        let lexed = Lexer::new(&self.lexicon).lex(bytes);
        let input_evidence = vec![format!("source bytes {bytes:?}")];
        if let Some(primary) = lexed.primary() {
            return Observed {
                reached: Reached::Lexical,
                primary: Some(primary.id.to_string()),
                primary_stage: Some("lexical".into()),
                diagnostics: lexed
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect(),
                diagnostic_loci: lexed
                    .diagnostics()
                    .iter()
                    .map(|d| (d.id.to_string(), d.span.start, d.span.end))
                    .collect(),
                input_evidence,
                ..Observed::default()
            };
        }
        let parsed = Parser::new(&self.grammar)
            .parse(&lexed)
            .expect("a clean lexical stage permits parsing");
        Observed {
            reached: Reached::Grammar,
            primary: parsed.primary().map(|d| d.id.to_string()),
            primary_stage: parsed.primary().map(|_| "grammar_or_schema".into()),
            diagnostics: parsed
                .diagnostics()
                .iter()
                .map(|d| d.id.to_string())
                .collect(),
            diagnostic_loci: parsed
                .diagnostics()
                .iter()
                .map(|d| (d.id.to_string(), d.span.start, d.span.end))
                .collect(),
            input_evidence,
            ..Observed::default()
        }
    }

    /// Carry one source through every implemented stage and report what
    /// happened.
    ///
    /// Total: returns for every input, including bytes that are not LCL at all.
    pub fn run(&self, source: &str) -> Observed {
        self.run_with_imports(source, &MemoryProvider::new())
    }

    /// The same, with an explicit source provider for a document that imports.
    pub fn run_with_imports(&self, source: &str, provider: &MemoryProvider) -> Observed {
        let mut host = MockHost::new();
        self.run_on(source, provider, &mut host)
    }

    /// The same, against a supplied host.
    ///
    /// A witness about an addressable target — a file with content, a program
    /// that exits, a transport that answers — needs a host that has one.
    /// `lcl-stdlib`'s in-memory fixtures supply exactly that, deterministically,
    /// so such a witness becomes executable without a real machine behind it.
    pub fn run_on(&self, source: &str, provider: &MemoryProvider, host: &mut dyn Host) -> Observed {
        let id = SourceId::new("case.lcl");
        let unit = SourceUnit::new(id, source.as_bytes());
        self.run_input(&unit, provider, &Invocation::new(), host)
    }

    /// Run exact source bytes, imports and typed invocation data through the
    /// same production layers as an ordinary case. Fixture data supplies no
    /// verdict and cannot substitute for an operation implementation.
    pub fn run_input(
        &self,
        unit: &SourceUnit,
        provider: &MemoryProvider,
        invocation: &Invocation,
        host: &mut dyn Host,
    ) -> Observed {
        let mut observed = self.observe_input(unit, provider, invocation, host);
        observed.input_evidence = vec![
            format!("root {unit:?}"),
            format!("imports {provider:?}"),
            format!("supplied {invocation:?}"),
        ];
        observed
    }

    fn observe_input(
        &self,
        unit: &SourceUnit,
        provider: &MemoryProvider,
        invocation: &Invocation,
        host: &mut dyn Host,
    ) -> Observed {
        let resolved = match Resolver::new(&self.rules, &self.grammar, &self.lexicon)
            .resolve(unit, provider)
        {
            Ok(resolved) => resolved,
            Err(skipped) => {
                // A source that fails lexing or parsing has no resolved model.
                let stage = skipped.stage.as_registry_str().to_string();
                return Observed {
                    reached: if stage.contains("lexical") {
                        Reached::Lexical
                    } else {
                        Reached::Grammar
                    },
                    primary: Some(skipped.primary.clone()),
                    primary_stage: Some(stage),
                    diagnostics: vec![skipped.primary],
                    terminal_status: None,
                    checks: Vec::new(),
                    outputs: Vec::new(),
                    ..Observed::default()
                };
            }
        };
        if let Some(primary) = resolved.primary() {
            return Observed {
                reached: Reached::Resolution,
                primary: Some(primary.id.to_string()),
                primary_stage: Some(primary.stage().as_registry_str().to_string()),
                diagnostics: resolved
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect(),
                terminal_status: None,
                checks: Vec::new(),
                outputs: Vec::new(),
                ..Observed::default()
            };
        }

        let checked: Checked = match Checker::new(&self.statics).check(&resolved) {
            Ok(checked) => checked,
            Err(skipped) => {
                return Observed {
                    reached: Reached::Resolution,
                    primary: Some(skipped.primary.clone()),
                    primary_stage: Some("resolution".to_string()),
                    diagnostics: vec![skipped.primary],
                    terminal_status: None,
                    checks: Vec::new(),
                    outputs: Vec::new(),
                    ..Observed::default()
                }
            }
        };
        if let Some(primary) = checked.primary() {
            return Observed {
                reached: Reached::StaticChecking,
                primary: Some(primary.id.to_string()),
                primary_stage: Some(primary.stage().as_registry_str().to_string()),
                diagnostics: checked
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect(),
                terminal_status: None,
                checks: Vec::new(),
                outputs: Vec::new(),
                ..Observed::default()
            };
        }

        let planned = match Preflight::new(&self.preflight).plan(&checked, &resolved, invocation) {
            Ok(planned) => planned,
            Err(skipped) => {
                return Observed {
                    reached: Reached::StaticChecking,
                    primary: Some(skipped.primary.clone()),
                    primary_stage: Some("static_or_expression".to_string()),
                    diagnostics: vec![skipped.primary],
                    terminal_status: None,
                    checks: Vec::new(),
                    outputs: Vec::new(),
                    ..Observed::default()
                }
            }
        };
        if planned.outcome() != Outcome::Planned {
            let primary = planned.primary();
            return Observed {
                reached: Reached::Preflight,
                primary: primary.map(|d| d.id.to_string()),
                primary_stage: primary.map(|d| d.stage.as_registry_str().to_string()),
                diagnostics: planned
                    .diagnostics()
                    .iter()
                    .map(|d| d.id.to_string())
                    .collect(),
                terminal_status: None,
                checks: Vec::new(),
                outputs: Vec::new(),
                ..Observed::default()
            };
        }

        let mut stdlib = self.stdlib.borrow_mut();
        let execution = match Runtime::new(&self.runtime).execute_with(
            &planned,
            &checked,
            &resolved,
            &mut *stdlib,
            host,
        ) {
            Ok(execution) => execution,
            Err(_) => {
                return Observed {
                    reached: Reached::Preflight,
                    primary: planned.primary().map(|d| d.id.to_string()),
                    primary_stage: None,
                    diagnostics: Vec::new(),
                    terminal_status: None,
                    checks: Vec::new(),
                    outputs: Vec::new(),
                    ..Observed::default()
                }
            }
        };

        let completion =
            match Completion::of(&self.completion, &planned, &checked, &resolved, &execution) {
                Ok(completion) => completion,
                Err(_) => {
                    return Observed {
                        reached: Reached::Execution,
                        primary: execution.primary().map(|d| d.id.to_string()),
                        primary_stage: None,
                        diagnostics: execution
                            .diagnostics()
                            .iter()
                            .map(|d| d.id.to_string())
                            .collect(),
                        terminal_status: None,
                        checks: Vec::new(),
                        outputs: Vec::new(),
                        ..Observed::default()
                    }
                }
            };

        // Diagnostics from both post-preflight layers, execution first, which
        // is `earliest_stage_rule` order for these two stages.
        let mut diagnostics: Vec<String> = execution
            .diagnostics()
            .iter()
            .map(|d| d.id.to_string())
            .collect();
        diagnostics.extend(
            completion
                .diagnostics()
                .iter()
                .map(|d| d.id.as_registry_str().to_string()),
        );

        let primary = execution
            .primary()
            .map(|d| (d.id.to_string(), d.stage().as_registry_str().to_string()))
            .or_else(|| {
                completion.diagnostics().first().map(|d| {
                    (
                        d.id.as_registry_str().to_string(),
                        d.stage().as_registry_str().to_string(),
                    )
                })
            });

        let mut checks: Vec<(String, String)> = completion
            .checks()
            .results()
            .iter()
            .map(|c| (c.id.clone(), c.outcome.to_string()))
            .collect();
        // A skipped check has no result. It is reported as absent rather than
        // omitted silently, so a case can assert that absence.
        for skipped in completion.checks().skipped() {
            checks.push((skipped.id.clone(), "SKIPPED".to_string()));
        }
        checks.sort();

        let mut outputs: Vec<(String, String)> = completion
            .outputs()
            .records()
            .iter()
            .map(|r| {
                let publication = match &r.publication {
                    lcl_completion::Publication::Published(value) => value.to_string(),
                    lcl_completion::Publication::Unbound => "UNBOUND".to_string(),
                    lcl_completion::Publication::Ambiguous(n) => format!("AMBIGUOUS({n})"),
                };
                (r.id.clone(), publication)
            })
            .collect();
        outputs.sort();

        Observed {
            reached: Reached::Completion,
            primary: primary.as_ref().map(|(id, _)| id.clone()),
            primary_stage: primary.map(|(_, stage)| stage),
            diagnostics,
            terminal_status: Some(completion.terminal_status().to_string()),
            checks,
            outputs,
            invocations: execution.invocations().to_vec(),
            events: execution.events().to_vec(),
            retry_proofs: execution.retry_proofs().to_vec(),
            input_evidence: Vec::new(),
            diagnostic_loci: Vec::new(),
            ..Observed::default()
        }
    }

    /// Execute one case and judge it against its own expectation.
    pub fn execute(
        &self,
        id: &str,
        contract: &str,
        source: &str,
        expectation: Expectation,
    ) -> ExecutedCase {
        self.execute_with_imports(id, contract, source, expectation, &MemoryProvider::new())
    }

    /// Execute one case against a supplied host.
    pub fn execute_on(
        &self,
        id: &str,
        contract: &str,
        source: &str,
        expectation: Expectation,
        host: &mut dyn Host,
    ) -> ExecutedCase {
        let observed = self.run_on(source, &MemoryProvider::new(), host);
        let verdict = judge(&expectation, &observed);
        ExecutedCase {
            id: id.to_string(),
            contract: contract.to_string(),
            source: source.to_string(),
            expectation,
            observed,
            verdict,
        }
    }

    pub fn execute_with_imports(
        &self,
        id: &str,
        contract: &str,
        source: &str,
        expectation: Expectation,
        provider: &MemoryProvider,
    ) -> ExecutedCase {
        let observed = self.run_with_imports(source, provider);
        let verdict = judge(&expectation, &observed);
        ExecutedCase {
            id: id.to_string(),
            contract: contract.to_string(),
            source: source.to_string(),
            expectation,
            observed,
            verdict,
        }
    }
}

/// One exact field of the subject action's first attempt.
pub fn attempt_field(field: &str, value: &str) -> Expectation {
    Expectation::AttemptField {
        declaration: "action.subject".into(),
        attempt: 0,
        field: field.into(),
        value: value.into(),
    }
}

/// Compare one expectation against one observation.
///
/// Free function, and deliberately the only place a verdict is decided.
pub fn judge(expectation: &Expectation, observed: &Observed) -> Verdict {
    let passed = match expectation {
        Expectation::Runs(runs) => {
            !runs.is_empty()
                && runs.len() == observed.runs.len()
                && runs
                    .iter()
                    .zip(&observed.runs)
                    .all(|(e, o)| !o.input_evidence.is_empty() && judge(e, o) == Verdict::Passed)
        }
        Expectation::Component(values) => {
            !values.is_empty()
                && !observed.input_evidence.is_empty()
                && *values == observed.component
        }
        Expectation::SourcePass(stage) => {
            matches!(stage, Reached::Lexical | Reached::Grammar)
                && (observed.reached > *stage
                    || (observed.reached == *stage && observed.primary.is_none()))
        }
        Expectation::All(assertions) => {
            !assertions.is_empty()
                && assertions
                    .iter()
                    .all(|e| judge(e, observed) == Verdict::Passed)
        }
        Expectation::Attempts {
            declaration,
            statuses,
        } => {
            let actual: Vec<_> = observed
                .invocations
                .iter()
                .filter(|i| i.declaration.as_ref() == Some(declaration))
                .collect();
            actual.len() == statuses.len()
                && actual
                    .iter()
                    .zip(statuses)
                    .enumerate()
                    .all(|(attempt, (record, status))| {
                        record.id.attempt == attempt && record.status() == status
                    })
        }
        Expectation::RetryProofs(count) => observed.retry_proofs.len() == *count,
        Expectation::NoDiagnostic(id) => {
            observed.primary.as_ref() != Some(id) && !observed.diagnostics.contains(id)
        }
        Expectation::DiagnosticAt { id, start, end } => {
            observed
                .diagnostic_loci
                .contains(&(id.clone(), *start, *end))
        }
        Expectation::Diagnostic(id) => observed.diagnostics.contains(id),
        Expectation::Recovered(id) => observed.events.iter().any(|event| {
            observed
                .invocations
                .iter()
                .any(|i| i.id == event.producer && i.declaration.as_ref() == Some(id))
                && matches!(
                    event.disposition,
                    lcl_runtime::Disposition::Selected {
                        recovered: true,
                        ..
                    }
                )
        }),
        Expectation::Output { id, value } => observed
            .outputs
            .iter()
            .any(|(found, actual)| found == id && actual == value),
        Expectation::AttemptField {
            declaration,
            attempt,
            field,
            value,
        } => {
            observed
                .invocations
                .iter()
                .find(|i| i.declaration.as_ref() == Some(declaration) && i.id.attempt == *attempt)
                .and_then(|i| {
                    if field == "initial_output" {
                        i.initial_output.as_ref().map(ToString::to_string)
                    } else {
                        i.result.as_ref().and_then(|r| match field.as_str() {
                            "output_binding" => Some(r.output_binding.to_string()),
                            "failure_phase" => Some(r.failure_phase.to_string()),
                            "effect_state" => Some(r.effect_state.to_string()),
                            "status" => Some(r.status.clone()),
                            "schema" => Some(r.schema.clone()),
                            field => r.field(field).map(ToString::to_string),
                        })
                    }
                })
                .as_ref()
                == Some(value)
        }
        Expectation::Rejects(id) => {
            observed.primary.as_deref() == Some(id.as_str())
                || (observed.primary.is_none() && observed.diagnostics.iter().any(|d| d == id))
        }
        Expectation::RejectsAtStage(stage) => {
            observed.primary.is_some() && observed.primary_stage.as_deref() == Some(stage.as_str())
        }
        Expectation::Accepts => {
            observed.primary.is_none() && observed.reached == Reached::Completion
        }
        Expectation::Check { id, outcome } => observed
            .checks
            .iter()
            .any(|(check, value)| check == id && value == outcome),
        Expectation::Terminal(status) => observed.terminal_status.as_deref() == Some(status),
    };
    if passed {
        Verdict::Passed
    } else {
        Verdict::Failed
    }
}

#[cfg(test)]
mod grouped_evidence_tests {
    use super::*;
    #[test]
    fn grouped_evidence_requires_every_ordered_observation_and_exact_input() {
        let component = vec![("transition".into(), "refused".into())];
        let run = Observed {
            component: component.clone(),
            input_evidence: vec!["ready -> invented".into()],
            ..Observed::default()
        };
        let expectation = Expectation::Runs(vec![
            Expectation::Component(component.clone()),
            Expectation::Component(component),
        ]);
        let mut observed = Observed {
            runs: vec![run.clone(), run],
            ..Observed::default()
        };
        assert_eq!(judge(&expectation, &observed), Verdict::Passed);
        observed.runs[1].component[0].1 = "allowed".into();
        assert_eq!(judge(&expectation, &observed), Verdict::Failed);
        observed.runs.pop();
        assert_eq!(judge(&expectation, &observed), Verdict::Failed);
        assert_eq!(
            judge(&Expectation::Runs(vec![]), &Observed::default()),
            Verdict::Failed
        );
        observed.runs[0].input_evidence.clear();
        assert_eq!(
            judge(
                &Expectation::Runs(vec![Expectation::Component(
                    observed.runs[0].component.clone()
                )]),
                &observed
            ),
            Verdict::Failed
        );
    }

    #[test]
    fn run_labels_render_beside_their_runs() {
        let run = Observed {
            component: vec![("transition".into(), "refused".into())],
            input_evidence: vec!["ready -> invented".into()],
            ..Observed::default()
        };
        let mut observed = Observed {
            runs: vec![run.clone(), run],
            run_labels: vec!["form/0".into(), "form/1".into()],
            ..Observed::default()
        };
        let rendered = observed.serialize();
        assert!(rendered.contains(" run[0:form/0]{"), "{rendered}");
        assert!(rendered.contains(" run[1:form/1]{"), "{rendered}");
        observed.run_labels.clear();
        let unlabelled = observed.serialize();
        assert!(
            unlabelled.contains(" run[0]{") && !unlabelled.contains(":form/"),
            "{unlabelled}"
        );
    }
}
