//! The projects a paired device may open, and one engine for each.
//!
//! A device never names a folder. It names a project by an opaque id this
//! module gave it, and the only projects there are is the default workspace
//! (the one the desktop workspace opens: the folder chosen in its Settings, or
//! the built-in one) and the folders shared explicitly with
//! `lcl-remote projects add`. Inside a project, every path goes through the
//! workspace's own resolution, which refuses anything outside it.
//!
//! Each project is opened once, as an [`lcl_workspace::Routes`] over an
//! [`lcl_workspace::Workspace`]: the same object the desktop workspace serves
//! its browser page from. A remote request is one of those routes, called in
//! process.
//!
//! What is shared is decided afresh on every access, from `remote.json` as it
//! is on disk at that moment, so `lcl-remote projects add` and `remove` take
//! effect for a running service from its next request, with no restart. A
//! project that is no longer shared cannot be opened, and routes kept open for
//! it are closed.

use crate::config::Config;
use crate::paths::Paths;
use lcl_workspace::{Routes, Workspace};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// One shared project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// 16 hex digits, from the canonical root. Stable across restarts.
    pub id: String,
    /// The folder's own name.
    pub name: String,
    pub root: PathBuf,
    /// Whether this is the desktop workspace's default folder.
    pub default: bool,
}

/// The specification packages every project is judged against.
#[derive(Debug, Clone)]
pub struct Specs {
    pub core: PathBuf,
    pub localized: Option<PathBuf>,
}

impl Specs {
    /// `explicit`, then `LCL_SPEC`, then the installed package — the same
    /// order the desktop launcher's baked paths give the workspace.
    pub fn locate(
        paths: &Paths,
        explicit: Option<PathBuf>,
        explicit_localized: Option<PathBuf>,
    ) -> Result<Specs, String> {
        let from_env = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let installed = |version: &str| {
            let path = paths.data.join(format!("LCL_Core_{version}"));
            path.is_dir().then_some(path)
        };
        let core = explicit
            .or_else(|| from_env("LCL_SPEC"))
            .or_else(|| installed("0.1.0"))
            .ok_or("no specification package: pass --spec, set LCL_SPEC, or install LCL")?;
        let localized = explicit_localized
            .or_else(|| from_env("LCL_LOCALIZED_SPEC"))
            .or_else(|| installed("0.2.0"));
        Ok(Specs { core, localized })
    }
}

/// Open projects, by id.
pub struct Projects {
    specs: Specs,
    settings_file: PathBuf,
    open: Mutex<BTreeMap<String, Arc<Routes>>>,
    /// How many times routes have been closed, so a session can notice at
    /// once that routes it holds may be among them.
    closings: AtomicU64,
}

impl Projects {
    pub fn new(specs: Specs, paths: &Paths) -> Projects {
        Projects {
            specs,
            settings_file: paths.workspace_settings.clone(),
            open: Mutex::new(BTreeMap::new()),
            closings: AtomicU64::new(0),
        }
    }

    pub fn specs(&self) -> &Specs {
        &self.specs
    }

    /// Every shared project that exists now.
    pub fn list(&self, paths: &Paths, config: &Config) -> Vec<Project> {
        let settings = lcl_workspace::settings::load(&paths.workspace_settings).settings;
        let default = settings
            .default_workspace
            .filter(|p| p.is_dir())
            .unwrap_or_else(|| paths.builtin_workspace.clone());
        let mut out: Vec<Project> = Vec::new();
        for (root, is_default) in std::iter::once((default, true))
            .chain(config.projects.iter().map(|p| (p.clone(), false)))
        {
            let Ok(root) = root.canonicalize() else {
                continue;
            };
            if !root.is_dir() || out.iter().any(|p| p.root == root) {
                continue;
            }
            out.push(Project {
                id: project_id(&root),
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| root.display().to_string()),
                root,
                default: is_default,
            });
        }
        out
    }

    /// The projects shared at this moment, and the routes of any project that
    /// is no longer shared, closed here and handed back so the runs in them
    /// can be stopped.
    pub fn current(&self, paths: &Paths) -> (Vec<Project>, Vec<Arc<Routes>>) {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let projects = self.list(paths, &shared_now(paths));
        let mut closed = Vec::new();
        open.retain(|id, routes| {
            let shared = projects.iter().any(|p| &p.id == id);
            if !shared {
                closed.push(Arc::clone(routes));
            }
            shared
        });
        if !closed.is_empty() {
            self.closings.fetch_add(1, Ordering::SeqCst);
        }
        (projects, closed)
    }

    /// How many times [`Projects::current`] has closed routes so far.
    pub fn closings(&self) -> u64 {
        self.closings.load(Ordering::SeqCst)
    }

    /// Whether `routes` are the ones open for project `id` now: false once
    /// they were closed, even if the project has been shared again since.
    pub fn is_open(&self, id: &str, routes: &Arc<Routes>) -> bool {
        let open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        open.get(id)
            .is_some_and(|current| Arc::ptr_eq(current, routes))
    }

    /// One project shared at this moment, by id, and its routes, opened the
    /// first time. `None` when no project with that id is shared now.
    ///
    /// Deciding that the project is shared and handing out its routes happen
    /// under one lock, the one [`Projects::current`] closes routes under, so a
    /// project is never opened again once its removal has been seen.
    pub fn open(&self, paths: &Paths, id: &str) -> Result<Option<(Project, Arc<Routes>)>, String> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let Some(project) = self
            .list(paths, &shared_now(paths))
            .into_iter()
            .find(|p| p.id == id)
        else {
            return Ok(None);
        };
        if let Some(routes) = open.get(&project.id) {
            return Ok(Some((project, Arc::clone(routes))));
        }
        let workspace = Workspace::open_with(
            &project.root,
            &self.specs.core,
            self.specs.localized.clone(),
        )
        .map_err(|e| e.to_string())?;
        let routes = Arc::new(
            Routes::new(Arc::new(workspace)).with_settings_file(Some(self.settings_file.clone())),
        );
        open.insert(project.id.clone(), Arc::clone(&routes));
        Ok(Some((project, routes)))
    }
}

/// `remote.json` as it is on disk now. One that cannot be read shares no
/// folder beyond the default workspace: a running service does not guess
/// which folders were meant.
fn shared_now(paths: &Paths) -> Config {
    Config::load(paths).unwrap_or_default()
}

/// The stable id of a project root.
pub fn project_id(root: &Path) -> String {
    lcl_spec::sha256::hex_digest(root.display().to_string().as_bytes())[..16].to_string()
}
