//! Resource bounds for host work: deadlines, size caps and cancellation.
//!
//! ## Why bounds live on the host side
//!
//! `05_SEMANTICS/11` keeps wall-clock timing out of language meaning unless it
//! is "explicitly supplied as an input", and the execution contract repeats it:
//! observable meaning must not depend on "wall-clock timing unless explicitly
//! supplied as an input". A `TIMEOUT` parameter *is* such an explicit input, so
//! a deadline is carried here as a declared quantity that an adapter enforces —
//! never as a clock the language reads.
//!
//! The consequence is exact: exceeding a bound is a **host limitation**, and a
//! host limitation "produces error.host.constraint and never changes LCL
//! meaning". A timed-out `core.execute` does not become a different operation
//! with a different result; it becomes the same operation reporting that the
//! host could not complete it within the declared bound.
//!
//! ## Why every bound has a finite default
//!
//! An unbounded read is a denial-of-service surface reachable from a document
//! that merely names a large target. Every bound below therefore has a finite
//! default, and a caller raises one deliberately rather than discovering the
//! absence of one at runtime.

use std::fmt;
use std::time::Duration;

/// A declared, explicit time limit for one unit of host work.
///
/// Constructed from a declared `DURATION`, never from a clock reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Deadline {
    nanos: u128,
}

impl Deadline {
    pub fn from_nanos(nanos: u128) -> Deadline {
        Deadline { nanos }
    }

    pub fn nanos(self) -> u128 {
        self.nanos
    }

    /// The bound as a `std::time::Duration`, saturating at its maximum.
    ///
    /// Saturation is not an approximation of the declared value: a duration
    /// beyond `u64::MAX` seconds is longer than any process this adapter will
    /// outlive, so both spellings mean "do not stop waiting".
    pub fn as_duration(self) -> Duration {
        let secs = (self.nanos / 1_000_000_000).min(u64::MAX as u128) as u64;
        let sub = (self.nanos % 1_000_000_000) as u32;
        Duration::new(secs, sub)
    }

    pub fn is_zero(self) -> bool {
        self.nanos == 0
    }
}

impl fmt::Display for Deadline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_duration())
    }
}

/// The finite envelope one host request runs inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounds {
    /// A declared timeout, when the invocation supplies one.
    pub deadline: Option<Deadline>,
    /// Maximum bytes one read or transfer may produce.
    pub max_bytes: u64,
    /// Maximum bytes captured from one stream of a process.
    pub max_stream_bytes: u64,
    /// Maximum entries one structural listing may return.
    pub max_entries: u64,
    /// Maximum structural depth one inspection may descend.
    ///
    /// The registry's own `core.inspect` bound is `0..100`; this is the host's
    /// independent ceiling, and the tighter of the two applies.
    pub max_depth: u64,
}

impl Bounds {
    /// Conservative defaults: large enough for real work, finite in every axis.
    pub fn new() -> Bounds {
        Bounds {
            deadline: None,
            max_bytes: 64 * 1024 * 1024,
            max_stream_bytes: 8 * 1024 * 1024,
            max_entries: 100_000,
            max_depth: 100,
        }
    }

    pub fn with_deadline(mut self, deadline: Option<Deadline>) -> Bounds {
        self.deadline = deadline;
        self
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Bounds {
        self.max_bytes = max_bytes;
        self
    }

    pub fn with_max_stream_bytes(mut self, max_stream_bytes: u64) -> Bounds {
        self.max_stream_bytes = max_stream_bytes;
        self
    }

    pub fn with_max_entries(mut self, max_entries: u64) -> Bounds {
        self.max_entries = max_entries;
        self
    }

    pub fn with_max_depth(mut self, max_depth: u64) -> Bounds {
        self.max_depth = max_depth;
        self
    }

    /// Check one measured quantity against a bound.
    pub fn check(&self, measured: u64, limit: u64, what: &str) -> Result<(), Cancelled> {
        if measured > limit {
            return Err(Cancelled::exceeded(what, measured, limit));
        }
        Ok(())
    }
}

impl Default for Bounds {
    fn default() -> Bounds {
        Bounds::new()
    }
}

/// Why bounded host work stopped before finishing.
///
/// This is a host limitation, so the standard library maps it to
/// `error.host.constraint`. It never carries an LCL status or error identifier:
/// deciding those is the language's, and an adapter has no field to write one
/// into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cancelled {
    pub reason: String,
}

impl Cancelled {
    pub fn new(reason: impl Into<String>) -> Cancelled {
        Cancelled {
            reason: reason.into(),
        }
    }

    /// A measured quantity past its bound.
    pub fn exceeded(what: &str, measured: u64, limit: u64) -> Cancelled {
        Cancelled::new(format!(
            "{what} reached {measured}, past the bound of {limit}"
        ))
    }

    /// A declared deadline that elapsed.
    pub fn timed_out(deadline: Deadline) -> Cancelled {
        Cancelled::new(format!("the declared timeout of {deadline} elapsed"))
    }
}

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}
