//! The machine-readable record of what the engine did.
//!
//! ## One record, two consumers
//!
//! A UI written in Rust holds these types directly. A UI written in anything
//! else runs the CLI and reads [`Report::to_json`]. Both see the same fields,
//! produced by the same code, because the JSON projection is a rendering of
//! these structs and never a second assembly of the same facts.
//!
//! ## What a record carries, and why each part is required
//!
//! The task's acceptance criterion names four things a machine record must
//! carry, and each is here for a reason the canonical text already fixed:
//!
//! * **exact source identity** — `07_VERSIONING_AND_EXTENSIONS/02` makes a
//!   document's identity and its exact bytes load-bearing for imports, so a
//!   record names every unit it loaded and the SHA-256 of the bytes that were
//!   actually used;
//! * **byte spans** — `02_LEXICAL/01` makes source bytes normative and every
//!   engine crate documents its derived line and column as presentation only,
//!   so [`DiagnosticRecord::span`] is the offset pair and the position is
//!   carried beside it, never instead of it;
//! * **diagnostic identifiers, stages and statuses** — the closed stage order
//!   and the registered `default_status` are what a consumer needs to reproduce
//!   the earliest-stage rule without reimplementing it;
//! * **execution and evidence records** — steps 10 through 13 are observations,
//!   and a consumer that could not see them would have to guess why an
//!   invocation ended as it did.
//!
//! ## Values are carried as the engine renders them
//!
//! A `Value` appears as one string, produced by the engine's own `Display`.
//! Projecting the value model into JSON types would be a second value model of
//! the same language living in the product layer, and the first disagreement
//! between them would be a silent semantic fork. The rendering is exact —
//! `Decimal` renders its exact base-10 digits — and order-stable.

use crate::json::{Node, Object};
use lcl_diagnostics::Stage;
use lcl_lexer::{Position, Span};

/// The protocol version this build emits.
///
/// A consumer reads it before anything else. It changes only when a field's
/// meaning changes, never when a field is added.
pub const PROTOCOL: &str = "lcl.engine/1";

/// How far a source got through the canonical processing order.
///
/// The names are the canonical stage names from
/// `01_FOUNDATION/03_NORMATIVE_PROCESSING_MODEL.txt`, with `completion`
/// covering steps 11 to 13, so a consumer reading `reached` is reading the
/// language's own vocabulary rather than a product invention.
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

impl std::fmt::Display for Reached {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the engine was asked to do.
///
/// Each stops at a canonical boundary rather than at a convenient one:
/// [`Command::Check`] ends after step 5, [`Command::Validate`] after step 9 and
/// before any effect, and [`Command::Run`] carries through step 13.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Steps 1 to 5. Lexical, grammar, resolution and static checking.
    Check,
    /// Steps 1 to 9. Everything `check` does, plus the no-effect semantic
    /// preflight that produces an accepted execution plan.
    Validate,
    /// Steps 1 to 13. Execution through the capability boundary, then
    /// verification, evidence, success or failure, and one terminal status.
    Run,
    /// Steps 1 to 9, reported structurally: units, declarations, imports and
    /// the ordered plan. Performs no effect.
    Inspect,
}

impl Command {
    pub fn as_str(self) -> &'static str {
        match self {
            Command::Check => "check",
            Command::Validate => "validate",
            Command::Run => "run",
            Command::Inspect => "inspect",
        }
    }

    /// The last stage this command evaluates when nothing stops it earlier.
    pub fn final_stage(self) -> Reached {
        match self {
            Command::Check => Reached::StaticChecking,
            Command::Validate | Command::Inspect => Reached::Preflight,
            Command::Run => Reached::Completion,
        }
    }
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The verdict on one request, distinct from any LCL status.
///
/// `Accepted` says the requested stages produced no unhandled diagnostic. It is
/// never a claim that the document ran or succeeded; `05_SEMANTICS/10` keeps
/// those apart and so does this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Every requested stage produced no unhandled diagnostic.
    Accepted,
    /// A stage produced a primary unhandled diagnostic. The document did not
    /// advance past it.
    Rejected,
    /// The request itself was not well formed, so the document was never
    /// judged.
    ///
    /// Distinct from `Rejected` on purpose. A caller who supplied an input the
    /// engine could not turn into a value has learned nothing about the
    /// document, and a report saying "rejected" would be claiming they had.
    Refused,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Accepted => "accepted",
            Outcome::Rejected => "rejected",
            Outcome::Refused => "refused",
        }
    }
}

