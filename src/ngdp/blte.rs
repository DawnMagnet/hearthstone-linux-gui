use super::{read_be_u24, verify_md5};
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

    let header_size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    let mut offset = 8;
    let mut blocks = Vec::new();

    if header_size > 0 {
        anyhow::ensure!(data.len() >= header_size, "BLTE header is truncated");
        anyhow::ensure!(data[offset] == 0x0f, "unsupported BLTE header version");
        offset += 1;
        let block_count = read_be_u24(&data[offset..offset + 3]) as usize;
        offset += 3;
        for _ in 0..block_count {
            let encoded_size =
                u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
            let decoded_size =
                u32::from_be_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let digest = hex::encode(&data[offset + 8..offset + 24]);
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
