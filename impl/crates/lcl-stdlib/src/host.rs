//! The host half of the boundary: what the machine will do, and what happened.
//!
//! ## What this type may decide, and what it may not
//!
//! [`HostAdapter`] answers two questions and no others: *do I permit this?* and
//! *what happened when I did it?* It never selects an LCL status, an LCL error
//! identifier or a failure phase, because
//! [`lcl_runtime::capability::CapabilityOutcome`] has no field to put one in —
//! the runtime chooses those from what the adapter observed.
//!
//! ## Permission is derived from the request's resolved axes
//!
//! The standard library has already resolved this invocation's *actual*
//! dependency and effect sets from its target and destination address classes,
//! so the adapter asks for exactly the grants those sets imply: a resolved
//! `filesystem` effect needs a granted writable scope over the target, a
//! resolved `host` dependency with no effect needs a readable one, and a row
//! that resolved neither needs nothing. An adapter that re-derived permission
//! from the operation *name* would be guessing where the axes already answered.
//!
//! ## An absent capability is a limitation, not a refusal
//!
//! A request for a capability this adapter was never given — no filesystem, no
//! process runner, no transport — is [`Permission::Unavailable`], which the
//! runtime turns into `error.host.constraint`: "Host limitations produce
//! error.host.constraint and never change LCL meaning." It is not
//! `Permission::Denied`, which would say the operator refused something they
//! were never asked about.

use crate::schema;
use lcl_capabilities::fs::{FileSystem, FsError, Location, WriteMode};
use lcl_capabilities::net::{Address, NetError, Transport};
use lcl_capabilities::process::{Command, Process, ProcessError, ProcessFailure, Responder};
use lcl_capabilities::{Bounds, Dependency, Effect, Grant, Grants, Refusal};
use lcl_runtime::capability::{
    CapabilityOutcome, CapabilityRequest, Host, Observation, Permission,
};
use lcl_runtime::result::{EffectClass, ObservedEffect, RecordState};
use lcl_runtime::{strict_equal, Value};
use std::path::PathBuf;

/// A host built from explicit grants and explicitly installed capabilities.
pub struct HostAdapter {
    grants: Grants,
    bounds: Bounds,
    filesystem: Option<Box<dyn FileSystem>>,
    process: Option<Box<dyn Process>>,
    transport: Option<Box<dyn Transport>>,
    responder: Option<Box<dyn Responder>>,
    log: Vec<CapabilityRequest>,
    delays: Vec<Value>,
}

impl HostAdapter {
    /// A host that permits what `grants` permits and can do nothing else.
    pub fn new(grants: Grants) -> HostAdapter {
        HostAdapter {
            grants,
            bounds: Bounds::new(),
            filesystem: None,
            process: None,
            transport: None,
            responder: None,
            log: Vec::new(),
            delays: Vec::new(),
        }
    }

    /// Install a filesystem capability.
    pub fn with_filesystem(mut self, filesystem: impl FileSystem + 'static) -> HostAdapter {
        self.filesystem = Some(Box::new(filesystem));
        self
    }

    /// Install a process capability.
    pub fn with_process(mut self, process: impl Process + 'static) -> HostAdapter {
        self.process = Some(Box::new(process));
        self
    }

    /// Install a network transport.
    pub fn with_transport(mut self, transport: impl Transport + 'static) -> HostAdapter {
        self.transport = Some(Box::new(transport));
        self
    }

    /// Install a human responder.
    pub fn with_responder(mut self, responder: impl Responder + 'static) -> HostAdapter {
        self.responder = Some(Box::new(responder));
        self
    }

    /// Replace the resource bounds every request runs inside.
    pub fn with_bounds(mut self, bounds: Bounds) -> HostAdapter {
        self.bounds = bounds;
        self
    }

    /// Every request that crossed the boundary, in order.
    pub fn requests(&self) -> &[CapabilityRequest] {
        &self.log
    }

    /// Every declared delay the runtime handed over, in order.
    pub fn delays(&self) -> &[Value] {
        &self.delays
    }

    pub fn grants(&self) -> &Grants {
        &self.grants
    }

    /// The primitive grants this request's resolved axes imply.
    fn required_grants(&self, request: &CapabilityRequest) -> Vec<Grant> {
        let effects: Vec<Effect> = request
            .possible_effects
            .iter()
            .filter_map(|e| Effect::from_registry_str(e))
            .collect();
        let dependencies: Vec<Dependency> = request
            .possible_dependencies
            .iter()
            .filter_map(|d| Dependency::from_registry_str(d))
            .collect();

        let mut required = Vec::new();
        let target_path = path_of(request.target.as_ref());
        let destination_path = path_of(request.parameters.get("destination"));

        for effect in &effects {
            match effect {
                Effect::Filesystem => {
                    // The side that is written is the destination when one is
                    // declared, and the target otherwise.
                    if let Some(path) = destination_path.clone().or_else(|| target_path.clone()) {
                        required.push(Grant::WritePath(path));
                    }
                }
                Effect::Network => required.push(Grant::Network {
                    host: host_of(request).unwrap_or_default(),
                    secure: is_secure(request),
                }),
                Effect::Process => {
                    required.push(Grant::RunProgram(program_of(request).unwrap_or_default()))
                }
                Effect::Package => required.push(Grant::Package),
                Effect::Memory | Effect::State => required.push(Grant::InternalStore),
                Effect::Message => {
                    if dependencies.contains(&Dependency::Human) {
                        required.push(Grant::Human);
                    } else {
                        required.push(Grant::Network {
                            host: host_of(request).unwrap_or_default(),
                            secure: is_secure(request),
                        });
                    }
                }
            }
        }

        // A read that changes nothing still needs the host's permission to look.
        if effects.is_empty() && dependencies.contains(&Dependency::Host) {
            if let Some(path) = target_path.clone() {
                required.push(Grant::ReadPath(path));
            }
        }
        // Reading the source side of a transfer.
        if !effects.is_empty() {
            if let (Some(source), true) = (target_path, destination_path.is_some()) {
                required.push(Grant::ReadPath(source));
            }
        }
        if dependencies.contains(&Dependency::Model) {
            required.push(Grant::Model);
        }
        if dependencies.contains(&Dependency::Human) && !required.contains(&Grant::Human) {
            required.push(Grant::Human);
        }
        required
    }

