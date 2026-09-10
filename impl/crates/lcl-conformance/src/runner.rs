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
use lcl_lexer::Lexicon;
use lcl_parser::Grammar;
use lcl_resolver::{MemoryProvider, Resolver, Rules, SourceId, SourceUnit};
use lcl_runtime::{Contracts as RuntimeContracts, Host, MockHost, Runtime};
use lcl_semantics::{Contracts as PreflightContracts, Invocation, Outcome, Preflight};
use lcl_spec::SpecPackage;
use lcl_stdlib::Stdlib;
use std::cell::RefCell;
use std::fmt;

/// The furthest canonical stage a source reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reached {
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The furthest stage the source reached.
    pub reached: Reached,
    /// The registered identifier of the first unhandled diagnostic, if any.
    pub primary: Option<String>,
    /// That diagnostic's registered stage.
    pub primary_stage: Option<String>,
    /// Every registered identifier any stage produced, in stage order.
    pub diagnostics: Vec<String>,
    /// The one terminal status, when the invocation completed.
    pub terminal_status: Option<String>,
    /// Each post-execution check and the Boolean domain outcome it recorded.
    pub checks: Vec<(String, String)>,
    /// Each declared root output and its publication, rendered.
    pub outputs: Vec<(String, String)>,
}

impl Observed {
    /// A canonical, order-stable rendering, for evidence in a report.
    pub fn serialize(&self) -> String {
        let mut out = format!("reached={}", self.reached);
        if let Some(primary) = &self.primary {
            out.push_str(&format!(" primary={primary}"));
        }
        if let Some(status) = &self.terminal_status {
            out.push_str(&format!(" terminal={status}"));
        }
        for (id, outcome) in &self.checks {
            out.push_str(&format!(" check[{id}]={outcome}"));
        }
        for (id, publication) in &self.outputs {
            out.push_str(&format!(" output[{id}]={publication}"));
        }
        out
    }
}

/// What a case requires of the engine.
///
/// Data, not code: a case states its expectation and the runner compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expectation {
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
            "{} [{}] {} | expected: {} | observed: {}",
            self.id,
            self.contract,
            self.verdict,
            self.expectation.serialize(),
            self.observed.serialize()
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
        let e = |d: String| RunnerError::Spec(d);
        let lexicon = Lexicon::load(spec).map_err(|x| e(format!("{x}")))?;
        let grammar = Grammar::load(spec).map_err(|x| e(format!("{x}")))?;
        let rules = Rules::load(spec, &grammar).map_err(|x| e(format!("{x}")))?;
        let statics = StaticContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let preflight = PreflightContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let runtime = RuntimeContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        let completion = CompletionContracts::load(spec).map_err(|x| e(format!("{x}")))?;
        // Every registered implementation profile is installed. A `Stdlib`
        // starts with none, which makes every profile-requiring row fail its
        // precondition before effects — correct for an engine with nothing
        // installed, and useless for a conformance runner, which must be able
        // to reach the rows the registry actually defines.
        let stdlib = Stdlib::load(spec)
            .map_err(|x| e(format!("{x}")))?
            .with_profiles(
                lcl_stdlib::checking_profiles()
                    .into_iter()
                    .chain(lcl_stdlib::filesystem_profiles())
                    .chain(lcl_stdlib::process_profiles())
                    .chain(lcl_stdlib::transport_profiles())
                    .collect::<Vec<_>>(),
            );
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

        let resolved = match Resolver::new(&self.rules, &self.grammar, &self.lexicon)
            .resolve(&unit, provider)
        {
            Ok(resolved) => resolved,
            Err(skipped) => {
                // A source that fails lexing or parsing has no resolved model.
                let stage = format!("{:?}", skipped.stage).to_lowercase();
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
            };
        }

        let planned =
            match Preflight::new(&self.preflight).plan(&checked, &resolved, &Invocation::new()) {
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

/// Compare one expectation against one observation.
///
/// Free function, and deliberately the only place a verdict is decided.
pub fn judge(expectation: &Expectation, observed: &Observed) -> Verdict {
    let passed = match expectation {
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
