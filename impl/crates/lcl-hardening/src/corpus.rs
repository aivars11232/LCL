//! Where hostile input comes from.
//!
//! Four sources, in increasing distance from real LCL:
//!
//! 1. the canonical examples, which are the shapes a real document has;
//! 2. mutations of those, which are documents that were nearly right;
//! 3. generated documents built from the real lexicon, which are documents a
//!    generator thought were right;
//! 4. arbitrary bytes, which are not documents at all.
//!
//! The first two matter most. A parser is rarely defeated by noise; it is
//! defeated by something that looks almost exactly like what it expects.

use crate::rng::Rng;
use std::fs;
use std::path::Path;

/// A set of sources to drive a stage with.
pub struct Corpus {
    entries: Vec<(String, String)>,
}

impl Corpus {
    pub fn new() -> Corpus {
        Corpus {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, label: impl Into<String>, source: impl Into<String>) {
        self.entries.push((label.into(), source.into()));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|(label, source)| (label.as_str(), source.as_str()))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Corpus {
    fn default() -> Corpus {
        Corpus::new()
    }
}

/// Every canonical example, valid and invalid, by file name.
///
/// The invalid ones belong here as much as the valid ones: each is a document
/// that fails at exactly one known stage, which is precisely the state in which
/// a later stage is most likely to be handed something it did not expect.
pub fn canonical_examples(root: &Path) -> Corpus {
    let mut corpus = Corpus::new();
    for directory in ["08_EXAMPLES/VALID", "08_EXAMPLES/INVALID"] {
        let path = root.join(directory);
        let mut names: Vec<_> = fs::read_dir(&path)
            .expect("the canonical examples are readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".lcl"))
            .collect();
        // Filesystem enumeration order is not an input to anything.
        names.sort();
        for name in names {
            let source = fs::read_to_string(path.join(&name)).expect("the example is readable");
            corpus.push(name, source);
        }
    }
    corpus
}

/// Every single-byte mutation this seed selects, from one source.
///
/// A byte is replaced, deleted, or duplicated. The result is usually invalid
/// and occasionally still valid, and both are interesting: the first exercises
/// diagnostics, the second exercises everything after them.
pub fn mutate(rng: &mut Rng, source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    if bytes.is_empty() {
        return String::from_utf8_lossy(&[rng.next_u64() as u8]).to_string();
    }
    let position = rng.below(bytes.len());
    match rng.below(4) {
        0 => bytes[position] = rng.next_u64() as u8,
        1 => {
            bytes.remove(position);
        }
        2 => {
            let byte = bytes[position];
            bytes.insert(position, byte);
        }
        _ => {
            // A byte from the alphabet LCL actually uses, which produces a much
            // more plausible near-miss than a random one.
            let alphabet =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 :.\"[](),_-\n";
            bytes[position] = *rng.pick(alphabet).expect("the alphabet is not empty");
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

/// Every prefix and every suffix of one source, at line boundaries.
///
/// Truncation is how a real document arrives when a write was interrupted, and
/// it is the cheapest way to produce a document that is structurally
/// incomplete at every possible depth.
pub fn truncations(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    for cut in 0..lines.len() {
        out.push(lines[..cut].concat());
        out.push(lines[cut..].concat());
    }
    out
}

/// Bytes that are not a document at all.
pub fn arbitrary_bytes(rng: &mut Rng, length: usize) -> String {
    let mut bytes = Vec::with_capacity(length);
    for _ in 0..length {
        bytes.push(rng.next_u64() as u8);
    }
    String::from_utf8_lossy(&bytes).to_string()
}

/// A document assembled from real block words and real field keys.
///
/// It is almost never valid. That is the point: it is wrong in the way a
/// document written by someone who knows the vocabulary and not the grammar is
/// wrong, which is the wrongness a parser has to survive most often.
pub fn generated_document(rng: &mut Rng, words: &[String], fields: &[String]) -> String {
    let mut out = String::from("LCL:\n    VERSION: \"0.1.0\"\n");
    let blocks = 1 + rng.below(12);
    for _ in 0..blocks {
        let Some(word) = rng.pick(words) else {
            break;
        };
        out.push('\n');
        out.push_str(word);
        out.push_str(":\n");
        let lines = 1 + rng.below(6);
        for _ in 0..lines {
            let Some(field) = rng.pick(fields) else {
                break;
            };
            let indent = "    ".repeat(1 + rng.below(3));
            let value = match rng.below(6) {
                0 => "TRUE".to_string(),
                1 => format!("{}", rng.below(1000)),
                2 => format!("\"{}\"", rng.below(1000)),
                3 => format!("REF(id.{})", rng.below(20)),
                4 => format!("[{}, {}]", rng.below(10), rng.below(10)),
                _ => String::new(),
            };
            match value.is_empty() {
                true => out.push_str(&format!("{indent}{field}:\n")),
                false => out.push_str(&format!("{indent}{field}: {value}\n")),
            }
        }
    }
    out
}
