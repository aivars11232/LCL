//! Core result records: the six common fields, the three closed axes, and the
//! invariants that tie them together.
//!
//! Authority: `05_SEMANTICS/05_INPUT_DATA_OUTPUT_RESULT_AND_FORMAT.txt`,
//! `05_SEMANTICS/09_VALIDATION_EXECUTION_FAILURE_AND_TERMINATION.txt` and
//! `built_in_groups_and_results_v0.1.0.json#/result_contract`.
//!
//! ## Three axes, not one
//!
//! The single most load-bearing sentence here is that these are *separate*:
//!
//! > Failure phase, effect state, and OUTPUT binding are separate axes.
//!
//! and
//!
//! > status.partial means only that the producer completed a declared subset of
//! > its required work. It does not imply that OUTPUT is partial and does not
//! > imply that effects are partial. Conversely, effect_state partial does not
//! > force status.partial.
//!
//! So none of these types is derived from another. [`ResultRecord::violations`]
//! checks the exact cross-axis constraints the registry states, and it reports
//! them rather than repairing them: a runtime that silently normalised an
//! inconsistent record would be inventing an observation.
//!
//! ## Absence of evidence
//!
//! > Absence of evidence never proves absence of effects or OUTPUT.
//!
//! That is why [`EffectState::Indeterminate`] and
//! [`FailurePhase::Indeterminate`] exist as first-class values instead of
//! defaulting to `None`, and why nothing in this module infers a phase from a
//! status or a status from a phase.

use crate::value::Value;
use std::collections::BTreeMap;
use std::fmt;

/// `failure_phase`: when a failure happened relative to the producer's effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum FailurePhase {
    /// No failure exists.
    #[default]
    None,
    /// The producer failed before any concrete effect began. "This includes a
    /// command that could not start."
    PreEffect,
    /// At least one effect began before the failure.
    PostEffect,
    /// Available observations cannot establish whether an effect began.
    Indeterminate,
}

impl FailurePhase {
    pub const ALL: [FailurePhase; 4] = [
        FailurePhase::None,
        FailurePhase::PreEffect,
        FailurePhase::PostEffect,
        FailurePhase::Indeterminate,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            FailurePhase::None => "none",
            FailurePhase::PreEffect => "pre_effect",
            FailurePhase::PostEffect => "post_effect",
            FailurePhase::Indeterminate => "indeterminate",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<FailurePhase> {
        FailurePhase::ALL
            .into_iter()
            .find(|p| p.as_registry_str() == s)
    }
}

impl fmt::Display for FailurePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// `effect_state`: the extent of the producer's concrete effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum EffectState {
    /// No concrete effect began.
    #[default]
    None,
    /// Every begun authorized effect completed.
    Applied,
    /// At least one begun authorized effect did not complete.
    Partial,
    /// The extent of effects cannot be established.
    Indeterminate,
}

impl EffectState {
    pub const ALL: [EffectState; 4] = [
        EffectState::None,
        EffectState::Applied,
        EffectState::Partial,
        EffectState::Indeterminate,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            EffectState::None => "none",
            EffectState::Applied => "applied",
            EffectState::Partial => "partial",
            EffectState::Indeterminate => "indeterminate",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<EffectState> {
        EffectState::ALL
            .into_iter()
            .find(|p| p.as_registry_str() == s)
    }
}

impl fmt::Display for EffectState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// `output_binding`: whether the selected `OUTPUT` received its projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum OutputBinding {
    /// The `ACTION` selected no `OUTPUT`.
    #[default]
    NotRequested,
    /// An `OUTPUT` was selected but no complete legal value was assigned.
    Unbound,
    /// The complete projection was assigned.
    Bound,
    /// Permitted only where the result schema declares partial binding.
    Partial,
}

impl OutputBinding {
    pub const ALL: [OutputBinding; 4] = [
        OutputBinding::NotRequested,
        OutputBinding::Unbound,
        OutputBinding::Bound,
        OutputBinding::Partial,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            OutputBinding::NotRequested => "not_requested",
            OutputBinding::Unbound => "unbound",
            OutputBinding::Bound => "bound",
            OutputBinding::Partial => "partial",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<OutputBinding> {
        OutputBinding::ALL
            .into_iter()
            .find(|p| p.as_registry_str() == s)
    }
}

impl fmt::Display for OutputBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// One concrete effect class.
///
/// `none` is deliberately absent: "No record is created for the none
/// sentinel", so an effect class that means "no effect" is unrepresentable
/// here rather than merely discouraged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EffectClass {
    Filesystem,
    Network,
    Process,
    Package,
    Message,
    Memory,
    State,
}

