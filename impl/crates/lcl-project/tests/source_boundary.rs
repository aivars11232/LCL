//! The source boundary: only what a document named, only inside the root.
//!
//! These are the tests the acceptance criterion "project loading never includes
//! unreferenced ambient files as normative input" rests on, and the ones that
//! prove `05_SEMANTICS/02`'s containment rule is enforced against the *resolved*
//! target rather than the spelling: "A workspace-relative PATH is legal only
//! when its resolved target is the WORKSPACE root or a descendant; textual
//! prefix alone does not establish containment."

mod common;

use common::{example, scratch, write};
use lcl_project::{FileProvider, Project};
use lcl_resolver::{SourceId, SourceProvider, SourceRef, SourceRequest, SourceUnit};

/// One request for `path`, as if written in `origin`.
fn request(origin: &str, path: &str) -> SourceRequest {
    SourceRequest {
        origin: SourceId::new(origin),
        reference: SourceRef::Path(path.to_string()),
        span: lcl_lexer::Span::new(0, 0),
    }
}

#[test]
fn a_root_document_gets_a_root_relative_identity() {
    let root = scratch("boundary_identity");
    write(root.join("src/main.lcl"), example("01_MINIMAL_TASK.lcl"));

    let provider = FileProvider::new(&root).expect("the root opens");
    let unit = provider
        .root_unit(root.join("src/main.lcl"))
        .expect("the document loads");

    assert_eq!(unit.id().as_str(), "src/main.lcl");
    assert_eq!(
        unit.digest(),
        lcl_spec::sha256::hex_digest(example("01_MINIMAL_TASK.lcl").as_bytes())
    );
}

/// The same project at two absolute paths produces identical identities.
///
/// This is what makes a diagnostic's source identity reproducible across
/// machines. An identity carrying an absolute path would make every report
/// machine-specific.
#[test]
fn identity_does_not_depend_on_where_the_project_lives() {
    let ids = ["boundary_place_a", "boundary_place_b"].map(|name| {
        let root = scratch(name);
        write(root.join("src/main.lcl"), example("01_MINIMAL_TASK.lcl"));
        let provider = FileProvider::new(&root).expect("opens");
        provider
            .root_unit(root.join("src/main.lcl"))
            .expect("loads")
            .id()
            .to_string()
    });
    assert_eq!(ids[0], ids[1]);
    assert_eq!(ids[0], "src/main.lcl");
}