    /// Whether this adapter has the capability one request needs at all.
    fn capability_missing(&self, request: &CapabilityRequest) -> Option<String> {
        let touches_filesystem = request.possible_effects.iter().any(|e| e == "filesystem")
            || (request.possible_effects.is_empty()
                && request.possible_dependencies.iter().any(|d| d == "host")
                && path_of(request.target.as_ref()).is_some());
        if touches_filesystem && self.filesystem.is_none() {
            return Some("no filesystem capability is installed".to_string());
        }
        let effects = |class: &str| request.possible_effects.iter().any(|e| e == class);
        if (effects("process") || effects("package")) && self.process.is_none() {
            return Some("no process capability is installed".to_string());
        }
        if effects("network") && self.transport.is_none() {
            return Some("no network transport is installed".to_string());
        }
        // A valid URI this host cannot address is a limitation of the host,
        // decided before any grant, because no grant could make it reachable.
        let to_person = request.possible_dependencies.iter().any(|d| d == "human");
        if effects("network") || (effects("message") && !to_person) {
            if let Some(Err(NetError::Refused(Refusal::Unavailable(detail)))) = address_of(request)
            {
                return Some(detail);
            }
        }
        if request.possible_dependencies.iter().any(|d| d == "human") && self.responder.is_none() {
            return Some("no human responder is installed".to_string());
        }
        if request.possible_dependencies.iter().any(|d| d == "model") {
            return Some("no model capability is installed".to_string());
        }
        None
    }
}

/// The path one value addresses, when it addresses one.
fn path_of(value: Option<&Value>) -> Option<PathBuf> {
    match value? {
        Value::Constructed { constructor, text } if constructor == "PATH" => {
            Some(PathBuf::from(text))
        }
        Value::WorkspacePath { resolved, .. } => Some(PathBuf::from(resolved)),
        _ => None,
    }
}

/// One filesystem address a request names, with the WORKSPACE root a
/// WORKSPACE-form PATH may not leave.
///
/// The root travels to the filesystem rather than being checked here, because
/// only the adapter that opens the target can decide containment on the same
/// resolution it opens.
struct Addressed {
    path: PathBuf,
    within: Option<PathBuf>,
}

impl Addressed {
    fn location(&self) -> Location<'_> {
        Location {
            path: &self.path,
            within: self.within.as_deref(),
        }
    }
}

fn addressed(value: Option<&Value>) -> Option<Addressed> {
    match value? {
        Value::WorkspacePath { root, resolved, .. } => Some(Addressed {
            path: PathBuf::from(resolved),
            within: Some(PathBuf::from(root)),
        }),
        other => path_of(Some(other)).map(|path| Addressed { path, within: None }),
    }
}

/// The refusal for a `core.read` format this adapter cannot produce, if any.
///
/// The row types `format` as `qualified_identifier(format)` with no declared
/// default, so an absent or MISSING format asks for nothing beyond the target's
/// own representation. This adapter produces exactly one, Unicode text
/// (`format.plain_text`, "Unicode plain text."), and implements no other
/// registered format. The checker leaves the source spelling of such a
/// parameter to its value, so the registered name is accepted as an identifier
/// or as text. Every other value is a host limitation, never plain text
/// substituted for the representation that was requested.
fn unproducible_format(requested: Option<&Value>) -> Option<CapabilityOutcome> {
    match requested {
        None | Some(Value::Missing) => None,
        Some(Value::Identifier(name) | Value::Text(name)) if name == "format.plain_text" => None,
        Some(other) => Some(CapabilityOutcome::Unavailable(format!(
            "core.read: this host produces only format.plain_text and cannot produce the \
             requested format {other}"
        ))),
    }
}

/// The URI one value addresses, when it addresses one.
fn uri_of(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::Constructed { constructor, text } if constructor == "URI" => Some(text.as_str()),
        _ => None,
    }
}

/// The network address one request reaches, from whichever side is a URI,
/// parsed by the transport's own [`Address::parse`], so that the grant decided
/// and the address reached are the same.
fn address_of(request: &CapabilityRequest) -> Option<Result<Address, NetError>> {
    let uri = uri_of(request.target.as_ref())
        .or_else(|| uri_of(request.parameters.get("destination")))
        .or_else(|| uri_of(request.parameters.get("recipient")))?;
    Some(Address::parse(uri))
}

/// The network host one request reaches.
fn host_of(request: &CapabilityRequest) -> Option<String> {
    address_of(request)?.ok().map(|address| address.host)
}

/// Whether the addressed scheme requires a secure transport.
fn is_secure(request: &CapabilityRequest) -> bool {
    matches!(address_of(request), Some(Ok(address)) if address.is_secure())
}

/// The program one execution request runs.
fn program_of(request: &CapabilityRequest) -> Option<String> {
    match request.target.as_ref()? {
        Value::Constructed { constructor, text } if constructor == "PATH" => Some(text.clone()),
        Value::WorkspacePath { resolved, .. } => Some(resolved.clone()),
        // "A PATH, URI, or STRING core.execute target requires the execution
        // role"; a STRING target is the command line, whose first word is the
        // program.
        Value::Text(command) => command.split_whitespace().next().map(|p| p.to_string()),
        _ => None,
    }
}

/// The text one value carries as content.
pub(crate) fn content_bytes(value: Option<&Value>) -> Vec<u8> {
    match value {
        Some(Value::Text(text)) => text.as_bytes().to_vec(),
        Some(other) => other.to_string().into_bytes(),
        None => Vec::new(),
    }
}

fn flag(request: &CapabilityRequest, name: &str) -> bool {
    matches!(request.parameters.get(name), Some(Value::Boolean(true)))
}

/// One filesystem failure as a host observation.
///
/// The adapter reports that the operation did not complete and why, in ordinary
/// words. Which registered identifier that becomes is the runtime's decision.
fn failed(detail: String, effect: Option<ObservedEffect>) -> CapabilityOutcome {
    let observation = match effect {
        Some(effect) => Observation::none().with_effect(effect),
        // Nothing began, and the adapter can prove it: the request was refused
        // by a precondition before any byte moved.
        None => Observation::none(),
    };
    CapabilityOutcome::Failed {
        detail,
        observation,
    }
}

fn applied(class: EffectClass, target: Option<String>) -> ObservedEffect {
    ObservedEffect {
        class,
        state: RecordState::Applied,
        target,
        evidence: Vec::new(),
    }
}

impl Host for HostAdapter {
    fn permits(&mut self, request: &CapabilityRequest) -> Permission {
        if let Some(detail) = self.capability_missing(request) {
            return Permission::Unavailable(detail);
        }
        for grant in self.required_grants(request) {
            match self.grants.decide(&grant) {
                Ok(()) => {}
                Err(Refusal::Denied(detail)) => return Permission::Denied(detail),
                Err(Refusal::Unavailable(detail)) => return Permission::Unavailable(detail),
            }
        }
        Permission::Granted
    }

    fn invoke(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        self.log.push(request.clone());
        match request.operation.as_str() {
            "core.read" => self.read(request),
            "core.inspect" => self.inspect(request),
            "core.create" => self.create(request),
            "core.write" => self.write(request),
            "core.append" => self.append(request),
            "core.modify" => self.modify(request),
            "core.delete" => self.delete(request),
            "core.rename" => self.rename(request),
            "core.move" => self.transfer(request, true),
            "core.copy" => self.transfer(request, false),
            "core.execute" => self.execute(request),
            "core.start" => self.start(request),
            "core.download" => self.download(request),
            "core.upload" | "core.publish" => self.upload(request),
            "core.send" => self.send(request),
            "core.ask" => self.ask(request),
            // Every other row's capability is not installed here yet. Saying so
            // is a limitation, not a refusal, and never a fabricated success.
            other => CapabilityOutcome::Unavailable(format!(
                "this host installs no capability for {other}"
            )),
        }
    }

