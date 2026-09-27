//! Master templates on disk, and creating files and projects from scaffolds.
//!
//! The engine decides what a scaffold is and whether a Master is valid
//! ([`lcl_protocol::scaffold`]). This module keeps a person's Masters, chooses
//! which one a new file starts from, and writes a previewed project.
//!
//! ## Where
//!
//! `$XDG_CONFIG_HOME/lcl/masters/`, or `$HOME/.config/lcl/masters/`, beside
//! the workspace settings file: one `<id>.json` per Master, in the format
//! [`lcl_protocol::scaffold::parse_master`] reads, and `defaults.json` naming
//! the default Master of each role:
//!
//! ```text
//! {"format": 1, "defaults": {"kind.part.task": "my-task"}}
//! ```
//!
//! ## Which text a new file gets
//!
//! An explicitly selected Master first, then the default Master for the role,
//! then the canonical scaffold. A selected or default Master that is missing
//! or invalid is an error, never a silent fall back to the canonical scaffold.
//!
//! ## Copies, never links
//!
//! A file made from a Master receives the Master's text when it is created and
//! nothing else: no file records which Master it came from. Editing or
//! deleting a Master changes only what later creations get, and editing a
//! created file never touches a Master.

use crate::document;
use crate::settings;
use lcl_protocol::json::{Node, Object};
use lcl_protocol::scaffold::{
    self, Content, LocaleTag, Master, Mode, Plan, Scaffold, ScaffoldError, PROJECT_KIND,
};
use lcl_protocol::Engine;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file naming the default Master of each role.
pub const DEFAULTS_FILE: &str = "defaults.json";

/// The `defaults.json` format this build reads and writes.
pub const DEFAULTS_FORMAT: u64 = 1;

/// Where Masters live for this process's environment.
pub fn location() -> Option<PathBuf> {
    settings::location().and_then(|file| file.parent().map(|dir| dir.join("masters")))
}

/// Where a new file's text comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection<'a> {
    /// This Master, whatever the defaults say.
    Master(&'a str),
    /// The canonical scaffold in this mode, whatever the defaults say.
    Canonical(Mode),
    /// The role's default Master, or the canonical scaffold in this mode when
    /// the role has none.
    Automatic(Mode),
}

/// Where a planned file's text came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Canonical(Mode),
    Master(String),
}

/// One file to be created, exactly as it will be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// Relative to the project folder, `/`-separated.
    pub path: String,
    pub role: String,
    pub origin: Origin,
    pub scaffold: Scaffold,
}

/// A person's Masters, in one directory.
#[derive(Debug, Clone)]
pub struct Masters {
    dir: PathBuf,
}

impl Masters {
    pub fn new(dir: impl Into<PathBuf>) -> Masters {
        Masters { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, id: &str) -> Result<PathBuf, String> {
        scaffold::check_id(id).map_err(|e| e.0)?;
        Ok(self.dir.join(format!("{id}.json")))
    }

    /// The ids of every stored Master, in ascending order. A missing directory
    /// holds none.
    pub fn ids(&self) -> Result<Vec<String>, String> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{} could not be read: {e}", self.dir.display())),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("{} could not be read: {e}", self.dir.display()))?;
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if id != "defaults" && scaffold::check_id(id).is_ok() {
                ids.push(id.to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Read one Master. Its file name must match the id it declares.
    pub fn read(&self, id: &str) -> Result<Master, String> {
        let path = self.path(id)?;
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("Master {id} could not be read from {}: {e}", path.display()))?;
        let master = scaffold::parse_master(&text).map_err(|e| format!("Master {id}: {e}"))?;
        if master.id != id {
            return Err(format!("{} declares the id {}", path.display(), master.id));
        }
        Ok(master)
    }

    fn lookup(&self) -> impl Fn(&str) -> Result<Master, ScaffoldError> + '_ {
        move |id| self.read(id).map_err(ScaffoldError)
    }

    /// Check a Master against `engine`, with the Masters it names read from here.
    pub fn check(&self, engine: &Engine, master: &Master) -> Result<(), String> {
        scaffold::check_master(engine, master, &self.lookup())
            .map(|_| ())
            .map_err(|e| e.0)
    }

    /// Store a new Master, or replace the one with the same id. An invalid
    /// Master is refused and nothing is written.
    pub fn save(&self, engine: &Engine, json_text: &str) -> Result<Master, String> {
        let master = scaffold::parse_master(json_text).map_err(|e| e.0)?;
        self.check(engine, &master)?;
        settings::write_atomically(&self.path(&master.id)?, json_text.as_bytes())?;
        Ok(master)
    }

    /// Delete a Master and any default naming it. Files already made from it
    /// are not touched.
    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path(id)?;
        std::fs::remove_file(&path)
            .map_err(|e| format!("Master {id} could not be deleted: {e}"))?;
        let mut defaults = self.defaults()?;
        let before = defaults.len();
        defaults.retain(|_, master| master != id);
        if defaults.len() != before {
            self.store_defaults(&defaults)?;
        }
        Ok(())
    }

