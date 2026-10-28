use super::bytes;
use super::verify_md5;
use anyhow::{Context, Result};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct EncodingFile {
    content_to_encoding: HashMap<String, String>,
    encoding_specs: HashMap<String, String>,
}

impl EncodingFile {
    pub fn parse(data: &[u8], content_key: &str, verify: bool) -> Result<Self> {
        verify_md5("encoding file", data, content_key, verify)?;
        anyhow::ensure!(data.len() >= 22, "encoding file is too short");
        anyhow::ensure!(&data[0..2] == b"EN", "invalid encoding file magic");
        anyhow::ensure!(data[2] == 1, "unsupported encoding file version");

        let content_hash_size = data[3] as usize;
        let encoding_hash_size = data[4] as usize;
        // A zero hash width is not a key: every entry would share the empty
        // string, and a page walk whose step is only those key bytes would
        // never advance.
        anyhow::ensure!(
            content_hash_size > 0 && encoding_hash_size > 0,
            "invalid encoding hash size"
        );
        let content_page_table_page_size = bytes::be_u16(data, 5)? as usize;
        let encoding_page_table_page_size = bytes::be_u16(data, 7)? as usize;
        let content_page_table_page_count = bytes::be_u32(data, 9)? as usize;
        let encoding_page_table_page_count = bytes::be_u32(data, 13)? as usize;
        let encoding_spec_block_size = bytes::be_u32(data, 18)? as usize;

        let mut offset = 22;
        let spec_data = bytes::take(data, &mut offset, encoding_spec_block_size)?;
        let specs: Vec<String> = spec_data
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();

        let content_index_stride = content_hash_size
            .checked_mul(2)
            .context("encoding section size overflow")?;
        let content_index_size = scaled_len(content_page_table_page_count, content_index_stride)?;
        bytes::take(data, &mut offset, content_index_size)?;
        let content_table_size =
            table_byte_len(content_page_table_page_count, content_page_table_page_size)?;
        let content_table = bytes::take(data, &mut offset, content_table_size)?;

        let encoding_index_stride = encoding_hash_size
            .checked_mul(2)
            .context("encoding section size overflow")?;
        let encoding_index_size =
            scaled_len(encoding_page_table_page_count, encoding_index_stride)?;
        bytes::take(data, &mut offset, encoding_index_size)?;
        let encoding_table_size = table_byte_len(
            encoding_page_table_page_count,
            encoding_page_table_page_size,
        )?;
        let encoding_table = bytes::take(data, &mut offset, encoding_table_size)?;

        let mut content_to_encoding = HashMap::new();
        if content_page_table_page_count > 0 {
            let page_size = checked_page_size(content_page_table_page_size)?;
            for page in content_table
                .chunks(page_size)
                .take(content_page_table_page_count)
            {
                parse_content_page(
                    page,
                    content_hash_size,
                    encoding_hash_size,
                    &mut content_to_encoding,
                )?;
            }
        }

        let mut encoding_specs = HashMap::new();
        if encoding_page_table_page_count > 0 {
            let page_size = checked_page_size(encoding_page_table_page_size)?;
            for page in encoding_table
                .chunks(page_size)
                .take(encoding_page_table_page_count)
            {
                parse_encoding_page(page, encoding_hash_size, &specs, &mut encoding_specs)?;
            }
        }

        Ok(Self {
            content_to_encoding,
            encoding_specs,
        })
    }

    pub fn find_by_content_key(&self, key: &str) -> Option<&str> {
        self.content_to_encoding.get(key).map(String::as_str)
    }

    pub fn encoding_spec(&self, key: &str) -> Option<&str> {
        self.encoding_specs.get(key).map(String::as_str)
    }
}

fn scaled_len(count: usize, stride: usize) -> Result<usize> {
    count
        .checked_mul(stride)
        .context("encoding section size overflow")
}

/// Page size is in KiB. Zero reaches `slice::chunks(0)`, which panics.
fn checked_page_size(page_size_kb: usize) -> Result<usize> {
    anyhow::ensure!(page_size_kb > 0, "invalid encoding page size");
    page_size_kb
        .checked_mul(1024)
        .context("invalid encoding page size")
}

