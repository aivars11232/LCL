//! Core 0.3 Task 03, server side: revision-aware desktop saves (A14), files by
//! role, New Project, Master templates, project readiness, Run refusal before
//! admission, conversion of a standalone document and the packaged manual.

mod common;

use common::{send, Running, Scratch};
use lcl_spec::json::Json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Every document type New Document offers, in the read order, and the role
/// a file of that type declares.
const TYPES: [(&str, &str); 12] = [
    ("kind.part.description", "kind.part.description"),
    ("kind.part.context", "kind.part.context"),
    ("kind.part.definitions", "kind.part.definitions"),
    ("kind.part.rules", "kind.part.rules"),
    ("contracts", "kind.part.rules"),
    ("bindings", "kind.part.definitions"),
    ("usage", "kind.part.description"),
    ("stop_conditions", "kind.part.checks"),
    ("kind.part.task", "kind.part.task"),
    ("kind.part.checks", "kind.part.checks"),
    ("kind.part.data", "kind.part.data"),
    ("kind.part.output", "kind.part.output"),
];

fn project_spec() -> PathBuf {
    common::canonical_root()
        .parent()
        .unwrap()
        .join("LCL_Core_0.3.0")
}

fn fixture(name: &str) -> PathBuf {
    project_spec()
        .join("09_CONFORMANCE/PROJECT_FIXTURES")
        .join(name)
}

/// A workspace judging Core 0.1, 0.2 and 0.3 documents, with a configuration
/// directory of its own for Masters.
fn serve(name: &str) -> (Scratch, Scratch, Running) {
    let project = Scratch::new(name);
    let config = Scratch::new(&format!("{name}-config"));
    let workspace = lcl_workspace::Workspace::open_with_specs(
        &project.path,
        common::canonical_root(),
        Some(
            common::canonical_root()
                .parent()
                .unwrap()
                .join("LCL_Core_0.2.0"),
        ),
        Some(project_spec()),
        &[],
    )
    .expect("the workspace opens");
    let routes = lcl_workspace::Routes::new(Arc::new(workspace))
        .with_settings_file(Some(config.join("lcl/workspace-settings.json")));
    let running = common::start(Arc::new(routes));
    (project, config, running)
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

fn json(reply: &common::Reply) -> Json {
    lcl_spec::json::parse(&reply.body).unwrap_or_else(|e| panic!("{e}: {}", reply.body))
}

fn text<'a>(value: &'a Json, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("no string {key}"))
}

fn digest(bytes: &[u8]) -> String {
    lcl_spec::sha256::hex_digest(bytes)
}

fn copy_fixture(name: &str, into: &Path) {
    for entry in std::fs::read_dir(fixture(name)).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), into.join(entry.file_name())).unwrap();
    }
}

// --------------------------------------------------------------------- A14

