//! Creating files and projects by role, Master templates, project readiness
//! and converting a standalone document into a project, as HTTP routes.
//!
//! Nothing here decides a language rule. Roles, scaffolds and Master validity
//! are the Core 0.3.0 engine's ([`lcl_protocol::scaffold`]); which text a new
//! file starts from is [`crate::masters`]'s priority; readiness is the
//! engine's own project record in a `validate` report. These routes only carry
//! those answers to the desktop page and, through the PC service, to a phone.

use crate::document;
use crate::http::{Request, Response};
use crate::intelligence;
use crate::masters::{self, Masters, Origin, Planned, Selection};
use crate::project::Workspace;
use lcl_lexer::TokenKind;
use lcl_protocol::json::{Node, Object};
use lcl_protocol::scaffold::{self, Mode, PROJECT_KIND};
use lcl_protocol::{Engine, Inputs};
use lcl_resolver::MemoryProvider;
use std::path::{Path, PathBuf};

/// The Core 0.3.0 engine, or the refusal that says how to provide it.
pub fn engine(workspace: &Workspace) -> Result<&Engine, Response> {
    workspace.project_engine().ok_or_else(|| {
        Response::error(
            409,
            "Core 0.3.0 is not available in this workspace: start it with --project-spec \
             <LCL_Core_0.3.0>, set LCL_PROJECT_SPEC, or declare \"project_spec\" in \
             lcl.project.json",
        )
    })
}

/// What a person reads for a role: the last segment of its canonical kind,
/// capitalised. `kind.part.task` is "Task", `kind.project` is "Project".
pub fn label(role: &str) -> String {
    let last = role.rsplit('.').next().unwrap_or(role);
    let mut chars = last.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Which text a request asks for: `master=<id>` names a Master; otherwise
/// `source=canonical` asks for the canonical scaffold and anything else for
/// the role's default Master or, without one, the canonical scaffold. `mode`
/// is `minimal` or `guided` (the default).
fn selection(request: &Request) -> Result<Selection<'_>, Response> {
    let mode = match request.param("mode").filter(|m| !m.is_empty()) {
        None => Mode::Guided,
        Some(text) => {
            Mode::parse(text).ok_or_else(|| Response::error(400, "mode is minimal or guided"))?
        }
    };
    match (
        request.param("master").filter(|m| !m.is_empty()),
        request.param("source").filter(|s| !s.is_empty()),
    ) {
        (Some(id), None) => Ok(Selection::Master(id)),
        (None, Some("canonical")) => Ok(Selection::Canonical(mode)),
        (None, None | Some("default")) => Ok(Selection::Automatic(mode)),
        _ => Err(Response::error(
            400,
            "choose a Master (master=<id>), the canonical scaffold (source=canonical) or the \
             default (source=default)",
        )),
    }
}

fn marks_json(marks: &[scaffold::Mark]) -> Node {
    Node::array(marks.iter().map(|mark| {
        Object::new()
            .with("kind", Node::string(mark.kind.as_str()))
            .with("line", Node::usize(mark.line))
            .with("block", Node::string(&mark.block))
            .with("field", Node::string(&mark.field))
            .into()
    }))
}

fn planned_json(file: &Planned, path: &str) -> Node {
    let origin = match &file.origin {
        Origin::Canonical(mode) => Object::new()
            .with("kind", Node::string("canonical"))
            .with("mode", Node::string(mode.as_str())),
        Origin::Master(id) => Object::new()
            .with("kind", Node::string("master"))
            .with("master", Node::string(id)),
    };
    let shown = scaffold::DOCUMENT_TYPES
        .iter()
        .find(|t| t.id == file.doc_type)
        .map_or_else(|| label(&file.role), |t| t.label.to_string());
    Object::new()
        .with("path", Node::string(path))
        .with("role", Node::string(&file.role))
        .with("type", Node::string(&file.doc_type))
        .with("label", Node::string(shown))
        .with("origin", origin.into())
        .with("text", Node::string(&file.scaffold.text))
        .with(
            "digest",
            Node::string(lcl_spec::sha256::hex_digest(file.scaffold.text.as_bytes())),
        )
        .with("marks", marks_json(&file.scaffold.marks))
        .into()
}

