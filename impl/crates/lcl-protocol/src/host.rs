//! Turning what a caller granted into an operation surface and a host.
//!
//! ## Why this is not in the CLI
//!
//! It was. The `lcl` binary assembled its own `Stdlib` and `HostAdapter` from
//! its own flags, which was correct while the CLI was the only consumer. It is
//! not any more: the workspace UI runs the same documents under the same
//! grants, and the acceptance criterion for that UI is that its run "matches
//! CLI for identical inputs/capabilities".
//!
//! Two copies of this function that agree today are not a guarantee of that.
//! One copy that both call is. So it lives here, beside [`crate::Engine`],
//! which is the surface both consumers already share.
//!
//! ## What it decides
//!
//! Nothing about the language. It grants the host exactly what it was told to
//! grant and installs an implementation profile only beside the capability that
//! profile describes, because `operations_v0.1.0.json#/axis_contract/
//! implementation_profile` makes a profile "a claim an implementation exists"
//! and claiming one that is not there would be a lie the engine then acts on.
//!
//! An empty [`Granted`] is the honest default. The capability contract keeps
//! host permission and language authorization as separate gates, and a tool
//! that granted anything by default would be answering the operator's gate on
//! their behalf.

use crate::engine::Engine;
use lcl_capabilities::{RealFileSystem, RealProcess, TcpTransport};
use lcl_stdlib::{HostAdapter, Stdlib, StdlibError};
use std::path::PathBuf;

/// What a run is allowed to touch, exactly as the caller stated it.
///
/// Empty by default, and every field is additive: nothing here can widen a
/// grant the caller did not write down.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Granted {
    /// Paths a run may read.
    pub read: Vec<PathBuf>,
    /// Paths a run may write.
    pub write: Vec<PathBuf>,
    /// Programs a run may execute.
    pub programs: Vec<String>,
    /// Network hosts a run may reach.
    pub hosts: Vec<String>,
}

impl Granted {
    /// A run that may touch nothing outside the engine's own stores.
    pub fn none() -> Granted {
        Granted::default()
    }

    /// True when this grants no host capability at all.
    pub fn is_empty(&self) -> bool {
        self.read.is_empty()
            && self.write.is_empty()
            && self.programs.is_empty()
            && self.hosts.is_empty()
    }

    pub fn permit_read(mut self, path: impl Into<PathBuf>) -> Granted {
        self.read.push(path.into());
        self
    }

    pub fn permit_write(mut self, path: impl Into<PathBuf>) -> Granted {
        self.write.push(path.into());
        self
    }

    pub fn permit_program(mut self, program: impl Into<String>) -> Granted {
        self.programs.push(program.into());
        self
    }

    pub fn permit_host(mut self, host: impl Into<String>) -> Granted {
        self.hosts.push(host.into());
        self
    }
}

/// Assemble the operation surface and the host from what the caller granted.
///
/// A profile is installed only when the capability it describes is actually
/// present. The in-language verifier is the exception and is always available,
/// because it reaches nothing outside the language.
pub fn surface(engine: &Engine, granted: &Granted) -> Result<(Stdlib, HostAdapter), StdlibError> {
    let mut grants = lcl_capabilities::Grants::internal();
    for path in &granted.read {
        grants = grants.permit_read(path.clone());
    }
    for path in &granted.write {
        grants = grants.permit_write(path.clone());
    }
    for program in &granted.programs {
        grants = grants.permit_program(program.clone());
    }
    for host in &granted.hosts {
        grants = grants.permit_network_host(host.clone());
    }

    let mut profiles = lcl_stdlib::checking_profiles();
    let mut host = HostAdapter::new(grants.clone());
    if !granted.read.is_empty() || !granted.write.is_empty() {
        host = host.with_filesystem(RealFileSystem::new(grants.clone()));
        profiles.extend(lcl_stdlib::filesystem_profiles());
    }
    if !granted.programs.is_empty() {
        host = host.with_process(RealProcess::new(grants.clone()));
        profiles.extend(lcl_stdlib::process_profiles());
    }
    if !granted.hosts.is_empty() {
        host = host.with_transport(TcpTransport::new(grants.clone()));
        profiles.extend(lcl_stdlib::transport_profiles());
    }

    let stdlib = engine.stdlib()?.with_profiles(profiles).with_grants(grants);
    Ok((stdlib, host))
}
