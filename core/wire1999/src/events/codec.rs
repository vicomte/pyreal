//! The 1999 string over `pyreal_protocol`'s cursor in its error type, so one reader covers every
//! field width.
//!
//! Read a whole message with [`reader`]: padding counts from the message's opcode.

use pyreal_primitives::text::cp1252;
use pyreal_protocol::{MessageError, Reader, Writer};

use crate::strings::ReadError;

/// A reader over a whole 1999 message, past its opcode.
///
/// # Errors
/// [`MessageError::UnexpectedEof`] for a message shorter than its opcode.
pub fn reader(message: &[u8]) -> Result<Reader<'_>, MessageError> {
    let mut r = Reader::new(message);
    r.u32()?;
    Ok(r)
}

/// A 1999 string, read by [`crate::objects::read_string`] and reported as a [`MessageError`].
///
/// # Errors
/// [`MessageError`] for a short message, a missing or embedded NUL, or non-zero padding.
pub fn read_string(r: &mut Reader<'_>) -> Result<String, MessageError> {
    let at = r.blob_offset();
    crate::objects::read_string(r).map_err(|e| match e {
        ReadError::Short => MessageError::UnexpectedEof {
            at,
            needed: 2,
            available: r.remaining(),
        },
        ReadError::Bad(field) => MessageError::InvalidValue {
            field,
            value: at as u64,
        },
    })
}

/// The inverse of [`read_string`]; text with no Windows-1252 form is written lossily and cut to fit.
pub fn write_string(w: &mut Writer, s: &str) {
    let mut bytes = cp1252::encode_lossy(s);
    bytes.retain(|&b| b != 0);
    bytes.truncate(usize::from(u16::MAX) - 1);
    if bytes.is_empty() {
        w.u16(0);
        return;
    }
    w.u16(u16::try_from(bytes.len() + 1).unwrap_or(u16::MAX));
    w.bytes(&bytes);
    w.u8(0);
    w.align4();
}

/// Everything read, or only zero padding to the next word left.
///
/// # Errors
/// [`MessageError::TrailingBytes`] for anything else.
pub fn finish(r: &mut Reader<'_>) -> Result<(), MessageError> {
    let left = r.remaining();
    if left < 4 && r.rest().iter().all(|&b| b == 0) {
        Ok(())
    } else {
        Err(MessageError::TrailingBytes { left })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_1999_string_pads_from_the_opcode_and_an_empty_one_is_two_bytes() {
        let mut w = Writer::new();
        w.u32(0x0037);
        write_string(&mut w, "Hi");
        write_string(&mut w, "");
        write_string(&mut w, "É");
        let b = w.into_inner();
        assert_eq!(&b[4..12], &[3, 0, b'H', b'i', 0, 0, 0, 0]);
        assert_eq!(&b[12..14], &[0, 0], "empty: the count alone");
        assert_eq!(&b[14..20], &[2, 0, 0xC9, 0, 0, 0]);
        let mut r = reader(&b).unwrap();
        assert_eq!(read_string(&mut r).unwrap(), "Hi");
        assert_eq!(read_string(&mut r).unwrap(), "");
        assert_eq!(read_string(&mut r).unwrap(), "É");
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn a_1999_string_without_its_nul_or_with_dirty_padding_is_refused() {
        let no_nul = [0, 0, 0, 0, 2, 0, b'a', b'b'];
        assert!(read_string(&mut reader(&no_nul).unwrap()).is_err());
        let dirty = [0, 0, 0, 0, 1, 0, 0, 7];
        assert!(read_string(&mut reader(&dirty).unwrap()).is_err());
    }
}
