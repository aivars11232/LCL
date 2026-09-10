//! The stable engine facade: one assembled engine, one staged walk.
//!
//! ## Why this exists
//!
//! Before this crate, the sequence "lexicon, grammar, rules, static contracts,
//! preflight contracts, runtime contracts, completion contracts, then resolve,
//! check, plan, execute, complete" was written out by hand in the conformance
//! runner, in every milestone report example, and in six `tests/common/mod.rs`
//! files. Each copy is a place where a stage could be skipped, reordered or
//! quietly substituted. There is now one copy, and it is this one.
//!
//! ## Stage monotonicity is the shape of the code
//!
//! [`Engine::request`] advances only while the earlier stage produced no
//! unhandled diagnostic, exactly as
//! `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt` requires: "Each source
//! unit or invocation path advances only while its earlier applicable stage has
//! no unhandled unsuppressed diagnostic." There is no argument that skips a
//! stage and no entry point that starts in the middle.
//!
//! ## The facade decides nothing about the language
//!
//! Every identifier, stage, status, span and value in a [`Report`] is copied
//! from the layer that produced it. This module selects no diagnostic, resolves
//! no alias, and classifies no stage. What it *does* decide is where a command
//! stops, which is a product question: `check` stops after step 5, `validate`
//! and `inspect` after step 9 and before any effect, `run` after step 13.
//!
//! ## The host is the caller's
//!
//! [`Engine::request`] takes a `&mut dyn Host` and a `&mut Stdlib`. It installs
//! no adapter and grants no permission, because deciding what a run may touch
//! is the embedder's decision, not the engine's. An engine handed a host with
//! nothing installed runs a document that needs nothing and reports
//! `error.host.constraint` for one that does — which is the truthful outcome,
//! not a degraded one.

use crate::inputs::{Inputs, Supplied};
use crate::record::{
    CheckRecord, Command, CompletionRecord, DiagnosticRecord, EventRecord, EvidenceRecord,
    ExecutionRecord, ImportRecord, InputRecord, InvocationRecord, Outcome, OutputRecord,
    PlanRecord, Reached, Report, SourceRecord, SpecRecord, StructureRecord, VerdictRecord,
};
use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_completion::{Completion, Contracts as CompletionContracts, Publication, SkipReason};
use lcl_diagnostics::Stage;
use lcl_lexer::{Lexer, Lexicon, Position, Span};
use lcl_parser::syntax::Expr;
use lcl_parser::{Grammar, Parser};
use lcl_resolver::{Resolved, ResolvedUnit, Resolver, Rules, SourceId, SourceProvider, SourceUnit};
use lcl_runtime::{Contracts as RuntimeContracts, Execution, Host, Runtime};
use lcl_semantics::{
    Contracts as PreflightContracts, Invocation, Outcome as PreflightOutcome, Planned, Preflight,
};
use lcl_spec::{SpecError, SpecPackage};
use lcl_stdlib::{Stdlib, StdlibError};
use std::fmt;
use std::path::Path;

/// Why an engine could not be assembled.
///
/// Assembly failure is not a language outcome. It means the tool could not
/// reach an authoritative specification package, so no report is produced at
/// all rather than a report claiming an empty result.
#[derive(Debug)]
pub enum EngineError {
    /// The package did not open, verify, or match the trust anchor.
    Package(SpecError),
    /// A layer's contracts did not load from the package.
    Contracts { layer: &'static str, detail: String },
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Package(inner) => {
                write!(f, "the specification package did not load: {inner}")
            }
            EngineError::Contracts { layer, detail } => {
                write!(f, "the {layer} contracts did not load: {detail}")
            }
        }
    }
}

impl std::error::Error for EngineError {}

