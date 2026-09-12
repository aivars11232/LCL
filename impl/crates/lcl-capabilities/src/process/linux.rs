//! Linux process-group ownership and nonblocking capture, without reader threads.
//!
//! The direct child remains waitable (WNOWAIT) until the final group signal, so
//! its PID/PGID cannot be recycled underneath that signal. This owns a process
//! group, not a sandbox for programs deliberately creating other sessions.
//! Teardown has an explicit one-second allowance after the termination request;
//! failure to establish cleanup is returned with the owned PID, never success.

use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::raw::c_int;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_millis(5);
pub(crate) const TEARDOWN: Duration = Duration::from_secs(1);

// Linux x86_64 ABI; the caller enables this module only on that target.
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const O_NONBLOCK: c_int = 0o4000;
const SIGKILL: c_int = 9;
const P_PID: c_int = 1;
const WNOHANG: c_int = 1;
const WEXITED: c_int = 4;
const WNOWAIT: c_int = 0x0100_0000;

extern "C" {
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn kill(pid: c_int, signal: c_int) -> c_int;
    fn waitid(kind: c_int, id: u32, info: *mut SignalInfo, options: c_int) -> c_int;
}

/// Linux siginfo_t has 128 bytes, eight-byte alignment on x86_64. Only its
/// first int (si_signo) is read: waitid writes SIGCHLD for an observed exit and
/// clears it when WNOHANG observes none. No union layout is interpreted.
#[repr(C, align(8))]
struct SignalInfo([u8; 128]);

#[derive(Debug, Default)]
pub(crate) struct Output {
    pub exit_code: Option<i64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug)]
pub(crate) struct Failure {
    pub started: bool,
    pub timed_out: bool,
    pub cleanup_complete: bool,
    pub detail: String,
    pub output: Output,
}

