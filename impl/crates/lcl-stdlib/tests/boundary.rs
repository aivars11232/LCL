//! Phase C: what a host may not claim, and what a real adapter actually does.

mod common;

use lcl_capabilities::{Grants, RealFileSystem};
use lcl_runtime::capability::{
    CapabilityOutcome, CapabilityRequest, Host, Observation, Permission,
};
use lcl_runtime::result::{EffectClass, ObservedEffect, RecordState};
use lcl_runtime::{Execution, Runtime, Value};
use lcl_stdlib::{filesystem_profiles, HostAdapter};
use std::path::PathBuf;

/// A host that reports an effect the invocation never resolved.
struct FabricatingHost;

impl Host for FabricatingHost {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        Permission::Granted
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        CapabilityOutcome::Completed(
            Observation::none()
                .with("value", Value::Text("read".to_string()))
                .with_effect(ObservedEffect {
                    class: EffectClass::Filesystem,
                    state: RecordState::Applied,
                    target: None,
                    evidence: Vec::new(),
                }),
        )
    }
}

fn run_against(source: &str, host: &mut dyn Host) -> Execution {
    let mut stdlib = common::stdlib().with_profiles(filesystem_profiles());
    let fixture = common::fixture(source);
    Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            host,
        )
        .expect("the document planned")
}

#[test]
fn a_host_may_not_report_an_effect_the_invocation_did_not_resolve() {
    // `core.read` is read_only, and "read_only requires possible effects exactly
    // {none}". A host claiming a filesystem effect would be adding meaning the
    // document never authorized, which is what
    // `CapabilityRequest::possible_effects` means by "A host may not report an
    // effect outside it."
    let source = common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/report.txt\")"),
        &["ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)"],
    );
    let execution = run_against(&source, &mut FabricatingHost);

    assert_eq!(
        common::errors_of(&execution, "action.read"),
        vec!["error.operation.postcondition".to_string()]
    );
    let result = common::result_of(&execution, "action.read");
    // The host said something happened and could not say what, so neither axis
    // is knowable: "Absence of evidence never proves absence of effects."
    assert_eq!(
        result.failure_phase,
        lcl_runtime::FailurePhase::Indeterminate
    );
    assert_eq!(result.effect_state, lcl_runtime::EffectState::Indeterminate);
    assert!(
        result.fields.is_empty(),
        "the refused observation contributes no fields"
    );
}

// ---------------------------------------------------------------------------
// The real adapter
// ---------------------------------------------------------------------------

/// A private directory for one test, created under the system temporary root.
///
/// std-only, so this is built by hand rather than by a crate. The name carries
/// the test's own label, so two tests never share a directory.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("lcl-stdlib-{}-{label}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temporary directory is creatable");
        TempDir { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn the_real_filesystem_adapter_writes_and_reads_actual_bytes() {
    // Runtime acceptance requires runtime evidence, so this one does not use a
    // fixture: the bytes are written to a real directory by the real adapter,
    // and read back off the disk by the test rather than from the engine.
    let directory = TempDir::new("write");
    let target = directory.join("report.txt");
    let root = directory.path.display().to_string();

    let source = common::task(
        &common::data(
            "data.target",
            "PATH",
            &format!("PATH({:?})", target.display().to_string()),
        ),
        &[
            "ID: action.write\nOPERATION: core.write\nTARGET: REF(data.target)\n\
           PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
           VALUE: \"written by the real adapter\"\n\
           PARAMETER:\n    NAME: create_if_missing\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    \
           VALUE: TRUE",
        ],
    );

    let grants = Grants::none().permit_write(&root);
    let mut host = HostAdapter::new(grants.clone()).with_filesystem(RealFileSystem::new(grants));
    let execution = run_against(&source, &mut host);

    let result = common::result_of(&execution, "action.write");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(
        std::fs::read_to_string(&target).expect("the file exists on disk"),
        "written by the real adapter"
    );
}

#[test]
fn the_real_filesystem_adapter_refuses_a_symlink_out_of_its_scope() {
    // The lexical check cannot see a symlink, so the adapter canonicalises and
    // checks containment again. Without that second check, a link inside a
    // granted root would be a way out of it.
    let directory = TempDir::new("symlink");
    let outside = TempDir::new("symlink-outside");
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, "not yours").expect("writable");

    let link = directory.join("link.txt");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&secret, &link).is_ok();
    #[cfg(not(unix))]
    let linked = false;
    if !linked {
        // Without symlink support the second check has nothing to catch, and
        // the lexical one already covers this platform.
        return;
    }

    let root = directory.path.display().to_string();
    let source = common::task(
        &common::data(
            "data.target",
            "PATH",
            &format!("PATH({:?})", link.display().to_string()),
        ),
        &["ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)"],
    );

    let grants = Grants::none().permit_read(&root);
    let mut host = HostAdapter::new(grants.clone()).with_filesystem(RealFileSystem::new(grants));
    let execution = run_against(&source, &mut host);

    assert_eq!(
        common::errors_of(&execution, "action.read"),
        vec!["error.permission.denied".to_string()],
        "a link out of the granted scope is refused"
    );
    assert_eq!(
        std::fs::read_to_string(&secret).expect("the target is untouched"),
        "not yours"
    );
}