/// One datum a caller supplied for a declared `INPUT`, `STATE`, `MEMORY` or
/// `CONTEXT`, and what became of it.
///
/// Recorded whether or not it was used. `05_SEMANTICS/02` rules out ambient
/// data, so every datum that entered an invocation must be attributable to a
/// caller who named it, and one that went nowhere must be visible rather than
/// silently absorbed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputRecord {
    /// The qualified declaration id this datum was supplied for.
    pub id: String,
    /// The expression exactly as supplied, for a datum given as text.
    pub expression: Option<String>,
    /// The value, as the engine renders it, when one was obtained.
    pub value: Option<String>,
    /// Why no value was obtained, when none was.
    pub reason: Option<String>,
}

impl InputRecord {
    pub fn accepted(&self) -> bool {
        self.reason.is_none()
    }

    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("expression", Node::optional(self.expression.clone()))
            .with("value", Node::optional(self.value.clone()))
            .with("reason", Node::optional(self.reason.clone()))
            .with("accepted", Node::Bool(self.accepted()))
            .into()
    }
}

/// The specification package a report was produced against.
///
/// Present in every report because a result is only reproducible if the
/// authority that produced it is named. The identity digest is the external
/// trust anchor's, so two reports carrying the same digest were produced
/// against the same bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecRecord {
    pub root: String,
    pub formal_version: String,
    pub identity_digest: String,
    /// `authoritative` or `unverified`.
    pub authority: String,
}

impl SpecRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("root", Node::string(&self.root))
            .with("formal_version", Node::string(&self.formal_version))
            .with("identity_digest", Node::string(&self.identity_digest))
            .with("authority", Node::string(&self.authority))
            .into()
    }
}

/// One source unit the engine actually loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRecord {
    /// The unit's identity, exactly as the provider assigned it.
    pub id: String,
    /// Lowercase hex SHA-256 of the exact bytes that were used.
    pub digest: String,
    pub bytes: usize,
    /// True for the unit the request named.
    pub root: bool,
}

impl SourceRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("digest", Node::string(format!("sha256:{}", self.digest)))
            .with("bytes", Node::usize(self.bytes))
            .with("root", Node::Bool(self.root))
            .into()
    }
}

/// One diagnostic, normalised across every layer that can emit one.
///
/// The seven engine crates each define their own `Diagnostic`, and each carries
/// the same registry-derived fields. This is the one shape a consumer sees, and
/// building it is a projection: no field is computed here, and no identifier,
/// stage or status is reclassified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRecord {
    /// The registered error identifier, e.g. `error.id.duplicate`.
    pub id: String,
    /// The stage that governs ordering and classification, as the emitting
    /// layer resolved it. Never rewritten by this crate.
    pub stage: Stage,
    /// The unit this locus belongs to.
    pub source: String,
    /// The authoritative byte locus.
    pub span: Span,
    /// Derived line and column. Presentation only.
    pub position: Position,
    /// `errors.<id>.meaning`, verbatim from the registry.
    pub meaning: String,
    /// `errors.<id>.default_status`, verbatim from the registry.
    pub default_status: String,
    /// `diagnostic_selection.specificity_rank`.
    pub specificity_rank: u64,
    /// `errors.<id>.event`, when the identifier raises one.
    pub event: Option<String>,
    /// The `cause_identity` component of `duplicate_key`.
    pub cause: String,
    /// Non-normative human detail.
    pub detail: Option<String>,
    /// The emitting layer's stable emission identity, where it has one.
    ///
    /// Present for execution and completion diagnostics. Selection reorders and
    /// merges diagnostics, so a position in a list is not an identity; an event
    /// records the diagnostic it was raised by, and a consumer needs the same
    /// handle to join the two after ordering.
    pub sequence: Option<usize>,
    /// True for the one diagnostic `primary_rule` selects.
    pub primary: bool,
}

