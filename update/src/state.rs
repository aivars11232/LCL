//! Where the updater keeps things, and what it last found.
//!
//! The installation is the one `install.sh` made: binaries in
//! `${XDG_BIN_HOME:-~/.local/bin}`, packages under
//! `${XDG_DATA_HOME:-~/.local/share}/lcl`. The updater's own state lives in
//! `${XDG_STATE_HOME:-~/.local/state}/lcl/update` and its downloads in
//! `${XDG_CACHE_HOME:-~/.cache}/lcl/update`, never in a repository, a project
//! or the configuration directory. The cache holds at most one staged update
//! and, while one is being installed, one rollback copy.

use crate::json::Object;
use lcl_spec::json::Json;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// How old the last successful check may be before another is due.
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub bin: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    /// The locations `install.sh` uses, from the same environment.
    pub fn from_env() -> Result<Paths, String> {
        Paths::from_vars(|name| std::env::var_os(name))
    }

    /// The locations from `var`. `HOME` must be absolute. An XDG variable
    /// counts only when it names an absolute path: a relative or empty one is
    /// ignored for its default under `HOME`, as the XDG Base Directory
    /// Specification requires, and as `install.sh` reads them too.
    pub fn from_vars(var: impl Fn(&str) -> Option<OsString>) -> Result<Paths, String> {
        let absolute = |name: &str| var(name).map(PathBuf::from).filter(|p| p.is_absolute());
        let home = absolute("HOME").ok_or("HOME is not set to an absolute path")?;
        let base = |name: &str, default: &str| absolute(name).unwrap_or_else(|| home.join(default));
        Ok(Paths {
            bin: base("XDG_BIN_HOME", ".local/bin"),
            data: base("XDG_DATA_HOME", ".local/share"),
            state: base("XDG_STATE_HOME", ".local/state").join("lcl/update"),
            cache: base("XDG_CACHE_HOME", ".cache").join("lcl/update"),
        })
    }

    pub fn state_file(&self) -> PathBuf {
        self.state.join("state.json")
    }
    pub fn manifest(&self) -> PathBuf {
        self.state.join("update-manifest.json")
    }
    pub fn signature(&self) -> PathBuf {
        self.state.join("update-manifest.sig")
    }
    pub fn staging(&self) -> PathBuf {
        self.cache.join("staging")
    }
    pub fn rollback(&self) -> PathBuf {
        self.cache.join("rollback")
    }
    pub fn log(&self) -> PathBuf {
        self.state.join("install.log")
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// What the updater last found, as the state file and `status --json` hold it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct State {
    /// up_to_date, checking, update_available, downloading, ready_to_install,
    /// installing, restart_required, failed, offline, not_configured.
    pub state: String,
    pub checked_at: Option<u64>,
    /// When the source last answered, whatever it said.
    pub last_success_at: Option<u64>,
    pub available: Option<Available>,
    pub progress: Option<(u64, u64)>,
    /// offline, invalid, verification, download, install, unsupported,
    /// not_configured, busy.
    pub error: Option<(String, String)>,
    /// The version installed by the last update, and when.
    pub updated: Option<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub version: String,
    pub tag: String,
    pub published_at: String,
    pub release_notes: String,
    pub size: u64,
}

impl State {
    pub fn check_due(&self, now: u64) -> bool {
        self.last_success_at
            .is_none_or(|at| now.saturating_sub(at) >= CHECK_INTERVAL_SECS)
    }

    pub fn failed(mut self, kind: &str, message: impl Into<String>) -> State {
        self.state = "failed".to_string();
        self.progress = None;
        self.error = Some((kind.to_string(), message.into()));
        self
    }

    pub fn to_json(&self) -> String {
        let opt = |n: Option<u64>| n.map_or("null".to_string(), |n| n.to_string());
        let available = self.available.as_ref().map_or("null".to_string(), |a| {
            Object::new()
                .str("version", &a.version)
                .str("tag", &a.tag)
                .str("published_at", &a.published_at)
                .str("release_notes", &a.release_notes)
                .num("size", a.size)
                .finish()
        });
        let progress = self.progress.map_or("null".to_string(), |(done, total)| {
            Object::new().num("done", done).num("total", total).finish()
        });
        let error = self
            .error
            .as_ref()
            .map_or("null".to_string(), |(kind, message)| {
                Object::new()
                    .str("kind", kind)
                    .str("message", message)
                    .finish()
            });
        let updated = self
            .updated
            .as_ref()
            .map_or("null".to_string(), |(version, at)| {
                Object::new()
                    .str("version", version)
                    .num("at", *at)
                    .finish()
            });
        Object::new()
            .num("format", 1)
            .str("state", &self.state)
            .raw("checked_at", opt(self.checked_at))
            .raw("last_success_at", opt(self.last_success_at))
            .raw("available", available)
            .raw("progress", progress)
            .raw("error", error)
            .raw("updated", updated)
            .finish()
    }

