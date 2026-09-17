//! Projects and documents on disk: opening, saving, reloading, and the
//! encoding rule the editor refuses to break.

mod common;

use common::{canonical_root, example, project_of_examples, valid_examples, Scratch};
use lcl_workspace::document;
use lcl_workspace::{Workspace, WorkspaceError};

#[test]
fn a_new_project_is_created_and_reopens() {
    let scratch = Scratch::new("create");
    let workspace =
        Workspace::create(&scratch.path, canonical_root()).expect("a new project is created");
    assert!(scratch.join("lcl.project.json").is_file());

    // And it reopens as a project, with the spec the manifest declared.
    let reopened = Workspace::open(&scratch.path, canonical_root()).expect("it reopens");
    assert_eq!(reopened.root(), workspace.root());
    let declared = reopened
        .project()
        .spec_path()
        .expect("the manifest declares a spec");
    assert_eq!(
        declared.canonicalize().unwrap(),
        canonical_root().canonicalize().unwrap()
    );
}

#[test]
fn creating_a_project_where_one_exists_is_refused_rather_than_overwriting_it() {
    let scratch = Scratch::new("no-clobber");
    Workspace::create(&scratch.path, canonical_root()).expect("first create");
    let again = Workspace::create(&scratch.path, canonical_root());
    assert!(matches!(again, Err(WorkspaceError::Io { .. })));
    assert!(again.err().unwrap().to_string().contains("open it instead"));
}

#[test]
fn a_directory_without_a_manifest_still_opens() {
    // An editor that demanded a manifest before it would show a folder of
    // documents would be useless for the first five minutes of every project.
    let scratch = Scratch::new("rootless");
    scratch.put("one.lcl", &example("01_MINIMAL_TASK.lcl"));
    let workspace = Workspace::open(&scratch.path, canonical_root()).expect("rootless opens");
    let documents = workspace.documents().expect("it lists");
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0].id, "one.lcl");
}

#[test]
fn every_canonical_example_survives_a_save_and_reload_byte_for_byte() {
    // The whole point of the encoding rule. If an editor rewrites bytes on the
    // way to disk, the user is debugging a document they did not write.
    let (_scratch, workspace) = project_of_examples("roundtrip");
    for name in valid_examples() {
        let original = example(&name);
        let loaded = workspace.read(&name).expect("reads");
        assert_eq!(loaded.text, original, "{name}: reading changed the bytes");

        let saved = workspace.save(&name, &loaded.text).expect("saves");
        assert_eq!(saved.text, original, "{name}: saving changed the bytes");

        let reloaded = workspace.read(&name).expect("reloads");
        assert_eq!(
            reloaded.text, original,
            "{name}: the round trip changed the bytes"
        );
        assert_eq!(reloaded.digest, loaded.digest, "{name}: the digest moved");
    }
}

#[test]
fn a_document_with_carriage_returns_is_refused_and_the_file_is_untouched() {
    let (scratch, workspace) = project_of_examples("crlf");
    let name = "01_MINIMAL_TASK.lcl";
    let before = std::fs::read(scratch.join(name)).expect("read");

    let crlf = example(name).replace('\n', "\r\n");
    let refused = workspace.save(name, &crlf);
    assert!(refused.is_err());
    let message = refused.err().unwrap().to_string();
    assert!(message.contains("carriage return"), "{message}");
    assert!(
        message.contains("will not rewrite your source"),
        "{message}"
    );

    let after = std::fs::read(scratch.join(name)).expect("read");
    assert_eq!(before, after, "a refused save must not touch the file");
}

#[test]
fn a_byte_order_mark_is_refused_and_not_stripped() {
    let (scratch, workspace) = project_of_examples("bom");
    let name = "01_MINIMAL_TASK.lcl";
    let before = std::fs::read(scratch.join(name)).expect("read");

    let with_bom = format!("\u{feff}{}", example(name));
    let refused = workspace.save(name, &with_bom);
    assert!(refused.is_err());
    assert!(refused
        .err()
        .unwrap()
        .to_string()
        .contains("byte order mark"));

    assert_eq!(before, std::fs::read(scratch.join(name)).expect("read"));
}

