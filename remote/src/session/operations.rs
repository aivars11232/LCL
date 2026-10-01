//! What a device may ask once it is let in: one of a fixed list of operations.
//!
//! A few are about the PC itself. Every other one names a shared project and
//! is answered by the desktop workspace's own routes, called in process, so a
//! device is refused exactly what the workspace would refuse.

use super::admission::pc_node;
use super::legacy_tree::legacy_tree;
use super::wire::{failure, response};
use super::{Session, PROTOCOL, VERSION};
use crate::paths;
use crate::projects::Project;
use lcl_protocol::json::{Node, Object};
use lcl_spec::json::Json;
use lcl_workspace::http::Request;
use lcl_workspace::{Outcome, Route, Routes};
use std::collections::BTreeMap;
use std::sync::Arc;

/// What an operation comes to: a status and a JSON body, or why the request
/// cannot be answered as asked — a field it needs is missing, the operation
/// is not one of the list — which the device is told with status 400.
pub(super) type Answer = Result<(u16, String), String>;

/// A request about one shared project: the project, its routes, and the
/// message asking.
pub(super) struct Asked<'a> {
    pub(super) project: &'a Project,
    pub(super) routes: &'a Arc<Routes>,
    pub(super) message: &'a Json,
}

impl Asked<'_> {
    /// A text field of the message.
    pub(super) fn param(&self, key: &str) -> Option<&str> {
        self.message.get(key).and_then(Json::as_str)
    }

    /// A text field the operation cannot do without; `what` names it in the
    /// refusal.
    pub(super) fn need(&self, key: &str, what: &str) -> Result<String, String> {
        self.param(key)
            .map(str::to_string)
            .ok_or(format!("{what} is required"))
    }

    /// One call to the project's routes.
    pub(super) fn call(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: &str,
    ) -> (u16, String) {
        route(self.routes, method, path, query, body)
    }
}

impl Session {
    /// Answer one request. `None` for a message that is not one, which closes
    /// the connection: a peer that speaks the protocol wrongly is not guessed at.
    pub(super) fn handle(&mut self, frame: &str) -> Option<String> {
        let message = lcl_spec::json::parse(frame).ok()?;
        if message.get("type").and_then(Json::as_str) != Some("request") {
            return None;
        }
        let id = message.get("id").and_then(Json::as_u64)?;
        let op = message.get("op").and_then(Json::as_str)?;
        Some(match op {
            "ping" => response(
                id,
                200,
                &Object::new()
                    .with("pong", Node::u64(paths::now()))
                    .compact(),
            ),
            "about" => response(id, 200, &self.about()),
            // The device forgot this PC and asks to be trusted no longer. It
            // can only ever end its own trust: the device is the one this
            // connection's certificate proves, never one the message names.
            "unpair" => match self.shared.registry.authorize(&self.fingerprint) {
                Ok(device) => match self.shared.registry.revoke(&device.id, paths::now()) {
                    Ok(_) => response(id, 200, "{\"unpaired\":true}"),
                    Err(detail) => failure(id, 500, &detail),
                },
                Err(_) => failure(id, 403, "this device is not trusted here"),
            },
            "projects" => {
                let projects = self.projects();
                let items = projects.iter().map(|p| {
                    Object::new()
                        .with("id", Node::string(&p.id))
                        .with("name", Node::string(&p.name))
                        .with("root", Node::string(p.root.display().to_string()))
                        .with("default", Node::Bool(p.default))
                        .into()
                });
                response(
                    id,
                    200,
                    &Object::new().with("projects", Node::array(items)).compact(),
                )
            }
            _ => self.project_op(id, op, &message),
        })
    }

    /// One operation on a shared project, which the request names.
    fn project_op(&mut self, id: u64, op: &str, message: &Json) -> String {
        let opened = match message.get("project").and_then(Json::as_str) {
            Some(project) => self.shared.projects.open(&self.shared.paths, project),
            None => Ok(None),
        };
        let (project, routes) = match opened {
            Ok(Some(opened)) => opened,
            Ok(None) => return failure(id, 404, "no such project is shared by this PC"),
            Err(e) => return failure(id, 500, &e),
        };
        let asked = Asked {
            project: &project,
            routes: &routes,
            message,
        };
        match self.perform(op, &asked) {
            Ok((status, body)) => response(id, status, &body),
            Err(detail) => failure(id, 400, &detail),
        }
    }

