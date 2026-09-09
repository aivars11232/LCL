//! Step 11: run post-execution `VERIFY` and `TEST` against what was observed.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/check_selection_contract` and
//! `05_SEMANTICS/10`.
//!
//! ## The three questions, in order
//!
//! 1. **Is it selected?** `selection`: "Select VALIDATE, VERIFY, and FAILURE
//!    declarations in the EXECUTE root source document. A targetless clause
//!    applies to that invocation." For a post-execution check the targeted case
//!    is narrower than preflight's, and deliberately so: "Targeted
//!    post-execution VERIFY applies only to an actually activated producer or
//!    an observed target of that invocation, not an unselected IF branch merely
//!    present in the candidate graph." So this module asks
//!    [`crate::Observation`], never the plan.
//! 2. **Is it applicable?** `demand`: "Where WHEN exists, absence means TRUE
//!    and FALSE skips applicability. A skipped check has no result."
//! 3. **What does it assert?** Demanded through M6's evaluator, over the
//!    bindings the execution actually produced.
//!
//! `REQUIRED` enters only at the end, and means one narrow thing: "REQUIRED
//! controls whether a FALSE result blocks, not whether a selected check runs."
//! An optional check still runs and still records its Boolean outcome; it
//! simply raises no `error.verification.failed` when that outcome is FALSE.
//!
//! ## Why a skipped check has no record
//!
//! [`CheckOutcome`] carries a `Value`, not an `Option<Value>`, because a
//! skipped check is not an outcome of any kind. It goes in [`Checks::skipped`]
//! instead, and — critically — is never bound into the evaluator's scope. A
//! later read of it therefore yields `MISSING` through the ordinary lookup
//! path, satisfying "never implicit TRUE" structurally rather than by a rule
//! someone has to remember.
//!
//! ## `TEST` is not ambient
//!
//! "TEST declarations are activated only as an explicit TEST root or operation
//! target, never as ambient tasks." A document holding ten `TEST` declarations
//! and executing a `TASK` runs none of them. Only the one a `TEST` root names
//! runs here; the operation-target case is `core.test`'s, and already belongs
//! to the standard library.

use crate::contracts::Contracts;
use crate::diagnostic::CompletionError;
use crate::engine::{Emission, Engine};
use crate::syntax;
use lcl_lexer::Span;
use lcl_resolver::SourceId;
use lcl_runtime::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Which post-execution form a check is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckKind {
    Verify,
    Test,
}

impl CheckKind {
    pub fn block(self) -> &'static str {
        match self {
            CheckKind::Verify => "VERIFY",
            CheckKind::Test => "TEST",
        }
    }
}

impl std::fmt::Display for CheckKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.block())
    }
}

/// Why a post-execution check is in the selected set.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Selection {
    /// "A targetless clause applies to that invocation."
    Targetless,
    /// The resolved `TARGET` names a producer this invocation actually
    /// activated.
    ActivatedProducer(String),
    /// The resolved `TARGET` names a target this invocation actually observed:
    /// an effect target or a bound output.
    ObservedTarget(String),
    /// The resolved `TARGET` is the exact material value an activated `ACTION`
    /// acted on.
    MaterialTarget(String),
    /// "Include check prerequisites explicitly referenced by selected checks or
    /// the root SUCCESS."
    Prerequisite,
    /// The `EXECUTE` root names this `TEST` directly.
    TestRoot,
}

/// Why a selected check produced no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// `WHEN` evaluated FALSE.
    WhenFalse,
    /// `WHEN` could not be demanded, and the fault was reported.
    WhenFaulted(String),
}

/// One selected post-execution check that produced a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    pub declaration: usize,
    pub id: String,
    pub kind: CheckKind,
    pub source: SourceId,
    pub span: Span,
    /// `REQUIRED` controls blocking, never whether a selected check runs.
    pub required: bool,
    pub selection: Selection,
    /// The check's Boolean domain outcome: TRUE, FALSE or UNKNOWN.
    pub outcome: Value,
    /// Declared `EVIDENCE` references, in declaration order.
    pub evidence: Vec<String>,
}

impl CheckOutcome {
    /// True exactly when the assertion held.
    pub fn held(&self) -> bool {
        matches!(self.outcome, Value::Boolean(true))
    }

    /// True when a required check failed, which prevents success.
    pub fn blocks(&self) -> bool {
        self.required && !self.held()
    }

