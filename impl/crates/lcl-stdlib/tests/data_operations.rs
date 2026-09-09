//! Phase C: filesystem and addressable-data rows, and the engine's own stores.

mod common;

use lcl_capabilities::Grants;
use lcl_runtime::{Execution, Runtime, Value};
use lcl_stdlib::{filesystem_profiles, HostAdapter, MemoryFileSystem, Stdlib};

/// Execute one document against an installed filesystem and profile set.
fn run_fs(source: &str, filesystem: MemoryFileSystem, grants: Grants) -> Execution {
    let mut stdlib = common::stdlib().with_profiles(filesystem_profiles());
    let mut host = HostAdapter::new(grants).with_filesystem(filesystem);
    let fixture = common::fixture(source);
    Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned")
}

/// A document declaring one PATH and one action over it.
fn path_document(path: &str, action: &str) -> String {
    common::task(
        &common::data("data.target", "PATH", &format!("PATH({path:?})")),
        &[action],
    )
}

const WRITE_ACTION: &str = "ID: action.write\nOPERATION: core.write\nTARGET: REF(data.target)\n\
                            PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                            VALUE: \"hello\"\n\
                            PARAMETER:\n    NAME: create_if_missing\n    TYPE: BOOLEAN\n    \
                            REQUIRED: FALSE\n    VALUE: TRUE";

#[test]
fn core_write_puts_the_declared_content_at_the_resolved_path() {
    let source = path_document("/srv/data/report.txt", WRITE_ACTION);
    let filesystem = MemoryFileSystem::new().with_scope("/srv/data");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );

    let result = common::result_of(&execution, "action.write");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
    assert_eq!(result.fields.get("changed"), Some(&Value::Boolean(true)));
    assert_eq!(
        result.effect_state,
        lcl_runtime::EffectState::Applied,
        "a write that happened reports its effect"
    );
    assert_eq!(result.observed_effects.len(), 1);
    assert_eq!(
        result.observed_effects[0].class,
        lcl_runtime::EffectClass::Filesystem
    );
}

#[test]
fn core_read_returns_the_bytes_the_filesystem_holds() {
    let action = "ID: action.read\nOPERATION: core.read\nTARGET: REF(data.target)";
    let source = path_document("/srv/data/report.txt", action);
    let filesystem = MemoryFileSystem::new()
        .with_scope("/srv/data")
        .with_file("/srv/data/report.txt", "the exact content");
    let execution = run_fs(&source, filesystem, Grants::none().permit_read("/srv/data"));

    assert_eq!(
        common::field(&execution, "action.read", "value"),
        &Value::Text("the exact content".to_string())
    );
    // "read_only requires possible effects exactly {none}."
    let result = common::result_of(&execution, "action.read");
    assert!(result.observed_effects.is_empty());
    assert_eq!(result.effect_state, lcl_runtime::EffectState::None);
}

#[test]
fn a_path_outside_every_granted_scope_is_denied() {
    let source = path_document("/etc/passwd", WRITE_ACTION);
    let filesystem = MemoryFileSystem::new().with_scope("/srv/data");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );

    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.permission.denied".to_string()]
    );
    let result = common::result_of(&execution, "action.write");
    assert_eq!(result.failure_phase, lcl_runtime::FailurePhase::PreEffect);
    assert_eq!(result.effect_state, lcl_runtime::EffectState::None);
}

#[test]
fn a_traversal_out_of_a_granted_scope_is_denied() {
    // The grant is over /srv/data, and the path spells its way out of it.
    let source = path_document("/srv/data/../../etc/passwd", WRITE_ACTION);
    let filesystem = MemoryFileSystem::new().with_scope("/srv/data");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.permission.denied".to_string()]
    );
}

#[test]
fn lcl_authorization_alone_cannot_force_an_absent_capability() {
    // The document authorizes the write and the operator granted the scope;
    // the host simply has no filesystem. That is a limitation, and
    // "Host limitations produce error.host.constraint and never change LCL
    // meaning."
    let source = path_document("/srv/data/report.txt", WRITE_ACTION);
    let mut stdlib = common::stdlib().with_profiles(filesystem_profiles());
    let mut host = HostAdapter::new(Grants::none().permit_write("/srv/data"));
    let fixture = common::fixture(&source);
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.host.constraint".to_string()]
    );
}

#[test]
fn host_permission_alone_cannot_bypass_a_missing_profile() {
    // Everything the host needs is granted and installed. What is missing is
    // the *language* side of the implementation contract: no profile fills the
    // `write` role, so the row fails its precondition before effects.
    let source = path_document("/srv/data/report.txt", WRITE_ACTION);
    let mut stdlib = common::stdlib(); // no profiles installed
    let mut host = HostAdapter::new(Grants::none().permit_write("/srv/data"))
        .with_filesystem(MemoryFileSystem::new().with_scope("/srv/data"));
    let fixture = common::fixture(&source);
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("planned");

    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.operation.precondition".to_string()]
    );
    assert_eq!(
        host.requests().len(),
        0,
        "a missing profile stops the request before the boundary"
    );
}

#[test]
fn core_create_refuses_to_replace_an_existing_target_by_default() {
    // "fail_if_exists" defaults TRUE.
    let action = "ID: action.create\nOPERATION: core.create\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: FALSE\n    \
                  VALUE: \"new\"";
    let source = path_document("/srv/data/report.txt", action);
    let filesystem = MemoryFileSystem::new()
        .with_scope("/srv/data")
        .with_file("/srv/data/report.txt", "old");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );
    assert_eq!(
        common::result_of(&execution, "action.create").status,
        "status.failed"
    );
}