impl DiagnosticRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("stage", Node::string(self.stage.as_registry_str()))
            .with("source", Node::string(&self.source))
            .with(
                "span",
                Object::new()
                    .with("start", Node::usize(self.span.start))
                    .with("end", Node::usize(self.span.end))
                    .into(),
            )
            .with(
                "position",
                Object::new()
                    .with("line", Node::u64(self.position.line as u64))
                    .with("column", Node::u64(self.position.column as u64))
                    .with("offset", Node::usize(self.position.offset))
                    .into(),
            )
            .with("meaning", Node::string(&self.meaning))
            .with("default_status", Node::string(&self.default_status))
            .with("specificity_rank", Node::u64(self.specificity_rank))
            .with("event", Node::optional(self.event.clone()))
            .with("cause", Node::string(&self.cause))
            .with("detail", Node::optional(self.detail.clone()))
            .with(
                "sequence",
                match self.sequence {
                    Some(sequence) => Node::usize(sequence),
                    None => Node::Null,
                },
            )
            .with("primary", Node::Bool(self.primary))
            .into()
    }

    /// One line, for a human renderer that wants the canonical shape.
    pub fn render(&self) -> String {
        let mut out = format!(
            "{}:{}:{}: {} [{}]",
            self.source, self.position.line, self.position.column, self.id, self.stage
        );
        if let Some(detail) = &self.detail {
            out.push_str(&format!(": {detail}"));
        }
        out
    }
}

/// One import or extension the resolver acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRecord {
    /// `IMPORT` or `EXTENSION`.
    pub kind: String,
    /// The unit whose block this is.
    pub origin: String,
    /// The block's own `ID`.
    pub id: String,
    pub namespace: String,
    /// The `SOURCE` value as written, e.g. `PATH("lib.lcl")`.
    pub reference: Option<String>,
    /// The requested exact `VERSION`.
    pub version: String,
    pub checksum: Option<String>,
    /// `loaded`, `not_found`, `checksum_mismatch`, `cycle` or `not_requested`.
    pub outcome: String,
    /// The identity of the unit that loaded, when one did.
    pub loaded: Option<String>,
}

impl ImportRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("kind", Node::string(&self.kind))
            .with("origin", Node::string(&self.origin))
            .with("id", Node::string(&self.id))
            .with("namespace", Node::string(&self.namespace))
            .with("reference", Node::optional(self.reference.clone()))
            .with("version", Node::string(&self.version))
            .with("checksum", Node::optional(self.checksum.clone()))
            .with("outcome", Node::string(&self.outcome))
            .with("loaded", Node::optional(self.loaded.clone()))
            .into()
    }
}

/// One node of the accepted execution plan, for `inspect`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    /// Index into the plan's node list.
    pub index: usize,
    /// The declaring block, e.g. `ACTION`, or the control form.
    pub block: String,
    /// The activated declaration's qualified id, when it activates one.
    pub id: Option<String>,
    pub source: String,
    pub span: Span,
    pub parent: Option<usize>,
    pub required: bool,
    /// The operation this node authorizes, when it acts.
    pub operation: Option<String>,
    /// Position in the final ordering, when the node is ordered.
    pub order: Option<usize>,
}

impl PlanRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("index", Node::usize(self.index))
            .with("block", Node::string(&self.block))
            .with("id", Node::optional(self.id.clone()))
            .with("source", Node::string(&self.source))
            .with(
                "span",
                Object::new()
                    .with("start", Node::usize(self.span.start))
                    .with("end", Node::usize(self.span.end))
                    .into(),
            )
            .with(
                "parent",
                match self.parent {
                    Some(p) => Node::usize(p),
                    None => Node::Null,
                },
            )
            .with("required", Node::Bool(self.required))
            .with("operation", Node::optional(self.operation.clone()))
            .with(
                "order",
                match self.order {
                    Some(o) => Node::usize(o),
                    None => Node::Null,
                },
            )
            .into()
    }
}

