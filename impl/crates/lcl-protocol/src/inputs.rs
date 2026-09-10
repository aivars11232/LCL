//! What a caller supplies to one invocation.
//!
//! ## Why text, and not only values
//!
//! A terminal hands a tool text. A UI form hands it text. Turning that text
//! into an LCL value is an act of the language — literal families, exact
//! decimals, typed constructors, collections — and the implementation contract
//! is explicit that no language rule may live only in CLI or project glue. So
//! the text arrives here, and the *engine* converts it, through the same
//! evaluation `DATA` and `INPUT` resolution already performs at step 7.
//!
//! [`Supplied::Value`] exists for a caller that already holds a value, such as
//! a test or an embedder wiring two engines together. Both forms end in the
//! same [`lcl_semantics::Invocation`].
//!
//! ## What a supplied expression may be
//!
//! Exactly one expression, self-contained. A `REF` is refused, and refusing is
//! a decision about what this *tool surface* accepts, not about what the
//! language means: a datum that reads the document it is being supplied to
//! would make the supplied value depend on the document's own resolution
//! order, and no canonical rule defines that. Refusal is stated, never
//! guessed around.
//!
//! `05_SEMANTICS/02` supplies the reason the interface has no other shape:
//! "Ambient current directory and implied nearby files do not exist in
//! portable LCL." There is no way here to discover, enumerate or default a
//! datum; a caller names one and supplies one.

use lcl_semantics::Value;

/// One supplied datum, in either form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Supplied {
    /// One LCL expression, as written by a caller.
    Text(String),
    /// A value the caller already holds.
    Value(Value),
}

/// The data one invocation supplies, in the order supplied.
///
/// Order is preserved for reporting only. Resolution is by declaration id, and
/// a repeated id replaces the earlier entry, so no ambiguity can survive
/// construction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inputs {
    entries: Vec<(String, Supplied)>,
}

impl Inputs {
    /// An invocation that supplies nothing.
    ///
    /// The honest default: a document whose `INPUT` has no declared `VALUE` and
    /// no supplied value resolves to `MISSING`, and its required readers block.
    pub fn new() -> Inputs {
        Inputs::default()
    }

    /// Supply one datum as an LCL expression.
    pub fn with_text(mut self, id: impl Into<String>, expression: impl Into<String>) -> Inputs {
        self.set(id.into(), Supplied::Text(expression.into()));
        self
    }

    /// Supply one datum as a value the caller already holds.
    pub fn with_value(mut self, id: impl Into<String>, value: Value) -> Inputs {
        self.set(id.into(), Supplied::Value(value));
        self
    }

    fn set(&mut self, id: String, supplied: Supplied) {
        match self.entries.iter_mut().find(|(k, _)| *k == id) {
            Some((_, slot)) => *slot = supplied,
            None => self.entries.push((id, supplied)),
        }
    }

    pub fn entries(&self) -> &[(String, Supplied)] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
