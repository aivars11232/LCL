//! Phase C: filesystem and addressable-data rows, and the engine's own stores.

mod common;

use lcl_capabilities::Grants;
use lcl_runtime::{Execution, Runtime, Value};
use lcl_stdlib::{filesystem_profiles, store_profiles, HostAdapter, MemoryFileSystem, Stdlib};

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
    let stdlib = common::stdlib().with_profiles(store_profiles());
    let execution = common::run_with(&source, stdlib, lcl_runtime::MockHost::new());

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
    let stdlib = common::stdlib().with_profiles(store_profiles());
    let execution = common::run_with(&source, stdlib, lcl_runtime::MockHost::new());
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
    // The storage profile is installed, so the grant gate is what refuses.
    let stdlib: Stdlib = common::stdlib()
        .with_profiles(store_profiles())
        .with_grants(Grants::none());
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

// ---------------------------------------------------------------------------
// STORE-ROLE-01 — the storage profile role both store rows require
// ---------------------------------------------------------------------------

const STATE_DECLARATIONS: &str = "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n\
     \nSTATE:\n    ID: state.revision\n    TYPE: INTEGER\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: 2\n";

const MEMORY_WRITE_ACTION: &str =
    "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(memory.notes)\n\
     PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: \"written\"";

const STATE_UPDATE_ACTION: &str =
    "ID: action.write\nOPERATION: core.state_update\nTARGET: REF(state.revision)\n\
     PARAMETER:\n    NAME: value\n    TYPE: INTEGER\n    REQUIRED: TRUE\n    VALUE: 3";

/// One store write refused for its missing `storage` profile, before any effect.
fn assert_refused_before_effects(execution: &Execution) {
    let result = common::result_of(execution, "action.write");
    assert_eq!(
        result.execution_errors,
        vec!["error.operation.precondition".to_string()],
        "a store row with no storage profile installed must fail its precondition"
    );
    assert_eq!(result.status, "status.failed");
    assert_eq!(result.failure_phase.to_string(), "pre_effect");
    assert_eq!(result.effect_state.to_string(), "none");
    assert!(
        result.observed_effects.is_empty(),
        "{:?}",
        result.observed_effects
    );
    // `result.operation` registers `changed` as `exactly_one`, and "changed
    // remains present after failure". It is FALSE because the refusal happened
    // before any requested target state changed.
    assert_eq!(result.fields.get("changed"), Some(&Value::Boolean(false)));
}

/// STORE-ROLE-01: `core.memory_write` requires its registered `storage` profile.
///
/// `operations_v0.1.0.json#/axis_contract/implementation_profile` names the
/// `storage` role in `required_roles_by_operation` for every `core.memory_write`
/// invocation, and the row's resolution begins "Resolve the authorized MEMORY
/// storage profile". `06_STANDARD_LIBRARY/10`: "A missing, ambiguous,
/// incomplete, or out-of-bounds required profile role emits
/// error.operation.precondition before effects." `common::stdlib()` installs no
/// profile of any kind, so the write must be refused before the store changes.
#[test]
fn a_memory_write_without_a_storage_profile_fails_its_precondition_before_effects() {
    let source = common::task(MEMORY_DECLARATIONS, &[MEMORY_WRITE_ACTION]);
    assert_refused_before_effects(&common::run(&source));
}

/// STORE-ROLE-01: `core.state_update` requires the same role. Its resolution
/// begins "Resolve the authorized STATE storage profile, including its
/// transaction variant".
#[test]
fn a_state_update_without_a_storage_profile_fails_its_precondition_before_effects() {
    let source = common::task(STATE_DECLARATIONS, &[STATE_UPDATE_ACTION]);
    assert_refused_before_effects(&common::run(&source));
}

/// The control: with its `storage` profile installed, `core.state_update` writes
/// the engine's own STATE store and reports the effect.
#[test]
fn a_state_update_with_its_storage_profile_writes_the_store() {
    let source = common::task(STATE_DECLARATIONS, &[STATE_UPDATE_ACTION]);
    let stdlib = common::stdlib().with_profiles(store_profiles());
    let execution = common::run_with(&source, stdlib, lcl_runtime::MockHost::new());
    let written = common::result_of(&execution, "action.write");
    assert!(
        written.execution_errors.is_empty(),
        "{:?}",
        written.execution_errors
    );
    assert_eq!(written.status, "status.succeeded");
    assert_eq!(written.fields.get("changed"), Some(&Value::Boolean(true)));
    assert_eq!(
        written.observed_effects.first().map(|e| e.class),
        Some(lcl_runtime::EffectClass::State)
    );
}