/// One invocation the runtime entered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationRecord {
    /// The plan node this invocation activates.
    pub node: usize,
    /// The full enclosing iteration path, empty at the root.
    pub iteration: String,
    /// Zero-based attempt index; above zero only under a declared `RETRY`.
    pub attempt: usize,
    pub block: String,
    pub declaration: Option<String>,
    /// The invocation's lifecycle status.
    pub status: String,
    /// The producer result's registered schema, when it produced one.
    pub result_schema: Option<String>,
    /// The producer result's status, when it produced one.
    pub result_status: Option<String>,
    /// Effects the host reported, in the order they began.
    pub effects: Vec<String>,
}

impl InvocationRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("node", Node::usize(self.node))
            .with("iteration", Node::string(&self.iteration))
            .with("attempt", Node::usize(self.attempt))
            .with("block", Node::string(&self.block))
            .with("declaration", Node::optional(self.declaration.clone()))
            .with("status", Node::string(&self.status))
            .with("result_schema", Node::optional(self.result_schema.clone()))
            .with("result_status", Node::optional(self.result_status.clone()))
            .with(
                "effects",
                Node::array(self.effects.iter().map(Node::string)),
            )
            .into()
    }
}

/// One raised event and what handler selection did with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub event: String,
    /// The emission identity of the diagnostic that raised it.
    pub diagnostic: usize,
    /// Zero-based occurrence index across the whole execution.
    pub occurrence: usize,
    pub producer: String,
    pub disposition: String,
}

impl EventRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("event", Node::string(&self.event))
            .with("diagnostic", Node::usize(self.diagnostic))
            .with("occurrence", Node::usize(self.occurrence))
            .with("producer", Node::string(&self.producer))
            .with("disposition", Node::string(&self.disposition))
            .into()
    }
}

/// What one execution did. Observations only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub invocations: Vec<InvocationRecord>,
    pub events: Vec<EventRecord>,
    /// How many steps the queue executed. Evidence that the run was bounded.
    pub steps: usize,
}

impl ExecutionRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with(
                "invocations",
                Node::array(self.invocations.iter().map(InvocationRecord::to_json)),
            )
            .with(
                "events",
                Node::array(self.events.iter().map(EventRecord::to_json)),
            )
            .with("steps", Node::usize(self.steps))
            .into()
    }
}

/// One post-execution `VERIFY` or `TEST`.
///
/// A skipped check has no outcome. `05_SEMANTICS/10` makes absence meaningful —
/// a skipped check is not implicitly true — so the record keeps the skip and
/// its reason rather than omitting the check or inventing a value for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRecord {
    pub id: String,
    /// `VERIFY` or `TEST`.
    pub kind: String,
    pub source: String,
    pub span: Span,
    pub required: bool,
    /// Why this check was selected.
    pub selection: String,
    /// `TRUE`, `FALSE` or `UNKNOWN`, as the value renders. `None` when skipped.
    pub outcome: Option<String>,
    /// Why it was skipped, when it was.
    pub skipped: Option<String>,
    pub evidence: Vec<String>,
}

impl CheckRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("kind", Node::string(&self.kind))
            .with("source", Node::string(&self.source))
            .with(
                "span",
                Object::new()
                    .with("start", Node::usize(self.span.start))
                    .with("end", Node::usize(self.span.end))
                    .into(),
            )
            .with("required", Node::Bool(self.required))
            .with("selection", Node::string(&self.selection))
            .with("outcome", Node::optional(self.outcome.clone()))
            .with("skipped", Node::optional(self.skipped.clone()))
            .with(
                "evidence",
                Node::array(self.evidence.iter().map(Node::string)),
            )
            .into()
    }
}

