//! The Projects home and the window's active project: the Projects folder, the projects
//! kept on the home, New Project, and opening a folder as the project.

use super::*;

impl Routes {
    /// Where New Project creates projects and the Projects home looks: the
    /// Projects folder chosen in Settings, which must be a folder that exists
    /// and can be written — a chosen folder is never silently replaced by
    /// another — or, when none is chosen, the launcher's built-in default
    /// workspace, or the folder this window was launched with when no
    /// launcher named one. A settings file that cannot be read does not say
    /// which folder was chosen, so it is an error here too.
    pub(super) fn projects_folder(&self, _workspace: &Workspace) -> Result<PathBuf, String> {
        let loaded = self.settings();
        if let Some(problem) = loaded.problem {
            return Err(format!(
                "{problem} New Project creates projects in the Projects folder those settings \
                 name, so it waits until they are saved again in Settings."
            ));
        }
        match loaded.settings.default_workspace {
            Some(chosen) => {
                if !chosen.is_dir() {
                    return Err(format!(
                        "The Projects folder {} does not exist or is not a folder. Choose \
                         another one in Settings, or create it there; nothing was created.",
                        chosen.display()
                    ));
                }
                settings::writable(&chosen).map_err(|e| {
                    format!(
                        "The Projects folder {} cannot be written to ({e}). Choose another \
                         one in Settings; nothing was created.",
                        chosen.display()
                    )
                })?;
                Ok(chosen)
            }
            None => Ok(self
                .builtin_default
                .clone()
                .unwrap_or_else(|| self.launched.clone())),
        }
    }

    /// `GET /api/project/plan?name=`: New Project's exact files, where they
    /// will go, and whether that folder is already taken. Written nowhere.
    pub(super) fn preview_named_project(
        &self,
        workspace: &Workspace,
        request: &Request,
    ) -> Response {
        let projects = match self.projects_folder(workspace) {
            Ok(projects) => projects,
            Err(detail) => return Response::error(409, &detail),
        };
        match authoring::named_project(
            workspace,
            &self.masters(),
            request,
            &projects,
            self.ending(),
        ) {
            Ok(plan) => Response::json(authoring::named_plan_json(&plan, &projects)),
            Err(refusal) => refusal,
        }
    }

    /// `POST /api/project?name=&plan_digest=`: create the previewed project in
    /// its own new folder, all files or none. A folder of that name that
    /// already exists is never written into. The new project then becomes
    /// this window's active project: its explorer shows the project's own
    /// files, and nothing beside it in the Projects folder.
    pub(super) fn create_named_project(
        &self,
        workspace: &Workspace,
        request: &Request,
    ) -> Response {
        let projects = match self.projects_folder(workspace) {
            Ok(projects) => projects,
            Err(detail) => return Response::error(409, &detail),
        };
        let plan = match authoring::named_project(
            workspace,
            &self.masters(),
            request,
            &projects,
            self.ending(),
        ) {
            Ok(plan) => plan,
            Err(refusal) => return refusal,
        };
        if let Err(refusal) = authoring::previewed(request, &plan.digest) {
            return refusal;
        }
        if std::fs::symlink_metadata(&plan.dir).is_ok() {
            return Response::error(
                409,
                &format!(
                    "a project named {} already exists at {}; nothing was created or changed",
                    plan.name,
                    plan.dir.display()
                ),
            );
        }
        if self.reopen.is_none() {
            return Response::error(
                409,
                "this window cannot open another folder, so it cannot show a new project; \
                 nothing was created",
            );
        }
        if let Err(detail) = masters::create(&plan.dir, &plan.files) {
            return Response::error(409, &detail);
        }
        if let Err(detail) = self.open_folder(&plan.dir, false) {
            return Response::error(
                500,
                &format!(
                    "The project was created in {}, but this window could not open it: {detail}",
                    plan.dir.display()
                ),
            );
        }
        // Made by LCL, so known to the Projects home from now on.
        if let Err(detail) = self.keep_project(&plan.dir, true) {
            return Response::error(
                500,
                &format!(
                    "The project was created in {} and opened, but could not be kept on the \
                     Projects home: {detail}",
                    plan.dir.display()
                ),
            );
        }
        Response::json(authoring::named_plan_json(&plan, &projects))
    }

    /// Keep `folder` on the Projects home, or drop it from there: the
    /// settings file's list of explicitly known projects, by canonical path.
    /// Nothing on disk but that file changes.
    pub(super) fn keep_project(&self, folder: &Path, keep: bool) -> Result<(), String> {
        let Some(file) = &self.settings_file else {
            return Err(
                "this computer keeps no settings file (no HOME), so the Projects \
                        home cannot keep projects"
                    .to_string(),
            );
        };
        let loaded = self.settings();
        if let Some(problem) = loaded.problem {
            return Err(format!("{problem} Save Settings again first."));
        }
        let mut next = loaded.settings;
        if next.keep_project(&normal(folder), keep) {
            settings::store(file, &next)?;
        }
        Ok(())
    }

