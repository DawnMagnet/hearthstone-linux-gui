use super::{blte, cdn::RemoteCdn, configfile::CdnConfig, verify_md5};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, trace};

#[derive(Clone, Debug)]
struct ArchiveItem {
    archive_key: String,
    size: usize,
    offset: usize,
}

#[cfg(test)]
mod tests {
    use super::ArchiveIndex;

    #[test]
    fn parses_archive_index_footer_layout() {
        let key = "0770d323992903c1ae8a682adc8ce023";
        let footer = [
            0xf6, 0xb0, 0x9e, 0xe7, 0x98, 0x39, 0xf2, 0xb3, 0x01, 0x00, 0x00, 0x04, 0x04, 0x04,
            0x10, 0x08, 0x7a, 0x05, 0x00, 0x00, 0xa2, 0x64, 0x5c, 0xe4, 0xbe, 0x2f, 0x37, 0x7f,
        ];
        let mut data = vec![0; 37108];
        let footer_start = data.len() - footer.len();
        data[footer_start..].copy_from_slice(&footer);

        let index = ArchiveIndex::parse(&data, key, true).expect("archive index should parse");
        assert_eq!(index.items.len(), 1402);
    }
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveMap {
    items: HashMap<String, ArchiveItem>,
}

impl ArchiveMap {
    pub async fn load(
        cdn: &RemoteCdn,
        config: &CdnConfig,
        verify: bool,
        mut progress: impl FnMut(String, Option<f64>),
    ) -> Result<Self> {
        let mut map = HashMap::new();
        debug!(
            archive_group = %config.archive_group,
            archive_count = config.archives.len(),
            "loading archive map"
        );
        for (idx, archive_key) in config.archives.iter().enumerate() {
            let fraction = if config.archives.is_empty() {
                1.0
            } else {
                idx as f64 / config.archives.len() as f64
            };
            progress(
                format!(
                    "Reading archive index {}/{}",
                    idx + 1,
                    config.archives.len()
                ),
                Some(fraction),
            );
            let index = cdn.fetch_data_index(archive_key, verify).await?;
            if let Some(transfer) = cdn.last_transfer_label() {
                progress(
                    format!(
                        "Read archive index {}/{} ({transfer})",
                        idx + 1,
                        config.archives.len()
                    ),
                    Some(fraction),
                );
            }
            let parsed = ArchiveIndex::parse(&index, archive_key, verify)
                .with_context(|| format!("failed to parse index for archive {archive_key}"))?;
            debug!(
                archive_key = %archive_key,
                item_count = parsed.items.len(),
                "parsed archive index"
            );
            for item in parsed.items {
                map.insert(
                    item.key,
                    ArchiveItem {
                        archive_key: archive_key.clone(),
                        size: item.size,
                        offset: item.offset,
                    },
                );
            }
        }
        progress("Archive indices ready".into(), Some(1.0));
        debug!(item_count = map.len(), "archive map ready");

        Ok(Self { items: map })
    }

    pub async fn fetch_file(&self, cdn: &RemoteCdn, key: &str, verify: bool) -> Result<Vec<u8>> {
        let item = self
            .items
            .get(key)
            .context("file is not present in archive map")?;
        debug!(
            key = %key,
            archive_key = %item.archive_key,
            offset = item.offset,
            size = item.size,
            "fetching file from archive"
        );
        let archive = cdn.fetch_data(&item.archive_key, false).await?;
        let end = item.offset + item.size;
        anyhow::ensure!(end <= archive.len(), "archive item exceeds archive size");
        let decoded = blte::decode(&archive[item.offset..end], key, verify)?;
        trace!(
            key = %key,
            archive_key = %item.archive_key,
            encoded_bytes = item.size,
            decoded_bytes = decoded.len(),
            "decoded archive file"
        );
        Ok(decoded)
    }

    pub fn contains(&self, key: &str) -> bool {
        self.items.contains_key(key)
    }
}

#[derive(Clone, Debug)]
struct ArchiveIndexItem {
    key: String,
    size: usize,
    offset: usize,
}

#[derive(Clone, Debug)]
struct ArchiveIndex {
    items: Vec<ArchiveIndexItem>,
}

impl ArchiveIndex {
    fn parse(data: &[u8], key: &str, verify: bool) -> Result<Self> {
        anyhow::ensure!(data.len() >= 28, "archive index is too short");
        let footer_offset = data.len() - 28;
        let footer = &data[footer_offset..];
        verify_md5("archive index footer", footer, key, verify)?;

        let block_size_kb = footer[11] as usize;
        let offset_size = footer[12] as usize;
        let size_size = footer[13] as usize;
        let key_size = footer[14] as usize;
        let num_items = u32::from_le_bytes(footer[16..20].try_into().unwrap()) as usize;
        trace!(
            key = %key,
            block_size_kb = block_size_kb,
            offset_size = offset_size,
            size_size = size_size,
            key_size = key_size,
            num_items = num_items,
            "archive index footer parsed"
        );

        anyhow::ensure!(
            key_size == 16 && size_size == 4 && offset_size == 4,
            "unsupported archive index layout"
        );

        let mut items = Vec::with_capacity(num_items);
        let block_size = block_size_kb * 1024;
        let record_size = key_size + size_size + offset_size;
        let mut position = 0usize;
        let mut bytes_left_in_block = block_size;

        for _ in 0..num_items {
            if record_size > bytes_left_in_block {
                position += bytes_left_in_block;
                bytes_left_in_block = block_size;
            }
            anyhow::ensure!(
                position + record_size <= footer_offset,
                "archive index item is truncated"
            );
            let record = &data[position..position + record_size];
            items.push(ArchiveIndexItem {
                key: hex::encode(&record[0..16]),
                size: u32::from_be_bytes(record[16..20].try_into().unwrap()) as usize,
                offset: u32::from_be_bytes(record[20..24].try_into().unwrap()) as usize,
            });
            position += record_size;
            bytes_left_in_block -= record_size;
        }

        Ok(Self { items })
    }
}