/// One collected `EVIDENCE` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub id: String,
    pub source: String,
    pub span: Span,
    pub declared_type: Option<String>,
    pub required: bool,
    /// `value`, `source` or `unresolved`.
    pub provision: String,
    /// The rendered value, or the declared source, or why it did not resolve.
    pub detail: String,
    pub checksum: Option<String>,
    pub provenance: Option<String>,
    pub referenced_by: Vec<String>,
    pub satisfied: bool,
}

impl EvidenceRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("source", Node::string(&self.source))
            .with(
                "span",
                Object::new()
                    .with("start", Node::usize(self.span.start))
                    .with("end", Node::usize(self.span.end))
                    .into(),
            )
            .with("declared_type", Node::optional(self.declared_type.clone()))
            .with("required", Node::Bool(self.required))
            .with("provision", Node::string(&self.provision))
            .with("detail", Node::string(&self.detail))
            .with("checksum", Node::optional(self.checksum.clone()))
            .with("provenance", Node::optional(self.provenance.clone()))
            .with(
                "referenced_by",
                Node::array(self.referenced_by.iter().map(Node::string)),
            )
            .with("satisfied", Node::Bool(self.satisfied))
            .into()
    }
}

/// One declared root `OUTPUT` and its publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputRecord {
    pub id: String,
    pub declared_type: Option<String>,
    pub required: bool,
    /// `published`, `unbound` or `ambiguous`.
    pub publication: String,
    /// The rendered value, when published.
    pub value: Option<String>,
    /// How many loop instances bound it, when ambiguous.
    pub instances: Option<usize>,
}

impl OutputRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("id", Node::string(&self.id))
            .with("declared_type", Node::optional(self.declared_type.clone()))
            .with("required", Node::Bool(self.required))
            .with("publication", Node::string(&self.publication))
            .with("value", Node::optional(self.value.clone()))
            .with(
                "instances",
                match self.instances {
                    Some(n) => Node::usize(n),
                    None => Node::Null,
                },
            )
            .into()
    }
}

/// What `SUCCESS` and `FAILURE` decided. Deliberately status-free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerdictRecord {
    /// The root `SUCCESS` declaration's id, when the root references one.
    pub success: Option<String>,
    /// `ALL`, `ANY` or `NONE`.
    pub quantifier: Option<String>,
    /// `TRUE`, `FALSE` or `UNKNOWN`.
    pub success_value: Option<String>,
    /// Each member reference and the value it contributed, in source order.
    pub members: Vec<(String, String)>,
    /// The selected `FAILURE` clause's id, when one was selected.
    pub failure: Option<String>,
    /// The status that clause requests, after alias resolution.
    pub failure_status: Option<String>,
    /// Its `ERROR` classification metadata, which emits nothing.
    pub failure_classification: Option<String>,
}

impl VerdictRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with("success", Node::optional(self.success.clone()))
            .with("quantifier", Node::optional(self.quantifier.clone()))
            .with("value", Node::optional(self.success_value.clone()))
            .with(
                "members",
                Node::array(self.members.iter().map(|(id, value)| {
                    Object::new()
                        .with("id", Node::string(id))
                        .with("value", Node::string(value))
                        .into()
                })),
            )
            .with("failure", Node::optional(self.failure.clone()))
            .with(
                "failure_status",
                Node::optional(self.failure_status.clone()),
            )
            .with(
                "failure_classification",
                Node::optional(self.failure_classification.clone()),
            )
            .into()
    }
}

/// Canonical steps 11 to 13 as one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRecord {
    pub checks: Vec<CheckRecord>,
    pub evidence: Vec<EvidenceRecord>,
    pub verdict: VerdictRecord,
    pub outputs: Vec<OutputRecord>,
    /// The one terminal status of this invocation.
    pub terminal_status: String,
    /// Why it is that one, in the completion layer's own vocabulary.
    pub reason: String,
}

