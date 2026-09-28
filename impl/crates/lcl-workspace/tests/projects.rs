//! New Project by name: every project is a real folder of its own in the
//! Projects folder chosen in Settings, holds one file of every document type
//! in the read order, never overwrites anything, and is in the window's tree
//! the moment it exists.

mod common;

use common::{send, Running, Scratch};
use lcl_spec::json::Json;
use lcl_workspace::{Routes, Workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn project_spec() -> PathBuf {
    common::canonical_root()
        .parent()
        .unwrap()
        .join("LCL_Core_0.3.0")
}

fn open(folder: &Path) -> Result<Workspace, String> {
    Workspace::open_with_specs(
        folder,
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
    .map_err(|e| e.to_string())
}

/// A window over its own folder, with a settings file, a built-in default
/// folder and the ability to open the projects folder, as a desktop launch.
struct Desk {
    window: Scratch,
    config: Scratch,
    builtin: Scratch,
    running: Running,
}

fn routes(window: &Path, config: &Scratch, builtin: &Scratch) -> Routes {
    Routes::new(Arc::new(open(window).expect("the workspace opens")))
        .with_settings_file(Some(config.join("lcl/workspace-settings.json")))
        .with_builtin_default(Some(builtin.path.clone()))
        .with_reopen(Box::new(open))
}

fn desk(name: &str) -> Desk {
    let window = Scratch::new(name);
    let config = Scratch::new(&format!("{name}-config"));
    let builtin = Scratch::new(&format!("{name}-builtin"));
    let running = common::start(Arc::new(routes(&window.path, &config, &builtin)));
    Desk {
        window,
        config,
        builtin,
        running,
    }
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

fn flag(value: &Json, key: &str) -> bool {
    value
        .get(key)
        .and_then(Json::as_bool)
        .unwrap_or_else(|| panic!("no boolean {key}"))
}

fn real(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn choose(running: &Running, projects: &Path, ending: &str) {
    let body = format!(
        "{{\"default_workspace\": \"{}\", \"default_extension\": \"{ending}\"}}",
        projects.display()
    );
    let reply = request(running, "PUT", "/api/settings", &body);
    assert_eq!(reply.status, 200, "{}", reply.body);
}

/// Preview and create project `name`; the preview, then the creation's reply.
fn create(running: &Running, name: &str) -> (Json, common::Reply) {
    let plan = request(
        running,
        "GET",
        &format!("/api/project/plan?name={name}"),
        "",
    );
    assert_eq!(plan.status, 200, "{}", plan.body);
    let plan = json(&plan);
    let reply = request(
        running,
        "POST",
        &format!(
            "/api/project?name={name}&plan_digest={}",
            text(&plan, "plan_digest")
        ),
        "",
    );
    (plan, reply)
}

/// Every file under `root`, relative, with its bytes.
fn tree(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    out
}

const LAYOUT: [&str; 13] = [
    "main",
    "description/description",
    "context/context",
    "definitions/definitions",
    "rules/rules",
    "contracts/contracts",
    "bindings/bindings",
    "usage/usage",
    "stop_conditions/stop_conditions",
    "tasks/task_001",
    "checks/checks",
    "data/data",
    "output/output",
];

/// A, B, C, G and I: the chosen folder decides where projects go; each is its
/// own folder with every document type; a second project is its sibling; the
/// window opens the projects folder so both are in the tree; a name that is
/// taken is refused and the project there is left exactly as it was.
#[test]
fn projects_are_real_sibling_folders_in_the_chosen_projects_folder() {
    let desk = desk("named");
    let projects = Scratch::new("named-projects");
    choose(&desk.running, &projects.path, ".lcl.txt");

    let (plan, reply) = create(&desk.running, "Alpha");
    assert_eq!(
        text(&plan, "path"),
        projects.join("Alpha").display().to_string()
    );
    assert!(!flag(&plan, "exists"));
    assert!(
        flag(&plan, "opens_projects_folder"),
        "the window shows another folder"
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let alpha = projects.join("Alpha");
    assert!(alpha.is_dir(), "a real folder named after the project");
    let written = tree(&alpha);
    let wanted: Vec<String> = LAYOUT.iter().map(|p| format!("{p}.lcl.txt")).collect();
    assert_eq!(
        written
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        wanted.iter().cloned().collect(),
        "one file of every document type and nothing else"
    );
    let files = plan.get("files").and_then(Json::as_array).unwrap();
    assert_eq!(files.len(), 13);
    for (file, path) in files.iter().zip(&wanted) {
        assert_eq!(text(file, "path"), format!("Alpha/{path}"));
        assert_eq!(written[path.as_str()], text(file, "text").as_bytes());
    }
    let entry = String::from_utf8(written["main.lcl.txt"].clone()).unwrap();
    let mut at = 0;
    for path in &wanted[1..] {
        at += entry[at..]
            .find(&format!("PATH(\"{path}\")"))
            .unwrap_or_else(|| panic!("{path} is not listed in the read order"));
    }
    assert!(
        !desk.builtin.join("Alpha").exists(),
        "the built-in folder is not used"
    );
    assert!(!desk.window.join("Alpha").exists());

    // The window now shows the projects folder, and the tree the real
    // hierarchy with what each file declares.
    let session = json(&request(&desk.running, "GET", "/api/session", ""));
    assert_eq!(PathBuf::from(text(&session, "root")), real(&projects.path));
    let listed = json(&request(&desk.running, "GET", "/api/documents", ""));
    let entries = listed.get("entries").and_then(Json::as_array).unwrap();
    let find = |id: &str| {
        entries
            .iter()
            .find(|e| text(e, "id") == id)
            .unwrap_or_else(|| panic!("{id} is not in the tree"))
    };
    assert!(flag(find("Alpha"), "directory"));
    assert!(flag(find("Alpha/stop_conditions"), "directory"));
    for (id, kind) in [
        ("Alpha/rules/rules.lcl.txt", "kind.part.rules"),
        ("Alpha/contracts/contracts.lcl.txt", "kind.part.rules"),
        (
            "Alpha/stop_conditions/stop_conditions.lcl.txt",
            "kind.part.checks",
        ),
        ("Alpha/tasks/task_001.lcl.txt", "kind.part.task"),
    ] {
        assert!(!flag(find(id), "directory"));
        assert_eq!(text(find(id), "kind"), kind, "{id}");
    }

    // B: a sibling, created where the window already is.
    let (plan, reply) = create(&desk.running, "Beta");
    assert!(!flag(&plan, "opens_projects_folder"));
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(text(&json(&reply), "entry"), "Beta/main.lcl.txt");
    assert!(projects.join("Beta/tasks/task_001.lcl.txt").is_file());
    assert_eq!(tree(&alpha), written, "creating Beta leaves Alpha alone");

    // G: the same name again is refused, and Alpha is untouched.
    std::fs::write(alpha.join("rules/rules.lcl.txt"), "mine\n").unwrap();
    let before = tree(&alpha);
    let (plan, reply) = create(&desk.running, "Alpha");
    assert!(flag(&plan, "exists"), "the preview says the name is taken");
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(reply.body.contains("already exists"), "{}", reply.body);
    assert_eq!(
        tree(&alpha),
        before,
        "nothing in the existing project changed"
    );

    // Names that are not one plain folder name are refused, and nothing is
    // made anywhere.
    for name in [
        "..%2Fevil",
        "%2Ftmp%2Fevil",
        "..",
        ".hidden",
        "a%2Fb",
        "",
        "two%20words",
    ] {
        let reply = request(
            &desk.running,
            "GET",
            &format!("/api/project/plan?name={name}"),
            "",
        );
        assert_eq!(reply.status, 400, "{name}: {}", reply.body);
    }
    assert_eq!(
        std::fs::read_dir(&projects.path).unwrap().count(),
        2,
        "only Alpha and Beta"
    );
    assert!(!projects.path.parent().unwrap().join("evil").exists());
}

/// F: a chosen Projects folder that cannot be used is an error the person
/// sees, never a quiet switch to the built-in folder; with none chosen, the
/// built-in folder is used.
#[test]
fn an_unusable_projects_folder_is_refused_visibly() {
    let desk = desk("unusable");
    let settings = desk.config.join("lcl/workspace-settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();

    let reply = request(&desk.running, "GET", "/api/project/plan?name=Kept", "");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        text(&json(&reply), "path"),
        desk.builtin.join("Kept").display().to_string(),
        "no folder chosen: the built-in one"
    );

    let gone = desk.window.join("gone");
    let file = desk.window.join("a-file");
    std::fs::write(&file, "x").unwrap();
    for (chosen, needle) in [(&gone, "does not exist"), (&file, "is not a folder")] {
        std::fs::write(
            &settings,
            format!(
                "{{\"version\": 1, \"default_workspace\": \"{}\"}}",
                chosen.display()
            ),
        )
        .unwrap();
        for (method, target) in [
            ("GET", "/api/project/plan?name=Lost"),
            ("POST", "/api/project?name=Lost&plan_digest=x"),
        ] {
            let reply = request(&desk.running, method, target, "");
            assert_eq!(reply.status, 409, "{}", reply.body);
            assert!(reply.body.contains(needle), "{}", reply.body);
        }
    }
    std::fs::write(&settings, "{not json").unwrap();
    let reply = request(&desk.running, "GET", "/api/project/plan?name=Lost", "");
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert!(
        reply.body.contains("saved again in Settings"),
        "{}",
        reply.body
    );
    assert!(!desk.builtin.join("Lost").exists() && !gone.exists());

    // Settings' Check says what the folder is, writable included.
    let folder = json(&request(
        &desk.running,
        "GET",
        &format!("/api/folder?path={}", desk.window.path.display()),
        "",
    ));
    assert!(flag(&folder, "directory") && flag(&folder, "writable"));
    let folder = json(&request(
        &desk.running,
        "GET",
        &format!("/api/folder?path={}", gone.display()),
        "",
    ));
    assert!(!flag(&folder, "exists") && !flag(&folder, "writable"));
}

/// E: a new document named without an ending takes the configured one; an
/// explicit `.lcl` or `.lcl.txt` is kept and never doubled. A new project's
/// files take the configured ending too.
#[test]
fn endings_follow_the_setting_and_explicit_endings_are_kept() {
    let desk = desk("endings");
    let projects = Scratch::new("endings-projects");
    choose(&desk.running, &projects.path, ".lcl.txt");
    for (typed, made) in [
        ("note", "note.lcl.txt"),
        ("plain.lcl", "plain.lcl"),
        ("text.lcl.txt", "text.lcl.txt"),
    ] {
        let reply = request(
            &desk.running,
            "POST",
            &format!("/api/document?id={typed}"),
            "LCL:\n    VERSION: \"0.1.0\"\n",
        );
        assert_eq!(reply.status, 200, "{typed}: {}", reply.body);
        assert_eq!(text(&json(&reply), "id"), made);
        assert!(desk.window.join(made).is_file());
    }
    assert!(!desk.window.join("text.lcl.txt.lcl.txt").exists());

    // A Contracts document is the Contracts template, named by the rule.
    let preview = json(&request(
        &desk.running,
        "GET",
        "/api/scaffold?role=contracts&path=deal.lcl.txt",
        "",
    ));
    let reply = request(
        &desk.running,
        "POST",
        &format!(
            "/api/document?id=deal&role=contracts&scaffold_digest={}",
            text(&preview, "digest")
        ),
        "",
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let deal = std::fs::read_to_string(desk.window.join("deal.lcl.txt")).unwrap();
    assert!(
        deal.contains("KIND: kind.part.rules") && deal.contains("PRESERVE:"),
        "{deal}"
    );

    choose(&desk.running, &projects.path, ".lcl");
    let (_, reply) = create(&desk.running, "Native");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let wanted: std::collections::BTreeSet<String> =
        LAYOUT.iter().map(|p| format!("{p}.lcl")).collect();
    assert_eq!(
        tree(&projects.join("Native"))
            .into_keys()
            .collect::<std::collections::BTreeSet<_>>(),
        wanted
    );
}

/// J: the Projects folder is kept by the computer, so a restarted workspace
/// reads it back and creates the next project there. "Open folder" shows it.
#[test]
fn the_projects_folder_survives_a_restart() {
    let desk = desk("restart");
    let projects = Scratch::new("restart-projects");
    choose(&desk.running, &projects.path, ".lcl.txt");
    drop(desk.running);

    let again = common::start(Arc::new(routes(
        &desk.window.path,
        &desk.config,
        &desk.builtin,
    )));
    let settings = json(&request(&again, "GET", "/api/settings", ""));
    assert_eq!(
        text(&settings, "default_workspace"),
        projects.path.display().to_string()
    );
    assert_eq!(text(&settings, "default_extension"), ".lcl.txt");
    let (_, reply) = create(&again, "After_Restart");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(projects.join("After_Restart/main.lcl.txt").is_file());

    let other = Scratch::new("restart-other");
    let later = common::start(Arc::new(routes(&other.path, &desk.config, &desk.builtin)));
    let reply = request(&later, "POST", "/api/projects/open", "");
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        PathBuf::from(text(&json(&reply), "root")),
        real(&projects.path)
    );
}