/// One assembled engine.
///
/// Holds every layer's loaded contracts and the verified package they came
/// from. Assembling is the expensive part and is done once; a request holds no
/// state and leaves none behind, so one engine serves any number of documents
/// with identical results for identical input.
pub struct Engine {
    spec: SpecPackage,
    lexicon: Lexicon,
    grammar: Grammar,
    rules: Rules,
    statics: StaticContracts,
    preflight: PreflightContracts,
    runtime: RuntimeContracts,
    completion: CompletionContracts,
    record: SpecRecord,
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Engine")
            .field("spec", &self.record.root)
            .field("formal_version", &self.record.formal_version)
            .field("authority", &self.record.authority)
            .finish()
    }
}

impl Engine {
    /// Open the approved package at `root` and assemble every layer.
    ///
    /// The package is opened through [`SpecPackage::open`], so the version pin,
    /// the internal integrity check and the external trust anchor all apply. An
    /// engine cannot be built on an unverified package.
    pub fn open(root: impl AsRef<Path>) -> Result<Engine, EngineError> {
        let spec = SpecPackage::open(root).map_err(EngineError::Package)?;
        Engine::assemble(spec)
    }

    /// Assemble every layer from an already-opened package.
    pub fn assemble(spec: SpecPackage) -> Result<Engine, EngineError> {
        let fail =
            |layer: &'static str| move |detail: String| EngineError::Contracts { layer, detail };
        let lexicon = Lexicon::load(&spec).map_err(|e| fail("lexicon")(e.to_string()))?;
        let grammar = Grammar::load(&spec).map_err(|e| fail("grammar")(e.to_string()))?;
        let rules = Rules::load(&spec, &grammar).map_err(|e| fail("resolution")(e.to_string()))?;
        let statics = StaticContracts::load(&spec).map_err(|e| fail("static")(e.to_string()))?;
        let preflight =
            PreflightContracts::load(&spec).map_err(|e| fail("preflight")(e.to_string()))?;
        let runtime = RuntimeContracts::load(&spec).map_err(|e| fail("runtime")(e.to_string()))?;
        let completion =
            CompletionContracts::load(&spec).map_err(|e| fail("completion")(e.to_string()))?;
        let record = SpecRecord {
            root: spec.root().display().to_string(),
            formal_version: spec.formal_version().to_string(),
            identity_digest: spec.identity_digest().to_string(),
            authority: if spec.is_authoritative() {
                "authoritative".to_string()
            } else {
                "unverified".to_string()
            },
        };
        Ok(Engine {
            spec,
            lexicon,
            grammar,
            rules,
            statics,
            preflight,
            runtime,
            completion,
            record,
        })
    }

    /// The verified package this engine reads its authority from.
    pub fn spec(&self) -> &SpecPackage {
        &self.spec
    }

    /// The package record every report carries.
    pub fn spec_record(&self) -> &SpecRecord {
        &self.record
    }

    /// The lexicon, for a caller that needs the closed vocabulary as data.
    pub fn lexicon(&self) -> &Lexicon {
        &self.lexicon
    }

    /// A fresh Core operation surface with no implementation profile installed.
    ///
    /// Deliberately bare. A row that requires a profile role then fails its
    /// precondition before effects, which is correct for an engine holding no
    /// implementations. The embedder installs the profiles matching the
    /// capabilities it actually gave the host.
    pub fn stdlib(&self) -> Result<Stdlib, StdlibError> {
        Stdlib::load(&self.spec)
    }

    /// Steps 1 to 5, for one document.
    ///
    /// Takes no supplied data: nothing before step 7 reads any.
    pub fn check(&self, unit: &SourceUnit, provider: &dyn SourceProvider) -> Report {
        self.request(Command::Check, unit, provider, &Inputs::new(), None)
    }

    /// Steps 1 to 9. No effect can occur.
    pub fn validate(
        &self,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
    ) -> Report {
        self.request(Command::Validate, unit, provider, inputs, None)
    }

    /// Steps 1 to 9, reported structurally. No effect can occur.
    pub fn inspect(
        &self,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
    ) -> Report {
        self.request(Command::Inspect, unit, provider, inputs, None)
    }

    /// Steps 1 to 13, against an explicit operation surface and host.
    pub fn run(
        &self,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
        stdlib: &mut Stdlib,
        host: &mut dyn Host,
    ) -> Report {
        self.request(Command::Run, unit, provider, inputs, Some((stdlib, host)))
    }

