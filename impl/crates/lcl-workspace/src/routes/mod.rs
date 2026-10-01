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
//!
//! ## Layout
//!
//! This file holds the routes' state, the dispatch table ([`Routes::reply`])
//! and what every area shares. Each area's handlers live in a module of their
//! own: `explorer` (the project tree), `documents`, `preferences` (the
//! workspace's settings), `projects` (the Projects home and the active
//! project), `runs`, and `devices` (Settings → Android devices).

use crate::authoring;
use crate::execution::{Answer, Breaks, Runs, Session, WatchedHost, WatchedOperations};
use crate::http::{Request, Response};
use crate::intelligence;
use crate::manual;
use crate::masters::{self, Masters};
use crate::project::Workspace;
use crate::remote;
use crate::server::{Outcome as RouteOutcome, Route};
use crate::settings;
use lcl_protocol::json::{Node, Object};
use lcl_protocol::{Command, Granted, Inputs, Report};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

mod devices;
mod documents;
mod explorer;
mod preferences;
mod projects;
mod runs;

use devices::*;

/// The frontend, compiled in.
///
/// Served from the binary rather than from disk: a workspace that read its own
/// interface off the filesystem at runtime would have one more thing that can
/// be swapped underneath it, for no gain.
const INDEX_HTML: &str = include_str!("../../assets/index.html");
const APP_CSS: &str = include_str!("../../assets/app.css");
const APP_JS: &str = include_str!("../../assets/app.js");

/// The product's own mark, compiled in beside the rest of the frontend.
///
/// Derived from `assets/brand/lcl-logo-master.png` by
/// `assets/brand/derive_brand_assets.py`, which records every derivative's
/// checksum. Bytes, not text: `include_str!` would refuse a PNG, and forcing
/// one through a string is how an image arrives corrupted.
const BRAND_MARK_PNG: &[u8] = include_bytes!("../../assets/brand/lcl-mark.png");
const BRAND_ICON_PNG: &[u8] = include_bytes!("../../assets/brand/lcl-icon-32.png");

/// Opens another folder exactly as this process's launch opened its own: the
/// same specification packages and locale profiles, located the same way.
pub type Reopen = Box<dyn Fn(&Path) -> Result<Workspace, String> + Send + Sync>;

/// Everything the routes need, shared across connection threads.
pub struct Routes {
    /// The folder this window shows: the active project, or, on the Projects
    /// home, the Projects folder itself. It changes when a project is chosen,
    /// created or opened, or the home is shown ([`Routes::with_reopen`]).
    workspace: RwLock<Arc<Workspace>>,
    /// Whether this window shows the Projects home — the projects in the
    /// Projects folder, to choose one — rather than a project's explorer. A
    /// desktop launch with no project or document starts here; the Projects
    /// folder is a container of projects, never a project of its own.
    home: RwLock<bool>,
    runs: Runs,
    /// The user's settings file, when this process knows where one lives.
    settings_file: Option<PathBuf>,
    /// The launcher's built-in default workspace, when a launcher named one.
    builtin_default: Option<PathBuf>,
    /// The folder this window was launched with: the Projects folder of last
    /// resort, when none is chosen in Settings and no launcher named one. It
    /// does not move with the active project.
    launched: PathBuf,
    /// Something the person should be told about how this launch chose its
    /// project, such as a chosen default workspace that no longer exists. It
    /// is cleared when another folder is opened.
    notice: RwLock<Option<String>>,
    reopen: Option<Reopen>,
    /// The installed updater, for Settings → Updates; a window without one
    /// (the phone's routes, a development build) answers that it has none.
    updater: Option<crate::updates::Updater>,
}

impl Routes {
    /// Routes over one workspace, with no settings file: every setting is its
    /// default and none can be saved. [`Routes::with_settings_file`] names one.
    pub fn new(workspace: Arc<Workspace>) -> Routes {
        Routes {
            launched: workspace.root().to_path_buf(),
            workspace: RwLock::new(workspace),
            home: RwLock::new(false),
            runs: Runs::new(),
            settings_file: None,
            builtin_default: None,
            notice: RwLock::new(None),
            reopen: None,
            updater: None,
        }
    }

    /// Offer Settings → Updates through this updater.
    pub fn with_updater(mut self, updater: Option<crate::updates::Updater>) -> Routes {
        self.updater = updater;
        self
    }

    /// Let this window open the projects folder in place of its own folder.
    /// Without this, the folder never changes, and a new project must be
    /// created where the window already shows it.
    pub fn with_reopen(mut self, reopen: Reopen) -> Routes {
        self.reopen = Some(reopen);
        self
    }

    /// Read and write the workspace settings at `file`.
    pub fn with_settings_file(mut self, file: Option<PathBuf>) -> Routes {
        self.settings_file = file;
        self
    }