    /// The operations on a project. The ones that are one call to the
    /// workspace are made here; the ones that touch what this session holds —
    /// its open documents, its runs — are in `documents` and `runs`.
    fn perform(&mut self, op: &str, asked: &Asked) -> Answer {
        Ok(match op {
            "session" => asked.call("GET", "/api/session", &[], ""),
            // The explorer, one folder at a time: the direct children of
            // `parent` (the root for none), and nothing below them.
            "children" => {
                let parent = asked.param("parent").unwrap_or("").to_string();
                asked.call("GET", "/api/tree", &[("parent", parent)], "")
            }
            // One empty folder in the project, where the device asked.
            "mkdir" => {
                let folder = asked.need("folder", "a folder")?;
                asked.call("POST", "/api/tree/folder", &[("id", folder)], "")
            }
            // Kept for the apps published before `children` existed (LCL
            // 0.5.0 and earlier), which draw the whole project from one
            // answer: the same explorer, folder by folder, flattened into
            // that answer's shape, parents before children, bounded as
            // that answer was. A current app never asks for it.
            "tree" => legacy_tree(asked),
            "settings" => asked.call("GET", "/api/settings", &[], ""),
            "open" => self.open(asked)?,
            "close" => self.close(asked)?,
            "save" => self.save(asked)?,
            "roles" => asked.call("GET", "/api/roles", &[], ""),
            "scaffold" => scaffold(asked)?,
            // Project readiness: the engine's validate report for the
            // entry as it is on the PC's disk.
            "project" => {
                let entry = asked.need("entry", "an entry")?;
                asked.call("GET", "/api/project/status", &[("entry", entry)], "")
            }
            "create" => self.create(asked)?,
            "delete" => self.delete(asked)?,
            "tokens" | "check" | "validate" | "inspect" => {
                let document = asked.need("document", "a document")?;
                let text = asked.need("text", "the text")?;
                asked.call("POST", &format!("/api/{op}"), &[("id", document)], &text)
            }
            "run" => self.run(asked)?,
            "follow" => self.follow(asked)?,
            "answer" => self.answer(asked)?,
            other => return Err(format!("unknown operation {other}")),
        })
    }

    /// Who this PC is and what it runs, from the engine itself.
    fn about(&self) -> String {
        let mut cached = self.shared.about.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(about) = cached.as_ref() {
            return about.clone();
        }
        let spec = |path: Option<&std::path::Path>, localized: bool| -> Node {
            let Some(path) = path else { return Node::Null };
            let engine = if localized {
                lcl_protocol::Engine::open_localized(path, &[])
            } else {
                lcl_protocol::Engine::open(path)
            };
            match engine {
                Ok(engine) => {
                    let record = engine.spec_record();
                    Object::new()
                        .with("formal_version", Node::string(&record.formal_version))
                        .with("identity_digest", Node::string(&record.identity_digest))
                        .with("authority", Node::string(&record.authority))
                        .into()
                }
                Err(e) => Object::new()
                    .with("error", Node::string(e.to_string()))
                    .into(),
            }
        };
        let specs = self.shared.projects.specs();
        let about = Object::new()
            .with("pc", pc_node(&self.shared.identity))
            .with("service", Node::string(format!("lcl-remote {VERSION}")))
            .with("protocol", Node::string(PROTOCOL))
            .with("engine_protocol", Node::string(lcl_protocol::PROTOCOL))
            .with("core", spec(Some(&specs.core), false))
            .with("localized", spec(specs.localized.as_deref(), true))
            .compact();
        *cached = Some(about.clone());
        about
    }
}

/// The exact starting text of a file of one role, written nowhere: the PC's
/// scaffold or Master, never a phone's copy.
fn scaffold(asked: &Asked) -> Answer {
    let mut query = vec![
        ("role", asked.need("role", "a role")?),
        ("path", asked.need("path", "a path")?),
    ];
    for key in ["mode", "master", "source"] {
        if let Some(value) = asked.param(key) {
            query.push((key, value.to_string()));
        }
    }
    Ok(asked.call("GET", "/api/scaffold", &query, ""))
}

/// Call one workspace route in process. The route table is the workspace's
/// own; nothing here reaches past it.
pub(super) fn route(
    routes: &Routes,
    method: &str,
    path: &str,
    query: &[(&str, String)],
    body: &str,
) -> (u16, String) {
    let request = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: query
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
    };
    match routes.handle(&request) {
        Outcome::Reply(response) => (
            response.status,
            String::from_utf8(response.body).unwrap_or_else(|_| "{}".into()),
        ),
        Outcome::Stream(_) => (
            400,
            "{\"error\":\"streams are not available remotely\"}".into(),
        ),
    }
}
