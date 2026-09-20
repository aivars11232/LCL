//! Step 12a: collect the evidence the invocation declared it needs.
//!
//! `05_SEMANTICS/10`:
//!
//! > EVIDENCE must be observable, typed, and traceable; declared required
//! > provenance/checksum must resolve.
//!
//! and the block schema: "Exactly one SOURCE or VALUE when evaluated."
//!
//! ## Selected, not swept
//!
//! An `EVIDENCE` declaration is not collected because it exists. It is
//! collected because something that ran named it: a check that produced a
//! result, the selected `FAILURE` clause, or an activated `ACTION`. A document
//! may declare evidence for a branch it did not take, and that evidence is not
//! required — the same reason an unselected `IF` arm's `VERIFY` is not
//! selected.
//!
//! ## Required, and what that costs
//!
//! `REQUIRED` defaults TRUE in the field signature, so silence means required.
//! A required declaration that does not resolve emits `error.evidence.missing`,
//! whose registered `default_status` is `status.failed`. An optional one that
//! does not resolve is recorded as unresolved and emits nothing.
//!
//! ## What "resolve" is allowed to mean here
//!
//! A `VALUE` resolves when it demands to a material value. MISSING and UNKNOWN
//! are not material: `indeterminate_state_rule` forbids inferring "successful
//! postcondition, or OUTPUT value from absence of evidence", and treating
//! UNKNOWN evidence as present would be exactly that inference.
//!
//! A `SOURCE` names something outside the document. This layer performs no
//! effect, so it does not fetch one — it records the declared source as
//! unverified provenance and leaves fetching to a host that has a capability
//! for it. What it does check is the traceability the language itself can
//! check: that a declared `CHECKSUM` or `PROVENANCE` is present and non-empty
//! when the declaration is required, because "declared required
//! provenance/checksum must resolve".

use crate::check::Checks;
use crate::diagnostic::CompletionError;
use crate::engine::{Emission, Engine};
use crate::success::Verdict;
use crate::syntax;
use lcl_lexer::Span;
use lcl_resolver::SourceId;
use lcl_runtime::Value;
use std::collections::{BTreeMap, BTreeSet};

/// How one evidence declaration supplies its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provision {
    /// An inline `VALUE` that demanded to a material value.
    Value(Value),
    /// A declared `SOURCE`, recorded as written. This layer performs no effect,
    /// so the source is traceable but not fetched.
    Source(String),
    /// Neither `SOURCE` nor `VALUE` resolved.
    Unresolved(String),
}

impl Provision {
    pub fn resolved(&self) -> bool {
        !matches!(self, Provision::Unresolved(_))
    }
}

/// One collected evidence declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: String,
    pub source: SourceId,
    pub span: Span,
    /// The declared `TYPE`, verbatim. Evidence must be typed.
    pub declared_type: Option<String>,
    pub required: bool,
    pub provision: Provision,
    /// `CHECKSUM`, as written.
    pub checksum: Option<String>,
    /// `PROVENANCE`, as written.
    pub provenance: Option<String>,
    /// Why this declaration was collected.
    pub referenced_by: Vec<String>,
}

impl EvidenceRecord {
    /// True when this record satisfies its declared obligation.
    pub fn satisfied(&self) -> bool {
        if !self.provision.resolved() {
            return false;
        }
        if self.declared_type.is_none() {
            return false;
        }
        // "declared required provenance/checksum must resolve." A field the
        // declaration did not write imposes nothing; one it wrote empty does
        // not resolve.
        if self.checksum.as_deref().is_some_and(str::is_empty) {
            return false;
        }
        if self.provenance.as_deref().is_some_and(str::is_empty) {
            return false;
        }
        true
    }

    /// True when an unsatisfied required declaration blocks success.
    pub fn blocks(&self) -> bool {
        self.required && !self.satisfied()
    }

    pub fn serialize(&self) -> String {
        format!(
            "{} required={} satisfied={} provision={:?}",
            self.id,
            self.required,
            self.satisfied(),
            self.provision
        )
    }
}

/// Every evidence declaration this invocation actually required.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    records: Vec<EvidenceRecord>,
}

impl Evidence {
    pub fn records(&self) -> &[EvidenceRecord] {
        &self.records
    }

    pub fn record(&self, id: &str) -> Option<&EvidenceRecord> {
        self.records.iter().find(|r| r.id == id)
    }

    /// Every required declaration that did not resolve.
    pub fn missing(&self) -> impl Iterator<Item = &EvidenceRecord> {
        self.records.iter().filter(|r| r.blocks())
    }

