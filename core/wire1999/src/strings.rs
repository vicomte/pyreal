//! The 1999 string and account field, and a reader and writer for whole 1999 messages.
//!
//! Offsets count from the message's first byte, its opcode, because strings pad to that origin.

use pyreal_primitives::text::cp1252;

/// The account field's marker when the client writes it.
pub const ACCOUNT_MARKER: u32 = 0xBEEF_BEEF;
/// Bytes in an account field.
pub const ACCOUNT_FIELD_SIZE: usize = 28;
/// Why a message did not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadError {
    /// The message ended inside a field.
    Short,
    /// A field broke its rule.
    Bad(&'static str),
}

/// A reader over a whole message, opcode included.
#[derive(Debug)]
pub struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// A reader past the opcode.
    #[must_use]
    pub fn after_opcode(message: &'a [u8]) -> Self {
        Self {
            buf: message,
            at: 4,
        }
    }

    /// A reader at `offset` from the message's first byte.
    #[must_use]
    pub fn at(message: &'a [u8], offset: usize) -> Self {
        Self {
            buf: message,
            at: offset,
        }
    }

    /// Where the next field starts, from the message's first byte.
    #[must_use]
    pub fn position(&self) -> usize {
        self.at
    }

    /// Bytes left.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.at)
    }

    /// The next `n` bytes as they are.
    ///
    /// # Errors
    /// [`ReadError::Short`].
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], ReadError> {
        let b = self.buf.get(self.at..self.at + n).ok_or(ReadError::Short)?;
        self.at += n;
        Ok(b)
    }

    /// # Errors
    /// [`ReadError::Short`].
    pub fn u16(&mut self) -> Result<u16, ReadError> {
        let w = self.bytes(2)?;
        Ok(u16::from_le_bytes([w[0], w[1]]))
    }

    /// # Errors
    /// [`ReadError::Short`].
    pub fn u32(&mut self) -> Result<u32, ReadError> {
        let w = self.bytes(4)?;
        Ok(u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
    }

    /// # Errors
    /// [`ReadError::Short`].
    pub fn i32(&mut self) -> Result<i32, ReadError> {
        self.u32().map(|v| i32::from_le_bytes(v.to_le_bytes()))
    }

    /// Packed, not aligned.
    ///
    /// # Errors
    /// [`ReadError::Short`].
    pub fn f64(&mut self) -> Result<f64, ReadError> {
        let w = self.bytes(8)?;
        Ok(f64::from_le_bytes([
            w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7],
        ]))
    }

    /// Zero bytes up to the message's next four-byte boundary.
    ///
    /// # Errors
    /// [`ReadError`] for a short message or non-zero padding.
    pub fn align(&mut self) -> Result<(), ReadError> {
        let end = self.at.next_multiple_of(4);
        if self.bytes(end - self.at)?.iter().any(|&b| b != 0) {
            return Err(ReadError::Bad("padding"));
        }
        Ok(())
    }

    /// A 1999 string: a u16 length counting one trailing NUL, the bytes, then zero padding to the
    /// message's next four-byte boundary. An empty string is the two length bytes alone.
    ///
    /// # Errors
    /// [`ReadError`] for a short message, a missing or extra NUL, or non-zero padding.
    pub fn string(&mut self) -> Result<String, ReadError> {
        let len = self.buf.get(self.at..self.at + 2).ok_or(ReadError::Short)?;
        let len = usize::from(u16::from_le_bytes([len[0], len[1]]));
        self.at += 2;
        if len == 0 {
            return Ok(String::new());
        }
        let bytes = self
            .buf
            .get(self.at..self.at + len)
            .ok_or(ReadError::Short)?;
        let (&nul, text) = bytes.split_last().ok_or(ReadError::Short)?;
        if nul != 0 || text.contains(&0) {
            return Err(ReadError::Bad("string NUL"));
        }
        self.at += len;
        let end = self.at.next_multiple_of(4);
        let pad = self.buf.get(self.at..end).ok_or(ReadError::Short)?;
        if pad.iter().any(|&b| b != 0) {
            return Err(ReadError::Bad("string padding"));
        }
        self.at = end;
        Ok(cp1252::decode(text))
    }

    /// An account field: marker, 20 bytes of text up to the first NUL, a reserved word.
    ///
    /// # Errors
    /// [`ReadError::Short`].
    pub fn account(&mut self) -> Result<String, ReadError> {
        let f = self
            .buf
            .get(self.at..self.at + ACCOUNT_FIELD_SIZE)
            .ok_or(ReadError::Short)?;
        self.at += ACCOUNT_FIELD_SIZE;
        let text = &f[4..24];
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        Ok(cp1252::decode(&text[..end]))
    }

    /// Everything read, or only zero padding left.
    ///
    /// # Errors
    /// [`ReadError::Bad`] for anything else left over.
    pub fn finish(&self) -> Result<(), ReadError> {
        let rest = self.buf.get(self.at..).unwrap_or_default();
        if rest.len() < 4 && rest.iter().all(|&b| b == 0) {
            Ok(())
        } else {
            Err(ReadError::Bad("trailing bytes"))
        }
    }
}

