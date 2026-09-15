//! Command-line parsing.
//!
//! ## Why this is written out rather than pulled in
//!
//! `impl/Cargo.toml` states the workspace's dependency policy: std only,
//! because "the spec authority loader is the trust root for every later layer,
//! so it carries no third-party supply-chain surface". A CLI that added an
//! argument-parsing crate would put a dependency in front of the binary that
//! reads the trust root. The parsing this tool needs is a few hundred lines,
//! and it is here.
//!
//! ## What the parser refuses
//!
//! An unknown option, a missing value, a repeated single-valued option, and a
//! command it does not know. Every refusal names what was wrong. Nothing is
//! guessed: there is no abbreviation matching, no "did you mean", and no
//! positional argument that silently becomes an option's value.

use std::fmt;
use std::path::PathBuf;

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Steps 1 to 5 over one document.
    Check(Document),
    /// Steps 1 to 9 over one document.
    Validate(Document),
    /// Steps 1 to 13 over one document.
    Run(Document),
    /// Steps 1 to 9 over one document, reported structurally.
    Inspect(Document),
    /// Write the lock file from an actual resolution.
    Lock(Document),
    /// Compare the lock file against an actual resolution.
    Verify(Document),
    /// Put a local file into the package cache under a URI.
    Vendor {
        uri: String,
        file: PathBuf,
        common: Common,
    },
    /// List what the package cache holds.
    Cache(Common),
    /// Report the specification package's identity.
    Spec(Common),
    /// Emit registry-derived syntax metadata for editors and tooling.
    Syntax(Common),
    /// Print usage.
    Help,
    /// Print the tool, protocol and language versions.
    Version(Common),
}

/// One document command's subject and options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The document to act on. `None` uses the project manifest's entry.
    pub path: Option<PathBuf>,
    pub common: Common,
}

/// Options every command shares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Common {
    /// The canonical specification package root.
    pub spec: Option<PathBuf>,
    /// The canonical LCL Core 0.2.0 package root, whose localization stage
    /// judges localized documents. Absent: every document is Core 0.1.0's.
    pub localized_spec: Option<PathBuf>,
    /// Locale profile files, in the order given.
    pub profiles: Vec<PathBuf>,
    /// The project root.
    pub project: Option<PathBuf>,
    /// Emit the machine-readable record instead of human text.
    pub machine: bool,
    /// Supplied invocation data, in the order given.
    pub inputs: Vec<(String, String)>,
    /// Refuse to proceed when the lock file disagrees with what was loaded.
    pub locked: bool,
    /// Host capabilities the caller explicitly granted.
    pub grants: Grants,
}

/// What a run is allowed to touch, exactly as the caller stated it.
///
/// This is `lcl-protocol`'s type, not one of the CLI's own. The workspace UI
/// grants capabilities through the same struct and assembles its host through
/// the same function, which is what makes "the UI matches the CLI for identical
/// capabilities" a property of the code rather than of two authors agreeing.
pub use lcl_protocol::Granted as Grants;

/// Why a command line could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    pub detail: String,
}

impl UsageError {
    fn new(detail: impl Into<String>) -> UsageError {
        UsageError {
            detail: detail.into(),
        }
    }
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for UsageError {}

/// Parse one command line, excluding the program name.
pub fn parse(argv: &[String]) -> Result<Command, UsageError> {
    let Some(first) = argv.first() else {
        return Ok(Command::Help);
    };
    let rest = &argv[1..];
    match first.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "version" | "--version" => Ok(Command::Version(only_options(rest)?)),
        "check" => Ok(Command::Check(document(rest)?)),
        "validate" => Ok(Command::Validate(document(rest)?)),
        "run" => Ok(Command::Run(document(rest)?)),
        "inspect" => Ok(Command::Inspect(document(rest)?)),
        "spec" => Ok(Command::Spec(only_options(rest)?)),
        "syntax" => Ok(Command::Syntax(only_options(rest)?)),
        "package" => package(rest),
        other if other.starts_with('-') => Err(UsageError::new(format!(
            "{other} is not a command; run `lcl help`"
        ))),
        other => Err(UsageError::new(format!(
            "unknown command {other:?}; run `lcl help`"
        ))),
    }
}

