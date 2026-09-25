//! Frames: one JSON message each, behind a four-byte big-endian length.
//!
//! Inside TLS, so the framing needs no integrity of its own — TLS 1.3 already
//! authenticates and orders every byte and refuses a replayed record. What it
//! does need is a bound: a length larger than the reader's limit closes the
//! connection before anything is allocated for it. The limit is small until
//! the device is authenticated ([`MAX_HELLO_FRAME`]) and [`MAX_FRAME`] after.

/// The largest frame either side accepts from an authenticated peer:
/// comfortably above any document the workspace will save, and far below what
/// would exhaust memory.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

/// The largest first message, `hello`, the PC accepts. A hello is a few short
/// fields, and a peer nobody has authenticated yet is given no more buffer
/// than one needs.
pub const MAX_HELLO_FRAME: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge(usize),
    NotUtf8,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => {
                write!(
                    f,
                    "a frame of {n} bytes is larger than this connection accepts"
                )
            }
            FrameError::NotUtf8 => f.write_str("a frame is not UTF-8"),
        }
    }
}

/// One frame, ready to write.
pub fn encode(message: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + message.len());
    out.extend_from_slice(&(message.len() as u32).to_be_bytes());
    out.extend_from_slice(message.as_bytes());
    out
}

/// Frames reassembled from whatever arrives, however it is split.
pub struct Reader {
    buffer: Vec<u8>,
    limit: usize,
}

impl Default for Reader {
    /// A reader for an authenticated peer: frames up to [`MAX_FRAME`].
    fn default() -> Reader {
        Reader::with_limit(MAX_FRAME)
    }
}

impl Reader {
    /// A reader that refuses any frame longer than `limit` bytes.
    pub fn with_limit(limit: usize) -> Reader {
        Reader {
            buffer: Vec::new(),
            limit,
        }
    }

    /// The limit for the frames still to come.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete frame, if one has arrived.
    pub fn next_frame(&mut self) -> Result<Option<String>, FrameError> {
        let Some(header) = self.buffer.get(..4) else {
            return Ok(None);
        };
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if len > self.limit {
            return Err(FrameError::TooLarge(len));
        }
        if self.buffer.len() < 4 + len {
            return Ok(None);
        }
        let payload: Vec<u8> = self.buffer.drain(..4 + len).skip(4).collect();
        String::from_utf8(payload)
            .map(Some)
            .map_err(|_| FrameError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_survive_any_split() {
        let wire = [encode("{\"a\":1}"), encode(""), encode("{\"b\":\"ü\"}")].concat();
        for cut in 0..wire.len() {
            let mut reader = Reader::default();
            let mut got = Vec::new();
            reader.feed(&wire[..cut]);
            while let Some(frame) = reader.next_frame().unwrap() {
                got.push(frame);
            }
            reader.feed(&wire[cut..]);
            while let Some(frame) = reader.next_frame().unwrap() {
                got.push(frame);
            }
            assert_eq!(got, vec!["{\"a\":1}", "", "{\"b\":\"ü\"}"], "cut at {cut}");
        }
    }

    #[test]
    fn an_oversized_or_invalid_frame_is_refused() {
        let mut reader = Reader::default();
        reader.feed(&(MAX_FRAME as u32 + 1).to_be_bytes());
        assert_eq!(
            reader.next_frame(),
            Err(FrameError::TooLarge(MAX_FRAME + 1))
        );
        let mut reader = Reader::default();
        reader.feed(&[0, 0, 0, 2, 0xff, 0xfe]);
        assert_eq!(reader.next_frame(), Err(FrameError::NotUtf8));
    }

    #[test]
    fn a_first_message_gets_a_small_limit_and_later_ones_the_full_one() {
        let hello = "x".repeat(MAX_HELLO_FRAME);
        let mut reader = Reader::with_limit(MAX_HELLO_FRAME);
        reader.feed(&encode(&hello));
        assert_eq!(reader.next_frame(), Ok(Some(hello)));
        // Refused on the length alone, before a byte of the body is waited for.
        let mut reader = Reader::with_limit(MAX_HELLO_FRAME);
        reader.feed(&(MAX_HELLO_FRAME as u32 + 1).to_be_bytes());
        assert_eq!(
            reader.next_frame(),
            Err(FrameError::TooLarge(MAX_HELLO_FRAME + 1))
        );
        // Once raised, a document-sized frame is read whole.
        let document = "y".repeat(MAX_HELLO_FRAME * 64);
        let mut reader = Reader::with_limit(MAX_HELLO_FRAME);
        reader.set_limit(MAX_FRAME);
        reader.feed(&encode(&document));
        assert_eq!(reader.next_frame(), Ok(Some(document)));
    }
}
