//! The deterministic mock host.
//!
//! Conformance needs a host that is *the same host* on every run, on every
//! machine, in every order: the implementation contract requires "deterministic
//! mock/in-memory implementations" as a mandatory part of the capability layer,
//! and this milestone's acceptance criteria are stated over "the same program,
//! inputs and mock capabilities".
//!
//! So this host has no clock, no filesystem, no process, no network and no
//! randomness. Its answer to a request is a pure function of the request and of
//! how many times that operation has already been asked — which is exactly what
//! a retry test needs, since `CLOSURE-022` is "initial failure then successful
//! first retry" and that is a *sequence*, not a coin flip.
//!
//! ## What it is not
//!
//! It is not the standard library. M7 owns "every executable Core
//! operation/function contract" and the real adapters. This host produces
//! schema-shaped observations so that M6's own subject — control flow,
//! handlers, retries, concurrency and the boundary itself — can be executed and
//! observed. A default observation here is a placeholder with the registered
//! shape, never a claim about what `core.download` really does.

use crate::capability::{CapabilityOutcome, CapabilityRequest, Host, Observation, Permission};
use crate::result::{EffectClass, ObservedEffect, RecordState};
use crate::value::Value;
use lcl_checker::numeric::{Decimal, Integer};
use std::collections::BTreeMap;

/// A deterministic, in-memory [`Host`].
#[derive(Debug, Default)]
pub struct MockHost {
    /// Every request that crossed the boundary, in order.
    log: Vec<CapabilityRequest>,
    /// Every delay the runtime handed over, in order.
    delays: Vec<Value>,
    /// Per-operation permission override.
    permissions: BTreeMap<String, Permission>,
    /// Per-operation scripted outcomes, consumed in order. Once exhausted, the
    /// default outcome applies.
    scripted: BTreeMap<String, Vec<CapabilityOutcome>>,
    /// How many times each operation has been invoked.
    counts: BTreeMap<String, usize>,
}

impl MockHost {
    /// A host that permits everything and completes every operation.
    pub fn new() -> MockHost {
        MockHost::default()
    }

    /// Refuse one operation. The runtime raises `error.permission.denied`.
    pub fn deny(mut self, operation: impl Into<String>, reason: impl Into<String>) -> MockHost {
        self.permissions
            .insert(operation.into(), Permission::Denied(reason.into()));
        self
    }

    /// Make one operation unavailable. The runtime raises
    /// `error.host.constraint`.
    pub fn unavailable(
        mut self,
        operation: impl Into<String>,
        reason: impl Into<String>,
    ) -> MockHost {
        self.permissions
            .insert(operation.into(), Permission::Unavailable(reason.into()));
        self
    }

    /// Script successive outcomes for one operation, in invocation order.
    ///
    /// The first invocation gets `outcomes[0]`, the second `outcomes[1]`, and
    /// so on; after the list is exhausted the default outcome applies. This is
    /// how an attempt sequence is expressed without any nondeterminism.
    pub fn script(
        mut self,
        operation: impl Into<String>,
        outcomes: Vec<CapabilityOutcome>,
    ) -> MockHost {
        self.scripted.insert(operation.into(), outcomes);
        self
    }

    /// Every request that crossed the boundary, in order.
    pub fn requests(&self) -> &[CapabilityRequest] {
        &self.log
    }

    /// Every delay handed over, in order.
    pub fn delays(&self) -> &[Value] {
        &self.delays
    }

    /// How many times one operation was invoked.
    pub fn count(&self, operation: &str) -> usize {
        self.counts.get(operation).copied().unwrap_or(0)
    }

    /// A completed outcome with no fields and no effects.
    pub fn completed() -> CapabilityOutcome {
        CapabilityOutcome::Completed(Observation::none())
    }

    /// A failure that is proven to have caused no effect.
    ///
    /// The `pre_effect_rule` in `retry_safety` permits another attempt only
    /// after "A failed attempt proven pre_effect with effect_state none", so a
    /// retry fixture needs exactly this shape.
    pub fn failed_before_effect(detail: impl Into<String>) -> CapabilityOutcome {
        CapabilityOutcome::Failed {
            detail: detail.into(),
            observation: Observation::none(),
        }
    }

