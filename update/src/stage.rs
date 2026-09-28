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
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// The most a program run here may print on either output; more is refused
/// rather than held. A release payload's full listing is under 200 KB.
const MAX_OUTPUT: u64 = 16 * 1024 * 1024;

/// Read `pipe` to its end on a thread of its own, so the program writing it
/// never waits on a full pipe: its bytes, at most `MAX_OUTPUT` + 1 of them.
/// Past that the pipe is closed, and the program's next write to it fails.
fn drain<R: Read + Send + 'static>(pipe: R) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = pipe
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(read);
    });
    receiver
}

/// Run `program` with `args`, at most `limit` long; its exit status and
/// standard output.
///
/// Both outputs are read while the program runs. A pipe holds only so much
/// (64 KiB on Linux), and a program that fills one waits for it to be read:
/// forever, if nothing reads until the program has exited. A program still
/// running at the limit is killed and reaped, and outputs that something it
/// started keeps open are not waited for past the limit either.
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
    let deadline = Instant::now() + limit;
    let mut child = command
        .spawn()
        .map_err(|e| format!("{}: {e}", program.display()))?;
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);
    let too_slow = || {
        format!(
            "{} did not finish in {}s",
            program.display(),
            limit.as_secs()
        )
    };
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(too_slow());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let collect = |output: Option<mpsc::Receiver<io::Result<Vec<u8>>>>| {
        let output = output.ok_or_else(|| format!("{}: no output pipe", program.display()))?;
        match output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(bytes)) if bytes.len() as u64 <= MAX_OUTPUT => Ok(bytes),
            Ok(Ok(_)) => Err(format!(
                "{} printed more than {MAX_OUTPUT} bytes",
                program.display()
            )),
            Ok(Err(e)) => Err(format!("{}: {e}", program.display())),
            Err(_) => Err(too_slow()),
        }
    };
    let out = collect(stdout)?;
    collect(stderr)?;
    Ok((status.success(), String::from_utf8_lossy(&out).into_owned()))
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

    fn sh(script: &str, limit: u64) -> Result<(bool, String), String> {
        run(
            Path::new("/bin/sh"),
            &["-c", script],
            None,
            Duration::from_secs(limit),
        )
    }

    #[test]
    fn output_larger_than_a_pipe_is_read_while_the_program_runs() {
        // Far more than a pipe holds, on both outputs at once.
        let started = Instant::now();
        let (ok, out) = sh(
            "head -c 3000000 /dev/zero | tr '\\0' x; head -c 3000000 /dev/zero >&2",
            20,
        )
        .expect("the program finishes");
        assert!(ok);
        assert_eq!(out.len(), 3_000_000);
        assert!(out.bytes().all(|b| b == b'x'));
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn output_past_the_bound_is_refused_rather_than_held() {
        let why = sh(&format!("head -c {} /dev/zero", MAX_OUTPUT + 10), 60).unwrap_err();
        assert!(why.contains("printed more than"), "{why}");
        let why = sh(&format!("head -c {} /dev/zero >&2", MAX_OUTPUT + 10), 60).unwrap_err();
        assert!(why.contains("printed more than"), "{why}");
    }

    #[test]
    fn a_program_past_its_limit_is_killed_and_the_run_ends_on_time() {
        let started = Instant::now();
        let why = sh("sleep 5", 1).unwrap_err();
        assert!(why.contains("did not finish in 1s"), "{why}");
        assert!(started.elapsed() < Duration::from_secs(4));
        // A program that has exited while something it started still holds
        // its output open is not waited for past the limit either.
        let started = Instant::now();
        let why = sh("sleep 5 & echo started", 1).unwrap_err();
        assert!(why.contains("did not finish in 1s"), "{why}");
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn a_payload_with_a_release_sized_listing_unpacks() {
        // A real payload lists about 1600 members, 174 KB with -tvzf: well
        // past what a pipe holds. This one lists 3000.
        let scratch =
            std::env::temp_dir().join(format!("lcl-update-listing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        let top = "lcl-9.9.9-linux-x86_64";
        for n in 0..3000 {
            let dir = scratch.join("src").join(top).join(format!(
                "share/LCL_Core_0.1.0/registries/group-{:02}",
                n % 40
            ));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("entry-{n:05}.json")), b"{}").unwrap();
        }
        let archive = scratch.join("payload.tar.gz");
        let status = Command::new("tar")
            .args(["-czf", archive.to_str().unwrap(), "-C"])
            .arg(scratch.join("src"))
            .arg(top)
            .status()
            .unwrap();
        assert!(status.success());
        let listing = run(
            Path::new("tar"),
            &["-tvzf", archive.to_str().unwrap()],
            None,
            Duration::from_secs(60),
        )
        .unwrap()
        .1;
        assert!(listing.len() > 256 * 1024, "{}", listing.len());

        let started = Instant::now();
        let payload = extract(&archive, &scratch.join("into")).expect("the payload unpacks");
        assert!(started.elapsed() < Duration::from_secs(60));
        assert!(payload
            .join("share/LCL_Core_0.1.0/registries/group-39/entry-02999.json")
            .is_file());
        std::fs::remove_dir_all(&scratch).unwrap();
    }

    #[test]
    fn digests_are_rings_sha256() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