    /// The one staged walk every command uses.
    fn request(
        &self,
        command: Command,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
        effects: Option<(&mut Stdlib, &mut dyn Host)>,
    ) -> Report {
        let mut report = Report {
            command,
            spec: self.record.clone(),
            units: vec![SourceRecord {
                id: unit.id().to_string(),
                digest: unit.digest(),
                bytes: unit.bytes().len(),
                root: true,
            }],
            reached: Reached::Lexical,
            outcome: Outcome::Rejected,
            inputs: Vec::new(),
            diagnostics: Vec::new(),
            structure: None,
            execution: None,
            completion: None,
        };

        // Steps 1 to 4. The resolver drives the lexer and parser itself, so a
        // failure of either arrives here as a skipped stage rather than as a
        // resolution result.
        let resolved = match Resolver::new(&self.rules, &self.grammar, &self.lexicon)
            .resolve(unit, provider)
        {
            Ok(resolved) => resolved,
            Err(skipped) => {
                // The root did not survive step 1 or 3. Its full diagnostic
                // list is not in a `Resolved` that was never built, so the two
                // stages are re-run over the same bytes to report all of them.
                // Same lexer, same parser, same input: a second pass observes,
                // it does not decide.
                report.reached = match skipped.stage {
                    Stage::Lexical => Reached::Lexical,
                    _ => Reached::Grammar,
                };
                report.diagnostics = self.early_diagnostics(unit);
                mark_primary(&mut report.diagnostics);
                return report;
            }
        };

        // Every loaded unit, root first, in the resolver's load order.
        report.units = resolved
            .units()
            .map(|u| SourceRecord {
                id: u.id().to_string(),
                digest: u.digest().to_string(),
                bytes: u.source().len(),
                root: u.id() == resolved.root(),
            })
            .collect();

        // Steps 1 to 3 for every loaded unit, including imported ones that
        // failed an earlier stage while the root did not.
        let early: Vec<DiagnosticRecord> = resolved.units().flat_map(unit_diagnostics).collect();
        if !early.is_empty() {
            report.reached = early
                .iter()
                .map(|d| match d.stage {
                    Stage::Lexical => Reached::Lexical,
                    _ => Reached::Grammar,
                })
                .min()
                .unwrap_or(Reached::Lexical);
            report.diagnostics = early;
            mark_primary(&mut report.diagnostics);
            return report;
        }

        report.reached = Reached::Resolution;
        if resolved.primary().is_some() {
            report.diagnostics = resolved
                .diagnostics()
                .iter()
                .map(|d| DiagnosticRecord {
                    id: d.id.to_string(),
                    stage: d.stage(),
                    source: d.source.to_string(),
                    span: d.span,
                    position: d.position,
                    meaning: d.meaning.clone(),
                    default_status: d.default_status.clone(),
                    specificity_rank: d.specificity_rank,
                    event: None,
                    cause: format!("{:?}", d.cause),
                    detail: d.detail.clone(),
                    sequence: None,
                    primary: false,
                })
                .collect();
            mark_primary(&mut report.diagnostics);
            return report;
        }

        // Step 5.
        let checked = match Checker::new(&self.statics).check(&resolved) {
            Ok(checked) => checked,
            Err(skipped) => {
                // Structurally unreachable: the resolver's outcome was clean.
                // Reported rather than unwrapped, because a panic in a product
                // facade would destroy the report a caller needs.
                report.diagnostics.push(placeholder(
                    &skipped.primary,
                    Stage::Resolution,
                    resolved.root(),
                ));
                mark_primary(&mut report.diagnostics);
                return report;
            }
        };
        report.reached = Reached::StaticChecking;
        if checked.primary().is_some() {
            report.diagnostics = checked
                .diagnostics()
                .iter()
                .map(|d| DiagnosticRecord {
                    id: d.id.to_string(),
                    stage: d.stage(),
                    source: d.source.to_string(),
                    span: d.span,
                    position: d.position,
                    meaning: d.meaning.clone(),
                    default_status: d.default_status.clone(),
                    specificity_rank: d.specificity_rank,
                    event: None,
                    cause: format!("{:?}", d.cause),
                    detail: d.detail.clone(),
                    sequence: None,
                    primary: false,
                })
                .collect();
            mark_primary(&mut report.diagnostics);
            return report;
        }

        if command == Command::Check {
            report.outcome = Outcome::Accepted;
            return report;
        }

        // Supplied data becomes values here: after step 5, because conversion
        // uses the language's own evaluation and that needs a checked program,
        // and before step 7, which is the step that reads it.
        let (invocation, records) = self.convert(&checked, &resolved, inputs);
        report.inputs = records;
        if report.inputs.iter().any(|record| !record.accepted()) {
            // The request was not well formed. The document is not judged, and
            // the report says so rather than reporting a stage verdict it never
            // reached.
            report.outcome = Outcome::Refused;
            return report;
        }

        // Steps 6 to 9. No effect occurs in this layer.
        let planned = match Preflight::new(&self.preflight).plan(&checked, &resolved, &invocation) {
            Ok(planned) => planned,
            Err(skipped) => {
                report.diagnostics.push(placeholder(
                    &skipped.primary,
                    Stage::StaticOrExpression,
                    resolved.root(),
                ));
                mark_primary(&mut report.diagnostics);
                return report;
            }
        };
        report.reached = Reached::Preflight;
        report.diagnostics = planned
            .diagnostics()
            .iter()
            .map(|d| DiagnosticRecord {
                id: d.id.to_string(),
                stage: d.stage,
                source: d.source.to_string(),
                span: d.span,
                position: d.position,
                meaning: d.meaning.clone(),
                default_status: d.default_status.clone(),
                specificity_rank: d.specificity_rank,
                event: d.event.clone(),
                cause: format!("{:?}", d.cause),
                detail: d.detail.clone(),
                sequence: None,
                primary: false,
            })
            .collect();

        if command == Command::Inspect {
            report.structure = Some(structure(&resolved, &planned));
        }

        if planned.outcome() != PreflightOutcome::Planned {
            mark_primary(&mut report.diagnostics);
            return report;
        }

        if command != Command::Run {
            report.outcome = Outcome::Accepted;
            return report;
        }

        // Step 10. The first stage permitted to reach outside the language.
        let Some((stdlib, host)) = effects else {
            // `run` was requested without an operation surface. Reported, never
            // substituted: a run with no host is not a run with a silent one.
            report.diagnostics.push(placeholder(
                "error.host.constraint",
                Stage::Execution,
                resolved.root(),
            ));
            mark_primary(&mut report.diagnostics);
            return report;
        };
        let execution = match Runtime::new(&self.runtime)
            .execute_with(&planned, &checked, &resolved, stdlib, host)
        {
            Ok(execution) => execution,
            Err(not_planned) => {
                report.diagnostics.push(placeholder(
                    not_planned
                        .primary
                        .as_deref()
                        .unwrap_or("error.execution.order"),
                    Stage::Validation,
                    resolved.root(),
                ));
                mark_primary(&mut report.diagnostics);
                return report;
            }
        };
        report.reached = Reached::Execution;
        report.execution = Some(execution_record(&execution));

        // Steps 11 to 13.
        let completion =
            match Completion::of(&self.completion, &planned, &checked, &resolved, &execution) {
                Ok(completion) => completion,
                Err(_) => {
                    report.diagnostics.extend(execution_diagnostics(&execution));
                    mark_primary(&mut report.diagnostics);
                    return report;
                }
            };
        report.reached = Reached::Completion;
        report.outcome = Outcome::Accepted;

        // `earliest_stage_rule` order for the two post-preflight stages:
        // execution first, then verification and completion.
        //
        // The two layers number their emission identities independently, so
        // the execution layer's records are located by the range they occupy
        // rather than by a sequence value that both layers can produce.
        let execution_range = {
            let start = report.diagnostics.len();
            report.diagnostics.extend(execution_diagnostics(&execution));
            start..report.diagnostics.len()
        };
        report
            .diagnostics
            .extend(completion.diagnostics().iter().map(|d| DiagnosticRecord {
                id: d.id.as_registry_str().to_string(),
                stage: d.stage(),
                source: d.source.to_string(),
                span: d.span,
                position: d.position,
                meaning: d.meaning.clone(),
                default_status: d.default_status.clone(),
                specificity_rank: d.specificity_rank,
                event: d.event.clone(),
                cause: d.cause.clone(),
                detail: Some(d.detail.clone()).filter(|t| !t.is_empty()),
                sequence: Some(d.sequence),
                primary: false,
            }));
        // `primary_rule` at these stages excludes a recovered diagnostic and a
        // diagnostic a successful `FALLBACK` substituted for. The runtime
        // already decided both, and its answer is adopted by emission identity
        // rather than by list position, because selection reorders the list.
        match execution.primary().map(|d| d.sequence) {
            Some(sequence) => {
                if let Some(record) = report.diagnostics[execution_range]
                    .iter_mut()
                    .find(|d| d.sequence == Some(sequence))
                {
                    record.primary = true;
                }
            }
            None => {
                if let Some(record) = report
                    .diagnostics
                    .iter_mut()
                    .find(|d| d.stage == Stage::VerificationOrCompletion)
                {
                    record.primary = true;
                }
            }
        }

        report.completion = Some(completion_record(&completion));
        report
    }

