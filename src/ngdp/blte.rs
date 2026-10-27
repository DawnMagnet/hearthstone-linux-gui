use super::bytes;
use super::verify_md5;
use anyhow::{Context, Result};
use flate2::read::ZlibDecoder;
use std::io::Read;

#[derive(Clone, Debug)]
struct BlockInfo {
    encoded_size: usize,
    decoded_size: usize,
    digest: String,
}

pub fn decode(data: &[u8], key: &str, verify: bool) -> Result<Vec<u8>> {
    anyhow::ensure!(data.len() >= 8, "BLTE data is too short");
    anyhow::ensure!(&data[0..4] == b"BLTE", "missing BLTE magic");

    let header_size = bytes::be_u32(data, 4)? as usize;
    let mut offset = 8;
    let mut blocks = Vec::new();

    if header_size > 0 {
        anyhow::ensure!(data.len() >= header_size, "BLTE header is truncated");
        anyhow::ensure!(data[offset] == 0x0f, "unsupported BLTE header version");
        offset += 1;
        let block_count = bytes::be_u24(data, offset)? as usize;
        offset += 3;
        for _ in 0..block_count {
            let encoded_size = bytes::be_u32(data, offset)? as usize;
            let decoded_size = bytes::be_u32(data, offset + 4)? as usize;
            let digest = hex::encode(
                data.get(offset + 8..offset + 24)
                    .context("truncated BLTE block table")?,
            );
            offset += 24;
            blocks.push(BlockInfo {
                encoded_size,
                decoded_size,
                digest,
            });
        }
        verify_md5("BLTE header", &data[..header_size], key, verify)?;
        offset = header_size;
    } else {
        verify_md5("single-frame BLTE", data, key, verify)?;
        blocks.push(BlockInfo {
            encoded_size: data.len() - offset,
            decoded_size: 0,
            digest: String::new(),
        });
    }

    let mut decoded = Vec::new();
    for block in blocks {
        let end = offset + block.encoded_size;
        anyhow::ensure!(end <= data.len(), "BLTE block is truncated");
        let encoded_block = &data[offset..end];
        if !block.digest.is_empty() {
            verify_md5("BLTE block", encoded_block, &block.digest, verify)?;
        }
        let block_decoded = decode_block(encoded_block)?;
        if block.decoded_size != 0 {
            anyhow::ensure!(
                block_decoded.len() == block.decoded_size,
                "BLTE decoded block size mismatch"
            );
        }
        decoded.extend_from_slice(&block_decoded);
        offset = end;
    }

    Ok(decoded)
}

fn decode_block(data: &[u8]) -> Result<Vec<u8>> {
    let (&kind, payload) = data.split_first().context("empty BLTE block")?;
    match kind {
        b'N' => Ok(payload.to_vec()),
        b'Z' => {
            let mut decoder = ZlibDecoder::new(payload);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            Ok(out)
        }
        other => anyhow::bail!("unsupported BLTE block type `{}`", other as char),
    }
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn truncated_block_table_is_an_error() {
        // Header claims one block, but the table stops after the count.
        let mut data = b"BLTE".to_vec();
        data.extend_from_slice(&12u32.to_be_bytes());
        data.extend_from_slice(&[0x0f, 0x00, 0x00, 0x01]);
        let error = decode(&data, "ignored", false).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("truncated"),
            "expected a truncation error, got {message}"
        );
    }
}
