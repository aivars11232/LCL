//! The route table: every URL the frontend can reach, and nothing else.
//!
//! ## Shape of a reply
//!
//! Every engine reply is one [`lcl_protocol::Report`], projected to JSON by
//! `lcl-protocol`'s own writer. Not a reshaped version of one, not a summary,
//! and not a view model: the same bytes the CLI would print for the same
//! request. That is what makes the UI-versus-CLI equivalence gate a byte
//! comparison instead of a field-by-field argument.
//!
//! The only replies this module composes itself are about the *workspace* —
//! which documents exist, what was saved, why a path was refused. None of them
//! carries an LCL diagnostic, a span, a status or a stage, because those are
//! engine truth and travel in a report.

use crate::execution::{Answer, Breaks, Runs, Session, WatchedHost, WatchedOperations};
use crate::http::{Request, Response};
use crate::intelligence;
use crate::project::Workspace;
use crate::server::{Outcome as RouteOutcome, Route};
use lcl_protocol::json::{Node, Object};
use lcl_protocol::{Command, Granted, Inputs, Report};
use std::path::PathBuf;
use std::sync::Arc;

/// The frontend, compiled in.
///
/// Served from the binary rather than from disk: a workspace that read its own
/// interface off the filesystem at runtime would have one more thing that can
/// be swapped underneath it, for no gain.
const INDEX_HTML: &str = include_str!("../assets/index.html");
const APP_CSS: &str = include_str!("../assets/app.css");
const APP_JS: &str = include_str!("../assets/app.js");

/// The product's own mark, compiled in beside the rest of the frontend.
///
/// Derived from `assets/brand/lcl-logo-master.png` by
/// `assets/brand/derive_brand_assets.py`, which records every derivative's
/// checksum. Bytes, not text: `include_str!` would refuse a PNG, and forcing
/// one through a string is how an image arrives corrupted.
const BRAND_MARK_PNG: &[u8] = include_bytes!("../assets/brand/lcl-mark.png");
const BRAND_ICON_PNG: &[u8] = include_bytes!("../assets/brand/lcl-icon-32.png");

/// Everything the routes need, shared across connection threads.
pub struct Routes {
    workspace: Arc<Workspace>,
    runs: Runs,
}

impl Routes {
    pub fn new(workspace: Arc<Workspace>) -> Routes {
        Routes {
            workspace,
            runs: Runs::new(),
        }
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }
}

impl Route for Routes {
    fn handle(&self, request: &Request) -> RouteOutcome {
        // One route takes the connection over as an event stream; every other
        // one answers and closes.
        if request.method == "GET" && request.path == "/api/events" {
            return self.events(request);
        }
        RouteOutcome::Reply(self.reply(request))
    }
}