/// An import is resolved relative to the unit that wrote it.
#[test]
fn a_path_import_resolves_relative_to_the_importing_unit() {
    let root = scratch("boundary_sibling");
    write(root.join("lib/util.lcl"), "x");
    write(root.join("lib/main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider
        .load(&request("lib/main.lcl", "util.lcl"))
        .expect("the sibling resolves");
    assert_eq!(unit.id().as_str(), "lib/util.lcl");
    assert_eq!(unit.bytes(), b"x");
}

/// A leading slash selects the project root, the explicit-workspace form.
#[test]
fn a_rooted_path_resolves_against_the_project_root() {
    let root = scratch("boundary_rooted");
    write(root.join("shared/util.lcl"), "x");
    write(root.join("deep/nest/main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider
        .load(&request("deep/nest/main.lcl", "/shared/util.lcl"))
        .expect("the rooted path resolves");
    assert_eq!(unit.id().as_str(), "shared/util.lcl");
}

/// A file nothing imports never enters the program.
///
/// The strongest available form of the ambient-file rule: the directory holds a
/// second document, the provider is asked for the one that was named, and the
/// record of what it supplied contains only that one.
#[test]
fn an_unreferenced_file_is_never_loaded() {
    let root = scratch("boundary_ambient");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));
    write(root.join("stray.lcl"), example("02_IMPORT_LIBRARY.lcl"));
    write(root.join("notes.txt"), "not lcl at all");
    write(root.join("nested/deeper.lcl"), "also stray");

    let provider = FileProvider::new(&root).expect("opens");
    let _ = provider.root_unit(root.join("main.lcl")).expect("loads");

    assert_eq!(
        provider.loaded(),
        vec!["main.lcl".to_string()],
        "only the named document was supplied"
    );
}

/// A `..` that leaves the root is refused.
#[test]
fn a_parent_escape_is_refused() {
    let root = scratch("boundary_escape");
    write(root.join("inside/main.lcl"), "y");
    // A real file outside the root, so the refusal is about containment and not
    // about the file being absent.
    write(
        root.parent().unwrap().join("boundary_escape_outside.lcl"),
        "x",
    );

    let provider = FileProvider::new(&root).expect("opens");
    let error = provider
        .load(&request(
            "inside/main.lcl",
            "../../boundary_escape_outside.lcl",
        ))
        .expect_err("the escape is refused");
    assert!(
        error.message().contains("outside the project root")
            || error.message().contains("above the project root"),
        "{}",
        error.message()
    );
}

/// A `..` that stays inside the root is allowed.
#[test]
fn a_parent_reference_inside_the_root_is_allowed() {
    let root = scratch("boundary_inside_parent");
    write(root.join("a/main.lcl"), "y");
    write(root.join("b/util.lcl"), "x");

    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider
        .load(&request("a/main.lcl", "../b/util.lcl"))
        .expect("staying inside the root is fine");
    assert_eq!(unit.id().as_str(), "b/util.lcl");
}

/// An absolute path outside the root is refused even though it names a real
/// file.
#[test]
fn an_absolute_outside_path_is_refused() {
    let root = scratch("boundary_absolute");
    write(root.join("main.lcl"), "y");
    let outside = scratch("boundary_absolute_other");
    write(outside.join("util.lcl"), "x");

    let provider = FileProvider::new(&root).expect("opens");
    // A leading `/` is the project-rooted form, so this reaches for
    // `<root>/<outside>/util.lcl`, which does not exist. Either way it must not
    // reach the real file outside the root.
    let error = provider
        .load(&request(
            "main.lcl",
            &outside.join("util.lcl").display().to_string(),
        ))
        .expect_err("an outside path is refused");
    assert!(!error.message().is_empty());
    assert!(provider.loaded().is_empty(), "nothing was supplied");
}

/// A symbolic link that resolves outside the root is refused.
///
/// The spelling is entirely innocent — a plain sibling name, with no `..` in
/// it — so nothing but canonicalisation can catch this. It is the exact case
/// "textual prefix alone does not establish containment" is written for.
#[cfg(unix)]
#[test]
fn a_symlink_escaping_the_root_is_refused() {
    let root = scratch("boundary_symlink");
    write(root.join("main.lcl"), "y");
    let outside = scratch("boundary_symlink_target");
    write(outside.join("secret.lcl"), "x");
    std::os::unix::fs::symlink(outside.join("secret.lcl"), root.join("innocent.lcl"))
        .expect("the link is created");

    // The link is readable through the filesystem, which is what makes the
    // refusal meaningful rather than incidental.
    assert_eq!(
        std::fs::read_to_string(root.join("innocent.lcl")).expect("readable"),
        "x"
    );

    let provider = FileProvider::new(&root).expect("opens");
    let error = provider
        .load(&request("main.lcl", "innocent.lcl"))
        .expect_err("the link is refused");
    assert!(
        error.message().contains("outside the project root"),
        "{}",
        error.message()
    );
    assert!(provider.loaded().is_empty());
}

/// A symbolic link that stays inside the root is allowed.
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_root_is_allowed() {
    let root = scratch("boundary_symlink_inside");
    write(root.join("main.lcl"), "y");
    write(root.join("real/util.lcl"), "x");
    std::os::unix::fs::symlink(root.join("real/util.lcl"), root.join("alias.lcl"))
        .expect("the link is created");

    let provider = FileProvider::new(&root).expect("opens");
    let unit = provider
        .load(&request("main.lcl", "alias.lcl"))
        .expect("a link inside the root resolves");
    // Identity is the canonical target's, so two names for one file are one
    // unit — which is what makes import cycle detection and single-load
    // memoization well defined.
    assert_eq!(unit.id().as_str(), "real/util.lcl");
}

/// A missing file is an ordinary load failure.
#[test]
fn a_missing_file_is_reported() {
    let root = scratch("boundary_missing");
    write(root.join("main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    let error = provider
        .load(&request("main.lcl", "absent.lcl"))
        .expect_err("a missing file fails");
    assert!(
        error.message().contains("not readable"),
        "{}",
        error.message()
    );
}

/// A document outside the root cannot be made a root unit.
#[test]
fn a_root_unit_must_be_inside_the_root() {
    let root = scratch("boundary_root_unit");
    let outside = scratch("boundary_root_unit_other");
    write(outside.join("main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    let error = provider
        .root_unit(outside.join("main.lcl"))
        .expect_err("outside the root is refused");
    assert!(error.detail.contains("outside the project root"));
}

/// A URI import is unresolvable without a cache, and says why.
#[test]
fn a_uri_import_is_not_fetched() {
    let root = scratch("boundary_uri");
    write(root.join("main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    let error = provider
        .load(&SourceRequest {
            origin: SourceId::new("main.lcl"),
            reference: SourceRef::Uri("https://example.invalid/lib.lcl".to_string()),
            span: lcl_lexer::Span::new(0, 0),
        })
        .expect_err("nothing is fetched");
    assert!(
        error.message().contains("does not fetch"),
        "{}",
        error.message()
    );
}

/// An empty `SOURCE` path is refused rather than treated as the root.
#[test]
fn an_empty_path_is_refused() {
    let root = scratch("boundary_empty");
    write(root.join("main.lcl"), "y");

    let provider = FileProvider::new(&root).expect("opens");
    assert!(provider.load(&request("main.lcl", "")).is_err());
}

/// A project opens from its manifest, and a directory without one does not.
#[test]
fn a_project_is_the_directory_holding_a_manifest() {
    let root = scratch("boundary_project");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));

    assert!(Project::open(&root).is_err(), "no manifest, no project");

    write(
        root.join("lcl.project.json"),
        common::manifest_with(",\n  \"entry\": \"main.lcl\""),
    );
    let project = Project::open(&root).expect("the project opens");
    assert_eq!(project.root(), root.as_path());
    assert_eq!(
        project.entry_path(),
        Some(root.join("main.lcl")),
        "a relative entry resolves against the root"
    );
    assert_eq!(project.lock_path(), root.join("lcl.lock"));
}

/// A rootless project still fixes a root and still contains.
#[test]
fn a_rootless_project_still_contains() {
    let root = scratch("boundary_rootless");
    write(root.join("main.lcl"), example("01_MINIMAL_TASK.lcl"));

    let project = Project::rootless(&root).expect("opens");
    assert!(project.manifest().entry.is_none());
    let provider = project.provider().expect("a provider");
    let unit: SourceUnit = provider
        .root_unit(root.join("main.lcl"))
        .expect("the document loads");
    assert_eq!(unit.id().as_str(), "main.lcl");
}

/// F16: a project document, named as the root unit or imported by PATH, is
/// read within the product limit.
#[test]
fn project_documents_are_read_within_the_product_limit() {
    let root = scratch("boundary_oversized");
    std::fs::File::create(root.join("huge.lcl"))
        .and_then(|file| file.set_len(lcl_project::MAX_FILE_BYTES + 1))
        .expect("an oversized fixture");
    let provider = FileProvider::new(&root).expect("opens");
    let Err(error) = provider.root_unit(root.join("huge.lcl")) else {
        panic!("an oversized root unit was read");
    };
    assert!(error.to_string().contains("limit"), "{error}");
    let Err(error) = provider.load(&request("main.lcl", "huge.lcl")) else {
        panic!("an oversized import was read");
    };
    assert!(error.message().contains("limit"), "{}", error.message());
}

/// F21: one rule gives a document its project root and identity: the nearest
/// ancestor holding a manifest, else the document's own directory.
#[test]
fn a_document_belongs_to_its_nearest_enclosing_project() {
    let root = scratch("boundary_locate");
    write(root.join("lcl.project.json"), common::manifest_with(""));
    write(
        root.join("src/deep/main.lcl"),
        example("01_MINIMAL_TASK.lcl"),
    );
    assert_eq!(
        lcl_project::locate_document(&root.join("src/deep/./main.lcl")),
        Ok((root.clone(), "src/deep/main.lcl".to_string()))
    );

    write(root.join("src/lcl.project.json"), common::manifest_with(""));
    assert_eq!(
        lcl_project::locate_document(&root.join("src/deep/main.lcl")),
        Ok((root.join("src"), "deep/main.lcl".to_string())),
        "the nearest manifest wins"
    );

    let loose = scratch("boundary_locate_loose");
    write(loose.join("one.lcl"), example("01_MINIMAL_TASK.lcl"));
    assert_eq!(
        lcl_project::locate_document(&loose.join("one.lcl")),
        Ok((loose.clone(), "one.lcl".to_string())),
        "no manifest: the document's own directory"
    );
    assert!(
        lcl_project::locate_document(&loose).is_err(),
        "a directory is not a document"
    );
}
