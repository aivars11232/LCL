//! Master templates on disk: which text a new file gets, copies that never
//! link back, and projects created exactly as previewed or not at all.

use lcl_protocol::json::Node;
use lcl_protocol::scaffold::{self, Content, Mode};
use lcl_protocol::Engine;
use lcl_workspace::masters::{self, Masters, Origin, Planned, Selection};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../canonical/LCL_Core_0.3.0");
        Engine::open_project(root, &[]).expect("the Core 0.3.0 engine opens")
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lcl-masters-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// Every file under `root`, by `/`-separated relative path.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("a readable folder") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, std::fs::read(&path).expect("a readable file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out
}

fn role_master(id: &str, role: &str, text: &str) -> String {
    format!(
        "{{\"format\": 1, \"id\": \"{id}\", \"name\": \"{id}\", \"core\": \"0.3.0\", \"role\": \
         \"{role}\", \"text\": {}}}",
        Node::string(text).compact()
    )
}

fn task_text(name: &str) -> String {
    format!(
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: specification.{name}\n    NAME: \
         \"{name}\"\n    VERSION:\n    KIND: kind.part.task\n"
    )
}

fn project_master(id: &str, part_master: &str) -> String {
    format!(
        "{{\"format\": 1, \"id\": \"{id}\", \"name\": \"{id}\", \"core\": \"0.3.0\", \"role\": \
         \"kind.project\", \"mode\": \"minimal\", \"entry\": \"main.lcl\", \"parts\": [{{\"path\": \
         \"task/build.lcl\", \"role\": \"kind.part.task\", \"master\": \"{part_master}\"}}, \
         {{\"path\": \"rules.lcl\", \"role\": \"kind.part.rules\"}}]}}"
    )
}

/// A project made from the project Master `web`, whose task part comes from
/// the role Master `build`.
fn project_from_masters(name: &str) -> (Masters, PathBuf) {
    let base = scratch(name);
    let store = Masters::new(base.join("masters"));
    store
        .save(
            engine(),
            &role_master("build", "kind.part.task", &task_text("build")),
        )
        .unwrap();
    store
        .save(engine(), &project_master("web", "build"))
        .unwrap();
    let root = base.join("project");
    let files = store
        .project(engine(), Selection::Master("web"), None)
        .unwrap();
    masters::create(&root, &files).unwrap();
    (store, root)
}

#[test]
fn explicit_master_overrides_default() {
    let store = Masters::new(scratch("explicit").join("masters"));
    store
        .save(
            engine(),
            &role_master("first", "kind.part.task", &task_text("first")),
        )
        .unwrap();
    store
        .save(
            engine(),
            &role_master("second", "kind.part.task", &task_text("second")),
        )
        .unwrap();
    store
        .set_default(engine(), "kind.part.task", Some("first"))
        .unwrap();
    let chosen = store
        .file(
            engine(),
            "kind.part.task",
            Selection::Master("second"),
            "x.lcl",
            None,
        )
        .unwrap();
    assert_eq!(chosen.origin, Origin::Master("second".to_string()));
    assert_eq!(chosen.scaffold.text, task_text("second"));
    let canonical = store
        .file(
            engine(),
            "kind.part.task",
            Selection::Canonical(Mode::Minimal),
            "x.lcl",
            None,
        )
        .unwrap();
    assert_eq!(
        canonical.scaffold,
        scaffold::part(engine(), "kind.part.task", Mode::Minimal, "x.lcl", None).unwrap()
    );
}

