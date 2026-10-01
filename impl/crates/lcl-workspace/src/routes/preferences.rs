//! The workspace's own settings, and the one-folder check Settings uses to choose a folder.

use super::*;

impl Routes {
    /// The workspace settings, as the Settings dialog shows them.
    pub(super) fn read_settings(&self) -> Response {
        Response::json(self.settings_json(&self.settings()))
    }

    pub(super) fn settings_json(&self, loaded: &settings::Loaded) -> String {
        let chosen = &loaded.settings.default_workspace;
        Object::new()
            .with("available", Node::Bool(self.settings_file.is_some()))
            .with(
                "file",
                Node::optional(self.settings_file.as_ref().map(|f| f.display().to_string())),
            )
            .with("problem", Node::optional(loaded.problem.clone()))
            .with("version", Node::u64(settings::VERSION))
            .with(
                "default_extension",
                Node::string(loaded.settings.default_extension),
            )
            .with(
                "default_workspace",
                Node::optional(chosen.as_ref().map(|p| p.display().to_string())),
            )
            // Whether the chosen folder is usable right now: a launch would
            // otherwise open the built-in default instead.
            .with(
                "default_workspace_exists",
                match chosen {
                    Some(path) => Node::Bool(path.is_dir()),
                    None => Node::Null,
                },
            )
            .with(
                "builtin_default_workspace",
                Node::optional(
                    self.builtin_default
                        .as_ref()
                        .map(|p| p.display().to_string()),
                ),
            )
            .with(
                "current_workspace",
                Node::string(self.workspace().root().display().to_string()),
            )
            .pretty()
    }

    /// Save the workspace settings from a JSON object.
    ///
    /// Either key may be left out to keep its current value. A default file
    /// type must be `.lcl` or `.lcl.txt`. A default workspace must be an
    /// absolute path to a folder that exists, or null for the built-in one;
    /// creating a missing folder is a separate, explicit request.
    pub(super) fn save_settings(&self, request: &Request) -> Response {
        let Some(file) = &self.settings_file else {
            return Response::error(
                409,
                "this workspace has no settings file: set HOME or XDG_CONFIG_HOME",
            );
        };
        let text = match request.text() {
            Ok(text) => text,
            Err(e) => return Response::error(400, &e.to_string()),
        };
        let body = match lcl_spec::json::parse(text) {
            Ok(body) if body.as_object().is_some() => body,
            _ => return Response::error(400, "the settings must be one JSON object"),
        };
        let mut next = self.settings().settings;
        match body.get("default_extension") {
            None => {}
            Some(value) => match value.as_str().and_then(settings::ending) {
                Some(suffix) => next.default_extension = suffix,
                None => {
                    return Response::error(422, "the default file type must be .lcl or .lcl.txt")
                }
            },
        }
        match body.get("default_workspace") {
            None => {}
            Some(lcl_spec::json::Json::Null) => next.default_workspace = None,
            Some(value) => match value.as_str() {
                Some("") => next.default_workspace = None,
                Some(raw) => {
                    let path = PathBuf::from(raw);
                    if !path.is_absolute() {
                        return Response::error(
                            422,
                            &format!("{raw} is not an absolute path; write it from /"),
                        );
                    }
                    if !path.exists() {
                        return Response::error(422, &format!("{raw} does not exist"));
                    }
                    if !path.is_dir() {
                        return Response::error(422, &format!("{raw} is not a folder"));
                    }
                    next.default_workspace = Some(path);
                }
                None => {
                    return Response::error(422, "the default workspace must be a path or null")
                }
            },
        }
        if let Err(detail) = settings::store(file, &next) {
            return Response::error(500, &detail);
        }
        Response::json(self.settings_json(&settings::Loaded {
            settings: next,
            problem: None,
        }))
    }

    /// Check one folder path, or create it.
    ///
    /// For choosing a default workspace, and nothing more: it answers about the
    /// one absolute path it is given — whether it exists and is a folder — and
    /// never lists what is in it, so it is not a way to browse the computer.
    /// Creating happens only on `POST`, which the page sends after the person
    /// confirmed the path it shows.
    pub(super) fn folder(&self, request: &Request, create: bool) -> Response {
        let Some(raw) = request.param("path") else {
            return Response::error(400, "a path is required");
        };
        let path = PathBuf::from(raw);
        let absolute = path.is_absolute();
        let mut created = false;
        if create {
            if !absolute {
                return Response::error(400, &format!("{raw} is not an absolute path"));
            }
            if path.exists() && !path.is_dir() {
                return Response::error(409, &format!("{raw} exists and is not a folder"));
            }
            if !path.exists() {
                if let Err(e) = std::fs::create_dir_all(&path) {
                    return Response::error(500, &format!("{raw} could not be created: {e}"));
                }
                created = true;
            }
        }
        let directory = absolute && path.is_dir();
        Response::json(
            Object::new()
                .with("path", Node::string(raw))
                .with("absolute", Node::Bool(absolute))
                .with("exists", Node::Bool(absolute && path.exists()))
                .with("directory", Node::Bool(directory))
                // Whether a project can be created in it now, found by doing
                // it: permission bits alone do not say (ACLs, read-only
                // mounts).
                .with(
                    "writable",
                    Node::Bool(directory && settings::writable(&path).is_ok()),
                )
                .with("created", Node::Bool(created))
                .pretty(),
        )
    }
}