    /// Turn supplied data into an [`Invocation`], reporting each conversion.
    ///
    /// The conversion is the language's own: the expression is lexed by the
    /// real lexer, parsed by the real parser through
    /// [`Parser::expression_fragment`], and evaluated by
    /// [`Preflight::value_of`], which is the same function `DATA` and `INPUT`
    /// resolution uses at step 7. Nothing here reads a literal itself.
    ///
    /// A supplied expression carries its own source identity, `<input id>`, so
    /// its spans cannot match a static annotation belonging to a real unit.
    fn convert(
        &self,
        checked: &Checked,
        resolved: &Resolved,
        inputs: &Inputs,
    ) -> (Invocation, Vec<InputRecord>) {
        let mut invocation = Invocation::new();
        let mut records = Vec::new();
        for (id, supplied) in inputs.entries() {
            match supplied {
                Supplied::Value(value) => {
                    records.push(InputRecord {
                        id: id.clone(),
                        expression: None,
                        value: Some(value.to_string()),
                        reason: None,
                    });
                    invocation = invocation.with(id.clone(), value.clone());
                }
                Supplied::Text(text) => {
                    let record = match Parser::new(&self.grammar)
                        .expression_fragment(&self.lexicon, text)
                    {
                        Err(error) => InputRecord {
                            id: id.clone(),
                            expression: Some(text.clone()),
                            value: None,
                            reason: Some(format!("not one LCL expression: {error}")),
                        },
                        Ok(expression) if contains_reference(&expression) => InputRecord {
                            id: id.clone(),
                            expression: Some(text.clone()),
                            value: None,
                            reason: Some(
                                "a supplied datum may not read the document it is supplied \
                                 to; write the value itself"
                                    .to_string(),
                            ),
                        },
                        Ok(expression) => {
                            let source = SourceId::new(format!("<input {id}>"));
                            match Preflight::new(&self.preflight).value_of(
                                checked,
                                resolved,
                                &source,
                                &expression,
                            ) {
                                Some(value) => {
                                    let rendered = value.to_string();
                                    invocation = invocation.with(id.clone(), value);
                                    InputRecord {
                                        id: id.clone(),
                                        expression: Some(text.clone()),
                                        value: Some(rendered),
                                        reason: None,
                                    }
                                }
                                None => InputRecord {
                                    id: id.clone(),
                                    expression: Some(text.clone()),
                                    value: None,
                                    reason: Some(
                                        "the expression resolves to no material value".to_string(),
                                    ),
                                },
                            }
                        }
                    };
                    records.push(record);
                }
            }
        }
        (invocation, records)
    }

