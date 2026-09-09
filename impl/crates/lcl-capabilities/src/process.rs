//! The process capability: running a program, within a declared bound.
//!
//! ## Where the clock lives
//!
//! A `TIMEOUT` is a declared `DURATION`, which `05_SEMANTICS/11` permits to
//! influence execution precisely because it is "explicitly supplied as an
//! input". Waiting for it is the host's business, and this is the only place in
//! the implementation that consults a clock at all: the runtime core never
//! does, so the deterministic scheduler above it stays meaningful.
//!
//! Exceeding the bound produces [`Cancelled`], which the standard library maps
//! to `error.host.constraint` — a limitation that "never changes LCL meaning".
//! A timed-out command does not become a different command with a different
//! result; it becomes the same command the host could not finish in time.
//!
//! ## Output is captured, and capped
//!
//! A program can write more than memory holds. Both streams are truncated at
//! [`Bounds::max_stream_bytes`], and truncation is reported rather than hidden,
//! because a silently shortened `stdout` would be a wrong answer that looked
//! like a right one.

use crate::bounds::{Bounds, Cancelled};
use crate::grant::{Grant, Grants, Refusal};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// What one process request could not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError {
    /// The program could not be started at all.
    NotStarted(String),
    /// The host refuses or cannot supply the capability.
    Refused(Refusal),
    /// A declared bound stopped the work.
    Bounded(Cancelled),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::NotStarted(detail) => f.write_str(detail),
            ProcessError::Refused(refusal) => write!(f, "{refusal}"),
            ProcessError::Bounded(cancelled) => write!(f, "{cancelled}"),
        }
    }
}

/// One program to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
    /// The exact environment, in sorted order. An empty map is an empty
    /// environment, not an inherited one: an inherited environment would make
    /// the result depend on ambient state the document never declared.
    pub environment: BTreeMap<String, String>,
}

/// What running one program produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub started: bool,
    pub completed: bool,
    /// `None` when the program was stopped before it reported one.
    pub exit_code: Option<i64>,
    pub stdout: String,
    pub stderr: String,
    /// True when a stream reached its cap and was cut short.
    pub truncated: bool,
}

/// The primitive process capability.
pub trait Process {
    /// Run one program to completion, or until a bound stops it.
    fn run(&mut self, command: &Command, bounds: &Bounds) -> Result<Completion, ProcessError>;
}

/// The real process capability, confined to granted programs.
#[derive(Debug, Clone)]
pub struct RealProcess {
    grants: Grants,
}

impl RealProcess {
    pub fn new(grants: Grants) -> RealProcess {
        RealProcess { grants }
    }
}

impl Process for RealProcess {
    fn run(&mut self, command: &Command, bounds: &Bounds) -> Result<Completion, ProcessError> {
        self.grants
            .decide(&Grant::RunProgram(command.program.clone()))
            .map_err(ProcessError::Refused)?;
        if let Some(directory) = &command.working_directory {
            self.grants
                .decide(&Grant::ReadPath(directory.clone()))
                .map_err(ProcessError::Refused)?;
        }

        let mut process = std::process::Command::new(&command.program);
        process
            .args(&command.arguments)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            // A declared environment is the whole environment. Inheriting the
            // host's would make the result depend on undeclared state.
            .env_clear()
            .envs(&command.environment);
        if let Some(directory) = &command.working_directory {
            process.current_dir(directory);
        }

        let child = process
            .spawn()
            .map_err(|error| ProcessError::NotStarted(error.to_string()))?;
        wait(child, bounds)
    }
}

/// Wait for a child, honouring a declared deadline.
fn wait(mut child: std::process::Child, bounds: &Bounds) -> Result<Completion, ProcessError> {
    let Some(deadline) = bounds.deadline else {
        let output = child
            .wait_with_output()
            .map_err(|error| ProcessError::NotStarted(error.to_string()))?;
        return Ok(completion(
            output.status.code().map(|code| code as i64),
            output.stdout,
            output.stderr,
            bounds,
        ));
    };

    // Poll rather than block, so the declared bound can end the wait. The
    // interval is small enough to be imperceptible and large enough not to spin.
    let limit = deadline.as_duration();
    let started = std::time::Instant::now();
    let interval = std::time::Duration::from_millis(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|error| ProcessError::NotStarted(error.to_string()))?;
                return Ok(completion(
                    status.code().map(|code| code as i64),
                    output.stdout,
                    output.stderr,
                    bounds,
                ));
            }
            Ok(None) => {}
            Err(error) => return Err(ProcessError::NotStarted(error.to_string())),
        }
        if started.elapsed() >= limit {
            // The program began and did not finish. Killing it is the bound
            // being enforced, and the caller is told the truth about both.
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProcessError::Bounded(Cancelled::timed_out(deadline)));
        }
        std::thread::sleep(interval);
    }
}

fn completion(
    exit_code: Option<i64>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    bounds: &Bounds,
) -> Completion {
    let cap = bounds.max_stream_bytes as usize;
    let truncated = stdout.len() > cap || stderr.len() > cap;
    Completion {
        started: true,
        completed: true,
        exit_code,
        stdout: capped(stdout, cap),
        stderr: capped(stderr, cap),
        truncated,
    }
}

fn capped(bytes: Vec<u8>, cap: usize) -> String {
    let slice = if bytes.len() > cap {
        &bytes[..cap]
    } else {
        &bytes[..]
    };
    String::from_utf8_lossy(slice).to_string()
}

/// Whether one path is a plausible program location, for a caller that needs to
/// distinguish a command string from a path.
pub fn is_program_path(candidate: &Path) -> bool {
    candidate.is_absolute() || candidate.components().count() > 1
}

/// The human capability: obtaining one authoritative answer.
///
/// `core.ask` is the one row whose dependency is a person. It is a capability
/// like any other — installed explicitly, refused when ungranted — and the
/// answer it returns is data, never a decision about the language.
pub trait Responder {
    /// Ask one question, optionally constrained to a closed set of options.
    ///
    /// `Ok(None)` is a legitimate outcome: "when no authorized valid answer is
    /// provided, the answer remains MISSING and uses error.required.missing".
    fn ask(&mut self, question: &str, options: &[String]) -> Result<Option<String>, ProcessError>;
}