    pub fn serialize(&self) -> String {
        format!(
            "{} {} required={} outcome={} selection={:?}",
            self.kind, self.id, self.required, self.outcome, self.selection
        )
    }
}

/// One selected check that was skipped and therefore has no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedCheck {
    pub declaration: usize,
    pub id: String,
    pub kind: CheckKind,
    pub source: SourceId,
    pub span: Span,
    pub reason: SkipReason,
}

/// The post-execution check results of one invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Checks {
    results: Vec<CheckOutcome>,
    skipped: Vec<SkippedCheck>,
}

impl Checks {
    /// Every check that produced a result, in evaluation order.
    pub fn results(&self) -> &[CheckOutcome] {
        &self.results
    }

    /// Every selected check that was skipped, in selection order.
    ///
    /// These have no result. They are listed so a report can say a check was
    /// selected and inapplicable, which is a different fact from a check that
    /// was never selected.
    pub fn skipped(&self) -> &[SkippedCheck] {
        &self.skipped
    }

    pub fn result(&self, id: &str) -> Option<&CheckOutcome> {
        self.results.iter().find(|c| c.id == id)
    }

    /// Every required check that failed.
    pub fn blocking(&self) -> impl Iterator<Item = &CheckOutcome> {
        self.results.iter().filter(|c| c.blocks())
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty() && self.skipped.is_empty()
    }

    pub fn serialize(&self) -> String {
        let mut out = String::from("CHECKS\n");
        for result in &self.results {
            out.push_str(&format!("  {}\n", result.serialize()));
        }
        for skipped in &self.skipped {
            out.push_str(&format!(
                "  {} {} skipped ({:?})\n",
                skipped.kind, skipped.id, skipped.reason
            ));
        }
        out
    }
}

/// One selected check, before evaluation.
struct Selected {
    declaration: usize,
    id: String,
    kind: CheckKind,
    source: SourceId,
    span: Span,
    required: bool,
    selection: Selection,
}

/// Select and run every applicable post-execution check.
pub(crate) fn run(engine: &mut Engine) -> Checks {
    let selected = select(engine);
    let Some(ordered) = order_by_prerequisites(engine, selected) else {
        // A prerequisite cycle was reported. Evaluating any member would be
        // evaluating a chain with no first element.
        return Checks::default();
    };
    evaluate(engine, ordered)
}

/// Every post-execution check this invocation selects.
fn select(engine: &Engine) -> Vec<Selected> {
    let root = engine.root_source();
    let prerequisites = prerequisite_ids(engine);
    let test_root = explicit_test_root(engine);
    let material_targets = activated_material_targets(engine);

    let mut out = Vec::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        let kind = match declaration.block.as_str() {
            "VERIFY" => CheckKind::Verify,
            "TEST" => CheckKind::Test,
            _ => continue,
        };
        let id = declaration.id.qualified();

        // "TEST declarations are activated only as an explicit TEST root or
        // operation target, never as ambient tasks."
        if kind == CheckKind::Test {
            if test_root.as_deref() != Some(id.as_str()) {
                continue;
            }
            out.push(Selected {
                declaration: index,
                id,
                kind,
                source: declaration.source.clone(),
                span: declaration.id_span,
                required: required_field(engine, index, "TEST"),
                selection: Selection::TestRoot,
            });
            continue;
        }

        // "Select ... declarations in the EXECUTE root source document. ...
        // Unrelated imported checks never run merely because their document was
        // imported."
        let in_root = declaration.source == root;
        let is_prerequisite = prerequisites.contains(&id);
        if !in_root && !is_prerequisite {
            continue;
        }

        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        let target = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body));

        let selection = match target {
            // "A targetless clause applies to that invocation."
            None => Selection::Targetless,
            Some(expr) => match syntax::reference_target(expr) {
                Some(named) => {
                    let named = named.to_string();
                    if engine.observation.activated(&named) {
                        Selection::ActivatedProducer(named)
                    } else if engine.observation.observed(&named) {
                        Selection::ObservedTarget(named)
                    } else if is_prerequisite {
                        Selection::Prerequisite
                    } else {
                        // "not an unselected IF branch merely present in the
                        // candidate graph." A target this invocation never
                        // reached selects nothing at all.
                        continue;
                    }
                }
                None => {
                    let rendered = lcl_runtime::syntax::render(expr);
                    if material_targets.contains(&rendered) {
                        Selection::MaterialTarget(rendered)
                    } else if is_prerequisite {
                        Selection::Prerequisite
                    } else {
                        continue;
                    }
                }
            },
        };

        // A check selected only because something references it is a
        // prerequisite, whatever its own target says.
        let selection = if is_prerequisite && !in_root {
            Selection::Prerequisite
        } else {
            selection
        };

        out.push(Selected {
            declaration: index,
            id,
            kind,
            source: declaration.source.clone(),
            span: declaration.id_span,
            required: required_field(engine, index, "VERIFY"),
            selection,
        });
    }
    out
}

