//! # lcl-diagnostics — diagnostics skeleton
//!
//! Milestone M0 component B.
//!
//! Models the **registered** diagnostic vocabulary of LCL Core 0.1.0: the
//! closed stage order, the 12 statuses and the 77 errors, each with its
//! normative stage, recoverability, event mapping and default status.
//!
//! ## What this crate is
//!
//! A typed, validated, in-memory view of the diagnostic registry, built
//! entirely from `10_REGISTRIES/statuses_and_errors_v0.1.0.json` by way of
//! [`lcl_spec::SpecPackage`]. Every identifier, stage and status here comes
//! from the registry at load time. Nothing is transcribed into Rust source: the
//! [`Stage`] enum is the sole exception, and it is *validated against* the
//! registry's `stage_order` rather than trusted.
//!
//! ## What this crate is NOT (M0 boundary)
//!
//! It does **not** implement diagnostic *selection*. The normative selection
//! algorithm — `expression_demand_resolution`, earliest-stage rule,
//! multiplicity, supersession, duplicate suppression, stable ordering, and
//! primary/secondary classification — is deliberately out of M0 scope. The raw
//! contract text is exposed via [`DiagnosticRegistry::selection_contract`] so
//! that a later milestone implements it against the registry rather than
//! against a paraphrase.
//!
//! Emitting a diagnostic requires a source location, which requires a lexer.
//! There is no lexer in M0, so there is no emission API here.

use lcl_spec::json::Json;
use lcl_spec::SpecPackage;
use std::collections::BTreeMap;
use std::fmt;

/// The closed diagnostic selection stages, in normative order.
///
/// Order is significant: `stage_order` in the registry is the normative
/// sequence, and [`DiagnosticRegistry::load`] refuses to build if this enum
/// disagrees with it. Grammar and schema form one stage; verification and
/// completion form one stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stage {
    Lexical,
    GrammarOrSchema,
    Resolution,
    StaticOrExpression,
    Validation,
    Execution,
    VerificationOrCompletion,
}

impl Stage {
    /// The seven stages in normative order.
    pub const ORDER: [Stage; 7] = [
        Stage::Lexical,
        Stage::GrammarOrSchema,
        Stage::Resolution,
        Stage::StaticOrExpression,
        Stage::Validation,
        Stage::Execution,
        Stage::VerificationOrCompletion,
    ];

    pub fn as_registry_str(self) -> &'static str {
        match self {
            Stage::Lexical => "lexical",
            Stage::GrammarOrSchema => "grammar_or_schema",
            Stage::Resolution => "resolution",
            Stage::StaticOrExpression => "static_or_expression",
            Stage::Validation => "validation",
            Stage::Execution => "execution",
            Stage::VerificationOrCompletion => "verification_or_completion",
        }
    }

    pub fn from_registry_str(s: &str) -> Option<Stage> {
        Stage::ORDER
            .into_iter()
            .find(|st| st.as_registry_str() == s)
    }

    /// Zero-based position in the normative stage order.
    pub fn index(self) -> usize {
        Stage::ORDER
            .iter()
            .position(|s| *s == self)
            .expect("stage is in ORDER")
    }

    /// True when `self` is strictly earlier than `other`.
    ///
    /// The processing model advances only while the earlier applicable stage
    /// has no unhandled unsuppressed diagnostic, so stage comparison is a
    /// primitive that later milestones depend on.
    pub fn precedes(self, other: Stage) -> bool {
        self.index() < other.index()
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_registry_str())
    }
}

/// A registered status identifier and its lifecycle contract.
#[derive(Debug, Clone)]
pub struct StatusDef {
    pub id: String,
    pub meaning: String,
    pub result_meaning: String,
    pub terminal: bool,
    pub allowed_next: Vec<String>,
    pub scope: String,
}

/// A registered error identifier and its normative classification.
#[derive(Debug, Clone)]
pub struct ErrorDef {
    pub id: String,
    pub meaning: String,
    /// Registered normative stage. Per the processing model this is the default
    /// classification; the only contextual exception is the closed
    /// `expression_demand_resolution` map, which M0 does not apply.
    pub stage: Stage,
    pub recoverable_with_declared_handler: bool,
    /// Event identifier this error maps to, if any.
    pub event: Option<String>,
    pub default_status: String,
}

