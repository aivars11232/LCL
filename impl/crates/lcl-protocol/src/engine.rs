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
//! [`Engine::request`] takes a `&mut dyn Host` and an operation dispatcher. It
//! installs no adapter and grants no permission, because deciding what a run
//! may touch is the embedder's decision, not the engine's. An engine handed a
//! host with nothing installed runs a document that needs nothing and reports
//! `error.host.constraint` for one that does — which is the truthful outcome,
//! not a degraded one.

use crate::inputs::{Inputs, Supplied};
use crate::record::{
    CheckRecord, Command, CompletionRecord, DeclarationRecord, DiagnosticRecord, EventRecord,
    EvidenceRecord, ExecutionRecord, ImportRecord, InputRecord, InvocationRecord, LocaleRecord,
    NavigationRecord, Outcome, OutputRecord, PlanRecord, Reached, ReferenceRecord, Report,
    SourceRecord, SpecRecord, StructureRecord, VerdictRecord,
};
use lcl_checker::{Checked, Checker, Contracts as StaticContracts};
use lcl_completion::{Completion, Contracts as CompletionContracts, Publication, SkipReason};
use lcl_diagnostics::Stage;
use lcl_lexer::{Lexicon, Position, Span, TokenKind};
use lcl_localization::{
    localization_decides, read_profile_file, Contract, CoverageDetector, LocaleDetector,
    LocaleProfileResolver, LocaleTag, Localization, MemoryResolver, Pin, LANGUAGE_VERSION,
    MAX_PROFILE_FILES,
};
use lcl_parser::syntax::Expr;
use lcl_parser::{Grammar, Parser};
use lcl_resolver::{
    declared_lcl_version, BindingTarget, LocalizationSetup, Resolved, ResolvedUnit, Resolver,
    Rules, SourceId, SourceProvider, SourceUnit,
};
use lcl_runtime::{Contracts as RuntimeContracts, Execution, Host, Operations, Runtime};
use lcl_semantics::{
    Contracts as PreflightContracts, Invocation, Outcome as PreflightOutcome, Planned, Preflight,
};
use lcl_spec::anchor::APPROVED_PACKAGE_0_2_0;
use lcl_spec::{SpecError, SpecPackage};
use lcl_stdlib::{Stdlib, StdlibError};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    localization: Option<EngineLocalization>,
}

/// The Core 0.2.0 localization stage an engine applies to every unit
/// (`02_LEXICAL/13_LOCALIZED_SOURCE_AND_LOCALE_DIRECTIVE.txt`).
struct EngineLocalization {
    contract: Contract,
    resolver: Arc<dyn LocaleProfileResolver + Send + Sync>,
    detector: Arc<dyn LocaleDetector + Send + Sync>,
    pins: BTreeMap<SourceId, Pin>,
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
            localization: None,
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

