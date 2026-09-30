//! The project explorer: one folder listed at a time and nothing below it,
//! every folder shown, a per-folder bound, containment, folder creation, and
//! the `KIND` labels it reads once per unchanged document.

mod common;

use common::{canonical_root, example, Scratch};
use lcl_workspace::project::MAX_CHILDREN;
use lcl_workspace::Workspace;
use std::time::{Duration, SystemTime};

fn open(scratch: &Scratch) -> Workspace {
    Workspace::open(&scratch.path, canonical_root()).expect("the folder opens")
}

/// The kind the tree shows for each document listed in one folder, by
/// identity: what the page gets when that folder is unfolded.
fn kinds_in(workspace: &Workspace, folder: &str) -> Vec<(String, Option<String>)> {
    let children = workspace.children(folder).expect("it lists");
    let kinds = workspace.tree_kinds(&children);
    children
        .entries
        .into_iter()
        .zip(kinds)
        .filter(|(e, _)| !e.directory)
        .map(|(e, k)| (e.id, k))
        .collect()
}

fn kinds(workspace: &Workspace) -> Vec<(String, Option<String>)> {
    kinds_in(workspace, "")
}

fn kind_of(workspace: &Workspace, id: &str) -> Option<String> {
    let folder = id.rsplit_once('/').map_or("", |(f, _)| f);
    kinds_in(workspace, folder)
        .into_iter()
        .find(|(listed, _)| listed == id)
        .unwrap_or_else(|| panic!("{id} is not listed"))
        .1
}

/// The ids one folder lists, in the order the explorer shows them, and
/// whether the folder's bound left any out.
fn listed(workspace: &Workspace, folder: &str) -> (Vec<String>, bool) {
    let children = workspace.children(folder).expect("it lists");
    (
        children.entries.into_iter().map(|e| e.id).collect(),
        children.truncated,
    )
}

/// Put a document whose timestamps are an hour old, as a file nobody has
/// touched lately has.
fn put_old(scratch: &Scratch, id: &str, text: &str) {
    let path = scratch.put(id, text);
    let hour_ago = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(path)
        .and_then(|f| f.set_modified(hour_ago))
        .expect("the modification time is set");
}

/// Wait out the margin within which a stamp is too new to trust.
fn settle() {
    std::thread::sleep(Duration::from_millis(3200));
}

#[test]
fn an_unchanged_document_is_read_for_its_kind_once() {
    let scratch = Scratch::new("tree-kind-reuse");
    put_old(
        &scratch,
        "a.lcl",
        &example("07_DOMAIN_EXTENSION_OPERATION.lcl"),
    );
    put_old(&scratch, "b.lcl", &example("03_IMPORTING_TASK.lcl"));
    let workspace = open(&scratch);
    settle();

    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    assert_eq!(workspace.tree_kind_stats(), (2, 2));
    for _ in 0..3 {
        assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    }
    assert_eq!(
        workspace.tree_kind_stats(),
        (2, 2),
        "an unchanged file was read again"
    );
}

#[test]
fn a_rewrite_of_the_same_length_and_modification_time_is_read_again() {
    let scratch = Scratch::new("tree-kind-rewrite");
    let task = example("07_DOMAIN_EXTENSION_OPERATION.lcl");
    put_old(&scratch, "a.lcl", &task);
    let workspace = open(&scratch);
    settle();
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    let modified = std::fs::metadata(scratch.join("a.lcl"))
        .and_then(|m| m.modified())
        .unwrap();

    // Same length, and the modification time put back as it was: only the
    // inode's change time can tell, and it does.
    let retyped = task.replacen("KIND: kind.task", "KIND: kind.type", 1);
    assert_eq!(retyped.len(), task.len());
    std::fs::write(scratch.join("a.lcl"), &retyped).unwrap();
    std::fs::File::options()
        .write(true)
        .open(scratch.join("a.lcl"))
        .and_then(|f| f.set_modified(modified))
        .unwrap();
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.type"));

    // A replacing rename, as an atomic save makes, is a new file.
    let replacement = scratch.put("a.lcl.new", &task);
    std::fs::rename(replacement, scratch.join("a.lcl")).unwrap();
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
}

