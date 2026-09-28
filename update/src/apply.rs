//! Installing a staged update over the user-local installation.
//!
//! The staged, verified payload installs itself with its own `install.sh`,
//! exactly as a person would, so there is one installation model, not two.
//! Before it runs, everything that installer will write is copied aside; if the
//! installer fails, or the installed LCL then fails its health check, or the
//! remote service that was running does not come back, that copy is put back.
//! User data — projects, `~/.config/lcl`, `~/.local/state/lcl`, Masters,
//! settings, the PC identity and the trusted devices — is never among what the
//! installer writes, and is never touched here.

use crate::check::{self, Context};
use crate::stage::{self, run};
use crate::state::{self, State};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const REMOTE_UNIT: &str = "lcl-remote.service";

fn systemctl() -> PathBuf {
    #[cfg(feature = "test-endpoint")]
    if let Some(program) = std::env::var_os("LCL_UPDATE_SYSTEMCTL") {
        return PathBuf::from(program);
    }
    PathBuf::from("systemctl")
}

fn service(action: &str) -> bool {
    matches!(
        run(
            &systemctl(),
            &["--user", action, "--quiet", REMOTE_UNIT],
            None,
            Duration::from_secs(60)
        ),
        Ok((true, _))
    )
}

/// Wait for process `pid` to end: the workspace that asked for this update.
fn wait_for_exit(pid: u32, limit: Duration) -> bool {
    let started = Instant::now();
    while Path::new(&format!("/proc/{pid}")).exists() {
        if started.elapsed() > limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// Copy a file, a directory tree or a link from `from` to `to`.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(from)?, to)
    } else if meta.is_dir() {
        std::fs::create_dir(to)?;
        std::fs::set_permissions(to, meta.permissions())?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        // Nothing there: not even the folders on the way to it.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// What was at each path the installer writes, copied aside.
struct Rollback {
    dir: PathBuf,
    /// Each target, and the copy of what was there, if anything was.
    entries: Vec<(PathBuf, Option<PathBuf>)>,
}

impl Rollback {
    fn take(dir: PathBuf, targets: &[PathBuf]) -> Result<Rollback, String> {
        let _ = std::fs::remove_dir_all(&dir);
        state::private_dir(&dir)?;
        let mut entries = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            let copy = if std::fs::symlink_metadata(target).is_ok() {
                let copy = dir.join(i.to_string());
                copy_tree(target, &copy).map_err(|e| format!("{}: {e}", target.display()))?;
                Some(copy)
            } else {
                None
            };
            entries.push((target.clone(), copy));
        }
        Ok(Rollback { dir, entries })
    }

    /// Put every target back as it was; true when all of it was restored.
    fn restore(&self) -> bool {
        let mut whole = true;
        for (target, copy) in self.entries.iter().rev() {
            whole &= remove_tree(target).is_ok();
            if let Some(copy) = copy {
                whole &= copy_tree(copy, target).is_ok();
            }
        }
        whole
    }

    fn discard(self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The installed LCL is the one `version`: its binaries report it and open
/// their specification package.
fn healthy(ctx: &Context, version: &str) -> Result<(), String> {
    let limit = Duration::from_secs(60);
    let answers = |program: &str, args: &[&str], wanted: &str| match run(
        &ctx.paths.bin.join(program),
        args,
        None,
        limit,
    )? {
        (true, out) if out.trim() == wanted => Ok(()),
        (_, out) => Err(format!("the installed {program} answered {:?}", out.trim())),
    };
    answers("lcl-update", &["version"], &format!("lcl-update {version}"))?;
    answers("lcl", &["--version"], &format!("lcl {version}"))?;
    let spec = ctx.paths.data.join("lcl/LCL_Core_0.1.0");
    match run(
        &ctx.paths.bin.join("lcl"),
        &["spec", "--spec", spec.to_str().unwrap_or("")],
        None,
        limit,
    )? {
        (true, _) => Ok(()),
        (false, _) => Err("the installed lcl could not open its specification package".to_string()),
    }
}

/// Start the workspace again through its launcher, detached from this process.
fn relaunch(ctx: &Context) {
    let launcher = ctx.paths.bin.join("lcl-workspace-launch");
    let mut command = Command::new(launcher);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let _ = command.spawn();
}

/// Install the staged update. `wait_pid` is the workspace that asked for it,
/// which must have exited first; with `relaunch`, the workspace is started
/// again afterwards, on the new version or, after a failure, the old one.
pub fn apply(
    ctx: &Context,
    wait_pid: Option<u32>,
    relaunch_after: bool,
    report: &mut dyn FnMut(&State),
) -> State {
    let mut state = state::load(&ctx.paths);
    state.error = None;
    let fail = |state: State, kind: &str, why: String| {
        let state = state.failed(kind, why);
        if relaunch_after {
            relaunch(ctx);
        }
        state
    };
    // The running updater must be the installed one: an update replaces the
    // installation this program belongs to, never another.
    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let bin = ctx.paths.bin.canonicalize().ok();
    if here.as_deref().and_then(Path::parent) != bin.as_deref() {
        return fail(state, "install", format!(
            "this lcl-update is not the one installed in {}; updates apply only to that installation",
            ctx.paths.bin.display()
        ));
    }
    let manifest = match check::saved_manifest(ctx) {
        Ok(manifest) => manifest,
        Err(why) => return fail(state, "verification", why),
    };
    match check::applicable(&manifest, &ctx.installed) {
        Ok(true) => {}
        Ok(false) => {
            return fail(
                state,
                "unsupported",
                "the staged update is not newer than LCL here".to_string(),
            )
        }
        Err((kind, why)) => return fail(state, kind, why),
    }
    let payload = match stage::staged_payload(ctx, &manifest) {
        Ok(payload) => payload,
        Err(why) => return fail(state, "verification", why),
    };
    if let Some(pid) = wait_pid {
        if !wait_for_exit(pid, Duration::from_secs(60)) {
            return fail(
                state,
                "install",
                "LCL Workspace did not close, so nothing was changed".to_string(),
            );
        }
    }
    state.state = "installing".to_string();
    report(&state);

    let installer = payload.join("install.sh");
    let targets: Vec<PathBuf> = match run(
        &installer,
        &["--list"],
        Some(&payload),
        Duration::from_secs(60),
    ) {
        Ok((true, out)) => out
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect(),
        _ => {
            return fail(
                state,
                "install",
                "the staged installer could not list what it installs".to_string(),
            )
        }
    };
    // Every target is an absolute path under this installation's own
    // directories; anything else is not the installer's to write.
    let owned = |t: &Path| {
        t.is_absolute()
            && !t
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            && (t.starts_with(&ctx.paths.bin) || t.starts_with(&ctx.paths.data))
    };
    if let Some(bad) = targets.iter().find(|t| !owned(t)) {
        return fail(
            state,
            "install",
            format!(
                "the installer names {}, outside the installation",
                bad.display()
            ),
        );
    }
    let rollback = match Rollback::take(ctx.paths.rollback(), &targets) {
        Ok(rollback) => rollback,
        Err(why) => {
            let _ = std::fs::remove_dir_all(ctx.paths.rollback());
            return fail(
                state,
                "install",
                format!("could not keep a copy to roll back to, so nothing was changed: {why}"),
            );
        }
    };
    let remote_installed = ctx.paths.bin.join("lcl-remote").exists();
    let remote_was_running = remote_installed && service("is-active");
    if remote_was_running && !service("stop") {
        rollback.discard();
        return fail(
            state,
            "install",
            "the LCL remote service could not be stopped, so nothing was changed".to_string(),
        );
    }

    let version = manifest.product_version.to_string();
    let log = std::fs::File::create(ctx.paths.log());
    let installed = Command::new(&installer)
        .current_dir(&payload)
        .stdin(Stdio::null())
        .stdout(
            log.as_ref()
                .ok()
                .and_then(|f| f.try_clone().ok())
                .map_or(Stdio::null(), Stdio::from),
        )
        .stderr(
            log.as_ref()
                .ok()
                .and_then(|f| f.try_clone().ok())
                .map_or(Stdio::null(), Stdio::from),
        )
        .status()
        .map_err(|e| e.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(format!("the installer failed ({status})"))
            }
        })
        .and_then(|_| healthy(ctx, &version))
        .and_then(|_| {
            if remote_was_running && !(service("start") && service("is-active")) {
                Err("the LCL remote service did not start again on the new version".to_string())
            } else {
                Ok(())
            }
        });
    match installed {
        Ok(()) => {
            rollback.discard();
            let _ = std::fs::remove_dir_all(ctx.paths.staging());
            let _ = std::fs::remove_file(ctx.paths.manifest());
            let _ = std::fs::remove_file(ctx.paths.signature());
            state.state = "up_to_date".to_string();
            state.available = None;
            state.progress = None;
            state.updated = Some((version, state::now()));
            if relaunch_after {
                relaunch(ctx);
            }
            state
        }
        Err(why) => {
            let restored = rollback.restore();
            if remote_was_running {
                service("start");
            }
            if restored {
                rollback.discard();
                fail(
                    state,
                    "install",
                    format!("{why}; the previous version was restored"),
                )
            } else {
                fail(state, "install", format!(
                    "{why}; the previous version could not be fully restored, and its copy is kept in {}",
                    ctx.paths.rollback().display()
                ))
            }
        }
    }
}
