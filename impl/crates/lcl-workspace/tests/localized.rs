//! LCL-FEATURE-04 D4: the workspace edits localized documents in place.
//!
//! A project naming the Core 0.2.0 package and a profile directory opens with
//! the localized engine. The document keeps the author's own bytes through
//! open, analysis, save and reopen; token spans paint localized words by their
//! canonical classes on the original bytes; and a Core 0.1.0 document in the
//! same workspace is still judged by Core 0.1.0.

mod common;

use common::{canonical_root, example, valid_examples, Scratch};
use lcl_protocol::{Command, Outcome};
use lcl_workspace::{intelligence, Routes, Workspace};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn localized_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../canonical/LCL_Core_0.2.0")
        .canonicalize()
        .expect("the 0.2.0 package must be present")
}

fn fixture(relative: &str) -> String {
    std::fs::read_to_string(
        localized_root()
            .join("09_CONFORMANCE/LOCALIZATION_FIXTURES")
            .join(relative),
    )
    .expect("fixture")
}

fn localized_project(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    scratch.put("main.lcl", &fixture("sources/auto_lv.lcl"));
    scratch.put("profiles/lv-LV.json", &fixture("profiles/lv-LV.json"));
    scratch.put(
        "lcl.project.json",
        &format!(
            "{{\n  \"format\": \"lcl.project/1\",\n  \"spec\": {:?},\n  \"localized_spec\": {:?},\n  \"profiles\": \"profiles\",\n  \"entry\": \"main.lcl\"\n}}\n",
            canonical_root().display().to_string(),
            localized_root().display().to_string(),
        ),
    );
    scratch
}

fn classes(workspace: &Workspace, id: &str, text: &str) -> Vec<(&'static str, &'static str)> {
    let unit = intelligence::unit_of(id, text);
    intelligence::tokens(workspace.engine_for(&unit), &unit)
        .into_iter()
        .map(|span| (span.terminal, span.class))
        .collect()
}

#[test]
fn a_localized_document_is_judged_by_the_0_2_0_engine() {
    let scratch = localized_project("workspace-localized-check");
    let workspace =
        Arc::new(Workspace::open(&scratch.path, canonical_root()).expect("the project opens"));
    assert!(workspace.localized_spec_root().is_some());
    let document = workspace.read("main.lcl").expect("it reads");
    assert_eq!(document.text, fixture("sources/auto_lv.lcl"));

    let routes = Routes::new(Arc::clone(&workspace));
    let report = routes
        .report("main.lcl", &document.text, Command::Check)
        .expect("a report");
    assert_eq!(
        report.outcome,
        Outcome::Accepted,
        "{:?}",
        report.diagnostics
    );
    assert_eq!(report.spec.formal_version, "0.2.0");
    let locale = report.units[0].locale.as_ref().expect("a locale record");
    assert_eq!(
        (locale.method.as_str(), locale.locale.as_deref()),
        ("auto", Some("lv-LV"))
    );

    let name = valid_examples()
        .into_iter()
        .next()
        .expect("a valid example");
    let core = routes
        .report(&name, &example(&name), Command::Check)
        .expect("a report");
    assert_eq!(core.spec.formal_version, "0.1.0");
    assert!(core.units.iter().all(|unit| unit.locale.is_none()));
}

/// LCL-REPAIR-03 B-10: a profile file serves a project that declares no profile
/// directory, as the command line's `--profile` does, and without one the
/// document is refused rather than read under canonical spellings.
#[test]
fn a_profile_file_serves_a_project_without_a_profile_directory() {
    let scratch = Scratch::new("workspace-localized-profile-file");
    scratch.put("main.lcl", &fixture("sources/explicit_lv.lcl"));
    let profile = scratch.put("lv-LV.json", &fixture("profiles/lv-LV.json"));
    let check = |profiles: &[PathBuf]| {
        let workspace = Workspace::open_with_profiles(
            &scratch.path,
            canonical_root(),
            Some(localized_root()),
            profiles,
        )
        .expect("the project opens");
        let text = workspace.read("main.lcl").expect("it reads").text;
        Routes::new(Arc::new(workspace))
            .report("main.lcl", &text, Command::Check)
            .expect("a report")
    };

    let with = check(std::slice::from_ref(&profile));
    assert_eq!(with.outcome, Outcome::Accepted, "{:?}", with.diagnostics);
    assert_eq!(with.spec.formal_version, "0.2.0");

    let without = check(&[]);
    assert_eq!(without.spec.formal_version, "0.2.0");
    assert_eq!(
        without.primary().map(|d| d.id.as_str()),
        Some("error.localization.profile_unavailable")
    );
}

#[test]
fn token_spans_paint_localized_words_by_their_canonical_class() {
    let scratch = localized_project("workspace-localized-tokens");
    let workspace = Workspace::open(&scratch.path, canonical_root()).expect("opens");
    let text = fixture("sources/auto_lv.lcl");
    let unit = intelligence::unit_of("main.lcl", &text);
    let spans = intelligence::tokens(workspace.engine_for(&unit), &unit);
    assert!(spans
        .iter()
        .all(|s| text.is_char_boundary(s.start) && text.is_char_boundary(s.end)));
    let painted = |word: &str| {
        let start = text.find(word).expect("the word is in the source");
        spans
            .iter()
            .find(|span| span.start == start)
            .map(|span| (span.end - span.start, span.class))
    };
    assert_eq!(
        painted("SPECIFIKĀCIJA"),
        Some(("SPECIFIKĀCIJA".len(), "block"))
    );
    assert_eq!(painted("DATI"), Some((4, "block")));
    assert_eq!(painted("VESELS"), Some((6, "type")));

    // The same program in canonical English paints the same classes in the
    // same order.
    assert_eq!(
        classes(&workspace, "main.lcl", &text),
        classes(
            &workspace,
            "canonical_en.lcl",
            &fixture("sources/canonical_en.lcl")
        )
    );
}

#[test]
fn saving_and_reopening_keeps_the_localized_bytes() {
    let scratch = localized_project("workspace-localized-save");
    let workspace = Workspace::open(&scratch.path, canonical_root()).expect("opens");
    let mut edited = fixture("sources/auto_lv.lcl").replace("VĒRTĪBA: 3", "VĒRTĪBA: 4");
    if !edited.ends_with('\n') {
        edited.push('\n');
    }
    workspace.save("main.lcl", &edited).expect("it saves");
    assert_eq!(
        std::fs::read(scratch.join("main.lcl")).expect("on disk"),
        edited.as_bytes()
    );

    let reopened = Arc::new(Workspace::open(&scratch.path, canonical_root()).expect("reopens"));
    assert_eq!(reopened.read("main.lcl").expect("reads").text, edited);
    let documents: Vec<String> = reopened
        .documents()
        .expect("lists")
        .into_iter()
        .filter(|entry| !entry.directory)
        .map(|entry| entry.id)
        .collect();
    assert_eq!(documents, vec!["main.lcl".to_string()]);
    let report = Routes::new(Arc::clone(&reopened))
        .report("main.lcl", &edited, Command::Check)
        .expect("a report");
    assert_eq!(
        report.outcome,
        Outcome::Accepted,
        "{:?}",
        report.diagnostics
    );
    assert_eq!(
        report.units[0]
            .locale
            .as_ref()
            .and_then(|locale| locale.locale.as_deref()),
        Some("lv-LV")
    );
}
