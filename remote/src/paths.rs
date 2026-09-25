//! Where the remote service keeps what it knows, and how it writes it.
//!
//! Everything lives under the user's own XDG directories, never in a project:
//!
//! * `$XDG_CONFIG_HOME/lcl/remote/` — the PC's identity (its private key among
//!   it), the trusted-device registry and the service configuration;
//! * `$XDG_STATE_HOME/lcl/remote/` — one-time pairing challenges and the
//!   running service's status, which change as it runs.
//!
//! Both directories are created `0700` and every file in them is written
//! `0600`, atomically: a temporary file in the same directory, flushed to disk
//! and renamed over the old one, so a crash never leaves half a registry.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The remote service's directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `$XDG_CONFIG_HOME/lcl/remote`.
    pub config: PathBuf,
    /// `$XDG_STATE_HOME/lcl/remote`.
    pub state: PathBuf,
    /// The workspace settings file the desktop workspace writes, which names
    /// the default workspace folder and the default file type.
    pub workspace_settings: PathBuf,
    /// The built-in default workspace, `$XDG_DATA_HOME/lcl/workspace`.
    pub builtin_workspace: PathBuf,
    /// Where an installation keeps the specification packages.
    pub data: PathBuf,
}

impl Paths {
    /// The directories for this process's environment. `HOME` or the XDG
    /// variables must name absolute paths; a relative one is ignored, as the
    /// XDG Base Directory Specification requires.
    pub fn from_env() -> Result<Paths, String> {
        let var = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let home = var("HOME");
        let base = |xdg: &str, fallback: &str| {
            var(xdg)
                .or_else(|| home.as_ref().map(|h| h.join(fallback)))
                .ok_or_else(|| format!("neither {xdg} nor HOME names an absolute directory"))
        };
        Ok(Paths::rooted(
            &base("XDG_CONFIG_HOME", ".config")?,
            &base("XDG_STATE_HOME", ".local/state")?,
            &base("XDG_DATA_HOME", ".local/share")?,
        ))
    }

    /// The directories under three XDG bases.
    pub fn rooted(config_home: &Path, state_home: &Path, data_home: &Path) -> Paths {
        Paths {
            config: config_home.join("lcl/remote"),
            state: state_home.join("lcl/remote"),
            workspace_settings: config_home.join("lcl/workspace-settings.json"),
            builtin_workspace: data_home.join("lcl/workspace"),
            data: data_home.join("lcl"),
        }
    }

    /// Everything under one directory, for tests and for isolated runs.
    pub fn under(root: &Path) -> Paths {
        Paths::rooted(
            &root.join("config"),
            &root.join("state"),
            &root.join("data"),
        )
    }
}

/// Create a directory, and its parents, readable by the user alone.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Write a file readable by the user alone, atomically.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    private_dir(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let temporary = dir.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed)
    ));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        });
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// An exclusive lock on one registry, held until dropped.
///
/// The service and the command line both change the device registry and the
/// pairing challenges; a read-modify-write under this lock is how two of them
/// at once cannot lose each other's change.
pub struct Locked {
    _file: File,
}

pub fn lock(path: &Path) -> std::io::Result<Locked> {
    let dir = path.parent().unwrap_or(Path::new("."));
    private_dir(dir)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.lock()?;
    Ok(Locked { _file: file })
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
