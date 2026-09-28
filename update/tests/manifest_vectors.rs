//! The update manifest vectors in `manifest_vectors/`: this parser reaches the
//! result `expected.txt` lists for every one, as LCL for Android's must.

use lcl_update::check;
use lcl_update::manifest;
use lcl_update::version::Version;
use std::path::PathBuf;

fn vectors() -> (PathBuf, Vec<(String, bool)>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("manifest_vectors");
    let listed = std::fs::read_to_string(dir.join("expected.txt")).unwrap();
    let cases = listed
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|line| match line.split(' ').collect::<Vec<_>>()[..] {
            [name, "ACCEPT"] => (name.to_string(), true),
            [name, "REFUSE"] => (name.to_string(), false),
            _ => panic!("expected.txt: {line:?}"),
        })
        .collect();
    (dir, cases)
}

#[test]
fn every_vector_is_accepted_or_refused_as_listed() {
    let (dir, cases) = vectors();
    let mut files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name != "expected.txt")
        .collect();
    files.sort();
    let mut listed: Vec<String> = cases.iter().map(|(name, _)| name.clone()).collect();
    listed.sort();
    assert_eq!(files, listed, "every vector is listed exactly once");
    assert!(cases.len() >= 40);
    for (name, accept) in &cases {
        let result = manifest::parse(&std::fs::read(dir.join(name)).unwrap());
        assert_eq!(result.is_ok(), *accept, "{name}: {result:?}");
    }
}

#[test]
fn the_accepted_vectors_for_another_pc_are_refused_by_the_pc_later() {
    let (dir, _) = vectors();
    let installed = Version::parse("0.1.0").unwrap();
    for name in ["wrong-architecture.json", "future-updater-version.json"] {
        let manifest = manifest::parse(&std::fs::read(dir.join(name)).unwrap()).unwrap();
        match check::applicable(&manifest, &installed) {
            Err(("unsupported", _)) => {}
            other => panic!("{name}: {other:?}"),
        }
    }
    let valid = manifest::parse(&std::fs::read(dir.join("valid.json")).unwrap()).unwrap();
    assert_eq!(check::applicable(&valid, &installed), Ok(true));
}
