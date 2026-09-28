//! Downloading an update and staging it: the artifact is fetched from the same
//! release the manifest came from, trusted only when its size and SHA-256 are
//! the signed ones, unpacked into the cache with every member checked first,
//! and then made to prove itself — its own binaries must report the signed
//! version and open their specification package — before anything installed
//! is touched.

use crate::check::{self, Context};
use crate::manifest::Manifest;
use crate::state::{self, State};
use crate::trust;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Run `program` with `args`, at most `limit` long; its exit status and
/// standard output.
pub(crate) fn run(
    program: &Path,
    args: &[&str],
    dir: Option<&Path>,
    limit: Duration,
) -> Result<(bool, String), String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let output = child.wait_with_output().map_err(|e| e.to_string())?;
            return Ok((
                status.success(),
                String::from_utf8_lossy(&output.stdout).into_owned(),
            ));
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{} did not finish in {}s",
                program.display(),
                limit.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The SHA-256 of `bytes`, by `ring`.
pub fn sha256(bytes: &[u8]) -> String {
    trust::hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}

/// Whether one member name of an archive may be unpacked: relative, below one
/// top-level directory, and without `.` or `..` components.
fn safe_member(name: &str, top: &str) -> bool {
    let Some(rest) = name.strip_prefix(top) else {
        return false;
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return false;
    }
    let inner = rest.strip_prefix('/').unwrap_or(rest);
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    inner.is_empty()
        || inner
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Unpack `archive` into `into` after checking every member: only regular
/// files and directories, all under one `lcl-<version>-linux-x86_64`
/// directory. Returns that directory.
pub fn extract(archive: &Path, into: &Path) -> Result<PathBuf, String> {
    let tar = Path::new("tar");
    let limit = Duration::from_secs(300);
    let text = |args: &[&str]| -> Result<String, String> {
        match run(tar, args, None, limit)? {
            (true, out) => Ok(out),
            (false, _) => Err(format!("{} is not a readable archive", archive.display())),
        }
    };
    let archive_arg = archive.to_str().ok_or("the archive path is not UTF-8")?;
    let names = text(&["-tzf", archive_arg])?;
    let long = text(&["-tvzf", archive_arg])?;
    let top = names
        .lines()
        .next()
        .and_then(|first| first.split('/').next())
        .unwrap_or("")
        .to_string();
    let top_ok = top.starts_with("lcl-")
        && top.ends_with("-linux-x86_64")
        && top
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'));
    if !top_ok {
        return Err("the archive is not an LCL release payload".to_string());
    }
    if let Some(bad) = names.lines().find(|name| !safe_member(name, &top)) {
        return Err(format!("the archive holds an unsafe member {bad:?}"));
    }
    if long
        .lines()
        .any(|line| !(line.starts_with('-') || line.starts_with('d')))
    {
        return Err("the archive holds a link or a special file".to_string());
    }
    state::private_dir(into)?;
    let into_arg = into.to_str().ok_or("the staging path is not UTF-8")?;
    text(&["-xzf", archive_arg, "-C", into_arg, "--no-same-owner"])?;
    Ok(into.join(top))
}

/// Make a staged payload prove it is the release its manifest signed.
pub fn validate(payload: &Path, manifest: &Manifest) -> Result<(), String> {
    for required in [
        "bin/lcl",
        "bin/lcl-workspace",
        "bin/lcl-update",
        "install.sh",
        "share/LCL_Core_0.1.0",
    ] {
        if !payload.join(required).exists() {
            return Err(format!("the staged update lacks {required}"));
        }
    }
    let version = manifest.product_version.to_string();
    let limit = Duration::from_secs(60);
    let answers = |program: &str, args: &[&str], wanted: &str| -> Result<(), String> {
        match run(&payload.join(program), args, Some(payload), limit)? {
            (true, out) if out.trim() == wanted => Ok(()),
            (_, out) => Err(format!(
                "the staged {program} answered {:?}, not {wanted:?}",
                out.trim()
            )),
        }
    };
    answers(
        "bin/lcl-update",
        &["version"],
        &format!("lcl-update {version}"),
    )?;
    answers("bin/lcl", &["--version"], &format!("lcl {version}"))?;
    let spec = payload.join("share/LCL_Core_0.1.0");
    let spec_arg = spec.to_str().ok_or("the staging path is not UTF-8")?;
    match run(
        &payload.join("bin/lcl"),
        &["spec", "--spec", spec_arg],
        Some(payload),
        limit,
    )? {
        (true, _) => {}
        (false, _) => {
            return Err("the staged lcl could not open its specification package".to_string())
        }
    }
    match run(
        &payload.join("install.sh"),
        &["--list"],
        Some(payload),
        limit,
    )? {
        (true, out) if !out.trim().is_empty() => Ok(()),
        _ => Err("the staged installer could not list what it installs".to_string()),
    }
}

/// Download, verify and stage the update a check found. `report` sees each
/// state as it changes, so a window can show progress.
pub fn download(ctx: &Context, report: &mut dyn FnMut(&State)) -> State {
    let mut state = state::load(&ctx.paths);
    state.error = None;
    let manifest = match check::saved_manifest(ctx) {
        Ok(manifest) => manifest,
        Err(why) => return state.failed("verification", why),
    };
    match check::applicable(&manifest, &ctx.installed) {
        Ok(true) => {}
        Ok(false) => {
            return state.failed("unsupported", "the saved update is not newer than LCL here")
        }
        Err((kind, why)) => return state.failed(kind, why),
    }
    let release = match ctx.source.latest() {
        Ok(Some(release)) if release.tag == manifest.release_tag => release,
        Ok(_) => {
            return state.failed(
                "invalid",
                "the latest release changed since the last check; check again",
            )
        }
        Err(failure) => return state.failed(check::kind(&failure), failure.to_string()),
    };
    match release.asset(&manifest.pc.artifact_name) {
        Some(asset) if asset.size == manifest.pc.size => {}
        _ => {
            return state.failed(
                "invalid",
                "the release does not hold the signed PC artifact",
            )
        }
    }
    // One staged update at most: whatever was staged before goes first.
    let staging = ctx.paths.staging();
    let _ = std::fs::remove_dir_all(&staging);
    state.state = "downloading".to_string();
    state.progress = Some((0, manifest.pc.size));
    report(&state);
    let mut last = 0;
    let bytes = {
        let mut progress = |done: u64, _: Option<u64>| {
            if done - last > manifest.pc.size / 100 || done == manifest.pc.size {
                last = done;
                state.progress = Some((done, manifest.pc.size));
                report(&state);
            }
        };
        ctx.source.fetch(
            &release,
            &manifest.pc.artifact_name,
            manifest.pc.size,
            &mut progress,
        )
    };
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(failure) => {
            let kind = if check::kind(&failure) == "offline" {
                "offline"
            } else {
                "download"
            };
            return state.failed(kind, failure.to_string());
        }
    };
    if bytes.len() as u64 != manifest.pc.size || sha256(&bytes) != manifest.pc.sha256 {
        return state.failed(
            "verification",
            "the downloaded update does not match the size and SHA-256 its manifest signed",
        );
    }
    let staged = state::private_dir(&staging)
        .and_then(|_| {
            let archive = staging.join(&manifest.pc.artifact_name);
            std::fs::write(&archive, &bytes).map_err(|e| e.to_string())?;
            extract(&archive, &staging.join("payload"))
        })
        .and_then(|payload| validate(&payload, &manifest));
    if let Err(why) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return state.failed("verification", why);
    }
    state.state = "ready_to_install".to_string();
    state.progress = None;
    state
}

