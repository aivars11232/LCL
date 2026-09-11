//! Syntax metadata for editors and toolchains, derived from the registries.
//!
//! ## Why a command rather than a checked-in file
//!
//! An editor needs to know which words are reserved, which are callable, which
//! symbols exist and how a comment starts. Every one of those is already closed
//! canonical data, and the workspace's first design rule is "No transcription.
//! No registry table is written into Rust source." A checked-in syntax file
//! would be exactly that transcription, and it would drift the first time the
//! registry moved.
//!
//! So this reads a loaded [`Lexicon`] — which itself refuses to load unless the
//! package is authoritative — and emits what it finds. The metadata cannot
//! disagree with the language, because it is the language's own data.
//!
//! ## It is metadata, not a grammar
//!
//! Highlighting a word is not parsing a document. Nothing here decides whether
//! source is valid, and an editor that coloured a keyword correctly has learned
//! nothing about whether the document compiles — for that it runs `lcl check`
//! and reads the diagnostics. The editor milestone builds on the same engine
//! rather than on this.

use lcl_lexer::Lexicon;
use lcl_protocol::json::{Node, Object};

/// The `.lcl` media type this product registers with the desktop.
///
/// A product convention, not a language rule: LCL Core 0.1.0 defines
/// `format.lcl` as "A document conforming to an exact LCL version" and says
/// nothing about file names or media types. Recognizing a file by extension
/// changes no document's meaning.
pub const MEDIA_TYPE: &str = "text/x-lcl";

/// The file extension this product associates with LCL documents.
///
/// Unchanged, and still reported under its own key, so an editor reading this
/// metadata keeps working. `EXTENSIONS` is the complete list.
pub const EXTENSION: &str = "lcl";

/// Every file ending this product recognises, most specific first.
///
/// `.lcl.txt` is the ending a newly created document is given, so that a
/// document can be shared and edited anywhere plain text is. It is recognised
/// in addition to `.lcl`, never instead of it, and it changes no language rule:
/// both are read, checked and run by the same engine under the same contracts.
///
/// A file that merely ends in `.txt` is not an LCL document. Only the exact
/// two-part ending is recognised.
pub const EXTENSIONS: [&str; 2] = [lcl_project::TEXT_SUFFIX, ".lcl"];

/// Everything an editor needs to colour a document, as data.
pub struct Metadata {
    pub formal_version: String,
    pub reserved_words: Vec<String>,
    pub callables: Vec<String>,
    pub block_names: Vec<String>,
    pub type_words: Vec<String>,
    pub literal_words: Vec<String>,
    pub adopted_symbols: Vec<String>,
    pub excluded_lexemes: Vec<String>,
}

/// Read one lexicon's closed vocabulary.
pub fn metadata(lexicon: &Lexicon) -> Metadata {
    let sorted = |words: Vec<&str>| {
        let mut words: Vec<String> = words.into_iter().map(str::to_string).collect();
        words.sort();
        words.dedup();
        words
    };
    Metadata {
        formal_version: lexicon.formal_version().to_string(),
        reserved_words: sorted(lexicon.reserved_words().collect()),
        callables: sorted(lexicon.callables().collect()),
        block_names: sorted(lexicon.block_names().collect()),
        type_words: sorted(lexicon.type_words().collect()),
        literal_words: sorted(lexicon.literal_words().collect()),
        adopted_symbols: sorted(lexicon.adopted_symbols().collect()),
        excluded_lexemes: sorted(lexicon.excluded_lexemes().collect()),
    }
}

impl Metadata {
    pub fn to_json(&self) -> Node {
        let words = |list: &[String]| Node::array(list.iter().map(Node::string));
        Object::new()
            .with("protocol", Node::string(lcl_protocol::PROTOCOL))
            .with("command", Node::string("syntax"))
            .with("language_version", Node::string(&self.formal_version))
            .with("media_type", Node::string(MEDIA_TYPE))
            .with("extension", Node::string(EXTENSION))
            .with(
                "extensions",
                Node::array(EXTENSIONS.iter().map(|e| Node::string(*e))),
            )
            .with(
                "source",
                Object::new()
                    // `02_LEXICAL/01` and `02_LEXICAL/02`, which an editor needs
                    // in order to avoid writing a file the lexer will refuse.
                    .with("encoding", Node::string("utf-8"))
                    .with("byte_order_mark", Node::Bool(false))
                    .with("line_terminator", Node::string("\n"))
                    .with("final_line_feed", Node::Bool(true))
                    .with("indent", Node::string("    "))
                    .with("indent_width", Node::usize(4))
                    .with("tabs", Node::Bool(false))
                    .with("trailing_space", Node::Bool(false))
                    .into(),
            )
            .with(
                "comment",
                Object::new().with("line", Node::string("COMMENT:")).into(),
            )
            .with("reserved_words", words(&self.reserved_words))
            .with("callables", words(&self.callables))
            .with("block_names", words(&self.block_names))
            .with("type_words", words(&self.type_words))
            .with("literal_words", words(&self.literal_words))
            .with("adopted_symbols", words(&self.adopted_symbols))
            .with("excluded_lexemes", words(&self.excluded_lexemes))
            .into()
    }

    /// A human summary. The lists themselves belong in `--machine` output.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("LCL {} syntax metadata\n", self.formal_version));
        out.push_str(&format!("  media type        {MEDIA_TYPE}\n"));
        out.push_str(&format!("  extension         .{EXTENSION}\n"));
        out.push_str(&format!("  recognised        {}\n", EXTENSIONS.join(", ")));
        out.push_str("  encoding          UTF-8, no BOM, LF only, final LF required\n");
        out.push_str("  indentation       four spaces, no tabs, no trailing space\n");
        out.push_str(&format!(
            "  reserved words    {}\n",
            self.reserved_words.len()
        ));
        out.push_str(&format!("  callables         {}\n", self.callables.len()));
        out.push_str(&format!("  block names       {}\n", self.block_names.len()));
        out.push_str(&format!("  type words        {}\n", self.type_words.len()));
        out.push_str(&format!(
            "  literal words     {}\n",
            self.literal_words.len()
        ));
        out.push_str(&format!(
            "  adopted symbols   {}\n",
            self.adopted_symbols.len()
        ));
        out.push_str(&format!(
            "  excluded lexemes  {}\n",
            self.excluded_lexemes.len()
        ));
        out.push_str("\nRun with --machine for the complete lists as JSON.\n");
        out
    }
}
