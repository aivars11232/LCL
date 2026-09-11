//! What an LCL document is called, and what a new one is called by default.
//!
//! ## This decides nothing about meaning
//!
//! LCL Core 0.1.0 registers `format.lcl` as "A document conforming to an exact
//! LCL version" and says nothing about file names, extensions or media types. A
//! document's meaning comes from its bytes and the version it declares. The
//! toolchain acts accordingly: `lcl check` reads whatever path it is given, and
//! `lcl_integration`'s own test runs identical source as `main.lcl`,
//! `main.txt`, `main` and `main.LCL` and asserts the records match.
//!
//! So everything here is a product convention about *recognition*: which files
//! a project browser lists, and what a newly created document is called. None
//! of it is consulted when a document is judged.
//!
//! ## Two suffixes, one language
//!
//! `.lcl.txt` is the default for a new document so that a document can be
//! shared, opened and edited anywhere plain text is, without anything having to
//! know what LCL is. That is a distribution convenience and nothing more: a
//! `.lcl.txt` file is checked, run and refused by exactly the same engine and
//! exactly the same rules as a `.lcl` file, and ending a name in `.txt` never
//! relaxes validation.
//!
//! `.lcl` remains fully supported. Nothing renames an existing document,
//! rewrites an import, or changes the name a file is saved under.
//!
//! A file that merely ends in `.txt` is **not** an LCL document. Only the exact
//! `.lcl.txt` ending is recognised, so ordinary text files stay ordinary.

/// The historical suffix, still fully supported.
pub const SUFFIX: &str = ".lcl";

/// The default suffix for a newly created document.
pub const TEXT_SUFFIX: &str = ".lcl.txt";

/// Every recognised document suffix, most specific first.
///
/// Order matters: `.lcl.txt` also ends in `.txt` and must be tested before any
/// shorter suffix, and a name ending `.lcl.txt` must never be read as a `.lcl`
/// name with extra characters.
pub const SUFFIXES: [&str; 2] = [TEXT_SUFFIX, SUFFIX];

/// Whether one file name is an LCL document by this product's convention.
///
/// The comparison is exact and case-sensitive. `document.LCL` is not matched,
/// for the same reason the media type's glob is not: guessing at case would
/// make recognition depend on the filesystem's own case rules, which differ
/// between them.
pub fn is_document(name: &str) -> bool {
    SUFFIXES
        .iter()
        .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix))
}

/// The name a newly created document is given.
///
/// `name` keeps a recognised suffix it already has, and is given the default
/// one when it has none. Suffixes are never stacked:
///
/// | written | created as |
/// | --- | --- |
/// | `notes` | `notes.lcl.txt` |
/// | `notes.lcl` | `notes.lcl.txt` |
/// | `notes.lcl.txt` | `notes.lcl.txt` |
///
/// `notes.lcl` becomes `notes.lcl.txt` rather than staying as written because
/// this is the *default* applied to a name being created, and the default is
/// the text form. Opening, saving and running a `.lcl` document is untouched by
/// this: nothing here renames a file that already exists.
pub fn default_name(name: &str) -> String {
    let name = name.trim();
    if name.ends_with(TEXT_SUFFIX) {
        return name.to_string();
    }
    if let Some(stem) = name.strip_suffix(SUFFIX) {
        return format!("{stem}{TEXT_SUFFIX}");
    }
    format!("{name}{TEXT_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_suffixes_are_recognised_and_nothing_else_is() {
        for name in [
            "main.lcl",
            "main.lcl.txt",
            "a/b/deep.lcl",
            "a/b/deep.lcl.txt",
        ] {
            assert!(is_document(name), "{name} is an LCL document");
        }
        for name in [
            "notes.txt",
            "main.rs",
            "main",
            "lcl.project.json",
            // A bare suffix is a name with nothing in front of it.
            ".lcl",
            ".lcl.txt",
            // Case is not guessed at.
            "main.LCL",
            "main.lcl.TXT",
            // Neither is a suffix in the middle of a name.
            "main.lcl.bak",
        ] {
            assert!(!is_document(name), "{name} is not an LCL document");
        }
    }

    #[test]
    fn a_default_name_never_stacks_a_suffix() {
        assert_eq!(default_name("notes"), "notes.lcl.txt");
        assert_eq!(default_name("notes.lcl"), "notes.lcl.txt");
        assert_eq!(default_name("notes.lcl.txt"), "notes.lcl.txt");
        // Applying it twice changes nothing, which is what "never stacks"
        // means in the one case a caller is most likely to get wrong.
        assert_eq!(default_name(&default_name("notes")), "notes.lcl.txt");
        assert_eq!(default_name(&default_name("notes.lcl")), "notes.lcl.txt");
    }

    #[test]
    fn a_default_name_keeps_the_path_in_front_of_it() {
        assert_eq!(default_name("src/main"), "src/main.lcl.txt");
        assert_eq!(default_name("src/main.lcl"), "src/main.lcl.txt");
        assert_eq!(default_name("  spaced  "), "spaced.lcl.txt");
    }

    #[test]
    fn every_default_name_is_a_recognised_document() {
        for written in ["notes", "notes.lcl", "notes.lcl.txt", "a/b/c"] {
            assert!(
                is_document(&default_name(written)),
                "{written} produced a name the tree would not list"
            );
        }
    }
}
