//! `.lcl` and `.lcl.txt` are two names for one language.
//!
//! `.lcl` is the native ending and `.lcl.txt` the optional compatibility
//! ending, for places that only know how to handle plain text. LCL Core 0.1.0
//! defines no file extension, so the ending can only ever be a product
//! convention about recognition (`lcl_project::is_document`). It must never
//! change a verdict.
//!
//! Each test below runs the same bytes under both endings and compares whole
//! machine records. Only the document's own file name is normalised, and it is
//! replaced by one placeholder in both records, so any other difference,
//! however small, fails the comparison.

mod common;

use common::{canonical_root, lcl_in, scratch, write};
use std::path::{Path, PathBuf};

const ENDINGS: [&str; 2] = [".lcl", ".lcl.txt"];
const COMMANDS: [&str; 4] = ["check", "validate", "inspect", "run"];

/// A project root whose manifest names the canonical package.
fn project_root(name: &str, entry: Option<&str>) -> PathBuf {
    let root = scratch(name);
    let entry = entry
        .map(|entry| format!(",\n  \"entry\": {entry:?}"))
        .unwrap_or_default();
    write(
        root.join("lcl.project.json"),
        format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}{entry}\n}}\n",
            canonical_root().display().to_string()
        ),
    );
    root
}

/// One command's exit code and machine record, with `name` normalised.
fn judged(root: &Path, command: &str, name: &str) -> (i32, String) {
    let run = lcl_in(root, &[command, "--machine", name], &[]);
    (run.code, run.stdout.replace(name, "<document>"))
}

fn canonical_documents(directory: &str) -> Vec<(String, String)> {
    let path = canonical_root().join("08_EXAMPLES").join(directory);
    let mut names: Vec<String> = std::fs::read_dir(&path)
        .expect("the canonical examples are readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".lcl"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let source = std::fs::read_to_string(path.join(&name)).expect("readable");
            (name.trim_end_matches(".lcl").to_string(), source)
        })
        .collect()
}

/// Every canonical example, valid and invalid, gets the same record from all
/// four commands under either ending.
///
/// The invalid examples make this the malformed-source test too: a
/// `.lcl.txt` ending must not get a malformed document past any stage, and
/// must not change which diagnostic refuses it.
#[test]
fn every_canonical_example_is_judged_identically_under_either_ending() {
    // `03_IMPORTING_TASK` imports `02_IMPORT_LIBRARY.lcl` by that exact name.
    // The library keeps its `.lcl` name in both projects, so the `.lcl.txt`
    // run also proves that a `.lcl.txt` document imports a `.lcl` one.
    let library = canonical_root().join("08_EXAMPLES/VALID/02_IMPORT_LIBRARY.lcl");
    let library = std::fs::read_to_string(library).expect("readable");

    let mut compared = 0;
    for directory in ["VALID", "INVALID"] {
        for (stem, source) in canonical_documents(directory) {
            let mut results = Vec::new();
            for ending in ENDINGS {
                let tag = ending.replace('.', "_");
                let root = project_root(&format!("ext_{directory}_{stem}{tag}"), None);
                write(root.join("02_IMPORT_LIBRARY.lcl"), &library);
                let name = format!("{stem}{ending}");
                write(root.join(&name), &source);
                let records: Vec<(i32, String)> = COMMANDS
                    .iter()
                    .map(|command| judged(&root, command, &name))
                    .collect();
                results.push(records);
            }
            for (index, command) in COMMANDS.iter().enumerate() {
                assert_eq!(
                    results[0][index], results[1][index],
                    "{directory}/{stem}: `lcl {command}` differs between .lcl and .lcl.txt"
                );
                compared += 1;
            }
            if directory == "INVALID" {
                // Refused by `check`, or for a rules-stage defect by `validate`.
                assert!(
                    results[1].iter().any(|(code, _)| *code != 0),
                    "{stem}.lcl.txt must be refused, as its .lcl form is"
                );
            }
        }
    }
    // 13 valid and 21 invalid examples, four commands each.
    assert_eq!(compared, 34 * 4, "every canonical example was compared");
}

/// A project's `entry` may name either form, and a bare command acts on it.
#[test]
fn a_project_entry_may_use_either_ending() {
    let source =
        std::fs::read_to_string(canonical_root().join("08_EXAMPLES/VALID/01_MINIMAL_TASK.lcl"))
            .expect("readable");
    let mut records = Vec::new();
    for ending in ENDINGS {
        let entry = format!("src/main{ending}");
        let root = project_root(
            &format!("ext_entry{}", ending.replace('.', "_")),
            Some(&entry),
        );
        write(root.join(&entry), &source);
        // No document on the command line: the manifest's entry is used.
        let run = lcl_in(&root, &["run", "--machine"], &[]);
        assert_eq!(run.code, 0, "{entry}: {}{}", run.stdout, run.stderr);
        records.push(run.stdout.replace(&entry, "<entry>"));
    }
    assert_eq!(
        records[0], records[1],
        "the entry's ending must not change the record"
    );
}

