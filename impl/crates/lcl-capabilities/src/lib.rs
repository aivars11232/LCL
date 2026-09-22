//! # lcl-capabilities — the host effect boundary
//!
//! Milestone M7, host side.
//!
//! This crate is deliberately **not** part of the language. It holds no LCL
//! value model, no evaluator, no diagnostic selection and no operation
//! semantics. What it holds is everything the implementation contract calls the
//! host side of the boundary:
//!
//! > `lcl-capabilities` Defines the host effect boundary. Real filesystem /
//! > process / network / provider / model adapters live behind it.
//! > Deterministic mock/in-memory implementations are mandatory for
//! > conformance.
//!
//! ## Why it sits below the runtime, not above it
//!
//! It depends only on `lcl-spec` and `lcl-diagnostics`. That is not tidiness:
//! `10_REGISTRIES/operations_v0.1.0.json#/axis_contract/implementation_profile`
//! makes profile selection a **pre-effect** decision that also "supplies the
//! resolved determinism category", and the determinism category is needed by
//! validation — an earlier stage than execution. A profile catalog that lived
//! above the runtime could never reach it. Sitting here, one catalog serves
//! both the preflight that validates `DETERMINISTIC TRUE` and the standard
//! library that selects profiles before effects.
//!
//! ## What crosses, and in which direction
//!
//! The language asks in primitives — a path, a byte range, a program and its
//! arguments — and an adapter answers in primitives. No adapter sees an LCL
//! value, an LCL status or an LCL error identifier, so none can decide one.
//! Translating between the two vocabularies is `lcl-stdlib`'s job, and it is
//! the only place that translation exists.

pub mod address;
pub mod bounds;
pub mod fs;
pub mod grant;
pub mod net;
pub mod process;
pub mod profile;

pub use address::{verify_vocabulary, AddressClass, Axes, Dependency, Effect, VocabularyMismatch};
pub use bounds::{Bounds, Cancelled, Deadline};
pub use fs::{Copied, FileSystem, FsError, Location, Metadata, RealFileSystem, WriteMode};
pub use grant::{contains, normalize, Grant, Grants, Refusal, Scope};
pub use net::{Address, NetError, Response, TcpTransport, Transport};
pub use process::{Command, Completion, Process, ProcessError, RealProcess, Responder};
pub use profile::{
    Determinism, Profile, ProfileCatalog, ProfileError, ProfileFault, Role, RoleRequirement, Row,
    RowDeterminism, Selection, TargetClass,
};