#[test]
fn a14_a_save_must_name_its_revision_and_a_stale_one_writes_nothing() {
    let (project, _config, running) = serve("a14-stale");
    project.put("doc.lcl", "one\n");
    let base = digest(b"one\n");

    // No revision named: refused, nothing written.
    let reply = request(&running, "PUT", "/api/document?id=doc.lcl", "mine\n");
    assert_eq!(reply.status, 428, "{}", reply.body);
    assert_eq!(
        std::fs::read_to_string(project.join("doc.lcl")).unwrap(),
        "one\n"
    );

    // Another writer (the phone, an external editor) changes the file.
    std::fs::write(project.join("doc.lcl"), "theirs\n").unwrap();
    let reply = request(
        &running,
        "PUT",
        &format!("/api/document?id=doc.lcl&base={base}"),
        "mine\n",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
    let conflict = json(&reply);
    assert_eq!(conflict.get("conflict").and_then(Json::as_bool), Some(true));
    assert_eq!(conflict.get("exists").and_then(Json::as_bool), Some(true));
    assert_eq!(text(&conflict, "digest"), digest(b"theirs\n"));
    assert_eq!(
        std::fs::read_to_string(project.join("doc.lcl")).unwrap(),
        "theirs\n",
        "a stale save must write nothing"
    );

    // Keep mine: a deliberate save over exactly the revision now on disk.
    let reply = request(
        &running,
        "PUT",
        &format!("/api/document?id=doc.lcl&base={}", digest(b"theirs\n")),
        "mine\n",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        std::fs::read_to_string(project.join("doc.lcl")).unwrap(),
        "mine\n"
    );

    // Reload disk: the read route gives the current revision and digest.
    let reread = json(&request(&running, "GET", "/api/document?id=doc.lcl", ""));
    assert_eq!(text(&reread, "digest"), digest(b"mine\n"));
}

#[test]
fn a14_sequential_saves_chain_on_the_acknowledged_digest() {
    let (project, _config, running) = serve("a14-chain");
    project.put("doc.lcl", "zero\n");
    let mut base = digest(b"zero\n");
    for text_ in ["first\n", "second\n", "third\n"] {
        let reply = request(
            &running,
            "PUT",
            &format!("/api/document?id=doc.lcl&base={base}"),
            text_,
        );
        assert_eq!(reply.status, 200, "{}", reply.body);
        base = text(&json(&reply), "digest").to_string();
    }
    assert_eq!(
        std::fs::read_to_string(project.join("doc.lcl")).unwrap(),
        "third\n"
    );
    // A second save from the same stale base loses, visibly.
    let reply = request(
        &running,
        "PUT",
        &format!("/api/document?id=doc.lcl&base={}", digest(b"second\n")),
        "late\n",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert_eq!(
        std::fs::read_to_string(project.join("doc.lcl")).unwrap(),
        "third\n"
    );
}

#[test]
fn a14_a_deleted_file_is_a_conflict_not_a_recreation() {
    let (project, _config, running) = serve("a14-deleted");
    project.put("doc.lcl", "one\n");
    std::fs::remove_file(project.join("doc.lcl")).unwrap();
    let reply = request(
        &running,
        "PUT",
        &format!("/api/document?id=doc.lcl&base={}", digest(b"one\n")),
        "mine\n",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert_eq!(
        json(&reply).get("exists").and_then(Json::as_bool),
        Some(false)
    );
    assert!(!project.join("doc.lcl").exists());
}

// ------------------------------------------------------------ files by role

#[test]
fn every_role_creates_its_own_scaffold_and_the_file_is_the_preview() {
    let (project, _config, running) = serve("by-role");
    let roles = json(&request(&running, "GET", "/api/roles", ""));
    assert_eq!(roles.get("available").and_then(Json::as_bool), Some(true));
    let listed: Vec<&str> = roles
        .get("roles")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .map(|r| text(r, "role"))
        .collect();
    assert_eq!(listed, TYPES.map(|(id, _)| id));

    let mut texts = std::collections::BTreeSet::new();
    let mut declared = std::collections::BTreeMap::new();
    for (doc_type, kind) in TYPES {
        let role = doc_type;
        let stem = doc_type.rsplit('.').next().unwrap();
        for mode in ["minimal", "guided"] {
            let path = format!("{stem}_{mode}.lcl");
            let preview = json(&request(
                &running,
                "GET",
                &format!("/api/scaffold?role={role}&mode={mode}&path={path}"),
                "",
            ));
            assert_eq!(text(&preview, "role"), kind);
            assert_eq!(text(&preview, "type"), doc_type);
            assert!(text(&preview, "text").contains(&format!("KIND: {kind}")));
            let reply = request(
                &running,
                "POST",
                &format!(
                    "/api/document?id={stem}_{mode}&role={role}&mode={mode}&source=canonical&scaffold_digest={}",
                    text(&preview, "digest")
                ),
                "",
            );
            assert_eq!(reply.status, 200, "{role} {mode}: {}", reply.body);
            assert_eq!(text(&json(&reply), "id"), path);
            let written = std::fs::read_to_string(project.join(&path)).unwrap();
            assert_eq!(written, text(&preview, "text"), "{role} {mode}");
            texts.insert(written);
            declared.insert(path, kind);
        }
    }
    assert_eq!(
        texts.len(),
        TYPES.len() * 2,
        "every type and mode has its own text"
    );

    // The tree says what each file declares, never what its name suggests.
    let tree = json(&request(&running, "GET", "/api/tree", ""));
    for entry in tree.get("entries").and_then(Json::as_array).unwrap() {
        let id = text(entry, "id");
        assert_eq!(text(entry, "kind"), declared[id], "{id}");
    }

    // A role with a body, and an unknown role, are refused and write nothing.
    let reply = request(
        &running,
        "POST",
        "/api/document?id=extra&role=kind.part.task",
        "LCL:\n",
    );
    assert_eq!(reply.status, 400, "{}", reply.body);
    let reply = request(
        &running,
        "POST",
        "/api/document?id=extra&role=kind.part.story",
        "",
    );
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert!(!project.join("extra.lcl").exists());
}

#[test]
fn slot_marks_are_the_engines_and_follow_the_text() {
    let (project, _config, running) = serve("slots");
    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/document?id=house&role=kind.part.rules&mode=minimal&source=canonical&scaffold_digest={}",
            preview_digest(&running, "role=kind.part.rules&mode=minimal&source=canonical&path=house.lcl")
        ),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let written = std::fs::read_to_string(project.join("house.lcl")).unwrap();
    let slots = json(&request(
        &running,
        "POST",
        "/api/slots?id=house.lcl",
        &written,
    ));
    assert_eq!(text(&slots, "role"), "kind.part.rules");
    let required: Vec<usize> = slots
        .get("marks")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .filter(|m| text(m, "kind") == "required_slot")
        .map(|m| m.get("line").and_then(Json::as_u64).unwrap() as usize)
        .collect();
    assert!(!required.is_empty(), "{}", slots.get("marks").is_some());
    for line in &required {
        assert!(written
            .lines()
            .nth(line - 1)
            .unwrap()
            .trim_end()
            .ends_with(':'));
    }
    // Filled in, the slots are gone; a file of no role has none.
    let filled = written
        .replace("    NAME:\n", "    NAME: \"House rules\"\n")
        .replace("    VERSION:\n", "    VERSION: \"1.0.0\"\n");
    let slots = json(&request(
        &running,
        "POST",
        "/api/slots?id=house.lcl",
        &filled,
    ));
    assert!(slots
        .get("marks")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .all(|m| text(m, "kind") != "required_slot"));
    let none = json(&request(
        &running,
        "POST",
        "/api/slots?id=x.lcl",
        &common::example("01_MINIMAL_TASK.lcl"),
    ));
    assert!(none
        .get("marks")
        .and_then(Json::as_array)
        .unwrap()
        .is_empty());
}

/// The digest `GET /api/scaffold` returns for `query`: what a person saw.
fn preview_digest(running: &Running, query: &str) -> String {
    let preview = request(running, "GET", &format!("/api/scaffold?{query}"), "");
    assert_eq!(preview.status, 200, "{}", preview.body);
    text(&json(&preview), "digest").to_string()
}

/// C03-AUDIT-04: a file of a role is created only from the exact scaffold the
/// person previewed; anything else creates nothing.
#[test]
fn a_role_file_is_created_only_from_the_scaffold_that_was_previewed() {
    let (project, _config, running) = serve("scaffold-binding");
    let create = |id: &str, extra: &str, digest: Option<&str>| {
        let digest = digest.map_or(String::new(), |d| format!("&scaffold_digest={d}"));
        request(
            &running,
            "POST",
            &format!("/api/document?id={id}&role=kind.part.rules{extra}{digest}"),
            "",
        )
    };

    // Canonical preview, unchanged: created, and the file is the preview.
    let canonical = preview_digest(&running, "role=kind.part.rules&mode=guided&path=a.lcl");
    let reply = create("a", "&mode=guided", Some(&canonical));
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        digest(&std::fs::read(project.join("a.lcl")).unwrap()),
        canonical
    );

    // No preview: 428, nothing written.
    let reply = create("b", "&mode=guided", None);
    assert_eq!(reply.status, 428, "{}", reply.body);
    assert!(!project.join("b.lcl").exists());

    // Another name makes another scaffold: the old preview is refused.
    let reply = create("c", "&mode=guided", Some(&canonical));
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(!project.join("c.lcl").exists());

    // A Master, previewed and unchanged: created.
    let starter = json(&request(
        &running,
        "GET",
        "/api/master/starter?role=kind.part.rules&mode=minimal",
        "",
    ));
    let master = text(&starter, "json").replace("\"my-rules\"", "\"house\"");
    assert_eq!(
        request(&running, "PUT", "/api/master?create=1", &master).status,
        200
    );
    let seen = preview_digest(&running, "role=kind.part.rules&master=house&path=d.lcl");
    let reply = create("d", "&master=house", Some(&seen));
    assert_eq!(reply.status, 200, "{}", reply.body);

    // The Master edited between preview and Create: 409, nothing written.
    let seen = preview_digest(&running, "role=kind.part.rules&master=house&path=e.lcl");
    let edited = master
        .replace("\"name\": \"My Rules\"", "\"name\": \"Edited\"")
        .replace(
            "KIND: kind.part.rules\\n",
            "KIND: kind.part.rules\\nCOMMENT:\\n    CONTENT: \\\"edited\\\"\\n",
        );
    assert_ne!(edited, master, "the edit must change the Master's text");
    let reply = request(&running, "PUT", "/api/master?replace=house", &edited);
    assert_eq!(reply.status, 200, "{}", reply.body);
    let reply = create("e", "&master=house", Some(&seen));
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(!project.join("e.lcl").exists());

    // The role's default changed between preview and Create: 409.
    let seen = preview_digest(
        &running,
        "role=kind.part.rules&source=default&mode=guided&path=f.lcl",
    );
    let reply = request(
        &running,
        "PUT",
        "/api/masters/default?role=kind.part.rules&id=house",
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let reply = create("f", "&source=default&mode=guided", Some(&seen));
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(!project.join("f.lcl").exists());

    // A blank file needs no scaffold digest.
    let reply = request(&running, "POST", "/api/document?id=g", "LCL:\n");
    assert_eq!(reply.status, 200, "{}", reply.body);
}

// --------------------------------------------------------------- projects

#[test]
fn a_new_project_is_created_exactly_as_previewed_or_not_at_all() {
    let (project, _config, running) = serve("new-project");
    let plan = json(&request(
        &running,
        "GET",
        "/api/project/plan?folder=shop&mode=guided",
        "",
    ));
    let files = plan.get("files").and_then(Json::as_array).unwrap().to_vec();
    assert!(files.len() >= 2);
    assert_eq!(text(&plan, "entry"), "shop/main.lcl");
    assert!(!project.join("shop").exists(), "a preview writes nothing");

    // Not previewed, or previewed differently: nothing is created.
    let reply = request(&running, "POST", "/api/project?folder=shop&mode=guided", "");
    assert_eq!(reply.status, 428, "{}", reply.body);
    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/project?folder=shop&mode=minimal&plan_digest={}",
            text(&plan, "plan_digest")
        ),
        "",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(!project.join("shop").exists());

    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/project?folder=shop&mode=guided&plan_digest={}",
            text(&plan, "plan_digest")
        ),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    for file in files.iter() {
        let on_disk = std::fs::read_to_string(project.join(text(file, "path"))).unwrap();
        assert_eq!(on_disk, text(file, "text"));
    }
    // A second creation over existing files creates nothing.
    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/project?folder=shop&mode=guided&plan_digest={}",
            text(&plan, "plan_digest")
        ),
        "",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);

    // A folder outside the workspace is refused.
    let reply = request(&running, "GET", "/api/project/plan?folder=../outside", "");
    assert_eq!(reply.status, 400, "{}", reply.body);
}

