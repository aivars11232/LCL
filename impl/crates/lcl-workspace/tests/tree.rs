//! The project tree at scale: the `KIND` labels it reads once per unchanged
//! document, and the limits it reports when it leaves anything out.

mod common;

use common::{canonical_root, example, Scratch};
use lcl_workspace::project::{MAX_DEPTH, MAX_ENTRIES};
use lcl_workspace::Workspace;
use std::time::{Duration, SystemTime};

fn open(scratch: &Scratch) -> Workspace {
    Workspace::open(&scratch.path, canonical_root()).expect("the folder opens")
}

/// The kind the tree shows for each listed document, by identity.
fn kinds(workspace: &Workspace) -> Vec<(String, Option<String>)> {
    let entries = workspace.documents().expect("it lists");
    let kinds = workspace.tree_kinds(&entries);
    entries
        .into_iter()
        .zip(kinds)
        .filter(|(e, _)| !e.directory)
        .map(|(e, k)| (e.id, k))
        .collect()
}

fn kind_of(workspace: &Workspace, id: &str) -> Option<String> {
    kinds(workspace)
        .into_iter()
        .find(|(listed, _)| listed == id)
        .unwrap_or_else(|| panic!("{id} is not listed"))
        .1
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

/// A project holding `count` tree entries: empty documents, flat.
fn flat(name: &str, count: usize) -> Scratch {
    let scratch = Scratch::new(name);
    for i in 0..count {
        scratch.put(&format!("d{i:05}.lcl"), "");
    }
    scratch
}

#[test]
fn a_listing_below_the_limit_is_complete() {
    let scratch = flat("tree-below", 10);
    let listing = open(&scratch).listing().unwrap();
    assert_eq!(listing.entries.len(), 10);
    assert!(!listing.truncated);
}

#[test]
fn exactly_the_limit_is_still_a_complete_listing() {
    // Folders count as entries: 4095 documents in one folder is 4096.
    let scratch = flat("tree-exact", 0);
    for i in 0..MAX_ENTRIES - 1 {
        scratch.put(&format!("dir/d{i:05}.lcl"), "");
    }
    let listing = open(&scratch).listing().unwrap();
    assert_eq!(listing.entries.len(), MAX_ENTRIES);
    assert!(
        !listing.truncated,
        "a complete tree of exactly the limit was called truncated"
    );
}

#[test]
fn one_entry_over_the_limit_is_reported() {
    let scratch = flat("tree-over", MAX_ENTRIES + 1);
    let listing = open(&scratch).listing().unwrap();
    assert_eq!(listing.entries.len(), MAX_ENTRIES);
    assert!(listing.truncated);
    let ids: Vec<&str> = listing.entries.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "a truncated tree is still in identity order");
}

#[test]
fn a_document_below_the_depth_limit_is_reported() {
    // The walk reads folders down to MAX_DEPTH below the root, so the deepest
    // listed entry is MAX_DEPTH + 1 components long.
    let at_limit = (0..MAX_DEPTH)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("/");
    let scratch = Scratch::new("tree-depth");
    scratch.put(&format!("{at_limit}/deepest.lcl"), "");
    let listing = open(&scratch).listing().unwrap();
    assert!(!listing.truncated, "the deepest listed level is complete");
    assert!(listing
        .entries
        .iter()
        .any(|e| e.id.ends_with("deepest.lcl")));

    scratch.put(&format!("{at_limit}/l{MAX_DEPTH}/too-deep.lcl"), "");
    let listing = open(&scratch).listing().unwrap();
    assert!(listing.truncated);
    assert!(!listing
        .entries
        .iter()
        .any(|e| e.id.ends_with("too-deep.lcl")));
}

/// The ids a listing shows, and whether it says it left anything out.
fn shown(scratch: &Scratch) -> (Vec<String>, bool) {
    let listing = open(scratch).listing().unwrap();
    (
        listing.entries.into_iter().map(|e| e.id).collect(),
        listing.truncated,
    )
}

