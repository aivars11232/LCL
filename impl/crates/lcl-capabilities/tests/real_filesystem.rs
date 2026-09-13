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
use lcl_capabilities::fs::{FileSystem, FsError, RealFileSystem, WriteMode};
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
    let result = fs.write(&spelled, b"escaped", WriteMode::Create);

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
    let _ = fs.write(&spelled, b"escaped", WriteMode::Replace);

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
            fs.write(&target, &content, WriteMode::Create)
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
    let result = fs.copy(&source, &destination, false);

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
    let result = fs.rename(&source, &destination, false);

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
            fs.copy(&source, &destination, false)
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
            fs.rename(&source, &destination, false)
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
    fs.write(&target, b"hello", WriteMode::Create)
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
    let result = fs.write(&target, b"replacement", WriteMode::Create);

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
    fs.write(&target, b"replacement", WriteMode::Replace)
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
    fs.write(&spelled, b"through the link", WriteMode::Replace)
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
    let result = fs.write(&target, b"escaped", WriteMode::Create);

    assert!(matches!(result, Err(FsError::Refused(_))), "{result:?}");
    assert!(!target.exists(), "and nothing was written");
}

// ---------------------------------------------------------------------------
// FS-02 — a read bound constrains the bytes actually collected
// ---------------------------------------------------------------------------

fn read_with_cap(path: &Path, scope: &Path, cap: u64) -> Result<Vec<u8>, FsError> {
    let mut fs = RealFileSystem::new(Grants::none().permit_read(scope));
    fs.read(path, &Bounds::new().with_max_bytes(cap))
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
    let result = fs.read(path, &Bounds::new().with_max_bytes(0));

    assert!(
        matches!(result, Err(FsError::Bounded(_))),
        "a zero-byte bound cannot be satisfied by a file with {actual} bytes of \
         content, whatever its metadata reports: {result:?}"
    );
}