fn package(argv: &[String]) -> Result<Command, UsageError> {
    let Some(first) = argv.first() else {
        return Err(UsageError::new(
            "`lcl package` needs a subcommand: lock, verify, vendor or list",
        ));
    };
    let rest = &argv[1..];
    match first.as_str() {
        "lock" => Ok(Command::Lock(document(rest)?)),
        "verify" => Ok(Command::Verify(document(rest)?)),
        "list" => Ok(Command::Cache(only_options(rest)?)),
        "vendor" => {
            let (positional, common) = split(rest)?;
            match positional.len() {
                2 => Ok(Command::Vendor {
                    uri: positional[0].clone(),
                    file: PathBuf::from(&positional[1]),
                    common,
                }),
                _ => Err(UsageError::new(
                    "`lcl package vendor` needs exactly two arguments: <uri> <file>",
                )),
            }
        }
        other => Err(UsageError::new(format!(
            "unknown package subcommand {other:?}; expected lock, verify, vendor or list"
        ))),
    }
}

/// A command taking at most one document path.
fn document(argv: &[String]) -> Result<Document, UsageError> {
    let (positional, common) = split(argv)?;
    match positional.len() {
        0 => Ok(Document { path: None, common }),
        1 => Ok(Document {
            path: Some(PathBuf::from(&positional[0])),
            common,
        }),
        _ => Err(UsageError::new(format!(
            "expected at most one document, got {}: {}",
            positional.len(),
            positional.join(", ")
        ))),
    }
}

/// A command taking no positional argument.
fn only_options(argv: &[String]) -> Result<Common, UsageError> {
    let (positional, common) = split(argv)?;
    if let Some(extra) = positional.first() {
        return Err(UsageError::new(format!(
            "this command takes no argument, got {extra:?}"
        )));
    }
    Ok(common)
}

/// Split options from positional arguments.
fn split(argv: &[String]) -> Result<(Vec<String>, Common), UsageError> {
    let mut positional = Vec::new();
    let mut common = Common::default();
    let mut index = 0usize;
    let mut options_ended = false;

    while index < argv.len() {
        let argument = &argv[index];
        index += 1;

        if options_ended || !argument.starts_with("--") {
            if argument.starts_with('-') && argument.len() > 1 && !options_ended {
                return Err(UsageError::new(format!(
                    "unknown option {argument}; options are spelled --like-this"
                )));
            }
            positional.push(argument.clone());
            continue;
        }
        if argument == "--" {
            options_ended = true;
            continue;
        }

        // `--name value` and `--name=value` are the same option.
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (argument.as_str(), None),
        };

        let mut value = |option: &str| -> Result<String, UsageError> {
            if let Some(inline) = inline.clone() {
                return Ok(inline);
            }
            let next = argv
                .get(index)
                .cloned()
                .ok_or_else(|| UsageError::new(format!("{option} needs a value")))?;
            index += 1;
            Ok(next)
        };

        match name {
            "--machine" => {
                if inline.is_some() {
                    return Err(UsageError::new("--machine takes no value"));
                }
                common.machine = true;
            }
            "--locked" => {
                if inline.is_some() {
                    return Err(UsageError::new("--locked takes no value"));
                }
                common.locked = true;
            }
            "--spec" => {
                let path = value("--spec")?;
                if common.spec.is_some() {
                    return Err(UsageError::new("--spec was given more than once"));
                }
                common.spec = Some(PathBuf::from(path));
            }
            "--localized-spec" => {
                let path = value("--localized-spec")?;
                if common.localized_spec.is_some() {
                    return Err(UsageError::new("--localized-spec was given more than once"));
                }
                common.localized_spec = Some(PathBuf::from(path));
            }
            "--profile" => common.profiles.push(PathBuf::from(value("--profile")?)),
            "--project" => {
                let path = value("--project")?;
                if common.project.is_some() {
                    return Err(UsageError::new("--project was given more than once"));
                }
                common.project = Some(PathBuf::from(path));
            }
            "--input" => {
                let pair = value("--input")?;
                let Some((id, expression)) = pair.split_once('=') else {
                    return Err(UsageError::new(format!(
                        "--input needs <id>=<expression>, got {pair:?}"
                    )));
                };
                if id.is_empty() {
                    return Err(UsageError::new("--input names no declaration"));
                }
                common.inputs.push((id.to_string(), expression.to_string()));
            }
            "--allow-read" => common
                .grants
                .read
                .push(PathBuf::from(value("--allow-read")?)),
            "--allow-write" => common
                .grants
                .write
                .push(PathBuf::from(value("--allow-write")?)),
            "--allow-run" => common.grants.programs.push(value("--allow-run")?),
            "--allow-net" => common.grants.hosts.push(value("--allow-net")?),
            other => {
                return Err(UsageError::new(format!(
                    "unknown option {other}; run `lcl help`"
                )))
            }
        }
    }

    Ok((positional, common))
}