fn nonblocking(stream: &impl AsRawFd) -> io::Result<()> {
    let fd = stream.as_raw_fd();
    // SAFETY: the borrowed stream owns a live fd throughout both calls;
    // F_GETFL takes no third argument, F_SETFL takes one c_int.
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn exited(pid: u32) -> io::Result<bool> {
    let mut info = SignalInfo([0; 128]);
    // SAFETY: info has the Linux x86_64 siginfo_t size and alignment; the child
    // belongs to this supervisor. WNOWAIT observes without releasing ownership.
    let result = unsafe { waitid(P_PID, pid, &mut info, WEXITED | WNOHANG | WNOWAIT) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(i32::from_ne_bytes(info.0[..4].try_into().unwrap()) != 0)
    }
}

/// A bounded turn for one stream. Even a continuous flood cannot starve the
/// other stream, child observation, or the deadline. Excess bytes are drained.
fn drain_turn(
    stream: &mut impl Read,
    retained: &mut Vec<u8>,
    truncated: &mut bool,
    cap: usize,
) -> io::Result<bool> {
    let mut buffer = [0; 8192];
    for _ in 0..8 {
        match stream.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                let keep = count.min(cap.saturating_sub(retained.len()));
                retained.extend_from_slice(&buffer[..keep]);
                *truncated |= keep < count;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

/// All group members except the held leader, including zombies. The leader is
/// still unreaped, so this process-group identity cannot belong to a new job.
fn remaining_group_members(leader: u32) -> io::Result<Vec<u32>> {
    let mut members = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == leader {
            continue;
        }
        let stat = match std::fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let fields = stat
            .rsplit_once(')')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid process stat"))?
            .1;
        let group = fields
            .split_whitespace()
            .nth(2)
            .and_then(|s| s.parse::<u32>().ok());
        if group == Some(leader) {
            members.push(pid);
        }
    }
    Ok(members)
}

fn teardown(child: &mut Child) -> Result<Option<i64>, String> {
    let pid = child.id();
    // First establish that the child is still ours. In particular, ECHILD is
    // not permission to signal a PID/PGID that somebody else may have reaped.
    exited(pid).map_err(|error| format!("cannot establish ownership of child {pid}: {error}"))?;
    let started = Instant::now();
    // SAFETY: Command::process_group(0) made this owned, unreaped child the
    // group leader; conversion is checked and negative PID targets only it.
    let group = i32::try_from(pid).map_err(|error| error.to_string())?;
    if unsafe { kill(-group, SIGKILL) } < 0 {
        return Err(format!(
            "cannot terminate owned group {pid}: {}",
            io::Error::last_os_error()
        ));
    }
    loop {
        let child_exited = exited(pid).map_err(|error| error.to_string())?;
        let members = remaining_group_members(pid).map_err(|error| error.to_string())?;
        if child_exited && members.is_empty() {
            return child
                .wait()
                .map(|status| status.code().map(i64::from))
                .map_err(|error| error.to_string());
        }
        if started.elapsed() >= TEARDOWN {
            // Reap the direct child if it is observable; do not wait without a
            // bound on an uninterruptible child. Explicitly identify any work
            // whose cleanup could not be established in the allowance.
            if child_exited {
                child.wait().map_err(|error| error.to_string())?;
            }
            return Err(format!(
                "owned group {pid} cleanup exceeded {TEARDOWN:?} (elapsed {:?}; direct child exited: {child_exited}; remaining members: {members:?})",
                started.elapsed()
            ));
        }
        std::thread::sleep(INTERVAL.min(TEARDOWN.saturating_sub(started.elapsed())));
    }
}

pub(crate) fn run(
    command: &mut Command,
    deadline: Option<Duration>,
    cap: u64,
) -> Result<Output, Failure> {
    let started = Instant::now();
    let mut child = command.process_group(0).spawn().map_err(|error| Failure {
        started: false,
        timed_out: false,
        cleanup_complete: true,
        detail: error.to_string(),
        output: Output::default(),
    })?;
    let mut output = Output::default();
    let cap = usize::try_from(cap).unwrap_or(usize::MAX);
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let collection = (|| -> Result<(), (bool, String)> {
        let out = stdout
            .as_mut()
            .ok_or((false, "stdout capture is absent".into()))?;
        let err = stderr
            .as_mut()
            .ok_or((false, "stderr capture is absent".into()))?;
        nonblocking(out)
            .and_then(|()| nonblocking(err))
            .map_err(|e| (false, e.to_string()))?;
        let mut out_eof = false;
        let mut err_eof = false;
        loop {
            if !out_eof {
                out_eof = drain_turn(out, &mut output.stdout, &mut output.truncated, cap)
                    .map_err(|e| (false, format!("stdout capture failed: {e}")))?;
            }
            if !err_eof {
                err_eof = drain_turn(err, &mut output.stderr, &mut output.truncated, cap)
                    .map_err(|e| (false, format!("stderr capture failed: {e}")))?;
            }
            let done = exited(child.id())
                .map_err(|e| (false, format!("child observation failed: {e}")))?;
            // EOF only describes the streams. A descendant may have closed
            // them and still be doing authorized work. Do not implicitly
            // terminate that work on an invocation without a deadline.
            let complete = done
                && out_eof
                && err_eof
                && remaining_group_members(child.id())
                    .map_err(|e| (false, format!("owned group observation failed: {e}")))?
                    .is_empty();
            // Check after this bounded collection/observation turn, including
            // before accepting completion, so the final turn cannot skip it.
            if deadline.is_some_and(|limit| started.elapsed() >= limit) {
                return Err((
                    true,
                    format!(
                        "the declared timeout of {:?} elapsed during process/capture supervision",
                        deadline.unwrap()
                    ),
                ));
            }
            if complete {
                return Ok(());
            }
            let interval = deadline.map_or(INTERVAL, |limit| {
                INTERVAL.min(limit.saturating_sub(started.elapsed()))
            });
            std::thread::sleep(interval);
        }
    })();
    // Closing both descriptors is synchronous. There are no reader threads or
    // blocking joins, including on capture errors and deadline expiration.
    drop(stdout);
    drop(stderr);
    let cleanup = teardown(&mut child);
    if let Ok(code) = &cleanup {
        output.exit_code = *code;
    }
    match (collection, cleanup) {
        (Ok(()), Ok(_)) => Ok(output),
        (collection, cleanup) => {
            let (timed_out, mut detail) = collection
                .err()
                .unwrap_or((false, "process collection completed".into()));
            let cleanup_complete = cleanup.is_ok();
            if let Err(error) = cleanup {
                detail.push_str(&format!("; cleanup incomplete: {error}"));
            }
            Err(Failure {
                started: true,
                timed_out,
                cleanup_complete,
                detail,
                output,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_nonblocking_empty_pipe_does_not_wait_for_its_writer() {
        let (mut reader, _writer) = std::os::unix::net::UnixStream::pair().unwrap();
        nonblocking(&reader).unwrap();
        assert_eq!(
            reader.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn capped_capture_drains_excess_and_distinguishes_eof() {
        let (mut reader, mut writer) = std::os::unix::net::UnixStream::pair().unwrap();
        writer.write_all(b"abcdef").unwrap();
        // A concurrent spawn can briefly inherit CLOEXEC descriptors between
        // fork and exec. Define end-of-stream on the socket itself, rather
        // than assuming this local descriptor is its last reference.
        writer.shutdown(std::net::Shutdown::Write).unwrap();
        drop(writer);
        nonblocking(&reader).unwrap();
        let mut retained = Vec::new();
        let mut truncated = false;
        assert!(drain_turn(&mut reader, &mut retained, &mut truncated, 3).unwrap());
        assert_eq!(retained, b"abc");
        assert!(truncated);
    }

    #[test]
    fn a_read_error_is_reported_and_preserves_already_observed_bytes() {
        struct Broken(bool);
        impl Read for Broken {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if self.0 {
                    return Err(io::Error::other("injected read error"));
                }
                self.0 = true;
                buffer[0] = b'x';
                Ok(1)
            }
        }
        let mut retained = Vec::new();
        assert!(drain_turn(&mut Broken(false), &mut retained, &mut false, 8).is_err());
        assert_eq!(retained, b"x");
    }

    #[test]
    fn exit_observation_keeps_the_child_waitable_until_explicit_reaping() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 7"]).process_group(0);
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        while !exited(child.id()).unwrap() {
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::sleep(INTERVAL);
        }
        assert!(exited(child.id()).unwrap());
        assert_eq!(teardown(&mut child).unwrap(), Some(7));
    }

    #[test]
    fn a_supervision_setup_error_cleans_up_its_started_child() {
        let mut command = Command::new("/bin/sleep");
        command.arg("3").stdin(std::process::Stdio::null());
        // Deliberately missing capture descriptors: this must be a reported
        // supervision error, never a successful empty command result.
        let started = Instant::now();
        let failure = run(&mut command, Some(Duration::from_secs(2)), 8).unwrap_err();
        assert!(failure.started && failure.cleanup_complete);
        assert!(!failure.timed_out);
        assert!(failure.detail.contains("stdout capture is absent"));
        assert!(started.elapsed() < Duration::from_millis(1500));
    }

    #[test]
    fn a_missing_executable_is_distinguished_from_a_started_failure() {
        let mut command = Command::new("/lcl-residual-fixture-missing/executable");
        let failure = run(&mut command, None, 8).unwrap_err();
        assert!(!failure.started && !failure.timed_out && failure.cleanup_complete);
    }

    #[test]
    fn the_teardown_allowance_is_not_an_undeclared_execution_timeout() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "/bin/sleep 1.2; printf done"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = run(&mut command, None, 8).unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, b"done");
        assert!(!output.truncated);
    }

    #[test]
    fn eof_does_not_terminate_unbounded_owned_work_before_it_finishes() {
        struct Fixture(std::path::PathBuf);
        impl Drop for Fixture {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let path = std::env::temp_dir().join(format!("lcl-eof-work-{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let fixture = Fixture(path);
        let marker = fixture.0.join("completed");
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                "(/bin/sleep 0.2; printf finished > \"$MARKER\") >/dev/null 2>&1 & exit 0",
            ])
            .env_clear()
            .env("MARKER", &marker)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = run(&mut command, None, 8).unwrap();
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(std::fs::read(marker).unwrap(), b"finished");
    }
}