    /// Open the approved Core 0.2.0 package at `root` with its localization
    /// stage, over the locale profiles in `profile_files`.
    ///
    /// The package is opened against the Core 0.2.0 trust anchor. Each profile
    /// file is named `<locale>.json`, and a later file for the same locale
    /// replaces an earlier one. Detection is the package's coverage detector.
    /// Each file is read bounded, and more than [`MAX_PROFILE_FILES`] files is
    /// refused as a host limit.
    /// This is the one way a tool builds a localized engine from files.
    pub fn open_localized(
        root: impl AsRef<Path>,
        profile_files: &[PathBuf],
    ) -> Result<Engine, EngineError> {
        let spec = SpecPackage::open_with_anchor(root, &APPROVED_PACKAGE_0_2_0)
            .map_err(EngineError::Package)?;
        if profile_files.len() > MAX_PROFILE_FILES {
            return Err(EngineError::Contracts {
                layer: "localization",
                detail: format!(
                    "{} locale profile files exceed the host limit of {MAX_PROFILE_FILES}",
                    profile_files.len()
                ),
            });
        }
        let mut profiles = MemoryResolver::new("lcl.profile.files");
        for file in profile_files {
            let refuse = |detail: String| EngineError::Contracts {
                layer: "localization",
                detail: format!("{}: {detail}", file.display()),
            };
            let locale = file
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| LocaleTag::parse(stem).ok())
                .ok_or_else(|| {
                    refuse("a locale profile file is named <locale>.json".to_string())
                })?;
            let bytes = read_profile_file(file).map_err(|e| refuse(e.to_string()))?;
            profiles.insert(locale, bytes);
        }
        Engine::assemble(spec)?.with_localization(Arc::new(profiles), Arc::new(CoverageDetector))
    }

    /// Apply the Core 0.2.0 localization stage to every unit this engine
    /// resolves. The package must carry the localization contract, which no
    /// Core 0.1.0 package does.
    pub fn with_localization(
        mut self,
        resolver: Arc<dyn LocaleProfileResolver + Send + Sync>,
        detector: Arc<dyn LocaleDetector + Send + Sync>,
    ) -> Result<Engine, EngineError> {
        let contract = Contract::load(&self.spec).map_err(|e| EngineError::Contracts {
            layer: "localization",
            detail: e.to_string(),
        })?;
        self.localization = Some(EngineLocalization {
            contract,
            resolver,
            detector,
            pins: BTreeMap::new(),
        });
        Ok(self)
    }

    /// Pin the locale selection of units, from a project lock. A pinned unit
    /// must reproduce its recorded locale and profile identity exactly, or it
    /// fails with `error.localization.profile_drift`. An engine without the
    /// localization stage has nothing to pin and is returned unchanged.
    pub fn with_locale_pins(mut self, pins: BTreeMap<SourceId, Pin>) -> Engine {
        if let Some(localization) = &mut self.localization {
            localization.pins = pins;
        }
        self
    }

    /// The localization contract, when this engine applies the localization
    /// stage.
    pub fn localization_contract(&self) -> Option<&Contract> {
        self.localization.as_ref().map(|l| &l.contract)
    }

    /// Localize (when this engine applies the localization stage), lex and
    /// parse one unit exactly as resolution does, without resolving it.
    pub fn stage(&self, unit: &SourceUnit) -> ResolvedUnit {
        self.resolver().stage(unit)
    }

    /// A resolver over this engine's layers, with its localization stage.
    fn resolver(&self) -> Resolver<'_> {
        let resolver = Resolver::new(&self.rules, &self.grammar, &self.lexicon);
        match &self.localization {
            Some(l) => resolver.with_localization(LocalizationSetup {
                contract: &l.contract,
                resolver: l.resolver.as_ref(),
                detector: l.detector.as_ref(),
                pins: &l.pins,
            }),
            None => resolver,
        }
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

    /// Steps 1 to 13, against an explicit operation dispatcher and host.
    ///
    /// [`Engine::run`] is this with the dispatcher fixed to a [`Stdlib`], which
    /// is what almost every caller wants. This exists for a caller that needs
    /// to observe the dispatch seam — a debugger that pauses before an
    /// operation, a recorder, a test double — and it mirrors
    /// [`lcl_runtime::Runtime::execute_with`], which has always taken the
    /// dispatcher as a parameter for the same reason.
    ///
    /// Without it, such a caller would have to reassemble the thirteen-step
    /// walk itself, which is the exact duplication this crate exists to
    /// prevent. The dispatcher still decides what an operation means; wrapping
    /// one does not move that decision.
    pub fn run_with(
        &self,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
        operations: &mut dyn Operations,
        host: &mut dyn Host,
    ) -> Report {
        self.request(
            Command::Run,
            unit,
            provider,
            inputs,
            Some((operations, host)),
        )
    }

    /// The one staged walk every command uses.
    fn request(
        &self,
        command: Command,
        unit: &SourceUnit,
        provider: &dyn SourceProvider,
        inputs: &Inputs,
        effects: Option<(&mut dyn Operations, &mut dyn Host)>,
    ) -> Report {
        let mut report = Report {
            command,
            spec: self.record.clone(),
            units: vec![SourceRecord::new(
                unit.id().to_string(),
                unit.digest(),
                unit.bytes().len(),
                true,
            )],
            reached: Reached::Lexical,
            outcome: Outcome::Rejected,
            inputs: Vec::new(),
            diagnostics: Vec::new(),
            structure: None,
            navigation: None,
            execution: None,
            completion: None,
        };

        // Steps 1 to 4. The resolver drives the lexer and parser itself, so a
        // failure of either arrives here as a skipped stage rather than as a
        // resolution result.
        let resolved = match self.resolver().resolve(unit, provider) {
            Ok(resolved) => resolved,
            Err(skipped) => {
                // The root did not survive step 1 or 3. Its full diagnostic
                // list is not in a `Resolved` that was never built, so the two
                // stages are re-run over the same bytes to report all of them.
                // Same lexer, same parser, same input: a second pass observes,
                // it does not decide.
                report.reached = match skipped.stage {
                    Stage::Localization | Stage::Lexical => Reached::Lexical,
                    _ => Reached::Grammar,
                };
                let staged = self.stage(unit);
                report.diagnostics = unit_diagnostics(&staged, self.localization.as_ref());
                if let (Some(record), Some(locale)) = (
                    report.units.first_mut(),
                    staged.localization().and_then(locale_record),
                ) {
                    record.locale = Some(locale);
                }
                mark_primary(&mut report.diagnostics);
                return report;
            }
        };

        // Every loaded unit, root first, in the resolver's load order.
        report.units = resolved
            .units()
            .map(|u| {
                let record = SourceRecord::new(
                    u.id().to_string(),
                    u.digest(),
                    u.source().len(),
                    u.id() == resolved.root(),
                );
                match u.localization().and_then(locale_record) {
                    Some(locale) => record.with_locale(locale),
                    None => record,
                }
            })
            .collect();

        // Steps 1 to 3 for every loaded unit, including imported ones that
        // failed an earlier stage while the root did not.
        let early: Vec<DiagnosticRecord> = resolved
            .units()
            .flat_map(|u| unit_diagnostics(u, self.localization.as_ref()))
            .collect();
        if !early.is_empty() {
            report.reached = early
                .iter()
                .map(|d| match d.stage {
                    Stage::Localization | Stage::Lexical => Reached::Lexical,
                    _ => Reached::Grammar,
                })
                .min()
                .unwrap_or(Reached::Lexical);
            report.diagnostics = early;
            mark_primary(&mut report.diagnostics);
            return report;
        }

        report.reached = Reached::Resolution;

        // Navigation is the resolver's own output, so it is available as soon
        // as the resolver ran — including when it rejected the document. An
        // editor needs to jump to a definition most while a document is broken,
        // and withholding bindings that step 4 already made would push it into
        // searching for identifiers itself, which is the one thing a UI must
        // never do. An unresolved occurrence is reported as unresolved.
        if command == Command::Inspect {
            report.navigation = Some(navigation(&resolved));
        }

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
        let Some((operations, host)) = effects else {
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
            .execute_with(&planned, &checked, &resolved, operations, host)
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
}

/// The Core 0.1.0 engine and, optionally, the Core 0.2.0 engine with its
/// localization stage, with the one rule that chooses between them.
///
/// A document is judged by exactly one engine. `07_VERSIONING_AND_EXTENSIONS/05`
/// keeps every document that does not declare 0.2.0 under its Core 0.1.0
/// reading, and owner decision D9 (2026-09-15) sends a document whose
/// localization fails to the 0.2.0 engine when that failure is the result.
pub struct Engines {
    core: Engine,
    localized: Option<Engine>,
}

impl Engines {
    /// `core` must not apply the localization stage; `localized`, when given,
    /// must.
    pub fn new(core: Engine, localized: Option<Engine>) -> Result<Engines, EngineError> {
        let refuse = |detail: &str| EngineError::Contracts {
            layer: "dispatch",
            detail: detail.to_string(),
        };
        if core.localization_contract().is_some() {
            return Err(refuse(
                "the core engine must not apply the localization stage",
            ));
        }
        if localized
            .as_ref()
            .is_some_and(|engine| engine.localization_contract().is_none())
        {
            return Err(refuse(
                "the localized engine must apply the localization stage",
            ));
        }
        Ok(Engines { core, localized })
    }

    pub fn core(&self) -> &Engine {
        &self.core
    }

    pub fn localized(&self) -> Option<&Engine> {
        self.localized.as_ref()
    }

    /// The engine that judges `unit`.
    ///
    /// * When the Core 0.1.0 reading lexes, the declared `LCL` `VERSION`
    ///   decides: exactly `0.2.0` is the 0.2.0 engine's, anything else stays
    ///   with Core 0.1.0.
    /// * Otherwise the localized reading decides. A rejected localization is
    ///   the 0.2.0 engine's exactly when it decides the result (D9). An
    ///   accepted reading is the 0.2.0 engine's only when it declares exactly
    ///   `0.2.0`; a selected profile never establishes that authority.
    ///   A reading the lexical stage rejected declares the `VERSION` of a
    ///   header line that precedes its first lexical defect.
    pub fn engine_for(&self, unit: &SourceUnit) -> &Engine {
        let Some(localized) = &self.localized else {
            return &self.core;
        };
        let declared = |staged: &ResolvedUnit| {
            staged
                .document()
                .and_then(declared_lcl_version)
                .or_else(|| lexed_lcl_version(staged))
        };
        let canonical = self.core.stage(unit);
        if canonical.lexed().primary().is_none() {
            return if declared(&canonical).as_deref() == Some(LANGUAGE_VERSION) {
                localized
            } else {
                &self.core
            };
        }
        let staged = localized.stage(unit);
        let Some(outcome) = staged.localization() else {
            return &self.core;
        };
        if !outcome.is_accepted() {
            return if localization_decides(outcome, unit.bytes()) {
                localized
            } else {
                &self.core
            };
        }
        // `02_LEXICAL/13`: localization applies only to a document that
        // declares VERSION "0.2.0". A selected profile is not a declaration.
        if declared(&staged).as_deref() == Some(LANGUAGE_VERSION) {
            localized
        } else {
            &self.core
        }
    }
}

/// The declared `VERSION` of a unit the lexical stage rejected.
///
/// `DOCUMENT` begins `{ BLANK_LINE }, LCL_HEADER`, and `LCL_HEADER` is the
/// token line `LCL : NEWLINE INDENT VERSION : SPACE STRING NEWLINE`. The
/// declaration is readable only when that whole line precedes the unit's first
/// lexical defect: no later stage repairs an earlier invalid stage by guessing
/// intent (`01_FOUNDATION/03`), so nothing at or after a defect is read.
fn lexed_lcl_version(unit: &ResolvedUnit) -> Option<String> {
    if unit.parsed().is_some() {
        return None;
    }
    let lexed = unit.lexed();
    let boundary = lexed.diagnostics().iter().map(|d| d.span.start).min()?;
    let mut tokens = lexed
        .tokens()
        .iter()
        .skip_while(|t| t.kind == TokenKind::BlankLine);
    let mut version = None;
    for (kind, word) in [
        (TokenKind::ReservedWord, "LCL"),
        (TokenKind::Symbol, ":"),
        (TokenKind::Newline, ""),
        (TokenKind::Indent, ""),
        (TokenKind::ReservedWord, "VERSION"),
        (TokenKind::Symbol, ":"),
        (TokenKind::Space, ""),
        (TokenKind::String, ""),
        (TokenKind::Newline, ""),
    ] {
        let token = tokens
            .next()
            .filter(|t| t.kind == kind && t.span.end <= boundary)?;
        let spelled = token
            .canonical
            .as_deref()
            .or_else(|| lexed.source().get(token.span.start..token.span.end))?;
        match kind {
            TokenKind::ReservedWord | TokenKind::Symbol if spelled != word => return None,
            TokenKind::String => version = token.value.clone(),
            _ => {}
        }
    }
    version
}

/// Steps 1 and 3 for one unit the resolver did load.
fn unit_diagnostics(
    unit: &ResolvedUnit,
    localization: Option<&EngineLocalization>,
) -> Vec<DiagnosticRecord> {
    let source = unit.id().to_string();
    // A unit the localization stage rejected was not lexed under any profile;
    // its localization diagnostics are its whole early-stage result.
    if let (Some(outcome), Some(engine)) = (unit.localization(), localization) {
        if !outcome.diagnostics.is_empty() {
            return outcome
                .diagnostics
                .iter()
                .map(|d| {
                    let metadata = engine.contract.error_metadata(d.id);
                    DiagnosticRecord {
                        id: d.id.to_string(),
                        stage: Stage::Localization,
                        source: source.clone(),
                        span: Span {
                            start: d.offset,
                            end: d.end,
                        },
                        position: position(unit.source(), d.offset),
                        meaning: metadata.map(|m| m.meaning.clone()).unwrap_or_default(),
                        default_status: metadata
                            .map(|m| m.default_status.clone())
                            .unwrap_or_default(),
                        specificity_rank: metadata.map_or(0, |m| m.specificity_rank),
                        event: None,
                        cause: "localization".to_string(),
                        detail: Some(d.detail.clone()),
                        sequence: None,
                        primary: false,
                    }
                })
                .collect();
        }
    }
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

/// The protocol record of a unit's locale selection, when one was made.
fn locale_record(localization: &Localization) -> Option<LocaleRecord> {
    let record = localization.record.as_ref()?;
    Some(LocaleRecord {
        method: record.method.as_str().to_string(),
        locale: record.locale.as_ref().map(|l| l.as_str().to_string()),
        profile_identity: record.profile_identity.clone(),
        detector_identity: record.detector_identity.clone(),
        candidate_locales: record
            .candidate_locales
            .iter()
            .map(|l| l.as_str().to_string())
            .collect(),
        lcl_version: record.lcl_version.to_string(),
    })
}

/// Line and column of a byte offset, derived as the lexer derives them.
fn position(text: &str, offset: usize) -> Position {
    let line_start = text
        .get(..offset)
        .and_then(|s| s.rfind('\n'))
        .map_or(0, |i| i + 1);
    let line = text
        .get(..line_start)
        .map_or(0, |s| s.matches('\n').count())
        + 1;
    let column = text
        .get(line_start..offset)
        .map_or(0, |s| s.chars().count())
        + 1;
    Position {
        offset,
        line: u32::try_from(line).unwrap_or(u32::MAX),
        column: u32::try_from(column).unwrap_or(u32::MAX),
    }
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

/// Project what the resolver bound into the navigation record.
///
/// Every field is copied. This function searches nothing, matches no text and
/// resolves no name: step 4 already did all of it, and `lcl-protocol`'s whole
/// contract is that it "resolves nothing, checks nothing and classifies
/// nothing". An editor that jumped to a definition by grepping for an
/// identifier would be a second resolver living in a UI, which is exactly what
/// this exists to prevent.
fn navigation(resolved: &Resolved) -> NavigationRecord {
    let index = resolved.declarations();
    let position_in = |source: &SourceId, offset: usize| match resolved.unit(source) {
        Some(unit) => lcl_runtime::position_of(unit.source(), offset),
        // A binding names the unit it was found in, so its unit is loaded.
        // Reported rather than unwrapped: a panic in a product facade would
        // destroy the record a caller needs.
        None => Position {
            offset,
            line: 0,
            column: 0,
        },
    };

    let declarations = index
        .all()
        .iter()
        .enumerate()
        .map(|(i, d)| DeclarationRecord {
            index: i,
            id: d.id.qualified(),
            block: d.block.clone(),
            definition_kind: d.definition_kind.clone(),
            source: d.source.to_string(),
            id_span: d.id_span,
            id_position: position_in(&d.source, d.id_span.start),
            block_span: d.block_span,
            parent: d.parent,
        })
        .collect();

    let references = resolved
        .bindings()
        .iter()
        .map(|b| ReferenceRecord {
            source: b.source.to_string(),
            span: b.span,
            position: position_in(&b.source, b.span.start),
            text: b.text.clone(),
            slot: b.slot.clone(),
            target: match b.target {
                BindingTarget::Declaration(_) => "declaration",
                BindingTarget::LoopLocal { .. } => "loop_local",
                BindingTarget::Unresolved => "unresolved",
            }
            .to_string(),
            declaration: match b.target {
                BindingTarget::Declaration(i) => Some(i),
                _ => None,
            },
            binding_span: match b.target {
                BindingTarget::LoopLocal { binding_span } => Some(binding_span),
                _ => None,
            },
            resolved_id: b.resolved_id.as_ref().map(|id| id.qualified()),
        })
        .collect();

    NavigationRecord {
        declarations,
        references,
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