impl EffectClass {
    pub const ALL: [EffectClass; 7] = [
        EffectClass::Filesystem,
        EffectClass::Network,
        EffectClass::Process,
        EffectClass::Package,
        EffectClass::Message,
        EffectClass::Memory,
        EffectClass::State,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            EffectClass::Filesystem => "filesystem",
            EffectClass::Network => "network",
            EffectClass::Process => "process",
            EffectClass::Package => "package",
            EffectClass::Message => "message",
            EffectClass::Memory => "memory",
            EffectClass::State => "state",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<EffectClass> {
        EffectClass::ALL
            .into_iter()
            .find(|c| c.as_registry_str() == s)
    }
}

impl fmt::Display for EffectClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// The record state of one observed effect. `none` is not among them, for the
/// same reason it is not an [`EffectClass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordState {
    Applied,
    Partial,
    Indeterminate,
}

impl RecordState {
    pub const ALL: [RecordState; 3] = [
        RecordState::Applied,
        RecordState::Partial,
        RecordState::Indeterminate,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            RecordState::Applied => "applied",
            RecordState::Partial => "partial",
            RecordState::Indeterminate => "indeterminate",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<RecordState> {
        RecordState::ALL
            .into_iter()
            .find(|c| c.as_registry_str() == s)
    }
}

impl fmt::Display for RecordState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// One `result.effect` record: a concrete effect that began.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObservedEffect {
    pub class: EffectClass,
    pub state: RecordState,
    /// The resolved target, when one is recorded.
    pub target: Option<String>,
    /// "Evidence may be an empty list only when the effect truth is directly
    /// observed without a declared EVIDENCE object."
    pub evidence: Vec<String>,
}

impl fmt::Display for ObservedEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.class, self.state)?;
        if let Some(target) = &self.target {
            write!(f, " on {target}")?;
        }
        Ok(())
    }
}

/// One producer's core result record.
///
/// The six common fields are always present exactly once; `fields` carries the
/// schema-local fields under their registered names, and the registry — not
/// this type — says which names a schema admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRecord {
    /// Which of the nine closed schemas this record is, e.g. `result.value`.
    pub schema: String,
    /// Producer execution status, not the schema-local domain outcome.
    pub status: String,
    pub output_binding: OutputBinding,
    /// Errors in producing the result, as canonical identifiers in order.
    pub execution_errors: Vec<String>,
    pub failure_phase: FailurePhase,
    pub effect_state: EffectState,
    pub observed_effects: Vec<ObservedEffect>,
    /// Schema-local fields, by registered name.
    pub fields: BTreeMap<String, Value>,
}

impl ResultRecord {
    /// A record for a producer that has not failed and caused no effect.
    pub fn new(schema: impl Into<String>, status: impl Into<String>) -> ResultRecord {
        ResultRecord {
            schema: schema.into(),
            status: status.into(),
            output_binding: OutputBinding::NotRequested,
            execution_errors: Vec::new(),
            failure_phase: FailurePhase::None,
            effect_state: EffectState::None,
            observed_effects: Vec::new(),
            fields: BTreeMap::new(),
        }
    }

    pub fn with_field(mut self, name: impl Into<String>, value: Value) -> ResultRecord {
        self.fields.insert(name.into(), value);
        self
    }