/// A writer for a whole message, opcode included.
#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    #[must_use]
    pub fn new(opcode: u32) -> Self {
        Self {
            buf: opcode.to_le_bytes().to_vec(),
        }
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    /// Packed, not aligned.
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.buf.extend_from_slice(&v.to_le_bytes());
        self
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(b);
        self
    }

    /// Zero bytes up to the message's next four-byte boundary.
    pub fn align(&mut self) -> &mut Self {
        self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        self
    }

    /// Bytes written, opcode included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// A 1999 string. Text with no Windows-1252 form, or longer than the u16 count allows, is
    /// written as lossy Windows-1252 and cut.
    pub fn string(&mut self, s: &str) -> &mut Self {
        let mut bytes = cp1252::encode_lossy(s);
        bytes.retain(|&b| b != 0);
        bytes.truncate(usize::from(u16::MAX) - 1);
        if bytes.is_empty() {
            self.buf.extend_from_slice(&0u16.to_le_bytes());
            return self;
        }
        let len = u16::try_from(bytes.len() + 1).unwrap_or(u16::MAX);
        self.buf.extend_from_slice(&len.to_le_bytes());
        self.buf.extend_from_slice(&bytes);
        self.buf.push(0);
        self.buf.resize(self.buf.len().next_multiple_of(4), 0);
        self
    }

    /// An account field with `marker`, the text cut to 20 bytes.
    pub fn account(&mut self, marker: u32, account: &str) -> &mut Self {
        let mut text = cp1252::encode_lossy(account);
        text.truncate(20);
        text.resize(20, 0);
        self.buf.extend_from_slice(&marker.to_le_bytes());
        self.buf.extend_from_slice(&text);
        self.buf.extend_from_slice(&0u32.to_le_bytes());
        self
    }

    #[must_use]
    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_pad_to_the_message_origin_and_an_empty_one_does_not_pad() {
        // The spec's worked example: one character at offset 34 pads to 40.
        let mut w = Writer::new(0);
        w.buf.resize(34, 0);
        w.string("\u{c9}");
        assert_eq!(&w.buf[34..], &[0x02, 0x00, 0xC9, 0x00, 0x00, 0x00]);
        let mut r = Reader {
            buf: &w.buf,
            at: 34,
        };
        assert_eq!(r.string(), Ok("\u{c9}".to_owned()));
        assert_eq!(r.at, 40);

        let mut w = Writer::new(0);
        w.string("").u32(7);
        assert_eq!(w.buf, [0, 0, 0, 0, 0, 0, 7, 0, 0, 0]);
        let mut r = Reader::after_opcode(&w.buf);
        assert_eq!(r.string(), Ok(String::new()));
        assert_eq!(r.u32(), Ok(7), "the next field is unaligned");

        let mut bad = Writer::new(0);
        bad.string("ab");
        bad.buf[9] = 1;
        assert!(Reader::after_opcode(&bad.buf).string().is_err(), "padding");
        let bad = [0, 0, 0, 0, 3, 0, b'a', b'b', b'c', 0, 0, 0];
        assert!(Reader::after_opcode(&bad).string().is_err(), "no NUL");
    }

    #[test]
    fn the_account_field_reads_up_to_its_first_nul() {
        let mut w = Writer::new(0);
        w.account(ACCOUNT_MARKER, "demo");
        assert_eq!(w.buf.len(), 4 + ACCOUNT_FIELD_SIZE);
        assert_eq!(&w.buf[4..8], &[0xEF, 0xBE, 0xEF, 0xBE]);
        w.buf[14] = b'x';
        assert_eq!(
            Reader::after_opcode(&w.buf).account(),
            Ok("demo".to_owned())
        );
    }
}