    /// A state file's contents; anything unreadable is no state at all.
    pub fn from_json(text: &str) -> State {
        let Ok(json) = lcl_spec::json::parse(text) else {
            return State::default();
        };
        let s = |value: &Json, key: &str| value.get(key).and_then(Json::as_str).map(str::to_string);
        let n = |value: &Json, key: &str| value.get(key).and_then(Json::as_u64);
        State {
            state: s(&json, "state").unwrap_or_default(),
            checked_at: n(&json, "checked_at"),
            last_success_at: n(&json, "last_success_at"),
            available: json.get("available").and_then(|a| {
                Some(Available {
                    version: s(a, "version")?,
                    tag: s(a, "tag")?,
                    published_at: s(a, "published_at")?,
                    release_notes: s(a, "release_notes")?,
                    size: n(a, "size")?,
                })
            }),
            progress: json
                .get("progress")
                .and_then(|p| Some((n(p, "done")?, n(p, "total")?))),
            error: json
                .get("error")
                .and_then(|e| Some((s(e, "kind")?, s(e, "message")?))),
            updated: json
                .get("updated")
                .and_then(|u| Some((s(u, "version")?, n(u, "at")?))),
        }
    }
}

pub fn load(paths: &Paths) -> State {
    std::fs::read_to_string(paths.state_file())
        .map(|text| State::from_json(&text))
        .unwrap_or_default()
}

pub fn save(paths: &Paths, state: &State) -> Result<(), String> {
    write_atomically(&paths.state_file(), state.to_json().as_bytes())
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Create a directory, and its parents, readable by the user alone.
pub fn private_dir(dir: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)
        .and_then(|_| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)))
        .map_err(|e| format!("{}: {e}", dir.display()))
}

/// Write `bytes` to `path` atomically and durably: temporary file, sync,
/// rename, sync of the directory.
pub fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("a file needs a directory")?;
    private_dir(dir)?;
    let temporary = dir.join(format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default(),
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{}: {e}", path.display()));
    }
    std::fs::rename(&temporary, path).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        format!("{}: {e}", path.display())
    })?;
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("{}: {e}", dir.display()))
}

/// Exclusive use of the updater, held until dropped. Two update actions at
/// once, from two windows or a window and a terminal, never interleave.
pub struct Lock {
    _file: File,
}

pub fn lock(paths: &Paths) -> Result<Lock, String> {
    private_dir(&paths.state)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.state.join("lock"))
        .map_err(|e| e.to_string())?;
    match file.try_lock() {
        Ok(()) => Ok(Lock { _file: file }),
        Err(_) => Err("another update action is running".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_reads_back_exactly_and_a_check_falls_due_after_a_day() {
        let state = State {
            state: "update_available".into(),
            checked_at: Some(100),
            last_success_at: Some(100),
            available: Some(Available {
                version: "0.2.0".into(),
                tag: "v0.2.0".into(),
                published_at: "2026-10-01T12:00:00Z".into(),
                release_notes: "notes \"quoted\"\n".into(),
                size: 42,
            }),
            progress: Some((1, 2)),
            error: Some(("download".into(), "cut short".into())),
            updated: Some(("0.1.0".into(), 7)),
        };
        assert_eq!(State::from_json(&state.to_json()), state);
        assert!(!state.check_due(100 + CHECK_INTERVAL_SECS - 1));
        assert!(state.check_due(100 + CHECK_INTERVAL_SECS));
        assert!(State::default().check_due(0));
        assert_eq!(State::from_json("{broken"), State::default());
    }

    #[test]
    fn only_absolute_locations_count_and_the_rest_fall_back_under_home() {
        let paths = |pairs: &[(&str, &str)]| {
            let pairs: Vec<(String, OsString)> = pairs
                .iter()
                .map(|(k, v)| (k.to_string(), OsString::from(v)))
                .collect();
            Paths::from_vars(move |name| {
                pairs
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
            })
        };
        let defaults = Paths {
            bin: PathBuf::from("/home/u/.local/bin"),
            data: PathBuf::from("/home/u/.local/share"),
            state: PathBuf::from("/home/u/.local/state/lcl/update"),
            cache: PathBuf::from("/home/u/.cache/lcl/update"),
        };
        assert_eq!(paths(&[("HOME", "/home/u")]), Ok(defaults.clone()));
        let relative = [
            ("HOME", "/home/u"),
            ("XDG_BIN_HOME", "bin"),
            ("XDG_DATA_HOME", "./share"),
            ("XDG_STATE_HOME", "../state"),
            ("XDG_CACHE_HOME", ""),
        ];
        assert_eq!(paths(&relative), Ok(defaults));
        let absolute = paths(&[
            ("HOME", "/home/u"),
            ("XDG_BIN_HOME", "/opt/b"),
            ("XDG_DATA_HOME", "/opt/d"),
            ("XDG_STATE_HOME", "/opt/s"),
            ("XDG_CACHE_HOME", "/opt/c"),
        ])
        .unwrap();
        assert_eq!(absolute.bin, PathBuf::from("/opt/b"));
        assert_eq!(absolute.data, PathBuf::from("/opt/d"));
        assert_eq!(absolute.state, PathBuf::from("/opt/s/lcl/update"));
        assert_eq!(absolute.cache, PathBuf::from("/opt/c/lcl/update"));
        for home in [None, Some("home/u"), Some("")] {
            let vars: Vec<(&str, &str)> = home.map(|h| ("HOME", h)).into_iter().collect();
            assert!(paths(&vars).is_err(), "{home:?}");
        }
    }

    #[test]
    fn one_update_action_at_a_time() {
        let root = std::env::temp_dir().join(format!("lcl-update-lock-{}", std::process::id()));
        let paths = Paths {
            bin: root.join("bin"),
            data: root.join("data"),
            state: root.join("state"),
            cache: root.join("cache"),
        };
        let held = lock(&paths).unwrap();
        assert!(lock(&paths).is_err());
        drop(held);
        assert!(lock(&paths).is_ok());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
