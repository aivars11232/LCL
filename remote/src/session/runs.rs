//! The runs a device starts: started through the workspace's own run route,
//! owned by the device that started them, and followed event by event.

use super::operations::{route, Answer, Asked};
use super::wire::{error_body, text, End, Wire};
use super::{Session, Shared};
use lcl_spec::json::Json;
use lcl_workspace::Routes;
use std::sync::{Arc, Weak};

/// The device that started a run: the only one that may follow it or answer
/// its pauses, cancelling included.
///
/// Run ids are counted per project and are easy to guess, and every paired
/// device can reach every shared project, so knowing an id must not be enough.
/// The record binds the run to the device the PC authenticated when the run
/// was started — its id and the fingerprint of the certificate it proved — and
/// to the very routes the run lives in: run ids start again in routes opened
/// afresh, and a record must never answer for a run it did not see started.
pub(super) struct Owner {
    device: String,
    fingerprint: String,
    routes: Weak<Routes>,
}

/// A run this session started, and how much of it the device has been sent.
pub(super) struct Watching {
    project: String,
    run: String,
    routes: Arc<Routes>,
    sent: usize,
}

impl Shared {
    /// Cancel the runs devices started in `closed`, the routes of projects no
    /// longer shared: nothing can reach them to answer their pauses any more.
    pub(super) fn cancel_runs_in(&self, closed: &[Arc<Routes>]) {
        let mut owners = self.owners.lock().unwrap_or_else(|e| e.into_inner());
        owners.retain(|(_, run), owner| {
            let Some(routes) = owner.routes.upgrade() else {
                return false;
            };
            if !closed.iter().any(|c| Arc::ptr_eq(c, &routes)) {
                return true;
            }
            let _ = route(
                &routes,
                "POST",
                "/api/answer",
                &[
                    ("run", run.clone()),
                    ("sequence", "0".into()),
                    ("answer", "cancel".into()),
                ],
                "",
            );
            false
        });
    }
}

impl Session {
    /// Start a run through the workspace's own run route, and follow it.
    pub(super) fn run(&mut self, asked: &Asked) -> Answer {
        let doc = asked.need("document", "a document")?;
        let text = asked.need("text", "the text")?;
        let query = run_query(doc, asked.message)?;
        let (status, body) = asked.call("POST", "/api/run", &query, &text);
        if status == 200 {
            let started = lcl_spec::json::parse(&body)
                .ok()
                .and_then(|b| b.get("run").and_then(Json::as_str).map(str::to_string));
            if let Some(run) = started {
                self.own(asked, &run);
                self.runs.push(Watching {
                    project: asked.project.id.clone(),
                    run,
                    routes: Arc::clone(asked.routes),
                    sent: 0,
                });
            }
        }
        Ok((status, body))
    }

    /// Record that this device started `run`.
    fn own(&self, asked: &Asked, run: &str) {
        let mut owners = self.shared.owners.lock().unwrap_or_else(|e| e.into_inner());
        // Records of runs their routes no longer keep answer nothing.
        owners.retain(|(_, run), owner| {
            owner
                .routes
                .upgrade()
                .is_some_and(|routes| routes.run_events(run, 0).is_some())
        });
        owners.insert(
            (asked.project.id.clone(), run.to_string()),
            Owner {
                device: self.device.clone(),
                fingerprint: self.fingerprint.clone(),
                routes: Arc::downgrade(asked.routes),
            },
        );
    }

    /// Whether this device started `run`, in these very routes.
    fn owns(&self, asked: &Asked, run: &str) -> bool {
        let owners = self.shared.owners.lock().unwrap_or_else(|e| e.into_inner());
        owners
            .get(&(asked.project.id.clone(), run.to_string()))
            .is_some_and(|owner| {
                owner.device == self.device
                    && owner.fingerprint == self.fingerprint
                    && Weak::ptr_eq(&owner.routes, &Arc::downgrade(asked.routes))
            })
    }

    /// Resume a run's events after a reconnect, from the first one the device
    /// has not seen. The log is append-only, so events from that index on are
    /// exactly what it missed.
    pub(super) fn follow(&mut self, asked: &Asked) -> Answer {
        let run = asked.need("run", "a run")?;
        let from = asked
            .message
            .get("from")
            .and_then(Json::as_u64)
            .unwrap_or(0) as usize;
        if asked.routes.run_events(&run, 0).is_none() {
            return Err(format!("no run {run} is known to this project any more"));
        }
        if !self.owns(asked, &run) {
            return Ok(not_yours(&run));
        }
        let project = &asked.project.id;
        if !self
            .runs
            .iter()
            .any(|w| &w.project == project && w.run == run)
        {
            self.runs.push(Watching {
                project: project.clone(),
                run,
                routes: Arc::clone(asked.routes),
                sent: from,
            });
        }
        Ok((200, "{\"following\":true}".to_string()))
    }