#[test]
fn a_missing_final_line_feed_is_added_and_nothing_else_is() {
    // The one repair, stated rather than silent. 02_LEXICAL/01 requires a final
    // line terminator, and every editor treats the final newline as its own.
    let (_scratch, workspace) = project_of_examples("final-lf");
    let name = "01_MINIMAL_TASK.lcl";
    let without = example(name).trim_end_matches('\n').to_string();

    let (repaired, changed) = document::admissible(&without).expect("admissible");
    assert!(changed);
    assert_eq!(repaired, format!("{without}\n"));

    let saved = workspace.save(name, &without).expect("saves");
    assert_eq!(saved.text, format!("{without}\n"));
    // Interior bytes are untouched.
    assert_eq!(saved.text.trim_end_matches('\n'), without);
}

#[test]
fn a_path_outside_the_project_is_refused_however_it_is_spelled() {
    let (_scratch, workspace) = project_of_examples("containment");
    for attempt in [
        "../escape.lcl",
        "sub/../../escape.lcl",
        "/etc/passwd",
        "./../../escape.lcl",
    ] {
        let read = workspace.read(attempt);
        assert!(read.is_err(), "{attempt} must not be readable");
        let write = workspace.save(attempt, "LCL:\n");
        assert!(write.is_err(), "{attempt} must not be writable");
    }
}

#[test]
fn a_new_document_can_be_created_inside_the_project() {
    let (_scratch, workspace) = project_of_examples("new-doc");
    let text = example("01_MINIMAL_TASK.lcl");
    let saved = workspace
        .save("nested/fresh.lcl", &text)
        .expect("a new document in a new directory");
    assert_eq!(saved.text, text);

    let listed = workspace.documents().expect("lists");
    assert!(listed
        .iter()
        .any(|e| e.id == "nested/fresh.lcl" && !e.directory));
    assert!(listed.iter().any(|e| e.id == "nested" && e.directory));
}

#[test]
fn the_document_tree_is_ordered_and_does_not_depend_on_the_filesystem() {
    let (_scratch, workspace) = project_of_examples("ordering");
    let first = workspace.documents().expect("lists");
    let second = workspace.documents().expect("lists again");
    assert_eq!(first, second, "two reads must agree");

    let ids: Vec<&str> = first.iter().map(|e| e.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "the tree is in ascending identity order");
}

#[test]
fn a_dot_directory_is_not_source_and_is_not_listed() {
    let (scratch, workspace) = project_of_examples("dotdir");
    scratch.put(".lcl-cache/vendored.lcl", &example("01_MINIMAL_TASK.lcl"));
    let listed = workspace.documents().expect("lists");
    assert!(!listed.iter().any(|e| e.id.starts_with(".lcl-cache")));
}

#[test]
fn an_atomic_save_leaves_no_temporary_behind() {
    let (scratch, workspace) = project_of_examples("atomic");
    workspace
        .save("01_MINIMAL_TASK.lcl", &example("01_MINIMAL_TASK.lcl"))
        .expect("saves");
    let leftovers: Vec<_> = std::fs::read_dir(&scratch.path)
        .expect("readable")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporaries left behind: {leftovers:?}"
    );
}