#[test]
fn core_delete_of_an_absent_target_that_need_not_exist_changed_nothing() {
    // "changed is FALSE when no requested target state changed."
    let action = "ID: action.delete\nOPERATION: core.delete\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: require_exists\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    \
                  VALUE: FALSE";
    let source = path_document("/srv/data/absent.txt", action);
    let filesystem = MemoryFileSystem::new().with_scope("/srv/data");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );

    let result = common::result_of(&execution, "action.delete");
    assert_eq!(result.status, "status.succeeded");
    assert_eq!(result.fields.get("changed"), Some(&Value::Boolean(false)));
    assert!(result.observed_effects.is_empty(), "nothing was deleted");
}

#[test]
fn core_rename_requires_a_new_name_that_differs() {
    let action = "ID: action.rename\nOPERATION: core.rename\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: new_name\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"report.txt\"";
    let source = path_document("/srv/data/report.txt", action);
    let filesystem = MemoryFileSystem::new()
        .with_scope("/srv/data")
        .with_file("/srv/data/report.txt", "content");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );
    assert_eq!(
        common::errors_of(&execution, "action.rename"),
        vec!["error.operation.precondition".to_string()]
    );
}

#[test]
fn core_rename_refuses_a_name_carrying_a_path_separator() {
    let action = "ID: action.rename\nOPERATION: core.rename\nTARGET: REF(data.target)\n\
                  PARAMETER:\n    NAME: new_name\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"../escaped.txt\"";
    let source = path_document("/srv/data/report.txt", action);
    let filesystem = MemoryFileSystem::new()
        .with_scope("/srv/data")
        .with_file("/srv/data/report.txt", "content");
    let execution = run_fs(
        &source,
        filesystem,
        Grants::none().permit_write("/srv/data"),
    );
    assert_eq!(
        common::errors_of(&execution, "action.rename"),
        vec!["error.operation.precondition".to_string()]
    );
}

#[test]
fn a_mutating_row_refuses_an_internal_store_target() {
    // "core.create, core.write, … prohibit MEMORY and STATE targets … Those
    // mutations use core.memory_write or core.state_update."
    let declarations = MEMORY_DECLARATIONS;
    let action = "ID: action.write\nOPERATION: core.write\nTARGET: REF(memory.notes)\n\
                  PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"replaced\"";
    let source = common::task(declarations, &[action]);
    let execution = run_fs(
        &source,
        MemoryFileSystem::new().with_scope("/srv"),
        Grants::none().permit_write("/srv"),
    );
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.operation.precondition".to_string()]
    );
}

// ---------------------------------------------------------------------------
// The engine's own stores
// ---------------------------------------------------------------------------

const MEMORY_DECLARATIONS: &str = "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n\
     \nMEMORY:\n    ID: memory.notes\n    TYPE: STRING\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: \"kept\"\n";

#[test]
fn core_memory_write_changes_what_a_later_read_sees() {
    let actions = [
        "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(memory.notes)\n\
         PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
         VALUE: \"written\"",
        "ID: action.read\nOPERATION: core.return\nTARGET: REF(memory.notes)",
    ];
    let source = common::task(MEMORY_DECLARATIONS, &actions);
    let execution = common::run(&source);

    let written = common::result_of(&execution, "action.write");
    assert_eq!(
        written.status, "status.succeeded",
        "{:?}",
        written.execution_errors
    );
    assert_eq!(written.fields.get("changed"), Some(&Value::Boolean(true)));
    assert_eq!(
        written.observed_effects.first().map(|e| e.class),
        Some(lcl_runtime::EffectClass::Memory)
    );

    // `05_SEMANTICS/07` calls MEMORY "retained data": the write is what a later
    // read resolves, not the declared sample.
    assert_eq!(
        common::field(&execution, "action.read", "value"),
        &Value::Text("written".to_string())
    );
}

#[test]
fn writing_the_value_a_store_already_holds_changed_nothing() {
    let action = "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(memory.notes)\n\
                  PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"kept\"";
    let source = common::task(MEMORY_DECLARATIONS, &[action]);
    let execution = common::run(&source);
    assert_eq!(
        common::result_of(&execution, "action.write")
            .fields
            .get("changed"),
        Some(&Value::Boolean(false))
    );
}

#[test]
fn an_ungranted_store_is_denied() {
    // The plan authorizes the write; the host does not permit the store. Both
    // gates are real, and neither implies the other.
    let action = "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(memory.notes)\n\
                  PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"written\"";
    let source = common::task(MEMORY_DECLARATIONS, &[action]);
    let stdlib: Stdlib = common::stdlib().with_grants(Grants::none());
    let execution = common::run_with(&source, stdlib, lcl_runtime::MockHost::new());
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.permission.denied".to_string()]
    );
}

#[test]
fn a_store_row_refuses_a_target_of_the_wrong_declaration_kind() {
    let declarations = format!(
        "{MEMORY_DECLARATIONS}{}",
        common::data("data.plain", "STRING", "\"not a store\"")
    );
    let action = "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(data.plain)\n\
                  PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    \
                  VALUE: \"written\"";
    let source = common::task(&declarations, &[action]);
    let execution = common::run(&source);
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.reference.kind".to_string()]
    );
}