    /// Steps 1 and 3 over one unit's bytes, for the root that never reached
    /// resolution.
    fn early_diagnostics(&self, unit: &SourceUnit) -> Vec<DiagnosticRecord> {
        let source = unit.id().to_string();
        let lexed = Lexer::new(&self.lexicon).lex(unit.bytes());
        let mut records: Vec<DiagnosticRecord> = lexed
            .diagnostics()
            .iter()
            .map(|d| DiagnosticRecord {
                id: d.id.to_string(),
                stage: Stage::Lexical,
                source: source.clone(),
                span: d.span,
                position: d.position,
                meaning: d.meaning.clone(),
                default_status: d.default_status.clone(),
                specificity_rank: d.specificity_rank,
                event: None,
                cause: format!("{:?}", d.cause),
                detail: d.detail.clone(),
                sequence: None,
                primary: false,
            })
            .collect();
        if !records.is_empty() {
            return records;
        }
        if let Ok(parsed) = Parser::new(&self.grammar).parse(&lexed) {
            records.extend(parsed.diagnostics().iter().map(|d| DiagnosticRecord {
                id: d.id.to_string(),
                stage: Stage::GrammarOrSchema,
                source: source.clone(),
                span: d.span,
                position: d.position,
                meaning: d.meaning.clone(),
                default_status: d.default_status.clone(),
                specificity_rank: d.specificity_rank,
                event: None,
                cause: format!("{:?}", d.cause),
                detail: d.detail.clone(),
                sequence: None,
                primary: false,
            }));
        }
        records
    }
}