/// The `REQUIRED` field of one check, defaulted from the field signature.
fn required_field(engine: &Engine, declaration: usize, block_name: &str) -> bool {
    let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration) else {
        return default_required(engine.contracts, block_name);
    };
    match lcl_runtime::syntax::field_text(&block, "REQUIRED").as_deref() {
        Some("TRUE") => true,
        Some("FALSE") => false,
        _ => default_required(engine.contracts, block_name),
    }
}

/// `REQUIRED` defaults, read from the registry rather than assumed.
fn default_required(contracts: &Contracts, block_name: &str) -> bool {
    contracts
        .runtime()
        .preflight()
        .field_default_boolean(block_name, "REQUIRED")
        .unwrap_or(true)
}

/// The `TEST` declaration the `EXECUTE` root names, if it names one.
fn explicit_test_root(engine: &Engine) -> Option<String> {
    let declaration = engine.root_declaration()?;
    let decl = engine.resolved.declarations().get(declaration)?;
    if decl.block != "TEST" {
        return None;
    }
    Some(decl.id.qualified())
}

/// The exact material targets activated actions acted on.
///
/// Only *activated* actions count. An action present in the graph but never
/// entered contributes no observed target, which is the post-execution half of
/// "never expand ambient resources".
fn activated_material_targets(engine: &Engine) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "ACTION" {
            continue;
        }
        if !engine.observation.activated(&declaration.id.qualified()) {
            continue;
        }
        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        if let Some(expr) = block
            .field("TARGET")
            .and_then(|f| syntax::inline_expr(&f.body))
        {
            if syntax::reference_target(expr).is_none() {
                out.insert(lcl_runtime::syntax::render(expr));
            }
        }
    }
    out
}

/// Check ids explicitly referenced by a selected check or the root `SUCCESS`.
///
/// "Include check prerequisites explicitly referenced by selected checks or the
/// root SUCCESS."
fn prerequisite_ids(engine: &Engine) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let root = engine.root_source();

    // The root SUCCESS, reached through the root declaration's SUCCESS field
    // rather than by scanning every SUCCESS in the document: an unreferenced
    // SUCCESS declaration selects nothing.
    if let Some(success) = crate::success::root_success_declaration(engine) {
        collect_reference_ids(engine, success, &mut out);
    }

    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        let interesting =
            declaration.source == root && matches!(declaration.block.as_str(), "VERIFY" | "TEST");
        if !interesting {
            continue;
        }
        collect_reference_ids(engine, index, &mut out);
    }
    out
}

/// Every declaration id one declaration's fields reference.
fn collect_reference_ids(engine: &Engine, declaration: usize, out: &mut BTreeSet<String>) {
    let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration) else {
        return;
    };
    for statement in block.statements() {
        let lcl_parser::syntax::Statement::Field(field) = statement else {
            continue;
        };
        for (id, _) in syntax::reference_list(&field.body) {
            out.insert(id);
        }
        if let Some(expr) = syntax::inline_expr(&field.body) {
            for id in syntax::referenced_ids(expr) {
                out.insert(id);
            }
        }
    }
}

