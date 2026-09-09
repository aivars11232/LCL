//! Deterministic in-memory capabilities.
//!
//! The implementation contract makes these mandatory rather than convenient:
//!
//! > Deterministic mock/in-memory implementations are mandatory for
//! > conformance.
//!
//! and the execution contract says why: for the same canonical version, source
//! bytes, inputs, state and capabilities, observable meaning must not depend on
//! "filesystem enumeration order", "thread scheduling" or "wall-clock timing".
//! A fixture that consulted the real machine could not give the same answer
//! twice on two machines, so none of these consults it. Each is a pure function
//! of what it was given and what has already been asked of it.

pub mod fs;
pub mod net;
pub mod process;

pub use fs::MemoryFileSystem;
pub use net::{Exchange, MemoryTransport};
pub use process::{MemoryProcess, MemoryResponder};
