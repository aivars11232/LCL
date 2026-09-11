//! Running a document, and watching it happen.
//!
//! ## Why a session and not a request
//!
//! A run can pause. The capability boundary is where an effect leaves the
//! language, and that is exactly where a person wants to be asked before it
//! does, so a run has to be able to stop mid-flight, tell the browser what it
//! is about to do, and wait for an answer. An HTTP request that returned a
//! report could not do that.
//!
//! So a run is a [`Session`]: one worker thread executing the engine, one
//! event stream carrying what it observed, and one small protocol for
//! answering it. The engine call itself is ordinary and synchronous —
//! `Engine::run` — and nothing about the language is changed by being watched.
//!
//! ## Determinism survives the thread
//!
//! Contract 5.3 requires that observable meaning not depend on thread
//! scheduling. It does not here, structurally: one run is one thread, the
//! runtime's own scheduler is a single deterministic queue, and the only thing
//! the UI thread does is hand back answers at pause points. Two runs never
//! share engine state, because each builds its own.
//!
//! What a pause *does* change is wall-clock timing, and the canonical rule is
//! already explicit that timing is not meaning unless it was supplied as an
//! input.

use lcl_runtime::{
    CapabilityOutcome, CapabilityRequest, Host, Invocation, Operations, Permission, Resolution,
    Value,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Where a run may stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Breaks {
    /// Ask before every operation the standard library dispatches.
    pub on_operation: bool,
    /// Ask before every effect that reaches the host.
    pub on_effect: bool,
}

impl Breaks {
    pub fn any(self) -> bool {
        self.on_operation || self.on_effect
    }
}

/// What the operator answered at a pause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Carry on.
    Continue,
    /// Refuse this one effect. The host reports a refusal, and the engine
    /// decides what that means for the invocation.
    Deny(String),
    /// Stop the run. Every later effect is refused the same way.
    Cancel,
}

/// Why the run is paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pause {
    /// The standard library is about to dispatch an operation.
    Operation,
    /// An effect is about to cross the host boundary.
    Effect,
}

/// One paused request, as the browser sees it.
#[derive(Debug, Clone)]
pub struct Paused {
    pub kind: Pause,
    pub request: CapabilityRequest,
}

/// The live state of one run.
#[derive(Debug, Default)]
struct State {
    paused: Option<PausedInner>,
    answer: Option<Answer>,
    cancelled: bool,
    finished: bool,
}

/// The pause the run thread is currently sitting at.
///
/// Only the sequence number is kept. What the pause *was* is already in the
/// event log, which a stream replays from the start, so holding a second copy
/// here would be one more thing that can disagree with it.
#[derive(Debug, Clone)]
struct PausedInner {
    sequence: u64,
}

/// One run: its thread, its pause gate and its event log.
pub struct Session {
    id: String,
    state: Mutex<State>,
    waiting: Condvar,
    /// Every event emitted so far, in order. A stream that connects late
    /// replays them, so a browser cannot miss the pause it has to answer.
    log: Mutex<Vec<(String, String)>>,
    arrived: Condvar,
    sequence: AtomicU64,
    breaks: Breaks,
}

impl Session {
    pub fn new(id: String, breaks: Breaks) -> Arc<Session> {
        Arc::new(Session {
            id,
            state: Mutex::new(State::default()),
            waiting: Condvar::new(),
            log: Mutex::new(Vec::new()),
            arrived: Condvar::new(),
            sequence: AtomicU64::new(0),
            breaks,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Append one event and wake anything streaming.
    pub fn emit(&self, event: &str, payload: String) {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        log.push((event.to_string(), payload));
        self.arrived.notify_all();
    }

    /// Every event from `from` onward, blocking until there is one or the run
    /// has finished.
    pub fn events_from(&self, from: usize) -> Vec<(String, String)> {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if log.len() > from {
                return log[from..].to_vec();
            }
            let finished = {
                let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                state.finished
            };
            if finished {
                return Vec::new();
            }
            let (next, timeout) = self
                .arrived
                .wait_timeout(log, std::time::Duration::from_millis(500))
                .unwrap_or_else(|e| e.into_inner());
            log = next;
            if timeout.timed_out() && log.len() <= from {
                // A heartbeat keeps a proxy-free loopback stream honest and
                // lets a closed browser tab be noticed.
                return Vec::new();
            }
        }
    }

    pub fn finish(&self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.finished = true;
            state.paused = None;
        }
        self.arrived.notify_all();
        self.waiting.notify_all();
    }

    pub fn is_finished(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .finished
    }

    /// Answer the pause the run is sitting at.
    ///
    /// Returns false when nothing is waiting, so a stale click from a browser
    /// cannot pre-authorise the *next* effect.
    pub fn answer(&self, sequence: u64, answer: Answer) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let Some(paused) = &state.paused else {
            return false;
        };
        if paused.sequence != sequence {
            return false;
        }
        if answer == Answer::Cancel {
            state.cancelled = true;
        }
        state.answer = Some(answer);
        state.paused = None;
        self.waiting.notify_all();
        true
    }