fn events(running: &Running, run: &str) -> String {
    let mut socket = TcpStream::connect(running.address).unwrap();
    let head = format!(
        "GET /api/events?t={}&run={run} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\n\r\n",
        running.token,
        running.address.port()
    );
    socket.write_all(head.as_bytes()).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .unwrap();
    let mut raw = String::new();
    socket.read_to_string(&mut raw).unwrap();
    raw
}

#[test]
fn readiness_is_the_engines_and_an_incomplete_project_cannot_run() {
    let (project, _config, running) = serve("readiness");
    std::fs::create_dir(project.join("ok")).unwrap();
    copy_fixture("valid_minimal", &project.join("ok"));
    std::fs::create_dir(project.join("broken")).unwrap();
    copy_fixture("missing_required_part", &project.join("broken"));

    let ok = json(&request(
        &running,
        "GET",
        "/api/project/status?entry=ok/main.lcl",
        "",
    ));
    let record = ok.get("project").expect("a project record");
    assert_eq!(text(record, "admission"), "admitted");
    for part in record.get("parts").and_then(Json::as_array).unwrap() {
        assert_eq!(text(part, "status"), "ready");
    }

    let broken = json(&request(
        &running,
        "GET",
        "/api/project/status?entry=broken/main.lcl",
        "",
    ));
    let record = broken.get("project").expect("a project record");
    assert_eq!(record.get("complete").and_then(Json::as_bool), Some(false));
    assert_eq!(text(record, "admission"), "rejected");
    assert!(record
        .get("parts")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .any(|p| text(p, "status") == "missing"));

    // Run is refused by the engine, before any effect, whatever a UI shows.
    let entry = std::fs::read_to_string(project.join("broken/main.lcl")).unwrap();
    let reply = request(&running, "POST", "/api/run?id=broken/main.lcl", &entry);
    assert_eq!(reply.status, 200, "{}", reply.body);
    let run = text(&json(&reply), "run").to_string();
    let raw = events(&running, &run);
    assert!(
        !raw.contains("event: invocation"),
        "an effect was reached: {raw}"
    );
    assert!(raw.contains("\"admission\": \"rejected\""), "{raw}");

    // The complete project runs.
    let entry = std::fs::read_to_string(project.join("ok/main.lcl")).unwrap();
    let reply = request(&running, "POST", "/api/run?id=ok/main.lcl", &entry);
    let run = text(&json(&reply), "run").to_string();
    let raw = events(&running, &run);
    assert!(raw.contains("\"admission\": \"admitted\""), "{raw}");
}

