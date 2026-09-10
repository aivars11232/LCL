//! # lcl-protocol — the stable headless engine surface
//!
//! Milestone M9 component A.
//!
//! Two things, and deliberately only two:
//!
//! 1. [`Engine`], one assembled engine that carries source through every
//!    canonical stage in order and stops where the requested [`Command`] says;
//! 2. [`Report`], one machine-readable record of what happened, with a JSON
//!    projection for a consumer that is not written in Rust.
//!
//! ## What "stable" means here
//!
//! A consumer — the CLI in this workspace, the workspace UI a later milestone
//! builds, or a third-party tool — can depend on the record's field meanings.
//! [`PROTOCOL`] names the version. Adding a field does not change it; changing
//! what an existing field means does.
//!
//! ## Why there is no transport
//!
//! A protocol crate could have shipped a socket, a JSON-RPC dispatcher or a
//! language server. It ships none, because the UI milestone has not chosen a
//! toolkit and a transport chosen now would be a guess that later work has to
//! live with. What a UI actually needs is a stable set of records and one way
//! to obtain them, and both forms are here: a Rust consumer calls [`Engine`]
//! directly, and any other consumer runs the CLI and reads
//! [`Report::to_json`]. Both paths produce the same records from the same
//! code. A transport can be added later without changing either.
//!
//! ## What this crate must never become
//!
//! The implementation contract is explicit that "CLI and UI consume engine
//! APIs/protocols. Neither may contain a second private implementation of
//! language semantics." That applies here first: this crate resolves nothing,
//! checks nothing, evaluates nothing and classifies nothing. Every identifier,
//! stage, status, span and value in a report is copied from the layer that
//! decided it. The one thing it decides is where a command stops, which is a
//! product boundary rather than a language rule.

pub mod engine;
pub mod inputs;
pub mod json;
pub mod record;

pub use engine::{Engine, EngineError};
pub use inputs::{Inputs, Supplied};
pub use json::{Node, Object};
pub use record::{
    CheckRecord, Command, CompletionRecord, DiagnosticRecord, EventRecord, EvidenceRecord,
    ExecutionRecord, ImportRecord, InputRecord, InvocationRecord, Outcome, OutputRecord,
    PlanRecord, Reached, Report, SourceRecord, SpecRecord, StructureRecord, VerdictRecord,
    PROTOCOL,
};