/// Imports name files by path, and either form may import either form.
#[test]
fn imports_work_between_both_endings_in_every_direction() {
    let library = "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: lib.numbers\n    \
                   NAME: \"Numbers\"\n    VERSION: \"1.0.0\"\n    KIND: kind.library\n\n\
                   DEFINE:\n    ID: constant.factor\n    KIND: kind.constant\n    TYPE: INTEGER\n    \
                   VALUE: 3\n";
    let main = |library_name: &str| {
        format!(
            "LCL:\n    VERSION: \"0.1.0\"\n\nSPECIFICATION:\n    ID: app.numbers\n    \
             NAME: \"Uses the library\"\n    VERSION: \"1.0.0\"\n    KIND: kind.task\n\n\
             IMPORT:\n    ID: import.numbers\n    SOURCE: PATH({library_name:?})\n    \
             NAMESPACE: numbers\n    VERSION: \"1.0.0\"\n\n\
             OUTPUT:\n    ID: output.value\n    TYPE: INTEGER\n    FORMAT: format.plain_text\n\n\
             GOAL:\n    ID: goal.value\n    ASSERT: REF(output.value) == 12\n\n\
             ACTION:\n    ID: action.value\n    OPERATION: core.calculate\n    PARAMETER:\n        \
             NAME: expression\n        TYPE: STRING\n        REQUIRED: TRUE\n        \
             VALUE: \"4 * REF(numbers.constant.factor)\"\n    OUTPUT: REF(output.value)\n\n\
             VERIFY:\n    ID: verify.value\n    ASSERT: REF(output.value) == 12\n\n\
             SUCCESS:\n    ID: success.value\n    ALL: [REF(verify.value)]\n\n\
             TASK:\n    ID: task.value\n    GOAL: REF(goal.value)\n    ACTION: REF(action.value)\n    \
             OUTPUT: REF(output.value)\n    SUCCESS: REF(success.value)\n\n\
             EXECUTE:\n    REFERENCE: REF(task.value)\n"
        )
    };

    // The main document names its library, so its bytes depend on the
    // library's ending. Records are therefore compared between runs of
    // identical bytes: the same library, with the main document under either
    // ending. Every one of the four combinations must also succeed.
    for library_ending in ENDINGS {
        let mut records = Vec::new();
        for main_ending in ENDINGS {
            let tag = format!(
                "{}{}",
                main_ending.replace('.', "_"),
                library_ending.replace('.', "_")
            );
            let root = project_root(&format!("ext_import{tag}"), None);
            let library_name = format!("numbers{library_ending}");
            let main_name = format!("main{main_ending}");
            write(root.join(&library_name), library);
            write(root.join(&main_name), main(&library_name));
            let mut per_command = Vec::new();
            for command in COMMANDS {
                let run = lcl_in(&root, &[command, "--machine", &main_name], &[]);
                assert_eq!(
                    run.code, 0,
                    "{main_name} importing {library_name}: `lcl {command}` {}{}",
                    run.stdout, run.stderr
                );
                per_command.push(run.stdout.replace(&main_name, "<main>"));
            }
            records.push(per_command);
        }
        for (index, command) in COMMANDS.iter().enumerate() {
            assert_eq!(
                records[0][index], records[1][index],
                "`lcl {command}`: main.lcl and main.lcl.txt importing numbers{library_ending} differ"
            );
        }
    }
}

/// Recognition is exact: `.lcl` and `.lcl.txt` are LCL documents, and an
/// ordinary `.txt` file, or any other near miss, is not.
#[test]
fn only_the_two_exact_endings_are_recognised() {
    for name in [
        "task.lcl",
        "task.lcl.txt",
        "a/b/task.lcl",
        "a/b/task.lcl.txt",
    ] {
        assert!(lcl_project::is_document(name), "{name} is an LCL document");
    }
    for name in [
        "notes.txt",
        "task.txt",
        "task.lcl.bak",
        "task.LCL",
        "task.lcl.TXT",
        "lcl.txt",
    ] {
        assert!(
            !lcl_project::is_document(name),
            "{name} is not an LCL document"
        );
    }
    assert_eq!(lcl_project::SUFFIX, ".lcl", ".lcl stays the native ending");
    assert_eq!(lcl_project::TEXT_SUFFIX, ".lcl.txt");
}