/// Order selected checks so a prerequisite is evaluated before its dependent.
///
/// "Evaluate selected check-result prerequisites in stable topological order
/// with source-unit namespace then source declaration order as tie-breakers. A
/// prerequisite cycle uses error.reference.cycle."
///
/// `None` means a cycle was found and reported.
fn order_by_prerequisites(engine: &mut Engine, selected: Vec<Selected>) -> Option<Vec<Selected>> {
    let ids: BTreeSet<String> = selected.iter().map(|s| s.id.clone()).collect();

    let mut needs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for check in &selected {
        let mut required = BTreeSet::new();
        if let Some(block) =
            lcl_runtime::syntax::declaration_block(engine.resolved, check.declaration)
        {
            for name in ["ASSERT", "WHEN", "EXPECTED", "ACTUAL"] {
                let Some(field) = block.field(name) else {
                    continue;
                };
                for (id, _) in syntax::reference_list(&field.body) {
                    if ids.contains(&id) && id != check.id {
                        required.insert(id);
                    }
                }
                if let Some(expr) = syntax::inline_expr(&field.body) {
                    for id in syntax::referenced_ids(expr) {
                        if ids.contains(&id) && id != check.id {
                            required.insert(id);
                        }
                    }
                }
            }
        }
        needs.insert(check.id.clone(), required);
    }

    // The stable tie-break order: source unit, then source declaration order.
    let mut pending: Vec<&Selected> = selected.iter().collect();
    pending.sort_by(|a, b| {
        a.source
            .cmp(&b.source)
            .then(a.span.start.cmp(&b.span.start))
    });

    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = Vec::new();
    loop {
        if done.len() == pending.len() {
            break;
        }
        let mut progressed = false;
        for check in &pending {
            if done.contains(&check.id) {
                continue;
            }
            let ready = needs
                .get(&check.id)
                .map(|r| r.iter().all(|id| done.contains(id)))
                .unwrap_or(true);
            if ready {
                done.insert(check.id.clone());
                order.push(check.id.clone());
                progressed = true;
            }
        }
        if !progressed {
            let mut stuck: Vec<(SourceId, Span, String)> = pending
                .iter()
                .filter(|c| !done.contains(&c.id))
                .map(|c| (c.source.clone(), c.span, c.id.clone()))
                .collect();
            stuck.sort();
            let phase = engine.observed_phase();
            for (source, span, id) in stuck {
                engine.emit(Emission {
                    id: CompletionError::ReferenceCycle,
                    source: &source,
                    span,
                    declaration: Some(id.clone()),
                    cause: id.clone(),
                    detail: format!(
                        "check `{id}` is in a prerequisite cycle, so no first check exists"
                    ),
                    phase,
                });
            }
            return None;
        }
    }

    let mut by_id: BTreeMap<String, Selected> =
        selected.into_iter().map(|s| (s.id.clone(), s)).collect();
    Some(
        order
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect(),
    )
}

/// Evaluate each selected check in order and record its outcome.
fn evaluate(engine: &mut Engine, selected: Vec<Selected>) -> Checks {
    let mut checks = Checks::default();
    let phase = engine.observed_phase();

    for check in selected {
        let Some(block) =
            lcl_runtime::syntax::declaration_block(engine.resolved, check.declaration)
        else {
            continue;
        };

        // "Where WHEN exists, absence means TRUE and FALSE skips
        // applicability." `TEST` has no WHEN: "No WHEN field is invented for
        // TEST."
        if check.kind == CheckKind::Verify {
            match applicability(engine, check.declaration) {
                Applicability::Applicable => {}
                Applicability::Skipped => {
                    checks.skipped.push(SkippedCheck {
                        declaration: check.declaration,
                        id: check.id.clone(),
                        kind: check.kind,
                        source: check.source.clone(),
                        span: check.span,
                        reason: SkipReason::WhenFalse,
                    });
                    continue;
                }
                Applicability::Faulted(id) => {
                    checks.skipped.push(SkippedCheck {
                        declaration: check.declaration,
                        id: check.id.clone(),
                        kind: check.kind,
                        source: check.source.clone(),
                        span: check.span,
                        reason: SkipReason::WhenFaulted(id),
                    });
                    continue;
                }
            }
        }

        let evidence: Vec<String> = block
            .field("EVIDENCE")
            .map(|f| {
                syntax::reference_list(&f.body)
                    .into_iter()
                    .map(|(id, _)| id)
                    .collect()
            })
            .unwrap_or_default();

        let outcome = match check.kind {
            CheckKind::Verify => assertion_outcome(engine, check.declaration, &check),
            CheckKind::Test => crate::test_root::outcome(engine, check.declaration, &check.id),
        };

        // Publish the result so a later check or the root SUCCESS can read it.
        engine.bind_check_result(&check.id, outcome.clone());

        let result = CheckOutcome {
            declaration: check.declaration,
            id: check.id.clone(),
            kind: check.kind,
            source: check.source.clone(),
            span: check.span,
            required: check.required,
            selection: check.selection.clone(),
            outcome,
            evidence,
        };

        // "A required post-execution FALSE VERIFY or TEST assertion uses
        // error.verification.failed. Optional FALSE checks retain their Boolean
        // domain outcome without emitting a required-check failure."
        if result.blocks() {
            engine.emit(Emission {
                id: CompletionError::VerificationFailed,
                source: &result.source,
                span: result.span,
                declaration: Some(result.id.clone()),
                cause: result.id.clone(),
                detail: format!(
                    "required {} `{}` asserted {}",
                    result.kind, result.id, result.outcome
                ),
                phase,
            });
        }
        checks.results.push(result);
    }
    checks
}

