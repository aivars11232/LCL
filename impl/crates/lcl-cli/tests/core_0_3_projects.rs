//! LCL Core 0.3 Task 01: multi-file projects on the command line.
//!
//! The Core 0.3.0 package is named explicitly, by `--project-spec` or by the
//! manifest's `project_spec`. These cases need a real filesystem or a real
//! process: a part the host cannot read, one project created in different file
//! orders, a run whose effect must not happen before the whole project is
//! admitted, and standalone Core 0.1.0 and 0.2.0 documents that keep their
//! exact meaning when the 0.3.0 package is attached.

mod common;

use common::{canonical_root, example, lcl_in, scratch, write};
use lcl_spec::json::{self, Json};
use std::path::{Path, PathBuf};

const SUCCESS: i32 = 0;
const REJECTED: i32 = 1;

fn package(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../../canonical/LCL_Core_{version}"))
        .canonicalize()
        .expect("the package must be present")
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

fn fixture_dir(name: &str) -> PathBuf {
    package("0.3.0")
        .join("09_CONFORMANCE/PROJECT_FIXTURES")
        .join(name)
}

/// `lcl <command> --machine`, the three packages named, then `rest`.
fn machine(dir: &Path, command: &str, rest: &[String]) -> (i32, Json, String) {
    let mut args: Vec<String> = vec![command.into(), "--machine".into()];
    for (flag, version) in [
        ("--spec", "0.1.0"),
        ("--localized-spec", "0.2.0"),
        ("--project-spec", "0.3.0"),
    ] {
        args.push(flag.into());
        args.push(text(&package(version)));
    }
    args.extend(rest.iter().cloned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = lcl_in(dir, &args, &[]);
    let value =
        json::parse(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}{}", run.stdout, run.stderr));
    (run.code, value, run.stdout)
}

fn validate(dir: &Path, entry: &str) -> (i32, Json, String) {
    machine(dir, "validate", &[entry.to_string()])
}

/// `(id, source, byte offset)` of every reported diagnostic.
fn diagnostics(report: &Json) -> Vec<(String, String, usize)> {
    report
        .get("diagnostics")
        .and_then(Json::as_array)
        .expect("diagnostics")
        .iter()
        .map(|d| {
            let start = match d.get("span").and_then(|s| s.get("start")) {
                Some(Json::Number(n)) => *n as usize,
                other => panic!("span start: {other:?}"),
            };
            (field(d, "id"), field(d, "source"), start)
        })
        .collect()
}

fn field(value: &Json, key: &str) -> String {
    value
        .get(key)
        .and_then(Json::as_str)
        .unwrap_or_else(|| panic!("{key} is a string"))
        .to_string()
}

fn project(report: &Json) -> &Json {
    report.get("project").expect("a project record")
}

/// `(source, state, status)` of every PART row.
fn part_rows(report: &Json) -> Vec<(String, String, String)> {
    project(report)
        .get("parts")
        .and_then(Json::as_array)
        .expect("parts")
        .iter()
        .map(|p| (field(p, "source"), field(p, "state"), field(p, "status")))
        .collect()
}

fn fixture_files(name: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("fixture directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).expect("inside the fixture");
                out.push(relative.to_str().expect("UTF-8 name").replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(&fixture_dir(name), &fixture_dir(name), &mut out);
    out.sort();
    out
}

/// Copy one canonical fixture into `root`, creating its files in `order`.
fn copy_fixture(name: &str, root: &Path, order: &[String]) {
    for relative in order {
        let bytes = std::fs::read(fixture_dir(name).join(relative)).expect("fixture file");
        write(root.join(relative), bytes);
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .expect("the mode can be set");
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// `05_SEMANTICS/13`: only a source the host reports absent may be omitted.
/// A part that exists and cannot be read is `error.project.part_missing` at
/// its PART SOURCE, whether it is required or optional: unreadable is not
/// absent.
#[cfg(unix)]
#[test]
fn an_unreadable_part_blocks_the_project_whether_required_or_optional() {
    for (fixture, part) in [
        ("valid_minimal", "task.lcl"),
        ("valid_optional_present", "description.lcl"),
    ] {
        let root = scratch(&format!("c03_unreadable_{fixture}"));
        copy_fixture(fixture, &root, &fixture_files(fixture));
        let (code, _, before) = validate(&root, "main.lcl");
        assert_eq!(code, SUCCESS, "the readable project is admitted: {before}");

        set_mode(&root.join(part), 0o000);
        let (code, report, stdout) = validate(&root, "main.lcl");
        set_mode(&root.join(part), 0o644);

        assert_eq!(code, REJECTED, "{stdout}");
        let main = std::fs::read_to_string(root.join("main.lcl")).expect("entry");
        let at = main
            .find(&format!("PATH(\"{part}\")"))
            .expect("the PART SOURCE value");
        assert_eq!(
            diagnostics(&report),
            [(
                "error.project.part_missing".to_string(),
                "main.lcl".to_string(),
                at
            )],
            "{fixture}"
        );
        let row = part_rows(&report)
            .into_iter()
            .find(|(source, _, _)| source == part)
            .expect("the part's row");
        assert_eq!((row.1.as_str(), row.2.as_str()), ("missing", "missing"));
        assert_eq!(field(project(&report), "admission"), "rejected");
    }
}

/// Load order is PART order. The same project, created file by file in
/// opposite orders, is judged byte for byte the same, and a file no PART
/// names is never read, however its name sorts.
#[test]
fn load_order_is_part_order_whatever_the_filesystem_order() {
    let files = fixture_files("valid_all_roles");
    let forward = scratch("c03_order_forward");
    copy_fixture("valid_all_roles", &forward, &files);
    let backward = scratch("c03_order_backward");
    let reversed: Vec<String> = files.iter().rev().cloned().collect();
    copy_fixture("valid_all_roles", &backward, &reversed);
    for root in [&forward, &backward] {
        write(root.join("aaa_unlisted.lcl"), b"not LCL \xff");
        write(root.join("task/zzz_unlisted.lcl"), b"LCL:\n");
    }

    let (code, report, first) = validate(&forward, "main.lcl");
    let (code_back, _, second) = validate(&backward, "main.lcl");
    assert_eq!(code, SUCCESS, "{first}");
    assert_eq!(code_back, SUCCESS, "{second}");
    assert_eq!(
        first.replace(&text(&forward), "<root>"),
        second.replace(&text(&backward), "<root>")
    );
    let order: Vec<String> = project(&report)
        .get("order")
        .and_then(Json::as_array)
        .expect("order")
        .iter()
        .map(|unit| unit.as_str().expect("a unit").to_string())
        .collect();
    assert_eq!(
        order,
        [
            "main.lcl",
            "description.lcl",
            "definitions.lcl",
            "data.lcl",
            "context.lcl",
            "rules.lcl",
            "output.lcl",
            "checks.lcl",
            "task/build.lcl"
        ]
    );
    assert!(
        !first.contains("unlisted"),
        "an unlisted file is never loaded"
    );
}

/// A Core 0.3.0 project is rejected, not guessed at, when the 0.3.0 package is
/// not named: the 0.2.0 engine does not know `PART`.
#[test]
fn a_project_is_rejected_when_the_0_3_0_package_is_not_named() {
    let root = scratch("c03_unnamed_package");
    copy_fixture("valid_minimal", &root, &fixture_files("valid_minimal"));
    let run = lcl_in(
        &root,
        &[
            "validate",
            "--spec",
            &text(&canonical_root()),
            "--localized-spec",
            &text(&package("0.2.0")),
            "main.lcl",
        ],
        &[],
    );
    assert_eq!(run.code, REJECTED, "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("error.keyword.unknown"),
        "{}",
        run.stdout
    );
}

/// The manifest's `project_spec` names the 0.3.0 package as the option does.
#[test]
fn the_manifest_names_the_0_3_0_package() {
    let root = scratch("c03_manifest");
    copy_fixture("valid_minimal", &root, &fixture_files("valid_minimal"));
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \
             \"project_spec\": {:?},\n  \"entry\": \"main.lcl\"\n}}\n",
            text(&canonical_root()),
            text(&package("0.3.0"))
        ),
    );
    let run = lcl_in(&root, &["validate", "--machine"], &[]);
    assert_eq!(run.code, SUCCESS, "{}{}", run.stdout, run.stderr);
    let report = json::parse(&run.stdout).expect("valid JSON");
    assert_eq!(field(project(&report), "admission"), "admitted");
}

// ---------------------------------------------------------------------------
// No effect before admission
// ---------------------------------------------------------------------------

/// A three-part project whose task writes one file inside the granted
/// directory. `broken` replaces one part's bytes, or removes it when `None`.
fn writing_project(name: &str, target: &Path, broken: Option<(&str, Option<&str>)>) -> PathBuf {
    let header = |id: &str, name: &str, kind: &str| {
        format!(
            "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: {id}\n    \
             NAME: \"{name}\"\n    VERSION: \"1.0.0\"\n    KIND: {kind}\n\n"
        )
    };
    let part = |id: &str, source: &str, kind: &str| {
        format!("PART:\n    ID: {id}\n    SOURCE: PATH(\"{source}\")\n    KIND: {kind}\n\n")
    };
    let main = format!(
        "{}{}{}{}EXECUTE:\n    REFERENCE: REF(task.write)\n",
        header("project.write", "Write one file", "kind.project"),
        part(
            "part.description",
            "description.lcl",
            "kind.part.description"
        ),
        part("part.rules", "rules.lcl", "kind.part.rules"),
        part("part.task", "task.lcl", "kind.part.task"),
    );
    let description = format!(
        "{}COMMENT:\n    CONTENT: \"Writes one file.\"\n",
        header(
            "project.write.description",
            "Description",
            "kind.part.description"
        )
    );
    let target = text(target);
    let rules = format!(
        "{}ALLOW:\n    ID: allow.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    AUTHORITY: 900\n",
        header("project.write.rules", "Rules", "kind.part.rules")
    );
    let task = format!(
        "{}DATA:\n    ID: data.content\n    TYPE: STRING\n    VALUE: \"written by lcl\"\n\n\
         OUTPUT:\n    ID: output.written\n    TYPE: PATH\n    FORMAT: format.plain_text\n\n\
         GOAL:\n    ID: goal.write\n    ASSERT: TRUE\n\n\
         ACTION:\n    ID: action.write\n    OPERATION: core.write\n    \
         TARGET: PATH({target:?})\n    PARAMETER:\n        NAME: content\n        \
         TYPE: STRING\n        REQUIRED: TRUE\n        VALUE: REF(data.content)\n    \
         PARAMETER:\n        NAME: create_if_missing\n        TYPE: BOOLEAN\n        \
         REQUIRED: FALSE\n        VALUE: TRUE\n    OUTPUT: REF(output.written)\n\n\
         SUCCESS:\n    ID: success.write\n    ALL: [REF(output.written)]\n\n\
         TASK:\n    ID: task.write\n    GOAL: REF(goal.write)\n    \
         ACTION: REF(action.write)\n    OUTPUT: REF(output.written)\n    \
         SUCCESS: REF(success.write)\n",
        header("project.write.task", "Task", "kind.part.task")
    );
    let root = scratch(name);
    for (file, bytes) in [
        ("main.lcl", main),
        ("description.lcl", description),
        ("rules.lcl", rules),
        ("task.lcl", task),
    ] {
        match broken {
            Some((broken_file, None)) if broken_file == file => {}
            Some((broken_file, Some(replacement))) if broken_file == file => {
                write(root.join(file), replacement)
            }
            _ => write(root.join(file), bytes),
        }
    }
    root
}

fn run_with_grant(root: &Path, dir: &Path) -> (i32, Json, String) {
    machine(
        root,
        "run",
        &["--allow-write".into(), text(dir), "main.lcl".into()],
    )
}

/// The control: the admitted project performs its effect, so the refusals
/// below are about admission and not about a project that could never write.
#[test]
fn an_admitted_project_performs_its_effect() {
    let dir = scratch("c03_effect_admitted_target");
    let target = dir.join("written.txt");
    let root = writing_project("c03_effect_admitted", &target, None);
    let (code, report, stdout) = run_with_grant(&root, &dir);
    assert_eq!(code, SUCCESS, "{stdout}");
    assert_eq!(
        std::fs::read_to_string(&target).expect("the file was written"),
        "written by lcl"
    );
    assert_eq!(field(project(&report), "admission"), "admitted");
}

/// `05_SEMANTICS/13`: "No effect of a project evaluation precedes
/// admission." One invalid, missing or wrongly placed part — even the
/// non-normative description — stops the whole project before any effect,
/// although the task part itself is valid and the write is granted.
#[test]
fn one_invalid_part_prevents_every_effect() {
    let malformed = "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: project.write.description\n    \
                     NAME: \"Description\"\n    VERSION: \"1.0.0\"\n    KIND: kind.part.description\n\n\
                     COMMENT:\n    CONTENT: [1, 2\n";
    let role_violation =
        "LCL:\n    VERSION: \"0.3.0\"\n\nSPECIFICATION:\n    ID: project.write.rules\n    \
                          NAME: \"Rules\"\n    VERSION: \"1.0.0\"\n    KIND: kind.part.rules\n\n\
                          GOAL:\n    ID: goal.extra\n    ASSERT: TRUE\n";
    for (label, broken, expected) in [
        (
            "malformed",
            ("description.lcl", Some(malformed)),
            "error.delimiter.unclosed",
        ),
        (
            "role",
            ("rules.lcl", Some(role_violation)),
            "error.block.context",
        ),
        (
            "missing",
            ("description.lcl", None),
            "error.project.part_missing",
        ),
    ] {
        let dir = scratch(&format!("c03_effect_{label}_target"));
        let target = dir.join("written.txt");
        let root = writing_project(&format!("c03_effect_{label}"), &target, Some(broken));
        let (code, report, stdout) = run_with_grant(&root, &dir);
        assert_eq!(code, REJECTED, "{label}: {stdout}");
        assert!(!target.exists(), "{label}: nothing was written");
        let ids: Vec<String> = diagnostics(&report).into_iter().map(|d| d.0).collect();
        assert_eq!(ids, [expected], "{label}");
        assert_eq!(field(project(&report), "admission"), "rejected", "{label}");
    }
}

// ---------------------------------------------------------------------------
// Standalone documents keep their meaning
// ---------------------------------------------------------------------------

/// Output of `validate --machine` with the named packages only.
fn standalone(file: &Path, packages: &[(&str, &str)], profile: Option<&Path>) -> (i32, String) {
    let mut args: Vec<String> = vec!["validate".into(), "--machine".into()];
    for (flag, version) in packages {
        args.push((*flag).into());
        args.push(text(&package(version)));
    }
    if let Some(profile) = profile {
        args.push("--profile".into());
        args.push(text(profile));
    }
    args.push(text(file));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = lcl_in(&std::env::temp_dir(), &args, &[]);
    (run.code, run.stdout)
}

/// A standalone Core 0.1.0 document and a localized Core 0.2.0 document are
/// judged byte for byte the same whether or not the 0.3.0 package is attached.
#[test]
fn standalone_0_1_and_0_2_documents_are_unchanged_by_the_0_3_package() {
    let dir = scratch("c03_standalone");
    let core = dir.join("minimal.lcl");
    write(&core, example("01_MINIMAL_TASK.lcl"));
    let fixtures = package("0.2.0").join("09_CONFORMANCE/LOCALIZATION_FIXTURES");
    let profile = fixtures.join("profiles/lv-LV.json");
    let localized = fixtures.join("sources/auto_lv.lcl");

    let without = [("--spec", "0.1.0"), ("--localized-spec", "0.2.0")];
    let with = [
        ("--spec", "0.1.0"),
        ("--localized-spec", "0.2.0"),
        ("--project-spec", "0.3.0"),
    ];
    for (file, profile) in [(&core, None), (&localized, Some(profile.as_path()))] {
        let (code, before) = standalone(file, &without, profile);
        let (code_after, after) = standalone(file, &with, profile);
        assert_eq!(code, SUCCESS, "{before}");
        assert_eq!(code_after, SUCCESS, "{after}");
        assert_eq!(before, after, "{}", file.display());
        assert!(
            !after.contains("\"project\""),
            "a standalone document is no project"
        );
    }
}

/// A standalone Core 0.3.0 task is judged by the 0.3.0 package and is no
/// project; a Core 0.3.0 document is refused by a 0.1.0-only command line.
#[test]
fn a_standalone_0_3_0_task_is_no_project() {
    let dir = scratch("c03_standalone_task");
    let task = dir.join("task.lcl");
    let source = example("01_MINIMAL_TASK.lcl").replacen("\"0.1.0\"", "\"0.3.0\"", 1);
    write(&task, &source);
    let (code, stdout) = standalone(
        &task,
        &[("--spec", "0.1.0"), ("--project-spec", "0.3.0")],
        None,
    );
    assert_eq!(code, SUCCESS, "{stdout}");
    let report = json::parse(&stdout).expect("valid JSON");
    assert!(report.get("project").is_none());
    let (code, stdout) = standalone(&task, &[("--spec", "0.1.0")], None);
    assert_ne!(code, SUCCESS, "{stdout}");
}