    /// A failure after a known applied effect.
    pub fn failed_after_effect(
        detail: impl Into<String>,
        class: EffectClass,
        target: impl Into<String>,
    ) -> CapabilityOutcome {
        CapabilityOutcome::Failed {
            detail: detail.into(),
            observation: Observation::none().with_effect(ObservedEffect {
                class,
                state: RecordState::Applied,
                target: Some(target.into()),
                evidence: Vec::new(),
            }),
        }
    }

    /// The schema-shaped observation this host returns by default.
    ///
    /// Every field is the one the registry marks required on producer success
    /// for that schema, so a default result satisfies the schema's own
    /// constraints rather than an invented shape.
    fn default_observation(request: &CapabilityRequest) -> Observation {
        let zero = || Value::Integer(Decimal::from_integer(Integer::zero()));
        let target_text = request
            .target
            .as_ref()
            .map(|t| t.to_string())
            .unwrap_or_default();

        let observation = match request.result_schema.as_str() {
            "result.value" => Observation::none()
                .with("value", Value::Text(format!("mock:{}", request.operation))),
            "result.collection" => Observation::none()
                .with("items", Value::List(Vec::new()))
                .with("count", zero()),
            "result.operation" => Observation::none().with("changed", Value::Boolean(true)),
            "result.command" => Observation::none()
                .with("mode", Value::Identifier("non_graph".to_string()))
                .with("started", Value::Boolean(true))
                .with("completed", Value::Boolean(true))
                .with("exit_code", zero())
                .with("stdout", Value::Text(String::new()))
                .with("stderr", Value::Text(String::new())),
            "result.validation" => Observation::none()
                .with("valid", Value::Boolean(true))
                .with("errors", Value::List(Vec::new())),
            "result.verification" => Observation::none()
                .with("verified", Value::Boolean(true))
                .with("observed", Value::List(Vec::new()))
                .with("errors", Value::List(Vec::new()))
                .with("evidence", Value::List(Vec::new())),
            "result.test" => Observation::none().with("passed", Value::Boolean(true)),
            "result.message" => Observation::none()
                .with("delivered", Value::Boolean(true))
                .with("recipient", Value::Text(target_text.clone())),
            "result.transfer" => Observation::none()
                .with("source", Value::Text(target_text.clone()))
                .with(
                    "destination",
                    request
                        .parameters
                        .get("destination")
                        .cloned()
                        .unwrap_or(Value::Null),
                )
                .with(
                    "bytes",
                    Value::Bytes(Decimal::from_integer(Integer::zero())),
                ),
            // An unregistered schema is not this host's to invent. It returns
            // no field, and the runtime's schema check reports the gap.
            _ => Observation::none(),
        };

        // A read-only operation has `possible_effects` exactly `{none}`, which
        // the axes loader represents as an empty set, so this records an effect
        // only where the operation's own contract admits one.
        match request
            .possible_effects
            .iter()
            .filter_map(|c| EffectClass::from_registry_str(c))
            .next()
        {
            Some(class) => observation.with_effect(ObservedEffect {
                class,
                state: RecordState::Applied,
                target: request.target.as_ref().map(|t| t.to_string()),
                evidence: Vec::new(),
            }),
            None => observation,
        }
    }
}

impl Host for MockHost {
    fn permits(&mut self, request: &CapabilityRequest) -> Permission {
        self.permissions
            .get(&request.operation)
            .cloned()
            .unwrap_or(Permission::Granted)
    }

    fn invoke(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        self.log.push(request.clone());
        let count = self.counts.entry(request.operation.clone()).or_insert(0);
        let index = *count;
        *count += 1;

        if let Some(scripted) = self.scripted.get(&request.operation) {
            if let Some(outcome) = scripted.get(index) {
                return outcome.clone();
            }
        }
        CapabilityOutcome::Completed(MockHost::default_observation(request))
    }

    fn delay(&mut self, duration: &Value) {
        self.delays.push(duration.clone());
    }
}
