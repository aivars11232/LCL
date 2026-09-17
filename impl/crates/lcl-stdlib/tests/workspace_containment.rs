//! PRETEST-02 F08: a WORKSPACE-form PATH stays inside its WORKSPACE at the
//! real filesystem, whatever the host grants.
//!
//! `03_TYPES_AND_VALUES/04`: "The WORKSPACE form must resolve to the workspace
//! root or one of its descendants. Containment is checked on the resolved
//! target, not by textual prefix, so parent traversal, links, or equivalent
//! indirection cannot escape the root. Escape produces error.value.out_of_range."
//! `05_SEMANTICS/03`: host permission and LCL authorization are separate
//! gates, so a broad grant cannot widen the WORKSPACE.

#![cfg(unix)]

mod common;

use lcl_capabilities::{Grants, RealFileSystem};
use lcl_runtime::{Execution, Runtime};
use lcl_stdlib::{filesystem_profiles, HostAdapter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// One owned scratch tree: a WORKSPACE root and a sibling outside it.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let path = std::env::temp_dir().join(format!(
            "lcl-ws-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("ws/src")).expect("workspace");
        std::fs::create_dir_all(path.join("outside")).expect("outside");
        Tree(path)
    }

    fn at(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).expect("a symbolic link");
}

/// One action over `PATH(REF(workspace.case), relative)`.
fn document(root: &Path, relative: &str, action: &str) -> String {
    let declarations = format!(
        "\nWORKSPACE:\n    ID: workspace.case\n    PATH: PATH({:?})\n    MODE: mode.read_write\n{}",
        root.display().to_string(),
        common::data(
            "data.target",
            "PATH",
            &format!("PATH(REF(workspace.case), {relative:?})"),
        )
    );
    common::task(&declarations, &[action])
}

const READ: &str = "ID: action.run\nOPERATION: core.read\nTARGET: REF(data.target)";
const CREATE: &str = "ID: action.run\nOPERATION: core.write\nTARGET: REF(data.target)\n\
    PARAMETER:\n    NAME: content\n    TYPE: STRING\n    REQUIRED: TRUE\n    VALUE: \"x\"\n\
    PARAMETER:\n    NAME: create_if_missing\n    TYPE: BOOLEAN\n    REQUIRED: FALSE\n    VALUE: TRUE";

fn run(source: &str, grants: Grants) -> Execution {
    let mut host = HostAdapter::new(grants.clone()).with_filesystem(RealFileSystem::new(grants));
    let mut stdlib = common::stdlib().with_profiles(filesystem_profiles());
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

fn broad() -> Grants {
    Grants::none().permit_write("/")
}

#[test]
fn a_symlink_inside_the_workspace_cannot_read_outside_it_under_a_broad_grant() {
    let tree = Tree::new("read-escape");
    std::fs::write(tree.at("outside/secret.txt"), "not yours").expect("secret");
    link(&tree.at("outside"), &tree.at("ws/src/linked"));

    let execution = run(
        &document(&tree.at("ws"), "src/linked/secret.txt", READ),
        broad(),
    );
    assert_eq!(
        common::errors_of(&execution, "action.run"),
        vec!["error.value.out_of_range".to_string()]
    );
}

#[test]
fn a_new_file_under_a_symlinked_ancestor_is_not_created_outside_the_workspace() {
    let tree = Tree::new("create-escape");
    link(&tree.at("outside"), &tree.at("ws/src/linked"));

    let execution = run(
        &document(&tree.at("ws"), "src/linked/new/escaped.txt", CREATE),
        broad(),
    );
    assert_eq!(
        common::errors_of(&execution, "action.run"),
        vec!["error.value.out_of_range".to_string()]
    );
    assert!(
        !tree.at("outside/new").exists(),
        "nothing was created outside"
    );
}

#[test]
fn a_legitimate_nested_path_and_a_link_that_stays_inside_still_work() {
    let tree = Tree::new("inside");
    std::fs::create_dir_all(tree.at("ws/src/deep")).expect("nested");
    std::fs::write(tree.at("ws/src/deep/a.txt"), "hello").expect("file");
    link(&tree.at("ws/src/deep"), &tree.at("ws/src/near"));

    for relative in ["src/deep/a.txt", "src/near/a.txt"] {
        let execution = run(&document(&tree.at("ws"), relative, READ), broad());
        let result = common::result_of(&execution, "action.run");
        assert_eq!(
            result.status, "status.succeeded",
            "{relative}: {:?}",
            result.execution_errors
        );
    }

    let created = run(&document(&tree.at("ws"), "src/deep/b.txt", CREATE), broad());
    assert!(common::errors_of(&created, "action.run").is_empty());
    assert!(tree.at("ws/src/deep/b.txt").is_file());
}

#[test]
fn a_workspace_root_spelled_through_a_link_is_judged_where_it_really_is() {
    let tree = Tree::new("linked-root");
    std::fs::write(tree.at("ws/src/a.txt"), "hello").expect("file");
    link(&tree.at("ws"), &tree.at("alias"));

    let execution = run(&document(&tree.at("alias"), "src/a.txt", READ), broad());
    let result = common::result_of(&execution, "action.run");
    assert_eq!(
        result.status, "status.succeeded",
        "{:?}",
        result.execution_errors
    );
}

#[test]
fn the_host_grant_still_applies_inside_the_workspace() {
    let tree = Tree::new("host-gate");
    std::fs::write(tree.at("ws/src/a.txt"), "hello").expect("file");

    let narrow = Grants::none().permit_read(tree.at("outside"));
    let execution = run(&document(&tree.at("ws"), "src/a.txt", READ), narrow);
    assert_eq!(
        common::errors_of(&execution, "action.run"),
        vec!["error.permission.denied".to_string()]
    );
}
