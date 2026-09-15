//! The `lcl-workspace` binary.
//!
//! Opens a project, binds loopback, prints the URL and serves until stopped.
//!
//! ## Why it prints a URL instead of opening a browser
//!
//! The URL carries the session token, and the token is the thing that decides
//! who may read and write files in the project. Handing it to whatever program
//! happens to be registered for `http` is a decision about trust, so the
//! default is to print it and let the operator open it. `--open` asks for the
//! other behaviour explicitly.

use lcl_workspace::{Routes, Server, Workspace};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "\
lcl-workspace — the LCL editor, inspector and debugger

USAGE:
    lcl-workspace [PROJECT] [OPTIONS]

ARGUMENTS:
    PROJECT             The project directory. Defaults to the working directory.

OPTIONS:
    --document <PATH>   Open one document. Its project is the nearest ancestor
                        holding lcl.project.json, or its own directory. This is
                        what a desktop file association passes; PROJECT is not.
    --spec <PATH>       The canonical specification package. Falls back to
                        LCL_SPEC, then to \"spec\" in lcl.project.json.
    --localized-spec <PATH>
                        The canonical LCL Core 0.2.0 package, for localized
                        documents. Falls back to LCL_LOCALIZED_SPEC, then to
                        \"localized_spec\" in lcl.project.json.
    --profile <FILE>    A locale profile <locale>.json for localized documents,
                        after the project's \"profiles\" directory. Repeatable;
                        a later file for the same locale replaces an earlier one.
    --create            Create a project here before opening it.
    --port <PORT>       Bind this loopback port instead of an ephemeral one.
    --open              Open the URL with xdg-open once the server is listening.
    --log               Print one line per request. The token is never printed.
    -h, --help          Print this message.

The server binds 127.0.0.1 only and requires the session token in the printed
URL. It grants the host no capability; a run asks before every effect.
";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(detail) => {
            eprintln!("lcl-workspace: {detail}");
            ExitCode::FAILURE
        }
    }
}

fn run(argv: &[String]) -> Result<(), String> {
    let mut root: Option<PathBuf> = None;
    let mut document: Option<PathBuf> = None;
    let mut spec: Option<PathBuf> = None;
    let mut localized_spec: Option<PathBuf> = None;
    let mut profiles: Vec<PathBuf> = Vec::new();
    let mut port: u16 = 0;
    let mut create = false;
    let mut open = false;
    let mut log = false;

    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            i += 1;
            argv.get(i)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match arg {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--document" => document = Some(PathBuf::from(value("--document")?)),
            "--spec" => spec = Some(PathBuf::from(value("--spec")?)),
            "--localized-spec" => localized_spec = Some(PathBuf::from(value("--localized-spec")?)),
            "--profile" => profiles.push(PathBuf::from(value("--profile")?)),
            "--port" => {
                port = value("--port")?
                    .parse()
                    .map_err(|_| "--port needs a number".to_string())?
            }
            "--create" => create = true,
            "--open" => open = true,
            "--log" => log = true,
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => {
                if root.is_some() {
                    return Err("only one project directory may be given".to_string());
                }
                root = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    // A document names its own project, so the two cannot both be given: one
    // of them would have to be ignored, and silently ignoring the operator's
    // argument is how a launcher opens the wrong thing.
    if document.is_some() && root.is_some() {
        return Err("pass either a project directory or --document, not both".to_string());
    }
    let (root, open_document) = match &document {
        Some(path) => {
            let (root, id) = Workspace::locate_document(path).map_err(|e| e.to_string())?;
            (root, Some(id))
        }
        None => {
            let root = match root {
                Some(root) => root,
                None => {
                    std::env::current_dir().map_err(|e| format!("no working directory: {e}"))?
                }
            };
            (root, None)
        }
    };
    let spec = Workspace::locate_spec(&root, spec).map_err(|e| e.to_string())?;

    if create {
        Workspace::create(&root, &spec).map_err(|e| e.to_string())?;
    }
    let localized_spec = Workspace::locate_localized_spec(localized_spec);
    let workspace = Workspace::open_with_profiles(&root, &spec, localized_spec, &profiles)
        .map_err(|e| e.to_string())?
        .with_open_document(open_document);

    let server = Server::bind_to(port)
        .map_err(|e| format!("could not bind loopback: {e}"))?
        .logging(log);
    let url = server.url();
    println!("LCL Workspace");
    println!("  project  {}", workspace.root().display());
    if let Some(id) = workspace.open_document() {
        println!("  document {id}");
    }
    println!("  spec     {}", workspace.spec_root().display());
    if let Some(localized) = workspace.localized_spec_root() {
        println!("  localized {}", localized.display());
    }
    println!("  open     {url}");
    println!();
    println!("The token in that URL is what authorises access. Do not share it.");

    if open {
        let _ = std::process::Command::new("xdg-open").arg(&url).status();
    }

    let routes = Routes::new(Arc::new(workspace));
    server
        .serve(Arc::new(routes))
        .map_err(|e| format!("the server stopped: {e}"))
}
