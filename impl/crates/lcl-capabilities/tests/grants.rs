//! Phase A: host permission, and the escapes it must refuse.

mod common;

use lcl_capabilities::{contains, normalize, Grant, Grants, Refusal};
use std::path::{Path, PathBuf};

fn denied(result: Result<(), Refusal>) -> String {
    match result {
        Err(Refusal::Denied(detail)) => detail,
        other => panic!("expected a denial, got {other:?}"),
    }
}

fn unavailable(result: Result<(), Refusal>) -> String {
    match result {
        Err(Refusal::Unavailable(detail)) => detail,
        other => panic!("expected a limitation, got {other:?}"),
    }
}

#[test]
fn nothing_is_permitted_by_default() {
    let grants = Grants::none();
    denied(grants.decide(&Grant::ReadPath(PathBuf::from("/srv/data/a.txt"))));
    denied(grants.decide(&Grant::WritePath(PathBuf::from("/srv/data/a.txt"))));
    denied(grants.decide(&Grant::RunProgram("ls".to_string())));
    denied(grants.decide(&Grant::Network {
        host: "example.invalid".to_string(),
        secure: false,
    }));
    denied(grants.decide(&Grant::Package));
    denied(grants.decide(&Grant::Model));
    denied(grants.decide(&Grant::Human));
    denied(grants.decide(&Grant::InternalStore));
}

#[test]
fn the_internal_stores_are_granted_without_granting_anything_external() {
    // core.memory_write and core.state_update change declared LCL state the
    // engine already owns; a document that touches no external resource must
    // still run.
    let grants = Grants::internal();
    grants
        .decide(&Grant::InternalStore)
        .expect("the engine's own stores are granted");
    denied(grants.decide(&Grant::ReadPath(PathBuf::from("/srv/data/a.txt"))));
    denied(grants.decide(&Grant::RunProgram("sh".to_string())));
}

#[test]
fn a_read_scope_does_not_become_a_write_scope() {
    let grants = Grants::none().permit_read("/srv/data");
    grants
        .decide(&Grant::ReadPath(PathBuf::from("/srv/data/report.txt")))
        .expect("reading inside a read scope");
    let detail = denied(grants.decide(&Grant::WritePath(PathBuf::from("/srv/data/report.txt"))));
    assert!(
        detail.contains("read-only"),
        "the denial must say the scope was read-only, got: {detail}"
    );
}

#[test]
fn a_write_scope_also_permits_reading_it() {
    let grants = Grants::none().permit_write("/srv/data");
    grants
        .decide(&Grant::ReadPath(PathBuf::from("/srv/data/report.txt")))
        .expect("a writable scope is readable");
    grants
        .decide(&Grant::WritePath(PathBuf::from("/srv/data/report.txt")))
        .expect("a writable scope is writable");
}

#[test]
fn a_dot_dot_escape_is_refused_without_touching_the_filesystem() {
    // The path need not exist: containment is decided lexically first, so a
    // traversal is refused before anything is opened.
    let grants = Grants::none().permit_write("/srv/data");
    let detail = denied(grants.decide(&Grant::ReadPath(PathBuf::from(
        "/srv/data/../../etc/passwd",
    ))));
    assert!(
        detail.contains("outside"),
        "the denial must name the escape, got: {detail}"
    );

    // A traversal that stays inside is still inside.
    grants
        .decide(&Grant::ReadPath(PathBuf::from("/srv/data/sub/../a.txt")))
        .expect("a traversal that resolves back inside the scope is inside it");
}

#[test]
fn a_sibling_with_a_shared_prefix_is_not_inside_the_scope() {
    // Textual prefix matching would admit /srv/data-private; component
    // matching does not.
    let grants = Grants::none().permit_write("/srv/data");
    denied(grants.decide(&Grant::ReadPath(PathBuf::from("/srv/data-private/secret"))));
}