#[test]
fn a_file_written_just_now_is_not_cached_until_its_stamp_settles() {
    let scratch = Scratch::new("tree-kind-racy");
    scratch.put("a.lcl", &example("07_DOMAIN_EXTENSION_OPERATION.lcl"));
    let workspace = open(&scratch);
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    // Both listings read it: a same-tick rewrite could share this stamp.
    assert_eq!(workspace.tree_kind_stats(), (2, 0));
}

#[test]
fn new_and_deleted_documents_are_added_and_forgotten() {
    let scratch = Scratch::new("tree-kind-churn");
    put_old(
        &scratch,
        "a.lcl",
        &example("07_DOMAIN_EXTENSION_OPERATION.lcl"),
    );
    let workspace = open(&scratch);
    settle();
    assert_eq!(kind_of(&workspace, "a.lcl").as_deref(), Some("kind.task"));
    assert_eq!(workspace.tree_kind_stats(), (1, 1));

    put_old(&scratch, "sub/b.lcl", &example("03_IMPORTING_TASK.lcl"));
    settle();
    assert_eq!(
        kind_of(&workspace, "sub/b.lcl").as_deref(),
        Some("kind.task")
    );
    assert_eq!(workspace.tree_kind_stats(), (2, 2));

    std::fs::remove_file(scratch.join("a.lcl")).unwrap();
    let listed = kinds(&workspace);
    assert!(!listed.iter().any(|(id, _)| id == "a.lcl"));
    assert_eq!(
        workspace.tree_kind_stats(),
        (2, 1),
        "a deleted document stayed cached"
    );
}

#[test]
fn malformed_oversized_and_unreadable_documents_have_no_kind() {
    let scratch = Scratch::new("tree-kind-null");
    put_old(&scratch, "broken.lcl", "SPECIFICATION:\n    KIND\n\u{0}\n");
    put_old(&scratch, "plain.lcl.txt", "just words, no specification\n");
    let big = format!(
        "{}\n#{}\n",
        example("07_DOMAIN_EXTENSION_OPERATION.lcl"),
        "x".repeat(1 << 20)
    );
    put_old(&scratch, "big.lcl", &big);
    put_old(
        &scratch,
        "locked.lcl",
        &example("07_DOMAIN_EXTENSION_OPERATION.lcl"),
    );
    #[cfg(unix)]
    let locked = {
        use std::os::unix::fs::PermissionsExt;
        let path = scratch.join("locked.lcl");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads through any permission; the check means nothing then.
        std::fs::read(&path).is_err()
    };
    let workspace = open(&scratch);
    let listed = kinds(&workspace);
    let kind = |id: &str| listed.iter().find(|(l, _)| l == id).unwrap().1.clone();
    assert_eq!(kind("broken.lcl"), None);
    assert_eq!(kind("plain.lcl.txt"), None);
    assert_eq!(kind("big.lcl"), None);
    #[cfg(unix)]
    if locked {
        assert_eq!(kind("locked.lcl"), None);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            scratch.join("locked.lcl"),
            std::fs::Permissions::from_mode(0o600),
        );
    }
}

