//! Running a document: starting a run, following its events, answering its questions.

use super::*;

impl Routes {
    /// Start one run on its own thread and return its identity.
    ///
    /// The reply is the run id, not the result: the result arrives on the
    /// event stream, because a run that can pause cannot be an HTTP round trip.
    pub(super) fn start_run(&self, request: &Request) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let text = match request.text() {
            Ok(text) => text.to_string(),
            Err(e) => return Response::error(422, &e.to_string()),
        };

        let granted = match grants_of(request) {
            Ok(granted) => granted,
            Err(detail) => return Response::error(400, &detail),
        };
        let inputs = match inputs_of(request) {
            Ok(inputs) => inputs,
            Err(detail) => return Response::error(400, &detail),
        };
        let breaks = Breaks {
            on_operation: request.param("break_operations") == Some("1"),
            on_effect: request.param("break_effects") == Some("1"),
        };

        let session = self.runs.open(breaks);
        let workspace = self.workspace();
        let document = id.to_string();
        let thread_session = Arc::clone(&session);

        std::thread::spawn(move || {
            let report = execute(
                &workspace,
                &document,
                &text,
                granted,
                inputs,
                &thread_session,
            );
            match report {
                Ok(report) => thread_session.emit("report", report.to_json().pretty()),
                Err(detail) => thread_session.emit(
                    "failed",
                    Object::new().with("error", Node::string(detail)).pretty(),
                ),
            }
            thread_session.finish();
        });

        Response::json(
            Object::new()
                .with("run", Node::string(session.id()))
                .pretty(),
        )
    }

    /// Stream one run's events.
    ///
    /// Replayed from the start, so a browser that connects late, reconnects,
    /// or was reloaded still sees the pause it has to answer. The stream ends
    /// when the run does.
    pub(super) fn events(&self, request: &Request) -> RouteOutcome {
        let Some(id) = request.param("run") else {
            return RouteOutcome::Reply(Response::error(400, "a run id is required"));
        };
        let Some(session) = self.runs.get(id) else {
            return RouteOutcome::Reply(Response::error(404, "no such run"));
        };
        RouteOutcome::Stream(Box::new(move |mut stream| {
            let mut sent = 0usize;
            loop {
                let batch = session.events_from(sent);
                for (event, payload) in &batch {
                    if stream.send(event, payload).is_err() {
                        // The browser went away. Nothing to do about it, and
                        // the run keeps going: it is the operator's document,
                        // not the tab's.
                        return;
                    }
                }
                sent += batch.len();
                if session.is_finished() && batch.is_empty() {
                    let _ = stream.send("end", "{}");
                    return;
                }
            }
        }))
    }

    /// Answer the pause a run is sitting at.
    pub(super) fn answer(&self, request: &Request) -> Response {
        let Some(id) = request.param("run") else {
            return Response::error(400, "a run id is required");
        };
        let Some(session) = self.runs.get(id) else {
            return Response::error(404, "no such run");
        };
        let sequence: u64 = match request.param("sequence").map(str::parse) {
            Some(Ok(sequence)) => sequence,
            _ => return Response::error(400, "a pause sequence number is required"),
        };
        let answer = match request.param("answer") {
            Some("continue") => Answer::Continue,
            Some("deny") => {
                Answer::Deny("the operator refused this effect in the workspace".to_string())
            }
            Some("cancel") => Answer::Cancel,
            _ => return Response::error(400, "answer must be continue, deny or cancel"),
        };

        // A cancel is answerable even when nothing is waiting: it means stop.
        if answer == Answer::Cancel {
            session.cancel();
            return Response::json("{\n  \"answered\": true\n}\n".to_string());
        }
        if session.answer(sequence, answer) {
            Response::json("{\n  \"answered\": true\n}\n".to_string())
        } else {
            // Stale click. Refusing it is what stops a browser from
            // pre-authorising an effect it has not been shown.
            Response::error(409, "that pause is no longer current")
        }
    }
}

/// One run, on the run thread.
fn execute(
    workspace: &Workspace,
    id: &str,
    text: &str,
    granted: Granted,
    inputs: Inputs,
    session: &Arc<Session>,
) -> Result<Report, String> {
    let provider = workspace.project().provider().map_err(|e| e.to_string())?;
    let unit = intelligence::unit_of(id, text);
    let engine = workspace.engine_for(&unit);

    let (mut stdlib, mut host) =
        lcl_protocol::surface(engine, &granted).map_err(|e| e.to_string())?;

    if session_watches(session) {
        let mut watched_host = WatchedHost::new(&mut host, Arc::clone(session));
        let mut watched_ops = WatchedOperations::new(&mut stdlib, Arc::clone(session));
        Ok(engine.run_with(
            &unit,
            &provider,
            &inputs,
            &mut watched_ops,
            &mut watched_host,
        ))
    } else {
        Ok(engine.run(&unit, &provider, &inputs, &mut stdlib, &mut host))
    }
}

/// Whether this session needs the wrappers at all.
///
/// Always: even without a breakpoint, the wrappers are what produce the
/// operation, permission and effect events the execution view shows.
fn session_watches(_session: &Arc<Session>) -> bool {
    true
}

/// The capability grants this request carries.
///
/// Nothing is granted by default, and every grant is one the operator wrote
/// down. Repeated parameters accumulate, so a run may name several paths.
fn grants_of(request: &Request) -> Result<Granted, String> {
    let mut granted = Granted::none();
    for (key, value) in &request.query {
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "allow_read" => {
                for path in value.split('\n') {
                    granted = granted.permit_read(PathBuf::from(path));
                }
            }
            "allow_write" => {
                for path in value.split('\n') {
                    granted = granted.permit_write(PathBuf::from(path));
                }
            }
            "allow_program" => {
                for program in value.split('\n') {
                    granted = granted.permit_program(program);
                }
            }
            "allow_host" => {
                for host in value.split('\n') {
                    granted = granted.permit_host(host);
                }
            }
            _ => {}
        }
    }
    Ok(granted)
}

/// The data this request supplies to the invocation.
///
/// Each is `id=expression`, and the engine turns the expression into a value
/// through its own step-7 evaluation. The workspace does not read literals.
fn inputs_of(request: &Request) -> Result<Inputs, String> {
    let mut inputs = Inputs::new();
    if let Some(raw) = request.param("input") {
        for line in raw.split('\n').filter(|l| !l.trim().is_empty()) {
            let Some((id, expression)) = line.split_once('=') else {
                return Err(format!("supplied data must be id=expression, not {line:?}"));
            };
            inputs = inputs.with_text(id.trim(), expression);
        }
    }
    Ok(inputs)
}