#[derive(Debug)]
pub enum DiagnosticsError {
    MissingRegistry(&'static str),
    Malformed(String),
    /// The registry's stage order disagrees with [`Stage::ORDER`].
    StageOrderMismatch {
        expected: Vec<String>,
        found: Vec<String>,
    },
    /// An error or status referenced something not in the closed registry.
    UnclosedReference(String),
}

impl fmt::Display for DiagnosticsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticsError::MissingRegistry(n) => write!(f, "registry {n:?} not loaded"),
            DiagnosticsError::Malformed(m) => write!(f, "malformed diagnostic registry: {m}"),
            DiagnosticsError::StageOrderMismatch { expected, found } => write!(
                f,
                "stage order mismatch: this build expects {expected:?}, registry declares {found:?}"
            ),
            DiagnosticsError::UnclosedReference(m) => write!(f, "unclosed registry reference: {m}"),
        }
    }
}

impl std::error::Error for DiagnosticsError {}

/// The registered diagnostic model, loaded from the canonical registry.
pub struct DiagnosticRegistry {
    statuses: BTreeMap<String, StatusDef>,
    errors: BTreeMap<String, ErrorDef>,
    stage_order: Vec<Stage>,
    selection_contract: Json,
    event_model: Json,
    failure_lifecycle: Json,
    check_selection_contract: Json,
}

