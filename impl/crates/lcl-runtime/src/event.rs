//! Events, and the single thing that can raise one.
//!
//! Authority: `statuses_and_errors_v0.1.0.json#/event_model` and
//! `05_SEMANTICS/06_MISSING_UNKNOWN_NULL_DEFAULT_ASSUME_AND_HANDLER_RESOLUTION.txt`.
//!
//! ## Only a diagnostic raises an event
//!
//! > Emission of a diagnostic is the only producer of an event in Core 0.1.0.
//! > No timer, host signal, external notification, or successful completion
//! > raises an event.
//!
//! This module has no constructor that takes an event identifier on its own.
//! The only way to obtain an [`EventRecord`] is [`EventLog::raise`], which
//! takes the *diagnostic* that raised it and copies the identifier from that
//! diagnostic's registered mapping. A host cannot inject one, and neither can a
//! successful completion, because neither has a diagnostic to hand over.
//!
//! ## An event carries nothing
//!
//! > An event exists only as the activation condition of a declared HANDLER and
//! > carries no value of its own.
//!
//! So [`EventRecord`] holds the identifier, the producer that emitted the
//! diagnostic, and the occurrence index — provenance, not payload. A handler
//! reads the failed attempt's result through event evidence, not through the
//! event.
//!
//! ## Distinct occurrences
//!
//! > Distinct eligible diagnostics raise distinct event occurrences even when
//! > the canonical event identifier is equal.
//!
//! Hence [`EventRecord::occurrence`]: two `event.missing` occurrences from two
//! producers are two events, each selecting its own handler at most once.

use crate::state::InvocationId;
use std::fmt;

/// One raised event occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    /// The canonical core event identifier, copied from the raising
    /// diagnostic's registered `event` field.
    pub event: String,
    /// Index into the execution's diagnostic list of the diagnostic that raised
    /// it. The event is provenance for that diagnostic and nothing else.
    pub diagnostic: usize,
    /// The producer that emitted the diagnostic. `handler_scope` is decided
    /// from the declared producer path, "never by host call order".
    pub producer: InvocationId,
    /// Zero-based occurrence index across the whole execution, so two events
    /// with the same identifier remain distinguishable.
    pub occurrence: usize,
    /// What handler selection did with it.
    pub disposition: Disposition,
}

/// What became of one raised event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    /// Selection has not run yet.
    Pending,
    /// `no_match_rule`: "When no candidate matches, the diagnostic remains
    /// unhandled ... A raised event that matches no handler is not itself a
    /// diagnostic and adds no identifier."
    NoMatch,
    /// The first matching candidate under `selection_order` was activated.
    Selected {
        /// The selected `HANDLER` declaration's qualified id.
        handler: String,
        /// True exactly when the handler invocation, including any permitted
        /// successful `FALLBACK` substitution, recorded `status.succeeded`.
        recovered: bool,
    },
    /// `non_reentrancy_rule` suppressed the event: it was raised while
    /// evaluating a candidate `WHEN`, resolving a handler invocation contract,
    /// or executing a handler or its `FALLBACK` for the same originating
    /// diagnostic.
    Suppressed,
}

impl fmt::Display for Disposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Disposition::Pending => f.write_str("pending"),
            Disposition::NoMatch => f.write_str("no matching handler"),
            Disposition::Selected {
                handler,
                recovered: true,
            } => write!(f, "{handler} recovered it"),
            Disposition::Selected {
                handler,
                recovered: false,
            } => write!(f, "{handler} did not recover it"),
            Disposition::Suppressed => f.write_str("suppressed by non-reentrancy"),
        }
    }
}

impl fmt::Display for EventRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} #{} at {} -> {}",
            self.event, self.occurrence, self.producer, self.disposition
        )
    }
}

/// Every event this execution raised, in the order they were raised.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventLog {
    records: Vec<EventRecord>,
}

impl EventLog {
    pub fn new() -> EventLog {
        EventLog::default()
    }

    /// Raise the event a surviving diagnostic maps to.
    ///
    /// `event` is the diagnostic's registered mapping: `None` means "A null
    /// mapping ... raises nothing further", and this returns `None` without
    /// recording anything. There is deliberately no way to raise an event for a
    /// diagnostic whose registered mapping is null.
    pub fn raise(
        &mut self,
        event: Option<&str>,
        diagnostic: usize,
        producer: InvocationId,
    ) -> Option<usize> {
        let event = event?;
        let occurrence = self.records.len();
        self.records.push(EventRecord {
            event: event.to_string(),
            diagnostic,
            producer,
            occurrence,
            disposition: Disposition::Pending,
        });
        Some(occurrence)
    }

    /// Record what selection did with one occurrence.
    pub fn dispose(&mut self, occurrence: usize, disposition: Disposition) {
        if let Some(record) = self.records.get_mut(occurrence) {
            record.disposition = disposition;
        }
    }

    pub fn records(&self) -> &[EventRecord] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// True when the diagnostic at `index` was recovered by a selected handler.
    ///
    /// `recovery_rule`: "The originating diagnostic is recovered exactly when
    /// that handler invocation's own result, including permitted successful
    /// FALLBACK substitution, records status.succeeded. Any other handler
    /// outcome leaves the originating diagnostic unhandled."
    pub fn recovered(&self, index: usize) -> bool {
        self.records.iter().any(|r| {
            r.diagnostic == index
                && matches!(
                    r.disposition,
                    Disposition::Selected {
                        recovered: true,
                        ..
                    }
                )
        })
    }
}