    /// Cancel from outside a pause, e.g. the operator closed the run.
    pub fn cancel(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.cancelled = true;
        state.answer = Some(Answer::Cancel);
        state.paused = None;
        self.waiting.notify_all();
    }

    pub fn cancelled(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled
    }

    /// Stop the run thread here and wait for an answer.
    fn hold(&self, kind: Pause, request: &CapabilityRequest) -> Answer {
        if self.cancelled() {
            return Answer::Cancel;
        }
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
        let payload = paused_json(sequence, kind, request);
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.paused = Some(PausedInner { sequence });
            state.answer = None;
        }
        self.emit("paused", payload);

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.answer.is_none() {
            state = self.waiting.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        let answer = state.answer.take().unwrap_or(Answer::Continue);
        drop(state);
        self.emit("resumed", format!("{{\"sequence\":{sequence}}}"));
        answer
    }
}

/// One paused request as JSON.
///
/// Everything the capability contract says a request must carry so an operator
/// can judge it: what operation, on what target, with what parameters, what
/// the language authorized and why, what effects it may have, and exactly
/// where in which document it is written.
fn paused_json(sequence: u64, kind: Pause, request: &CapabilityRequest) -> String {
    use lcl_protocol::json::{Node, Object};
    Object::new()
        .with("sequence", Node::u64(sequence))
        .with(
            "kind",
            Node::string(match kind {
                Pause::Operation => "operation",
                Pause::Effect => "effect",
            }),
        )
        .with("operation", Node::string(&request.operation))
        .with(
            "target",
            Node::optional(request.target.as_ref().map(|v| v.to_string())),
        )
        .with(
            "parameters",
            Node::array(request.parameters.iter().map(|(name, value)| {
                Object::new()
                    .with("name", Node::string(name))
                    .with("value", Node::string(value.to_string()))
                    .into()
            })),
        )
        .with("category", Node::string(&request.category))
        .with(
            "possible_effects",
            Node::array(request.possible_effects.iter().map(Node::string)),
        )
        .with(
            "authorization",
            Object::new()
                .with("operation", Node::string(&request.authorization.operation))
                .with("scope", Node::optional(request.authorization.scope.clone()))
                .with(
                    "permitted_by",
                    Node::array(request.authorization.permitted_by.iter().map(Node::string)),
                )
                .with(
                    "overridden",
                    Node::array(request.authorization.overridden.iter().map(Node::string)),
                )
                .into(),
        )
        .with("source", Node::string(request.source.to_string()))
        .with(
            "span",
            Object::new()
                .with("start", Node::usize(request.span.start))
                .with("end", Node::usize(request.span.end))
                .into(),
        )
        .with(
            "invocation",
            Node::string(format!("{:?}", request.invocation)),
        )
        .pretty()
}

// ---------------------------------------------------------------------------
// The two wrappers
// ---------------------------------------------------------------------------

/// An operation dispatcher that can pause before it answers.
///
/// It delegates every decision. `lcl-stdlib` decides what an operation means;
/// this decides only whether to stop first, which is a product behaviour and
/// not a language one. A denial here does not invent a result: it hands the
/// request on to the host, where a refusal has a registered meaning.
pub struct WatchedOperations<'a> {
    inner: &'a mut dyn Operations,
    session: Arc<Session>,
}

