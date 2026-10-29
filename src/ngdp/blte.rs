use super::bytes;
use super::verify_md5;
use anyhow::{Context, Result};
use flate2::read::ZlibDecoder;
use flate2::{Decompress, FlushDecompress, Status};
use std::io::Read;

/// Largest claimed output we will reserve up front. Release builds abort on a
/// failed allocation, and a block table can claim `u32::MAX` per block.
/// Anything above this grows incrementally, the way master does, so a hostile
/// size still ends in `BLTE decoded block size mismatch` without a huge alloc.
/// 512 MiB covers a legitimate 256 MiB block.
const MAX_PREALLOC: usize = 512 * 1024 * 1024;

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

    // Overflow or a huge claim: do not reserve. Incremental growth then hits
    // the same size check master uses.
    let prealloc = preallocated_len(&blocks);
    let mut decoded = Vec::with_capacity(prealloc);
    let mut zlib = None;
    for block in blocks {
        let end = offset + block.encoded_size;
        anyhow::ensure!(end <= data.len(), "BLTE block is truncated");
        let encoded_block = &data[offset..end];
        if !block.digest.is_empty() {
            verify_md5("BLTE block", encoded_block, &block.digest, verify)?;
        }
        if block.decoded_size == 0 {
            // No declared size: inflate straight into the output. This is the
            // single-frame path, which does not get the trusted reserve.
            append_unknown(&mut decoded, encoded_block)?;
        } else if prealloc > 0 {
            let start = decoded.len();
            append_trusted(&mut decoded, &mut zlib, encoded_block, block.decoded_size)?;
            anyhow::ensure!(
                decoded.len() - start == block.decoded_size,
                "BLTE decoded block size mismatch"
            );
        } else {
            let block_decoded = decode_block_incremental(encoded_block)?;
            anyhow::ensure!(
                block_decoded.len() == block.decoded_size,
                "BLTE decoded block size mismatch"
            );
            decoded.extend_from_slice(&block_decoded);
        }
        offset = end;
    }

    Ok(decoded)
}

/// Claimed output bytes to reserve, or `0` when the sum overflows or exceeds
/// [`MAX_PREALLOC`]. `0` means grow from the real output.
fn preallocated_len(blocks: &[BlockInfo]) -> usize {
    match blocks
        .iter()
        .try_fold(0usize, |sum, block| sum.checked_add(block.decoded_size))
    {
        Some(total) if total > 0 && total <= MAX_PREALLOC => total,
        _ => 0,
    }
}

/// Single-frame and other `decoded_size == 0` blocks. The output size is not
/// in the header, so this grows the real buffer. The first reserve is a
/// capped multiple of the compressed length; after that, growth is geometric
/// until 512 MiB and then fixed steps, so a zlib bomb cannot jump to a
/// multi-gigabyte allocation in one reserve.
fn append_unknown(out: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    let (&kind, payload) = data.split_first().context("empty BLTE block")?;
    match kind {
        b'N' => out.extend_from_slice(payload),
        b'Z' => inflate_growing(out, payload)?,
        other => anyhow::bail!("unsupported BLTE block type `{}`", other as char),
    }
    Ok(())
}