// ---------------------------------------------------------------------------
// FINAL-02: registered filesystem preconditions and address classification
// ---------------------------------------------------------------------------

/// One action over `/srv/data`, as `(id suffix, operation line and fields)`.
fn precondition_document(action: &str) -> String {
    common::task(
        &format!(
            "{}{}",
            common::data("data.target", "PATH", "PATH(\"/srv/data/report.txt\")"),
            common::data("data.other", "PATH", "PATH(\"/srv/data/other.txt\")")
        ),
        &[&format!("ID: action.subject\n{action}")],
    )
}

fn parameter(name: &str, ty: &str, value: &str) -> String {
    format!(
        "\nPARAMETER:\n    NAME: {name}\n    TYPE: {ty}\n    REQUIRED: FALSE\n    VALUE: {value}"
    )
}

/// Every registered filesystem precondition the shipped adapter can observe
/// fails with `error.operation.precondition` before any effect:
/// "A registered operation precondition ... is false" (statuses_and_errors),
/// "an immediate operation precondition fails before that operation's
/// effects" (failure_lifecycle).
#[test]
fn registered_filesystem_preconditions_fail_before_effects() {
    let absent = "TARGET: PATH(\"/srv/data/absent.txt\")";
    type Case<'a> = (&'a str, String, Vec<(&'a str, &'a str)>);
    let cases: Vec<Case<'_>> = vec![
        (
            "read: target exists and is readable",
            format!("OPERATION: core.read\n{absent}"),
            vec![],
        ),
        (
            "inspect: target exists",
            format!("OPERATION: core.inspect\n{absent}"),
            vec![],
        ),
        (
            "create: target does not exist when fail_if_exists is TRUE",
            format!(
                "OPERATION: core.create\nTARGET: REF(data.target){}",
                parameter("content", "STRING", "\"new\"")
            ),
            vec![],
        ),
        (
            "create: at least content or target_type is supplied",
            "OPERATION: core.create\nTARGET: REF(data.other)".to_string(),
            vec![],
        ),
        (
            "write: target exists unless create_if_missing is TRUE",
            format!(
                "OPERATION: core.write\n{absent}{}",
                parameter("content", "STRING", "\"x\"")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![],
        ),
        (
            "append: target exists and supports append",
            format!(
                "OPERATION: core.append\n{absent}{}",
                parameter("content", "STRING", "\"x\"")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![],
        ),
        (
            "modify: target exists",
            format!(
                "OPERATION: core.modify\n{absent}{}",
                parameter("change", "STRING", "\"x\"").replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![],
        ),
        (
            "modify: expected_before matches when supplied",
            format!(
                "OPERATION: core.modify\nTARGET: REF(data.target){}{}",
                parameter("change", "STRING", "\"x\"").replace("REQUIRED: FALSE", "REQUIRED: TRUE"),
                parameter("expected_before", "STRING", "\"not the content\"")
            ),
            vec![],
        ),
        (
            "delete: target exists unless require_exists is FALSE",
            format!("OPERATION: core.delete\n{absent}"),
            vec![],
        ),
        (
            "delete: recursive deletion is explicitly authorized when needed",
            "OPERATION: core.delete\nTARGET: PATH(\"/srv/data/tree\")".to_string(),
            vec![("/srv/data/tree/leaf.txt", "leaf")],
        ),
        (
            "rename: target exists",
            format!(
                "OPERATION: core.rename\n{absent}{}",
                parameter("new_name", "STRING", "\"renamed.txt\"")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![],
        ),
        (
            "rename: renamed destination is absent unless overwrite is TRUE",
            format!(
                "OPERATION: core.rename\nTARGET: REF(data.target){}",
                parameter("new_name", "STRING", "\"other.txt\"")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![("/srv/data/other.txt", "old")],
        ),
        (
            "copy: source exists",
            format!(
                "OPERATION: core.copy\n{absent}{}",
                parameter("destination", "PATH", "REF(data.other)")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![],
        ),
        (
            "copy: destination absent unless overwrite is TRUE",
            format!(
                "OPERATION: core.copy\nTARGET: REF(data.target){}",
                parameter("destination", "PATH", "REF(data.other)")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![("/srv/data/other.txt", "old")],
        ),
        (
            "move: destination absent unless overwrite is TRUE",
            format!(
                "OPERATION: core.move\nTARGET: REF(data.target){}",
                parameter("destination", "PATH", "REF(data.other)")
                    .replace("REQUIRED: FALSE", "REQUIRED: TRUE")
            ),
            vec![("/srv/data/other.txt", "old")],
        ),
    ];
    let mut wrong = Vec::new();
    for (name, action, extra) in cases {
        let mut filesystem = MemoryFileSystem::new()
            .with_scope("/srv/data")
            .with_file("/srv/data/report.txt", "content");
        for (path, content) in extra {
            filesystem = filesystem.with_file(path, content);
        }
        let execution = run_fs(
            &precondition_document(&action),
            filesystem,
            Grants::none().permit_write("/srv/data"),
        );
        let result = common::result_of(&execution, "action.subject");
        let observed = (
            result.execution_errors.clone(),
            result.failure_phase.to_string(),
            result.effect_state.to_string(),
        );
        let expected = (
            vec!["error.operation.precondition".to_string()],
            "pre_effect".to_string(),
            "none".to_string(),
        );
        if observed != expected {
            wrong.push(format!("{name}: {observed:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// "A REFERENCE used as an address is classified by the address class of its
/// resolved target, never by REFERENCE syntax ... mutation of OUTPUT ...
/// resolves state." An unbound OUTPUT reads MISSING; the address is still the
/// OUTPUT, so its mutation resolves the state effect and reaches the host.
#[test]
fn a_reference_address_is_classified_by_its_declaration_not_its_current_value() {
    let source = common::task(
        "\nOUTPUT:\n    ID: output.log\n    TYPE: STRING\n    FORMAT: format.plain_text\n",
        &[
            "ID: action.subject\nOPERATION: core.append\nTARGET: REF(output.log)\n\
           PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: \"x\"",
        ],
    );
    let mut stdlib = common::stdlib();
    let mut host = lcl_runtime::MockHost::new();
    let fixture = common::fixture(&source);
    let execution = Runtime::new(common::contracts())
        .execute_with(
            &fixture.planned,
            &fixture.checked,
            &fixture.resolved,
            &mut stdlib,
            &mut host,
        )
        .expect("the document planned");
    assert!(
        common::errors_of(&execution, "action.subject").is_empty(),
        "{:?}",
        common::errors_of(&execution, "action.subject")
    );
    let request = host
        .requests()
        .first()
        .expect("the append crossed the boundary");
    assert_eq!(
        request.possible_effects.iter().collect::<Vec<_>>(),
        vec!["state"],
        "{request:?}"
    );
}

/// The store rows' registered preconditions: "MEMORY mode permits write";
/// "when merge is FALSE, value matches the declared MEMORY type"; "when merge
/// is TRUE, the current MEMORY value and value parameter are OBJECT and the
/// computed merged OBJECT matches the declared MEMORY type"; and, for
/// `core.state_update`, "STATE mode permits write" and "value type matches".
#[test]
fn store_rows_check_mode_and_declared_type_before_effects() {
    const PAIR: &str = "\nDEFINE:\n    ID: type.pair\n    KIND: kind.type\n    BASE: OBJECT\n    \
        FIELD:\n        NAME: first\n        TYPE: INTEGER\n        REQUIRED: TRUE\n    \
        FIELD:\n        NAME: second\n        TYPE: INTEGER\n        REQUIRED: TRUE\n\
        \nMEMORY:\n    ID: memory.pair\n    TYPE: OBJECT[REF(type.pair)]\n    SCOPE: REF(scope.task)\n    \
        MODE: mode.read_write\n    VALUE:\n        first: 1\n        second: 2\n";
    const READ_ONLY: &str =
        "\nMEMORY:\n    ID: memory.frozen\n    TYPE: STRING\n    SCOPE: REF(scope.task)\n    \
        MODE: mode.read_only\n    VALUE: \"kept\"\n";
    let scope = "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n";
    let write = |target: &str, ty: &str, value: &str, merge: bool| {
        format!(
            "ID: action.write\nOPERATION: core.memory_write\nTARGET: REF({target})\n\
             PARAMETER:\n    NAME: value\n    TYPE: {ty}\n    REQUIRED: TRUE\n    VALUE: {value}{}",
            if merge {
                "\nPARAMETER:\n    NAME: merge\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    VALUE: TRUE"
            } else {
                ""
            }
        )
    };
    let cases: Vec<(&str, String, String)> = vec![
        (
            "merge FALSE requires the declared MEMORY type",
            format!("{scope}{MEMORY_DECLARATIONS_BODY}"),
            write("memory.notes", "INTEGER", "3", false),
        ),
        (
            "merge TRUE requires an OBJECT current value",
            format!("{scope}{MEMORY_DECLARATIONS_BODY}"),
            write("memory.notes", "OBJECT", "\n        first: 1", true),
        ),
        (
            "merge TRUE requires an OBJECT value parameter",
            format!("{scope}{PAIR}"),
            write("memory.pair", "STRING", "\"flat\"", true),
        ),
        (
            "merge TRUE requires the merged OBJECT to match the declared type",
            format!("{scope}{PAIR}"),
            write("memory.pair", "OBJECT", "\n        second: \"two\"", true),
        ),
        (
            "MEMORY mode must permit the write",
            format!("{scope}{READ_ONLY}"),
            write("memory.frozen", "STRING", "\"replaced\"", false),
        ),
    ];
    let mut wrong = Vec::new();
    for (name, declarations, action) in cases {
        let source = common::task(&declarations, &[&action]);
        let mut stdlib = common::stdlib().with_profiles(store_profiles());
        let mut host = lcl_runtime::MockHost::new();
        let fixture = common::fixture(&source);
        let execution = Runtime::new(common::contracts())
            .execute_with(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut stdlib,
                &mut host,
            )
            .expect("the document planned");
        let result = common::result_of(&execution, "action.write");
        let observed = (
            result.execution_errors.clone(),
            result.failure_phase.to_string(),
            result.effect_state.to_string(),
        );
        if observed
            != (
                vec!["error.operation.precondition".to_string()],
                "pre_effect".to_string(),
                "none".to_string(),
            )
        {
            wrong.push(format!("{name}: {observed:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The MEMORY declarations the store tests share, without their SCOPE.
const MEMORY_DECLARATIONS_BODY: &str =
    "\nMEMORY:\n    ID: memory.notes\n    TYPE: STRING\n    SCOPE: REF(scope.task)\n    \
     MODE: mode.read_write\n    VALUE: \"kept\"\n";

/// The storage profile decides how a store row is performed.
///
/// `operations_v0.1.0.json` gives both store rows `possible_dependencies:
/// ["host"]` and resolves "the authorized MEMORY storage profile". The shipped
/// profile declares no dependency and writes the engine-owned store in place;
/// a profile that declares the row's host dependency is an externally backed
/// store, and its write crosses the boundary, where the row's registered
/// host.constraint, execution.action and postcondition failures live.
#[test]
fn an_externally_backed_storage_profile_writes_through_the_host() {
    use lcl_capabilities::{
        profile::axes, Dependency, Determinism, Effect, Profile, Role, TargetClass,
    };
    let external = || {
        vec![Profile::builder(
            "core.memory_write",
            Role::new("storage"),
            "test.external_store",
            "1",
        )
        .serving(TargetClass::Only(vec![
            lcl_capabilities::AddressClass::Memory,
        ]))
        .determinism(Determinism::Deterministic, "test fixture")
        .axes(axes(&[Dependency::Host], &[Effect::Memory]))
        .resolving("Persist the declared store through the host that backs it.")]
    };
    let source = common::task(
        &format!(
            "\nSCOPE:\n    ID: scope.task\n    INCLUDE: REF(task.subject)\n{MEMORY_DECLARATIONS_BODY}"
        ),
        &["ID: action.write\nOPERATION: core.memory_write\nTARGET: REF(memory.notes)\n\
           PARAMETER:\n    NAME: value\n    TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: \"written\""],
    );
    let run = |profiles: Vec<Profile>, host: &mut lcl_runtime::MockHost| -> Execution {
        let mut stdlib = common::stdlib().with_profiles(profiles);
        let fixture = common::fixture(&source);
        Runtime::new(common::contracts())
            .execute_with(
                &fixture.planned,
                &fixture.checked,
                &fixture.resolved,
                &mut stdlib,
                host,
            )
            .expect("the document planned")
    };

    // The control: the shipped profile asks no host and writes in place.
    let mut host = lcl_runtime::MockHost::new();
    let execution = run(store_profiles(), &mut host);
    assert!(host.requests().is_empty(), "{:?}", host.requests());
    assert_eq!(
        execution.bindings().store("memory.notes"),
        Some(&Value::Text("written".to_string()))
    );

    // An externally backed profile crosses, and the engine's view follows the
    // profile: it holds the written value only because the host applied it.
    let mut host = lcl_runtime::MockHost::new();
    let execution = run(external(), &mut host);
    let request = host
        .requests()
        .first()
        .expect("the store crossed the boundary")
        .clone();
    assert_eq!(request.operation, "core.memory_write");
    assert_eq!(
        request.target,
        Some(Value::Reference("memory.notes".into()))
    );
    assert_eq!(
        request.possible_dependencies.iter().collect::<Vec<_>>(),
        vec!["host"]
    );
    assert!(common::errors_of(&execution, "action.write").is_empty());
    assert_eq!(
        execution.bindings().store("memory.notes"),
        Some(&Value::Text("written".to_string()))
    );

    // A host that cannot persist it leaves the previous value in place.
    let mut host = lcl_runtime::MockHost::new()
        .unavailable("core.memory_write", "test: the backing store is absent");
    let execution = run(external(), &mut host);
    assert_eq!(
        common::errors_of(&execution, "action.write"),
        vec!["error.host.constraint".to_string()]
    );
    assert_eq!(
        execution.bindings().store("memory.notes"),
        None,
        "a refused write must not update the engine's view"
    );
}

/// `core.execute` over a referenced execution unit runs that graph.
///
/// `operations_v0.1.0.json`: "A referenced TASK, PHASE, SEQUENCE, ACTION, or
/// TEST has no mandatory local process effect and resolves the final
/// determinism category and normalized transitive dependency and effect unions
/// of its reachable graph", and `result.command`: "In graph mode, started,
/// completed, exit_code, stdout, and stderr are absent ... value is present
/// exactly when the completed graph exposes one material primary result".
#[test]
fn core_execute_in_graph_mode_runs_the_referenced_unit() {
    // Both actions are members of the task, and the subject runs first: a
    // graph target is a declaration this document already carries, and
    // "no check reference or value read adds graph membership or edges".
    let document = |inner: &str| {
        format!(
            "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: example.stdlib\n    \
             NAME: \"Standard library fixture\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\
             \nDATA:\n    ID: data.subject\n    TYPE: STRING\n    VALUE: \"x\"\n\
             \nACTION:\n    ID: action.inner\n    {inner}\n\
             \nACTION:\n    ID: action.graph\n    OPERATION: core.execute\n    TARGET: REF(action.inner)\n\
             \nGOAL:\n    ID: goal.subject\n    ASSERT: TRUE\n\
             \nSUCCESS:\n    ID: success.subject\n    ALL: [TRUE]\n\
             \nTASK:\n    ID: task.subject\n    GOAL: REF(goal.subject)\n    ACTION: [REF(action.graph), REF(action.inner)]\n    SUCCESS: REF(success.subject)\n\
             \nEXECUTE:\n    REFERENCE: REF(task.subject)\n"
        )
    };
    let run = |source: &str, host: &mut lcl_runtime::MockHost| {
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
    };

    // One returning action: the graph exposes exactly one material primary.
    let mut host = lcl_runtime::MockHost::new();
    let execution = run(
        &document("OPERATION: core.return\n    TARGET: REF(data.subject)"),
        &mut host,
    );
    assert!(
        host.requests().is_empty(),
        "graph mode asks no host: {:?}",
        host.requests()
    );
    let result = common::result_of(&execution, "action.graph");
    assert_eq!(result.schema, "result.command");
    assert_eq!(
        result.fields.get("mode"),
        Some(&Value::Identifier("graph".to_string()))
    );
    for absent in ["started", "completed", "exit_code", "stdout", "stderr"] {
        assert!(
            !result.fields.contains_key(absent),
            "graph mode synthesizes no {absent}: {:?}",
            result.fields
        );
    }
    assert_eq!(
        result.fields.get("value"),
        Some(&Value::Text("x".to_string())),
        "one material primary result is bound"
    );
    assert!(
        result.observed_effects.is_empty(),
        "an effect-free graph has no local process effect: {:?}",
        result.observed_effects
    );
    assert!(common::errors_of(&execution, "action.graph").is_empty());

    // A failing graph member: the row reports error.execution.action, and the
    // member's own identifier stays in the retained evidence.
    let mut host =
        lcl_runtime::MockHost::new().unavailable("core.write", "test: no filesystem is installed");
    let execution = run(
        &document(
            "OPERATION: core.write\n    TARGET: PATH(\"/srv/report.txt\")\n    \
             PARAMETER:\n        NAME: content\n        TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: \"x\"",
        ),
        &mut host,
    );
    assert_eq!(
        common::errors_of(&execution, "action.graph"),
        vec!["error.execution.action".to_string()]
    );
    let retained: Vec<String> = execution
        .diagnostics()
        .iter()
        .map(|d| d.id.as_registry_str().to_string())
        .collect();
    assert!(
        retained.contains(&"error.host.constraint".to_string()),
        "the graph's own error is retained through the union: {retained:?}"
    );
}
