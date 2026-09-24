//! The workspace's own settings, stored once per user.
//!
//! ## Not LCL
//!
//! Two preferences about the product, and nothing about the language: which
//! folder a launch opens when it names no project, and which ending a new
//! document gets when its name has neither `.lcl` nor `.lcl.txt`. Neither can
//! change what a document means or how it runs. They are never written into a
//! document, a project manifest or a lock, and the engine never sees them.
//!
//! The browser keeps presentation (theme, font size, line numbers) in its own
//! storage, because that is about one screen. These are about the computer: a
//! launch from the desktop menu has to know them before any page has loaded,
//! so they live in a file under the user's configuration directory.
//!
//! ## Where
//!
//! `$XDG_CONFIG_HOME/lcl/workspace-settings.json`, or
//! `$HOME/.config/lcl/workspace-settings.json` when `XDG_CONFIG_HOME` is unset.
//! The XDG Base Directory Specification says a relative value is invalid and is
//! to be ignored, so one is. With neither variable there is no file, and the
//! defaults apply.
//!
//! ## Reading forgives, writing does not
//!
//! A missing file is the defaults. An unreadable file, one that is not JSON or
//! one written by another version is the defaults too, and says so. Each field
//! is checked on its own, so one bad value falls back without taking the
//! others with it. Writing goes to a temporary file in the same directory and
//! is then renamed over the old one, so a crash leaves the previous settings
//! whole rather than half a file.

use lcl_protocol::json::{Node, Object};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The schema this build reads and writes.
pub const VERSION: u64 = 1;

/// The file's name inside the `lcl` configuration directory.
pub const FILE_NAME: &str = "workspace-settings.json";

/// The workspace's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The folder a launch that names no project opens. `None` leaves that to
    /// the launcher's built-in default.
    pub default_workspace: Option<PathBuf>,
    /// The ending a new document gets when its name has neither LCL ending:
    /// one of [`lcl_project::SUFFIXES`].
    pub default_extension: &'static str,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            default_workspace: None,
            default_extension: lcl_project::SUFFIX,
        }
    }
}

/// What reading the settings file found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Loaded {
    pub settings: Settings,
    /// Why some or all of the file was not used, when it was not.
    pub problem: Option<String>,
}

/// The recognised ending `value` names, if it names one.
pub fn ending(value: &str) -> Option<&'static str> {
    lcl_project::SUFFIXES
        .into_iter()
        .find(|suffix| *suffix == value)
}

/// Where the settings file lives for this process's environment.
pub fn location() -> Option<PathBuf> {
    location_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Where the settings file lives, given the two variables that decide it.
pub fn location_from(xdg_config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg_config_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            home.map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|home| home.join(".config"))
        })?;
    Some(base.join("lcl").join(FILE_NAME))
}

/// Read the settings file. Never fails: see the module notes.
pub fn load(path: &Path) -> Loaded {
    match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => parse(&text),
            Err(_) => fallback(path, "it is not UTF-8"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Loaded::default(),
        Err(error) => fallback(path, &format!("it could not be read: {error}")),
    }
}

fn fallback(path: &Path, why: &str) -> Loaded {
    Loaded {
        settings: Settings::default(),
        problem: Some(format!(
            "The settings file {} was not used because {why}; the defaults apply.",
            path.display()
        )),
    }
}