#[test]
fn a_folder_is_listed_only_on_the_way_to_a_document() {
    let scratch = Scratch::new("tree-prune");
    scratch.put("main.lcl.txt", "LCL:\n");
    scratch.put("rules/rules.lcl.txt", "LCL:\n");
    scratch.put("outputs/result.json", "{}");
    scratch.put("logs/run.log", "ran\n");
    scratch.put("work/cache.bin", "\u{1}");
    scratch.put("notes.txt", "not a document");
    std::fs::create_dir(scratch.join("empty-folder")).unwrap();
    assert_eq!(
        shown(&scratch),
        (
            vec![
                "main.lcl.txt".to_string(),
                "rules".to_string(),
                "rules/rules.lcl.txt".to_string()
            ],
            false
        )
    );
    // Visibility only: nothing on disk is touched.
    for kept in [
        "outputs/result.json",
        "logs/run.log",
        "work/cache.bin",
        "empty-folder",
    ] {
        assert!(scratch.join(kept).exists(), "{kept}");
    }
}

#[test]
fn no_folder_name_is_hidden_when_a_document_is_inside() {
    let scratch = Scratch::new("tree-no-blacklist");
    scratch.put("docs/rules.lcl", "LCL:\n");
    scratch.put("data/model.lcl.txt", "LCL:\n");
    scratch.put("outputs/deep/er/x.lcl", "LCL:\n");
    scratch.put("outputs/deep/other.json", "{}");
    scratch.put("work/a/b/c.bin", "");
    std::fs::create_dir_all(scratch.join("docs/empty/inside")).unwrap();
    let (ids, truncated) = shown(&scratch);
    assert_eq!(
        ids,
        [
            "data",
            "data/model.lcl.txt",
            "docs",
            "docs/rules.lcl",
            "outputs",
            "outputs/deep",
            "outputs/deep/er",
            "outputs/deep/er/x.lcl"
        ]
    );
    assert!(!truncated);
}

#[test]
fn folders_of_other_files_never_use_up_the_entry_limit() {
    let scratch = Scratch::new("tree-clutter-budget");
    // Clutter before and after the documents, in name order.
    for i in 0..300 {
        scratch.put(&format!("a_clutter_{i:03}/data.bin"), "");
        scratch.put(&format!("zz_clutter_{i:03}/log.txt"), "");
    }
    for i in 0..MAX_ENTRIES {
        scratch.put(&format!("d{i:05}.lcl"), "");
    }
    let (ids, truncated) = shown(&scratch);
    assert_eq!(ids.len(), MAX_ENTRIES);
    assert!(ids.iter().all(|id| id.ends_with(".lcl")));
    assert!(
        !truncated,
        "folders without documents made a complete tree look cut short"
    );

    // One document more, inside a folder met at the limit: that one is cut.
    scratch.put("zz_more/x.lcl", "");
    let (ids, truncated) = shown(&scratch);
    assert_eq!(ids.len(), MAX_ENTRIES);
    assert!(truncated);
}

#[test]
fn a_deep_folder_without_documents_is_not_a_truncation() {
    let deep = (0..MAX_DEPTH + 3)
        .map(|i| format!("l{i}"))
        .collect::<Vec<_>>()
        .join("/");
    let scratch = Scratch::new("tree-deep-clutter");
    scratch.put(&format!("{deep}/data.bin"), "");
    scratch.put("main.lcl", "");
    assert_eq!(shown(&scratch), (vec!["main.lcl".to_string()], false));
}

#[test]
fn a_listing_examines_a_bounded_number_of_files() {
    let scratch = Scratch::new("tree-scan-budget");
    scratch.put("main.lcl", "");
    let bulk = scratch.join("bulk");
    std::fs::create_dir(&bulk).unwrap();
    for i in 0..=lcl_workspace::project::MAX_SCANNED {
        std::fs::File::create(bulk.join(format!("f{i}"))).unwrap();
    }
    // It stops, and says it could not look at everything.
    let (_, truncated) = shown(&scratch);
    assert!(truncated);
}
