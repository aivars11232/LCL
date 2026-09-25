//! Frames: one JSON message each, behind a four-byte big-endian length.
//!
//! Inside TLS, so the framing needs no integrity of its own — TLS 1.3 already
//! authenticates and orders every byte and refuses a replayed record. What it
//! does need is a bound: a length larger than [`MAX_FRAME`] closes the
//! connection before anything is allocated for it.

/// The largest frame either side accepts: comfortably above any document the
/// workspace will save, and far below what would exhaust memory.
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge(usize),
    NotUtf8,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => {
                write!(f, "a frame of {n} bytes exceeds the {MAX_FRAME}-byte limit")
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
#[derive(Default)]
pub struct Reader {
    buffer: Vec<u8>,
}

impl Reader {
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next complete frame, if one has arrived.
    pub fn next_frame(&mut self) -> Result<Option<String>, FrameError> {
        let Some(header) = self.buffer.get(..4) else {
            return Ok(None);
        };
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        if len > MAX_FRAME {
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
}
