//! Source locations.
//!
//! The canonical location contract is
//! `statuses_and_errors_v0.1.0.json#/diagnostic_selection/location_rule`:
//!
//! > Every source byte offset is zero-based. A source diagnostic for present
//! > bytes uses its first offending UTF-8 byte offset.
//!
//! So the **byte offset is normative** and everything else here is derived
//! presentation. [`Position`] exists to render a diagnostic for a human; it is
//! never an ordering key and never appears in an equality check that decides a
//! conformance result.

use std::fmt;

/// A half-open byte range `[start, end)` into the original source bytes.
///
/// Offsets are zero-based and always measured against the bytes handed to the
/// lexer, including a byte-order mark when one is present, so a span can be
/// used to slice the caller's own buffer.
///
/// A zero-width span (`start == end`) is used where the normative locus is a
/// position rather than a lexeme: an absent required byte, or an omitted child
/// block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The zero-width span at `offset`.
    pub const fn empty(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    /// Slice `source` with this span, or `None` when the span is not a valid
    /// character-boundary range of it.
    ///
    /// Total by construction: a lexer must never panic, so this never indexes
    /// blindly.
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            write!(f, "byte {}", self.start)
        } else {
            write!(f, "bytes {}..{}", self.start, self.end)
        }
    }
}

/// A derived, human-facing location. Not normative; see [`Span`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Zero-based byte offset. This is the normative part.
    pub offset: usize,
    /// One-based line number, counting U+000A LINE FEED terminators.
    pub line: u32,
    /// One-based column, counted in Unicode scalar values from the line start.
    pub column: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} (byte {})", self.line, self.column, self.offset)
    }
}

/// Maps byte offsets to derived line/column positions.
pub(crate) struct LineIndex {
    /// Byte offset of the first byte of each line.
    starts: Vec<usize>,
}

impl LineIndex {
    pub(crate) fn new(text: &str) -> Self {
        let mut starts = vec![0usize];
        for (offset, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(offset.saturating_add(1));
            }
        }
        Self { starts }
    }

    pub(crate) fn position(&self, text: &str, offset: usize) -> Position {
        // partition_point is total; `starts` is non-empty and sorted.
        let line_index = self.starts.partition_point(|&s| s <= offset).max(1) - 1;
        let line_start = self.starts.get(line_index).copied().unwrap_or(0);
        let column = text
            .get(line_start..offset)
            .map(|s| s.chars().count())
            .unwrap_or(0)
            .saturating_add(1);
        Position {
            offset,
            line: u32::try_from(line_index.saturating_add(1)).unwrap_or(u32::MAX),
            column: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }
}