// ---------------------------------------------------------------- Masters

#[test]
fn masters_are_validated_copied_and_never_linked() {
    let (project, config, running) = serve("masters");
    let starter = json(&request(
        &running,
        "GET",
        "/api/master/starter?role=kind.part.rules&mode=guided",
        "",
    ));
    let master_json = text(&starter, "json").replace("\"my-rules\"", "\"house-rules\"");

    // Invalid, wrong role for a default, and wrong version: refused.
    let reply = request(&running, "PUT", "/api/master?create=1", "{\"format\": 1}");
    assert_eq!(reply.status, 422, "{}", reply.body);
    let old = master_json.replace("\"core\": \"0.3.0\"", "\"core\": \"0.2.0\"");
    let reply = request(&running, "PUT", "/api/master?create=1", &old);
    assert_eq!(reply.status, 422, "{}", reply.body);

    let reply = request(&running, "PUT", "/api/master?create=1", &master_json);
    assert_eq!(reply.status, 200, "{}", reply.body);
    let reply = request(&running, "PUT", "/api/master?create=1", &master_json);
    assert_eq!(
        reply.status, 409,
        "an id in use is not overwritten: {}",
        reply.body
    );
    let reply = request(
        &running,
        "PUT",
        "/api/masters/default?role=kind.part.task&id=house-rules",
        "",
    );
    assert_eq!(reply.status, 422, "{}", reply.body);
    let reply = request(
        &running,
        "PUT",
        "/api/masters/default?role=kind.part.rules&id=house-rules",
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);

    let listed = json(&request(&running, "GET", "/api/masters", ""));
    let item = &listed.get("masters").and_then(Json::as_array).unwrap()[0];
    assert_eq!(text(item, "id"), "house-rules");
    assert_eq!(item.get("valid").and_then(Json::as_bool), Some(true));

    // The default Master wins over the canonical scaffold, and the file is a copy.
    let master_text = lcl_protocol::scaffold::parse_master(&master_json).unwrap();
    let lcl_protocol::scaffold::Content::Text(master_text) = master_text.content else {
        panic!("a role Master")
    };
    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/document?id=rules&role=kind.part.rules&scaffold_digest={}",
            preview_digest(&running, "role=kind.part.rules&path=rules.lcl")
        ),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let made = std::fs::read_to_string(project.join("rules.lcl")).unwrap();
    assert_eq!(made, master_text);
    // Canonical on request still gives the canonical scaffold.
    let canonical = json(&request(
        &running,
        "GET",
        "/api/scaffold?role=kind.part.rules&mode=minimal&source=canonical&path=rules.lcl",
        "",
    ));
    assert_eq!(text(canonical.get("origin").unwrap(), "kind"), "canonical");
    assert_ne!(text(&canonical, "text"), master_text);

    // Editing the Master leaves the created file alone, and the other way round.
    let edited = master_json.replace("\"My Rules\"", "\"House rules v2\"");
    let reply = request(&running, "PUT", "/api/master?replace=house-rules", &edited);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        std::fs::read_to_string(project.join("rules.lcl")).unwrap(),
        made
    );
    std::fs::write(project.join("rules.lcl"), format!("{made}# edited\n")).unwrap();
    let stored = config.join("lcl/masters/house-rules.json");
    assert_eq!(std::fs::read_to_string(&stored).unwrap(), edited);

    // Deleting it leaves the file, and clears the default.
    let reply = request(&running, "DELETE", "/api/master?id=house-rules", "");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(project.join("rules.lcl").exists());
    let roles = json(&request(&running, "GET", "/api/roles", ""));
    assert!(roles
        .get("roles")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .all(|r| r
            .get("default_master")
            .is_some_and(|d| d.as_str().is_none())));
}

