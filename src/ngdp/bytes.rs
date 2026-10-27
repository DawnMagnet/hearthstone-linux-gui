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