/// `POST /api/slots?id=`: the slot, guidance and generated-ID marks of a
/// buffer that declares a project file role, so an editor can show what is
/// still to be filled in. The engine computes them
/// ([`scaffold::check_text`]); a buffer that is not, or not yet, a
/// structurally valid file of its role has none, and says why.
pub fn slots(workspace: &Workspace, request: &Request) -> Response {
    let Some(id) = request.param("id") else {
        return Response::error(400, "a document id is required");
    };
    let text = match request.text() {
        Ok(text) => text,
        Err(e) => return Response::error(422, &e.to_string()),
    };
    let role = declared_kind(workspace, id, text);
    let (marks, problem) = match (workspace.project_engine(), role.as_deref()) {
        (Some(engine), Some(role)) if scaffold::roles(engine).iter().any(|r| r == role) => {
            match scaffold::check_text(engine, role, text) {
                Ok(marks) => (marks, None),
                Err(e) => (Vec::new(), Some(e.0)),
            }
        }
        _ => (Vec::new(), None),
    };
    Response::json(
        Object::new()
            .with("role", role.map_or(Node::Null, Node::string))
            .with("marks", marks_json(&marks))
            .with("problem", problem.map_or(Node::Null, Node::string))
            .pretty(),
    )
}

/// `GET /api/roles`: the roles a new file can have, from the engine.
pub fn roles(workspace: &Workspace, masters: &Masters) -> Response {
    let Some(engine) = workspace.project_engine() else {
        return Response::json(
            Object::new()
                .with("available", Node::Bool(false))
                .with("roles", Node::array(std::iter::empty()))
                .pretty(),
        );
    };
    let defaults = masters.defaults().unwrap_or_default();
    // `role` is what a request names to start a file of this type: a role's
    // own type is named by the role, so a caller that knows only roles keeps
    // working; `kind` is the role the file declares.
    let entry = |id: &str, label: &str, kind: &str| -> Node {
        Object::new()
            .with("role", Node::string(id))
            .with("label", Node::string(label))
            .with("kind", Node::string(kind))
            .with(
                "default_master",
                defaults.get(id).map_or(Node::Null, Node::string),
            )
            .into()
    };
    Response::json(
        Object::new()
            .with("available", Node::Bool(true))
            .with("core", Node::string(engine.spec().formal_version()))
            .with(
                "roles",
                Node::array(
                    scaffold::document_types(engine)
                        .iter()
                        .map(|t| entry(t.id, t.label, t.role)),
                ),
            )
            .with(
                "project",
                entry(PROJECT_KIND, &label(PROJECT_KIND), PROJECT_KIND),
            )
            .with(
                "modes",
                Node::array([Mode::Minimal, Mode::Guided].map(|m| Node::string(m.as_str()))),
            )
            .pretty(),
    )
}

/// The file `request` asks for at `path`, exactly as it would be written.
pub fn file(
    workspace: &Workspace,
    masters: &Masters,
    request: &Request,
    path: &str,
) -> Result<Planned, Response> {
    let engine = engine(workspace)?;
    let Some(role) = request.param("role") else {
        return Err(Response::error(400, "a role is required"));
    };
    if role == PROJECT_KIND {
        return Err(Response::error(
            400,
            "a project entry is made with New Project, not as one file",
        ));
    }
    let selection = selection(request)?;
    masters
        .file(engine, role, selection, path, None)
        .map_err(|e| Response::error(422, &e))
}

/// `GET /api/scaffold?role=&path=`: a new file's exact text and marks,
/// written nowhere.
pub fn preview_file(workspace: &Workspace, masters: &Masters, request: &Request) -> Response {
    let Some(path) = request.param("path") else {
        return Response::error(400, "a path is required");
    };
    match file(workspace, masters, request, path) {
        Ok(planned) => Response::json(planned_json(&planned, path).pretty()),
        Err(refusal) => refusal,
    }
}

/// The project folder a request names, checked to be a plain relative path
/// inside the workspace. The empty folder is the workspace root.
fn folder(workspace: &Workspace, request: &Request) -> Result<String, Response> {
    let folder = request.param("folder").unwrap_or("").trim_matches('/');
    if !folder.is_empty() {
        scaffold::check_path(&format!("{folder}/probe.lcl"))
            .map_err(|e| Response::error(400, &format!("the folder: {e}")))?;
        document::resolve(workspace.root(), &format!("{folder}/probe.lcl"))
            .map_err(|e| Response::error(400, &e.to_string()))?;
    }
    Ok(folder.to_string())
}

fn prefixed(folder: &str, path: &str) -> String {
    if folder.is_empty() {
        path.to_string()
    } else {
        format!("{folder}/{path}")
    }
}