/// Read settings from the file's text, field by field.
pub fn parse(text: &str) -> Loaded {
    let document = match lcl_spec::json::parse(text) {
        Ok(document) => document,
        Err(error) => {
            return Loaded {
                settings: Settings::default(),
                problem: Some(format!(
                    "The settings file is not valid JSON ({error}); the defaults apply."
                )),
            }
        }
    };
    if document.as_object().is_none() {
        return Loaded {
            settings: Settings::default(),
            problem: Some("The settings file does not hold an object; the defaults apply.".into()),
        };
    }
    // Another version's fields may mean something else, so none are guessed at.
    if document.get("version").and_then(|v| v.as_u64()) != Some(VERSION) {
        return Loaded {
            settings: Settings::default(),
            problem: Some(format!(
                "The settings file is not version {VERSION}; the defaults apply."
            )),
        };
    }

    let mut settings = Settings::default();
    let mut problems = Vec::new();
    match document.get("default_extension") {
        None | Some(lcl_spec::json::Json::Null) => {}
        Some(value) => match value.as_str().and_then(ending) {
            Some(suffix) => settings.default_extension = suffix,
            None => problems.push("its default file type is not .lcl or .lcl.txt, so .lcl applies"),
        },
    }
    match document.get("default_workspace") {
        None | Some(lcl_spec::json::Json::Null) => {}
        Some(value) => match value.as_str().map(PathBuf::from) {
            Some(path) if path.is_absolute() => settings.default_workspace = Some(path),
            _ => problems.push(
                "its default workspace is not an absolute path, so the built-in default applies",
            ),
        },
    }
    Loaded {
        settings,
        problem: (!problems.is_empty())
            .then(|| format!("In the settings file, {}.", problems.join("; "))),
    }
}

/// The file's text for `settings`.
pub fn to_json(settings: &Settings) -> String {
    Object::new()
        .with("version", Node::u64(VERSION))
        .with(
            "default_workspace",
            match &settings.default_workspace {
                Some(path) => Node::string(path.display().to_string()),
                None => Node::Null,
            },
        )
        .with(
            "default_extension",
            Node::string(settings.default_extension),
        )
        .pretty()
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Write the settings file atomically, creating its directory if needed.
///
/// A default workspace that is not an absolute path is refused here as well as
/// by every caller, so no code path can store one.
pub fn store(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(workspace) = &settings.default_workspace {
        if !workspace.is_absolute() {
            return Err(format!("{} is not an absolute path", workspace.display()));
        }
    }
    if ending(settings.default_extension).is_none() {
        return Err("the default file type must be .lcl or .lcl.txt".to_string());
    }
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    std::fs::create_dir_all(directory)
        .map_err(|e| format!("{} could not be created: {e}", directory.display()))?;
    let temporary = directory.join(format!(
        ".{FILE_NAME}.{}-{}.tmp",
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(to_json(settings).as_bytes())?;
            file.sync_all()
        });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{} could not be written: {error}", path.display()));
    }
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("{} could not be replaced: {error}", path.display())
    })
}