impl Routes {
    fn reply(&self, request: &Request) -> Response {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => {
                // The page is stamped with the session token, because the
                // browser fetches the stylesheet and the script by itself and
                // those requests pass the same gate every other one does.
                // Without this the page loads unstyled and never starts, which
                // is exactly what the first browser smoke test found.
                let token = request
                    .header("x-lcl-token")
                    .or_else(|| request.param("t"))
                    .unwrap_or_default();
                Response::html(&INDEX_HTML.replace("{{TOKEN}}", token))
            }
            ("GET", "/app.css") => Response::css(APP_CSS),
            ("GET", "/app.js") => Response::javascript(APP_JS),
            // Both are stamped with the session token by the page that
            // references them, exactly as the stylesheet and script are, so
            // they pass the same three gates as every other request. A browser
            // guessing at `/favicon.ico` is not served: an unauthenticated
            // route would be a hole opened for a picture.
            ("GET", "/brand/lcl-mark.png") => Response::png(BRAND_MARK_PNG),
            ("GET", "/brand/lcl-icon-32.png") => Response::png(BRAND_ICON_PNG),

            ("GET", "/api/session") => self.session(),
            ("GET", "/api/documents") => self.documents(),
            ("GET", "/api/document") => self.read_document(request),
            ("PUT", "/api/document") => self.save_document(request),
            ("POST", "/api/document") => self.create_document(request),

            ("POST", "/api/tokens") => self.tokens(request),
            ("POST", "/api/check") => self.analyse(request, Command::Check),
            ("POST", "/api/inspect") => self.analyse(request, Command::Inspect),
            ("POST", "/api/validate") => self.analyse(request, Command::Validate),
            ("POST", "/api/run") => self.start_run(request),
            ("POST", "/api/answer") => self.answer(request),

            ("GET", _) | ("PUT", _) | ("POST", _) => Response::error(404, "no such route"),
            _ => Response::error(405, "method not allowed"),
        }
    }

    /// What this workspace is, for a frontend that just loaded.
    fn session(&self) -> Response {
        let spec = self.workspace.engine().spec_record();
        let body = Object::new()
            .with("protocol", Node::string(lcl_protocol::PROTOCOL))
            .with(
                "root",
                Node::string(self.workspace.root().display().to_string()),
            )
            .with(
                "spec",
                Object::new()
                    .with("root", Node::string(&spec.root))
                    .with("formal_version", Node::string(&spec.formal_version))
                    .with("identity_digest", Node::string(&spec.identity_digest))
                    .with("authority", Node::string(&spec.authority))
                    .into(),
            )
            .with("entry", Node::optional(self.workspace.entry()))
            // The document this workspace was launched for, when a file
            // association supplied one. Separate from `entry`, which is the
            // manifest's declared starting document and belongs to the project
            // rather than to this launch.
            .with(
                "open",
                Node::optional(self.workspace.open_document().map(str::to_string)),
            )
            .pretty();
        Response::json(body)
    }

    /// Every `.lcl` document in the project.
    fn documents(&self) -> Response {
        match self.workspace.documents() {
            Ok(entries) => Response::json(
                Object::new()
                    .with(
                        "entries",
                        Node::array(entries.iter().map(|e| {
                            Object::new()
                                .with("id", Node::string(&e.id))
                                .with("directory", Node::Bool(e.directory))
                                .with(
                                    "bytes",
                                    match e.bytes {
                                        Some(n) => Node::u64(n),
                                        None => Node::Null,
                                    },
                                )
                                .into()
                        })),
                    )
                    .pretty(),
            ),
            Err(e) => Response::error(400, &e.to_string()),
        }
    }

    fn read_document(&self, request: &Request) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        match self.workspace.read(id) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    .with("text", Node::string(&document.text))
                    .with("digest", Node::string(&document.digest))
                    .pretty(),
            ),
            Err(e) => Response::error(404, &e.to_string()),
        }
    }

    /// Save one document. The body is the exact bytes to write.
    ///
    /// A refusal here is the encoding rule refusing, and the reply says which
    /// rule and why. The file is not touched.
    fn save_document(&self, request: &Request) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        match self.workspace.save(id, text) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    .with("digest", Node::string(&document.digest))
                    .with("bytes", Node::usize(document.text.len()))
                    .with(
                        "final_line_feed_added",
                        Node::Bool(document.text.len() != text.len()),
                    )
                    .pretty(),
            ),
            Err(e) => Response::error(422, &e.to_string()),
        }
    }

    /// Create one document, under the name the naming default gives it.
    ///
    /// Separate from `PUT` on purpose. Saving must write exactly the name it
    /// was given, or opening a `.lcl` document and pressing save would rename
    /// it; creating applies the default, which is the text form. Keeping the
    /// two in one route would mean guessing which of them the caller meant.
    ///
    /// An existing file is never overwritten. Creation that silently replaced
    /// a document would be a data-loss path reachable by typing a name.
    fn create_document(&self, request: &Request) -> Response {
        let Some(requested) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let id = lcl_project::default_name(requested);
        if id.is_empty() || id == lcl_project::TEXT_SUFFIX {
            return Response::error(400, "a document needs a name");
        }
        // Preserve the early response, but do not use this observation as a
        // reservation: create_document below is the atomic authority.
        if self.workspace.exists(&id) {
            return Response::error(409, &format!("{id} already exists"));
        }
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        match self.workspace.create_document(&id, text) {
            Ok(document) => Response::json(
                Object::new()
                    .with("id", Node::string(&document.id))
                    // What the caller asked for, so a frontend can say that the
                    // name it is about to show is not the one that was typed.
                    .with("requested", Node::string(requested))
                    .with("digest", Node::string(&document.digest))
                    .with("bytes", Node::usize(document.text.len()))
                    .pretty(),
            ),
            Err(crate::WorkspaceError::Document(crate::DocumentError::AlreadyExists(_))) => {
                Response::error(409, &format!("{id} already exists"))
            }
            Err(e) => Response::error(422, &e.to_string()),
        }
    }

    /// Token spans for one buffer, produced by the real lexer.
    fn tokens(&self, request: &Request) -> Response {
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        Response::json(intelligence::tokens_json(
            self.workspace.engine().lexicon(),
            text,
        ))
    }

    /// Run one engine command over the buffer and return its report.
    ///
    /// The reply is `Report::to_json`, unaltered. Not a view model and not a
    /// summary: the same bytes `lcl --machine` prints for the same request.
    fn analyse(&self, request: &Request, command: Command) -> Response {
        let Some(id) = request.param("id") else {
            return Response::error(400, "a document id is required");
        };
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        match self.report(id, text, command) {
            Ok(report) => Response::json(report.to_json().pretty()),
            Err(detail) => Response::error(400, &detail),
        }
    }

    /// One engine request over a buffer, through the project's own provider.
    pub fn report(&self, id: &str, text: &str, command: Command) -> Result<Report, String> {
        let provider = self
            .workspace
            .project()
            .provider()
            .map_err(|e| e.to_string())?;
        let unit = intelligence::unit_of(id, text);
        let engine = self.workspace.engine();
        Ok(match command {
            Command::Check => engine.check(&unit, &provider),
            Command::Inspect => engine.inspect(&unit, &provider, &Inputs::new()),
            Command::Validate => engine.validate(&unit, &provider, &Inputs::new()),
            // A run needs a host and a grant decision, which Phase D and E own.
            Command::Run => {
                return Err("a run is requested through /api/run".to_string());
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Running
// ---------------------------------------------------------------------------

impl Routes {
    /// Start one run on its own thread and return its identity.
    ///
    /// The reply is the run id, not the result: the result arrives on the
    /// event stream, because a run that can pause cannot be an HTTP round trip.
    fn start_run(&self, request: &Request) -> Response {
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
        let workspace = Arc::clone(&self.workspace);
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
    fn events(&self, request: &Request) -> RouteOutcome {
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
    fn answer(&self, request: &Request) -> Response {
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
    let engine = workspace.engine();

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
