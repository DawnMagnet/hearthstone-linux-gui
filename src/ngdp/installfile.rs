use super::bytes::{read_be_u16, read_be_u32, read_u8};
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
        let position = usize::try_from(cursor.position()).context("install header is too large")?;
        validate_install_counts(
            tag_count,
            entry_count,
            hash_size,
            tag_bytes,
            data.len().saturating_sub(position),
        )?;

        let mut tags = HashMap::new();
        for _ in 0..tag_count {
            let name = read_cstr(&mut cursor)?;
            let _tag_type = read_be_u16(&mut cursor)?;
            let mut bytes = vec![0u8; tag_bytes];
            cursor.read_exact(&mut bytes)?;
            tags.insert(name, bytes);
        }

        // The declared count is untrusted. Grow from what we actually read.
        let mut entries = Vec::new();
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

/// `entry_count` and `tag_count` come from the file. Reject a count whose
/// minimum encoding cannot fit in `remaining` before allocating bitmasks or
/// entry slots from it.
fn validate_install_counts(
    tag_count: usize,
    entry_count: usize,
    hash_size: usize,
    tag_bytes: usize,
    remaining: usize,
) -> Result<()> {
    let tags_needed = if tag_count == 0 {
        0
    } else {
        // Shortest tag: one NUL name byte, a u16 type, then the bitmask.
        let per_tag = tag_bytes
            .checked_add(3)
            .context("install tag size overflow")?;
        tag_count
            .checked_mul(per_tag)
            .context("install tag count exceeds file")?
    };
    // Shortest entry: one NUL path byte, the hash, and a u32 size.
    let per_entry = hash_size
        .checked_add(5)
        .context("install entry size overflow")?;
    let entries_needed = entry_count
        .checked_mul(per_entry)
        .context("install entry count exceeds file")?;
    let needed = tags_needed
        .checked_add(entries_needed)
        .context("install entry count exceeds file")?;
    anyhow::ensure!(needed <= remaining, "install entry count exceeds file");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::bytes::md5_hex;
    use super::super::test_support::build_install;
    use super::InstallFile;

    fn basic() -> Vec<u8> {
        build_install(
            1,
            16,
            &[
                ("Windows", 1, &[0b1010_0000]),
                ("enUS", 1, &[0b0110_0000]),
                ("OSX", 1, &[0b1110_0000]),
            ],
            &[
                ("a.txt", &[1; 16], 10),
                ("b.dll", &[2; 16], 20),
                ("c.bin", &[3; 16], 30),
            ],
        )
    }

    fn paths(entries: &[super::InstallEntry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.path.as_str()).collect()
    }

    #[test]
    fn parses_entries() {
        let file = InstallFile::parse(&basic(), "ignored", false).unwrap();
        let kept = file.filter_entries(&[]).unwrap();
        assert_eq!(paths(&kept), ["a.txt", "b.dll", "c.bin"]);
        assert_eq!(kept[0].content_key, hex::encode([1u8; 16]));
        assert_eq!(kept[0].size, 10);
        assert_eq!(kept[1].content_key, hex::encode([2u8; 16]));
        assert_eq!(kept[1].size, 20);
        assert_eq!(kept[2].content_key, hex::encode([3u8; 16]));
        assert_eq!(kept[2].size, 30);
    }

    #[test]
    fn filter_single_tag() {
        let file = InstallFile::parse(&basic(), "ignored", false).unwrap();
        let kept = file.filter_entries(&["Windows"]).unwrap();
        assert_eq!(paths(&kept), ["a.txt", "c.bin"]);
    }

    #[test]
    fn filter_multiple_tags_is_intersection() {
        let file = InstallFile::parse(&basic(), "ignored", false).unwrap();
        let kept = file.filter_entries(&["Windows", "enUS"]).unwrap();
        assert_eq!(paths(&kept), ["c.bin"]);
    }

    #[test]
    fn filter_unknown_tag_is_error() {
        let file = InstallFile::parse(&basic(), "ignored", false).unwrap();
        let error = file.filter_entries(&["xxXX"]).unwrap_err();
        assert!(format!("{error:#}").contains("not present"));
    }

    #[test]
    fn filter_empty_tag_list_returns_all() {
        let file = InstallFile::parse(&basic(), "ignored", false).unwrap();
        assert_eq!(paths(&file.filter_entries(&[]).unwrap()).len(), 3);
    }

    #[test]
    fn bitmask_is_msb_first_across_bytes() {
        let names: Vec<String> = (0..9).map(|index| format!("f{index}")).collect();
        let hashes: Vec<Vec<u8>> = (0..9).map(|index| vec![index]).collect();
        let entries: Vec<(&str, &[u8], u32)> = names
            .iter()
            .zip(hashes.iter())
            .enumerate()
            .map(|(index, (name, hash))| (name.as_str(), hash.as_slice(), index as u32))
            .collect();
        let data = build_install(1, 1, &[("T", 1, &[0x80, 0x80])], &entries);
        let file = InstallFile::parse(&data, "ignored", false).unwrap();
        assert_eq!(paths(&file.filter_entries(&["T"]).unwrap()), ["f0", "f8"]);
    }

    #[test]
    fn wrong_magic_is_error() {
        let mut data = basic();
        data[0] = b'N';
        data[1] = b'I';
        let error = InstallFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("magic"));
    }

    #[test]
    fn version_zero_rejected_newer_accepted() {
        let zero = build_install(0, 16, &[], &[]);
        assert!(InstallFile::parse(&zero, "ignored", false).is_err());
        let two = build_install(2, 16, &[], &[]);
        assert!(InstallFile::parse(&two, "ignored", false).is_ok());
    }

    #[test]
    fn verify_rejects_md5_mismatch() {
        let error = InstallFile::parse(&basic(), &"0".repeat(32), true).unwrap_err();
        assert!(format!("{error:#}").contains("md5"));
    }

    #[test]
    fn verify_accepts_matching_md5() {
        let data = basic();
        let key = md5_hex(&data);
        assert!(InstallFile::parse(&data, &key, true).is_ok());
    }

    #[test]
    fn truncated_header_is_error() {
        assert!(InstallFile::parse(b"IN\x01", "ignored", false).is_err());
        assert!(InstallFile::parse(b"", "ignored", false).is_err());
    }

    #[test]
    fn truncated_tag_bitmask_is_error() {
        // Nine entries need two bitmask bytes. The file stops after one, so
        // the following entry bytes are not available to satisfy the read.
        let mut data = build_install(1, 1, &[("T", 1, &[0x80])], &[]);
        data[6..10].copy_from_slice(&9u32.to_be_bytes());
        assert!(InstallFile::parse(&data, "ignored", false).is_err());
    }

    #[test]
    fn truncated_entry_is_error() {
        let mut data = basic();
        data.truncate(data.len() - 2);
        assert!(InstallFile::parse(&data, "ignored", false).is_err());
    }

    #[test]
    fn path_without_nul_at_eof_is_error() {
        let mut data = Vec::new();
        data.extend_from_slice(b"IN");
        data.push(1);
        data.push(4);
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(b"abc");
        assert!(InstallFile::parse(&data, "ignored", false).is_err());
    }

    #[test]
    fn invalid_utf8_path_is_error() {
        let mut data = Vec::new();
        data.extend_from_slice(b"IN");
        data.push(1);
        data.push(4);
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.push(0xFF);
        data.push(0);
        data.extend_from_slice(&[1, 2, 3, 4]);
        data.extend_from_slice(&9u32.to_be_bytes());
        let error = InstallFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("utf-8"));
    }

    #[test]
    fn zero_tags_zero_entries() {
        let data = build_install(1, 16, &[], &[]);
        let file = InstallFile::parse(&data, "ignored", false).unwrap();
        assert!(file.filter_entries(&[]).unwrap().is_empty());
    }

    // A declared entry_count of u32::MAX must fail from the bytes on hand.
    // It used to `Vec::with_capacity` that count and abort.
    #[test]
    fn huge_entry_count_errors_without_oom() {
        let mut data = build_install(1, 16, &[], &[]);
        data[6..10].copy_from_slice(&u32::MAX.to_be_bytes());
        let error = InstallFile::parse(&data, "ignored", false).unwrap_err();
        assert!(!format!("{error:#}").is_empty());
    }

    #[test]
    fn duplicate_tag_name_last_wins() {
        let data = build_install(
            1,
            16,
            &[
                ("Windows", 1, &[0b1000_0000]),
                ("Windows", 1, &[0b0010_0000]),
            ],
            &[
                ("a.txt", &[1; 16], 10),
                ("b.dll", &[2; 16], 20),
                ("c.bin", &[3; 16], 30),
            ],
        );
        let file = InstallFile::parse(&data, "ignored", false).unwrap();
        assert_eq!(
            paths(&file.filter_entries(&["Windows"]).unwrap()),
            ["c.bin"]
        );
    }
}