    fn delay(&mut self, duration: &Value) {
        self.delays.push(duration.clone());
    }
}

impl HostAdapter {
    fn filesystem(&mut self) -> Result<&mut Box<dyn FileSystem>, CapabilityOutcome> {
        self.filesystem.as_mut().ok_or_else(|| {
            CapabilityOutcome::Unavailable("no filesystem capability is installed".to_string())
        })
    }

    fn target_path(&self, request: &CapabilityRequest) -> Result<Addressed, CapabilityOutcome> {
        addressed(request.target.as_ref()).ok_or_else(|| {
            failed(
                format!(
                    "{} needs a PATH target, and the resolved target is not one",
                    request.operation
                ),
                None,
            )
        })
    }

    fn destination_path(
        &self,
        request: &CapabilityRequest,
    ) -> Result<Addressed, CapabilityOutcome> {
        addressed(request.parameters.get("destination")).ok_or_else(|| {
            failed(
                format!(
                    "{} needs a PATH destination, and the resolved destination is not one",
                    request.operation
                ),
                None,
            )
        })
    }

    fn read(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let bounds = self.bounds.clone();
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        let range = match Range::of(request.parameters.get("range")) {
            Ok(range) => range,
            Err(refusal) => return refusal,
        };
        // "Resolve the exact target representation and any requested format
        // before selecting the range." This adapter produces one representation,
        // Unicode text, so a format it cannot produce is refused before the
        // target is read, rather than answered with text that was not asked for.
        if let Some(refusal) = unproducible_format(request.parameters.get("format")) {
            return refusal;
        }
        match filesystem.read(path.location(), &bounds) {
            Ok(bytes) => {
                // STRING is "Unicode text", and the row promises "exact content"
                // with "No clipping or ambient encoding conversion". Bytes that
                // are not UTF-8 have no exact text representation, so they are
                // this host's limitation (`error.host.constraint`, which the row
                // registers), never a substitution reported as the content.
                let text = match String::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(error) => {
                        return CapabilityOutcome::Unavailable(format!(
                            "core.read: {} is not UTF-8 text (the first invalid byte is at \
                             offset {}), and this host has no exact representation for it",
                            path.path.display(),
                            error.utf8_error().valid_up_to()
                        ))
                    }
                };
                let content = Value::Text(text);
                match range {
                    None => CapabilityOutcome::Completed(schema::value(content)),
                    Some(range) => match range.select(&content) {
                        Ok(selected) => CapabilityOutcome::Completed(schema::value(selected)),
                        Err(refusal) => refusal,
                    },
                }
            }
            // "target exists and is readable".
            Err(error) => registered_precondition(error, "target"),
        }
    }

    fn inspect(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.metadata(path.location()) {
            // "target exists".
            Ok(metadata) if !metadata.exists => {
                unmet_precondition("target", format!("{} does not exist", path.path.display()))
            }
            Ok(metadata) => {
                let mut fields = std::collections::BTreeMap::new();
                fields.insert("exists".to_string(), Value::Boolean(metadata.exists));
                fields.insert(
                    "is_directory".to_string(),
                    Value::Boolean(metadata.is_directory),
                );
                fields.insert("bytes".to_string(), schema::count(metadata.bytes as usize));
                fields.insert(
                    "entries".to_string(),
                    Value::List(metadata.entries.into_iter().map(Value::Text).collect()),
                );
                CapabilityOutcome::Completed(schema::value(Value::Object(fields)))
            }
            Err(error) => fs_failure(error, None),
        }
    }

    fn create(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        // "fail_if_exists" defaults TRUE, so the ordinary create refuses to
        // silently replace something.
        let mode = if flag(request, "fail_if_exists") {
            WriteMode::Create
        } else {
            WriteMode::Replace
        };
        let content = content_bytes(request.parameters.get("content"));
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.write(path.location(), &content, mode) {
            Ok(()) => completed_write(target, path.path),
            // "target does not exist when fail_if_exists is TRUE".
            Err(error) => registered_precondition(error, "target"),
        }
    }

    fn write(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let mode = if flag(request, "create_if_missing") {
            WriteMode::Replace
        } else {
            WriteMode::ReplaceExisting
        };
        let content = content_bytes(request.parameters.get("content"));
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.write(path.location(), &content, mode) {
            Ok(()) => completed_write(target, path.path),
            // "target exists unless create_if_missing is TRUE".
            Err(error @ FsError::NotFound(_)) if mode == WriteMode::ReplaceExisting => {
                registered_precondition(error, "target")
            }
            Err(error) => fs_failure(error, None),
        }
    }

    fn append(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let content = content_bytes(request.parameters.get("content"));
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.append(path.location(), &content) {
            Ok(()) => completed_write(target, path.path),
            // "target exists and supports append".
            Err(error) => registered_precondition(error, "target"),
        }
    }

    fn modify(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let change = content_bytes(request.parameters.get("change"));
        let target = request.target.clone().unwrap_or(Value::Null);
        let bounds = self.bounds.clone();
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        // "expected_before matches when supplied": the pre-state guard is read
        // before anything changes.
        if let Some(expected) = request.parameters.get("expected_before") {
            match filesystem.read(path.location(), &bounds) {
                Ok(current) if current == content_bytes(Some(expected)) => {}
                Ok(_) => {
                    return unmet_precondition(
                        "expected_before",
                        format!(
                            "{} does not hold the expected pre-state",
                            path.path.display()
                        ),
                    )
                }
                Err(error) => return registered_precondition(error, "target"),
            }
        }
        // A modification requires the target to exist; it changes content, and
        // it does not create.
        match filesystem.write(path.location(), &change, WriteMode::ReplaceExisting) {
            Ok(()) => completed_write(target, path.path),
            // "target exists".
            Err(error) => registered_precondition(error, "target"),
        }
    }

    fn delete(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let recursive = flag(request, "recursive");
        let require_exists = flag(request, "require_exists");
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        // "recursive deletion is explicitly authorized when needed": a
        // directory holding entries needs recursive TRUE before anything is
        // removed.
        if !recursive {
            match filesystem.metadata(path.location()) {
                Ok(metadata) if metadata.is_directory && !metadata.entries.is_empty() => {
                    return unmet_precondition(
                        "recursive",
                        format!(
                            "{} holds entries and recursive deletion is not authorized",
                            path.path.display()
                        ),
                    )
                }
                Ok(_) => {}
                Err(error) => return fs_failure(error, None),
            }
        }
        match filesystem.delete(path.location(), recursive) {
            // "changed is FALSE when no requested target state changed": an
            // absent target that need not exist is a completed no-change.
            Ok(false) if !require_exists => {
                CapabilityOutcome::Completed(schema::operation(target, Value::Boolean(false)))
            }
            // "target exists unless require_exists is FALSE".
            Ok(false) => registered_precondition(FsError::NotFound(path.path), "target"),
            Ok(true) => completed_write(target, path.path),
            Err(error) => fs_failure(error, None),
        }
    }

    fn rename(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let Some(Value::Text(new_name)) = request.parameters.get("new_name") else {
            return failed("core.rename needs a STRING new_name".to_string(), None);
        };
        let destination = Addressed {
            path: path.path.with_file_name(new_name),
            within: path.within.clone(),
        };
        let overwrite = flag(request, "overwrite");
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.rename(path.location(), destination.location(), overwrite) {
            Ok(()) => completed_write(target, destination.path),
            // "target exists"; "renamed destination is absent unless overwrite
            // is TRUE".
            Err(error @ FsError::AlreadyExists(_)) => registered_precondition(error, "new_name"),
            Err(error) => registered_precondition(error, "target"),
        }
    }

    /// `core.move` and `core.copy`, which differ only in whether the source
    /// survives.
    fn transfer(&mut self, request: &CapabilityRequest, remove_source: bool) -> CapabilityOutcome {
        let source = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let destination = match self.destination_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let overwrite = flag(request, "overwrite");
        let source_value = request.target.clone().unwrap_or(Value::Null);
        let destination_value = request
            .parameters
            .get("destination")
            .cloned()
            .unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };

        if remove_source {
            return match filesystem.rename(source.location(), destination.location(), overwrite) {
                Ok(()) => CapabilityOutcome::Completed(
                    schema::transfer(source_value, destination_value, Value::Unknown).with_effect(
                        applied(
                            EffectClass::Filesystem,
                            Some(destination.path.display().to_string()),
                        ),
                    ),
                ),
                Err(error) => transfer_precondition(error),
            };
        }
        match filesystem.copy(source.location(), destination.location(), overwrite) {
            Ok(bytes) => CapabilityOutcome::Completed(
                schema::transfer(
                    source_value,
                    destination_value,
                    Value::Bytes(lcl_checker::numeric::Decimal::from_integer(
                        lcl_checker::numeric::Integer::from_u64(bytes),
                    )),
                )
                .with_effect(applied(
                    EffectClass::Filesystem,
                    Some(destination.path.display().to_string()),
                )),
            ),
            Err(error) => transfer_precondition(error),
        }
    }
}

