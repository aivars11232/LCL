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
use lcl_capabilities::fs::{FileSystem, FsError, WriteMode};
use lcl_capabilities::net::{Address, NetError, Transport};
use lcl_capabilities::process::{Command, Process, ProcessError, Responder};
use lcl_capabilities::{Bounds, Dependency, Effect, Grant, Grants, Refusal};
use lcl_runtime::capability::{
    CapabilityOutcome, CapabilityRequest, Host, Observation, Permission,
};
use lcl_runtime::result::{EffectClass, ObservedEffect, RecordState};
use lcl_runtime::Value;
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
        _ => None,
    }
}

/// The URI one value addresses, when it addresses one.
fn uri_of(value: Option<&Value>) -> Option<&str> {
    match value? {
        Value::Constructed { constructor, text } if constructor == "URI" => Some(text.as_str()),
        _ => None,
    }
}

/// The network host one request reaches, from whichever side is a URI.
fn host_of(request: &CapabilityRequest) -> Option<String> {
    let uri = uri_of(request.target.as_ref())
        .or_else(|| uri_of(request.parameters.get("destination")))
        .or_else(|| uri_of(request.parameters.get("recipient")))?;
    let without_scheme = uri.split_once("://").map(|(_, rest)| rest).unwrap_or(uri);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    Some(host.split(':').next().unwrap_or(host).to_string())
}

/// Whether the addressed scheme requires a secure transport.
fn is_secure(request: &CapabilityRequest) -> bool {
    let uri = uri_of(request.target.as_ref())
        .or_else(|| uri_of(request.parameters.get("destination")))
        .or_else(|| uri_of(request.parameters.get("recipient")));
    uri.is_some_and(|uri| {
        let scheme = uri.split("://").next().unwrap_or_default();
        matches!(scheme, "https" | "wss" | "ftps")
    })
}