/// Steps 1 and 3 for one unit the resolver did load.
fn unit_diagnostics(unit: &ResolvedUnit) -> Vec<DiagnosticRecord> {
    let source = unit.id().to_string();
    let mut records: Vec<DiagnosticRecord> = unit
        .lexed()
        .diagnostics()
        .iter()
        .map(|d| DiagnosticRecord {
            id: d.id.to_string(),
            stage: Stage::Lexical,
            source: source.clone(),
            span: d.span,
            position: d.position,
            meaning: d.meaning.clone(),
            default_status: d.default_status.clone(),
            specificity_rank: d.specificity_rank,
            event: None,
            cause: format!("{:?}", d.cause),
            detail: d.detail.clone(),
            sequence: None,
            primary: false,
        })
        .collect();
    if let Some(parsed) = unit.parsed() {
        records.extend(parsed.diagnostics().iter().map(|d| DiagnosticRecord {
            id: d.id.to_string(),
            stage: Stage::GrammarOrSchema,
            source: source.clone(),
            span: d.span,
            position: d.position,
            meaning: d.meaning.clone(),
            default_status: d.default_status.clone(),
            specificity_rank: d.specificity_rank,
            event: None,
            cause: format!("{:?}", d.cause),
            detail: d.detail.clone(),
            sequence: None,
            primary: false,
        }));
    }
    records
}

/// Mark the first diagnostic primary.
///
/// Correct for stages 1 to 5 and for preflight, where nothing is handled and
/// every layer has already applied supersession, duplicate suppression and
/// `stable_order`. Execution and completion do not use this: the runtime
/// excludes recovered and substituted diagnostics, and its answer is adopted.
fn mark_primary(diagnostics: &mut [DiagnosticRecord]) {
    if let Some(first) = diagnostics.first_mut() {
        first.primary = true;
    }
}

/// A record for a failure that carries an identifier but no locus.
fn placeholder(id: &str, stage: Stage, source: &SourceId) -> DiagnosticRecord {
    DiagnosticRecord {
        id: id.to_string(),
        stage,
        source: source.to_string(),
        span: Span::empty(0),
        position: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        meaning: String::new(),
        default_status: String::new(),
        specificity_rank: 0,
        event: None,
        cause: String::new(),
        detail: None,
        sequence: None,
        primary: false,
    }
}