fn inflate_growing(out: &mut Vec<u8>, payload: &[u8]) -> Result<()> {
    let start = out.len();
    let hint = payload
        .len()
        .saturating_mul(6)
        .clamp(64 * 1024, MAX_PREALLOC);
    out.reserve(hint);

    let mut zlib = Decompress::new(true);
    let mut offset = 0usize;
    loop {
        if out.len() == out.capacity() {
            let cap = out.capacity();
            let add = if cap >= MAX_PREALLOC {
                32 * 1024 * 1024
            } else {
                cap.max(64 * 1024)
            };
            out.reserve(add);
        }
        let before_in = zlib.total_in();
        let before_len = out.len();
        match zlib.decompress_vec(&payload[offset..], out, FlushDecompress::Finish) {
            Ok(Status::StreamEnd) => return Ok(()),
            Ok(_) => {
                let consumed =
                    usize::try_from(zlib.total_in().saturating_sub(before_in)).unwrap_or(0);
                offset = offset.saturating_add(consumed).min(payload.len());
                if out.len() == before_len && consumed == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Same reader master uses, so the message and source chain match.
    out.truncate(start);
    let mut full = Vec::new();
    ZlibDecoder::new(payload).read_to_end(&mut full)?;
    out.extend_from_slice(&full);
    Ok(())
}

/// Master’s per-block decode: a fresh buffer that grows with the real output,
/// never with the header’s `decoded_size`.
fn decode_block_incremental(data: &[u8]) -> Result<Vec<u8>> {
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

fn append_trusted(
    out: &mut Vec<u8>,
    zlib: &mut Option<Decompress>,
    data: &[u8],
    expected: usize,
) -> Result<()> {
    let (&kind, payload) = data.split_first().context("empty BLTE block")?;
    match kind {
        b'N' => out.extend_from_slice(payload),
        b'Z' if expected == 0 => {
            // Single-frame files do not declare a decoded size.
            let mut decoder = ZlibDecoder::new(payload);
            decoder.read_to_end(out)?;
        }
        b'Z' => {
            // `decompress_vec` would hand the inflater the entire spare tail.
            // Once the output is reserved for every block, that tail is the
            // rest of the file, and inflate time grows with it. Limit the
            // window to this block.
            out.reserve_exact(expected);
            let start = out.len();
            let written = {
                let spare = out.spare_capacity_mut();
                anyhow::ensure!(spare.len() >= expected, "BLTE decoded block size mismatch");
                let zlib = zlib.get_or_insert_with(|| Decompress::new(true));
                zlib.reset(true);
                let before = zlib.total_out();
                let direct = zlib.decompress_uninit(
                    payload,
                    &mut spare[..expected],
                    FlushDecompress::Finish,
                );
                match direct {
                    Ok(Status::StreamEnd) => {
                        let written = usize::try_from(zlib.total_out().saturating_sub(before))
                            .unwrap_or(expected.saturating_add(1));
                        if written > expected {
                            None
                        } else {
                            Some(written)
                        }
                    }
                    // Corrupt input and a short buffer both miss StreamEnd.
                    // Master's reader turns backend failures into
                    // "corrupt deflate stream" / "incomplete deflate stream"
                    // with no further source. Re-decode so the error matches.
                    Ok(_) | Err(_) => None,
                }
            };
            if let Some(written) = written {
                // Safety: StreamEnd wrote `written` initialized bytes into this
                // block's tail, and `written <= expected <= spare.len()`.
                unsafe { out.set_len(start + written) };
            } else {
                let mut full = Vec::new();
                ZlibDecoder::new(payload).read_to_end(&mut full)?;
                anyhow::ensure!(full.len() == expected, "BLTE decoded block size mismatch");
                out.extend_from_slice(&full);
            }
        }
        other => anyhow::bail!("unsupported BLTE block type `{}`", other as char),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::decode;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

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

    fn chunk(kind: u8, plain: &[u8]) -> Vec<u8> {
        match kind {
            b'N' => {
                let mut out = Vec::with_capacity(1 + plain.len());
                out.push(b'N');
                out.extend_from_slice(plain);
                out
            }
            b'Z' => {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(plain).unwrap();
                let compressed = encoder.finish().unwrap();
                let mut out = Vec::with_capacity(1 + compressed.len());
                out.push(b'Z');
                out.extend_from_slice(&compressed);
                out
            }
            _ => panic!("kind"),
        }
    }

    /// `decoded_sizes` is the size the header claims. Tests pass a lie to
    /// check the mismatch and cap paths.
    fn multi_block(chunks: &[Vec<u8>], decoded_sizes: &[u32]) -> Vec<u8> {
        assert_eq!(chunks.len(), decoded_sizes.len());
        let header_size = 12 + 24 * chunks.len();
        let mut out = Vec::new();
        out.extend_from_slice(b"BLTE");
        out.extend_from_slice(&(u32::try_from(header_size).unwrap()).to_be_bytes());
        out.push(0x0f);
        let count = u32::try_from(chunks.len()).unwrap();
        out.push((count >> 16) as u8);
        out.push((count >> 8) as u8);
        out.push(count as u8);
        for (body, size) in chunks.iter().zip(decoded_sizes) {
            out.extend_from_slice(&(u32::try_from(body.len()).unwrap()).to_be_bytes());
            out.extend_from_slice(&size.to_be_bytes());
            out.extend_from_slice(&md5::compute(body).0);
        }
        assert_eq!(out.len(), header_size);
        for body in chunks {
            out.extend_from_slice(body);
        }
        out
    }

    fn header_key(data: &[u8]) -> String {
        let header_size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
        format!("{:x}", md5::compute(&data[..header_size]))
    }

    #[test]
    fn single_frame_zlib_roundtrips() {
        let plain = b"the card minion spell ".repeat(4000);
        let body = chunk(b'Z', &plain);
        let mut encoded = Vec::new();
        encoded.extend_from_slice(b"BLTE");
        encoded.extend_from_slice(&0u32.to_be_bytes());
        encoded.extend_from_slice(&body);
        assert_eq!(decode(&encoded, "ignored", false).unwrap(), plain);
    }

    #[test]
    fn truncated_z_block_keeps_the_inflate_error() {
        let plain = b"abcdefghijklmnopqrstuvwxyz012345".repeat(30);
        let mut body = chunk(b'Z', &plain);
        body.truncate(body.len().saturating_sub(6).max(2));
        let encoded = multi_block(&[body], &[plain.len() as u32]);
        assert_master_inflate_error(
            &encoded,
            std::io::ErrorKind::UnexpectedEof,
            "incomplete deflate stream",
        );
    }

    fn assert_master_inflate_error(encoded: &[u8], kind: std::io::ErrorKind, message: &str) {
        let err = decode(encoded, "ignored", false).unwrap_err();
        assert_eq!(err.to_string(), message);
        assert_eq!(format!("{err:#}"), message);
        assert!(err.source().is_none(), "unexpected source: {err:#}");
        let io = err
            .downcast_ref::<std::io::Error>()
            .expect("inflate error should be an io::Error");
        assert_eq!(io.kind(), kind);
        assert_eq!(io.to_string(), message);
    }

    fn corrupt_cases() -> Vec<(&'static str, Vec<u8>)> {
        let plain = b"abcdefghijklmnopqrstuvwxyz012345".repeat(40);
        let body = chunk(b'Z', &plain);
        let mut bad_adler = body.clone();
        let last = bad_adler.len() - 1;
        bad_adler[last] ^= 0xff;
        let mut bad_header = body.clone();
        bad_header[1] = 0x00;
        bad_header[2] = 0x00;
        let mut bad_middle = body.clone();
        let mid = 1 + (bad_middle.len() - 1) / 2;
        bad_middle[mid] ^= 0xff;
        let declared = plain.len() as u32;
        vec![
            ("adler", multi_block(&[bad_adler], &[declared])),
            ("header", multi_block(&[bad_header], &[declared])),
            ("middle", multi_block(&[bad_middle], &[declared])),
        ]
    }

    #[test]
    fn corrupt_zlib_matches_master_message() {
        for (name, encoded) in corrupt_cases() {
            let err = decode(&encoded, "ignored", false).unwrap_err();
            assert_eq!(err.to_string(), "corrupt deflate stream", "{name}: {err:#}");
            assert_eq!(format!("{err:#}"), "corrupt deflate stream", "{name}");
            assert!(err.source().is_none(), "{name} source: {err:#}");
            let io = err
                .downcast_ref::<std::io::Error>()
                .unwrap_or_else(|| panic!("{name} was not an io::Error: {err:#}"));
            assert_eq!(io.kind(), std::io::ErrorKind::InvalidInput, "{name}");
        }
    }

    #[test]
    fn multi_block_mixes_raw_and_zlib_blocks() {
        let raw = b"RAW-BLOCK-0123456789abcdef";
        let text = b"Hearthstone/Data/Win/cardxml".repeat(40);
        let parts = [chunk(b'N', raw), chunk(b'Z', &text), chunk(b'N', b"tail")];
        let sizes = [raw.len() as u32, text.len() as u32, 4];
        let encoded = multi_block(&parts, &sizes);
        let decoded = decode(&encoded, &header_key(&encoded), true).unwrap();
        let mut expected = raw.to_vec();
        expected.extend_from_slice(&text);
        expected.extend_from_slice(b"tail");
        assert_eq!(decoded, expected);
    }

    #[test]
    fn decoded_size_mismatch_keeps_its_error() {
        let plain = b"abcdefghij";
        let encoded = multi_block(&[chunk(b'N', plain)], &[plain.len() as u32 + 5]);
        let message = format!("{:#}", decode(&encoded, "ignored", false).unwrap_err());
        assert!(
            message.contains("BLTE decoded block size mismatch"),
            "{message}"
        );
    }

    #[test]
    fn zlib_block_shorter_than_declared_keeps_size_mismatch() {
        let plain = b"hello hello hello";
        let encoded = multi_block(&[chunk(b'Z', plain)], &[4]);
        let message = format!("{:#}", decode(&encoded, "ignored", false).unwrap_err());
        assert!(
            message.contains("BLTE decoded block size mismatch"),
            "{message}"
        );
    }

    fn vm_hwm_kb() -> u64 {
        let text = std::fs::read_to_string("/proc/self/status").unwrap();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                return rest.split_whitespace().next().unwrap().parse().unwrap();
            }
        }
        panic!("VmHWM missing");
    }

    fn assert_mismatch(encoded: &[u8]) {
        assert_eq!(
            decode(encoded, "ignored", false).unwrap_err().to_string(),
            "BLTE decoded block size mismatch"
        );
    }

    /// Master stays around 2 MiB on these inputs. 4 MiB is 2× that. Measured in
    /// a child process so a parallel `cargo test` cannot bump VmHWM underneath us.
    fn assert_mismatch_without_a_huge_alloc(encoded: &[u8]) {
        let before = vm_hwm_kb();
        assert_mismatch(encoded);
        let grew = vm_hwm_kb().saturating_sub(before);
        assert!(
            grew <= 4 * 1024,
            "VmHWM grew by {grew} KiB on a hostile decoded_size"
        );
    }

    #[test]
    fn decoded_size_u32_max_matches_master_error() {
        let encoded = multi_block(&[chunk(b'N', &[0x11])], &[u32::MAX]);
        assert_mismatch(&encoded);
    }

    #[test]
    fn four_thousand_u32_max_blocks_match_master_error() {
        let body = chunk(b'N', &[0x11]);
        let chunks = vec![body; 4096];
        let sizes = vec![u32::MAX; 4096];
        let encoded = multi_block(&chunks, &sizes);
        assert_mismatch(&encoded);
    }

    #[test]
    fn hostile_decoded_sizes_do_not_raise_rss() {
        if std::env::var_os("BLTE_RSS_CHILD").is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ngdp::blte::tests::hostile_decoded_sizes_do_not_raise_rss",
                    "--test-threads=1",
                ])
                .env("BLTE_RSS_CHILD", "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let one = multi_block(&[chunk(b'N', &[0x11])], &[u32::MAX]);
        assert_mismatch_without_a_huge_alloc(&one);
        let body = chunk(b'N', &[0x11]);
        let many = multi_block(&vec![body; 4096], &vec![u32::MAX; 4096]);
        assert_mismatch_without_a_huge_alloc(&many);
    }

    #[test]
    fn overflowing_decoded_size_sum_is_not_reserved() {
        let blocks = [
            super::BlockInfo {
                encoded_size: 1,
                decoded_size: usize::MAX,
                digest: String::new(),
            },
            super::BlockInfo {
                encoded_size: 1,
                decoded_size: 1,
                digest: String::new(),
            },
        ];
        assert_eq!(super::preallocated_len(&blocks), 0);
    }

    #[test]
    fn decoded_size_zero_is_unchecked() {
        let plain = b"unchecked-size-payload".repeat(8);
        let encoded = multi_block(&[chunk(b'Z', &plain)], &[0]);
        let decoded = decode(&encoded, "ignored", false).unwrap();
        assert_eq!(decoded, plain);
    }

    #[test]
    fn zero_blocks_decode_to_empty() {
        let mut data = b"BLTE".to_vec();
        data.extend_from_slice(&12u32.to_be_bytes());
        data.extend_from_slice(&[0x0f, 0, 0, 0]);
        assert!(decode(&data, "ignored", false).unwrap().is_empty());
    }

    #[test]
    fn trailing_bytes_after_the_block_table_are_ignored() {
        let plain = b"payload-with-a-tail";
        let mut encoded = multi_block(&[chunk(b'N', plain)], &[plain.len() as u32]);
        encoded.extend_from_slice(&[0xab; 32]);
        assert_eq!(decode(&encoded, "ignored", false).unwrap(), plain);
    }

    #[test]
    fn claim_of_1kib_that_inflates_to_256mib_keeps_size_mismatch() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let zeros = [0u8; 64 * 1024];
        let mut left = 256 * 1024 * 1024;
        while left > 0 {
            let n = zeros.len().min(left);
            encoder.write_all(&zeros[..n]).unwrap();
            left -= n;
        }
        let compressed = encoder.finish().unwrap();
        let mut body = Vec::with_capacity(1 + compressed.len());
        body.push(b'Z');
        body.extend_from_slice(&compressed);
        let encoded = multi_block(&[body], &[1024]);
        assert_mismatch(&encoded);
    }

    #[test]
    fn legitimate_256mib_block_roundtrips() {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let zeros = [0u8; 64 * 1024];
        let total = 256 * 1024 * 1024;
        let mut left = total;
        while left > 0 {
            let n = zeros.len().min(left);
            encoder.write_all(&zeros[..n]).unwrap();
            left -= n;
        }
        let compressed = encoder.finish().unwrap();
        let mut body = Vec::with_capacity(1 + compressed.len());
        body.push(b'Z');
        body.extend_from_slice(&compressed);
        let encoded = multi_block(&[body], &[u32::try_from(total).unwrap()]);
        let decoded = decode(&encoded, &header_key(&encoded), true).unwrap();
        assert_eq!(decoded.len(), total);
        assert!(decoded.iter().all(|byte| *byte == 0));
    }
}