fn table_byte_len(count: usize, page_size_kb: usize) -> Result<usize> {
    if count == 0 {
        return Ok(0);
    }
    let page_size = checked_page_size(page_size_kb)?;
    count
        .checked_mul(page_size)
        .context("encoding table size overflow")
}

fn parse_content_page(
    page: &[u8],
    content_hash_size: usize,
    encoding_hash_size: usize,
    content_to_encoding: &mut HashMap<String, String>,
) -> Result<()> {
    let mut ofs = 0usize;
    // Header is key count (1) plus the 40-bit size (5), then one content key
    // and the first encoding key. Further encoding keys follow that.
    let header = 6 + content_hash_size + encoding_hash_size;
    while ofs < page.len() {
        let Some(header_end) = ofs.checked_add(header).filter(|end| *end <= page.len()) else {
            break;
        };
        let key_count = page[ofs] as usize;
        if key_count == 0 {
            break;
        }
        let content_off = ofs + 6;
        let content_key = hex::encode(&page[content_off..content_off + content_hash_size]);
        let ekey_off = content_off + content_hash_size;
        let encoding_key = hex::encode(&page[ekey_off..ekey_off + encoding_hash_size]);
        let extra_keys = key_count.saturating_sub(1);
        let tail = extra_keys
            .checked_mul(encoding_hash_size)
            .context("encoding content entry size overflow")?;
        let next = header_end
            .checked_add(tail)
            .context("encoding content entry size overflow")?;
        anyhow::ensure!(next > ofs, "encoding content entry has zero length");
        anyhow::ensure!(next <= page.len(), "encoding content entry is truncated");
        content_to_encoding.insert(content_key, encoding_key);
        ofs = next;
    }
    Ok(())
}