// ------------------------------------------------------------- conversion

#[test]
fn a_standalone_task_converts_into_an_admitted_project_and_stays_untouched() {
    let (project, _config, running) = serve("convert");
    let original = common::example("01_MINIMAL_TASK.lcl");
    project.put("double.lcl", &original);

    let plan = json(&request(
        &running,
        "GET",
        "/api/convert/plan?id=double.lcl&folder=double",
        "",
    ));
    let files = plan.get("files").and_then(Json::as_array).unwrap().to_vec();
    assert_eq!(files.len(), 2);
    assert!(!project.join("double").exists(), "a preview writes nothing");

    let reply = request(
        &running,
        "POST",
        "/api/convert?id=double.lcl&folder=double&plan_digest=0",
        "",
    );
    assert_eq!(reply.status, 409, "{}", reply.body);
    let reply = request(
        &running,
        "POST",
        &format!(
            "/api/convert?id=double.lcl&folder=double&plan_digest={}",
            text(&plan, "plan_digest")
        ),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    for file in files.iter() {
        assert_eq!(
            std::fs::read_to_string(project.join(text(file, "path"))).unwrap(),
            text(file, "text")
        );
    }
    assert_eq!(
        std::fs::read_to_string(project.join("double.lcl")).unwrap(),
        original
    );
    let status = json(&request(
        &running,
        "GET",
        "/api/project/status?entry=double/main.lcl",
        "",
    ));
    assert_eq!(
        text(status.get("project").unwrap(), "admission"),
        "admitted"
    );

    // A project part is not a standalone document to convert.
    let reply = request(
        &running,
        "GET",
        "/api/convert/plan?id=double/task.lcl&folder=again",
        "",
    );
    assert_eq!(reply.status, 422, "{}", reply.body);
}

// ----------------------------------------------------------------- manual

#[test]
fn the_manual_is_the_packaged_snapshot_and_nothing_else() {
    let (_project, _config, running) = serve("manual");
    let page = request(&running, "GET", "/manual/", "");
    assert_eq!(page.status, 200);
    assert!(page
        .body
        .contains(&format!("manual.js?t={}", running.token)));
    for asset in ["/manual/manual.js", "/manual/manual.css"] {
        assert_eq!(request(&running, "GET", asset, "").status, 200, "{asset}");
    }

    let snapshot = json(&request(&running, "GET", "/manual/snapshot", ""));
    let manual = common::canonical_root()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("users_manual");
    let manifest =
        lcl_spec::json::parse(&std::fs::read_to_string(manual.join("MANIFEST.json")).unwrap())
            .unwrap();
    assert_eq!(text(&snapshot, "digest"), text(&manifest, "digest"));
    assert_eq!(text(&snapshot, "version"), text(&manifest, "version"));

    // The snapshot is exactly the manual's top-level Markdown files, as on disk.
    let mut names: Vec<String> = std::fs::read_dir(&manual)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".md"))
        .collect();
    names.sort();
    let files = snapshot.get("files").and_then(Json::as_array).unwrap();
    assert_eq!(
        files.iter().map(|f| text(f, "name")).collect::<Vec<_>>(),
        names
    );
    for file in files {
        let on_disk = std::fs::read_to_string(manual.join(text(file, "name"))).unwrap();
        assert_eq!(text(file, "text"), on_disk, "{}", text(file, "name"));
    }

    // No route reaches past the packaged content.
    for escape in [
        "/manual/../Cargo.toml",
        "/manual/%2e%2e/Cargo.toml",
        "/manual/README.md",
        "/manual/snapshot/../../etc/passwd",
        "/manual/manual.js/../../../users_manual/README.md",
    ] {
        let reply = request(&running, "GET", escape, "");
        assert_ne!(reply.status, 200, "{escape} was served: {}", reply.body);
    }
    // And the manual needs the session token like everything else.
    let reply = send(running.address, "GET", "/manual/snapshot", &[], b"");
    assert_eq!(reply.status, 403);
}
