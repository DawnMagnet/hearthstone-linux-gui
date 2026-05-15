use super::{read_cstr, verify_md5};
use anyhow::{Context, Result};
use bitvec::{order::Msb0, view::BitView};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};

#[derive(Clone, Debug)]
pub struct InstallEntry {
    pub path: String,
    pub content_key: String,
    pub size: u32,
}

#[derive(Clone, Debug)]
pub struct InstallFile {
    tags: HashMap<String, Vec<u8>>,
    entries: Vec<InstallEntry>,
}

impl InstallFile {
    pub fn parse(data: &[u8], content_key: &str, verify: bool) -> Result<Self> {
        verify_md5("install file", data, content_key, verify)?;
        let mut cursor = Cursor::new(data);
        let mut magic = [0u8; 2];
        cursor.read_exact(&mut magic)?;
        anyhow::ensure!(&magic == b"IN", "invalid install file magic");

        let version = read_u8(&mut cursor)?;
        anyhow::ensure!(version >= 1, "unsupported install file version {version}");
        let hash_size = read_u8(&mut cursor)? as usize;
        let tag_count = read_be_u16(&mut cursor)? as usize;
        let entry_count = read_be_u32(&mut cursor)? as usize;
        let tag_bytes = entry_count.div_ceil(8);

        let mut tags = HashMap::new();
        for _ in 0..tag_count {
            let name = read_cstr(&mut cursor)?;
            let _tag_type = read_be_u16(&mut cursor)?;
            let mut bytes = vec![0u8; tag_bytes];
            cursor.read_exact(&mut bytes)?;
            tags.insert(name, bytes);
        }

        let mut entries = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let path = read_cstr(&mut cursor)?;
            let mut digest = vec![0u8; hash_size];
            cursor.read_exact(&mut digest)?;
            let size = read_be_u32(&mut cursor)?;
            entries.push(InstallEntry {
                path,
                content_key: hex::encode(digest),
                size,
            });
        }

        Ok(Self { tags, entries })
    }

    pub fn filter_entries(&self, tags: &[&str]) -> Result<Vec<InstallEntry>> {
        let tag_bits = tags
            .iter()
            .map(|tag| {
                self.tags
                    .get(*tag)
                    .with_context(|| format!("install tag `{tag}` is not present"))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(self
            .entries
            .iter()
            .enumerate()
            .filter(|(idx, _)| {
                tag_bits.iter().all(|bytes| {
                    bytes
                        .view_bits::<Msb0>()
                        .get(*idx)
                        .map(|bit| *bit)
                        .unwrap_or(false)
                })
            })
            .map(|(_, entry)| entry.clone())
            .collect())
    }
}

fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8> {
    let mut byte = [0u8; 1];
    cursor.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn read_be_u16(cursor: &mut Cursor<&[u8]>) -> Result<u16> {
    let mut bytes = [0u8; 2];
    cursor.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_be_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32> {
    let mut bytes = [0u8; 4];
    cursor.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}
