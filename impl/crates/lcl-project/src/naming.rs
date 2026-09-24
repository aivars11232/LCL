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
//! `.lcl` is the native suffix, and the default for a new document named
//! without one. `.lcl.txt` is an optional compatibility suffix, for places that
//! only know how to handle plain text: a document can be shared, opened and
//! edited anywhere plain text is, without anything having to know what LCL is.
//! That is a distribution convenience and nothing more: a `.lcl.txt` file is
//! checked, run and refused by exactly the same engine and exactly the same
//! rules as a `.lcl` file, and ending a name in `.txt` never relaxes
//! validation.
//!
//! A suffix someone chose explicitly is kept. Nothing renames an existing
//! document, rewrites an import, or changes the name a file is saved under.
//!
//! A file that merely ends in `.txt` is **not** an LCL document. Only the exact
//! `.lcl.txt` ending is recognised, so ordinary text files stay ordinary.

/// The native suffix, and the default for a new document named without one.
pub const SUFFIX: &str = ".lcl";

/// The optional compatibility suffix, kept whenever it is chosen explicitly.
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
/// `name` keeps a recognised suffix it already has, whichever of the two the
/// person chose, and is given the native `.lcl` suffix when it has none.
/// Suffixes are never stacked:
///
/// | written | created as |
/// | --- | --- |
/// | `notes` | `notes.lcl` |
/// | `notes.lcl` | `notes.lcl` |
/// | `notes.lcl.txt` | `notes.lcl.txt` |
/// | `notes.txt` | `notes.txt.lcl` |
///
/// A plain `.txt` ending is not a recognised suffix, so `notes.txt` is given
/// the default like any other name without one, and an ordinary text file is
/// never taken for a document. Opening, saving and running a document is
/// untouched by this: nothing here renames a file that already exists.
pub fn default_name(name: &str) -> String {
    default_name_with(name, SUFFIX)
}

/// The name a newly created document is given when the person has chosen
/// `ending` as their default for names written without one.
///
/// [`default_name`] is this with the native `.lcl`. A person may prefer the
/// compatibility `.lcl.txt` for new documents instead; that preference is a
/// product setting, and like everything in this module it changes a name and
/// never a meaning. An explicitly written `.lcl` or `.lcl.txt` is still kept
/// whichever default is set, and nothing is stacked. `ending` must be one of
/// [`SUFFIXES`]; anything else is not an LCL ending, and the native one is
/// given instead, so this can never produce a name the tree would not list.
pub fn default_name_with(name: &str, ending: &str) -> String {
    let name = name.trim();
    if name.ends_with(TEXT_SUFFIX) || name.ends_with(SUFFIX) {
        return name.to_string();
    }
    let ending = if SUFFIXES.contains(&ending) {
        ending
    } else {
        SUFFIX
    };
    format!("{name}{ending}")
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
    fn a_default_name_keeps_an_explicit_suffix_and_never_stacks_one() {
        assert_eq!(default_name("notes"), "notes.lcl");
        assert_eq!(default_name("notes.lcl"), "notes.lcl");
        assert_eq!(default_name("notes.lcl.txt"), "notes.lcl.txt");
        // Applying it twice changes nothing, which is what "never stacks"
        // means in the one case a caller is most likely to get wrong.
        for written in ["notes", "notes.lcl", "notes.lcl.txt"] {
            assert_eq!(
                default_name(&default_name(written)),
                default_name(written),
                "{written}"
            );
        }
    }

    #[test]
    fn a_default_name_keeps_the_path_in_front_of_it() {
        assert_eq!(default_name("src/main"), "src/main.lcl");
        assert_eq!(default_name("src/main.lcl"), "src/main.lcl");
        assert_eq!(default_name("src/main.lcl.txt"), "src/main.lcl.txt");
        assert_eq!(default_name("dir/task"), "dir/task.lcl");
        assert_eq!(default_name("dir/task.lcl"), "dir/task.lcl");
        assert_eq!(default_name("dir/task.lcl.txt"), "dir/task.lcl.txt");
        assert_eq!(default_name("  spaced  "), "spaced.lcl");
    }

    #[test]
    fn an_ordinary_text_name_is_not_taken_for_a_suffix() {
        // `.txt` alone is not recognised, so it gets the native default rather
        // than being kept as though it were an LCL ending.
        assert_eq!(default_name("notes.txt"), "notes.txt.lcl");
        assert!(!is_document("notes.txt"));
    }

    #[test]
    fn a_chosen_default_ending_applies_only_to_names_without_one() {
        assert_eq!(default_name_with("notes", TEXT_SUFFIX), "notes.lcl.txt");
        assert_eq!(
            default_name_with("dir/task", TEXT_SUFFIX),
            "dir/task.lcl.txt"
        );
        assert_eq!(default_name_with("notes", SUFFIX), "notes.lcl");
        // An explicit ending wins over either default, and is never converted.
        for ending in SUFFIXES {
            assert_eq!(default_name_with("notes.lcl", ending), "notes.lcl");
            assert_eq!(default_name_with("notes.lcl.txt", ending), "notes.lcl.txt");
        }
        // Never stacked, whichever default applied first.
        for written in ["notes", "notes.lcl", "notes.lcl.txt"] {
            let once = default_name_with(written, TEXT_SUFFIX);
            assert_eq!(default_name_with(&once, TEXT_SUFFIX), once, "{written}");
            assert_eq!(default_name_with(&once, SUFFIX), once, "{written}");
        }
        // `.txt` alone is still not an LCL ending.
        assert_eq!(
            default_name_with("notes.txt", TEXT_SUFFIX),
            "notes.txt.lcl.txt"
        );
    }

    #[test]
    fn an_ending_that_is_not_an_lcl_ending_falls_back_to_the_native_one() {
        for ending in [".txt", "", ".LCL", "lcl", ".lcl.bak"] {
            assert_eq!(
                default_name_with("notes", ending),
                "notes.lcl",
                "{ending:?}"
            );
        }
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