#[test]
fn default_master_overrides_canonical_scaffold() {
    let store = Masters::new(scratch("default").join("masters"));
    store
        .save(
            engine(),
            &role_master("first", "kind.part.task", &task_text("first")),
        )
        .unwrap();
    let automatic = Selection::Automatic(Mode::Guided);
    let before = store
        .file(engine(), "kind.part.task", automatic, "x.lcl", None)
        .unwrap();
    assert_eq!(before.origin, Origin::Canonical(Mode::Guided));
    assert_eq!(
        before.scaffold,
        scaffold::part(engine(), "kind.part.task", Mode::Guided, "x.lcl", None).unwrap()
    );
    store
        .set_default(engine(), "kind.part.task", Some("first"))
        .unwrap();
    let after = store
        .file(engine(), "kind.part.task", automatic, "x.lcl", None)
        .unwrap();
    assert_eq!(after.origin, Origin::Master("first".to_string()));
    assert_eq!(after.scaffold.text, task_text("first"));
    let rules = store
        .file(engine(), "kind.part.rules", automatic, "r.lcl", None)
        .unwrap();
    assert_eq!(rules.origin, Origin::Canonical(Mode::Guided));
    store.set_default(engine(), "kind.part.task", None).unwrap();
    let cleared = store
        .file(engine(), "kind.part.task", automatic, "x.lcl", None)
        .unwrap();
    assert_eq!(cleared.origin, Origin::Canonical(Mode::Guided));
}

#[test]
fn master_is_copied_not_linked() {
    let (store, root) = project_from_masters("copied");
    let created = std::fs::read_to_string(root.join("task/build.lcl")).unwrap();
    assert_eq!(created, task_text("build"));
    let home = store.dir().to_string_lossy().to_string();
    for (path, bytes) in tree(&root) {
        let text = String::from_utf8(bytes).unwrap();
        assert!(
            !text.contains("web") && !text.contains(&home),
            "{path} refers to a Master"
        );
    }
}

#[test]
fn editing_master_does_not_mutate_existing_project() {
    let (store, root) = project_from_masters("edit-master");
    let before = tree(&root);
    store
        .save(
            engine(),
            &role_master("build", "kind.part.task", &task_text("changed")),
        )
        .unwrap();
    store
        .save(
            engine(),
            &project_master("web", "build").replace("rules.lcl", "policy.lcl"),
        )
        .unwrap();
    assert_eq!(tree(&root), before);
    let later = store
        .file(
            engine(),
            "kind.part.task",
            Selection::Master("build"),
            "x.lcl",
            None,
        )
        .unwrap();
    assert_eq!(later.scaffold.text, task_text("changed"));
}

#[test]
fn editing_project_does_not_mutate_master() {
    let (store, root) = project_from_masters("edit-project");
    let masters_before = tree(store.dir());
    std::fs::write(root.join("task/build.lcl"), "edited by hand\n").unwrap();
    std::fs::remove_file(root.join("rules.lcl")).unwrap();
    assert_eq!(tree(store.dir()), masters_before);
    assert_eq!(
        store.read("build").unwrap().content,
        Content::Text(task_text("build"))
    );
}

#[test]
fn deleting_master_leaves_existing_projects_unchanged() {
    let (store, root) = project_from_masters("delete");
    store
        .set_default(engine(), "kind.part.task", Some("build"))
        .unwrap();
    let before = tree(&root);
    store.delete("build").unwrap();
    store.delete("web").unwrap();
    assert_eq!(tree(&root), before);
    assert!(store.ids().unwrap().is_empty());
    assert!(store.defaults().unwrap().is_empty());
    let now = store
        .file(
            engine(),
            "kind.part.task",
            Selection::Automatic(Mode::Minimal),
            "x.lcl",
            None,
        )
        .unwrap();
    assert_eq!(now.origin, Origin::Canonical(Mode::Minimal));
}

#[test]
fn same_master_produces_deterministic_starting_bytes() {
    let base = scratch("deterministic");
    let store = Masters::new(base.join("masters"));
    store
        .save(
            engine(),
            &role_master("build", "kind.part.task", &task_text("build")),
        )
        .unwrap();
    store
        .save(engine(), &project_master("web", "build"))
        .unwrap();
    let first = store
        .project(engine(), Selection::Master("web"), None)
        .unwrap();
    let second = store
        .project(engine(), Selection::Master("web"), None)
        .unwrap();
    assert_eq!(first, second);
    masters::create(&base.join("one"), &first).unwrap();
    masters::create(&base.join("two"), &second).unwrap();
    assert_eq!(tree(&base.join("one")), tree(&base.join("two")));
}