    /// The folder the desktop launcher opens when no default is chosen.
    pub fn with_builtin_default(mut self, folder: Option<PathBuf>) -> Routes {
        self.builtin_default = folder;
        self
    }

    /// Start on the Projects home instead of a project's explorer. The
    /// workspace given is the Projects folder, opened only so that New
    /// Project and the settings have an engine and a folder to work with;
    /// nothing lists it.
    pub fn with_home(mut self, home: bool) -> Routes {
        self.home = RwLock::new(home);
        self
    }

    /// Whether this window shows the Projects home now.
    pub fn is_home(&self) -> bool {
        *self.home.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Something to tell the person when the page loads.
    pub fn with_notice(mut self, notice: Option<String>) -> Routes {
        self.notice = RwLock::new(notice);
        self
    }

    /// The workspace this window shows now. A caller keeps the one it got for
    /// the whole of one request, so a request never mixes two folders.
    pub fn workspace(&self) -> Arc<Workspace> {
        Arc::clone(&self.workspace.read().unwrap_or_else(|e| e.into_inner()))
    }

    /// One run's events from `from` onward, and whether the run has finished,
    /// without waiting: the same log `/api/events` streams, for a caller that
    /// is not an HTTP stream. `None` when this workspace has no such run.
    pub fn run_events(&self, run: &str, from: usize) -> Option<(Vec<(String, String)>, bool)> {
        let session = self.runs.get(run)?;
        // Finished is read first: an event emitted after this read is still
        // returned by the read below, so a caller that sees `true` together
        // with a batch has everything.
        let finished = session.is_finished();
        Some((session.events_since(from), finished))
    }

    /// The settings as they stand, read afresh so a change another window
    /// saved is seen.
    fn settings(&self) -> settings::Loaded {
        match &self.settings_file {
            Some(file) => settings::load(file),
            None => settings::Loaded::default(),
        }
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
        let workspace = self.workspace();
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

            ("GET", "/api/session") => self.session(&workspace),
            ("GET", "/api/tree") => self.tree(&workspace, request),
            ("POST", "/api/tree/folder") => self.create_folder(&workspace, request),
            ("GET", "/api/document") => self.read_document(&workspace, request),
            ("PUT", "/api/document") => self.save_document(&workspace, request),
            ("POST", "/api/document") => self.create_document(&workspace, request),
            ("DELETE", "/api/document") => self.delete_document(&workspace, request),
            ("GET", "/api/roles") => authoring::roles(&workspace, &self.masters()),
            ("POST", "/api/slots") => authoring::slots(&workspace, request),
            ("GET", "/api/scaffold") => {
                authoring::preview_file(&workspace, &self.masters(), request)
            }
            // `name=` makes a new project in the projects folder; `folder=`
            // fills a folder of this workspace, as it always has.
            ("GET", "/api/project/plan") if request.param("name").is_some() => {
                self.preview_named_project(&workspace, request)
            }
            ("GET", "/api/project/plan") => {
                authoring::preview_project(&workspace, &self.masters(), request, self.ending())
            }
            ("POST", "/api/project") if request.param("name").is_some() => {
                self.create_named_project(&workspace, request)
            }
            ("POST", "/api/project") => {
                authoring::create_project(&workspace, &self.masters(), request, self.ending())
            }
            ("GET", "/api/projects") => self.list_projects(&workspace),
            ("POST", "/api/projects/open") => self.open_projects_folder(),
            ("POST", "/api/project/open") => self.open_project(request),
            ("POST", "/api/projects/forget") => self.forget_project(request),
            ("GET", "/api/project/status") => authoring::project_status(&workspace, request),
            ("GET", "/api/masters") => authoring::list_masters(&workspace, &self.masters()),
            ("GET", "/api/master") => authoring::read_master(&self.masters(), request),
            ("GET", "/api/master/starter") => {
                authoring::master_starter(&workspace, request, self.ending())
            }
            ("PUT", "/api/master") => authoring::save_master(&workspace, &self.masters(), request),
            ("DELETE", "/api/master") => authoring::delete_master(&self.masters(), request),
            ("PUT", "/api/masters/default") => {
                authoring::set_default(&workspace, &self.masters(), request)
            }
            ("GET", "/api/convert/plan") => authoring::preview_conversion(&workspace, request),
            ("POST", "/api/convert") => authoring::convert(&workspace, request),
            ("GET", "/manual") | ("GET", "/manual/") => {
                let token = request
                    .header("x-lcl-token")
                    .or_else(|| request.param("t"))
                    .unwrap_or_default();
                manual::page(token)
            }
            ("GET", "/manual/manual.js") => Response::javascript(manual::VIEWER_JS),
            ("GET", "/manual/manual.css") => Response::css(manual::VIEWER_CSS),
            ("GET", "/manual/snapshot") => manual::snapshot(),

            ("GET", "/api/update")
            | ("POST", "/api/update/check")
            | ("POST", "/api/update/download")
            | ("POST", "/api/update/install") => match &self.updater {
                None => Response::error(404, "this workspace has no updater installed beside it"),
                Some(updater) => match request.path.as_str() {
                    "/api/update" => updater.status(),
                    "/api/update/check" => updater.check(),
                    "/api/update/download" => updater.download(),
                    _ => updater.install(),
                },
            },
            ("GET", "/api/settings") => self.read_settings(),
            ("PUT", "/api/settings") => self.save_settings(request),
            ("GET", "/api/folder") => self.folder(request, false),
            ("POST", "/api/folder") => self.folder(request, true),

            ("POST", "/api/tokens") => self.tokens(request),
            ("POST", "/api/check") => self.analyse(request, Command::Check),
            ("POST", "/api/inspect") => self.analyse(request, Command::Inspect),
            ("POST", "/api/validate") => self.analyse(request, Command::Validate),
            ("POST", "/api/run") => self.start_run(request),
            ("POST", "/api/answer") => self.answer(request),

            // Android devices, through `lcl-remote`; see `crate::remote`.
            ("GET", "/api/remote/devices") => remote_devices(),
            ("POST", "/api/remote/pair") => remote_pair(),
            ("POST", "/api/remote/revoke") => remote_revoke(request),
            ("GET", "/api/remote/pending") => remote_pending(),
            ("POST", "/api/remote/approve") => remote_decide(request, "approve"),
            ("POST", "/api/remote/deny") => remote_decide(request, "deny"),

            ("GET", _) | ("PUT", _) | ("POST", _) | ("DELETE", _) => {
                Response::error(404, "no such route")
            }
            _ => Response::error(405, "method not allowed"),
        }
    }