impl<'a> WatchedOperations<'a> {
    pub fn new(inner: &'a mut dyn Operations, session: Arc<Session>) -> WatchedOperations<'a> {
        WatchedOperations { inner, session }
    }
}

impl Operations for WatchedOperations<'_> {
    fn invoke(&mut self, cx: &mut Invocation<'_>, request: &CapabilityRequest) -> Resolution {
        if self.session.breaks.on_operation && !self.session.cancelled() {
            match self.session.hold(Pause::Operation, request) {
                Answer::Continue => {}
                Answer::Deny(_) | Answer::Cancel => {
                    // Hand it to the host, which will refuse it with a
                    // registered meaning. Fabricating a result here would be
                    // this crate deciding a language outcome.
                    return Resolution::host(request.clone());
                }
            }
        }
        self.session.emit(
            "operation",
            format!(
                "{{\"operation\":\"{}\"}}",
                crate::http::escape_json(&request.operation)
            ),
        );
        self.inner.invoke(cx, request)
    }
}

/// A host that asks before it acts.
///
/// Contract 5.7: "Host permission does not imply LCL authorization. LCL
/// authorization does not force host permission. Both gates must pass for an
/// effect." This wrapper is the second gate and only the second gate. It is
/// reached only for a request the language already authorized, and a `yes`
/// here adds a host permission and nothing else.
pub struct WatchedHost<'a> {
    inner: &'a mut dyn Host,
    session: Arc<Session>,
}

impl<'a> WatchedHost<'a> {
    pub fn new(inner: &'a mut dyn Host, session: Arc<Session>) -> WatchedHost<'a> {
        WatchedHost { inner, session }
    }
}

impl Host for WatchedHost<'_> {
    fn permits(&mut self, request: &CapabilityRequest) -> Permission {
        if self.session.cancelled() {
            return Permission::Denied("the operator stopped this run".to_string());
        }
        if self.session.breaks.on_effect {
            match self.session.hold(Pause::Effect, request) {
                Answer::Continue => {}
                Answer::Deny(reason) => return Permission::Denied(reason),
                Answer::Cancel => {
                    return Permission::Denied("the operator stopped this run".to_string())
                }
            }
        }
        let permission = self.inner.permits(request);
        self.session.emit(
            "permission",
            format!(
                "{{\"operation\":\"{}\",\"permission\":\"{}\"}}",
                crate::http::escape_json(&request.operation),
                match &permission {
                    Permission::Granted => "granted",
                    Permission::Denied(_) => "denied",
                    Permission::Unavailable(_) => "unavailable",
                }
            ),
        );
        permission
    }

    fn invoke(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        if self.session.cancelled() {
            return CapabilityOutcome::Denied("the operator stopped this run".to_string());
        }
        let outcome = self.inner.invoke(request);
        self.session.emit(
            "effect",
            format!(
                "{{\"operation\":\"{}\",\"outcome\":\"{}\"}}",
                crate::http::escape_json(&request.operation),
                match &outcome {
                    CapabilityOutcome::Completed(_) => "completed",
                    CapabilityOutcome::Failed { .. } => "failed",
                    CapabilityOutcome::Refused { .. } => "refused",
                    CapabilityOutcome::Denied(_) => "denied",
                    CapabilityOutcome::Unavailable(_) => "unavailable",
                }
            ),
        );
        outcome
    }

    fn delay(&mut self, duration: &Value) {
        // The runtime hands over a declared RETRY.DELAY and does not wait
        // itself. Neither does this: a workspace that slept would make a
        // debugging session slower without changing any observable result.
        self.inner.delay(duration)
    }
}

/// Every live run, by id.
#[derive(Default)]
pub struct Runs {
    sessions: Mutex<BTreeMap<String, Arc<Session>>>,
    next: AtomicU64,
}

impl Runs {
    pub fn new() -> Runs {
        Runs::default()
    }

    pub fn open(&self, breaks: Breaks) -> Arc<Session> {
        let id = format!("run-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let session = Session::new(id.clone(), breaks);
        let mut sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        // A workspace keeps only the recent past; a finished session is kept
        // so a late stream can still read its result.
        if sessions.len() > 16 {
            let stale: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| s.is_finished())
                .map(|(k, _)| k.clone())
                .take(8)
                .collect();
            for key in stale {
                sessions.remove(&key);
            }
        }
        sessions.insert(id, Arc::clone(&session));
        session
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(Arc::clone)
    }
}