impl CompletionRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with(
                "checks",
                Node::array(self.checks.iter().map(CheckRecord::to_json)),
            )
            .with(
                "evidence",
                Node::array(self.evidence.iter().map(EvidenceRecord::to_json)),
            )
            .with("verdict", self.verdict.to_json())
            .with(
                "outputs",
                Node::array(self.outputs.iter().map(OutputRecord::to_json)),
            )
            .with("terminal_status", Node::string(&self.terminal_status))
            .with("reason", Node::string(&self.reason))
            .into()
    }
}

/// The structural view `inspect` produces. No effect, no execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureRecord {
    pub imports: Vec<ImportRecord>,
    /// Every declaration's qualified id, with its block, in source order.
    pub declarations: Vec<(String, String)>,
    /// The candidate graph's node count, before preflight ordering.
    pub candidates: usize,
    pub plan: Vec<PlanRecord>,
    /// Supplied invocation data whose declaration the document does not
    /// declare. Never read; reported so it cannot be silently absorbed.
    pub unused_inputs: Vec<String>,
}

impl StructureRecord {
    fn to_json(&self) -> Node {
        Object::new()
            .with(
                "imports",
                Node::array(self.imports.iter().map(ImportRecord::to_json)),
            )
            .with(
                "declarations",
                Node::array(self.declarations.iter().map(|(id, block)| {
                    Object::new()
                        .with("id", Node::string(id))
                        .with("block", Node::string(block))
                        .into()
                })),
            )
            .with("candidates", Node::usize(self.candidates))
            .with(
                "plan",
                Node::array(self.plan.iter().map(PlanRecord::to_json)),
            )
            .with(
                "unused_inputs",
                Node::array(self.unused_inputs.iter().map(Node::string)),
            )
            .into()
    }
}

/// The complete record of one engine request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub command: Command,
    pub spec: SpecRecord,
    /// Every unit that was loaded, root first, then imports in load order.
    pub units: Vec<SourceRecord>,
    pub reached: Reached,
    pub outcome: Outcome,
    /// Every datum the caller supplied, in the order supplied.
    pub inputs: Vec<InputRecord>,
    /// Every diagnostic any stage produced, in stage and stable order.
    pub diagnostics: Vec<DiagnosticRecord>,
    /// Present for `inspect`.
    pub structure: Option<StructureRecord>,
    /// Present for `run`, once execution began.
    pub execution: Option<ExecutionRecord>,
    /// Present for `run`, once execution finished.
    pub completion: Option<CompletionRecord>,
}

impl Report {
    /// The one diagnostic `primary_rule` selected, if any.
    pub fn primary(&self) -> Option<&DiagnosticRecord> {
        self.diagnostics.iter().find(|d| d.primary)
    }

    /// The root unit's identity.
    pub fn root(&self) -> Option<&str> {
        self.units.iter().find(|u| u.root).map(|u| u.id.as_str())
    }

    /// The terminal status, for a request that reached completion.
    pub fn terminal_status(&self) -> Option<&str> {
        self.completion.as_ref().map(|c| c.terminal_status.as_str())
    }

    /// The complete JSON projection.
    pub fn to_json(&self) -> Node {
        Object::new()
            .with("protocol", Node::string(PROTOCOL))
            .with("command", Node::string(self.command.as_str()))
            .with("spec", self.spec.to_json())
            .with(
                "units",
                Node::array(self.units.iter().map(SourceRecord::to_json)),
            )
            .with("reached", Node::string(self.reached.as_str()))
            .with("outcome", Node::string(self.outcome.as_str()))
            .with(
                "inputs",
                Node::array(self.inputs.iter().map(InputRecord::to_json)),
            )
            .with(
                "diagnostics",
                Node::array(self.diagnostics.iter().map(DiagnosticRecord::to_json)),
            )
            .with_some(
                "structure",
                self.structure.as_ref().map(StructureRecord::to_json),
            )
            .with_some(
                "execution",
                self.execution.as_ref().map(ExecutionRecord::to_json),
            )
            .with_some(
                "completion",
                self.completion.as_ref().map(CompletionRecord::to_json),
            )
            .into()
    }
}
