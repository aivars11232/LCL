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

use crate::config::Config;
use crate::paths::Paths;
use lcl_workspace::{Routes, Workspace};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
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
}

impl Projects {
    pub fn new(specs: Specs, paths: &Paths) -> Projects {
        Projects {
            specs,
            settings_file: paths.workspace_settings.clone(),
            open: Mutex::new(BTreeMap::new()),
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

    /// The routes for one project, opening it the first time.
    pub fn routes(&self, project: &Project) -> Result<Arc<Routes>, String> {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(routes) = open.get(&project.id) {
            return Ok(Arc::clone(routes));
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
        Ok(routes)
    }
}

/// The stable id of a project root.
pub fn project_id(root: &Path) -> String {
    lcl_spec::sha256::hex_digest(root.display().to_string().as_bytes())[..16].to_string()
}
