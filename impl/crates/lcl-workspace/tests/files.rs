//! Deleting documents and the workspace's file settings, over a real socket.
//!
//! Deletion removes only a regular `.lcl` or `.lcl.txt` file inside the
//! project, and only when it still holds the bytes that were confirmed. The
//! settings — default file type and default workspace — live in one versioned
//! file outside every project, and nothing here lets them reach a document.

mod common;

use common::{send, Running, Scratch};
use lcl_spec::json::Json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn text(value: &Json) -> &str {
    value.as_str().expect("a string")
}

fn parse(body: &str) -> Json {
    lcl_spec::json::parse(body).expect("a reply is JSON")
}

/// A project of canonical examples, served with a settings file of its own.
fn serve(name: &str) -> (Scratch, Scratch, Running) {
    let project = Scratch::new(name);
    for example in common::valid_examples() {
        project.put(&example, &common::example(&example));
    }
    let config = Scratch::new(&format!("{name}-config"));
    let workspace = lcl_workspace::Workspace::create(&project.path, common::canonical_root())
        .expect("a new project opens");
    let routes = lcl_workspace::Routes::new(Arc::new(workspace))
        .with_settings_file(Some(settings_file(&config)));
    let running = common::start(Arc::new(routes));
    (project, config, running)
}

fn settings_file(config: &Scratch) -> PathBuf {
    config.join("lcl/workspace-settings.json")
}

fn request(running: &Running, method: &str, target: &str, body: &str) -> common::Reply {
    let separator = if target.contains('?') { '&' } else { '?' };
    send(
        running.address,
        method,
        &format!("{target}{separator}t={}", running.token),
        &[],
        body.as_bytes(),
    )
}

fn digest(path: &Path) -> String {
    lcl_spec::sha256::hex_digest(&std::fs::read(path).expect("readable"))
}