#[test]
fn saving_never_reuses_a_preexisting_temporary_symlink() {
    let scratch = Scratch::new("temporary-symlink");
    let workspace = Workspace::open(&scratch.path, canonical_root()).unwrap();
    let notes = scratch.put("notes.txt", "unrelated user text\n");
    let old_temporary = scratch.join(&format!(".fresh.lcl.txt.{}.tmp", std::process::id()));
    std::os::unix::fs::symlink(&notes, &old_temporary).unwrap();

    workspace.save("fresh.lcl.txt", "LCL:\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(&notes).unwrap(),
        "unrelated user text\n"
    );
    assert!(std::fs::symlink_metadata(old_temporary)
        .unwrap()
        .file_type()
        .is_symlink());
    assert!(!std::fs::symlink_metadata(scratch.join("fresh.lcl.txt"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(workspace.read("fresh.lcl.txt").unwrap().text, "LCL:\n");
}

#[test]
fn a_workspace_locates_its_spec_without_searching_for_one() {
    let scratch = Scratch::new("locate");
    // Explicit wins.
    let explicit = Workspace::locate_spec(&scratch.path, Some(canonical_root()))
        .expect("an explicit package is used");
    assert_eq!(explicit, canonical_root());

    // With nothing explicit, no environment variable and no manifest, it
    // refuses rather than hunting the filesystem for a package.
    if std::env::var_os("LCL_SPEC").is_none() {
        let found = Workspace::locate_spec(&scratch.path, None);
        assert!(found.is_err());
        assert!(found
            .err()
            .unwrap()
            .to_string()
            .contains("no specification package"));
    }
}

#[test]
fn a_symbolic_link_pointing_out_of_the_project_is_refused() {
    // Regression. The path resolver was rewritten to let a new document be
    // created in a directory that does not exist yet, and the thing that must
    // not have been lost in the rewrite is this: containment is decided after
    // the filesystem resolves a link, never on the spelling of the path.
    let (scratch, workspace) = project_of_examples("symlink");
    let outside = Scratch::new("symlink-target");
    outside.put("secret.lcl", &example("01_MINIMAL_TASK.lcl"));

    let link = scratch.join("escape.lcl");
    if std::os::unix::fs::symlink(outside.join("secret.lcl"), &link).is_err() {
        return; // a filesystem without links cannot be escaped through one
    }

    let read = workspace.read("escape.lcl");
    assert!(
        read.is_err(),
        "a link out of the project must not be readable"
    );
    assert!(read
        .err()
        .unwrap()
        .to_string()
        .contains("outside the project"));

    let wrote = workspace.save("escape.lcl", "LCL:\n");
    assert!(
        wrote.is_err(),
        "a link out of the project must not be writable"
    );

    // And the file it pointed at is untouched.
    assert_eq!(
        std::fs::read_to_string(outside.join("secret.lcl")).unwrap(),
        example("01_MINIMAL_TASK.lcl")
    );
}

#[test]
fn a_new_document_in_a_directory_that_does_not_exist_yet_is_still_inside() {
    // Regression for the same rewrite, from the other side: this is the case
    // that was wrongly refused, because a path with no parent on disk cannot
    // be canonicalised.
    let (_scratch, workspace) = project_of_examples("deep-new");
    let saved = workspace
        .save("a/b/c/deep.lcl", &example("01_MINIMAL_TASK.lcl"))
        .expect("a document several new directories down");
    assert_eq!(saved.id, "a/b/c/deep.lcl");
    assert_eq!(workspace.read("a/b/c/deep.lcl").unwrap().text, saved.text);
}

// ---------------------------------------------------------------------------
// Two recognised endings
// ---------------------------------------------------------------------------
//
// New documents default to `.lcl.txt`, so that a document can be shared and
// edited anywhere plain text is. `.lcl` stays fully supported: nothing renames
// a file, rewrites an import, or changes what a save writes.

#[test]
fn the_tree_lists_both_endings_and_no_other_text_file() {
    let (scratch, workspace) = common::project_of_examples("both-endings");
    scratch.put("classic.lcl", &common::example("01_MINIMAL_TASK.lcl"));
    scratch.put("modern.lcl.txt", &common::example("01_MINIMAL_TASK.lcl"));
    scratch.put("notes.txt", "an ordinary text file");
    scratch.put("readme.md", "not a document");
    scratch.put("backup.lcl.bak", "not a document either");

    let ids: Vec<String> = workspace
        .documents()
        .expect("the tree lists")
        .into_iter()
        .filter(|e| !e.directory)
        .map(|e| e.id)
        .collect();

    assert!(ids.contains(&"classic.lcl".to_string()));
    assert!(ids.contains(&"modern.lcl.txt".to_string()));
    for absent in ["notes.txt", "readme.md", "backup.lcl.bak"] {
        assert!(
            !ids.contains(&absent.to_string()),
            "{absent} is not an LCL document and must not be listed: {ids:?}"
        );
    }
}

#[test]
fn a_document_of_either_ending_reads_saves_and_reopens_identically() {
    let (scratch, workspace) = common::project_of_examples("either-ending");
    let source = common::example("01_MINIMAL_TASK.lcl");
    for name in ["classic.lcl", "modern.lcl.txt"] {
        scratch.put(name, &source);

        let read = workspace.read(name).expect("the document reads");
        assert_eq!(read.text, source, "{name} did not read back its bytes");

        let edited = source.replace("\"Double one integer\"", "\"Edited\"");
        let saved = workspace.save(name, &edited).expect("the document saves");
        assert_eq!(
            saved.id, name,
            "saving must write the name it was given, never a renamed one"
        );

        let reopened = workspace.read(name).expect("the document reopens");
        assert_eq!(reopened.text, edited, "{name} did not reopen its new bytes");
        assert_eq!(reopened.digest, saved.digest);
        assert!(
            scratch.join(name).is_file(),
            "{name} is not on disk under the name it was saved as"
        );
    }
}

#[test]
fn saving_a_classic_document_never_renames_it() {
    // The rule that makes the new default safe: creation applies it, saving
    // does not. An open `.lcl` document stays `.lcl` for as long as it exists.
    let (scratch, workspace) = common::project_of_examples("no-rename");
    let source = common::example("01_MINIMAL_TASK.lcl");
    scratch.put("legacy.lcl", &source);

    workspace.save("legacy.lcl", &source).expect("saves");
    assert!(scratch.join("legacy.lcl").is_file());
    assert!(
        !scratch.join("legacy.lcl.txt").exists(),
        "saving created a second document under the default name"
    );
}

// ---------------------------------------------------------------------------
// PRETEST-04: the tree stays inside the project, and reads are bounded
// ---------------------------------------------------------------------------

/// F14: the project tree never enumerates through a link that leaves the
/// project, and still lists one that stays inside it.
#[test]
fn the_project_tree_never_lists_through_a_link_out_of_the_project() {
    let (scratch, workspace) = project_of_examples("tree-link");
    let outside = Scratch::new("tree-link-target");
    outside.put("private/secret.lcl", &example("01_MINIMAL_TASK.lcl"));
    outside.put("loose.lcl", &example("01_MINIMAL_TASK.lcl"));
    std::os::unix::fs::symlink(outside.join("private"), scratch.join("linked-dir")).unwrap();
    std::os::unix::fs::symlink(outside.join("loose.lcl"), scratch.join("linked.lcl")).unwrap();
    scratch.put("inner/kept.lcl", &example("01_MINIMAL_TASK.lcl"));
    std::os::unix::fs::symlink(scratch.join("inner"), scratch.join("alias")).unwrap();

    let ids: Vec<String> = workspace
        .documents()
        .expect("it lists")
        .into_iter()
        .map(|entry| entry.id)
        .collect();
    assert!(!ids.iter().any(|id| id.starts_with("linked")), "{ids:?}");
    assert!(ids.contains(&"inner/kept.lcl".to_string()), "{ids:?}");
    assert!(ids.contains(&"alias/kept.lcl".to_string()), "{ids:?}");
    assert_eq!(
        workspace.read("alias/kept.lcl").expect("inside").text,
        example("01_MINIMAL_TASK.lcl")
    );
}

/// F16: a document larger than the product read limit is refused, not held.
#[test]
fn an_oversized_document_is_refused_rather_than_read() {
    let (scratch, workspace) = project_of_examples("oversized-document");
    std::fs::File::create(scratch.join("huge.lcl"))
        .and_then(|file| file.set_len(lcl_project::MAX_FILE_BYTES + 1))
        .expect("an oversized fixture");
    let Err(refused) = workspace.read("huge.lcl") else {
        panic!("an oversized document was read");
    };
    assert!(refused.to_string().contains("limit"), "{refused}");
}
