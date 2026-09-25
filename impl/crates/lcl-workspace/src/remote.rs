//! Android devices, from the desktop: pairing, approving pairing requests,
//! the device list, revocation.
//!
//! The PC side of LCL for Android is its own program, `lcl-remote`, with its
//! own identity and trust store. The workspace reimplements none of it: its
//! routes run that program with fixed arguments — `devices --json`,
//! `pair --json`, `pending --json`, `approve ID`, `deny ID`, `revoke ID` —
//! and hand the answer to the page. The only thing a request can put on that
//! command line is a device or request id, and only one that looks like one.
//! There is no shell.
//!
//! The routes sit behind the same loopback address and session token as
//! every other workspace route, so only the person at this PC reaches them.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The program's name, as an installation puts it beside `lcl-workspace`.
pub const PROGRAM: &str = "lcl-remote";

/// Where `lcl-remote` is: `LCL_REMOTE_BIN`, then beside this program, then
/// on `PATH`. `None` when it is not installed.
pub fn binary() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("LCL_REMOTE_BIN").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(explicit));
    }
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(PROGRAM)));
    if let Some(path) = beside.filter(|p| p.is_file()) {
        return Some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(PROGRAM))
        .find(|candidate| candidate.is_file())
}

/// Run `lcl-remote` with these arguments: what it printed, or why it failed
/// in its own words.
pub fn run(binary: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{} could not be run: {e}", binary.display()))?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|_| format!("{PROGRAM} printed something that is not UTF-8"));
    }
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim().trim_start_matches("lcl-remote: ").trim();
    Err(if said.is_empty() {
        format!("{PROGRAM} failed ({})", output.status)
    } else {
        said.to_string()
    })
}

/// A device id as `lcl-remote` writes them: lowercase hexadecimal. Nothing
/// else is passed on, so no request can put an option or a path on the
/// command line.
pub fn valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A pairing request id as `lcl-remote` writes them: lowercase hexadecimal,
/// exactly like a device id, and checked the same way.
pub fn valid_request_id(id: &str) -> bool {
    valid_device_id(id)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in `lcl-remote`: a script that answers one way.
    fn fake(name: &str, script: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lcl-workspace-remote-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(PROGRAM);
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn what_the_program_prints_is_what_the_route_answers() {
        let program = fake("ok", "printf '%s ' \"$@\"; echo; echo '{\"devices\":[]}'");
        let printed = run(&program, &["devices", "--json"]).unwrap();
        assert_eq!(printed, "devices --json \n{\"devices\":[]}\n");
    }

    #[test]
    fn a_failure_is_reported_in_the_program_s_own_words() {
        let program = fake(
            "fails",
            "echo 'lcl-remote: no paired device has the id 00' >&2; exit 1",
        );
        assert_eq!(
            run(&program, &["revoke", "00"]).unwrap_err(),
            "no paired device has the id 00"
        );
        let silent = fake("silent", "exit 3");
        assert!(run(&silent, &[]).unwrap_err().contains("failed"));
        let missing = std::env::temp_dir().join("lcl-workspace-remote-nowhere/lcl-remote");
        assert!(run(&missing, &[]).unwrap_err().contains("could not be run"));
    }

    #[test]
    fn only_device_ids_reach_the_command_line() {
        assert!(valid_device_id("ce7f8408067ea40f"));
        for bad in [
            "",
            "-rf",
            "--json",
            "../devices.json",
            "a b",
            "CE7F",
            "id;rm",
            &"a".repeat(65),
        ] {
            assert!(!valid_device_id(bad), "{bad:?} was accepted");
            assert!(!valid_request_id(bad), "{bad:?} was accepted");
        }
        assert!(valid_request_id("8c1f2e3d4a5b6c7d"));
    }
}