impl DiagnosticRegistry {
    /// Build from a verified specification package.
    ///
    /// Performs closure checks the registry implies but does not restate:
    /// every error stage is a registered stage, every `default_status` is a
    /// registered status, and every `allowed_next` target is a registered
    /// status.
    pub fn load(spec: &SpecPackage) -> Result<Self, DiagnosticsError> {
        let reg = spec
            .registry("statuses_and_errors")
            .ok_or(DiagnosticsError::MissingRegistry("statuses_and_errors"))?;

        let selection = reg
            .get("diagnostic_selection")
            .ok_or_else(|| DiagnosticsError::Malformed("missing diagnostic_selection".into()))?;

        // Validate the stage order against this build's enum before anything else.
        let declared: Vec<String> = selection
            .get("stage_order")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                DiagnosticsError::Malformed("missing diagnostic_selection.stage_order".into())
            })?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        let expected: Vec<String> = Stage::ORDER
            .iter()
            .map(|s| s.as_registry_str().to_string())
            .collect();
        if declared != expected {
            return Err(DiagnosticsError::StageOrderMismatch {
                expected,
                found: declared,
            });
        }

        // Statuses.
        let mut statuses = BTreeMap::new();
        let status_obj = reg
            .get("statuses")
            .and_then(|v| v.as_object())
            .ok_or_else(|| DiagnosticsError::Malformed("missing statuses".into()))?;
        for (id, body) in status_obj {
            statuses.insert(
                id.clone(),
                StatusDef {
                    id: id.clone(),
                    meaning: str_field(body, "meaning", id)?,
                    result_meaning: str_field(body, "result_meaning", id)?,
                    terminal: body
                        .get("terminal")
                        .and_then(|v| v.as_bool())
                        .ok_or_else(|| {
                            DiagnosticsError::Malformed(format!("{id}: missing terminal"))
                        })?,
                    allowed_next: body
                        .get("allowed_next")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    scope: str_field(body, "scope", id)?,
                },
            );
        }

        // Errors.
        let mut errors = BTreeMap::new();
        let error_obj = reg
            .get("errors")
            .and_then(|v| v.as_object())
            .ok_or_else(|| DiagnosticsError::Malformed("missing errors".into()))?;
        for (id, body) in error_obj {
            let stage_str = str_field(body, "stage", id)?;
            let stage = Stage::from_registry_str(&stage_str).ok_or_else(|| {
                DiagnosticsError::UnclosedReference(format!(
                    "{id}: unregistered stage {stage_str:?}"
                ))
            })?;
            let default_status = str_field(body, "default_status", id)?;
            errors.insert(
                id.clone(),
                ErrorDef {
                    id: id.clone(),
                    meaning: str_field(body, "meaning", id)?,
                    stage,
                    recoverable_with_declared_handler: body
                        .get("recoverable_with_declared_handler")
                        .and_then(|v| v.as_bool())
                        .ok_or_else(|| {
                            DiagnosticsError::Malformed(format!("{id}: missing recoverable flag"))
                        })?,
                    event: body
                        .get("event")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    default_status,
                },
            );
        }

        let me = Self {
            statuses,
            errors,
            stage_order: Stage::ORDER.to_vec(),
            selection_contract: selection.clone(),
            event_model: reg.get("event_model").cloned().unwrap_or(Json::Null),
            failure_lifecycle: reg.get("failure_lifecycle").cloned().unwrap_or(Json::Null),
            check_selection_contract: reg
                .get("check_selection_contract")
                .cloned()
                .unwrap_or(Json::Null),
        };
        me.check_closure()?;
        Ok(me)
    }

    /// Every cross-reference resolves inside the closed registry.
    fn check_closure(&self) -> Result<(), DiagnosticsError> {
        for e in self.errors.values() {
            if !self.statuses.contains_key(&e.default_status) {
                return Err(DiagnosticsError::UnclosedReference(format!(
                    "{}: default_status {:?} is not a registered status",
                    e.id, e.default_status
                )));
            }
        }
        for s in self.statuses.values() {
            for next in &s.allowed_next {
                if !self.statuses.contains_key(next) {
                    return Err(DiagnosticsError::UnclosedReference(format!(
                        "{}: allowed_next {:?} is not a registered status",
                        s.id, next
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn stage_order(&self) -> &[Stage] {
        &self.stage_order
    }

    pub fn error(&self, id: &str) -> Option<&ErrorDef> {
        self.errors.get(id)
    }

    pub fn status(&self, id: &str) -> Option<&StatusDef> {
        self.statuses.get(id)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ErrorDef> {
        self.errors.values()
    }

    pub fn statuses(&self) -> impl Iterator<Item = &StatusDef> {
        self.statuses.values()
    }

    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    pub fn status_count(&self) -> usize {
        self.statuses.len()
    }

    /// Registered errors classified to `stage`, in identifier order.
    pub fn errors_by_stage(&self, stage: Stage) -> Vec<&ErrorDef> {
        self.errors.values().filter(|e| e.stage == stage).collect()
    }

    /// Error counts per stage, in normative stage order.
    pub fn stage_histogram(&self) -> Vec<(Stage, usize)> {
        self.stage_order
            .iter()
            .map(|s| (*s, self.errors.values().filter(|e| e.stage == *s).count()))
            .collect()
    }

    /// Errors a declared handler may recover from.
    pub fn recoverable_errors(&self) -> Vec<&ErrorDef> {
        self.errors
            .values()
            .filter(|e| e.recoverable_with_declared_handler)
            .collect()
    }

    pub fn terminal_statuses(&self) -> Vec<&StatusDef> {
        self.statuses.values().filter(|s| s.terminal).collect()
    }

    /// Raw `diagnostic_selection` contract.
    ///
    /// M0 exposes it as data and does not implement it. A later milestone must
    /// implement selection against this contract directly.
    pub fn selection_contract(&self) -> &Json {
        &self.selection_contract
    }

    pub fn event_model(&self) -> &Json {
        &self.event_model
    }

    pub fn failure_lifecycle(&self) -> &Json {
        &self.failure_lifecycle
    }

    pub fn check_selection_contract(&self) -> &Json {
        &self.check_selection_contract
    }
}

fn str_field(body: &Json, key: &str, id: &str) -> Result<String, DiagnosticsError> {
    body.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| DiagnosticsError::Malformed(format!("{id}: missing string field {key:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_order_is_total_and_normative() {
        assert_eq!(Stage::ORDER.len(), 7);
        assert!(Stage::Lexical.precedes(Stage::Execution));
        assert!(!Stage::Execution.precedes(Stage::Lexical));
        assert!(!Stage::Validation.precedes(Stage::Validation));
        for (i, s) in Stage::ORDER.iter().enumerate() {
            assert_eq!(s.index(), i);
            assert_eq!(Stage::from_registry_str(s.as_registry_str()), Some(*s));
        }
    }

    #[test]
    fn unknown_stage_string_is_rejected() {
        assert_eq!(Stage::from_registry_str("parsing"), None);
        assert_eq!(Stage::from_registry_str("grammar"), None);
    }
}