    /// What this workspace is, for a frontend that just loaded.
    fn session(&self, workspace: &Workspace) -> Response {
        let spec = workspace.engine().spec_record();
        let notice = self
            .notice
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let body = Object::new()
            .with("protocol", Node::string(lcl_protocol::PROTOCOL))
            .with("root", Node::string(workspace.root().display().to_string()))
            .with(
                "spec",
                Object::new()
                    .with("root", Node::string(&spec.root))
                    .with("formal_version", Node::string(&spec.formal_version))
                    .with("identity_digest", Node::string(&spec.identity_digest))
                    .with("authority", Node::string(&spec.authority))
                    .into(),
            )
            // The Core 0.2.0 package, when this workspace judges localized
            // documents with it. `spec` stays the Core 0.1.0 package.
            .with_some(
                "localized_spec",
                workspace.engines().localized().map(|engine| {
                    let spec = engine.spec_record();
                    Object::new()
                        .with("root", Node::string(&spec.root))
                        .with("formal_version", Node::string(&spec.formal_version))
                        .with("identity_digest", Node::string(&spec.identity_digest))
                        .with("authority", Node::string(&spec.authority))
                        .into()
                }),
            )
            .with("entry", Node::optional(workspace.entry()))
            // The document this workspace was launched for, when a file
            // association supplied one. Separate from `entry`, which is the
            // manifest's declared starting document and belongs to the project
            // rather than to this launch.
            .with(
                "open",
                Node::optional(workspace.open_document().map(str::to_string)),
            )
            // How this launch chose its project, when there is something to say.
            .with("notice", Node::optional(notice))
            // The Projects home: `root` is then the Projects folder, a
            // container of projects and not a project, and the page offers
            // the projects in it instead of an explorer.
            .with("home", Node::Bool(self.is_home()))
            .pretty();
        Response::json(body)
    }

    /// Where this person's Master templates live: beside the settings file.
    fn masters(&self) -> Masters {
        match self.settings_file.as_ref().and_then(|file| file.parent()) {
            Some(dir) => Masters::new(dir.join("masters")),
            None => Masters::none(),
        }
    }

    /// The configured default file type: the ending of a new document named
    /// without one, and of every file a canonical new project starts with.
    fn ending(&self) -> &'static str {
        self.settings().settings.default_extension
    }

    /// Token spans for one buffer, produced by the real lexer.
    fn tokens(&self, request: &Request) -> Response {
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(422, &e.to_string()),
        };
        // The id names the document the buffer is, so a localized document is
        // lexed by the engine that judges it.
        let unit = intelligence::unit_of(request.param("id").unwrap_or("buffer.lcl"), text);
        let workspace = self.workspace();
        Response::json(intelligence::tokens_json(
            workspace.engine_for(&unit),
            &unit,
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
        let workspace = self.workspace();
        let provider = workspace.project().provider().map_err(|e| e.to_string())?;
        let unit = intelligence::unit_of(id, text);
        let engine = workspace.engine_for(&unit);
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