impl HostAdapter {
    fn command_of(&self, request: &CapabilityRequest) -> Result<Command, CapabilityOutcome> {
        let program = program_of(request).ok_or_else(|| {
            failed(
                format!("{} needs a program to run", request.operation),
                None,
            )
        })?;
        // A STRING target is a command line; its first word is the program and
        // the rest are arguments, before any declared `arguments` are appended.
        let mut arguments: Vec<String> = match request.target.as_ref() {
            Some(Value::Text(line)) => line
                .split_whitespace()
                .skip(1)
                .map(|word| word.to_string())
                .collect(),
            _ => Vec::new(),
        };
        if let Some(Value::List(declared)) = request.parameters.get("arguments") {
            arguments.extend(declared.iter().map(text_of));
        }
        let environment = match request.parameters.get("environment") {
            Some(Value::Object(fields)) => fields
                .iter()
                .map(|(name, value)| (name.clone(), text_of(value)))
                .collect(),
            _ => std::collections::BTreeMap::new(),
        };
        Ok(Command {
            program,
            arguments,
            working_directory: path_of(request.parameters.get("working_directory")),
            environment,
        })
    }

    /// The bounds one request runs inside, with any declared timeout applied.
    fn request_bounds(&self, request: &CapabilityRequest) -> Bounds {
        let deadline = match request.parameters.get("timeout") {
            Some(Value::Quantity(magnitude, unit)) => {
                nanoseconds(magnitude, unit.as_str()).map(lcl_capabilities::Deadline::from_nanos)
            }
            _ => None,
        };
        match deadline {
            Some(deadline) => self.bounds.clone().with_deadline(Some(deadline)),
            None => self.bounds.clone(),
        }
    }

    fn execute(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let command = match self.command_of(request) {
            Ok(command) => command,
            Err(outcome) => return outcome,
        };
        let bounds = self.request_bounds(request);
        let Some(process) = self.process.as_mut() else {
            return CapabilityOutcome::Unavailable(
                "no process capability is installed".to_string(),
            );
        };
        match process.run_observed(&command, &bounds) {
            Ok(completion) => {
                // "status.succeeded ... may accompany a FALSE domain outcome or
                // a nonzero command exit_code." The exit code is a field, and a
                // command that ran is a command that completed its contract.
                let exit_code = completion.exit_code.map(integer).unwrap_or(Value::Unknown);
                CapabilityOutcome::Completed(
                    schema::command(
                        "non_graph",
                        completion.started,
                        completion.completed,
                        exit_code,
                        completion.stdout,
                        completion.stderr,
                    )
                    .with_effect(applied(EffectClass::Process, Some(command.program))),
                )
            }
            Err(failure) => observed_process_failure(failure, command.program, None),
        }
    }

    fn start(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let command = match self.command_of(request) {
            Ok(command) => command,
            Err(outcome) => return outcome,
        };
        let bounds = self.request_bounds(request);
        let target = request.target.clone().unwrap_or(Value::Null);
        let Some(process) = self.process.as_mut() else {
            return CapabilityOutcome::Unavailable(
                "no process capability is installed".to_string(),
            );
        };
        match process.run_observed(&command, &bounds) {
            Ok(completion) => CapabilityOutcome::Completed(
                schema::operation(target, Value::Boolean(completion.started))
                    .with_effect(applied(EffectClass::Process, Some(command.program))),
            ),
            Err(failure) => observed_process_failure(failure, command.program, Some(target)),
        }
    }