fn parse_encoding_page(
    page: &[u8],
    encoding_hash_size: usize,
    specs: &[String],
    encoding_specs: &mut HashMap<String, String>,
) -> Result<()> {
    // 4-byte spec index plus 5-byte size. `<=` keeps an entry that ends on
    // the last byte; `<` dropped that entry.
    let stride = encoding_hash_size
        .checked_add(9)
        .context("encoding spec entry size overflow")?;
    anyhow::ensure!(stride > 0, "encoding spec entry has zero length");
    let mut ofs = 0usize;
    while let Some(end) = ofs.checked_add(stride).filter(|end| *end <= page.len()) {
        let spec_index = bytes::be_i32(page, ofs + encoding_hash_size)?;
        if spec_index == -1 {
            break;
        }
        let encoding_key = hex::encode(&page[ofs..ofs + encoding_hash_size]);
        let spec = usize::try_from(spec_index)
            .ok()
            .and_then(|index| specs.get(index))
            .cloned()
            .unwrap_or_default();
        encoding_specs.insert(encoding_key, spec);
        // `end` is strictly past `ofs` because `stride > 0`.
        ofs = end;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::bytes::md5_hex;
    use super::super::test_support::{build_encoding, CEntry, EEntry};
    use super::EncodingFile;

    fn content(byte: u8) -> CEntry {
        CEntry {
            key_count: 1,
            size_hi: 0,
            size: 10,
            ckey: vec![byte; 16],
            ekeys: vec![vec![byte.wrapping_add(1); 16]],
        }
    }

    fn espec(byte: u8, spec_index: i32) -> EEntry {
        EEntry {
            ekey: vec![byte; 16],
            spec_index,
            size: [0; 5],
        }
    }

    fn sample() -> Vec<u8> {
        build_encoding(
            16,
            16,
            1,
            1,
            &["z"],
            vec![vec![CEntry {
                key_count: 1,
                size_hi: 0,
                size: 10,
                ckey: vec![0x11; 16],
                ekeys: vec![vec![0x22; 16]],
            }]],
            vec![vec![EEntry {
                ekey: vec![0x22; 16],
                spec_index: 0,
                size: [0; 5],
            }]],
        )
    }

    fn parse_unchecked(data: &[u8]) -> EncodingFile {
        EncodingFile::parse(data, "ignored", false).unwrap()
    }

    #[test]
    fn parses_single_content_and_encoding_entry() {
        let parsed = parse_unchecked(&sample());
        assert_eq!(
            parsed.find_by_content_key(&"11".repeat(16)),
            Some("22".repeat(16).as_str())
        );
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some("z"));
    }

    #[test]
    fn verify_true_accepts_matching_md5() {
        let data = sample();
        let key = md5_hex(&data);
        assert!(EncodingFile::parse(&data, &key, true).is_ok());
    }

    #[test]
    fn verify_true_rejects_md5_mismatch() {
        let error = EncodingFile::parse(&sample(), &"0".repeat(32), true).unwrap_err();
        assert!(format!("{error:#}").contains("md5"));
    }

    #[test]
    fn too_short_is_error() {
        for data in [Vec::new(), sample()[..21].to_vec()] {
            let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
            assert!(
                format!("{error:#}").contains("too short"),
                "len {}",
                data.len()
            );
        }
    }

    #[test]
    fn wrong_magic_is_error() {
        let mut data = sample();
        data[0] = b'E';
        data[1] = b'M';
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("magic"));
    }

    #[test]
    fn unsupported_version_is_error() {
        let mut data = sample();
        data[2] = 2;
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("version"));
    }

    #[test]
    fn truncated_spec_block_is_error() {
        let mut data = sample();
        data[18..22].copy_from_slice(&100u32.to_be_bytes());
        data.truncate(22);
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("truncated"));
    }

    #[test]
    fn truncated_content_table_is_error() {
        let mut data = build_encoding(
            16,
            16,
            1,
            1,
            &["z"],
            vec![vec![content(0x11)], vec![content(0x33)]],
            vec![vec![espec(0x22, 0)]],
        );
        data.truncate(data.len() - 1024);
        assert!(EncodingFile::parse(&data, "ignored", false).is_err());
    }

    #[test]
    fn huge_page_count_is_error_not_oom() {
        let mut data = sample();
        data[9..13].copy_from_slice(&u32::MAX.to_be_bytes());
        data.truncate(22 + 2);
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        assert!(format!("{error:#}").contains("truncated"));
    }

    #[test]
    fn multi_key_entry_maps_first_ekey_and_advances() {
        let data = build_encoding(
            16,
            16,
            1,
            1,
            &["z"],
            vec![vec![
                CEntry {
                    key_count: 2,
                    size_hi: 0,
                    size: 10,
                    ckey: vec![0x11; 16],
                    ekeys: vec![vec![0x22; 16], vec![0x33; 16]],
                },
                CEntry {
                    key_count: 1,
                    size_hi: 0,
                    size: 20,
                    ckey: vec![0x44; 16],
                    ekeys: vec![vec![0x55; 16]],
                },
            ]],
            vec![],
        );
        let parsed = parse_unchecked(&data);
        assert_eq!(
            parsed.find_by_content_key(&"11".repeat(16)),
            Some("22".repeat(16).as_str())
        );
        assert_eq!(
            parsed.find_by_content_key(&"44".repeat(16)),
            Some("55".repeat(16).as_str())
        );
    }

    #[test]
    fn zero_key_count_ends_page() {
        let data = build_encoding(
            16,
            16,
            1,
            1,
            &[],
            vec![vec![content(0x11)], vec![content(0x33)]],
            vec![],
        );
        let parsed = parse_unchecked(&data);
        assert_eq!(
            parsed.find_by_content_key(&"11".repeat(16)),
            Some("12".repeat(16).as_str())
        );
        assert_eq!(
            parsed.find_by_content_key(&"33".repeat(16)),
            Some("34".repeat(16).as_str())
        );
    }

    #[test]
    fn multiple_encoding_pages_parsed() {
        let data = build_encoding(
            16,
            16,
            1,
            1,
            &["z", "raw"],
            vec![],
            vec![vec![espec(0x22, 0)], vec![espec(0x44, 1)]],
        );
        let parsed = parse_unchecked(&data);
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some("z"));
        assert_eq!(parsed.encoding_spec(&"44".repeat(16)), Some("raw"));
    }

    #[test]
    fn spec_index_minus_one_ends_encoding_page() {
        let data = build_encoding(
            16,
            16,
            1,
            1,
            &["z"],
            vec![],
            vec![vec![espec(0x22, 0), espec(0x33, -1), espec(0x44, 0)]],
        );
        let parsed = parse_unchecked(&data);
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some("z"));
        assert_eq!(parsed.encoding_spec(&"33".repeat(16)), None);
        assert_eq!(parsed.encoding_spec(&"44".repeat(16)), None);
    }

    #[test]
    fn spec_index_out_of_range_is_empty_string() {
        let data = build_encoding(16, 16, 1, 1, &["z"], vec![], vec![vec![espec(0x22, 5)]]);
        let parsed = parse_unchecked(&data);
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some(""));
    }

    #[test]
    fn file_size_high_byte_does_not_break_parse() {
        let data = build_encoding(
            16,
            16,
            1,
            1,
            &["z"],
            vec![vec![CEntry {
                key_count: 1,
                size_hi: 0x01,
                size: 10,
                ckey: vec![0x11; 16],
                ekeys: vec![vec![0x22; 16]],
            }]],
            vec![vec![EEntry {
                ekey: vec![0x22; 16],
                spec_index: 0,
                size: [0; 5],
            }]],
        );
        let parsed = parse_unchecked(&data);
        assert_eq!(
            parsed.find_by_content_key(&"11".repeat(16)),
            Some("22".repeat(16).as_str())
        );
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some("z"));
    }

    #[test]
    fn non_16_byte_hash_sizes() {
        let data = build_encoding(
            9,
            9,
            1,
            1,
            &["z"],
            vec![vec![CEntry {
                key_count: 1,
                size_hi: 0,
                size: 4,
                ckey: vec![0xAB; 9],
                ekeys: vec![vec![0xCD; 9]],
            }]],
            vec![],
        );
        let parsed = parse_unchecked(&data);
        let ckey = hex::encode(vec![0xABu8; 9]);
        let ekey = hex::encode(vec![0xCDu8; 9]);
        assert_eq!(ckey.len(), 18);
        assert_eq!(parsed.find_by_content_key(&ckey), Some(ekey.as_str()));
    }

    #[test]
    fn empty_spec_strings_shift_indices() {
        // B13: empty spec strings are dropped, so index 1 resolves to "b"
        // rather than the empty string that occupies that slot on disk.
        let mut data = build_encoding(16, 16, 1, 1, &[], vec![], vec![vec![espec(0x22, 1)]]);
        let spec = b"a\0\0b\0";
        data[18..22].copy_from_slice(&(spec.len() as u32).to_be_bytes());
        let header = 22;
        data.splice(header..header, spec.iter().copied());
        let parsed = parse_unchecked(&data);
        assert_eq!(parsed.encoding_spec(&"22".repeat(16)), Some("b"));
    }

    // An encoding entry that ends on the last byte of the page is still an
    // entry. ehs = 23 makes each record 32 bytes, so 32 records fill 1 KiB.
    #[test]
    fn entry_exactly_filling_encoding_page_is_parsed() {
        let ehs = 23;
        let entries: Vec<EEntry> = (0..32)
            .map(|index| EEntry {
                ekey: {
                    let mut key = vec![0; ehs];
                    key[0] = index;
                    key
                },
                spec_index: 0,
                size: [0; 5],
            })
            .collect();
        let last = hex::encode(&entries[31].ekey);
        let data = build_encoding(16, ehs as u8, 1, 1, &["z"], vec![], vec![entries]);
        let parsed = EncodingFile::parse(&data, "ignored", false).unwrap();
        assert_eq!(parsed.encoding_spec(&last), Some("z"));
    }

    // A page size of 0 used to reach `chunks(0)`, which panics.
    #[test]
    fn zero_page_size_is_error_not_panic() {
        let data = build_encoding(16, 16, 0, 1, &["z"], vec![vec![]], vec![]);
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("page")
                || message.contains("truncated")
                || message.contains("invalid"),
            "{message}"
        );
    }

    #[test]
    fn zero_encoding_page_size_is_error_not_panic() {
        let data = build_encoding(16, 16, 1, 0, &["z"], vec![], vec![vec![]]);
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("page") || message.contains("invalid"),
            "{message}"
        );
    }

    // Hash width 0 collapses every key to "" and must not be walked.
    #[test]
    fn zero_hash_sizes_are_error_not_a_spin() {
        let data = build_encoding(
            0,
            0,
            1,
            1,
            &["z"],
            vec![vec![CEntry {
                key_count: 1,
                size_hi: 0,
                size: 1,
                ckey: vec![],
                ekeys: vec![vec![]],
            }]],
            vec![],
        );
        let error = EncodingFile::parse(&data, "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("hash") || message.contains("zero") || message.contains("invalid"),
            "{message}"
        );
    }
}