    /// `POST /api/projects/forget?path=`: drop a project from the Projects
    /// home. Only its registration goes; the folder and every file in it
    /// stay exactly as they are.
    pub(super) fn forget_project(&self, request: &Request) -> Response {
        let Some(raw) = request.param("path").filter(|p| !p.is_empty()) else {
            return Response::error(400, "a folder path is required");
        };
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Response::error(400, &format!("{raw} is not an absolute path"));
        }
        match self.keep_project(&path, false) {
            Ok(()) => Response::json(
                Object::new()
                    .with("path", Node::string(raw))
                    .with("registered", Node::Bool(false))
                    .pretty(),
            ),
            Err(detail) => Response::error(409, &detail),
        }
    }

    /// `GET /api/projects`: the Projects home's list, by name — the folders
    /// with an explicit reason to be LCL projects, and no other: a direct
    /// subfolder of the Projects folder that declares itself with
    /// `lcl.project.json`, and every project the settings file keeps (made by
    /// New Project, or kept on request when opened), wherever it is. A folder
    /// is never a project merely for being in the Projects folder, or for
    /// holding an LCL document somewhere: the person's own folders there are
    /// not offered, though Open project folder… can still open any. A shallow
    /// look, on purpose: the Projects folder's entries and each candidate's
    /// manifest file, nothing below. `manifest` says which declare themselves,
    /// `registered` which are kept and so can be dropped from the home.
    pub(super) fn list_projects(&self, workspace: &Workspace) -> Response {
        let projects = match self.projects_folder(workspace) {
            Ok(projects) => projects,
            Err(detail) => return Response::error(409, &detail),
        };
        let listing = match std::fs::read_dir(&projects) {
            Ok(listing) => listing,
            Err(e) => {
                return Response::error(
                    409,
                    &format!(
                        "the Projects folder {} is not readable: {e}",
                        projects.display()
                    ),
                )
            }
        };
        // Kept projects first, so a manifest project that is also kept is
        // listed once, as kept.
        let mut found: Vec<(String, PathBuf, bool)> = self
            .settings()
            .settings
            .projects
            .into_iter()
            .filter(|p| p.is_dir())
            .filter_map(|p| Some((p.file_name()?.to_string_lossy().to_string(), p, true)))
            .collect();
        for path in listing.filter_map(Result::ok).map(|e| e.path()) {
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            if name.starts_with('.')
                || !path.is_dir()
                || !path.join(lcl_project::MANIFEST_FILE).is_file()
            {
                continue;
            }
            let path = normal(&path);
            if !found.iter().any(|(_, kept, _)| *kept == path) {
                found.push((name, path, false));
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let current = if self.is_home() {
            None
        } else {
            Some(normal(workspace.root()))
        };
        Response::json(
            Object::new()
                .with("folder", Node::string(projects.display().to_string()))
                .with("home", Node::Bool(self.is_home()))
                .with(
                    "projects",
                    Node::array(found.into_iter().map(|(name, path, registered)| {
                        let here = current.as_deref() == Some(path.as_path());
                        let manifest = path.join(lcl_project::MANIFEST_FILE).is_file();
                        Object::new()
                            .with("name", Node::string(&name))
                            .with("path", Node::string(path.display().to_string()))
                            .with("manifest", Node::Bool(manifest))
                            .with("registered", Node::Bool(registered))
                            .with("current", Node::Bool(here))
                            .into()
                    })),
                )
                .pretty(),
        )
    }

    /// `POST /api/projects/open`: show the Projects home in this window. The
    /// Projects folder becomes this window's folder only as the place New
    /// Project creates in; its contents are offered as projects, never
    /// listed as one tree.
    pub(super) fn open_projects_folder(&self) -> Response {
        let workspace = self.workspace();
        let projects = match self.projects_folder(&workspace) {
            Ok(projects) => projects,
            Err(detail) => return Response::error(409, &detail),
        };
        if self.reopen.is_none() {
            return Response::error(409, "this window cannot open another folder");
        }
        match self.open_folder(&projects, true) {
            Ok(()) => self.session(&self.workspace()),
            Err(detail) => Response::error(500, &detail),
        }
    }

    /// `POST /api/project/open?path=&keep=`: make the folder at an absolute
    /// path this window's active project. A project from the Projects home,
    /// or any folder the person names explicitly: one without a manifest
    /// opens as a rootless project, and with `keep=1` it is kept on the
    /// Projects home from then on — only when asked, so a folder opened once
    /// is not registered behind the person's back.
    pub(super) fn open_project(&self, request: &Request) -> Response {
        let keep = matches!(request.param("keep"), Some("1" | "true"));
        let Some(raw) = request.param("path").filter(|p| !p.is_empty()) else {
            return Response::error(400, "a folder path is required");
        };
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Response::error(
                400,
                &format!("{raw} is not an absolute path; write it from /"),
            );
        }
        if !path.is_dir() {
            return Response::error(404, &format!("{raw} is not a folder"));
        }
        if self.reopen.is_none() {
            return Response::error(409, "this window cannot open another folder");
        }
        if let Err(detail) = self.open_folder(&path, false) {
            return Response::error(422, &detail);
        }
        if keep {
            if let Err(detail) = self.keep_project(&path, true) {
                return Response::error(
                    500,
                    &format!(
                        "{raw} was opened, but could not be kept on the Projects home: {detail}"
                    ),
                );
            }
        }
        self.session(&self.workspace())
    }

    /// Show `folder` in this window from now on, opened as this launch opened
    /// its own: as the active project, or, with `home`, as the Projects
    /// folder behind the Projects home. Runs already started keep the
    /// workspace they started in.
    pub(super) fn open_folder(&self, folder: &Path, home: bool) -> Result<(), String> {
        let Some(reopen) = &self.reopen else {
            return Err("this window cannot open another folder".to_string());
        };
        let next = Arc::new(reopen(folder)?);
        *self.workspace.write().unwrap_or_else(|e| e.into_inner()) = next;
        *self.home.write().unwrap_or_else(|e| e.into_inner()) = home;
        *self.notice.write().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
}

/// One spelling of a folder's path, for keeping and comparing: canonical when
/// the folder can be resolved, as given otherwise.
fn normal(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
