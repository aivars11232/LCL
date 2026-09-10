//! Exit codes: a closed table, and what each one promises.
//!
//! ## Why these are a product decision, and are still worth pinning
//!
//! LCL Core defines statuses, not process exit codes. Nothing canonical says
//! what a shell should see. But a script that runs `lcl check` in a pipeline
//! needs an answer it can branch on, and an answer that changes between builds
//! is worse than no answer. So the table is closed, documented, and tested.
//!
//! ## The distinctions the table is built on
//!
//! Three failures that a single non-zero code would blur together:
//!
//! * a **rejected document** is a language verdict — the source did not survive
//!   a stage, and the diagnostics say which;
//! * a **non-success terminal status** means the document ran to completion and
//!   the invocation did not succeed, which `05_SEMANTICS/10` treats as an
//!   entirely ordinary outcome rather than an error;
//! * a **usage or environment failure** means the tool never got far enough to
//!   judge anything.
//!
//! A pipeline that treats "your program failed its VERIFY" the same as "you
//! misspelled a flag" cannot report either accurately.

use lcl_protocol::{Outcome, Report};

/// The requested work completed and nothing failed.
///
/// For `run`, this additionally means the one terminal status was
/// `status.succeeded`.
pub const SUCCESS: i32 = 0;

/// The document was rejected by an LCL diagnostic before the requested stage
/// finished.
pub const REJECTED: i32 = 1;

/// The document ran to completion with a terminal status other than
/// `status.succeeded`.
pub const NOT_SUCCEEDED: i32 = 2;

/// The command line was not usable: an unknown option, a missing argument, or
/// an input the engine could not turn into a value.
pub const USAGE: i32 = 3;

/// The environment was not usable: no specification package, an unreadable
/// document, a project that would not open.
pub const ENVIRONMENT: i32 = 4;

/// The exit code one report earns.
pub fn of(report: &Report) -> i32 {
    match report.outcome {
        Outcome::Refused => USAGE,
        Outcome::Rejected => REJECTED,
        Outcome::Accepted => match report.terminal_status() {
            // Only a `run` carries a terminal status. A `check` or `validate`
            // that reached its own last stage cleanly is a success, and saying
            // otherwise would invent a verdict the command never produced.
            None => SUCCESS,
            Some("status.succeeded") => SUCCESS,
            Some(_) => NOT_SUCCEEDED,
        },
    }
}

/// The table, for `lcl help` and for the test that pins it.
pub const TABLE: &[(i32, &str)] = &[
    (
        SUCCESS,
        "the requested work completed; a run also succeeded",
    ),
    (REJECTED, "the document was rejected by a diagnostic"),
    (
        NOT_SUCCEEDED,
        "the document ran and its terminal status was not status.succeeded",
    ),
    (USAGE, "the command line or a supplied input was not usable"),
    (
        ENVIRONMENT,
        "the specification, project or document could not be read",
    ),
];