#[test]
fn a_folder_lists_its_direct_children_folders_first_and_nothing_below() {
    let scratch = Scratch::new("tree-children");
    scratch.put("main.lcl.txt", "LCL:\n");
    scratch.put("rules/behavior.lcl.txt", "LCL:\n");
    scratch.put("rules/security.lcl.txt", "LCL:\n");
    scratch.put("tasks/task_001.lcl.txt", "LCL:\n");
    scratch.put("tasks/archive/old.lcl", "LCL:\n");
    scratch.put("outputs/result.json", "{}");
    scratch.put("logs/run.log", "ran\n");
    scratch.put("notes.txt", "not a document");
    std::fs::create_dir(scratch.join("empty-folder")).unwrap();
    let workspace = open(&scratch);
    // The root: every folder, empty or of other files, then the documents.
    // Nothing inside any folder is listed or read.
    assert_eq!(
        listed(&workspace, ""),
        (
            [
                "empty-folder",
                "logs",
                "outputs",
                "rules",
                "tasks",
                "main.lcl.txt"
            ]
            .map(String::from)
            .to_vec(),
            false
        )
    );
    assert_eq!(
        listed(&workspace, "rules").0,
        ["rules/behavior.lcl.txt", "rules/security.lcl.txt"]
    );
    assert_eq!(
        listed(&workspace, "tasks").0,
        ["tasks/archive", "tasks/task_001.lcl.txt"]
    );
    assert_eq!(
        listed(&workspace, "tasks/archive").0,
        ["tasks/archive/old.lcl"]
    );
    // A folder of other files is a folder; the other files are not documents.
    assert_eq!(listed(&workspace, "outputs").0, Vec::<String>::new());
    assert_eq!(listed(&workspace, "empty-folder").0, Vec::<String>::new());
    let root = workspace.children("").unwrap();
    assert!(root.entries.iter().all(|e| e.name == e.id));
    assert_eq!(
        workspace.children("tasks").unwrap().entries[1].name,
        "task_001.lcl.txt"
    );
}

#[test]
fn a_folder_that_is_not_unfolded_is_never_read() {
    let scratch = Scratch::new("tree-lazy");
    scratch.put("main.lcl", "LCL:\n");
    scratch.put("huge/locked/secret.lcl", "LCL:\n");
    for i in 0..50 {
        scratch.put(&format!("huge/d{i:02}/x.lcl"), "LCL:\n");
    }
    #[cfg(unix)]
    let unreadable = {
        use std::os::unix::fs::PermissionsExt;
        let locked = scratch.join("huge/locked");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        // Root reads through any permission; the check means nothing then.
        std::fs::read_dir(&locked).is_err()
    };
    let workspace = open(&scratch);
    // The root and `huge` list without touching what is below them, so the
    // folder nobody can read breaks nothing until it is unfolded itself.
    assert_eq!(listed(&workspace, "").0, ["huge", "main.lcl"]);
    let (inside, truncated) = listed(&workspace, "huge");
    assert_eq!(inside.len(), 51);
    assert!(!truncated);
    #[cfg(unix)]
    if unreadable {
        let refused = workspace.children("huge/locked");
        assert!(refused.is_err(), "an unreadable folder was listed");
        assert!(refused.unwrap_err().to_string().contains("not readable"));
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            scratch.join("huge/locked"),
            std::fs::Permissions::from_mode(0o700),
        );
    }
}

#[test]
fn the_bound_is_per_folder_and_keeps_the_first_entries() {
    let scratch = Scratch::new("tree-bound");
    scratch.put("main.lcl", "");
    for i in 0..=MAX_CHILDREN {
        scratch.put(&format!("big/d{i:05}.lcl"), "");
    }
    for i in 0..MAX_CHILDREN {
        scratch.put(&format!("exact/d{i:05}.lcl"), "");
    }
    scratch.put("small/one.lcl", "");
    let workspace = open(&scratch);
    // The root is complete: the bound is about the folder that exceeds it.
    assert_eq!(
        listed(&workspace, ""),
        (
            ["big", "exact", "small", "main.lcl"]
                .map(String::from)
                .to_vec(),
            false
        )
    );
    let big = workspace.children("big").unwrap();
    assert_eq!(big.entries.len(), MAX_CHILDREN);
    assert!(big.truncated);
    assert_eq!(big.entries[0].id, "big/d00000.lcl");
    assert_eq!(
        big.entries[MAX_CHILDREN - 1].id,
        format!("big/d{:05}.lcl", MAX_CHILDREN - 1)
    );
    let exact = workspace.children("exact").unwrap();
    assert_eq!(exact.entries.len(), MAX_CHILDREN);
    assert!(
        !exact.truncated,
        "a folder of exactly the bound is complete"
    );
    assert_eq!(
        listed(&workspace, "small"),
        (vec!["small/one.lcl".to_string()], false)
    );
}

