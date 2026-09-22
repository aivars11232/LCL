//! `RealFileSystem` against a real filesystem, at the moment of the operation.
//!
//! FS-01 and FS-02. A grant decides what a document is allowed to reach, and
//! the adapter is the only thing standing between that decision and the actual
//! syscall. Two properties are asserted here that a lexical check cannot give:
//!
//! * where the bytes land is decided by the operation, not by what the path
//!   resolved to a moment earlier. A final component that is a symbolic link
//!   to somewhere outside the grant must not become an unauthorized write, and
//!   a create that must not replace anything must actually not replace it,
//!   including against another writer racing it;
//! * a read bound constrains the bytes this process actually collects, not the
//!   size some earlier `stat` reported.
//!
//! Nothing is mocked. A memory double would only agree with itself, and the
//! thing under test is what the kernel does with two descriptors and a link.

use lcl_capabilities::bounds::Bounds;
use lcl_capabilities::fs::{FileSystem, FsError, Location, RealFileSystem, WriteMode};
use lcl_capabilities::grant::Grants;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// One owned directory tree, removed when the case ends.
///
/// `env::temp_dir` honours `TMPDIR`, which the run sets to owned disk-backed
/// scratch. Nothing here writes outside this directory.
struct Owned(PathBuf);

impl Owned {
    fn new(name: &str) -> Owned {
        let path = std::env::temp_dir().join(format!(
            "lcl-fs-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("an owned scratch directory");
        Owned(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    /// A granted scope inside this tree, plus an ungranted sibling beside it.
    fn split(&self) -> (PathBuf, PathBuf) {
        let inside = self.join("granted");
        let outside = self.join("private");
        std::fs::create_dir_all(&inside).expect("granted scope");
        std::fs::create_dir_all(&outside).expect("ungranted sibling");
        (inside, outside)
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // Only this exact owned path, and a failure is visible rather than
        // swallowed.
        assert!(
            self.0.starts_with(std::env::temp_dir()),
            "refusing to clean a path outside the owned scratch root"
        );
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn adapter(scope: &Path) -> RealFileSystem {
    RealFileSystem::new(Grants::none().permit_write(scope))
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    std::os::unix::fs::symlink(target, at).expect("a symbolic link");
}

// ---------------------------------------------------------------------------
// FS-01 — containment holds at the operation, not at an earlier check
// ---------------------------------------------------------------------------

/// A create through a dangling link inside the grant must not create its
/// target outside the grant.
///
/// The link resolves to nothing, so a check that asks "does this exist?" is
/// answered "no" and a create is admitted — and then the open follows the link
/// and puts the bytes exactly where the grant said they could not go.
#[cfg(unix)]
#[test]
fn a_create_through_a_dangling_link_does_not_write_outside_the_grant() {
    let owned = Owned::new("dangling-create");
    let (inside, outside) = owned.split();
    let escape = outside.join("secret.txt");
    let spelled = inside.join("looks-local.txt");
    link(&escape, &spelled);
    assert!(!escape.exists(), "the link dangles before the operation");

    let mut fs = adapter(&inside);
    let result = fs.write((&spelled).into(), b"escaped", WriteMode::Create);

    assert!(
        !escape.exists(),
        "a create through a dangling link wrote {} — outside every granted \
         scope",
        escape.display()
    );
    assert!(
        matches!(
            result,
            Err(FsError::Refused(_)) | Err(FsError::AlreadyExists(_)) | Err(FsError::Io(_))
        ),
        "the attempt must be refused, not reported as a completed write: \
         {result:?}"
    );
}

/// The same link, with a replacing write. A `Replace` is allowed to overwrite
/// something inside the grant; it is not allowed to create something outside
/// one.
#[cfg(unix)]
#[test]
fn a_replacing_write_through_a_dangling_link_does_not_escape_the_grant() {
    let owned = Owned::new("dangling-replace");
    let (inside, outside) = owned.split();
    let escape = outside.join("secret.txt");
    let spelled = inside.join("looks-local.txt");
    link(&escape, &spelled);

    let mut fs = adapter(&inside);
    let _ = fs.write((&spelled).into(), b"escaped", WriteMode::Replace);

    assert!(
        !escape.exists(),
        "a replacing write through a dangling link created {}",
        escape.display()
    );
}

/// Two writers, both fully prepared, both told not to overwrite. Exactly one
/// may win, and the winner's bytes must be whole.
#[cfg(unix)]
#[test]
fn two_racing_creates_leave_exactly_one_winner_with_whole_content() {
    let owned = Owned::new("racing-create");
    let (inside, _outside) = owned.split();
    let target = inside.join("contended.txt");
    // Large enough that a partially written file is detectable as such.
    let first = vec![b'a'; 256 * 1024];
    let second = vec![b'b'; 256 * 1024];

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for content in [first.clone(), second.clone()] {
        let barrier = Arc::clone(&barrier);
        let scope = inside.clone();
        let target = target.clone();
        handles.push(std::thread::spawn(move || {
            let mut fs = adapter(&scope);
            barrier.wait();
            fs.write((&target).into(), &content, WriteMode::Create)
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("writer finished"))
        .collect();

    let winners = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        winners, 1,
        "a create that must not replace anything admits exactly one writer: \
         {results:?}"
    );
    let losing = results
        .iter()
        .find(|r| r.is_err())
        .expect("one writer is refused");
    assert!(
        matches!(losing, Err(FsError::AlreadyExists(_))),
        "the loser is told the target already exists: {losing:?}"
    );

    let published = std::fs::read(&target).expect("the winner published a file");
    assert!(
        published == first || published == second,
        "the published file is one writer's complete content, not a mixture \
         of {} bytes",
        published.len()
    );
}

/// A copy that must not overwrite must not overwrite, under the same race.
#[cfg(unix)]
#[test]
fn a_non_overwriting_copy_does_not_replace_an_existing_destination() {
    let owned = Owned::new("copy-no-overwrite");
    let (inside, _outside) = owned.split();
    let source = inside.join("source.txt");
    let destination = inside.join("destination.txt");
    std::fs::write(&source, b"source bytes").expect("source");
    std::fs::write(&destination, b"existing bytes").expect("destination");

    let mut fs = RealFileSystem::new(
        Grants::none()
            .permit_write(&inside)
            .permit_read(inside.clone()),
    );
    let result = fs.copy((&source).into(), (&destination).into(), false);

    assert!(
        matches!(result, Err(FsError::AlreadyExists(_))),
        "a copy with overwrite disabled is refused: {result:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("readable"),
        b"existing bytes",
        "and the existing destination is untouched"
    );
}

/// A rename that must not overwrite must not overwrite either.
#[cfg(unix)]
#[test]
fn a_non_overwriting_rename_does_not_replace_an_existing_destination() {
    let owned = Owned::new("rename-no-overwrite");
    let (inside, _outside) = owned.split();
    let source = inside.join("source.txt");
    let destination = inside.join("destination.txt");
    std::fs::write(&source, b"source bytes").expect("source");
    std::fs::write(&destination, b"existing bytes").expect("destination");

    let mut fs = adapter(&inside);
    let result = fs.rename((&source).into(), (&destination).into(), false);

    assert!(
        matches!(result, Err(FsError::AlreadyExists(_))),
        "a rename with overwrite disabled is refused: {result:?}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("readable"),
        b"existing bytes",
        "and the existing destination is untouched"
    );
}

/// The same race for `copy`. Two sources, one destination, overwrite off.
#[cfg(unix)]
#[test]
fn two_racing_copies_leave_exactly_one_winner() {
    let owned = Owned::new("racing-copy");
    let (inside, _outside) = owned.split();
    let destination = inside.join("contended.txt");
    let first = vec![b'a'; 256 * 1024];
    let second = vec![b'b'; 256 * 1024];
    let sources = [inside.join("a.txt"), inside.join("b.txt")];
    std::fs::write(&sources[0], &first).expect("source a");
    std::fs::write(&sources[1], &second).expect("source b");

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for source in sources.clone() {
        let barrier = Arc::clone(&barrier);
        let scope = inside.clone();
        let destination = destination.clone();
        handles.push(std::thread::spawn(move || {
            let mut fs = RealFileSystem::new(
                Grants::none()
                    .permit_write(&scope)
                    .permit_read(scope.clone()),
            );
            barrier.wait();
            fs.copy((&source).into(), (&destination).into(), false)
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("copier finished"))
        .collect();

    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "a copy with overwrite disabled admits exactly one writer: {results:?}"
    );
    let published = std::fs::read(&destination).expect("a destination exists");
    assert!(
        published == first || published == second,
        "the destination holds one source whole, not a mixture of {} bytes",
        published.len()
    );
}

/// The same race for `rename`.
#[cfg(unix)]
#[test]
fn two_racing_renames_leave_exactly_one_winner() {
    let owned = Owned::new("racing-rename");
    let (inside, _outside) = owned.split();
    let destination = inside.join("contended.txt");
    let first = vec![b'a'; 64 * 1024];
    let second = vec![b'b'; 64 * 1024];
    let sources = [inside.join("a.txt"), inside.join("b.txt")];
    std::fs::write(&sources[0], &first).expect("source a");
    std::fs::write(&sources[1], &second).expect("source b");

    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for source in sources.clone() {
        let barrier = Arc::clone(&barrier);
        let scope = inside.clone();
        let destination = destination.clone();
        handles.push(std::thread::spawn(move || {
            let mut fs = adapter(&scope);
            barrier.wait();
            fs.rename((&source).into(), (&destination).into(), false)
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("renamer finished"))
        .collect();

    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "a rename with overwrite disabled admits exactly one writer: {results:?}"
    );
    let published = std::fs::read(&destination).expect("a destination exists");
    assert!(
        published == first || published == second,
        "the destination holds one source whole"
    );
    let surviving = sources.iter().filter(|s| s.exists()).count();
    assert_eq!(
        surviving, 1,
        "the refused rename leaves its own source in place"
    );
}

// ---------------------------------------------------------------------------
// FS-01 controls — ordinary in-grant behaviour is unchanged
// ---------------------------------------------------------------------------

#[test]
fn an_ordinary_create_inside_the_grant_still_works() {
    let owned = Owned::new("ordinary-create");
    let (inside, _outside) = owned.split();
    let target = inside.join("new.txt");

    let mut fs = adapter(&inside);
    fs.write((&target).into(), b"hello", WriteMode::Create)
        .expect("an ordinary create inside the grant");
    assert_eq!(std::fs::read(&target).expect("readable"), b"hello");
}

#[test]
fn creating_over_an_existing_file_is_refused_and_preserves_it() {
    let owned = Owned::new("create-existing");
    let (inside, _outside) = owned.split();
    let target = inside.join("existing.txt");
    std::fs::write(&target, b"original").expect("original");

    let mut fs = adapter(&inside);
    let result = fs.write((&target).into(), b"replacement", WriteMode::Create);

    assert!(
        matches!(result, Err(FsError::AlreadyExists(_))),
        "{result:?}"
    );
    assert_eq!(std::fs::read(&target).expect("readable"), b"original");
}

#[test]
fn an_allowed_overwrite_still_replaces_the_content() {
    let owned = Owned::new("allowed-overwrite");
    let (inside, _outside) = owned.split();
    let target = inside.join("existing.txt");
    std::fs::write(&target, b"original").expect("original");

    let mut fs = adapter(&inside);
    fs.write((&target).into(), b"replacement", WriteMode::Replace)
        .expect("a replacing write is allowed to replace");
    assert_eq!(std::fs::read(&target).expect("readable"), b"replacement");
}

#[cfg(unix)]
#[test]
fn a_link_to_a_safe_in_grant_target_still_resolves_normally() {
    let owned = Owned::new("in-grant-link");
    let (inside, _outside) = owned.split();
    let real = inside.join("real.txt");
    let spelled = inside.join("alias.txt");
    std::fs::write(&real, b"original").expect("real file");
    link(&real, &spelled);

    let mut fs = adapter(&inside);
    fs.write((&spelled).into(), b"through the link", WriteMode::Replace)
        .expect("a link wholly inside the grant is ordinary");
    assert_eq!(
        std::fs::read(&real).expect("readable"),
        b"through the link",
        "and it still means the file it names"
    );
}

#[cfg(unix)]
#[test]
fn a_write_spelled_outside_the_grant_is_refused() {
    let owned = Owned::new("outside-spelled");
    let (inside, outside) = owned.split();
    let target = outside.join("secret.txt");

    let mut fs = adapter(&inside);
    let result = fs.write((&target).into(), b"escaped", WriteMode::Create);

    assert!(matches!(result, Err(FsError::Refused(_))), "{result:?}");
    assert!(!target.exists(), "and nothing was written");
}

// ---------------------------------------------------------------------------
// FS-02 — a read bound constrains the bytes actually collected
// ---------------------------------------------------------------------------

fn read_with_cap(path: &Path, scope: &Path, cap: u64) -> Result<Vec<u8>, FsError> {
    let mut fs = RealFileSystem::new(Grants::none().permit_read(scope));
    fs.read(path.into(), &Bounds::new().with_max_bytes(cap))
}

#[test]
fn an_empty_file_reads_as_empty_within_any_cap() {
    let owned = Owned::new("read-empty");
    let (inside, _outside) = owned.split();
    let target = inside.join("empty.txt");
    std::fs::write(&target, b"").expect("empty file");

    assert_eq!(read_with_cap(&target, &inside, 0).expect("empty"), b"");
    assert_eq!(read_with_cap(&target, &inside, 8).expect("empty"), b"");
}

#[test]
fn a_file_of_exactly_the_cap_is_read_whole() {
    let owned = Owned::new("read-exact");
    let (inside, _outside) = owned.split();
    let target = inside.join("exact.txt");
    std::fs::write(&target, b"0123456789").expect("ten bytes");

    assert_eq!(
        read_with_cap(&target, &inside, 10).expect("exactly the cap is allowed"),
        b"0123456789"
    );
}

#[test]
fn a_file_one_byte_past_the_cap_is_refused_and_returns_no_content() {
    let owned = Owned::new("read-over");
    let (inside, _outside) = owned.split();
    let target = inside.join("over.txt");
    std::fs::write(&target, b"0123456789A").expect("eleven bytes");

    let result = read_with_cap(&target, &inside, 10);
    assert!(
        matches!(result, Err(FsError::Bounded(_))),
        "a read past its bound is a bound, not a short answer: {result:?}"
    );
}

/// The discriminating case: metadata and content disagree.
///
/// Linux reports `st_size` zero for most of `/proc`, and the file still has
/// content. A bound applied to the reported size therefore bounds nothing at
/// all, and the adapter collects whatever the file actually produces.
#[cfg(target_os = "linux")]
#[test]
fn a_bound_constrains_the_bytes_collected_and_not_the_reported_size() {
    let path = Path::new("/proc/self/cmdline");
    let reported = std::fs::metadata(path).expect("readable").len();
    let actual = std::fs::read(path).expect("readable").len();
    assert_eq!(
        reported, 0,
        "this case needs a file whose size is not its content"
    );
    assert!(actual > 0, "and which nevertheless has content");

    let mut fs = RealFileSystem::new(Grants::none().permit_read("/proc/self"));
    let result = fs.read(path.into(), &Bounds::new().with_max_bytes(0));

    assert!(
        matches!(result, Err(FsError::Bounded(_))),
        "a zero-byte bound cannot be satisfied by a file with {actual} bytes of \
         content, whatever its metadata reports: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// PRETEST-02 F08 — a WORKSPACE root is a boundary no grant widens
// ---------------------------------------------------------------------------

/// A WORKSPACE `ws` with a sibling `outside`, under a grant of `/`.
fn workspace(owned: &Owned) -> (PathBuf, PathBuf, RealFileSystem) {
    let ws = owned.join("ws");
    let outside = owned.join("outside");
    std::fs::create_dir_all(ws.join("src")).expect("workspace");
    std::fs::create_dir_all(&outside).expect("outside");
    std::fs::write(outside.join("secret.txt"), "not yours").expect("secret");
    (
        ws,
        outside,
        RealFileSystem::new(Grants::none().permit_write("/")),
    )
}

fn within<'a>(path: &'a Path, root: &'a Path) -> Location<'a> {
    Location {
        path,
        within: Some(root),
    }
}

fn is_escape<T: std::fmt::Debug>(result: &Result<T, FsError>) -> bool {
    matches!(result, Err(FsError::Escape { .. }))
}

#[test]
fn parent_traversal_out_of_the_workspace_is_an_escape() {
    let owned = Owned::new("ws-dotdot");
    let (ws, _outside, mut fs) = workspace(&owned);
    let spelled = ws.join("src/../../outside/secret.txt");
    let read = fs.read(within(&spelled, &ws), &Bounds::new());
    assert!(is_escape(&read), "{read:?}");
    let created = ws.join("src/../../outside/new.txt");
    let write = fs.write(within(&created, &ws), b"x", WriteMode::Create);
    assert!(is_escape(&write), "{write:?}");
    assert!(!owned.join("outside/new.txt").exists());
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_workspace_is_an_escape_under_a_broad_grant() {
    let owned = Owned::new("ws-link");
    let (ws, outside, mut fs) = workspace(&owned);
    link(&outside, &ws.join("src/linked"));
    let spelled = ws.join("src/linked/secret.txt");

    let read = fs.read(within(&spelled, &ws), &Bounds::new());
    assert!(is_escape(&read), "{read:?}");
    let delete = fs.delete(within(&spelled, &ws), false);
    assert!(is_escape(&delete), "{delete:?}");
    assert!(outside.join("secret.txt").exists());

    // The same path as an absolute PATH is confined by the grant alone.
    assert_eq!(
        fs.read(spelled.as_path().into(), &Bounds::new())
            .expect("granted"),
        b"not yours"
    );
}

#[cfg(unix)]
#[test]
fn a_new_destination_under_a_symlinked_ancestor_is_an_escape() {
    let owned = Owned::new("ws-ancestor");
    let (ws, outside, mut fs) = workspace(&owned);
    link(&outside, &ws.join("src/linked"));
    let created = ws.join("src/linked/deeper/new.txt");

    let write = fs.write(within(&created, &ws), b"x", WriteMode::Replace);
    assert!(is_escape(&write), "{write:?}");
    let source = ws.join("src/a.txt");
    std::fs::write(&source, "a").expect("source");
    let copy = fs.copy(within(&source, &ws), within(&created, &ws), false);
    assert!(is_escape(&copy), "{copy:?}");
    assert!(
        !outside.join("deeper").exists(),
        "nothing was created outside"
    );
}

#[cfg(unix)]
#[test]
fn a_dangling_link_out_of_the_workspace_is_an_escape() {
    let owned = Owned::new("ws-dangling");
    let (ws, outside, mut fs) = workspace(&owned);
    let spelled = ws.join("src/looks-local.txt");
    link(&outside.join("created.txt"), &spelled);

    let write = fs.write(within(&spelled, &ws), b"x", WriteMode::Replace);
    assert!(is_escape(&write), "{write:?}");
    assert!(!outside.join("created.txt").exists());
}

#[cfg(unix)]
#[test]
fn legitimate_nested_workspace_paths_still_work() {
    let owned = Owned::new("ws-inside");
    let (ws, _outside, mut fs) = workspace(&owned);
    link(&ws.join("src"), &ws.join("alias"));
    let root_alias = owned.join("ws-alias");
    link(&ws, &root_alias);

    let nested = ws.join("src/deep/a.txt");
    fs.write(within(&nested, &ws), b"hello", WriteMode::Create)
        .expect("a new nested file inside the WORKSPACE");
    let through_link = ws.join("alias/deep/a.txt");
    assert_eq!(
        fs.read(within(&through_link, &ws), &Bounds::new())
            .expect("a link that stays inside"),
        b"hello"
    );
    let through_root = root_alias.join("src/deep/a.txt");
    assert_eq!(
        fs.read(within(&through_root, &root_alias), &Bounds::new())
            .expect("a root spelled through a link"),
        b"hello"
    );
    let root = fs.metadata(within(&ws, &ws)).expect("the root itself");
    assert!(root.is_directory);
}

#[cfg(unix)]
#[test]
fn the_workspace_does_not_replace_the_host_grant() {
    let owned = Owned::new("ws-grant");
    let (ws, outside, _) = workspace(&owned);
    let mut fs = RealFileSystem::new(Grants::none().permit_write(&outside));
    let target = ws.join("src/a.txt");
    let write = fs.write(within(&target, &ws), b"x", WriteMode::Create);
    assert!(matches!(write, Err(FsError::Refused(_))), "{write:?}");
    assert!(!target.exists());
}

// ---------------------------------------------------------------------------
// A-03 — a transfer whose two ends are one file
// ---------------------------------------------------------------------------
//
// `core.copy` promises "destination content equals source pre-state" *and*
// "source remains unchanged". `core.move` requires as a precondition that
// "resolved source and destination addresses are distinct", and promises
// "source no longer exists at original address".
//
// Neither survives two names for one file. `std::fs::copy` opens its
// destination truncating, so copying a file onto itself empties it — the
// source is not merely changed, it is gone. And POSIX `rename` on two entries
// for one file "shall return successfully and perform no other action", so a
// move reports that it relocated something while both names are still there.
//
// A path comparison alone does not find these. Two spellings and a symbolic
// link resolve to one canonical path and can be compared; a hard link is a
// second directory entry for the same inode and has its own canonical path, so
// the identity has to be the file's, not the name's.

/// A file and a second *name* for the very same file.
#[cfg(unix)]
fn aliases(owned: &Owned) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let (inside, _) = owned.split();
    let original = inside.join("original.txt");
    std::fs::write(&original, b"the original content").expect("seeded");
    let symlinked = inside.join("symlinked.txt");
    link(&original, &symlinked);
    let hard = inside.join("hard.txt");
    std::fs::hard_link(&original, &hard).expect("a hard link");
    (inside, original, symlinked, hard)
}

/// Copying a file onto itself must not destroy it.
#[cfg(unix)]
#[test]
fn a_copy_whose_ends_are_one_file_leaves_that_file_intact() {
    let owned = Owned::new("copy-same-file");
    let (inside, original, symlinked, hard) = aliases(&owned);
    let spelled = inside.join(".").join("original.txt");

    for (name, destination) in [
        ("itself", original.clone()),
        ("another spelling", spelled),
        ("a symbolic link", symlinked),
        ("a hard link", hard),
    ] {
        let mut fs = adapter(&inside);
        let _ = fs.copy((&original).into(), (&destination).into(), true);
        assert_eq!(
            std::fs::read(&original).expect("the source survives"),
            b"the original content",
            "copying onto {name} destroyed the source"
        );
        assert_eq!(
            std::fs::read(&destination).expect("the destination survives"),
            b"the original content",
            "copying onto {name} emptied the destination"
        );
    }
}

/// Moving a file onto itself must not claim a relocation that did not happen.
#[cfg(unix)]
#[test]
fn a_move_whose_ends_are_one_file_is_refused_rather_than_claimed() {
    let owned = Owned::new("move-same-file");
    let (inside, original, symlinked, hard) = aliases(&owned);

    for (name, destination) in [
        ("itself", original.clone()),
        ("a symbolic link", symlinked),
        ("a hard link", hard),
    ] {
        let mut fs = adapter(&inside);
        let result = fs.rename((&original).into(), (&destination).into(), true);
        assert!(
            result.is_err(),
            "moving onto {name} reported success while {} is still there",
            original.display()
        );
        assert!(
            original.exists(),
            "moving onto {name} was refused, so nothing moved"
        );
    }
}

/// The controls: genuinely distinct files still copy and move.
#[cfg(unix)]
#[test]
fn transfers_between_distinct_files_still_work() {
    let owned = Owned::new("distinct-transfer");
    let (inside, _) = owned.split();
    let source = inside.join("source.txt");
    let copied = inside.join("copied.txt");
    let moved = inside.join("moved.txt");
    std::fs::write(&source, b"payload").expect("seeded");

    let mut fs = adapter(&inside);
    fs.copy((&source).into(), (&copied).into(), false)
        .expect("a copy between two files");
    assert_eq!(std::fs::read(&copied).unwrap(), b"payload");
    assert!(source.exists(), "core.copy leaves its source in place");

    fs.rename((&source).into(), (&moved).into(), false)
        .expect("a move between two files");
    assert_eq!(std::fs::read(&moved).unwrap(), b"payload");
    assert!(!source.exists(), "core.move removes its source");
}

// ---------------------------------------------------------------------------
// A-04 — a multi-step transfer that failed halfway still changed something
// ---------------------------------------------------------------------------
//
// `FsError::Io` is documented as the failure reported "before the target was
// opened for modification. Nothing began, and that is established rather than
// assumed". `FsError::IoAfterChange` is the one for afterwards, where "what
// began cannot be proven not to have".
//
// A move without overwrite is two syscalls: link the destination, then unlink
// the source. If the unlink fails the link is already there — a new name for
// the file exists on disk — and reporting that as the error whose meaning is
// "nothing began" states the opposite of what happened. An overwrite copy has
// the same shape: `std::fs::copy` truncates its destination before it writes,
// and the non-overwrite path beside it already reports its failures as
// after-change.
//
// The failures below are injected at the real adapter boundary, by taking away
// the directory permission each syscall actually needs, so what is being
// tested is the conversion of a genuine kernel error.

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("mode");
}

/// An unlink that fails after the destination link was already created.
#[cfg(unix)]
#[test]
fn a_move_that_linked_but_could_not_unlink_reports_a_change() {
    let owned = Owned::new("move-half-done");
    let (inside, _) = owned.split();
    let locked = inside.join("locked");
    std::fs::create_dir_all(&locked).expect("a directory to lock");
    let source = locked.join("original.txt");
    std::fs::write(&source, b"payload").expect("seeded");
    let destination = inside.join("moved.txt");

    // Removing a file needs write permission on its *parent*; creating the
    // destination does not, because the destination is elsewhere.
    set_mode(&locked, 0o555);
    let mut fs = adapter(&inside);
    let error = fs
        .rename((&source).into(), (&destination).into(), false)
        .expect_err("the unlink cannot succeed");
    set_mode(&locked, 0o755);

    assert!(
        destination.exists(),
        "the destination link was created before the failure"
    );
    assert!(
        matches!(error, FsError::IoAfterChange { .. }),
        "a change that happened may not be reported as one that did not: {error:?}"
    );
}

/// An overwrite copy that fails after truncating says the destination changed.
///
/// The source is a directory: opening it for reading succeeds, so the
/// destination is opened and truncated, and the read then fails. That is
/// precisely the window in which the destination has already been emptied.
#[cfg(unix)]
#[test]
fn an_overwrite_copy_that_fails_after_truncating_reports_the_change() {
    let owned = Owned::new("copy-overwrite-half");
    let (inside, _) = owned.split();
    let source = inside.join("source-dir");
    std::fs::create_dir_all(&source).expect("a directory as the source");
    let destination = inside.join("destination.txt");
    std::fs::write(&destination, b"the previous content").expect("seeded");

    let mut fs = adapter(&inside);
    let error = fs
        .copy((&source).into(), (&destination).into(), true)
        .expect_err("a directory has no bytes to copy");

    assert_eq!(
        std::fs::read(&destination).expect("the destination is still there"),
        b"",
        "the destination really was truncated"
    );
    assert!(
        matches!(error, FsError::IoAfterChange { .. }),
        "the destination was emptied, so the error may not say nothing began: {error:?}"
    );
}

/// The other control: an overwrite copy that fails at the destination *open*
/// changed nothing, and says so.
#[cfg(unix)]
#[test]
fn an_overwrite_copy_that_could_not_open_its_destination_reports_no_change() {
    let owned = Owned::new("copy-overwrite-preopen");
    let (inside, _) = owned.split();
    let source = inside.join("source.txt");
    std::fs::write(&source, b"payload").expect("seeded");
    // A directory cannot be opened for writing, and the open is what would
    // have truncated anything.
    let destination = inside.join("destination");
    std::fs::create_dir_all(&destination).expect("a directory in the way");

    let mut fs = adapter(&inside);
    let error = fs
        .copy((&source).into(), (&destination).into(), true)
        .expect_err("copying onto a directory fails");

    assert!(
        matches!(error, FsError::Io(_)),
        "nothing was opened for writing, so nothing began: {error:?}"
    );
}

/// The control: a failure genuinely before anything opened stays "nothing
/// began".
///
/// Without it the repair could be "call every failure a change", which is the
/// opposite untruth.
#[cfg(unix)]
#[test]
fn a_transfer_that_failed_before_opening_anything_reports_no_change() {
    let owned = Owned::new("transfer-pre-open");
    let (inside, _) = owned.split();
    let missing = inside.join("not-here.txt");
    let destination = inside.join("destination.txt");

    let mut fs = adapter(&inside);
    let error = fs
        .copy((&missing).into(), (&destination).into(), true)
        .expect_err("an absent source cannot be copied");

    assert!(
        matches!(error, FsError::NotFound(_)),
        "nothing was opened, and the error says so: {error:?}"
    );
    assert!(!destination.exists(), "and nothing was created");
}
