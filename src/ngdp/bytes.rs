//! Checked integer reads for NGDP and BLTE files.
//!
//! A short or corrupt file returns an error. The previous slice conversions
//! panicked once the declared sizes ran past the buffer.

use anyhow::{Context, Result};
use std::io::{Cursor, Read};

fn array<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .context("integer read offset overflow")?;
    let slice = data.get(offset..end).with_context(|| {
        format!(
            "truncated: need {N} bytes at offset {offset}, have {}",
            data.len()
        )
    })?;
    let mut out = [0u8; N];
    out.copy_from_slice(slice);
    Ok(out)
}

pub(crate) fn be_u16(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(array(data, offset)?))
}

pub(crate) fn be_u32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(array(data, offset)?))
}

pub(crate) fn be_i32(data: &[u8], offset: usize) -> Result<i32> {
    Ok(i32::from_be_bytes(array(data, offset)?))
}

pub(crate) fn le_u32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(array(data, offset)?))
}

pub(crate) fn be_u24(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = array::<3>(data, offset)?;
    Ok(((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32)
}

/// `len` bytes at `offset`, then advances `offset`. An overflowing or
/// truncated range is an error rather than a panic.
pub(crate) fn take<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8]> {
    let start = *offset;
    let end = start.checked_add(len).context("section length overflow")?;
    let slice = data
        .get(start..end)
        .with_context(|| format!("truncated: need {len} bytes at offset {start}"))?;
    *offset = end;
    Ok(slice)
}

pub(crate) fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut byte = [0u8; 1];
    cursor.read_exact(&mut byte)?;
    Ok(byte[0])
}

pub(crate) fn read_be_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut bytes = [0u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

pub(crate) fn read_be_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

/// Hex MD5 of `data`, lowercase, matching the CDN's content keys.
pub(crate) fn md5_hex(data: &[u8]) -> String {
    format!("{:x}", md5::compute(data))
}

#[cfg(test)]
mod tests {
    use super::{
        be_i32, be_u16, be_u24, be_u32, le_u32, md5_hex, read_be_u16, read_be_u32, read_u8, take,
    };
    use std::io::Cursor;

    fn error_text(error: impl std::fmt::Display) -> String {
        format!("{error:#}")
    }

    #[test]
    fn be_u16_reads_big_endian() {
        assert_eq!(be_u16(&[0x12, 0x34], 0).unwrap(), 0x1234);
    }

    #[test]
    fn be_u32_reads_at_offset() {
        assert_eq!(be_u32(&[0xAA, 1, 2, 3, 4], 1).unwrap(), 0x0102_0304);
    }

    #[test]
    fn le_and_be_u32_differ() {
        let data = [1, 2, 3, 4];
        assert_eq!(le_u32(&data, 0).unwrap(), 0x0403_0201);
        assert_eq!(be_u32(&data, 0).unwrap(), 0x0102_0304);
    }

    #[test]
    fn be_i32_reads_minus_one_sentinel() {
        assert_eq!(be_i32(&[0xFF; 4], 0).unwrap(), -1);
    }

    #[test]
    fn be_u24_reads_three_bytes() {
        assert_eq!(be_u24(&[1, 2, 3], 0).unwrap(), 0x0001_0203);
        assert_eq!(be_u24(&[0xFF; 3], 0).unwrap(), 0x00FF_FFFF);
    }

    #[test]
    fn int_read_truncated_is_error() {
        let short = be_u32(&[1, 2, 3], 0).unwrap_err();
        assert!(error_text(short).contains("truncated"));
        let overlap = be_u16(&[1, 2], 1).unwrap_err();
        assert!(error_text(overlap).contains("truncated"));
    }

    #[test]
    fn int_read_offset_past_end_is_error() {
        assert!(be_u16(&[1, 2], 5).is_err());
    }

    #[test]
    fn int_read_offset_overflow_is_error() {
        let error = be_u32(&[0; 4], usize::MAX).unwrap_err();
        assert!(error_text(error).contains("overflow"));
    }

    #[test]
    fn take_returns_slice_and_advances() {
        let mut offset = 1;
        let slice = take(b"abcdef", &mut offset, 3).unwrap();
        assert_eq!(slice, b"bcd");
        assert_eq!(offset, 4);
    }

    #[test]
    fn take_zero_len_at_end() {
        let mut offset = 6;
        let slice = take(b"abcdef", &mut offset, 0).unwrap();
        assert_eq!(slice, b"");
        assert_eq!(offset, 6);
    }

    #[test]
    fn take_truncated_leaves_offset_unchanged() {
        let mut offset = 4;
        assert!(take(b"abcdef", &mut offset, 5).is_err());
        assert_eq!(offset, 4);
    }

    #[test]
    fn take_length_overflow_is_error() {
        let mut offset = 1;
        let error = take(b"abcdef", &mut offset, usize::MAX).unwrap_err();
        assert!(error_text(error).contains("overflow"));
        assert_eq!(offset, 1);
    }

    #[test]
    fn cursor_reads_sequence_then_eof() {
        let mut cursor = Cursor::new(&[0x01, 0x00, 0x02, 0, 0, 0, 0x03][..]);
        assert_eq!(read_u8(&mut cursor).unwrap(), 1);
        assert_eq!(read_be_u16(&mut cursor).unwrap(), 2);
        assert_eq!(read_be_u32(&mut cursor).unwrap(), 3);
        assert!(read_u8(&mut cursor).is_err());
    }

    #[test]
    fn md5_hex_known_vectors() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5_hex(b"abc").len(), 32);
        assert!(md5_hex(b"abc").chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
