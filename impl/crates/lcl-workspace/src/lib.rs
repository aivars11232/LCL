//! # lcl-workspace — the editor, project shell, inspector and debugger
//!
//! Milestone M10.
//!
//! ## What it is
//!
//! The product a person uses to write, run, inspect and debug LCL. It is a
//! loopback HTTP server serving a browser frontend, both written here, both on
//! `std` alone.
//!
//! ## The one rule everything else follows from
//!
//! The implementation contract is explicit: "CLI and UI consume engine
//! APIs/protocols. Neither may contain a second private implementation of
//! language semantics. Every displayed diagnostic/result must be reproducible
//! through the engine."
//!
//! So this crate does not tokenize, parse, resolve, check, plan or evaluate
//! anything. It does not decide what a diagnostic means, where a span is, what
//! a reference points at, or whether a document is valid. Every one of those
//! arrives from [`lcl_protocol`], and this crate's job is to put it on a
//! screen.
//!
//! **The frontend does not tokenize either.** A JavaScript regular-expression
//! highlighter would be a second lexer living in a UI, so there is not one: the
//! browser asks for token spans, the real [`lcl_lexer`] produces them, and the
//! browser paints them. This is the difference between a syntax theme and a
//! second implementation of the language, and it is worth the round trip.
//!
//! ## Byte offsets stay normative
//!
//! `02_LEXICAL/01` makes source bytes normative, and every engine crate treats
//! its derived line and column as presentation. That survives the whole way
//! out: spans cross the wire as byte offsets, the engine's own derived position
//! travels beside them, and the frontend maps bytes to screen positions with an
//! index it builds once per document rather than by counting characters itself.

pub mod document;
pub mod execution;
pub mod http;
pub mod intelligence;
pub mod project;
pub mod routes;
pub mod server;

pub use document::{Document, DocumentError};
pub use project::{Entry, Workspace, WorkspaceError};
pub use routes::Routes;
pub use server::{Outcome, Route, Server};