    /// True when every required declaration resolved.
    ///
    /// One of the four conditions root `status.succeeded` requires.
    pub fn complete(&self) -> bool {
        self.records.iter().all(|r| !r.blocks())
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn serialize(&self) -> String {
        let mut out = String::from("EVIDENCE\n");
        for record in &self.records {
            out.push_str(&format!("  {}\n", record.serialize()));
        }
        out
    }
}

/// Collect and resolve every evidence declaration this invocation required.
pub(crate) fn run(engine: &mut Engine, checks: &Checks, verdict: &Verdict) -> Evidence {
    let referenced = referenced_evidence(engine, checks, verdict);
    let phase = engine.observed_phase();
    let mut records = Vec::new();

    for (id, referrers) in referenced {
        let Some(declaration) = engine
            .resolved
            .declarations()
            .all()
            .iter()
            .position(|d| d.id.qualified() == id && d.block == "EVIDENCE")
        else {
            // A reference that names no EVIDENCE declaration is an M3 defect
            // and already carries error.reference.unresolved. Nothing to add.
            continue;
        };
        let (source, span) = {
            let decl = &engine.resolved.declarations().all()[declaration];
            (decl.source.clone(), decl.id_span)
        };
        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, declaration)
        else {
            continue;
        };

        let declared_type = lcl_runtime::syntax::field_text(&block, "TYPE");
        let checksum = lcl_runtime::syntax::field_text(&block, "CHECKSUM");
        let provenance = lcl_runtime::syntax::field_text(&block, "PROVENANCE");
        let required = match lcl_runtime::syntax::field_text(&block, "REQUIRED").as_deref() {
            Some("TRUE") => true,
            Some("FALSE") => false,
            _ => engine
                .contracts
                .runtime()
                .preflight()
                .field_default_boolean("EVIDENCE", "REQUIRED")
                .unwrap_or(true),
        };

        let value_expr = lcl_runtime::syntax::field_expr(&block, "VALUE").cloned();
        let source_text = block
            .field("SOURCE")
            .and_then(|f| syntax::inline_expr(&f.body))
            .map(lcl_runtime::syntax::render);

        let provision = match (value_expr, source_text) {
            (Some(expr), None) => match engine.evaluator().demand(&expr) {
                Ok(Value::Missing) => Provision::Unresolved("VALUE demanded MISSING".to_string()),
                // "do not infer ... an OUTPUT value from absence of evidence."
                Ok(Value::Unknown) => Provision::Unresolved("VALUE demanded UNKNOWN".to_string()),
                Ok(value) => Provision::Value(value),
                Err(fault) => Provision::Unresolved(format!("VALUE faulted: {}", fault.id)),
            },
            (None, Some(source)) => Provision::Source(source),
            // "Exactly one SOURCE or VALUE when evaluated."
            (Some(_), Some(_)) => {
                Provision::Unresolved("both SOURCE and VALUE are declared".to_string())
            }
            (None, None) => {
                Provision::Unresolved("neither SOURCE nor VALUE is declared".to_string())
            }
        };

        let record = EvidenceRecord {
            id: id.clone(),
            source: source.clone(),
            span,
            declared_type,
            required,
            provision,
            checksum,
            provenance,
            referenced_by: referrers.into_iter().collect(),
        };

        if record.blocks() {
            let detail = match &record.provision {
                Provision::Unresolved(why) => {
                    format!("required evidence `{id}` did not resolve: {why}")
                }
                _ => format!("required evidence `{id}` is not traceable as declared"),
            };
            engine.emit(Emission {
                id: CompletionError::EvidenceMissing,
                source: &source,
                span,
                declaration: Some(id.clone()),
                cause: id.clone(),
                detail,
                phase,
                demand_resolved: false,
            });
        }
        records.push(record);
    }

    Evidence { records }
}

/// The field bodies one ACTION block names evidence declarations in: its own
/// `EVIDENCE` field, and an `evidence` operation parameter's `VALUE`.
fn evidence_fields<'a>(
    block: &lcl_runtime::syntax::DeclBlock<'a>,
) -> Vec<&'a lcl_parser::syntax::Field> {
    use lcl_parser::syntax::Statement;
    let mut fields: Vec<&lcl_parser::syntax::Field> = block.field("EVIDENCE").into_iter().collect();
    for parameter in block.fields("PARAMETER") {
        let Some(nested) = parameter.body.as_nested() else {
            continue;
        };
        let named = |name: &str| {
            nested
                .statements
                .iter()
                .find_map(|statement| match statement {
                    Statement::Field(field) if field.key.text == name => Some(field),
                    _ => None,
                })
        };
        let is_evidence = named("NAME")
            .and_then(|field| syntax::inline_expr(&field.body))
            .and_then(syntax::literal_text)
            .is_some_and(|name| name == "evidence");
        if is_evidence {
            fields.extend(named("VALUE"));
        }
    }
    fields
}

/// Every evidence id something that actually ran referenced, with its referrers.
fn referenced_evidence(
    engine: &Engine,
    checks: &Checks,
    verdict: &Verdict,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    // Checks that produced a result. A skipped check has no result and
    // therefore no evidence obligation.
    for result in checks.results() {
        for id in &result.evidence {
            out.entry(id.clone()).or_default().insert(result.id.clone());
        }
    }

    // The selected FAILURE clause: "Its required EVIDENCE must still resolve."
    if let Some(failure) = &verdict.failure {
        for id in &failure.evidence {
            out.entry(id.clone())
                .or_default()
                .insert(failure.id.clone());
        }
    }

    // Activated producers. An ACTION that never ran imposes no evidence.
    for (index, declaration) in engine.resolved.declarations().all().iter().enumerate() {
        if declaration.block != "ACTION" {
            continue;
        }
        let id = declaration.id.qualified();
        if !engine.observation.activated(&id) {
            continue;
        }
        let Some(block) = lcl_runtime::syntax::declaration_block(engine.resolved, index) else {
            continue;
        };
        // An ACTION names the evidence it requires in its own EVIDENCE field,
        // and a row whose contract takes evidence declarations names them in
        // that parameter. `operations_v0.1.0.json#/contracts/core.verify`
        // calls its `evidence` parameter "Required evidence declarations", so
        // an activated invocation that supplied one named it exactly as the
        // field does.
        for field in evidence_fields(&block) {
            for (evidence, _) in syntax::reference_list(&field.body) {
                out.entry(evidence).or_default().insert(id.clone());
            }
        }
    }

    out
}