/// The usage text.
pub fn usage() -> String {
    let mut out = String::new();
    out.push_str("lcl — check, validate, run and inspect LCL Core 0.1.0 documents\n\n");
    out.push_str("USAGE\n    lcl <command> [options] [document]\n\n");
    out.push_str("COMMANDS\n");
    for (name, description) in [
        (
            "check",
            "canonical steps 1-5: lexical, grammar, resolution, static checking",
        ),
        (
            "validate",
            "steps 1-9: everything check does, plus no-effect semantic preflight",
        ),
        (
            "run",
            "steps 1-13: execution, verification, evidence and one terminal status",
        ),
        (
            "inspect",
            "steps 1-9, reported as units, imports, declarations and the ordered plan",
        ),
        (
            "package lock",
            "write the lock file from an actual resolution",
        ),
        (
            "package verify",
            "compare the lock file against an actual resolution",
        ),
        (
            "package vendor",
            "put a local file into the package cache under a URI",
        ),
        ("package list", "list what the package cache holds"),
        (
            "spec",
            "report the specification package's identity and authority",
        ),
        (
            "syntax",
            "emit registry-derived syntax metadata for editors and tooling",
        ),
        ("version", "print the tool, protocol and language versions"),
        ("help", "print this text"),
    ] {
        out.push_str(&format!("    {name:<16}{description}\n"));
    }
    out.push_str("\nOPTIONS\n");
    for (name, description) in [
        ("--spec <path>", "the canonical specification package root"),
        (
            "--localized-spec <path>",
            "the canonical LCL Core 0.2.0 package, for localized documents",
        ),
        (
            "--profile <file>",
            "a locale profile <locale>.json (repeatable)",
        ),
        (
            "--project <dir>",
            "the project root; defaults to the document's own directory",
        ),
        (
            "--input <id>=<expr>",
            "supply one declared INPUT, STATE, MEMORY or CONTEXT",
        ),
        (
            "--machine",
            "emit the machine-readable JSON record instead of human text",
        ),
        ("--locked", "refuse to proceed when the lock file disagrees"),
        (
            "--allow-read <path>",
            "grant the host read access to a path (run only)",
        ),
        (
            "--allow-write <path>",
            "grant the host write access to a path (run only)",
        ),
        (
            "--allow-run <program>",
            "grant the host permission to run a program (run only)",
        ),
        (
            "--allow-net <host>",
            "grant the host network access to a host (run only)",
        ),
    ] {
        out.push_str(&format!("    {name:<24}{description}\n"));
    }
    out.push_str("\nThe specification package is found from --spec, then the LCL_SPEC\n");
    out.push_str("environment variable, then the project manifest's \"spec\". It is never\n");
    out.push_str("searched for: an engine that guessed its own authority could not be\n");
    out.push_str("reproducible.\n");
    out.push_str("\nA run grants the host nothing by default. An operation needing a\n");
    out.push_str("capability that was not granted reports error.host.constraint, which is\n");
    out.push_str("the truthful outcome rather than a degraded one.\n");
    out.push_str("\nEXIT CODES\n");
    for (code, description) in crate::exit::TABLE {
        out.push_str(&format!("    {code}    {description}\n"));
    }
    out
}