#[test]
fn a_listing_never_leaves_the_project() {
    let scratch = Scratch::new("tree-contained");
    let outside = Scratch::new("tree-contained-outside");
    outside.put("private/leak.lcl", "LCL:\n");
    scratch.put("main.lcl", "LCL:\n");
    scratch.put("inner/kept.lcl", "LCL:\n");
    std::fs::create_dir(scratch.join(".git")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("private"), scratch.join("linked-dir")).unwrap();
        std::os::unix::fs::symlink(outside.join("private/leak.lcl"), scratch.join("linked.lcl"))
            .unwrap();
        std::os::unix::fs::symlink(scratch.join("inner"), scratch.join("alias")).unwrap();
    }
    let workspace = open(&scratch);
    let (root, _) = listed(&workspace, "");
    assert!(!root.iter().any(|id| id.starts_with("linked")), "{root:?}");
    assert!(!root.iter().any(|id| id.starts_with('.')), "{root:?}");
    #[cfg(unix)]
    {
        assert!(
            root.contains(&"alias".to_string()),
            "a link inside the project is listed"
        );
        assert_eq!(listed(&workspace, "alias").0, ["alias/kept.lcl"]);
        assert!(workspace.children("linked-dir").is_err());
    }
    for bad in [
        "..",
        "../",
        "/",
        "/etc",
        "inner/../..",
        ".git",
        "inner/../../tree-contained-outside",
    ] {
        assert!(workspace.children(bad).is_err(), "{bad} was listed");
    }
    assert!(workspace.children("missing").is_err());
    assert!(
        workspace.children("main.lcl").is_err(),
        "a document is not a folder"
    );
    assert!(!outside.join("private/anything").exists());
}

#[test]
fn a_folder_is_made_where_it_was_asked_for_and_shows_at_once() {
    let scratch = Scratch::new("tree-mkdir");
    scratch.put("main.lcl", "LCL:\n");
    scratch.put("tasks/task_001.lcl", "LCL:\n");
    let workspace = open(&scratch);
    assert_eq!(workspace.create_folder("planning").unwrap(), "planning");
    assert!(scratch.join("planning").is_dir());
    assert_eq!(listed(&workspace, "").0, ["planning", "tasks", "main.lcl"]);
    assert_eq!(listed(&workspace, "planning").0, Vec::<String>::new());
    assert_eq!(
        workspace.create_folder("tasks/archive/").unwrap(),
        "tasks/archive"
    );
    assert_eq!(
        listed(&workspace, "tasks").0,
        ["tasks/archive", "tasks/task_001.lcl"]
    );
    // A document made in the new folder is listed there, and only there.
    workspace
        .create_document("planning/phase_1.lcl", "LCL:\n")
        .unwrap();
    assert_eq!(listed(&workspace, "planning").0, ["planning/phase_1.lcl"]);
    // Refused: a taken name, a dot name, a parent nobody made, and anything
    // outside the project; nothing else is created.
    for bad in [
        "planning",
        "tasks/task_001.lcl",
        ".hidden",
        "planning/.x",
        "nowhere/deep",
        "../out",
        "/tmp/out",
        "",
    ] {
        assert!(workspace.create_folder(bad).is_err(), "{bad} was created");
    }
    assert!(!scratch.join("nowhere").exists());
    assert!(!scratch.path.parent().unwrap().join("out").exists());
    assert_eq!(listed(&workspace, "").0, ["planning", "tasks", "main.lcl"]);
}
