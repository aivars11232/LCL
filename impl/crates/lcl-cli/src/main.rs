//! `lcl` — the headless LCL command-line interface.
//!
//! ## What this binary is, and is not
//!
//! It is a caller of [`lcl_protocol::Engine`]. It parses a command line, finds
//! a specification package and a project, hands the engine a document and an
//! explicit host, and renders what comes back. The implementation contract
//! draws the line it stays behind: "CLI and UI consume engine APIs/protocols.
//! Neither may contain a second private implementation of language semantics."
//!
//! So there is no rule here about what a valid document is, what a diagnostic
//! means, which stage owns an identifier, or when an invocation succeeds. Every
//! one of those comes back inside a [`lcl_protocol::Report`], and `--machine`
//! prints the same facts a Rust consumer would read from the same struct.
//!
//! ## Nothing is discovered
//!
//! The specification package comes from `--spec`, then `LCL_SPEC`, then the
//! project manifest — and if none of those names one, the command stops rather
//! than looking for one. The project root is the directory a caller named, or
//! the document's own directory; no parent is searched. `05_SEMANTICS/02` is
//! the reason: "Ambient current directory and implied nearby files do not exist
//! in portable LCL", and a tool that guessed either would put the ambience back.
//!
//! ## A run is granted nothing by default
//!
//! `HostAdapter` starts with the engine's internal stores and no filesystem, no
//! process runner and no transport. Each `--allow-*` flag adds exactly what it
//! names. An operation that needs a capability nobody granted reports
//! `error.host.constraint`, which the capability contract calls a host
//! limitation that "never changes LCL meaning".

mod args;
mod exit;
mod render;
mod syntax;

use args::{Command, Common, Document, UsageError};
use lcl_localization::{LocaleTag, Pin};
use lcl_project::lock::LockedLocale;
use lcl_project::{Cache, Lock, Project};
use lcl_protocol::json::{Node, Object};
use lcl_protocol::{Engine, Engines, Inputs, Report};
use lcl_resolver::SourceId;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&argv) {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("lcl: {}", failure.detail);
            failure.code
        }
    };
    ExitCode::from(code as u8)
}

/// Anything that stops a command before it can produce a report.
struct Failure {
    detail: String,
    code: i32,
}

impl Failure {
    fn usage(detail: impl Into<String>) -> Failure {
        Failure {
            detail: detail.into(),
            code: exit::USAGE,
        }
    }

    fn environment(detail: impl Into<String>) -> Failure {
        Failure {
            detail: detail.into(),
            code: exit::ENVIRONMENT,
        }
    }
}

impl From<UsageError> for Failure {
    fn from(error: UsageError) -> Failure {
        Failure::usage(error.detail)
    }
}

fn run(argv: &[String]) -> Result<i32, Failure> {
    match args::parse(argv)? {
        Command::Help => {
            print!("{}", args::usage());
            Ok(exit::SUCCESS)
        }
        Command::Version(common) => {
            // The languages a command's engines judge documents under: Core
            // 0.1.0 always, and Core 0.2.0 only when a package named here opens
            // as the approved Core 0.2.0 package. One that does not open is
            // refused before anything is printed.
            let localized = localized_spec_root(&common, None)
                .map(|root| open_localized(&root, &[]))
                .transpose()?;
            println!("lcl {}", env!("CARGO_PKG_VERSION"));
            println!("protocol {}", lcl_protocol::PROTOCOL);
            println!("language 0.1.0");
            if localized.is_some() {
                println!("language 0.2.0");
            }
            Ok(exit::SUCCESS)
        }
        Command::Check(document) => stage_command(document, lcl_protocol::Command::Check),
        Command::Validate(document) => stage_command(document, lcl_protocol::Command::Validate),
        Command::Inspect(document) => stage_command(document, lcl_protocol::Command::Inspect),
        Command::Run(document) => stage_command(document, lcl_protocol::Command::Run),
        Command::Lock(document) => lock_command(document, true),
        Command::Verify(document) => lock_command(document, false),
        Command::Vendor { uri, file, common } => vendor(&uri, &file, &common),
        Command::Cache(common) => cache_list(&common),
        Command::Spec(common) => spec_command(&common),
        Command::Syntax(common) => syntax_command(&common),
    }
}