    /// Answer a run's pause: continue, deny or cancel.
    pub(super) fn answer(&self, asked: &Asked) -> Answer {
        let run = asked.need("run", "a run")?;
        // An answer to a run this device did not start is refused before it
        // reaches the run: continue, deny and cancel alike.
        if asked.routes.run_events(&run, 0).is_some() && !self.owns(asked, &run) {
            return Ok(not_yours(&run));
        }
        let sequence = asked
            .message
            .get("sequence")
            .and_then(Json::as_u64)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let answer = asked.need("answer", "an answer")?;
        Ok(asked.call(
            "POST",
            "/api/answer",
            &[("run", run), ("sequence", sequence), ("answer", answer)],
            "",
        ))
    }

    /// Send the device what its runs have said since it was last told, and
    /// the end of each one that is over.
    pub(super) fn forward_runs(&mut self, wire: &mut Wire) -> Result<(), End> {
        // Routes were closed somewhere since this session last looked: some
        // run followed here may be in them, and nothing more of it may pass.
        if self.shared.projects.closings() != self.closings_seen {
            self.forget_unshared(wire)?;
        }
        let mut index = 0;
        while index < self.runs.len() {
            let watching = &mut self.runs[index];
            let (events, finished) = watching
                .routes
                .run_events(&watching.run, watching.sent)
                .unwrap_or((Vec::new(), true));
            for (name, payload) in &events {
                let data = if payload.trim().is_empty() {
                    "{}"
                } else {
                    payload.as_str()
                };
                wire.send(&run_event(&watching.project, &watching.run, name, data))?;
            }
            watching.sent += events.len();
            if finished && events.is_empty() {
                wire.send(&run_event(&watching.project, &watching.run, "end", "{}"))?;
                self.runs.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    /// Report ended each run followed here whose project is not among
    /// `still_shared`, and send nothing more of it.
    pub(super) fn end_unshared_runs(
        &mut self,
        still_shared: &[String],
        wire: &mut Wire,
    ) -> Result<(), End> {
        let mut index = 0;
        while index < self.runs.len() {
            let watching = &self.runs[index];
            if still_shared.contains(&watching.project)
                && self
                    .shared
                    .projects
                    .is_open(&watching.project, &watching.routes)
            {
                index += 1;
                continue;
            }
            let gone = self.runs.remove(index);
            let stopped =
                error_body("this project is no longer shared by this PC, so the run was stopped")
                    .compact();
            for (name, data) in [("failed", stopped.as_str()), ("end", "{}")] {
                wire.send(&run_event(&gone.project, &gone.run, name, data))?;
            }
        }
        Ok(())
    }
}

/// What the run route is asked for `doc`: what the person granted, the inputs,
/// and where the run pauses.
fn run_query(doc: String, message: &Json) -> Result<Vec<(&'static str, String)>, String> {
    let mut query = vec![("id", doc)];
    let grants = message.get("grants");
    for (key, name) in [
        ("allow_read", "read"),
        ("allow_write", "write"),
        ("allow_program", "program"),
        ("allow_host", "host"),
    ] {
        let value = lines(grants.and_then(|g| g.get(name)))?;
        if !value.is_empty() {
            query.push((key, value));
        }
    }
    let inputs = lines(message.get("inputs"))?;
    if !inputs.is_empty() {
        query.push(("input", inputs));
    }
    // A remote run pauses before every effect, always. The PC enforces it
    // rather than trusting the device to ask: whatever `break_effects` a
    // request carries is ignored, so no paired device can have an effect
    // happen that the person holding it did not approve at its pause.
    query.push(("break_effects", "1".into()));
    if message.get("break_operations").and_then(Json::as_bool) == Some(true) {
        query.push(("break_operations", "1".into()));
    }
    Ok(query)
}

/// A list of strings as the run route takes one: a string to a line, and
/// nothing at all for no list.
fn lines(value: Option<&Json>) -> Result<String, String> {
    match value {
        None | Some(Json::Null) => Ok(String::new()),
        Some(v) => {
            let items = v
                .as_array()
                .ok_or("grants and inputs are lists of strings")?;
            let strings: Option<Vec<&str>> = items.iter().map(Json::as_str).collect();
            let strings = strings.ok_or("grants and inputs are lists of strings")?;
            if strings.iter().any(|s| s.contains('\n')) {
                return Err("a grant or input may not contain a line break".into());
            }
            Ok(strings.join("\n"))
        }
    }
}

/// The refusal a device gets for a run another device started. It is the same
/// whether the run is another device's or was never this one's, so it tells
/// the asker nothing about whose it is.
fn not_yours(run: &str) -> (u16, String) {
    let refusal = format!(
        "run {run} was not started by this device; \
         only the device that started a run can follow or answer it"
    );
    (403, error_body(refusal).compact())
}

/// One event of a run, as the device is sent it. `data` is JSON already.
fn run_event(project: &str, run: &str, name: &str, data: &str) -> String {
    format!(
        r#"{{"type":"event","event":"run","project":{},"run":{},"name":{},"data":{}}}"#,
        text(project),
        text(run),
        text(name),
        data
    )
}