    fn download(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let source = request.target.clone().unwrap_or(Value::Null);
        let Some(uri) = uri_of(request.target.as_ref()) else {
            return failed("core.download needs a URI target".to_string(), None);
        };
        let address = match Address::parse(uri) {
            Ok(address) => address,
            Err(error) => return net_failure(error),
        };
        let destination = match self.destination_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let destination_value = request
            .parameters
            .get("destination")
            .cloned()
            .unwrap_or(Value::Null);
        let bounds = self.request_bounds(request);
        // "destination is absent unless overwrite is TRUE" holds before the
        // primary download begins.
        if !flag(request, "overwrite") {
            if let Some(filesystem) = self.filesystem.as_mut() {
                match filesystem.metadata(destination.location()) {
                    Ok(metadata) if metadata.exists => {
                        return unmet_precondition(
                            "destination",
                            format!("{} already exists", destination.path.display()),
                        )
                    }
                    Ok(_) => {}
                    Err(error) => return fs_failure(error, None),
                }
            }
        }

        let body = {
            let Some(transport) = self.transport.as_mut() else {
                return CapabilityOutcome::Unavailable(
                    "no network transport is installed".to_string(),
                );
            };
            match transport.get(&address, &bounds) {
                Ok(response) if is_success(response.status) => response.body,
                // `core.download` registers the precondition "source is
                // accessible", and a status that is not a success says it is
                // not. The body of a 404 or a 500 is an error page and the body
                // of a 302 is a note about somewhere else; writing either to
                // the destination would satisfy "destination bytes equal
                // received source" with bytes that are not the source, and
                // report a transfer that did not happen.
                //
                // This is the row's own registered identifier, and the phase is
                // the one the registry assigns it: "an immediate operation
                // precondition fails before that operation's effects". Nothing
                // was written and no content was transferred into scope.
                Ok(response) => {
                    return unmet_precondition(
                        "source",
                        format!(
                            "{uri} answered with HTTP {}, so the source is not accessible",
                            response.status
                        ),
                    )
                }
                Err(error) => return net_failure(error),
            }
        };
        let bytes = body.len() as u64;
        let overwrite = flag(request, "overwrite");
        let Some(filesystem) = self.filesystem.as_mut() else {
            return CapabilityOutcome::Unavailable(
                "no filesystem capability is installed".to_string(),
            );
        };
        let mode = if overwrite {
            WriteMode::Replace
        } else {
            WriteMode::Create
        };
        match filesystem.write(destination.location(), &body, mode) {
            Ok(()) => CapabilityOutcome::Completed(
                schema::transfer(source, destination_value, bytes_value(bytes))
                    .with_effect(applied(EffectClass::Network, Some(uri.to_string())))
                    .with_effect(applied(
                        EffectClass::Filesystem,
                        Some(destination.path.display().to_string()),
                    )),
            ),
            // The transfer happened and the write did not: the effect state is
            // partial, and saying so is the only truthful answer.
            Err(error) => fs_failure(
                error,
                Some(applied(EffectClass::Network, Some(uri.to_string()))),
            ),
        }
    }

    fn upload(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let source_value = request.target.clone().unwrap_or(Value::Null);
        let destination_value = request
            .parameters
            .get("destination")
            .cloned()
            .unwrap_or(Value::Null);
        let bounds = self.request_bounds(request);

        // The content is whichever side names it: a PATH target is read, and a
        // material target is its own content.
        let body = match addressed(request.target.as_ref()) {
            Some(path) => {
                let Some(filesystem) = self.filesystem.as_mut() else {
                    return CapabilityOutcome::Unavailable(
                        "no filesystem capability is installed".to_string(),
                    );
                };
                match filesystem.read(path.location(), &bounds) {
                    Ok(bytes) => bytes,
                    // "source exists".
                    Err(error) => return registered_precondition(error, "source"),
                }
            }
            None => content_bytes(request.target.as_ref()),
        };

        // A PATH destination is a filesystem publication; a URI destination is
        // a network one.
        if let Some(destination) = addressed(request.parameters.get("destination")) {
            let Some(filesystem) = self.filesystem.as_mut() else {
                return CapabilityOutcome::Unavailable(
                    "no filesystem capability is installed".to_string(),
                );
            };
            let mode = if flag(request, "replace") || flag(request, "overwrite") {
                WriteMode::Replace
            } else {
                WriteMode::Create
            };
            return match filesystem.write(destination.location(), &body, mode) {
                Ok(()) => completed_transfer_or_operation(
                    request,
                    source_value,
                    destination_value,
                    body.len() as u64,
                    applied(
                        EffectClass::Filesystem,
                        Some(destination.path.display().to_string()),
                    ),
                ),
                // "destination is absent unless overwrite (or replace) is TRUE".
                Err(error @ FsError::AlreadyExists(_)) => {
                    registered_precondition(error, "destination")
                }
                Err(error) => fs_failure(error, None),
            };
        }

        let Some(uri) = uri_of(request.parameters.get("destination")) else {
            return failed(
                format!("{} needs a PATH or URI destination", request.operation),
                None,
            );
        };
        let address = match Address::parse(uri) {
            Ok(address) => address,
            Err(error) => return net_failure(error),
        };
        let uri = uri.to_string();
        let Some(transport) = self.transport.as_mut() else {
            return CapabilityOutcome::Unavailable("no network transport is installed".to_string());
        };
        match transport.put(&address, &body, &bounds) {
            Ok(response) if is_success(response.status) => completed_transfer_or_operation(
                request,
                source_value,
                destination_value,
                body.len() as u64,
                applied(EffectClass::Network, Some(uri)),
            ),
            // The content was sent, so the network effect began and is recorded
            // as such. What it did at the other end is not established: the
            // row's postcondition is "destination bytes equal source bytes" and
            // a refusing endpoint has not agreed to that. The registry admits
            // exactly this shape -- "a postcondition may fail ... after known
            // effects, or when the effect extent cannot be established".
            Ok(response) => {
                let observation = Observation::none().with_effect(ObservedEffect {
                    class: EffectClass::Network,
                    state: RecordState::Indeterminate,
                    target: Some(uri.clone()),
                    evidence: Vec::new(),
                });
                CapabilityOutcome::Failed {
                    detail: format!(
                        "{uri} answered with HTTP {}, which does not establish that the \
                         destination holds the uploaded bytes",
                        response.status
                    ),
                    observation,
                }
            }
            Err(error) => net_failure(error),
        }
    }

    fn send(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let recipient = request
            .parameters
            .get("recipient")
            .cloned()
            .unwrap_or(Value::Null);
        let body = content_bytes(request.target.as_ref());
        let bounds = self.request_bounds(request);

        let Some(uri) = uri_of(request.parameters.get("recipient")) else {
            // A recipient this transport cannot address is a limitation rather
            // than a delivery it may claim.
            return CapabilityOutcome::Unavailable(
                "core.send needs a URI recipient this host can address".to_string(),
            );
        };
        let address = match Address::parse(uri) {
            Ok(address) => address,
            Err(error) => return net_failure(error),
        };
        let uri = uri.to_string();
        let Some(transport) = self.transport.as_mut() else {
            return CapabilityOutcome::Unavailable("no network transport is installed".to_string());
        };
        match transport.put(&address, &body, &bounds) {
            Ok(response) => CapabilityOutcome::Completed(
                schema::message(
                    Value::Boolean((200..400).contains(&response.status)),
                    recipient,
                    Value::Null,
                )
                .with_effect(applied(EffectClass::Message, Some(uri))),
            ),
            Err(error) => net_failure(error),
        }
    }

