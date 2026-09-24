//! `.lcl` recognition: the shipped desktop files against the running tool.
//!
//! The acceptance criterion is that "`.lcl` source is recognized by the
//! intended product/toolchain integration without changing language
//! semantics". Both halves are tested. The media type and glob in the shipped
//! MIME package must be the ones the tool reports, or a desktop and a toolchain
//! would disagree about what a `.lcl` file is. And the extension must decide
//! nothing: the same bytes must produce the same verdict under any file name.

mod common;

use common::{canonical_root, example, lcl_in, project, scratch, write};
use lcl_spec::json::{self, Json};
use std::path::{Path, PathBuf};

fn integration_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../integration")
        .canonicalize()
        .expect("the integration directory is present")
}

/// Everything the tool says about `.lcl`, and the shipped file, agree.
#[test]
fn the_shipped_mime_package_matches_what_the_tool_reports() {
    let root = project(
        "integration_syntax",
        "main.lcl",
        &example("01_MINIMAL_TASK.lcl"),
    );
    let run = lcl_in(&root, &["syntax", "--machine"], &[]);
    let value = json::parse(&run.stdout).expect("valid JSON");
    let media_type = value
        .get("media_type")
        .and_then(Json::as_str)
        .expect("a media type");
    let extension = value
        .get("extension")
        .and_then(Json::as_str)
        .expect("an extension");

    let xml = std::fs::read_to_string(integration_dir().join("linux/lcl.xml"))
        .expect("the MIME package is present");
    assert!(
        xml.contains(&format!("type=\"{media_type}\"")),
        "the MIME package declares the media type the tool reports"
    );
    assert!(
        xml.contains(&format!("pattern=\"*.{extension}\"")),
        "the MIME package globs the extension the tool reports"
    );

    // Every ending the tool recognises is globbed, not only the first.
    let extensions: Vec<&str> = value
        .get("extensions")
        .and_then(Json::as_array)
        .expect("the reported endings")
        .iter()
        .filter_map(Json::as_str)
        .collect();
    assert!(
        extensions.contains(&".lcl") && extensions.contains(&".lcl.txt"),
        "both recognised endings must be reported: {extensions:?}"
    );
    for ending in &extensions {
        let pattern = format!("pattern=\"*{ending}\"");
        assert!(
            xml.contains(&pattern),
            "the MIME package does not glob {ending}"
        );
    }

    // And nothing claims plain text. A glob for `*.txt` would make every text
    // file on the machine an LCL document.
    assert!(
        !xml.contains("pattern=\"*.txt\""),
        "the MIME package must not claim every .txt file"
    );
    assert!(
        !xml.contains("<mime-type type=\"text/plain\""),
        "the MIME package must not redefine text/plain"
    );
}

/// A `.lcl.txt` document is the same document under a different name.
///
/// `.lcl.txt` is the optional compatibility ending, for documents that travel
/// as plain text. The name must change nothing about the verdict, and ending
/// in `.txt` must never relax validation.
#[test]
fn the_text_ending_neither_changes_a_verdict_nor_relaxes_validation() {
    let valid = example("01_MINIMAL_TASK.lcl");
    let mut records = Vec::new();
    for name in ["main.lcl", "main.lcl.txt"] {
        let root = scratch(&format!("integration_text_{}", name.replace('.', "_")));
        write(root.join(name), &valid);
        write(
            root.join("lcl.project.json"),
            format!(
                "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
                canonical_root().display().to_string()
            ),
        );
        let run = lcl_in(&root, &["run", "--machine", name], &[]);
        assert_eq!(run.code, 0, "{name}: {}{}", run.stdout, run.stderr);
        records.push(run.stdout.replace(name, "<document>"));
    }
    assert_eq!(
        records[0], records[1],
        "a .lcl.txt document must produce the same record as a .lcl one"
    );

    // Invalid source is refused identically. A `.txt` ending is not a way to
    // get a malformed document past the checker.
    let invalid = "LCL:\n    VERSION: \"0.1.0\"\n\nNOT_A_BLOCK:\n    ID: x\n";
    let mut refusals = Vec::new();
    for name in ["bad.lcl", "bad.lcl.txt"] {
        let root = scratch(&format!("integration_bad_{}", name.replace('.', "_")));
        write(root.join(name), invalid);
        write(
            root.join("lcl.project.json"),
            format!(
                "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
                canonical_root().display().to_string()
            ),
        );
        let run = lcl_in(&root, &["check", "--machine", name], &[]);
        assert_ne!(run.code, 0, "{name} was not refused: {}", run.stdout);
        refusals.push(run.stdout.replace(name, "<document>"));
    }
    assert_eq!(
        refusals[0], refusals[1],
        "an invalid document must be refused identically under either ending"
    );
}

/// The magic rule matches what a conforming document actually begins with.
///
/// `04_GRAMMAR/01`: "Every document starts with LCL then SPECIFICATION", and
/// `02_LEXICAL/01` forbids a byte-order mark, so the first four bytes are
/// exactly `LCL:`. A magic rule that did not match those bytes would misfile
/// every document in the canonical package.
#[test]
fn the_magic_rule_matches_every_canonical_example() {
    let xml = std::fs::read_to_string(integration_dir().join("linux/lcl.xml"))
        .expect("the MIME package is present");
    assert!(
        xml.contains("value=\"LCL:\"") && xml.contains("offset=\"0\""),
        "the magic rule matches the document header at offset zero"
    );

    let dir = canonical_root().join("08_EXAMPLES/VALID");
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&dir).expect("readable") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "lcl") {
            let bytes = std::fs::read(&path).expect("readable");
            assert!(
                bytes.starts_with(b"LCL:"),
                "{} begins with the header the magic rule matches",
                path.display()
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 13, "every valid canonical example was checked");
}

/// The extension decides nothing.
///
/// The same bytes under four different names produce the same record, apart
/// from the source identity itself. If the toolchain treated `.lcl` as special,
/// this is where it would show.
#[test]
fn the_extension_changes_no_verdict() {
    let source = example("01_MINIMAL_TASK.lcl");
    let mut records = Vec::new();
    for name in ["main.lcl", "main.txt", "main", "main.LCL"] {
        let root = scratch(&format!("integration_name_{}", name.replace('.', "_")));
        write(root.join(name), &source);
        write(
            root.join("lcl.project.json"),
            format!(
                "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?}\n}}\n",
                canonical_root().display().to_string()
            ),
        );
        let run = lcl_in(&root, &["run", "--machine", name], &[]);
        assert_eq!(run.code, 0, "{name}: {}{}", run.stdout, run.stderr);
        // Everything but the unit's own identity must be identical.
        records.push(run.stdout.replace(name, "<document>"));
    }
    let first = &records[0];
    for other in &records[1..] {
        assert_eq!(first, other, "the file name decides nothing");
    }
}

/// The installer only ever writes inside the user's own data directory.
#[test]
fn the_installer_touches_only_the_users_own_data_directory() {
    let script = std::fs::read_to_string(integration_dir().join("linux/install.sh"))
        .expect("the installer is present");
    assert!(script.contains("XDG_DATA_HOME"));
    assert!(
        !script.contains("sudo") && !script.contains("/usr/share") && !script.contains("/etc/"),
        "nothing system-wide and no elevation"
    );
    let uninstall = std::fs::read_to_string(integration_dir().join("linux/uninstall.sh"))
        .expect("the uninstaller is present");
    assert!(uninstall.contains("mime/packages"));
}