/// The program one execution request runs.
fn program_of(request: &CapabilityRequest) -> Option<String> {
    match request.target.as_ref()? {
        Value::Constructed { constructor, text } if constructor == "PATH" => Some(text.clone()),
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

    fn target_path(&self, request: &CapabilityRequest) -> Result<PathBuf, CapabilityOutcome> {
        path_of(request.target.as_ref()).ok_or_else(|| {
            failed(
                format!(
                    "{} needs a PATH target, and the resolved target is not one",
                    request.operation
                ),
                None,
            )
        })
    }

    fn destination_path(&self, request: &CapabilityRequest) -> Result<PathBuf, CapabilityOutcome> {
        path_of(request.parameters.get("destination")).ok_or_else(|| {
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
        match filesystem.read(&path, &bounds) {
            Ok(bytes) => CapabilityOutcome::Completed(schema::value(Value::Text(
                String::from_utf8_lossy(&bytes).to_string(),
            ))),
            Err(error) => fs_failure(error, None),
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
        match filesystem.metadata(&path) {
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
        match filesystem.write(&path, &content, mode) {
            Ok(()) => completed_write(target, path),
            Err(error) => fs_failure(error, None),
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
        match filesystem.write(&path, &content, mode) {
            Ok(()) => completed_write(target, path),
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
        match filesystem.append(&path, &content) {
            Ok(()) => completed_write(target, path),
            Err(error) => fs_failure(error, None),
        }
    }

    fn modify(&mut self, request: &CapabilityRequest) -> CapabilityOutcome {
        let path = match self.target_path(request) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let change = content_bytes(request.parameters.get("change"));
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        // A modification requires the target to exist; it changes content, and
        // it does not create.
        match filesystem.write(&path, &change, WriteMode::ReplaceExisting) {
            Ok(()) => completed_write(target, path),
            Err(error) => fs_failure(error, None),
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
        match filesystem.delete(&path, recursive) {
            // "changed is FALSE when no requested target state changed": an
            // absent target that need not exist is a completed no-change.
            Ok(false) if !require_exists => {
                CapabilityOutcome::Completed(schema::operation(target, Value::Boolean(false)))
            }
            Ok(false) => fs_failure(FsError::NotFound(path), None),
            Ok(true) => completed_write(target, path),
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
        let destination = path.with_file_name(new_name);
        let overwrite = flag(request, "overwrite");
        let target = request.target.clone().unwrap_or(Value::Null);
        let filesystem = match self.filesystem() {
            Ok(filesystem) => filesystem,
            Err(outcome) => return outcome,
        };
        match filesystem.rename(&path, &destination, overwrite) {
            Ok(()) => completed_write(target, destination),
            Err(error) => fs_failure(error, None),
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
            return match filesystem.rename(&source, &destination, overwrite) {
                Ok(()) => CapabilityOutcome::Completed(
                    schema::transfer(source_value, destination_value, Value::Unknown).with_effect(
                        applied(
                            EffectClass::Filesystem,
                            Some(destination.display().to_string()),
                        ),
                    ),
                ),
                Err(error) => fs_failure(error, None),
            };
        }
        match filesystem.copy(&source, &destination, overwrite) {
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
                    Some(destination.display().to_string()),
                )),
            ),
            Err(error) => fs_failure(error, None),
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
        match process.run(&command, &bounds) {
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
            Err(error) => process_failure(error),
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
        match process.run(&command, &bounds) {
            Ok(completion) => CapabilityOutcome::Completed(
                schema::operation(target, Value::Boolean(completion.started))
                    .with_effect(applied(EffectClass::Process, Some(command.program))),
            ),
            Err(error) => process_failure(error),
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

        let body = {
            let Some(transport) = self.transport.as_mut() else {
                return CapabilityOutcome::Unavailable(
                    "no network transport is installed".to_string(),
                );
            };
            match transport.get(&address, &bounds) {
                Ok(response) => response.body,
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
        match filesystem.write(&destination, &body, mode) {
            Ok(()) => CapabilityOutcome::Completed(
                schema::transfer(source, destination_value, bytes_value(bytes))
                    .with_effect(applied(EffectClass::Network, Some(uri.to_string())))
                    .with_effect(applied(
                        EffectClass::Filesystem,
                        Some(destination.display().to_string()),
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
        let body = match path_of(request.target.as_ref()) {
            Some(path) => {
                let Some(filesystem) = self.filesystem.as_mut() else {
                    return CapabilityOutcome::Unavailable(
                        "no filesystem capability is installed".to_string(),
                    );
                };
                match filesystem.read(&path, &bounds) {
                    Ok(bytes) => bytes,
                    Err(error) => return fs_failure(error, None),
                }
            }
            None => content_bytes(request.target.as_ref()),
        };

        // A PATH destination is a filesystem publication; a URI destination is
        // a network one.
        if let Some(destination) = path_of(request.parameters.get("destination")) {
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
            return match filesystem.write(&destination, &body, mode) {
                Ok(()) => completed_transfer_or_operation(
                    request,
                    source_value,
                    destination_value,
                    body.len() as u64,
                    applied(
                        EffectClass::Filesystem,
                        Some(destination.display().to_string()),
                    ),
                ),
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
            Ok(_) => completed_transfer_or_operation(
                request,
                source_value,
                destination_value,
                body.len() as u64,
                applied(EffectClass::Network, Some(uri)),
            ),
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
        let options: Vec<String> = match request.parameters.get("options") {
            Some(Value::List(options)) => options.iter().map(text_of).collect(),
            _ => Vec::new(),
        };
        let Some(responder) = self.responder.as_mut() else {
            return CapabilityOutcome::Unavailable("no human responder is installed".to_string());
        };
        match responder.ask(&question, &options) {
            // "when no authorized valid answer is provided, the answer remains
            // MISSING": the host reports the absence, and the runtime decides
            // what an absent required answer means.
            Ok(None) => CapabilityOutcome::Completed(
                schema::value(Value::Missing).with_effect(applied(EffectClass::Message, None)),
            ),
            Ok(Some(answer)) => CapabilityOutcome::Completed(
                schema::value(Value::Text(answer)).with_effect(applied(EffectClass::Message, None)),
            ),
            Err(error) => process_failure(error),
        }
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
        "unit.nanosecond" => 1u128,
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

/// One process failure as the outcome the boundary carries.
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
        FsError::Bounded(cancelled) => CapabilityOutcome::Unavailable(cancelled.reason),
        other => failed(other.to_string(), effect),
    }
}