// ---------------------------------------------------------------------------
// "A host may not report an effect outside it" — on every outcome that carries
// observations
// ---------------------------------------------------------------------------
//
// `FabricatingHost` above proves the rule for `Completed`. The rule is not
// about `Completed`: it is about the observations, and three of the outcome's
// arms carry them. A check that inspected only two would let a host state the
// same untrue thing by choosing a different arm to say it in.

/// A host that fabricates the same effect through whichever arm it is given.
struct FabricatingArm(&'static str);

impl FabricatingArm {
    fn observation() -> Observation {
        Observation::none()
            .with("value", Value::Text("read".to_string()))
            .with_effect(ObservedEffect {
                class: EffectClass::Filesystem,
                state: RecordState::Applied,
                target: None,
                evidence: Vec::new(),
            })
    }
}

impl Host for FabricatingArm {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        Permission::Granted
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        match self.0 {
            "completed" => CapabilityOutcome::Completed(FabricatingArm::observation()),
            "failed" => CapabilityOutcome::Failed {
                detail: "the host gave up".to_string(),
                observation: FabricatingArm::observation(),
            },
            "refused" => CapabilityOutcome::Refused {
                error: lcl_runtime::RuntimeError::ValueOutOfRange,
                cause: "range".to_string(),
                detail: "the host refused".to_string(),
                observation: FabricatingArm::observation(),
            },
            other => panic!("unknown arm {other}"),
        }
    }
}

/// A host that observes nothing at all, through the same three arms.
struct HonestArm(&'static str);

impl Host for HonestArm {
    fn permits(&mut self, _request: &CapabilityRequest) -> Permission {
        Permission::Granted
    }

    fn invoke(&mut self, _request: &CapabilityRequest) -> CapabilityOutcome {
        let observation = Observation::none().with("value", Value::Text("read".to_string()));
        match self.0 {
            "completed" => CapabilityOutcome::Completed(observation),
            "failed" => CapabilityOutcome::Failed {
                detail: "the host gave up".to_string(),
                observation,
            },
            "refused" => CapabilityOutcome::Refused {
                error: lcl_runtime::RuntimeError::ValueOutOfRange,
                cause: "range".to_string(),
                detail: "the host refused".to_string(),
                observation,
            },
            other => panic!("unknown arm {other}"),
        }
    }
}

fn reading_document() -> String {
    common::task(
        &common::data("data.target", "PATH", "PATH(\"/srv/data/report.txt\")"),
        &["ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)"],
    )
}

#[test]
fn an_out_of_set_effect_is_refused_through_every_observation_bearing_arm() {
    // `core.read` is read_only, and "read_only requires possible effects
    // exactly {none}", so a filesystem effect is outside its resolved set
    // whichever arm reports it.
    for arm in ["completed", "failed", "refused"] {
        let execution = run_against(&reading_document(), &mut FabricatingArm(arm));
        assert_eq!(
            common::errors_of(&execution, "action.read"),
            vec!["error.operation.postcondition".to_string()],
            "arm {arm}"
        );
        let result = common::result_of(&execution, "action.read");
        // Rejecting the claim is not proof that nothing happened: "Absence of
        // evidence never proves absence of effects."
        assert_eq!(
            result.failure_phase,
            lcl_runtime::FailurePhase::Indeterminate,
            "arm {arm}"
        );
        assert_eq!(
            result.effect_state,
            lcl_runtime::EffectState::Indeterminate,
            "arm {arm}"
        );
    }
}

/// The control: the same three arms observing nothing are not rejected.
///
/// Without it the rule above could be "reject every failure and refusal", which
/// would make the check meaningless.
#[test]
fn an_effect_free_outcome_is_not_rejected_through_any_arm() {
    for arm in ["completed", "failed", "refused"] {
        let execution = run_against(&reading_document(), &mut HonestArm(arm));
        assert!(
            !common::errors_of(&execution, "action.read")
                .contains(&"error.operation.postcondition".to_string()),
            "arm {arm}: {:?}",
            common::errors_of(&execution, "action.read")
        );
    }
}