/// Every file of a new project and the digest that binds a creation to it.
fn project_files(
    workspace: &Workspace,
    masters: &Masters,
    request: &Request,
    ending: &str,
) -> Result<(String, Vec<Planned>, String), Response> {
    let engine = engine(workspace)?;
    let folder = folder(workspace, request)?;
    let selection = selection(request)?;
    let files = masters
        .project(engine, selection, None, ending)
        .map_err(|e| Response::error(422, &e))?;
    for file in &files {
        let path = prefixed(&folder, &file.path);
        document::resolve(workspace.root(), &path)
            .map_err(|e| Response::error(400, &e.to_string()))?;
    }
    let digest = plan_digest(&folder, &files);
    Ok((folder, files, digest))
}

/// SHA-256 over every planned path, role and text, in plan order.
fn plan_digest(folder: &str, files: &[Planned]) -> String {
    let mut bytes = Vec::new();
    for file in files {
        for part in [
            prefixed(folder, &file.path).as_str(),
            file.role.as_str(),
            file.scaffold.text.as_str(),
        ] {
            bytes.extend_from_slice(part.len().to_string().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(part.as_bytes());
        }
    }
    lcl_spec::sha256::hex_digest(&bytes)
}

/// `GET /api/project/plan?folder=`: the exact files New Project would
/// create, written nowhere. `ending` is the configured default file type,
/// which the canonical plan's names take.
pub fn preview_project(
    workspace: &Workspace,
    masters: &Masters,
    request: &Request,
    ending: &str,
) -> Response {
    match project_files(workspace, masters, request, ending) {
        Ok((folder, files, digest)) => {
            Response::json(plan_json_rooted(workspace, &folder, &files, &digest))
        }
        Err(refusal) => refusal,
    }
}

fn plan_json_rooted(
    workspace: &Workspace,
    folder: &str,
    files: &[Planned],
    digest: &str,
) -> String {
    Object::new()
        .with("folder", Node::string(folder))
        .with("entry", Node::string(prefixed(folder, &files[0].path)))
        .with("plan_digest", Node::string(digest))
        .with(
            "files",
            Node::array(files.iter().map(|f| {
                let path = prefixed(folder, &f.path);
                let mut node = planned_json(f, &path);
                if let Node::Object(object) = &mut node {
                    object.set("exists", Node::Bool(workspace.exists(&path)));
                }
                node
            })),
        )
        .pretty()
}

/// `POST /api/project?folder=&plan_digest=`: create exactly the previewed
/// files, all or none. A plan that no longer has the previewed digest — a
/// Master or default changed in between — is refused and nothing is written.
pub fn create_project(
    workspace: &Workspace,
    masters: &Masters,
    request: &Request,
    ending: &str,
) -> Response {
    let (folder, files, digest) = match project_files(workspace, masters, request, ending) {
        Ok(planned) => planned,
        Err(refusal) => return refusal,
    };
    if let Err(refusal) = previewed(request, &digest) {
        return refusal;
    }
    let root = workspace.root().join(&folder);
    match masters::create(&root, &files) {
        Ok(()) => Response::json(plan_json_rooted(workspace, &folder, &files, &digest)),
        Err(detail) => Response::error(409, &detail),
    }
}

/// That a creation names the digest of the plan it was previewed as, and the
/// plan still has it: 428 without one, 409 when it changed in between.
pub fn previewed(request: &Request, digest: &str) -> Result<(), Response> {
    match request.param("plan_digest") {
        Some(previewed) if previewed == digest => Ok(()),
        Some(_) => Err(Response::error(
            409,
            "the project plan changed since it was previewed; nothing was created",
        )),
        None => Err(Response::error(
            428,
            "a project is created only from a preview: pass its plan_digest",
        )),
    }
}

/// A new project named by `name=`: its own new folder in the projects folder.
pub struct NamedProject {
    pub name: String,
    /// `<projects folder>/<name>`.
    pub dir: PathBuf,
    /// Relative to `dir`, in plan order, the entry first.
    pub files: Vec<Planned>,
    /// Binds a creation to this preview: the absolute folder and every file.
    pub digest: String,
}

/// A project name: one folder name of ASCII letters, digits, `_`, `-` and
/// `.`, not starting with `.`, so the project can only be created directly
/// inside the projects folder.
fn project_name(request: &Request) -> Result<String, Response> {
    let name = request.param("name").unwrap_or("");
    if name.is_empty() {
        return Err(Response::error(400, "a project name is required"));
    }
    if name.contains('/') || scaffold::check_path(&format!("{name}/probe.lcl")).is_err() {
        return Err(Response::error(
            400,
            &format!(
                "{name:?} cannot be a project name: use letters, digits, '_', '-' and '.', \
                 not starting with '.', and no '/'"
            ),
        ));
    }
    Ok(name.to_string())
}

/// The files a new project called `name=` starts with, in the folder of that
/// name inside `projects`. Nothing is checked on disk here: whether the
/// folder is free is for the caller to say and to insist on.
pub fn named_project(
    workspace: &Workspace,
    masters: &Masters,
    request: &Request,
    projects: &Path,
    ending: &str,
) -> Result<NamedProject, Response> {
    let engine = engine(workspace)?;
    let name = project_name(request)?;
    let selection = selection(request)?;
    let files = masters
        .project(engine, selection, None, ending)
        .map_err(|e| Response::error(422, &e))?;
    let dir = projects.join(&name);
    let digest = plan_digest(&dir.display().to_string(), &files);
    Ok(NamedProject {
        name,
        dir,
        files,
        digest,
    })
}

/// `dir`, relative to `root` with `/` separators, when it is inside `root`.
pub fn inside(dir: &Path, root: &Path) -> Option<String> {
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let dir = match (dir.parent(), dir.file_name()) {
        // The project folder may not exist yet; the projects folder does.
        (Some(parent), Some(name)) => real(parent).join(name),
        _ => real(dir),
    };
    let relative = dir.strip_prefix(real(root)).ok()?;
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A named project as the page shows it: where it goes, whether that folder
/// is already taken, whether creating it opens the projects folder in this
/// window (it does when the project would otherwise be outside what the
/// window shows), and its files. `folder` and `entry` are ids in the window
/// as it will be after the creation.
pub fn named_plan_json(plan: &NamedProject, projects: &Path, root: &Path) -> String {
    let (folder, opens) = match inside(&plan.dir, root) {
        Some(folder) => (folder, false),
        None => (plan.name.clone(), true),
    };
    Object::new()
        .with("name", Node::string(&plan.name))
        .with("projects", Node::string(projects.display().to_string()))
        .with("path", Node::string(plan.dir.display().to_string()))
        .with(
            "exists",
            Node::Bool(std::fs::symlink_metadata(&plan.dir).is_ok()),
        )
        .with("opens_projects_folder", Node::Bool(opens))
        .with("folder", Node::string(&folder))
        .with(
            "entry",
            Node::string(prefixed(&folder, &plan.files[0].path)),
        )
        .with("plan_digest", Node::string(&plan.digest))
        .with(
            "files",
            Node::array(
                plan.files
                    .iter()
                    .map(|f| planned_json(f, &prefixed(&folder, &f.path))),
            ),
        )
        .pretty()
}

// ---------------------------------------------------------------------------
// Masters
// ---------------------------------------------------------------------------

fn unavailable() -> Response {
    Response::error(
        409,
        "Master templates need a configuration directory ($XDG_CONFIG_HOME or $HOME)",
    )
}

/// `GET /api/masters`: every stored Master, whether it is valid now, and the
/// default of each document type.
pub fn list_masters(workspace: &Workspace, masters: &Masters) -> Response {
    if !masters.is_available() {
        return Response::json(
            Object::new()
                .with("available", Node::Bool(false))
                .with("masters", Node::array(std::iter::empty()))
                .pretty(),
        );
    }
    let ids = match masters.ids() {
        Ok(ids) => ids,
        Err(detail) => return Response::error(500, &detail),
    };
    let defaults = masters.defaults();
    let mut items = Vec::new();
    for id in &ids {
        let mut item = Object::new().with("id", Node::string(id));
        match masters.read(id) {
            Ok(master) => {
                let problem = match workspace.project_engine() {
                    Some(engine) => masters.check(engine, &master).err(),
                    None => Some("Core 0.3.0 is not available in this workspace".to_string()),
                };
                let shown = scaffold::DOCUMENT_TYPES
                    .iter()
                    .find(|t| t.id == master.doc_type)
                    .map_or_else(|| label(&master.role), |t| t.label.to_string());
                item = item
                    .with("name", Node::string(&master.name))
                    .with("role", Node::string(&master.role))
                    .with("type", Node::string(&master.doc_type))
                    .with("label", Node::string(shown))
                    .with("core", Node::string(&master.core))
                    .with("valid", Node::Bool(problem.is_none()))
                    .with("problem", problem.map_or(Node::Null, |p| Node::string(&p)));
            }
            Err(problem) => {
                item = item
                    .with("valid", Node::Bool(false))
                    .with("problem", Node::string(&problem));
            }
        }
        items.push(Node::from(item));
    }
    let mut defaults_node = Object::new();
    let defaults_problem = match defaults {
        Ok(defaults) => {
            for (role, id) in defaults {
                defaults_node.set(role, Node::string(&id));
            }
            Node::Null
        }
        Err(problem) => Node::string(&problem),
    };
    Response::json(
        Object::new()
            .with("available", Node::Bool(true))
            .with(
                "location",
                Node::string(masters.dir().display().to_string()),
            )
            .with("defaults", defaults_node.into())
            .with("defaults_problem", defaults_problem)
            .with("masters", Node::array(items))
            .pretty(),
    )
}

/// `GET /api/master?id=`: one Master's file, as stored.
pub fn read_master(masters: &Masters, request: &Request) -> Response {
    if !masters.is_available() {
        return unavailable();
    }
    let Some(id) = request.param("id") else {
        return Response::error(400, "a Master id is required");
    };
    match masters.raw(id) {
        Ok(json) => Response::json(
            Object::new()
                .with("id", Node::string(id))
                .with("json", Node::string(&json))
                .pretty(),
        ),
        Err(detail) => Response::error(404, &detail),
    }
}

/// `GET /api/master/starter?role=&mode=`: the text of a new Master for
/// document type `role`, starting from the canonical scaffold (a project
/// Master's names end with `ending`). Written nowhere.
pub fn master_starter(workspace: &Workspace, request: &Request, ending: &str) -> Response {
    let engine = match engine(workspace) {
        Ok(engine) => engine,
        Err(refusal) => return refusal,
    };
    let Some(role) = request.param("role") else {
        return Response::error(400, "a role is required");
    };
    let mode = match request.param("mode").map(Mode::parse) {
        None => Mode::Guided,
        Some(Some(mode)) => mode,
        Some(None) => return Response::error(400, "mode is minimal or guided"),
    };
    match masters::starter(engine, role, mode, ending) {
        Ok(json) => Response::json(Object::new().with("json", Node::string(&json)).pretty()),
        Err(detail) => Response::error(422, &detail),
    }
}

/// `PUT /api/master?create=1` or `PUT /api/master?replace=<id>`: store the
/// Master in the body after the engine accepts it. `create` refuses an id
/// already in use; `replace` refuses a body declaring another id.
pub fn save_master(workspace: &Workspace, masters: &Masters, request: &Request) -> Response {
    if !masters.is_available() {
        return unavailable();
    }
    let engine = match engine(workspace) {
        Ok(engine) => engine,
        Err(refusal) => return refusal,
    };
    let json = match request.text() {
        Ok(text) => text,
        Err(e) => return Response::error(422, &e.to_string()),
    };
    let declared = match scaffold::parse_master(json) {
        Ok(master) => master.id,
        Err(e) => return Response::error(422, &e.0),
    };
    match (request.param("create"), request.param("replace")) {
        (Some("1"), None) => {
            if masters.ids().unwrap_or_default().contains(&declared) {
                return Response::error(409, &format!("a Master {declared} already exists"));
            }
        }
        (None, Some(id)) => {
            if id != declared {
                return Response::error(
                    409,
                    &format!("editing Master {id} cannot change its id to {declared}; duplicate it instead"),
                );
            }
            if !masters.ids().unwrap_or_default().iter().any(|m| m == id) {
                return Response::error(404, &format!("no Master {id}"));
            }
        }
        _ => return Response::error(400, "say create=1 or replace=<id>"),
    }
    match masters.save(engine, json) {
        Ok(master) => Response::json(
            Object::new()
                .with("id", Node::string(&master.id))
                .with("role", Node::string(&master.role))
                .pretty(),
        ),
        Err(detail) => Response::error(422, &detail),
    }
}

/// `DELETE /api/master?id=`. Files made from it are never touched.
pub fn delete_master(masters: &Masters, request: &Request) -> Response {
    if !masters.is_available() {
        return unavailable();
    }
    let Some(id) = request.param("id") else {
        return Response::error(400, "a Master id is required");
    };
    match masters.delete(id) {
        Ok(()) => Response::json(Object::new().with("deleted", Node::string(id)).pretty()),
        Err(detail) => Response::error(404, &detail),
    }
}

/// `PUT /api/masters/default?role=&id=`: make `id` the role's default, or with
/// no `id` go back to the canonical scaffold.
pub fn set_default(workspace: &Workspace, masters: &Masters, request: &Request) -> Response {
    if !masters.is_available() {
        return unavailable();
    }
    let engine = match engine(workspace) {
        Ok(engine) => engine,
        Err(refusal) => return refusal,
    };
    let Some(role) = request.param("role") else {
        return Response::error(400, "a role is required");
    };
    let id = request.param("id").filter(|id| !id.is_empty());
    match masters.set_default(engine, role, id) {
        Ok(()) => Response::json(
            Object::new()
                .with("role", Node::string(role))
                .with("default", id.map_or(Node::Null, Node::string))
                .pretty(),
        ),
        Err(detail) => Response::error(422, &detail),
    }
}

// ---------------------------------------------------------------------------
// Roles in the tree, and readiness
// ---------------------------------------------------------------------------

/// The `SPECIFICATION` `KIND` a document declares, as the engine that judges
/// it parses it; `None` when it does not parse that far. This is what the
/// file says it is, never a guess from its name.
///
/// A file that does not parse — a new scaffold, whose slots are still empty,
/// is one — is read at the lexical level instead: the engine's own tokens,
/// `KIND`, `:` and an identifier directly inside the top-level
/// `SPECIFICATION` block.
pub fn declared_kind(workspace: &Workspace, id: &str, text: &str) -> Option<String> {
    let unit = intelligence::unit_of(id, text);
    let engine = workspace.engine_for(&unit);
    let staged = engine.stage(&unit);
    if let Some(document) = staged.document() {
        let spec = document.block("SPECIFICATION")?;
        let field = spec.fields("KIND").next()?;
        let raw = text.get(field.span.start..field.span.end)?;
        let (_, value) = raw.split_once(':')?;
        let value = value.trim();
        return (!value.is_empty() && !value.contains('\n')).then(|| value.to_string());
    }
    let source = staged.source();
    let tokens: Vec<_> = staged
        .lexed()
        .tokens()
        .iter()
        .filter(|t| t.kind != TokenKind::Space)
        .collect();
    let mut depth = 0i32;
    let mut in_spec = false;
    for (i, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Indent => depth += 1,
            TokenKind::Dedent => depth -= 1,
            TokenKind::ReservedWord => {
                let word = token.word(source).unwrap_or_default();
                if depth == 0 {
                    in_spec = word == "SPECIFICATION";
                } else if depth == 1 && in_spec && word == "KIND" {
                    let (colon, value) = (tokens.get(i + 1)?, tokens.get(i + 2)?);
                    let named = matches!(
                        value.kind,
                        TokenKind::QualifiedIdentifier | TokenKind::SimpleIdentifier
                    );
                    return (colon.kind == TokenKind::Symbol && named)
                        .then(|| {
                            text.get(value.span.start..value.span.end)
                                .map(str::to_string)
                        })
                        .flatten();
                }
            }
            _ => {}
        }
    }
    None
}

/// `GET /api/project/status?entry=`: `validate` over the entry as it is on
/// disk, which is what a Run would start from. The reply is the engine's
/// report, unaltered; its `project` member is the readiness record.
pub fn project_status(workspace: &Workspace, request: &Request) -> Response {
    let Some(entry) = request.param("entry") else {
        return Response::error(400, "an entry is required");
    };
    let document = match workspace.read(entry) {
        Ok(document) => document,
        Err(e) => return Response::error(404, &e.to_string()),
    };
    let provider = match workspace.project().provider() {
        Ok(provider) => provider,
        Err(e) => return Response::error(500, &e.to_string()),
    };
    let unit = intelligence::unit_of(entry, &document.text);
    let report = workspace
        .engine_for(&unit)
        .validate(&unit, &provider, &Inputs::new());
    Response::json(report.to_json().pretty())
}

// ---------------------------------------------------------------------------
// Converting a standalone document into a project
// ---------------------------------------------------------------------------

/// The file name of a converted project's entry and of its one part.
const CONVERTED_ENTRY: &str = "main.lcl";
const CONVERTED_PART: &str = "task.lcl";

/// A conversion: the original's id, every file to create as (path, text), and
/// the digest binding a creation to the preview.
type Conversion = (String, Vec<(String, String)>, String);

/// The one decomposition that needs no choice.
///
/// `05_SEMANTICS/13`: `kind.part.task` admits every family of `kind.task`
/// top-level blocks, and IMPORT, EXTENSION and EXECUTE belong to the entry.
/// So a `kind.task` document becomes an entry holding its IMPORT, EXTENSION
/// and EXECUTE blocks and one task part holding every other top-level item,
/// each in source order and byte for byte. Splitting the families further
/// would mean choosing, for COMMENT and EXAMPLE at least, which part a block
/// belongs to; that is left to the person, after conversion.
///
/// Refused, with the reason, unless: the document is judged by the canonical
/// (unlocalized) reading, declares `kind.task`, and the converted project is
/// admitted by the Core 0.3.0 engine's `validate`. The original is never
/// modified.
fn converted(workspace: &Workspace, request: &Request) -> Result<Conversion, Response> {
    let engine03 = engine(workspace)?;
    let Some(id) = request.param("id") else {
        return Err(Response::error(400, "a document id is required"));
    };
    let folder = folder(workspace, request)?;
    if folder.is_empty() {
        return Err(Response::error(
            400,
            "a converted project needs its own folder",
        ));
    }
    let original = workspace
        .read(id)
        .map_err(|e| Response::error(404, &e.to_string()))?;
    let text = original.text.as_str();
    let unit = intelligence::unit_of(id, text);
    let judge = workspace.engine_for(&unit);
    if judge.localization_contract().is_some() && judge.spec().formal_version() != "0.3.0" {
        return Err(Response::error(
            422,
            "a localized document is not converted automatically: the new entry would mix \
             spellings. Create a project with New Project and copy the blocks into its files",
        ));
    }
    let staged = judge.stage(&unit);
    let Some(parsed) = staged.document() else {
        return Err(Response::error(
            422,
            "the document does not parse, so its blocks cannot be placed; fix it first",
        ));
    };
    let refuse = |detail: &str| Response::error(422, detail);
    let (Some(lcl), Some(spec)) = (parsed.block("LCL"), parsed.block("SPECIFICATION")) else {
        return Err(refuse("the document has no LCL and SPECIFICATION headers"));
    };
    match declared_kind(workspace, id, text).as_deref() {
        Some("kind.task") => {}
        Some(PROJECT_KIND) => return Err(refuse("the document is already a project entry")),
        Some(other) => {
            return Err(refuse(&format!(
                "only a kind.task document can be converted; this one is {other}, and which \
                 part its blocks belong to is not decided by the language"
            )))
        }
        None => return Err(refuse("the document declares no SPECIFICATION KIND")),
    }
    let one_line_field = |block: &lcl_parser::syntax::Block, name: &str| {
        block
            .fields(name)
            .next()
            .filter(|f| !text[f.span.start..f.span.end].contains('\n'))
            .map(|f| (f.span.start, f.span.end))
    };
    let Some(version) = one_line_field(lcl, "VERSION") else {
        return Err(refuse("the LCL header has no one-line VERSION"));
    };
    let (Some(spec_id), Some(spec_kind)) =
        (one_line_field(spec, "ID"), one_line_field(spec, "KIND"))
    else {
        return Err(refuse("the SPECIFICATION has no one-line ID and KIND"));
    };
    let spec_identifier = text[spec_id.0..spec_id.1]
        .split_once(':')
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();

    // Each top-level item runs from its first byte to the next item's first
    // byte, so blank lines and comments between blocks travel with the block
    // before them.
    let starts: Vec<usize> = parsed.items.iter().map(|i| i.span().start).collect();
    let slice = |index: usize| -> &str {
        let end = starts.get(index + 1).copied().unwrap_or(text.len());
        &text[starts[index]..end]
    };
    let entry_blocks: Vec<usize> = parsed
        .blocks()
        .filter(|b| matches!(b.key.text.as_str(), "IMPORT" | "EXTENSION" | "EXECUTE"))
        .map(|b| b.span.start)
        .collect();
    let header_blocks = [lcl.span.start, spec.span.start];
    let mut entry_items = String::new();
    let mut part_items = String::new();
    for (index, start) in starts.iter().enumerate() {
        if header_blocks.contains(start) {
            continue;
        }
        let item = slice(index);
        let target = if entry_blocks.contains(start) {
            &mut entry_items
        } else {
            &mut part_items
        };
        target.push_str(item);
        if !target.ends_with("\n\n") {
            target.push('\n');
        }
    }
    let with = |source: &str, edits: &[((usize, usize), String)]| -> String {
        let mut out = String::new();
        let mut at = 0;
        let mut sorted: Vec<_> = edits.to_vec();
        sorted.sort_by_key(|e| e.0 .0);
        for ((start, end), replacement) in sorted {
            out.push_str(&source[at..start]);
            out.push_str(&replacement);
            at = end;
        }
        out.push_str(&source[at..]);
        out
    };
    let header =
        |block_start: usize, index_of_next: usize| -> &str { &text[block_start..index_of_next] };
    let next_after = |start: usize| {
        starts
            .iter()
            .copied()
            .find(|s| *s > start)
            .unwrap_or(text.len())
    };
    let lcl_text = header(lcl.span.start, next_after(lcl.span.start));
    let spec_text = header(spec.span.start, next_after(spec.span.start));
    let shift = |span: (usize, usize), base: usize| (span.0 - base, span.1 - base);
    let lcl_header = with(
        lcl_text,
        &[(
            shift(version, lcl.span.start),
            "VERSION: \"0.3.0\"".to_string(),
        )],
    );
    let entry_spec = with(
        spec_text,
        &[(
            shift(spec_kind, spec.span.start),
            format!("KIND: {PROJECT_KIND}"),
        )],
    );
    let part_spec = with(
        spec_text,
        &[
            (
                shift(spec_id, spec.span.start),
                format!("ID: {spec_identifier}.task"),
            ),
            (
                shift(spec_kind, spec.span.start),
                "KIND: kind.part.task".to_string(),
            ),
        ],
    );
    let part_declaration = format!(
        "PART:\n    ID: part.task\n    SOURCE: PATH(\"{CONVERTED_PART}\")\n    KIND: kind.part.task\n\n"
    );
    let tidy = |mut s: String| {
        while s.ends_with("\n\n") {
            s.pop();
        }
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    };
    let entry_text = tidy(format!(
        "{lcl_header}{entry_spec}{part_declaration}{entry_items}"
    ));
    let part_text = tidy(format!("{lcl_header}{part_spec}{part_items}"));
    let entry_path = format!("{folder}/{CONVERTED_ENTRY}");
    let part_path = format!("{folder}/{CONVERTED_PART}");
    for path in [&entry_path, &part_path] {
        document::resolve(workspace.root(), path)
            .map_err(|e| Response::error(400, &e.to_string()))?;
        if workspace.exists(path) {
            return Err(Response::error(409, &format!("{path} already exists")));
        }
    }

    // The converted project must be admitted before it is offered.
    let provider = MemoryProvider::new()
        .with(entry_path.clone(), entry_text.as_bytes().to_vec())
        .with(part_path.clone(), part_text.as_bytes().to_vec());
    let unit = intelligence::unit_of(&entry_path, &entry_text);
    let report = engine03.validate(&unit, &provider, &Inputs::new());
    let admitted = report
        .project
        .as_ref()
        .is_some_and(|p| p.admission == "admitted");
    if !admitted {
        let reasons: Vec<String> = report
            .diagnostics
            .iter()
            .map(|d| format!("{} in {} ({})", d.id, d.source, d.meaning))
            .collect();
        return Err(Response::error(
            422,
            &format!(
                "the converted project would not be admitted, so nothing is offered: {}",
                if reasons.is_empty() {
                    "no project record".to_string()
                } else {
                    reasons.join("; ")
                }
            ),
        ));
    }
    let files = vec![(entry_path, entry_text), (part_path, part_text)];
    let mut bytes = Vec::new();
    for (path, text) in &files {
        for part in [path.as_str(), text.as_str()] {
            bytes.extend_from_slice(part.len().to_string().as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(part.as_bytes());
        }
    }
    Ok((id.to_string(), files, lcl_spec::sha256::hex_digest(&bytes)))
}

fn conversion_json(original: &str, files: &[(String, String)], digest: &str) -> String {
    Object::new()
        .with("original", Node::string(original))
        .with("entry", Node::string(&files[0].0))
        .with("plan_digest", Node::string(digest))
        .with(
            "files",
            Node::array(files.iter().map(|(path, text)| {
                Object::new()
                    .with("path", Node::string(path))
                    .with("text", Node::string(text))
                    .into()
            })),
        )
        .pretty()
}

/// `GET /api/convert/plan?id=&folder=`: the exact files a conversion would
/// create, written nowhere.
pub fn preview_conversion(workspace: &Workspace, request: &Request) -> Response {
    match converted(workspace, request) {
        Ok((original, files, digest)) => {
            Response::json(conversion_json(&original, &files, &digest))
        }
        Err(refusal) => refusal,
    }
}

/// `POST /api/convert?id=&folder=&plan_digest=`: create exactly the previewed
/// files, all or none. The original document is left exactly as it is.
pub fn convert(workspace: &Workspace, request: &Request) -> Response {
    let (original, files, digest) = match converted(workspace, request) {
        Ok(planned) => planned,
        Err(refusal) => return refusal,
    };
    if request.param("plan_digest") != Some(digest.as_str()) {
        return Response::error(
            409,
            "the conversion changed since it was previewed (or was not previewed); nothing was created",
        );
    }
    let mut made: Vec<(String, String)> = Vec::new();
    for (path, text) in &files {
        match workspace.create_document(path, text) {
            Ok(document) => made.push((path.clone(), document.digest)),
            Err(error) => {
                for (path, digest) in made.iter().rev() {
                    let _ = workspace.delete(path, digest);
                }
                return Response::error(409, &format!("{path}: {error}; nothing was created"));
            }
        }
    }
    Response::json(conversion_json(&original, &files, &digest))
}
