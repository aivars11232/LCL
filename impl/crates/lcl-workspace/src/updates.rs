//! Settings → Updates, through the installed updater.
//!
//! The workspace never talks to the release source itself: it is built on
//! `std` alone, and finding, verifying and installing an update is
//! `lcl-update`'s job, with its own pinned source and trusted keys. These
//! routes run that program, the one installed beside this workspace, and pass
//! its answers on. A check waits for its answer; a download runs on while the
//! page reads its progress; an installation first lets this workspace close,
//! because the updater replaces it and starts it again.

use crate::http::Response;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The installed updater, and how this workspace leaves when it installs.
#[derive(Clone)]
pub struct Updater {
    program: PathBuf,
    quit: Arc<dyn Fn() + Send + Sync>,
}

impl Updater {
    /// `lcl-update` beside this program, if it is installed there. Installing
    /// ends this process shortly after the answer has been sent.
    pub fn installed() -> Option<Updater> {
        let program = std::env::current_exe().ok()?.parent()?.join("lcl-update");
        program.is_file().then(|| {
            Updater::new(
                program,
                Arc::new(|| {
                    std::thread::spawn(|| {
                        std::thread::sleep(Duration::from_millis(500));
                        std::process::exit(0);
                    });
                }),
            )
        })
    }

    pub fn new(program: PathBuf, quit: Arc<dyn Fn() + Send + Sync>) -> Updater {
        Updater { program, quit }
    }

    /// Run the updater and answer with the JSON it prints.
    fn answer(&self, args: &[&str], limit: Duration) -> Response {
        let child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => return Response::error(503, &format!("the updater could not be run: {e}")),
        };
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < limit => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Response::error(504, "the updater did not answer in time");
                }
            }
        }
        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(e) => return Response::error(503, &e.to_string()),
        };
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        // Refused and failed actions still print their state, which is the
        // answer the page shows; only no answer at all is an error here.
        match lcl_spec::json::parse(&text) {
            Ok(_) => Response::json(text),
            Err(_) => Response::error(
                502,
                &format!(
                    "the updater gave no answer: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ),
        }
    }

    /// Start the updater and let it run on without this workspace.
    fn start(&self, args: &[&str]) -> Result<(), String> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    /// `GET /api/update`: what the last check found. Never uses the network.
    pub fn status(&self) -> Response {
        self.answer(&["status", "--json"], Duration::from_secs(15))
    }

    /// `POST /api/update/check`: ask the release source now.
    pub fn check(&self) -> Response {
        self.answer(&["check", "--json"], Duration::from_secs(120))
    }

    /// `POST /api/update/download`: fetch, verify and stage the update found.
    /// The page follows its progress through `GET /api/update`.
    pub fn download(&self) -> Response {
        match self.start(&["download", "--json"]) {
            Ok(()) => Response::json_status(202, "{\"started\": true}".to_string()),
            Err(why) => Response::error(503, &format!("the updater could not be started: {why}")),
        }
    }

    /// `POST /api/update/install`: hand the staged update to the updater,
    /// which waits for this workspace to close, installs it (or restores the
    /// previous version) and starts the workspace again. Only a staged,
    /// verified update is installed; the page asks about unsaved work first.
    pub fn install(&self) -> Response {
        let status = self.status();
        let staged = lcl_spec::json::parse(std::str::from_utf8(&status.body).unwrap_or(""))
            .ok()
            .and_then(|json| {
                json.get("state")?
                    .get("state")?
                    .as_str()
                    .map(|s| s == "ready_to_install")
            })
            .unwrap_or(false);
        if !staged {
            return Response::error(409, "no verified update is ready to install");
        }
        let pid = std::process::id().to_string();
        match self.start(&["apply", "--wait-pid", &pid, "--relaunch"]) {
            Ok(()) => {
                (self.quit)();
                Response::json_status(202, "{\"installing\": true}".to_string())
            }
            Err(why) => Response::error(503, &format!("the updater could not be started: {why}")),
        }
    }
}