// ---------------------------------------------------------------------------
// Setting up: specification, project, document
// ---------------------------------------------------------------------------

/// Where the canonical package is, in the one order this tool accepts.
///
/// Each source is explicit. The environment variable is not ambience: it is a
/// value an operator set on purpose, and the resolved path and its identity
/// digest appear in every report, so a result can always be attributed.
fn spec_root(common: &Common, project: Option<&Project>) -> Result<PathBuf, Failure> {
    if let Some(path) = &common.spec {
        return Ok(path.clone());
    }
    if let Some(value) = std::env::var_os("LCL_SPEC") {
        if !value.is_empty() {
            return Ok(PathBuf::from(value));
        }
    }
    if let Some(path) = project.and_then(Project::spec_path) {
        return Ok(path);
    }
    Err(Failure::environment(
        "no specification package: pass --spec <path>, set LCL_SPEC, or declare \
         \"spec\" in lcl.project.json",
    ))
}

/// Where the canonical LCL Core 0.2.0 package is, when one is named.
///
/// The same explicit order as [`spec_root`]: `--localized-spec`, then
/// `LCL_LOCALIZED_SPEC`, then the manifest's `localized_spec`. Naming none is
/// not an error: every document is then judged by Core 0.1.0.
fn localized_spec_root(common: &Common, project: Option<&Project>) -> Option<PathBuf> {
    if let Some(path) = &common.localized_spec {
        return Some(path.clone());
    }
    if let Some(value) = std::env::var_os("LCL_LOCALIZED_SPEC") {
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    project.and_then(Project::localized_spec_path)
}

/// The engines a command judges documents with.
///
/// Core 0.1.0 always, and the Core 0.2.0 engine with its localization stage
/// only when a 0.2.0 package is named. Its locale profiles are the project's
/// `profiles` directory and then every `--profile` file, a later file for the
/// same locale replacing an earlier one. Under `--locked`, the lock file's
/// locale pins apply.
fn engines(common: &Common, project: Option<&Project>) -> Result<Engines, Failure> {
    let failed = |e: &dyn std::fmt::Display| Failure::environment(e.to_string());
    let core = Engine::open(spec_root(common, project)?).map_err(|e| failed(&e))?;
    let Some(root) = localized_spec_root(common, project) else {
        return Engines::new(core, None).map_err(|e| failed(&e));
    };
    let mut files = match project {
        Some(project) => project
            .profile_files()
            .map_err(|e| Failure::environment(format!("the locale profile directory: {e}")))?,
        None => Vec::new(),
    };
    files.extend(common.profiles.iter().cloned());
    let mut localized = open_localized(&root, &files)?;
    if common.locked {
        if let Some(lock) = project.and_then(|p| Lock::read(p.lock_path()).ok()) {
            let pins: BTreeMap<SourceId, Pin> = lock
                .locales
                .iter()
                .filter_map(|(unit, pinned)| {
                    Some((
                        SourceId::new(unit.clone()),
                        Pin {
                            locale: LocaleTag::parse(&pinned.locale).ok()?,
                            identity: pinned.profile_identity.clone(),
                        },
                    ))
                })
                .collect();
            localized = localized.with_locale_pins(pins);
        }
    }
    Engines::new(core, Some(localized)).map_err(|e| failed(&e))
}

/// Open the Core 0.2.0 package at `root` with its localization stage.
fn open_localized(root: &Path, profiles: &[PathBuf]) -> Result<Engine, Failure> {
    Engine::open_localized(root, profiles).map_err(|e| {
        Failure::environment(format!(
            "the localized specification package {}: {e}",
            root.display()
        ))
    })
}

/// Open the project a command acts within.
///
/// `--project` names it outright. Otherwise it is the document's own directory:
/// exactly one directory, examined once, with no walk upward. A project root
/// found by searching parent directories would be an implied nearby file by
/// another name.
fn open_project(common: &Common, document: Option<&Path>) -> Result<Project, Failure> {
    let root = match (&common.project, document) {
        (Some(root), _) => root.clone(),
        (None, Some(path)) => path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        (None, None) => PathBuf::from("."),
    };
    if root.join(lcl_project::MANIFEST_FILE).is_file() {
        Project::open(&root).map_err(|e| Failure::environment(e.to_string()))
    } else {
        Project::rootless(&root).map_err(|e| Failure::environment(e.to_string()))
    }
}

/// The document a command acts on: the one named, or the manifest's entry.
fn entry(document: &Document, project: &Project) -> Result<PathBuf, Failure> {
    match &document.path {
        Some(path) => Ok(path.clone()),
        None => project.entry_path().ok_or_else(|| {
            Failure::usage("no document: name one, or declare \"entry\" in lcl.project.json")
        }),
    }
}

// ---------------------------------------------------------------------------
// The four staged commands
// ---------------------------------------------------------------------------

fn stage_command(document: Document, command: lcl_protocol::Command) -> Result<i32, Failure> {
    let common = document.common.clone();
    let project = open_project(&common, document.path.as_deref())?;
    let engines = engines(&common, Some(&project))?;

    let path = entry(&document, &project)?;
    let provider = project
        .provider()
        .map_err(|e| Failure::environment(e.to_string()))?;
    let unit = provider
        .root_unit(&path)
        .map_err(|e| Failure::environment(e.to_string()))?;
    let engine = engines.engine_for(&unit);

    let mut inputs = Inputs::new();
    for (id, expression) in &common.inputs {
        inputs = inputs.with_text(id, expression);
    }

    let report = match command {
        lcl_protocol::Command::Check => engine.check(&unit, &provider),
        lcl_protocol::Command::Validate => engine.validate(&unit, &provider, &inputs),
        lcl_protocol::Command::Inspect => engine.inspect(&unit, &provider, &inputs),
        lcl_protocol::Command::Run => {
            let (mut stdlib, mut host) = lcl_protocol::surface(engine, &common.grants)
                .map_err(|e| Failure::environment(e.to_string()))?;
            engine.run(&unit, &provider, &inputs, &mut stdlib, &mut host)
        }
    };

    if common.locked {
        if let Some(drift) = locked_drift(&project, &report)? {
            return Err(Failure::environment(drift));
        }
    }

    emit(&report, &common);
    Ok(exit::of(&report))
}

// ---------------------------------------------------------------------------
// Packages
// ---------------------------------------------------------------------------

/// The lock a report's own units describe.
fn lock_of(report: &Report) -> Option<Lock> {
    let locales = report.units.iter().filter_map(|unit| {
        let selected = unit.locale.as_ref()?;
        Some((
            unit.id.clone(),
            LockedLocale {
                locale: selected.locale.clone()?,
                method: selected.method.clone(),
                profile_identity: selected.profile_identity.clone()?,
            },
        ))
    });
    Some(
        Lock::new(
            report.spec.identity_digest.clone(),
            report.spec.formal_version.clone(),
            report.root()?.to_string(),
            report
                .units
                .iter()
                .map(|unit| (unit.id.clone(), unit.digest.clone())),
        )
        .with_locales(locales),
    )
}

/// Under `--locked`, the drift that must stop the command.
fn locked_drift(project: &Project, report: &Report) -> Result<Option<String>, Failure> {
    let path = project.lock_path();
    if !path.is_file() {
        return Ok(Some(format!(
            "--locked was given but {} does not exist; run `lcl package lock` first",
            path.display()
        )));
    }
    let locked = Lock::read(&path).map_err(|e| Failure::environment(e.to_string()))?;
    let Some(actual) = lock_of(report) else {
        return Ok(None);
    };
    let drift = locked.drift(&actual);
    if drift.is_empty() {
        return Ok(None);
    }
    let mut detail = format!("{} does not describe what was loaded:", path.display());
    for entry in drift {
        detail.push_str(&format!("\n  {entry}"));
    }
    Ok(Some(detail))
}

/// `package lock` writes; `package verify` compares.
fn lock_command(document: Document, write: bool) -> Result<i32, Failure> {
    let common = document.common.clone();
    let project = open_project(&common, document.path.as_deref())?;
    let engines = engines(&common, Some(&project))?;
    let path = entry(&document, &project)?;
    let provider = project
        .provider()
        .map_err(|e| Failure::environment(e.to_string()))?;
    let unit = provider
        .root_unit(&path)
        .map_err(|e| Failure::environment(e.to_string()))?;
    let engine = engines.engine_for(&unit);

    // Locking records what a resolution actually loaded, so it runs one. Steps
    // 1 to 5 load every unit an import names, and nothing further is needed to
    // know the project's inputs.
    let report = engine.check(&unit, &provider);
    let Some(actual) = lock_of(&report) else {
        return Err(Failure::environment(
            "the document produced no source identity to lock",
        ));
    };

    let lock_path = project.lock_path();
    if write {
        actual
            .write(&lock_path)
            .map_err(|e| Failure::environment(e.to_string()))?;
        if common.machine {
            print!("{}", lock_json(&lock_path, &actual, &[]).pretty());
        } else {
            println!("wrote {}", lock_path.display());
            println!("  spec {} ({})", actual.spec_version, actual.spec_identity);
            for (unit, digest) in &actual.units {
                println!("  {digest}  {unit}");
            }
        }
        // A lock written from a rejected document still records the bytes that
        // were read, which is what a lock file is for. The document's own
        // verdict is reported through the same exit code as `check`.
        return Ok(exit::of(&report));
    }

    if !lock_path.is_file() {
        return Err(Failure::environment(format!(
            "{} does not exist; run `lcl package lock` first",
            lock_path.display()
        )));
    }
    let locked = Lock::read(&lock_path).map_err(|e| Failure::environment(e.to_string()))?;
    let drift = locked.drift(&actual);
    if common.machine {
        let rendered: Vec<String> = drift.iter().map(|d| d.to_string()).collect();
        print!("{}", lock_json(&lock_path, &actual, &rendered).pretty());
    } else if drift.is_empty() {
        println!("{} matches what was loaded", lock_path.display());
    } else {
        println!("{} does not describe what was loaded:", lock_path.display());
        for entry in &drift {
            println!("  {entry}");
        }
    }
    if drift.is_empty() {
        Ok(exit::of(&report))
    } else {
        Ok(exit::ENVIRONMENT)
    }
}

fn lock_json(path: &Path, lock: &Lock, drift: &[String]) -> Node {
    Object::new()
        .with("protocol", Node::string(lcl_protocol::PROTOCOL))
        .with("command", Node::string("package"))
        .with("lock", Node::string(path.display().to_string()))
        .with("spec_version", Node::string(&lock.spec_version))
        .with("spec_identity", Node::string(&lock.spec_identity))
        .with("root", Node::string(&lock.root))
        .with(
            "units",
            Node::array(lock.units.iter().map(|(id, digest)| {
                Object::new()
                    .with("id", Node::string(id))
                    .with("digest", Node::string(format!("sha256:{digest}")))
                    .into()
            })),
        )
        .with("drift", Node::array(drift.iter().map(Node::string)))
        .into()
}

/// Put one local file into the cache under the URI a document imports it by.
///
/// This is the whole of "acquiring" a package here. Nothing fetches: the
/// architecture contract rules out resolver-owned web browsing, and a tool that
/// downloaded on demand would make a document's meaning depend on when it was
/// resolved. An operator obtains the bytes however they like and vendors them
/// once; the importing document's `CHECKSUM` still decides whether the language
/// accepts them.
fn vendor(uri: &str, file: &Path, common: &Common) -> Result<i32, Failure> {
    let project = open_project(common, None)?;
    let dir = project
        .cache_path()
        .ok_or_else(|| Failure::usage("no package cache: declare \"cache\" in lcl.project.json"))?;
    let bytes = std::fs::read(file)
        .map_err(|e| Failure::environment(format!("{}: not readable: {e}", file.display())))?;
    let mut cache = Cache::open(&dir).map_err(|e| Failure::environment(e.to_string()))?;
    let digest = cache
        .put(uri, &bytes)
        .map_err(|e| Failure::environment(e.to_string()))?;

    if common.machine {
        print!(
            "{}",
            Object::new()
                .with("protocol", Node::string(lcl_protocol::PROTOCOL))
                .with("command", Node::string("package"))
                .with("cache", Node::string(dir.display().to_string()))
                .with("uri", Node::string(uri))
                .with("digest", Node::string(format!("sha256:{digest}")))
                .with("bytes", Node::usize(bytes.len()))
                .pretty()
        );
    } else {
        println!("cached {uri}");
        println!("  sha256:{digest}");
        println!("  declare CHECKSUM: \"sha256:{digest}\" on the importing block");
    }
    Ok(exit::SUCCESS)
}

fn cache_list(common: &Common) -> Result<i32, Failure> {
    let project = open_project(common, None)?;
    let dir = project
        .cache_path()
        .ok_or_else(|| Failure::usage("no package cache: declare \"cache\" in lcl.project.json"))?;
    let cache = Cache::open(&dir).map_err(|e| Failure::environment(e.to_string()))?;
    let faults = cache.verify();

    if common.machine {
        print!(
            "{}",
            Object::new()
                .with("protocol", Node::string(lcl_protocol::PROTOCOL))
                .with("command", Node::string("package"))
                .with("cache", Node::string(dir.display().to_string()))
                .with(
                    "entries",
                    Node::array(cache.entries().map(|(uri, digest)| {
                        Object::new()
                            .with("uri", Node::string(uri))
                            .with("digest", Node::string(format!("sha256:{digest}")))
                            .into()
                    })),
                )
                .with(
                    "faults",
                    Node::array(faults.iter().map(|f| Node::string(f.to_string()))),
                )
                .pretty()
        );
    } else {
        println!("{}", dir.display());
        for (uri, digest) in cache.entries() {
            println!("  sha256:{digest}  {uri}");
        }
        for fault in &faults {
            println!("  fault: {fault}");
        }
    }
    if faults.is_empty() {
        Ok(exit::SUCCESS)
    } else {
        Ok(exit::ENVIRONMENT)
    }
}

// ---------------------------------------------------------------------------
// Specification and syntax
// ---------------------------------------------------------------------------

fn spec_command(common: &Common) -> Result<i32, Failure> {
    let project = open_project(common, None).ok();
    let root = spec_root(common, project.as_ref())?;
    let engine = Engine::open(&root).map_err(|e| Failure::environment(e.to_string()))?;
    let record = engine.spec_record();

    if common.machine {
        print!(
            "{}",
            Object::new()
                .with("protocol", Node::string(lcl_protocol::PROTOCOL))
                .with("command", Node::string("spec"))
                .with("root", Node::string(&record.root))
                .with("formal_version", Node::string(&record.formal_version))
                .with("identity_digest", Node::string(&record.identity_digest))
                .with("authority", Node::string(&record.authority))
                .pretty()
        );
    } else {
        println!("{}", record.root);
        println!("  version   {}", record.formal_version);
        println!("  identity  {}", record.identity_digest);
        println!("  authority {}", record.authority);
    }
    Ok(exit::SUCCESS)
}

fn syntax_command(common: &Common) -> Result<i32, Failure> {
    let project = open_project(common, None).ok();
    let root = spec_root(common, project.as_ref())?;
    let engine = Engine::open(&root).map_err(|e| Failure::environment(e.to_string()))?;
    let metadata = syntax::metadata(engine.lexicon());

    if common.machine {
        print!("{}", metadata.to_json().pretty());
    } else {
        print!("{}", metadata.render());
    }
    Ok(exit::SUCCESS)
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// One report, in whichever form was asked for.
///
/// Both forms carry the same facts. `--machine` adds byte spans, registry
/// ranks, emission identities and the full execution record; the human form
/// orders and abbreviates. Neither computes anything the other does not have.
fn emit(report: &Report, common: &Common) {
    if common.machine {
        print!("{}", report.to_json().pretty());
    } else {
        print!("{}", render::report(report));
    }
}