fn execution_diagnostics(execution: &Execution) -> Vec<DiagnosticRecord> {
    execution
        .diagnostics()
        .iter()
        .map(|d| DiagnosticRecord {
            id: d.id.to_string(),
            stage: d.stage(),
            source: d.source.to_string(),
            span: d.span,
            position: d.position,
            meaning: d.meaning.clone(),
            default_status: d.default_status.clone(),
            specificity_rank: d.specificity_rank,
            event: d.event.clone(),
            cause: format!("{:?}", d.cause),
            detail: d.detail.clone(),
            sequence: Some(d.sequence),
            primary: false,
        })
        .collect()
}

fn execution_record(execution: &Execution) -> ExecutionRecord {
    ExecutionRecord {
        invocations: execution
            .invocations()
            .iter()
            .map(|record| InvocationRecord {
                node: record.id.node,
                iteration: record.id.iteration.to_string(),
                attempt: record.id.attempt,
                block: record.block.clone(),
                declaration: record.declaration.clone(),
                status: record.status().to_string(),
                result_schema: record.result.as_ref().map(|r| r.schema.clone()),
                result_status: record.result.as_ref().map(|r| r.status.clone()),
                effects: record
                    .result
                    .as_ref()
                    .map(|r| {
                        r.observed_effects
                            .iter()
                            .map(|e| format!("{e}"))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        events: execution
            .events()
            .iter()
            .map(|event| EventRecord {
                event: event.event.clone(),
                diagnostic: event.diagnostic,
                occurrence: event.occurrence,
                producer: event.producer.to_string(),
                disposition: format!("{:?}", event.disposition),
            })
            .collect(),
        steps: execution.steps(),
    }
}

fn completion_record(completion: &Completion) -> CompletionRecord {
    let mut checks: Vec<CheckRecord> = completion
        .checks()
        .results()
        .iter()
        .map(|c| CheckRecord {
            id: c.id.clone(),
            kind: c.kind.block().to_string(),
            source: c.source.to_string(),
            span: c.span,
            required: c.required,
            selection: format!("{:?}", c.selection),
            outcome: Some(c.outcome.to_string()),
            skipped: None,
            evidence: c.evidence.clone(),
        })
        .collect();
    checks.extend(completion.checks().skipped().iter().map(|s| CheckRecord {
        id: s.id.clone(),
        kind: s.kind.block().to_string(),
        source: s.source.to_string(),
        span: s.span,
        required: false,
        selection: String::new(),
        outcome: None,
        skipped: Some(match &s.reason {
            SkipReason::WhenFalse => "when_false".to_string(),
            SkipReason::WhenFaulted(detail) => format!("when_faulted: {detail}"),
        }),
        evidence: Vec::new(),
    }));

    let verdict = completion.verdict();
    CompletionRecord {
        checks,
        evidence: completion
            .evidence()
            .records()
            .iter()
            .map(|e| {
                let (provision, detail) = match &e.provision {
                    lcl_completion::Provision::Value(value) => {
                        ("value".to_string(), value.to_string())
                    }
                    lcl_completion::Provision::Source(source) => {
                        ("source".to_string(), source.clone())
                    }
                    lcl_completion::Provision::Unresolved(why) => {
                        ("unresolved".to_string(), why.clone())
                    }
                };
                EvidenceRecord {
                    id: e.id.clone(),
                    source: e.source.to_string(),
                    span: e.span,
                    declared_type: e.declared_type.clone(),
                    required: e.required,
                    provision,
                    detail,
                    checksum: e.checksum.clone(),
                    provenance: e.provenance.clone(),
                    referenced_by: e.referenced_by.clone(),
                    satisfied: e.satisfied(),
                }
            })
            .collect(),
        verdict: VerdictRecord {
            success: verdict.success.as_ref().map(|s| s.id.clone()),
            quantifier: verdict
                .success
                .as_ref()
                .map(|s| s.quantifier.field().to_string()),
            success_value: verdict.success.as_ref().map(|s| s.value.to_string()),
            members: verdict
                .success
                .as_ref()
                .map(|s| {
                    s.members
                        .iter()
                        .map(|(id, value)| (id.clone(), value.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            failure: verdict.failure.as_ref().map(|f| f.id.clone()),
            failure_status: verdict.failure.as_ref().map(|f| f.requested_status.clone()),
            failure_classification: verdict
                .failure
                .as_ref()
                .and_then(|f| f.classification.clone()),
        },
        outputs: completion
            .outputs()
            .records()
            .iter()
            .map(|o| {
                let (publication, value, instances) = match &o.publication {
                    Publication::Published(value) => ("published", Some(value.to_string()), None),
                    Publication::Unbound => ("unbound", None, None),
                    Publication::Ambiguous(n) => ("ambiguous", None, Some(*n)),
                };
                OutputRecord {
                    id: o.id.clone(),
                    declared_type: o.declared_type.clone(),
                    required: o.required,
                    publication: publication.to_string(),
                    value,
                    instances,
                }
            })
            .collect(),
        terminal_status: completion.terminal_status().to_string(),
        reason: format!("{:?}", completion.terminal().reason),
    }
}

fn structure(resolved: &Resolved, planned: &Planned) -> StructureRecord {
    let plan = planned.partial_plan();
    let order: std::collections::BTreeMap<usize, usize> = plan
        .order()
        .iter()
        .enumerate()
        .map(|(position, node)| (*node, position))
        .collect();
    StructureRecord {
        imports: resolved
            .imports()
            .iter()
            .map(|i| ImportRecord {
                kind: i.kind.block().to_string(),
                origin: i.origin.to_string(),
                id: i.id.qualified(),
                namespace: i.namespace.clone(),
                reference: i.reference.as_ref().map(|r| r.to_string()),
                version: i.version.clone(),
                checksum: i.checksum.clone(),
                outcome: match &i.outcome {
                    lcl_resolver::ImportOutcome::Loaded(_) => "loaded",
                    lcl_resolver::ImportOutcome::NotFound => "not_found",
                    lcl_resolver::ImportOutcome::ChecksumMismatch => "checksum_mismatch",
                    lcl_resolver::ImportOutcome::Cycle => "cycle",
                    lcl_resolver::ImportOutcome::NotRequested => "not_requested",
                }
                .to_string(),
                loaded: i.loaded().map(|id| id.to_string()),
            })
            .collect(),
        declarations: resolved
            .declarations()
            .all()
            .iter()
            .map(|d| (d.id.qualified(), d.block.clone()))
            .collect(),
        candidates: resolved.graph().len(),
        plan: plan
            .nodes()
            .iter()
            .enumerate()
            .map(|(index, node)| PlanRecord {
                index,
                block: node.block.clone(),
                id: node.id.clone(),
                source: node.source.to_string(),
                span: node.span,
                parent: node.parent,
                required: node.required,
                operation: node.authorization.as_ref().map(|a| a.operation.clone()),
                order: order.get(&index).copied(),
            })
            .collect(),
        unused_inputs: planned.unused_invocation_data().to_vec(),
    }
}

/// True when an expression contains a `REF` anywhere inside it.
///
/// Used to refuse a supplied datum that would read the document it is being
/// supplied to. The walk is total over the expression grammar: every variant is
/// named, so a future expression form cannot slip past by being unhandled.
fn contains_reference(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) | Expr::Identifier(_) => false,
        Expr::Call(call) => call.is_reference() || call.arguments.iter().any(contains_reference),
        Expr::Collection(collection) => collection.members.iter().any(contains_reference),
        Expr::Group(group) => contains_reference(&group.inner),
        Expr::Unary(unary) => contains_reference(&unary.operand),
        Expr::Binary(binary) => {
            contains_reference(&binary.left) || contains_reference(&binary.right)
        }
        Expr::Property(property) => contains_reference(&property.base),
        Expr::Index(index) => contains_reference(&index.base) || contains_reference(&index.index),
        // `TYPE_EXPRESSION` admits a `REFERENCE_CALL` for a custom type. A
        // supplied datum is a value, not a type, so a type expression is
        // refused by the evaluation that follows rather than treated as a
        // reference here.
        Expr::Type(_) => false,
    }
}