#[test]
fn project_preview_is_exactly_what_is_created() {
    let base = scratch("preview");
    let store = Masters::new(base.join("masters"));
    let root = base.join("new");
    let preview = store
        .project(engine(), Selection::Canonical(Mode::Guided), None)
        .unwrap();
    let paths: Vec<&str> = preview.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        ["main.lcl", "description.lcl", "rules.lcl", "task.lcl"]
    );
    assert!(
        !root.exists() && !store.dir().exists(),
        "a preview writes nothing"
    );
    masters::create(&root, &preview).unwrap();
    let written = tree(&root);
    assert_eq!(written.len(), preview.len(), "no other file is created");
    for file in &preview {
        assert!(!file.scaffold.text.is_empty(), "{} is empty", file.path);
        assert_eq!(
            written[&file.path],
            file.scaffold.text.as_bytes(),
            "{}",
            file.path
        );
    }
}

#[test]
fn project_creation_is_all_or_nothing_and_never_overwrites() {
    let base = scratch("atomic");
    let store = Masters::new(base.join("masters"));
    let preview = store
        .project(engine(), Selection::Canonical(Mode::Guided), None)
        .unwrap();
    let existing = base.join("existing");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("rules.lcl"), "mine\n").unwrap();
    let error = masters::create(&existing, &preview).unwrap_err();
    assert!(error.contains("rules.lcl already exists"), "{error}");
    assert_eq!(tree(&existing).len(), 1);
    assert_eq!(
        std::fs::read_to_string(existing.join("rules.lcl")).unwrap(),
        "mine\n"
    );
    let blocked = base.join("blocked");
    std::fs::create_dir_all(&blocked).unwrap();
    std::fs::write(blocked.join("task"), "a file where a folder must go\n").unwrap();
    let mut files = preview.clone();
    files.push(Planned {
        path: "task/later.lcl".to_string(),
        role: "kind.part.task".to_string(),
        origin: Origin::Canonical(Mode::Minimal),
        scaffold: scaffold::part(engine(), "kind.part.task", Mode::Minimal, "later.lcl", None)
            .unwrap(),
    });
    let error = masters::create(&blocked, &files).unwrap_err();
    assert!(error.ends_with("nothing was created"), "{error}");
    assert_eq!(tree(&blocked).into_keys().collect::<Vec<_>>(), ["task"]);
}

#[test]
fn invalid_master_is_never_stored_or_made_default() {
    let store = Masters::new(scratch("invalid").join("masters"));
    assert!(store
        .save(engine(), &role_master("bad", "kind.part.task", "LCL:\n"))
        .is_err());
    assert!(store.ids().unwrap().is_empty());
    assert!(store
        .set_default(engine(), "kind.part.task", Some("missing"))
        .is_err());
    let rules = scaffold::part(engine(), "kind.part.rules", Mode::Minimal, "r.lcl", None).unwrap();
    store
        .save(
            engine(),
            &role_master("rules", "kind.part.rules", &rules.text),
        )
        .unwrap();
    let error = store
        .set_default(engine(), "kind.part.task", Some("rules"))
        .unwrap_err();
    assert!(error.contains("is for kind.part.rules"), "{error}");
    store
        .save(
            engine(),
            &role_master("mine", "kind.part.task", &task_text("mine")),
        )
        .unwrap();
    store
        .set_default(engine(), "kind.part.task", Some("mine"))
        .unwrap();
    std::fs::write(
        store.dir().join("mine.json"),
        role_master("mine", "kind.part.task", "LCL:\n"),
    )
    .unwrap();
    let automatic = Selection::Automatic(Mode::Guided);
    let error = store
        .file(engine(), "kind.part.task", automatic, "x.lcl", None)
        .unwrap_err();
    assert!(
        error.contains("Master mine"),
        "a broken default is an error, not a fallback: {error}"
    );
    assert!(store
        .set_default(engine(), "kind.part.task", Some("mine"))
        .is_err());
}