    pub fn field(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    /// True when this producer completed its invocation contract.
    ///
    /// `05_SEMANTICS/05`: "A producer status.succeeded means its invocation
    /// contract completed; it does not mean that a Boolean domain outcome is
    /// TRUE."
    pub fn succeeded(&self) -> bool {
        self.status == "status.succeeded"
    }

    /// Check the closed registered field set before any OUTPUT projection.
    /// This reports malformed observations; it never fills missing fields or
    /// converts a supplied value to another family.
    pub fn schema_violations(&self, contracts: &crate::contracts::Contracts) -> Vec<String> {
        let mut out = self.violations();
        let Some(schema) = contracts.schema(&self.schema) else {
            out.push(format!("unregistered result schema {}", self.schema));
            return out;
        };
        if contracts.diagnostics().status(&self.status).is_none() {
            out.push(format!("unregistered producer status {}", self.status));
        }
        for error in &self.execution_errors {
            if contracts.diagnostics().error(error).is_none() {
                out.push(format!("unregistered execution error {error}"));
            }
        }
        for (name, value) in &self.fields {
            match schema.field_types.get(name) {
                None => out.push(format!("{} forbids field {name}", self.schema)),
                Some(ty) if !result_field_type(value, ty, contracts) => {
                    out.push(format!(
                        "{}.{} requires {ty}, found {}",
                        self.schema,
                        name,
                        value.family()
                    ));
                }
                Some(_) => {}
            }
        }
        for (name, cardinality) in &schema.fields {
            if cardinality == "exactly_one" && !self.fields.contains_key(name) {
                out.push(format!("{}.{} is required", self.schema, name));
            }
        }
        let present = |name: &str| self.fields.contains_key(name);
        let is_true = |name: &str| self.field(name) == Some(&Value::Boolean(true));
        let is_false = |name: &str| self.field(name) == Some(&Value::Boolean(false));
        let success_fields: &[&str] = match self.schema.as_str() {
            "result.value" => &["value"],
            "result.collection" => &["items", "count"],
            "result.validation" => &["valid"],
            "result.verification" => &["verified", "observed"],
            "result.test" => &["passed"],
            "result.transfer" => &["bytes"],
            _ => &[],
        };
        if self.succeeded() {
            for name in success_fields {
                if !present(name) {
                    out.push(format!("{} success requires {name}", self.schema));
                }
            }
        }
        if self.output_binding == OutputBinding::Partial && !schema.partial_supported {
            out.push(format!("{} does not permit partial OUTPUT", self.schema));
        }
        match self.schema.as_str() {
            "result.collection" => {
                if present("items") != present("count") {
                    out.push("items and count must both be absent or both present".into());
                }
                if let (Some(Value::List(items)), Some(Value::Integer(count))) =
                    (self.field("items"), self.field("count"))
                {
                    let actual = lcl_checker::numeric::Decimal::from_integer(
                        lcl_checker::numeric::Integer::from_u64(items.len() as u64),
                    );
                    if *count != actual {
                        out.push("count must equal the non-negative number of items".into());
                    }
                }
            }
            "result.command" => {
                match self.field("mode") {
                    Some(Value::Identifier(mode)) if mode == "graph" => {
                        for name in ["started", "completed", "exit_code", "stdout", "stderr"] {
                            if present(name) {
                                out.push(format!("graph mode forbids {name}"));
                            }
                        }
                    }
                    Some(Value::Identifier(mode)) if mode == "non_graph" => {
                        for name in ["started", "completed"] {
                            if !present(name) {
                                out.push(format!("non_graph mode requires {name}"));
                            }
                        }
                        if is_false("started") {
                            if !is_false("completed")
                                || ["exit_code", "stdout", "stderr"].iter().any(|n| present(n))
                                || self.failure_phase != FailurePhase::PreEffect
                                || self.effect_state != EffectState::None
                            {
                                out.push("failure to start requires completed FALSE, no exit or streams, and pre_effect/none".into());
                            }
                        } else if is_true("started") {
                            if !present("stdout") || !present("stderr") {
                                out.push("a started command requires both streams, including empty strings".into());
                            }
                            if present("exit_code") != is_true("completed") {
                                out.push(
                                    "exit_code is present exactly when the command completed"
                                        .into(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            "result.validation" => {
                if let Some(Value::List(errors)) = self.field("errors") {
                    if (is_true("valid") && !errors.is_empty())
                        || (is_false("valid") && errors.is_empty())
                    {
                        out.push(
                            "valid TRUE requires no findings; FALSE requires at least one finding"
                                .into(),
                        );
                    }
                }
            }
            "result.verification" => {
                if present("verified") != present("observed") {
                    out.push(
                        "verified and observed must occur together after verification runs".into(),
                    );
                }
            }
            "result.test" => {
                if present("expected") != present("actual") {
                    out.push("comparison form requires both expected and actual; assertion form omits both".into());
                }
            }
            "result.transfer" => {
                if self.failure_phase == FailurePhase::PreEffect && present("bytes") {
                    out.push("bytes is absent before transfer begins".into());
                }
                if !self.observed_effects.is_empty() && !present("bytes") {
                    out.push(
                        "bytes remains present after transfer begins, including interruption"
                            .into(),
                    );
                }
            }
            _ => {}
        }
        out
    }

    /// Every cross-axis constraint the registry states, checked rather than
    /// assumed.
    ///
    /// An empty result means the record is consistent. A non-empty result names
    /// each violated sentence; the caller decides what to do about it, because
    /// a runtime may not quietly rewrite an observation into a legal shape.
    pub fn violations(&self) -> Vec<String> {
        let mut out = Vec::new();

        // `effect_state_rule`: "pre_effect requires effect_state none and an
        // empty observed_effects list."
        if self.failure_phase == FailurePhase::PreEffect {
            if self.effect_state != EffectState::None {
                out.push(format!(
                    "pre_effect requires effect_state none, found {}",
                    self.effect_state
                ));
            }
            if !self.observed_effects.is_empty() {
                out.push(format!(
                    "pre_effect requires an empty observed_effects list, found {}",
                    self.observed_effects.len()
                ));
            }
            // `output_binding_rule`: "pre_effect leaves OUTPUT unbound and
            // forbids partial OUTPUT."
            if matches!(
                self.output_binding,
                OutputBinding::Bound | OutputBinding::Partial
            ) {
                out.push(format!(
                    "pre_effect forbids a {} OUTPUT",
                    self.output_binding
                ));
            }
        }

        // "post_effect requires at least one known begun effect".
        if self.failure_phase == FailurePhase::PostEffect
            && self.effect_state == EffectState::None
            && self.observed_effects.is_empty()
        {
            out.push("post_effect requires at least one known begun effect".to_string());
        }

        // "effect_state none requires no records."
        if self.effect_state == EffectState::None && !self.observed_effects.is_empty() {
            out.push(format!(
                "effect_state none requires no observed_effects records, found {}",
                self.observed_effects.len()
            ));
        }

        // "indeterminate failure_phase requires effect_state indeterminate
        // unless independent evidence proves the exact effect_state." Evidence
        // is what the records carry, so this holds when no record does.
        if self.failure_phase == FailurePhase::Indeterminate
            && self.effect_state != EffectState::Indeterminate
            && self.observed_effects.iter().all(|e| e.evidence.is_empty())
        {
            out.push(
                "indeterminate failure_phase requires effect_state indeterminate without \
                 independent proving evidence"
                    .to_string(),
            );
        }

        // "none is not an allowed failure phase when an unhandled error exists
        // at that producer."
        if self.failure_phase == FailurePhase::None && !self.execution_errors.is_empty() {
            out.push(
                "failure_phase none is not allowed while execution_errors is non-empty".to_string(),
            );
        }

        out
    }

    /// One line of canonical, order-stable text, for reports and comparison.
    pub fn serialize(&self) -> String {
        let mut out = format!(
            "{} status={} binding={} phase={} effects={}",
            self.schema, self.status, self.output_binding, self.failure_phase, self.effect_state
        );
        if !self.execution_errors.is_empty() {
            out.push_str(&format!(" errors=[{}]", self.execution_errors.join(",")));
        }
        for effect in &self.observed_effects {
            out.push_str(&format!(" effect({effect})"));
        }
        for (name, value) in &self.fields {
            out.push_str(&format!(" {name}={value}"));
        }
        out
    }
}

impl fmt::Display for ResultRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// The finite contract notation used by the nine result schemas. Nested
/// material values are traversed explicitly to avoid recursive host input.
fn result_field_type(value: &Value, ty: &str, contracts: &crate::contracts::Contracts) -> bool {
    match ty {
        "meta.material_value" | "target_expression" => {
            let mut pending = vec![value];
            while let Some(value) = pending.pop() {
                match value {
                    Value::Missing | Value::Unknown => return false,
                    Value::List(items) | Value::Set(items) => pending.extend(items),
                    Value::Object(fields) => pending.extend(fields.values()),
                    _ => {}
                }
            }
            true
        }
        "BOOLEAN" => matches!(value, Value::Boolean(_)),
        "BOOLEAN|UNKNOWN" => matches!(value, Value::Boolean(_) | Value::Unknown),
        "INTEGER" => matches!(value, Value::Integer(n) if n.is_integral()),
        "BYTES|UNKNOWN" => {
            matches!(value, Value::Unknown)
                || matches!(value, Value::Bytes(n) if n.is_integral() && !n.is_negative())
        }
        "STRING" => matches!(value, Value::Text(_)),
        "STRING|NULL" => matches!(value, Value::Text(_) | Value::Null),
        "OBJECT" => matches!(value, Value::Object(_)),
        "ENUM[non_graph|graph]" => {
            matches!(value, Value::Identifier(mode) if mode == "non_graph" || mode == "graph")
        }
        "LIST[T]" => {
            matches!(value, Value::List(items) if items.iter().all(|v| result_field_type(v, "meta.material_value", contracts)))
        }
        "LIST[REFERENCE[EVIDENCE]]" => {
            matches!(value, Value::List(items) if items.iter().all(|v| matches!(v, Value::Reference(id) if id.starts_with("evidence."))))
        }
        "LIST[qualified_identifier(error)]" => {
            matches!(value, Value::List(items) if items.iter().all(|v| matches!(v, Value::Identifier(id) if contracts.diagnostics().error(id).is_some())))
        }
        _ => false,
    }
}