/// Which folder a launch that named no project opens, and anything the person
/// should be told about that choice.
///
/// The chosen default when it is a folder that exists; `fallback`, the
/// launcher's built-in default, otherwise. A chosen folder that has gone is
/// not created again: making an arbitrary path on the person's behalf is not
/// this program's call, so the launch opens the fallback and says why.
pub fn default_project(loaded: &Loaded, fallback: &Path) -> (PathBuf, Option<String>) {
    match &loaded.settings.default_workspace {
        Some(chosen) if chosen.is_dir() => (chosen.clone(), loaded.problem.clone()),
        Some(chosen) => {
            let why = if chosen.exists() {
                "is not a folder"
            } else {
                "does not exist"
            };
            (
                fallback.to_path_buf(),
                Some(format!(
                    "The default workspace {} {why}, so {} was opened instead. \
                     Settings can choose another one.",
                    chosen.display(),
                    fallback.display()
                )),
            )
        }
        None => (fallback.to_path_buf(), loaded.problem.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_location_follows_xdg_and_ignores_a_relative_value() {
        let at = |xdg: Option<&str>, home: Option<&str>| {
            location_from(xdg.map(OsString::from), home.map(OsString::from))
        };
        assert_eq!(
            at(Some("/cfg"), Some("/home/u")),
            Some(PathBuf::from("/cfg/lcl/workspace-settings.json"))
        );
        assert_eq!(
            at(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/lcl/workspace-settings.json"))
        );
        assert_eq!(
            at(Some("relative/cfg"), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/lcl/workspace-settings.json")),
            "a relative XDG_CONFIG_HOME is invalid and ignored"
        );
        assert_eq!(at(None, None), None);
        assert_eq!(at(Some("relative"), Some("also relative")), None);
    }

    #[test]
    fn valid_settings_read_back_exactly() {
        let written = Settings {
            default_workspace: Some(PathBuf::from("/home/u/My LCL $work")),
            default_extension: lcl_project::TEXT_SUFFIX,
        };
        let loaded = parse(&to_json(&written));
        assert_eq!(loaded.settings, written);
        assert_eq!(loaded.problem, None);
    }

    #[test]
    fn a_corrupt_or_foreign_file_falls_back_to_the_defaults_and_says_so() {
        for text in [
            "{not json",
            "[]",
            "\"a string\"",
            "{}",
            "{\"version\": 2, \"default_extension\": \".lcl.txt\"}",
            "{\"version\": \"1\"}",
        ] {
            let loaded = parse(text);
            assert_eq!(loaded.settings, Settings::default(), "{text}");
            assert!(loaded.problem.is_some(), "{text} was accepted silently");
        }
    }

    #[test]
    fn each_bad_field_falls_back_alone() {
        let loaded = parse(
            "{\"version\": 1, \"default_extension\": \".txt\", \"default_workspace\": \"/kept\"}",
        );
        assert_eq!(loaded.settings.default_extension, ".lcl");
        assert_eq!(
            loaded.settings.default_workspace,
            Some(PathBuf::from("/kept"))
        );
        assert!(loaded.problem.is_some());

        let loaded = parse(
            "{\"version\": 1, \"default_extension\": \".lcl.txt\", \"default_workspace\": \"relative/dir\"}",
        );
        assert_eq!(loaded.settings.default_extension, ".lcl.txt");
        assert_eq!(loaded.settings.default_workspace, None);
        assert!(loaded.problem.is_some());

        for other in ["\".LCL\"", "\"lcl\"", "\".lcl.bak\"", "7", "true"] {
            let loaded = parse(&format!(
                "{{\"version\": 1, \"default_extension\": {other}}}"
            ));
            assert_eq!(loaded.settings.default_extension, ".lcl", "{other}");
        }
    }

    #[test]
    fn storing_is_atomic_and_leaves_nothing_else_behind() {
        let directory = std::env::temp_dir().join(format!(
            "lcl-settings-{}-{}",
            std::process::id(),
            NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
        ));
        let file = directory.join("lcl").join(FILE_NAME);
        let first = Settings {
            default_workspace: Some(PathBuf::from("/a")),
            default_extension: lcl_project::SUFFIX,
        };
        store(&file, &first).expect("stored");
        let second = Settings {
            default_workspace: None,
            default_extension: lcl_project::TEXT_SUFFIX,
        };
        store(&file, &second).expect("replaced");
        assert_eq!(load(&file).settings, second);
        let names: Vec<_> = std::fs::read_dir(file.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![OsString::from(FILE_NAME)],
            "a temporary was left behind"
        );
        assert!(store(
            &file,
            &Settings {
                default_workspace: Some(PathBuf::from("relative")),
                ..Settings::default()
            }
        )
        .is_err());
        assert_eq!(
            load(&file).settings,
            second,
            "a refused store changed nothing"
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_missing_default_workspace_is_not_created_and_the_fallback_opens() {
        let loaded = Loaded {
            settings: Settings {
                default_workspace: Some(PathBuf::from("/definitely/not/here/lcl")),
                ..Settings::default()
            },
            problem: None,
        };
        let (root, notice) = default_project(&loaded, Path::new("/fallback"));
        assert_eq!(root, PathBuf::from("/fallback"));
        assert!(notice.unwrap().contains("does not exist"));
        assert!(!Path::new("/definitely/not/here/lcl").exists());

        let (root, notice) = default_project(&Loaded::default(), Path::new("/fallback"));
        assert_eq!(root, PathBuf::from("/fallback"));
        assert_eq!(notice, None);
    }
}