    fn ask(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let question = match request.parameters.get("question") {
            Some(Value::Text(question)) => question.clone(),
            _ => return failed("core.ask needs a STRING question".to_string(), None),
        };
        let expected_type = match request.parameters.get("expected_type") {
            Some(Value::Text(name)) => name.clone(),
            Some(Value::Identifier(name)) => name.clone(),
            _ => return failed("core.ask needs an expected_type".to_string(), None),
        };
        // "every option is compatible with expected_type" is the row's own
        // precondition, and its implementation decided it before crossing.
        let listed: Vec<Value> = match request.parameters.get("options") {
            Some(Value::List(options)) => options.clone(),
            _ => Vec::new(),
        };
        let options: Vec<String> = listed.iter().map(text_of).collect();
        let Some(responder) = self.responder.as_mut() else {
            return CapabilityOutcome::Unavailable("no human responder is installed".to_string());
        };
        let answered = match responder.ask(&question, &options) {
            Ok(answered) => answered,
            Err(error) => return process_failure(error),
        };
        // The question was put, so the message effect began whatever came back.
        let asked = || Observation::none().with_effect(applied(EffectClass::Message, None));
        // "a non-MISSING answer is recorded and compatible with expected_type",
        // and "when options is supplied, every non-MISSING answer equals one
        // listed option". An answer that satisfies neither is not an authorized
        // valid answer.
        let answer = answered
            .map(Value::Text)
            .filter(|answer| compatible_answer(answer, &expected_type))
            .filter(|answer| {
                listed.is_empty() || listed.iter().any(|option| strict_equal(option, answer))
            });
        match answer {
            Some(answer) => CapabilityOutcome::Completed(
                schema::value(answer).with_effect(applied(EffectClass::Message, None)),
            ),
            // "when no authorized valid answer is provided, the value remains
            // MISSING and error.required.missing applies."
            None => CapabilityOutcome::Refused {
                observation: asked(),
                error: lcl_runtime::RuntimeError::RequiredMissing,
                cause: "answer".to_string(),
                detail: format!("no authorized valid answer to {question:?} was provided"),
            },
        }
    }
}

/// Whether one value is compatible with a written `expected_type`.
///
/// `core.ask`'s `expected_type` is a `type_expression`, and a human answer
/// arrives as text. A STRING answer is compatible with STRING; for any other
/// named scalar the text must denote a value of that type, which is what
/// "compatible with expected_type" means for something a person typed.
pub(crate) fn compatible_answer(value: &Value, expected_type: &str) -> bool {
    match expected_type {
        "STRING" => matches!(value, Value::Text(_)),
        "BOOLEAN" => match value {
            Value::Boolean(_) => true,
            Value::Text(text) => text == "TRUE" || text == "FALSE",
            _ => false,
        },
        "INTEGER" => match value {
            Value::Integer(_) => true,
            Value::Text(text) => text.parse::<i64>().is_ok(),
            _ => false,
        },
        "DECIMAL" => match value {
            Value::Integer(_) | Value::Decimal(_) => true,
            Value::Text(text) => text.parse::<f64>().is_ok(),
            _ => false,
        },
        // A type expression this adapter does not decide imposes nothing: the
        // declared parameter type already judged the written form, and
        // inventing a refusal here would be inventing language.
        _ => true,
    }
}

/// `core.upload` produces a transfer; `core.publish` produces an operation.
fn completed_transfer_or_operation(
    request: &CapabilityRequest,
    source: Value,
    destination: Value,
    bytes: u64,
    effect: ObservedEffect,
) -> CapabilityOutcome {
    if request.result_schema == "result.transfer" {
        return CapabilityOutcome::Completed(
            schema::transfer(source, destination, bytes_value(bytes)).with_effect(effect),
        );
    }
    CapabilityOutcome::Completed(
        schema::operation(destination, Value::Boolean(true)).with_effect(effect),
    )
}

fn bytes_value(bytes: u64) -> Value {
    Value::Bytes(lcl_checker::numeric::Decimal::from_integer(
        lcl_checker::numeric::Integer::from_u64(bytes),
    ))
}

fn integer(value: i64) -> Value {
    let magnitude = lcl_checker::numeric::Integer::from_u64(value.unsigned_abs());
    let decimal = lcl_checker::numeric::Decimal::from_integer(magnitude);
    if value < 0 {
        Value::Integer(decimal.negated())
    } else {
        Value::Integer(decimal)
    }
}

fn text_of(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        other => other.to_string(),
    }
}

/// One declared DURATION as whole nanoseconds.
///
/// The magnitude and unit are the document's own; nothing here reads a clock.
fn nanoseconds(magnitude: &lcl_checker::numeric::Decimal, unit: &str) -> Option<u128> {
    let seconds = magnitude.to_i64()? as u128;
    let factor = match unit {
        // The runtime normalizes a DURATION before crossing the host boundary.
        // This private representation already holds exact nanoseconds.
        "unit.nanosecond" | lcl_runtime::order_profile::DURATION_UNIT => 1u128,
        "unit.microsecond" => 1_000,
        "unit.millisecond" => 1_000_000,
        "unit.second" => 1_000_000_000,
        "unit.minute" => 60 * 1_000_000_000,
        "unit.hour" => 3_600 * 1_000_000_000,
        "unit.day" => 86_400 * 1_000_000_000,
        _ => return None,
    };
    Some(seconds * factor)
}

/// Preserve post-start observations independently of the runtime's registered
/// error/phase selection. Killing a process cannot prove what arbitrary code
/// already changed, so its effect extent is indeterminate, not effect-free.
fn observed_process_failure(
    failure: ProcessFailure,
    program: String,
    start_target: Option<Value>,
) -> CapabilityOutcome {
    let Some(completion) = failure.observation else {
        if let ProcessError::Bounded(cancelled) = failure.error {
            let mut observation = Observation::none();
            // A legacy adapter supplied no observation of whether anything
            // began. Absence of evidence is not proof of absence of effects.
            observation.proven_effect_free = false;
            observation.host_limited = true;
            return CapabilityOutcome::Failed {
                detail: cancelled.reason,
                observation,
            };
        }
        return process_failure(failure.error);
    };
    let detail = format!(
        "{}; deadline elapsed: {}; cleanup complete: {:?}; capture truncated: {}",
        failure.error, failure.timed_out, failure.cleanup_complete, completion.truncated
    );
    let mut observation = match start_target {
        Some(target) => schema::operation(target, Value::Unknown),
        None => schema::command(
            "non_graph",
            completion.started,
            completion.completed,
            completion.exit_code.map(integer).unwrap_or(Value::Unknown),
            completion.stdout,
            completion.stderr,
        ),
    };
    observation.host_limited = matches!(
        failure.error,
        ProcessError::Bounded(_) | ProcessError::Refused(Refusal::Unavailable(_))
    );
    if completion.started {
        observation = observation.with_effect(ObservedEffect {
            class: EffectClass::Process,
            state: RecordState::Indeterminate,
            target: Some(program),
            evidence: Vec::new(),
        });
    }
    CapabilityOutcome::Failed {
        detail,
        observation,
    }
}

