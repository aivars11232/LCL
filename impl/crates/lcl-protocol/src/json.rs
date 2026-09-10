//! A deterministic JSON writer.
//!
//! `lcl_spec::json` reads JSON, because the closed registries arrive as JSON.
//! Nothing in the workspace has ever needed to *write* it, and the trust root
//! is the wrong place to add an output path: it is the one crate whose whole
//! job is to verify bytes it did not produce.
//!
//! ## Why numbers are held as text
//!
//! [`Node::Number`] carries the rendered digits, never an `f64`. Every number
//! this protocol emits comes from an exact base-10 `Decimal`, a byte offset or
//! a count, and a round trip through binary floating point would silently
//! change some of them. A writer that could not represent an exact quantity
//! would be a writer that quietly reclassifies language data.
//!
//! ## Determinism
//!
//! Object members are emitted in insertion order, and every record in this
//! crate builds its objects in a fixed order, so two runs over the same inputs
//! produce byte-identical output. Duplicate keys are impossible to emit
//! usefully — `lcl_spec::json::parse` rejects them — so [`Object::set`]
//! replaces in place rather than appending a second member.

use std::fmt::Write as _;

/// One JSON value, built for output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Null,
    Bool(bool),
    /// A number, already rendered. The caller owns its exactness.
    Number(String),
    String(String),
    Array(Vec<Node>),
    /// An object in insertion order.
    Object(Object),
}

impl Node {
    /// A number from any integer that fits a `u64`.
    pub fn u64(value: u64) -> Node {
        Node::Number(value.to_string())
    }

    /// A number from a `usize`, the type every byte offset and count uses.
    pub fn usize(value: usize) -> Node {
        Node::Number(value.to_string())
    }

    pub fn string(value: impl Into<String>) -> Node {
        Node::String(value.into())
    }

    /// A string node, or [`Node::Null`] for an absent value.
    pub fn optional(value: Option<impl Into<String>>) -> Node {
        match value {
            Some(text) => Node::String(text.into()),
            None => Node::Null,
        }
    }

    pub fn array(items: impl IntoIterator<Item = Node>) -> Node {
        Node::Array(items.into_iter().collect())
    }

    /// Compact rendering, one line, no insignificant whitespace.
    pub fn compact(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, None, 0);
        out
    }

    /// Indented rendering, two spaces per level, one trailing line feed.
    ///
    /// The trailing feed matches the canonical source rule that "every
    /// non-empty source ends with one LINE FEED", and makes the output
    /// diffable and safe to append to a log.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, Some(2), 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, indent: Option<usize>, depth: usize) {
        match self {
            Node::Null => out.push_str("null"),
            Node::Bool(true) => out.push_str("true"),
            Node::Bool(false) => out.push_str("false"),
            Node::Number(text) => out.push_str(text),
            Node::String(text) => write_string(out, text),
            Node::Array(items) if items.is_empty() => out.push_str("[]"),
            Node::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent, depth + 1);
                    item.write(out, indent, depth + 1);
                }
                newline(out, indent, depth);
                out.push(']');
            }
            Node::Object(object) if object.is_empty() => out.push_str("{}"),
            Node::Object(object) => {
                out.push('{');
                for (i, (key, value)) in object.members().iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    newline(out, indent, depth + 1);
                    write_string(out, key);
                    out.push(':');
                    if indent.is_some() {
                        out.push(' ');
                    }
                    value.write(out, indent, depth + 1);
                }
                newline(out, indent, depth);
                out.push('}');
            }
        }
    }
}

fn newline(out: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(width) = indent {
        out.push('\n');
        for _ in 0..(width * depth) {
            out.push(' ');
        }
    }
}

/// Escape one string per RFC 8259.
///
/// Every scalar below U+0020 is escaped, because an unescaped control
/// character is not legal JSON. Everything at or above U+0020 other than `"`
/// and `\` is emitted as itself: the output is UTF-8, so no `\u` escape is
/// needed for a non-ASCII scalar, and emitting one would change the bytes of a
/// STRING value that the language preserved exactly.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// An object in insertion order, with no duplicate key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Object {
    members: Vec<(String, Node)>,
}

impl Object {
    pub fn new() -> Object {
        Object::default()
    }

    /// Set one member. Replaces in place when the key is already present, so
    /// an object can never carry the duplicate key a strict reader rejects.
    pub fn set(&mut self, key: impl Into<String>, value: Node) -> &mut Object {
        let key = key.into();
        match self.members.iter_mut().find(|(k, _)| *k == key) {
            Some((_, slot)) => *slot = value,
            None => self.members.push((key, value)),
        }
        self
    }

    /// The builder form of [`Object::set`].
    pub fn with(mut self, key: impl Into<String>, value: Node) -> Object {
        self.set(key, value);
        self
    }

    /// Set a member only when the value is present.
    ///
    /// Absence and `null` are different claims. A record uses this where the
    /// key's absence means "this command does not produce that section", and
    /// an explicit `null` where the section exists and holds nothing.
    pub fn with_some(mut self, key: impl Into<String>, value: Option<Node>) -> Object {
        if let Some(value) = value {
            self.set(key, value);
        }
        self
    }

    pub fn members(&self) -> &[(String, Node)] {
        &self.members
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&Node> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Compact rendering of this object as a document.
    pub fn compact(self) -> String {
        Node::from(self).compact()
    }

    /// Indented rendering of this object as a document.
    pub fn pretty(self) -> String {
        Node::from(self).pretty()
    }
}

impl From<Object> for Node {
    fn from(object: Object) -> Node {
        Node::Object(object)
    }
}
