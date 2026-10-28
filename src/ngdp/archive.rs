use super::bytes;
use super::{blte, cdn::RemoteCdn, configfile::CdnConfig, verify_md5};
use crate::phrase;
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

    fn index_with(block_size_kb: u8, num_items: u32) -> Vec<u8> {
        let mut data = vec![0u8; 64];
        let footer = data.len() - 28;
        data[footer + 11] = block_size_kb;
        data[footer + 12] = 4;
        data[footer + 13] = 4;
        data[footer + 14] = 16;
        data[footer + 16..footer + 20].copy_from_slice(&num_items.to_le_bytes());
        data
    }

    #[test]
    fn zero_block_size_is_error_not_panic() {
        let error = ArchiveIndex::parse(&index_with(0, 1), "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("block")
                || message.contains("invalid")
                || message.contains("underflow"),
            "{message}"
        );
    }

    #[test]
    fn huge_item_count_is_error_not_oom() {
        let error = ArchiveIndex::parse(&index_with(4, u32::MAX), "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("exceeds")
                || message.contains("overflow")
                || message.contains("truncated"),
            "{message}"
        );
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
        mut progress: impl FnMut(crate::i18n::Phrase, Option<f64>),
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
                phrase!(
                    "ngdp.archive_reading",
                    current = idx + 1,
                    total = config.archives.len()
                ),
                Some(fraction),
            );
            let index = cdn.fetch_data_index(archive_key, verify).await?;
            if let Some(transfer) = cdn.last_transfer_label() {
                progress(
                    phrase!(
                        "ngdp.archive_read",
                        current = idx + 1,
                        total = config.archives.len(),
                        transfer = transfer
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
        progress(phrase!("ngdp.archive_ready"), Some(1.0));
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
        let encoded = cdn
            .fetch_data_range(&item.archive_key, item.offset, item.size)
            .await?;
        let decoded = blte::decode(&encoded, key, verify)?;
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
        let num_items = bytes::le_u32(footer, 16)? as usize;
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
        // `block_size_kb == 0` used to underflow `bytes_left_in_block -= record`.
        anyhow::ensure!(block_size_kb > 0, "invalid archive index block size");
        let block_size = block_size_kb
            .checked_mul(1024)
            .context("archive index block size overflow")?;
        let record_size = key_size + size_size + offset_size;
        anyhow::ensure!(
            block_size >= record_size,
            "archive index block is smaller than a record"
        );
        let bytes_needed = num_items
            .checked_mul(record_size)
            .context("archive index item count overflows")?;
        anyhow::ensure!(
            bytes_needed <= footer_offset,
            "archive index item count exceeds file"
        );

        let mut items = Vec::new();
        let mut position = 0usize;
        let mut bytes_left_in_block = block_size;

        for _ in 0..num_items {
            if record_size > bytes_left_in_block {
                position = position
                    .checked_add(bytes_left_in_block)
                    .context("archive index offset overflow")?;
                bytes_left_in_block = block_size;
            }
            let record_end = position
                .checked_add(record_size)
                .context("archive index offset overflow")?;
            anyhow::ensure!(
                record_end <= footer_offset,
                "archive index item is truncated"
            );
            let record = &data[position..record_end];
            items.push(ArchiveIndexItem {
                key: hex::encode(&record[0..16]),
                size: bytes::be_u32(record, 16)? as usize,
                offset: bytes::be_u32(record, 20)? as usize,
            });
            position = record_end;
            bytes_left_in_block = bytes_left_in_block
                .checked_sub(record_size)
                .context("archive index block size underflow")?;
        }

        Ok(Self { items })
    }
}