/// Whether a check's `WHEN` makes it applicable.
enum Applicability {
    Applicable,
    Skipped,
    /// The condition could not be demanded; carries the reported identifier.
    Faulted(String),
}

fn applicability(engine: &mut Engine, declaration: usize) -> Applicability {
    let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration) else {
        return Applicability::Applicable;
    };
    // "absence means TRUE"
    let Some(expr) = lcl_runtime::syntax::field_expr(&block, "WHEN") else {
        return Applicability::Applicable;
    };
    let expr = expr.clone();
    let span = expr.span();
    let source = engine
        .resolved
        .declarations()
        .get(declaration)
        .map(|d| d.source.clone())
        .unwrap_or_else(|| engine.root_source());
    let id = engine
        .resolved
        .declarations()
        .get(declaration)
        .map(|d| d.id.qualified());

    match engine.evaluator().demand(&expr) {
        Ok(Value::Boolean(true)) => Applicability::Applicable,
        Ok(Value::Boolean(false)) => Applicability::Skipped,
        // "Required demanded MISSING and UNKNOWN use error.required.missing and
        // error.value.unknown."
        Ok(Value::Missing) => {
            let phase = engine.observed_phase();
            engine.emit(Emission {
                id: CompletionError::RequiredMissing,
                source: &source,
                span,
                declaration: id,
                cause: "WHEN".to_string(),
                detail: "a check applicability condition demanded MISSING".to_string(),
                phase,
            });
            Applicability::Faulted(CompletionError::RequiredMissing.to_string())
        }
        Ok(Value::Unknown) => {
            let phase = engine.observed_phase();
            engine.emit(Emission {
                id: CompletionError::ValueUnknown,
                source: &source,
                span,
                declaration: id,
                cause: "WHEN".to_string(),
                detail: "a check applicability condition demanded UNKNOWN".to_string(),
                phase,
            });
            Applicability::Faulted(CompletionError::ValueUnknown.to_string())
        }
        Ok(_) | Err(_) => {
            // A non-Boolean or faulted condition is an M4/M6 concern that
            // already has its own diagnostic; treating it as applicable here
            // would invent an outcome for a condition nobody could read.
            Applicability::Faulted("error.operator.operand".to_string())
        }
    }
}

/// Demand one `VERIFY`'s `ASSERT` and reduce it to a Boolean domain outcome.
fn assertion_outcome(engine: &mut Engine, declaration: usize, check: &Selected) -> Value {
    let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration) else {
        return Value::Unknown;
    };
    let Some(expr) = lcl_runtime::syntax::field_expr(&block, "ASSERT") else {
        // `VERIFY` requires `ASSERT`; a document without one never reaches
        // here, because M2 rejects it at grammar stage.
        return Value::Unknown;
    };
    let expr = expr.clone();
    match engine.evaluator().demand(&expr) {
        Ok(Value::Boolean(held)) => Value::Boolean(held),
        // "verified ... UNKNOWN when [it] cannot be established." UNKNOWN is a
        // transient observation, never a bound value.
        Ok(Value::Unknown) => Value::Unknown,
        Ok(Value::Missing) => {
            let phase = engine.observed_phase();
            engine.emit(Emission {
                id: CompletionError::RequiredMissing,
                source: &check.source,
                span: expr.span(),
                declaration: Some(check.id.clone()),
                cause: "ASSERT".to_string(),
                detail: format!("{} `{}` demanded a MISSING assertion", check.kind, check.id),
                phase,
            });
            Value::Missing
        }
        Ok(_) | Err(_) => Value::Unknown,
    }
}