/// One process failure without additional observations.
fn process_failure(error: ProcessError) -> CapabilityOutcome {
    match error {
        ProcessError::Refused(Refusal::Denied(detail)) => CapabilityOutcome::Denied(detail),
        ProcessError::Refused(Refusal::Unavailable(detail)) => {
            CapabilityOutcome::Unavailable(detail)
        }
        ProcessError::Bounded(cancelled) => CapabilityOutcome::Unavailable(cancelled.reason),
        // A program that never started began no effect, and the adapter can
        // prove it.
        ProcessError::NotStarted(detail) => failed(detail, None),
    }
}

/// One transport failure as the outcome the boundary carries.
fn net_failure(error: NetError) -> CapabilityOutcome {
    match error {
        NetError::Refused(Refusal::Denied(detail)) => CapabilityOutcome::Denied(detail),
        NetError::Refused(Refusal::Unavailable(detail)) => CapabilityOutcome::Unavailable(detail),
        NetError::Bounded(cancelled) => CapabilityOutcome::Unavailable(cancelled.reason),
        other => failed(other.to_string(), None),
    }
}

/// A completed filesystem mutation, with the effect it applied.
fn completed_write(target: Value, path: PathBuf) -> CapabilityOutcome {
    CapabilityOutcome::Completed(schema::operation(target, Value::Boolean(true)).with_effect(
        applied(EffectClass::Filesystem, Some(path.display().to_string())),
    ))
}

/// One filesystem error as the outcome the boundary carries.
fn fs_failure(error: FsError, effect: Option<ObservedEffect>) -> CapabilityOutcome {
    match error {
        // A refusal and a limitation are the host's two ways of saying no, and
        // the runtime maps them to two different registered identifiers.
        FsError::Refused(Refusal::Denied(detail)) => CapabilityOutcome::Denied(detail),
        FsError::Refused(Refusal::Unavailable(detail)) => CapabilityOutcome::Unavailable(detail),
        // Not the host's refusal: the language's own boundary. "Escape produces
        // error.value.out_of_range."
        escape @ FsError::Escape { .. } => CapabilityOutcome::Refused {
            observation: Observation::none(),
            error: lcl_runtime::RuntimeError::ValueOutOfRange,
            cause: "workspace".to_string(),
            detail: escape.to_string(),
        },
        FsError::Bounded(cancelled) => CapabilityOutcome::Unavailable(cancelled.reason),
        // The adapter opened the target and then failed. It cannot say that
        // nothing began, and `05_SEMANTICS/09` only permits the effect-free
        // claim for a failure positively established as pre-effect. How much
        // landed is not recoverable from the error, so the extent is
        // indeterminate — which is a fact, where `none` would be a guess.
        FsError::IoAfterChange { detail, target } => {
            let mut observation = Observation::none();
            if let Some(prior) = effect {
                observation = observation.with_effect(prior);
            }
            observation = observation.with_effect(ObservedEffect {
                class: EffectClass::Filesystem,
                state: RecordState::Indeterminate,
                target: Some(target.display().to_string()),
                // Directly observed: the adapter opened the target itself and
                // no declared EVIDENCE object is involved.
                evidence: Vec::new(),
            });
            CapabilityOutcome::Failed {
                detail,
                observation,
            }
        }
        other => failed(other.to_string(), effect),
    }
}

// ---------------------------------------------------------------------------
// core.read's range contract
// ---------------------------------------------------------------------------

/// `operations_v0.1.0.json#/contracts/core.read/parameters/range`.
///
/// > When supplied, range is an OBJECT with exactly unit: STRING, start:
/// > INTEGER, and end: INTEGER. No other keys or defaults are admitted. unit is
/// > exactly scalar, line, item, or byte.
///
/// > scalar requires STRING and indexes Unicode scalars; line requires STRING
/// > and indexes LF-terminated lines, retaining each selected line terminator
/// > and any final unterminated line. Empty STRING has zero lines. item
/// > requires LIST[T] and indexes elements. byte requires LIST[INTEGER] with
/// > every element in 0..255; BYTES is not content.
///
/// > Require 0 <= start <= end <= sequence length; otherwise
/// > error.value.out_of_range. Return the same representation family
/// > containing exactly positions start through end-1; equal bounds produce
/// > the corresponding empty value. No clipping or ambient encoding conversion
/// > occurs. An incompatible unit/representation or wrong key/type uses
/// > error.operation.parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Range {
    unit: Unit,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Scalar,
    Line,
    Item,
    Byte,
}

impl Unit {
    fn of(text: &str) -> Option<Unit> {
        match text {
            "scalar" => Some(Unit::Scalar),
            "line" => Some(Unit::Line),
            "item" => Some(Unit::Item),
            "byte" => Some(Unit::Byte),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Unit::Scalar => "scalar",
            Unit::Line => "line",
            Unit::Item => "item",
            Unit::Byte => "byte",
        }
    }
}

/// A wrong key, a wrong type, or a unit that does not index this
/// representation.
///
/// The row says these "use error.operation.parameter", and that is now what is
/// emitted.
///
/// ## What changed, and why the earlier reasoning was wrong
///
/// This previously selected `error.operation.precondition`, on the grounds that
/// `error.operation.parameter` is registered `static_or_expression` and
/// `expression_demand_resolution` does not make it eligible, so emitting it
/// here would relabel a stage.
///
/// The second half of that does not follow. The demand map moves an eligible
/// identifier's *stage*; it is not the only permission to *name* one. The
/// `exclusion_rule` settles it in the other direction: "Discovery time alone
/// never changes classification." An identifier discovered late keeps the
/// classification the registry gives it, and substituting a different
/// identifier is the one thing that really does change it. So this emits the
/// row's own identifier, and `lcl-runtime` carries its registered stage
/// `static_or_expression` and status `status.invalid` through unaltered.
///
/// ## What still reaches this arm
///
/// Less than used to. A range written as a literal OBJECT is knowable at M4,
/// and `lcl_checker`'s closed-object parameter check now decides a wrong key, a
/// wrong written type and an unregistered unit word there, at the stage
/// `earliest_stage_rule` assigns them to. What is left is what only execution
/// can see: a unit that does not index the representation the target actually
/// held, and a range that arrived through something no earlier stage read as a
/// literal.
fn wrong_parameter(detail: impl Into<String>) -> CapabilityOutcome {
    CapabilityOutcome::Refused {
        observation: Observation::none(),
        error: lcl_runtime::RuntimeError::OperationParameter,
        cause: "range".to_string(),
        detail: detail.into(),
    }
}

/// Whether an HTTP status reports that the request succeeded.
///
/// Only 2xx. A redirect is not content, and a client or server error is not
/// content either; each says the thing that was asked for was not delivered
/// here. This transport does not follow redirects, so a 3xx is an answer about
/// somewhere else rather than the source.
fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// A filesystem answer that falsifies the row's registered precondition on
/// `cause`: an absent target or source, or an existing destination, is
/// `error.operation.precondition` before effects. Every other error keeps its
/// own mapping.
fn registered_precondition(error: FsError, cause: &str) -> CapabilityOutcome {
    match error {
        FsError::NotFound(_) | FsError::AlreadyExists(_) => {
            unmet_precondition(cause, error.to_string())
        }
        other => fs_failure(other, None),
    }
}

