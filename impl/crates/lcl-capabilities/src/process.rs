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
//! ## Output is drained while the program runs, and capped while it is read
//!
//! A pipe holds one buffer, and a program that fills it blocks in `write`
//! until someone reads. Both streams are drained from the moment the child
//! starts, for the whole time it is supervised. On Linux x86_64 this is one
//! nonblocking loop, including after direct-child exit, with an owned process
//! group and an explicit one-second teardown allowance. Waiting
//! for exit first and reading afterwards is what made a healthy command that
//! merely printed a lot look like a command that timed out.
//!
//! A program can also write more than memory holds. Each stream retains at
//! most [`Bounds::max_stream_bytes`] and keeps draining past that point
//! without keeping the excess, so the bound holds during collection rather
//! than after it, and the child still never blocks. Truncation is reported
//! rather than hidden, because a silently shortened `stdout` would be a wrong
//! answer that looked like a right one.

use crate::bounds::{Bounds, Cancelled};
use crate::grant::{Grant, Grants, Refusal};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux;

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

/// A failed request and the observations actually available to its caller.
/// The legacy `run` method retains its error type; effect-aware callers use
/// `run_observed` so an already-started process is never reported as pre-effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessFailure {
    pub error: ProcessError,
    pub observation: Option<Completion>,
    /// Whether this adapter actually observed deadline expiry.
    pub timed_out: bool,
    /// None when a legacy adapter supplied no cleanup evidence.
    pub cleanup_complete: Option<bool>,
}

impl From<ProcessError> for ProcessFailure {
    fn from(error: ProcessError) -> Self {
        ProcessFailure {
            error,
            observation: None,
            timed_out: false,
            cleanup_complete: None,
        }
    }
}

/// The primitive process capability.
pub trait Process {
    /// Run one program to completion, or until a bound stops it.
    fn run(&mut self, command: &Command, bounds: &Bounds) -> Result<Completion, ProcessError>;

    /// Run with explicit post-start observations where the adapter has them.
    fn run_observed(
        &mut self,
        command: &Command,
        bounds: &Bounds,
    ) -> Result<Completion, ProcessFailure> {
        self.run(command, bounds).map_err(Into::into)
    }
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
        self.run_observed(command, bounds)
            .map_err(|failure| failure.error)
    }

    fn run_observed(
        &mut self,
        command: &Command,
        bounds: &Bounds,
    ) -> Result<Completion, ProcessFailure> {
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

        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            match linux::run(
                &mut process,
                bounds.deadline.map(|deadline| deadline.as_duration()),
                bounds.max_stream_bytes,
            ) {
                Ok(output) => Ok(observed_output(output, true)),
                Err(failure) => Err(ProcessFailure {
                    error: if failure.started {
                        ProcessError::Bounded(Cancelled::new(failure.detail))
                    } else {
                        ProcessError::NotStarted(failure.detail)
                    },
                    observation: failure
                        .started
                        .then(|| observed_output(failure.output, false)),
                    timed_out: failure.timed_out,
                    cleanup_complete: Some(failure.cleanup_complete),
                }),
            }
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let child = process
                .spawn()
                .map_err(|error| ProcessError::NotStarted(error.to_string()))?;
            wait(child, bounds).map_err(Into::into)
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn observed_output(output: linux::Output, completed: bool) -> Completion {
    Completion {
        started: true,
        completed,
        exit_code: output.exit_code,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        truncated: output.truncated,
    }
}

/// Wait for a child, draining both streams and honouring a declared deadline.
///
/// The two readers start before the wait does. That ordering is the whole
/// repair: a child that writes more than a pipe buffer holds blocks until it
/// is read, and a supervisor that waits for exit before reading waits for
/// something that cannot happen.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn wait(mut child: std::process::Child, bounds: &Bounds) -> Result<Completion, ProcessError> {
    let cap = bounds.max_stream_bytes as usize;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let out_reader = std::thread::spawn(move || match out {
        Some(stream) => drain(stream, cap),
        None => Drained::default(),
    });
    let err_reader = std::thread::spawn(move || match err {
        Some(stream) => drain(stream, cap),
        None => Drained::default(),
    });

    let status = match bounds.deadline {
        None => child
            .wait()
            .map_err(|error| ProcessError::NotStarted(error.to_string()))?,
        Some(deadline) => {
            // Poll rather than block, so the declared bound can end the wait.
            // The interval is small enough to be imperceptible and large enough
            // not to spin. The readers are running the whole time.
            let limit = deadline.as_duration();
            let started = std::time::Instant::now();
            let interval = std::time::Duration::from_millis(5);
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {}
                    Err(error) => {
                        // Reap what can be reaped before giving up on it.
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(ProcessError::NotStarted(error.to_string()));
                    }
                }
                if started.elapsed() >= limit {
                    // The program began and did not finish. Killing it is the
                    // bound being enforced, and the caller is told the truth
                    // about both. The readers are left to end on their own as
                    // the pipes close: joining them here would reintroduce
                    // exactly the wait this repair removed, for output that a
                    // timed-out call does not return.
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(ProcessError::Bounded(Cancelled::timed_out(deadline)));
                }
                std::thread::sleep(interval);
            }
        }
    };

    // The child has exited, so both pipes are closing and both reads end.
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    Ok(completion(status.code().map(|code| code as i64), out, err))
}

/// One stream, drained to its end and retained only up to a bound.
#[derive(Debug, Default)]
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
struct Drained {
    bytes: Vec<u8>,
    /// True when the stream produced more than the bound retained.
    truncated: bool,
}

/// Read one stream to its end, keeping at most `cap` bytes.
///
/// Reading continues past the cap. Stopping there would leave a full pipe and
/// a blocked child, which is the defect this function exists to avoid; the
/// excess is read and dropped instead, so peak memory is the bound rather than
/// whatever the program decided to print.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn drain(mut stream: impl std::io::Read, cap: usize) -> Drained {
    let mut drained = Drained::default();
    let mut buffer = [0u8; 8 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let room = cap.saturating_sub(drained.bytes.len());
                if read > room {
                    drained.bytes.extend_from_slice(&buffer[..room]);
                    drained.truncated = true;
                } else {
                    drained.bytes.extend_from_slice(&buffer[..read]);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            // A reader that cannot read cannot help the child either. Dropping
            // the stream closes this end, so the child's next write fails
            // instead of blocking on a pipe nobody is emptying.
            Err(_) => break,
        }
    }
    drained
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn completion(exit_code: Option<i64>, stdout: Drained, stderr: Drained) -> Completion {
    Completion {
        started: true,
        completed: true,
        exit_code,
        truncated: stdout.truncated || stderr.truncated,
        stdout: String::from_utf8_lossy(&stdout.bytes).to_string(),
        stderr: String::from_utf8_lossy(&stderr.bytes).to_string(),
    }
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