fn listed(running: &Running) -> Vec<String> {
    let reply = request(running, "GET", "/api/documents", "");
    assert_eq!(reply.status, 200, "{}", reply.body);
    parse(&reply.body)
        .get("entries")
        .and_then(Json::as_array)
        .expect("entries")
        .iter()
        .map(|entry| text(entry.get("id").unwrap()).to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Deleting
// ---------------------------------------------------------------------------

#[test]
fn a_document_is_deleted_only_with_the_digest_of_what_was_confirmed() {
    let (project, _config, running) = serve("delete-digest");
    let path = project.put("doomed.lcl", "LCL:\n");
    assert!(listed(&running).contains(&"doomed.lcl".to_string()));

    let reply = request(&running, "DELETE", "/api/document?id=doomed.lcl", "");
    assert_eq!(reply.status, 400, "no digest: {}", reply.body);
    let stale = lcl_spec::sha256::hex_digest(b"what was on screen before\n");
    let reply = request(
        &running,
        "DELETE",
        &format!("/api/document?id=doomed.lcl&digest={stale}"),
        "",
    );
    assert_eq!(reply.status, 409, "changed since shown: {}", reply.body);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "LCL:\n");

    let reply = request(
        &running,
        "DELETE",
        &format!("/api/document?id=doomed.lcl&digest={}", digest(&path)),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(!path.exists());
    assert!(
        !listed(&running).contains(&"doomed.lcl".to_string()),
        "the tree still lists it"
    );

    let reply = request(
        &running,
        "DELETE",
        &format!("/api/document?id=doomed.lcl&digest={stale}"),
        "",
    );
    assert_eq!(reply.status, 404, "already gone: {}", reply.body);
}

#[test]
fn both_endings_are_deleted_and_nothing_else_is() {
    let (project, _config, running) = serve("delete-scope");
    let delete = |id: &str, digest: &str| {
        request(
            &running,
            "DELETE",
            &format!("/api/document?id={id}&digest={digest}"),
            "",
        )
    };

    for id in ["classic.lcl", "sub/shared.lcl.txt"] {
        let path = project.put(id, "LCL:\n");
        let reply = delete(id, &digest(&path));
        assert_eq!(reply.status, 200, "{id}: {}", reply.body);
        assert!(!path.exists(), "{id} is still there");
    }
    assert!(
        project.join("sub").is_dir(),
        "a document's folder is not deleted with it"
    );

    // Not documents: an ordinary text file and the manifest.
    for id in ["notes.txt", "lcl.project.json"] {
        // The manifest is already there, and is kept as it is.
        let path = if project.join(id).exists() {
            project.join(id)
        } else {
            project.put(id, "not LCL\n")
        };
        let before = std::fs::read(&path).unwrap();
        let reply = delete(id, &digest(&path));
        assert_eq!(reply.status, 400, "{id}: {}", reply.body);
        assert_eq!(std::fs::read(&path).unwrap(), before, "{id} was touched");
    }

    // A directory named like a document.
    std::fs::create_dir_all(project.join("folder.lcl/inner")).unwrap();
    let reply = delete("folder.lcl", "0");
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert!(project.join("folder.lcl/inner").is_dir());

    // A link named like a document: neither it nor its target goes.
    let target = project.put("target.lcl", "kept\n");
    std::os::unix::fs::symlink(&target, project.join("link.lcl")).unwrap();
    let reply = delete("link.lcl", &digest(&target));
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert!(std::fs::symlink_metadata(project.join("link.lcl")).is_ok());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "kept\n");

    // Out of the project, however it is spelled. The file beside the project
    // is a real document, so a refusal is the only thing that keeps it.
    let outside = Scratch::new("delete-scope-outside");
    let beside = outside.put("beside.lcl", "LCL:\n");
    let beside_digest = digest(&beside);
    let sibling = outside
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    for id in [
        format!("../{sibling}/beside.lcl"),
        format!("..%2F{sibling}%2Fbeside.lcl"),
        beside.display().to_string(),
        "sub/../../escape.lcl".to_string(),
    ] {
        let reply = delete(&id, &beside_digest);
        assert_eq!(reply.status, 400, "{id}: {}", reply.body);
    }
    assert!(
        beside.is_file(),
        "a document outside the project was deleted"
    );

    let reply = delete("never-there.lcl", &beside_digest);
    assert_eq!(reply.status, 404, "{}", reply.body);
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[test]
fn settings_start_as_the_defaults_and_survive_a_restart() {
    let (project, config, running) = serve("settings-persist");
    let reply = parse(&request(&running, "GET", "/api/settings", "").body);
    assert_eq!(reply.get("available").and_then(Json::as_bool), Some(true));
    assert_eq!(text(reply.get("default_extension").unwrap()), ".lcl");
    assert_eq!(reply.get("default_workspace"), Some(&Json::Null));
    assert_eq!(
        text(reply.get("current_workspace").unwrap()),
        project.path.canonicalize().unwrap().display().to_string()
    );

    // A folder whose name a shell would misread, which is stored as written.
    let chosen = Scratch::new("settings-persist-chosen");
    let folder = chosen.join("My LCL $HOME `x` 'q' \"d\" ;&*");
    std::fs::create_dir_all(&folder).unwrap();
    let body = format!(
        "{{\"default_extension\": \".lcl.txt\", \"default_workspace\": \"{}\"}}",
        lcl_workspace::http::escape_json(&folder.display().to_string())
    );
    let reply = request(&running, "PUT", "/api/settings", &body);
    assert_eq!(reply.status, 200, "{}", reply.body);

    // A restart: new routes over the same file.
    let workspace =
        lcl_workspace::Workspace::open(&project.path, common::canonical_root()).unwrap();
    let restarted = common::start(Arc::new(
        lcl_workspace::Routes::new(Arc::new(workspace))
            .with_settings_file(Some(settings_file(&config))),
    ));
    let reply = parse(&request(&restarted, "GET", "/api/settings", "").body);
    assert_eq!(text(reply.get("default_extension").unwrap()), ".lcl.txt");
    assert_eq!(
        text(reply.get("default_workspace").unwrap()),
        folder.display().to_string()
    );
    assert_eq!(
        reply
            .get("default_workspace_exists")
            .and_then(Json::as_bool),
        Some(true)
    );

    // Versioned JSON on disk, and nothing else beside it.
    let stored = parse(&std::fs::read_to_string(settings_file(&config)).unwrap());
    assert_eq!(stored.get("version").and_then(Json::as_u64), Some(1));
    let names: Vec<_> = std::fs::read_dir(config.join("lcl"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["workspace-settings.json"],
        "a temporary was left behind"
    );
    // A setting never reaches the project.
    assert!(!std::fs::read_to_string(project.join("lcl.project.json"))
        .unwrap()
        .contains("default_"));
}

#[test]
fn invalid_settings_are_refused_and_nothing_is_stored() {
    let (_project, config, running) = serve("settings-invalid");
    let chosen = Scratch::new("settings-invalid-file");
    let a_file = chosen.put("not-a-folder.lcl", "LCL:\n");
    for (body, status) in [
        ("{\"default_extension\": \".txt\"}".to_string(), 422),
        ("{\"default_extension\": \".LCL\"}".to_string(), 422),
        (
            "{\"default_workspace\": \"relative/folder\"}".to_string(),
            422,
        ),
        (
            "{\"default_workspace\": \"/definitely/not/here/lcl\"}".to_string(),
            422,
        ),
        (
            format!("{{\"default_workspace\": \"{}\"}}", a_file.display()),
            422,
        ),
        ("{\"default_workspace\": 7}".to_string(), 422),
        ("{not json".to_string(), 400),
        ("[\".lcl\"]".to_string(), 400),
    ] {
        let reply = request(&running, "PUT", "/api/settings", &body);
        assert_eq!(reply.status, status, "{body}: {}", reply.body);
    }
    assert!(
        !settings_file(&config).exists(),
        "a refused setting was stored"
    );
    assert!(
        !Path::new("/definitely/not/here/lcl").exists(),
        "a folder was created"
    );
}

#[test]
fn a_corrupt_or_foreign_settings_file_falls_back_to_the_defaults() {
    let (project, config, running) = serve("settings-corrupt");
    std::fs::create_dir_all(config.join("lcl")).unwrap();
    for (stored, extension) in [
        ("{not json", ".lcl"),
        (
            "{\"version\": 2, \"default_extension\": \".lcl.txt\"}",
            ".lcl",
        ),
        ("{\"version\": 1, \"default_extension\": \".txt\"}", ".lcl"),
        (
            "{\"version\": 1, \"default_extension\": \".lcl.txt\"}",
            ".lcl.txt",
        ),
    ] {
        std::fs::write(settings_file(&config), stored).unwrap();
        let reply = parse(&request(&running, "GET", "/api/settings", "").body);
        assert_eq!(
            text(reply.get("default_extension").unwrap()),
            extension,
            "{stored}"
        );
        let problem = reply.get("problem").unwrap();
        assert_eq!(
            problem == &Json::Null,
            extension == ".lcl.txt",
            "{stored}: problem {problem:?}"
        );
        // And a new document gets the ending that was actually read.
        let name = format!("after-{}", extension.len());
        let reply = request(
            &running,
            "POST",
            &format!("/api/document?id={name}"),
            "LCL:\n",
        );
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(
            text(parse(&reply.body).get("id").unwrap()),
            format!("{name}{extension}")
        );
        std::fs::remove_file(project.join(&format!("{name}{extension}"))).unwrap();
    }
}

#[test]
fn without_a_settings_file_the_defaults_apply_and_nothing_can_be_saved() {
    let (_scratch, running) = common::serve_examples("settings-none");
    let reply = parse(&request(&running, "GET", "/api/settings", "").body);
    assert_eq!(reply.get("available").and_then(Json::as_bool), Some(false));
    assert_eq!(text(reply.get("default_extension").unwrap()), ".lcl");
    let reply = request(
        &running,
        "PUT",
        "/api/settings",
        "{\"default_extension\": \".lcl.txt\"}",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
}

#[test]
fn a_new_document_takes_the_default_file_type_and_an_explicit_one_wins() {
    let (project, _config, running) = serve("settings-ending");
    let create = |written: &str| {
        let reply = request(
            &running,
            "POST",
            &format!("/api/document?id={written}"),
            "LCL:\n",
        );
        assert_eq!(reply.status, 200, "{written}: {}", reply.body);
        text(parse(&reply.body).get("id").unwrap()).to_string()
    };
    assert_eq!(create("before"), "before.lcl");
    let reply = request(
        &running,
        "PUT",
        "/api/settings",
        "{\"default_extension\": \".lcl.txt\"}",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(create("test"), "test.lcl.txt");
    assert_eq!(create("test.lcl"), "test.lcl");
    assert_eq!(create("other.lcl.txt"), "other.lcl.txt");
    assert_eq!(create("nested/deep"), "nested/deep.lcl.txt");
    for absent in [
        "test.lcl.txt.lcl",
        "test.lcl.lcl.txt",
        "other.lcl.txt.lcl.txt",
    ] {
        assert!(!project.join(absent).exists(), "{absent} must not exist");
    }
    let reply = request(
        &running,
        "PUT",
        "/api/settings",
        "{\"default_extension\": \".lcl\"}",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(create("after"), "after.lcl");
}

#[test]
fn a_folder_is_checked_without_being_listed_and_created_only_on_request() {
    let (_project, _config, running) = serve("settings-folder");
    let base = Scratch::new("settings-folder-base");
    base.put("inside/secret.lcl", "LCL:\n");
    let missing = base.join("to be made");
    let encode = |path: &Path| path.display().to_string().replace(' ', "%20");

    let reply = parse(
        &request(
            &running,
            "GET",
            &format!("/api/folder?path={}", encode(&base.path)),
            "",
        )
        .body,
    );
    assert_eq!(reply.get("directory").and_then(Json::as_bool), Some(true));
    assert!(
        !reply_mentions(&reply, "secret"),
        "a folder check listed its contents"
    );

    let reply = parse(
        &request(
            &running,
            "GET",
            &format!("/api/folder?path={}", encode(&missing)),
            "",
        )
        .body,
    );
    assert_eq!(reply.get("exists").and_then(Json::as_bool), Some(false));
    assert!(!missing.exists(), "checking a folder created it");

    let reply = request(&running, "GET", "/api/folder?path=relative/place", "");
    assert_eq!(
        parse(&reply.body).get("absolute").and_then(Json::as_bool),
        Some(false)
    );
    let reply = request(&running, "POST", "/api/folder?path=relative/place", "");
    assert_eq!(reply.status, 400, "{}", reply.body);
    let a_file = base.join("inside/secret.lcl");
    let reply = request(
        &running,
        "POST",
        &format!("/api/folder?path={}", encode(&a_file)),
        "",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);

    let reply = request(
        &running,
        "POST",
        &format!("/api/folder?path={}", encode(&missing)),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        parse(&reply.body).get("created").and_then(Json::as_bool),
        Some(true)
    );
    assert!(missing.is_dir());
}

fn reply_mentions(reply: &Json, needle: &str) -> bool {
    format!("{reply:?}").contains(needle)
}

#[test]
fn the_session_says_why_a_launch_opened_its_folder() {
    let (_scratch, running) = common::serve_examples("session-quiet");
    let session = parse(&request(&running, "GET", "/api/session", "").body);
    assert_eq!(session.get("notice"), Some(&Json::Null));

    let project = Scratch::new("session-notice");
    let workspace =
        lcl_workspace::Workspace::create(&project.path, common::canonical_root()).unwrap();
    let told = common::start(Arc::new(
        lcl_workspace::Routes::new(Arc::new(workspace))
            .with_notice(Some("The default workspace /gone does not exist.".into())),
    ));
    let session = parse(&request(&told, "GET", "/api/session", "").body);
    assert_eq!(
        text(session.get("notice").unwrap()),
        "The default workspace /gone does not exist."
    );
}