/// `core.move` and `core.copy`: "source exists"; "destination absent unless
/// overwrite is TRUE".
fn transfer_precondition(error: FsError) -> CapabilityOutcome {
    match error {
        FsError::AlreadyExists(_) => registered_precondition(error, "destination"),
        other => registered_precondition(other, "source"),
    }
}

/// `error.operation.precondition`, with the row's own cause identity.
fn unmet_precondition(cause: &str, detail: impl Into<String>) -> CapabilityOutcome {
    CapabilityOutcome::Refused {
        observation: Observation::none(),
        error: lcl_runtime::RuntimeError::OperationPrecondition,
        cause: cause.to_string(),
        detail: detail.into(),
    }
}

/// `error.value.out_of_range`: bounds outside `0 <= start <= end <= length`.
fn out_of_range(detail: impl Into<String>) -> CapabilityOutcome {
    CapabilityOutcome::Refused {
        observation: Observation::none(),
        error: lcl_runtime::RuntimeError::ValueOutOfRange,
        cause: "range".to_string(),
        detail: detail.into(),
    }
}

impl Range {
    /// Read the declared range, or report exactly why it is not one.
    ///
    /// `Ok(None)` means no range was supplied, which the row admits: the
    /// parameter is optional and its default is no selection at all.
    fn of(value: Option<&Value>) -> Result<Option<Range>, CapabilityOutcome> {
        let Some(value) = value else {
            return Ok(None);
        };
        if matches!(value, Value::Missing | Value::Null) {
            return Ok(None);
        }
        let Value::Object(fields) = value else {
            return Err(wrong_parameter(format!(
                "core.read range is an OBJECT, found {}",
                value.family()
            )));
        };
        // "exactly unit, start and end. No other keys or defaults are
        // admitted."
        let keys: Vec<&str> = fields.keys().map(String::as_str).collect();
        if keys != ["end", "start", "unit"] {
            return Err(wrong_parameter(format!(
                "core.read range holds exactly unit, start and end; found {}",
                match keys.is_empty() {
                    true => "no key".to_string(),
                    false => keys.join(", "),
                }
            )));
        }
        let Some(Value::Text(unit)) = fields.get("unit") else {
            return Err(wrong_parameter("core.read range unit is a STRING"));
        };
        let Some(unit) = Unit::of(unit) else {
            return Err(wrong_parameter(format!(
                "core.read range unit is exactly scalar, line, item or byte; found {unit:?}"
            )));
        };
        let start = Range::bound(fields.get("start"), "start")?;
        let end = Range::bound(fields.get("end"), "end")?;
        // "Require 0 <= start <= end": the half that needs no content. A
        // negative bound already failed above, because a position is not a
        // negative number.
        if start > end {
            return Err(out_of_range(format!(
                "core.read range start {start} is after end {end}; bounds are never clipped"
            )));
        }
        Ok(Some(Range {
            unit,
            start: start as usize,
            end: end as usize,
        }))
    }

    /// One INTEGER bound, which must be a whole non-negative position.
    fn bound(value: Option<&Value>, name: &str) -> Result<i64, CapabilityOutcome> {
        let Some(Value::Integer(number)) = value else {
            return Err(wrong_parameter(format!(
                "core.read range {name} is an INTEGER"
            )));
        };
        // `INTEGER` is "an unbounded signed whole number", so a bound this
        // machine cannot hold is not an unreadable one: it is a perfectly well
        // typed position that is simply outside every sequence. The row says
        // which identifier that is — "Require 0 <= start <= end <= sequence
        // length; otherwise error.value.out_of_range" — and reserves
        // error.operation.parameter for "an incompatible unit/representation or
        // wrong key/type", which this is not. Reporting a malformed parameter
        // would answer a different question from the one the document asked.
        let Some(number) = number.to_i64() else {
            if !number.is_integral() {
                return Err(wrong_parameter(format!(
                    "core.read range {name} is an INTEGER"
                )));
            }
            return Err(out_of_range(format!(
                "core.read range {name} is {number}, outside every sequence;                  bounds are never clipped"
            )));
        };
        // "negative, inverted, or excessive bounds are never clipped."
        if number < 0 {
            return Err(out_of_range(format!(
                "core.read range {name} is {number}; a position is never negative"
            )));
        }
        Ok(number)
    }

    /// Select this range from one representation.
    fn select(self, content: &Value) -> Result<Value, CapabilityOutcome> {
        match (self.unit, content) {
            (Unit::Scalar, Value::Text(text)) => {
                let scalars: Vec<char> = text.chars().collect();
                let selected = self.slice(&scalars, "scalar")?;
                Ok(Value::Text(selected.iter().collect()))
            }
            // "line requires STRING and indexes LF-terminated lines, retaining
            // each selected line terminator and any final unterminated line.
            // Empty STRING has zero lines."
            (Unit::Line, Value::Text(text)) => {
                let lines = split_lines(text);
                let selected = self.slice(&lines, "line")?;
                Ok(Value::Text(selected.concat()))
            }
            (Unit::Item, Value::List(members)) => {
                let selected = self.slice(members, "item")?;
                Ok(Value::List(selected.to_vec()))
            }
            // "byte requires LIST[INTEGER] with every element in 0..255; BYTES
            // is not a count and is not content."
            (Unit::Byte, Value::List(members)) => {
                if let Some(outside) = members.iter().find(|m| !is_byte(m)) {
                    return Err(wrong_parameter(format!(
                        "core.read range unit byte requires LIST[INTEGER] with every element in \
                         0..255; found {outside}"
                    )));
                }
                let selected = self.slice(members, "byte")?;
                Ok(Value::List(selected.to_vec()))
            }
            (unit, other) => Err(wrong_parameter(format!(
                "core.read range unit {} does not index a {}",
                unit.as_str(),
                other.family()
            ))),
        }
    }

    /// "Return the same representation family containing exactly positions
    /// start through end-1; equal bounds produce the corresponding empty
    /// value."
    fn slice<'a, T>(self, sequence: &'a [T], unit: &str) -> Result<&'a [T], CapabilityOutcome> {
        if self.end > sequence.len() {
            return Err(out_of_range(format!(
                "core.read range end {} is past the {} {unit}s of the target; bounds are never \
                 clipped",
                self.end,
                sequence.len()
            )));
        }
        Ok(&sequence[self.start..self.end])
    }
}

/// Every LF-terminated line, plus any final unterminated one.
///
/// Each piece keeps its own terminator, so concatenating a selection
/// reconstructs exactly the bytes that were selected.
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    while let Some(position) = rest.find('\n') {
        let (line, after) = rest.split_at(position + 1);
        lines.push(line);
        rest = after;
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

/// One INTEGER in `0..255`.
fn is_byte(value: &Value) -> bool {
    match value {
        Value::Integer(number) => number.to_i64().is_some_and(|n| (0..=255).contains(&n)),
        _ => false,
    }
}
