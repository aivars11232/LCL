//! A deterministic process capability.
//!
//! It runs nothing. Each program is bound to a scripted completion, so a test
//! that needs "the command exited 1 with this on stderr" gets exactly that,
//! identically, on every machine — which a real command could not promise.

use lcl_capabilities::process::{Command, Completion, Process, ProcessError};
use lcl_capabilities::Bounds;
use std::collections::BTreeMap;

/// An in-memory process runner.
#[derive(Debug, Clone, Default)]
pub struct MemoryProcess {
    scripted: BTreeMap<String, Completion>,
    invocations: Vec<Command>,
}

impl MemoryProcess {
    pub fn new() -> MemoryProcess {
        MemoryProcess::default()
    }

    /// Bind one program to the completion it always produces.
    pub fn with_program(
        mut self,
        program: impl Into<String>,
        completion: Completion,
    ) -> MemoryProcess {
        self.scripted.insert(program.into(), completion);
        self
    }

    /// A completion that succeeded, writing `stdout`.
    pub fn succeeded(stdout: &str) -> Completion {
        Completion {
            started: true,
            completed: true,
            exit_code: Some(0),
            stdout: stdout.to_string(),
            stderr: String::new(),
            truncated: false,
        }
    }

    /// A completion that ran and reported a nonzero exit code.
    ///
    /// `status.succeeded` may accompany "a nonzero command exit_code": the
    /// producer completed its contract, and the exit code is a domain outcome.
    pub fn exited(code: i64, stderr: &str) -> Completion {
        Completion {
            started: true,
            completed: true,
            exit_code: Some(code),
            stdout: String::new(),
            stderr: stderr.to_string(),
            truncated: false,
        }
    }

    /// Every command this runner was asked to run, in order.
    pub fn invocations(&self) -> &[Command] {
        &self.invocations
    }
}

impl Process for MemoryProcess {
    fn run(&mut self, command: &Command, _bounds: &Bounds) -> Result<Completion, ProcessError> {
        self.invocations.push(command.clone());
        self.scripted.get(&command.program).cloned().ok_or_else(|| {
            ProcessError::NotStarted(format!("{} is not installed here", command.program))
        })
    }
}

/// A deterministic human responder.
///
/// It answers from a script rather than from a person, so a `core.ask` test is
/// reproducible. An unscripted question has no answer, which is the MISSING
/// case the row's own contract describes.
#[derive(Debug, Clone, Default)]
pub struct MemoryResponder {
    answers: BTreeMap<String, String>,
    asked: Vec<String>,
}

impl MemoryResponder {
    pub fn new() -> MemoryResponder {
        MemoryResponder::default()
    }

    /// Bind one exact question to the answer it always receives.
    pub fn with_answer(
        mut self,
        question: impl Into<String>,
        answer: impl Into<String>,
    ) -> MemoryResponder {
        self.answers.insert(question.into(), answer.into());
        self
    }

    /// Every question this responder was asked, in order.
    pub fn asked(&self) -> &[String] {
        &self.asked
    }
}

impl lcl_capabilities::Responder for MemoryResponder {
    fn ask(&mut self, question: &str, _options: &[String]) -> Result<Option<String>, ProcessError> {
        self.asked.push(question.to_string());
        Ok(self.answers.get(question).cloned())
    }
}