/// The staged payload directory of `manifest`, verified again byte for byte.
pub fn staged_payload(ctx: &Context, manifest: &Manifest) -> Result<PathBuf, String> {
    let staging = ctx.paths.staging();
    let archive = staging.join(&manifest.pc.artifact_name);
    let bytes = std::fs::read(&archive)
        .map_err(|_| "no update is staged; download it first".to_string())?;
    if bytes.len() as u64 != manifest.pc.size || sha256(&bytes) != manifest.pc.sha256 {
        return Err("the staged update no longer matches its signed manifest".to_string());
    }
    let payload = std::fs::read_dir(staging.join("payload"))
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .ok_or("the staged update was not unpacked")?;
    validate(&payload, manifest)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_members_inside_one_top_directory_are_safe() {
        let top = "lcl-0.3.0-linux-x86_64";
        for good in [
            "lcl-0.3.0-linux-x86_64/",
            "lcl-0.3.0-linux-x86_64/bin/lcl",
            "lcl-0.3.0-linux-x86_64/share/a/",
        ] {
            assert!(safe_member(good, top), "{good}");
        }
        for bad in [
            "/etc/passwd",
            "lcl-0.3.0-linux-x86_64/../evil",
            "lcl-0.3.0-linux-x86_64/./x",
            "lcl-0.3.0-linux-x86_64//x",
            "other/x",
            "lcl-0.3.0-linux-x86_64x/y",
        ] {
            assert!(!safe_member(bad, top), "{bad}");
        }
    }

    #[test]
    fn digests_are_rings_sha256() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