    /// The default Master of each role. A missing file names none.
    pub fn defaults(&self) -> Result<BTreeMap<String, String>, String> {
        let path = self.dir.join(DEFAULTS_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(format!("{} could not be read: {e}", path.display())),
        };
        let invalid = |detail: &str| format!("{} is invalid: {detail}", path.display());
        let json = lcl_spec::json::parse(&text).map_err(|e| invalid(&e.to_string()))?;
        let keys_ok = json.as_object().is_some_and(|members| {
            members
                .iter()
                .all(|(k, _)| k == "format" || k == "defaults")
        });
        if !keys_ok || json.get("format").and_then(|f| f.as_u64()) != Some(DEFAULTS_FORMAT) {
            return Err(invalid(&format!(
                "it must be {{\"format\": {DEFAULTS_FORMAT}, \"defaults\": {{...}}}}"
            )));
        }
        let mut defaults = BTreeMap::new();
        for (role, id) in json
            .get("defaults")
            .and_then(|d| d.as_object())
            .ok_or_else(|| invalid("\"defaults\" must be an object"))?
        {
            let id = id
                .as_str()
                .ok_or_else(|| invalid("each default is a Master id"))?;
            defaults.insert(role.clone(), id.to_string());
        }
        Ok(defaults)
    }

    fn store_defaults(&self, defaults: &BTreeMap<String, String>) -> Result<(), String> {
        let mut members = Object::new();
        for (role, id) in defaults {
            members.set(role.clone(), Node::string(id));
        }
        let json = Object::new()
            .with("format", Node::u64(DEFAULTS_FORMAT))
            .with("defaults", Node::from(members))
            .pretty();
        settings::write_atomically(&self.dir.join(DEFAULTS_FILE), json.as_bytes())
    }

    /// Make `id` the default Master of `role`, or with `None` clear the role's
    /// default. A Master becomes a default only when it is valid now and is for
    /// exactly that role.
    pub fn set_default(&self, engine: &Engine, role: &str, id: Option<&str>) -> Result<(), String> {
        let mut defaults = self.defaults()?;
        match id {
            Some(id) => {
                let master = self.read(id)?;
                if master.role != role {
                    return Err(format!("Master {id} is for {}, not {role}", master.role));
                }
                self.check(engine, &master)?;
                defaults.insert(role.to_string(), id.to_string());
            }
            None => {
                defaults.remove(role);
            }
        }
        self.store_defaults(&defaults)
    }

    /// The Master `selection` names for `role`, or `None` for the canonical
    /// scaffold in the returned mode.
    fn choose(&self, role: &str, selection: Selection) -> Result<Result<Master, Mode>, String> {
        let id = match selection {
            Selection::Master(id) => id.to_string(),
            Selection::Canonical(mode) => return Ok(Err(mode)),
            Selection::Automatic(mode) => match self.defaults()?.remove(role) {
                Some(id) => id,
                None => return Ok(Err(mode)),
            },
        };
        let master = self.read(&id)?;
        if master.role != role {
            return Err(format!("Master {id} is for {}, not {role}", master.role));
        }
        Ok(Ok(master))
    }