#[test]
fn the_scope_root_itself_is_inside_the_scope() {
    let grants = Grants::none().permit_read("/srv/data");
    grants
        .decide(&Grant::ReadPath(PathBuf::from("/srv/data")))
        .expect("the root is inside its own scope");
}

#[test]
fn only_named_programs_run() {
    let grants = Grants::none().permit_program("git");
    grants
        .decide(&Grant::RunProgram("git".to_string()))
        .expect("the granted program runs");
    denied(grants.decide(&Grant::RunProgram("rm".to_string())));

    let any = Grants::none().permit_any_program();
    any.decide(&Grant::RunProgram("rm".to_string()))
        .expect("an explicit any-program grant runs anything");
}

#[test]
fn only_named_network_hosts_are_reachable() {
    let grants = Grants::none().permit_network_host("Example.Invalid");
    grants
        .decide(&Grant::Network {
            host: "example.invalid".to_string(),
            secure: false,
        })
        .expect("host matching is case-insensitive, as DNS is");
    denied(grants.decide(&Grant::Network {
        host: "elsewhere.invalid".to_string(),
        secure: false,
    }));
}

#[test]
fn a_granted_host_without_tls_is_a_limitation_not_a_denial() {
    // The distinction is canonical: a refusal is error.permission.denied, and
    // a limitation is error.host.constraint, which "never changes LCL meaning".
    // Downgrading https to cleartext instead would change meaning silently.
    let grants = Grants::none().permit_network_host("example.invalid");
    let detail = unavailable(grants.decide(&Grant::Network {
        host: "example.invalid".to_string(),
        secure: true,
    }));
    assert!(
        detail.contains("TLS"),
        "the limitation must name TLS, got: {detail}"
    );

    let with_tls = Grants::none()
        .permit_network_host("example.invalid")
        .permit_tls();
    with_tls
        .decide(&Grant::Network {
            host: "example.invalid".to_string(),
            secure: true,
        })
        .expect("a TLS-capable transport reaches a granted host securely");
}

#[test]
fn an_ungranted_host_is_denied_before_tls_is_considered() {
    // Order matters: an operator learns that the host is not granted, rather
    // than being told to install TLS for a host they never permitted.
    let grants = Grants::none().permit_tls();
    denied(grants.decide(&Grant::Network {
        host: "elsewhere.invalid".to_string(),
        secure: true,
    }));
}

#[test]
fn normalization_resolves_dot_and_dot_dot_lexically() {
    assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
    assert_eq!(normalize(Path::new("a/b/../../c")), PathBuf::from("c"));
    // The filesystem treats /.. as /, and so does this.
    assert_eq!(normalize(Path::new("/../..")), PathBuf::from("/"));
    // A relative escape stays visibly escaped rather than collapsing inward.
    assert_eq!(
        normalize(Path::new("../secret")),
        PathBuf::from("../secret")
    );
}

#[test]
fn containment_compares_whole_components() {
    assert!(contains(Path::new("/srv/data"), Path::new("/srv/data")));
    assert!(contains(Path::new("/srv/data"), Path::new("/srv/data/a/b")));
    assert!(!contains(
        Path::new("/srv/data"),
        Path::new("/srv/data-private")
    ));
    assert!(!contains(Path::new("/srv/data"), Path::new("/srv")));
}

#[test]
fn several_scopes_are_searched_and_the_strongest_match_wins() {
    // A read scope and a write scope may overlap; a write inside the writable
    // one is permitted even though the read-only scope also contains it.
    let grants = Grants::none()
        .permit_read("/srv")
        .permit_write("/srv/data/out");
    grants
        .decide(&Grant::WritePath(PathBuf::from("/srv/data/out/report.txt")))
        .expect("the writable scope answers the write");
    grants
        .decide(&Grant::ReadPath(PathBuf::from("/srv/other/a.txt")))
        .expect("the read scope answers the read");
    denied(grants.decide(&Grant::WritePath(PathBuf::from("/srv/other/a.txt"))));
}