    /// The exact text a new file of `role` at `path` starts with.
    pub fn file(
        &self,
        engine: &Engine,
        role: &str,
        selection: Selection,
        path: &str,
        locale: Option<&LocaleTag>,
    ) -> Result<Planned, String> {
        let (origin, scaffold) = match self.choose(role, selection)? {
            Ok(master) => {
                let marks =
                    scaffold::check_master(engine, &master, &self.lookup()).map_err(|e| e.0)?;
                let Content::Text(text) = master.content else {
                    return Err(format!("Master {} is a project Master", master.id));
                };
                (Origin::Master(master.id), Scaffold { text, marks })
            }
            Err(mode) => (
                Origin::Canonical(mode),
                scaffold::part(engine, role, mode, path, locale).map_err(|e| e.0)?,
            ),
        };
        Ok(Planned {
            path: path.to_string(),
            role: role.to_string(),
            origin,
            scaffold,
        })
    }

    /// Every file a new project starts with, in plan order and exactly as it
    /// will be written: the entry first, then each part, whose text comes from
    /// the Master the plan names or else by the usual priority.
    pub fn project(
        &self,
        engine: &Engine,
        selection: Selection,
        locale: Option<&LocaleTag>,
    ) -> Result<Vec<Planned>, String> {
        let plan = match self.choose(PROJECT_KIND, selection)? {
            Ok(master) => {
                self.check(engine, &master)?;
                match master.content {
                    Content::Project(plan) => plan,
                    Content::Text(_) => {
                        return Err(format!("Master {} is not a project Master", master.id))
                    }
                }
            }
            Err(mode) => Plan::canonical(mode),
        };
        let entry = scaffold::entry(engine, &plan, locale).map_err(|e| e.0)?;
        let mut files = vec![Planned {
            path: plan.entry.clone(),
            role: PROJECT_KIND.to_string(),
            origin: Origin::Canonical(plan.mode),
            scaffold: entry,
        }];
        for part in &plan.parts {
            let selection = match &part.master {
                Some(id) => Selection::Master(id),
                None => Selection::Automatic(plan.mode),
            };
            files.push(self.file(engine, &part.role, selection, &part.path, locale)?);
        }
        Ok(files)
    }
}

/// Create every planned file under `root`, or none of them. Every path is
/// checked first, an existing file is never replaced, and a failure removes
/// the files and folders this call made. `root` and missing folders on the
/// way to a file are created.
pub fn create(root: &Path, files: &[Planned]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for file in files {
        scaffold::check_path(&file.path).map_err(|e| e.0)?;
        if !seen.insert(file.path.as_str()) {
            return Err(format!(
                "{} is planned twice; nothing was created",
                file.path
            ));
        }
        if std::fs::symlink_metadata(root.join(&file.path)).is_ok() {
            return Err(format!("{} already exists; nothing was created", file.path));
        }
    }
    let mut folders: Vec<PathBuf> = Vec::new();
    let mut made: Vec<(String, String)> = Vec::new();
    let result = (|| -> Result<(), String> {
        make_folders(root, &mut folders)?;
        for file in files {
            if let Some(parent) = Path::new(&file.path).parent() {
                make_folders(&root.join(parent), &mut folders)?;
            }
            let document = document::create(root, &file.path, &file.scaffold.text)
                .map_err(|e| format!("{}: {e}", file.path))?;
            made.push((file.path.clone(), document.digest));
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (path, digest) in made.iter().rev() {
            let _ = document::delete(root, path, digest);
        }
        for folder in folders.iter().rev() {
            let _ = std::fs::remove_dir(folder);
        }
        return Err(format!("{error}; nothing was created"));
    }
    Ok(())
}

/// Create `folder` and any missing folder above it, recording each one made.
fn make_folders(folder: &Path, made: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut missing = Vec::new();
    let mut current = Some(folder);
    while let Some(path) = current {
        if path.as_os_str().is_empty() || std::fs::symlink_metadata(path).is_ok() {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    for path in missing.into_iter().rev() {
        std::fs::create_dir(&path)
            .map_err(|e| format!("{} could not be created: {e}", path.display()))?;
        made.push(path);
    }
    Ok(())
}
