//! Hand-built NGDP fixtures. These are synthetic layouts, not Blizzard files.

use flate2::{write::ZlibEncoder, Compression};
use std::io::Write;

#[derive(Clone, Debug)]
pub struct CEntry {
    pub key_count: u8,
    pub size_hi: u8,
    pub size: u32,
    pub ckey: Vec<u8>,
    pub ekeys: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct EEntry {
    pub ekey: Vec<u8>,
    pub spec_index: i32,
    pub size: [u8; 5],
}

pub fn build_encoding(
    chs: u8,
    ehs: u8,
    cpage_kb: u16,
    epage_kb: u16,
    specs: &[&str],
    ce_pages: Vec<Vec<CEntry>>,
    e_pages: Vec<Vec<EEntry>>,
) -> Vec<u8> {
    let spec_block = spec_bytes(specs);
    let mut data = Vec::new();
    data.extend_from_slice(b"EN");
    data.push(1);
    data.push(chs);
    data.push(ehs);
    data.extend_from_slice(&cpage_kb.to_be_bytes());
    data.extend_from_slice(&epage_kb.to_be_bytes());
    data.extend_from_slice(&(ce_pages.len() as u32).to_be_bytes());
    data.extend_from_slice(&(e_pages.len() as u32).to_be_bytes());
    data.push(0);
    data.extend_from_slice(&(spec_block.len() as u32).to_be_bytes());
    data.extend_from_slice(&spec_block);

    let chs = chs as usize;
    let ehs = ehs as usize;
    data.extend(std::iter::repeat_n(0, ce_pages.len() * chs * 2));
    for page in &ce_pages {
        data.extend(render_page((cpage_kb as usize) * 1024, |bytes| {
            write_content_page(bytes, chs, ehs, page);
        }));
    }
    data.extend(std::iter::repeat_n(0, e_pages.len() * ehs * 2));
    for page in &e_pages {
        data.extend(render_page((epage_kb as usize) * 1024, |bytes| {
            write_encoding_page(bytes, ehs, page);
        }));
    }
    data
}

pub fn build_install(
    version: u8,
    hash_size: u8,
    tags: &[(&str, u16, &[u8])],
    entries: &[(&str, &[u8], u32)],
) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"IN");
    data.push(version);
    data.push(hash_size);
    data.extend_from_slice(&(u16::try_from(tags.len()).unwrap()).to_be_bytes());
    data.extend_from_slice(&(u32::try_from(entries.len()).unwrap()).to_be_bytes());
    for (name, kind, bits) in tags {
        data.extend_from_slice(name.as_bytes());
        data.push(0);
        data.extend_from_slice(&kind.to_be_bytes());
        data.extend_from_slice(bits);
    }
    for (path, hash, size) in entries {
        assert_eq!(
            hash.len(),
            hash_size as usize,
            "fixture hash width does not match hash_size"
        );
        data.extend_from_slice(path.as_bytes());
        data.push(0);
        data.extend_from_slice(hash);
        data.extend_from_slice(&size.to_be_bytes());
    }
    data
}

pub fn blte_plain(payload: &[u8]) -> Vec<u8> {
    let mut data = Vec::with_capacity(9 + payload.len());
    data.extend_from_slice(b"BLTE");
    data.extend_from_slice(&0u32.to_be_bytes());
    data.push(b'N');
    data.extend_from_slice(payload);
    data
}

pub fn blte_zlib(payload: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload).expect("zlib encode");
    let compressed = encoder.finish().expect("zlib finish");
    let mut data = Vec::with_capacity(9 + compressed.len());
    data.extend_from_slice(b"BLTE");
    data.extend_from_slice(&0u32.to_be_bytes());
    data.push(b'Z');
    data.extend_from_slice(&compressed);
    data
}

fn spec_bytes(specs: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for spec in specs {
        out.extend_from_slice(spec.as_bytes());
        out.push(0);
    }
    out
}

fn render_page(page_bytes: usize, write_entries: impl FnOnce(&mut [u8])) -> Vec<u8> {
    let mut page = vec![0u8; page_bytes];
    if page_bytes > 0 {
        write_entries(&mut page);
    }
    page
}

fn write_content_page(page: &mut [u8], chs: usize, ehs: usize, entries: &[CEntry]) {
    let mut ofs = 0;
    for entry in entries {
        let need = 6 + chs + ehs * entry.ekeys.len();
        assert!(
            ofs + need <= page.len(),
            "content entry does not fit in the page"
        );
        page[ofs] = entry.key_count;
        page[ofs + 1] = entry.size_hi;
        page[ofs + 2..ofs + 6].copy_from_slice(&entry.size.to_be_bytes());
        ofs += 6;
        assert_eq!(entry.ckey.len(), chs, "content key width");
        page[ofs..ofs + chs].copy_from_slice(&entry.ckey);
        ofs += chs;
        for ekey in &entry.ekeys {
            assert_eq!(ekey.len(), ehs, "encoding key width");
            page[ofs..ofs + ehs].copy_from_slice(ekey);
            ofs += ehs;
        }
    }
}

fn write_encoding_page(page: &mut [u8], ehs: usize, entries: &[EEntry]) {
    let mut ofs = 0;
    for entry in entries {
        let need = ehs + 9;
        assert!(
            ofs + need <= page.len(),
            "encoding entry does not fit in the page"
        );
        assert_eq!(entry.ekey.len(), ehs, "encoding key width");
        page[ofs..ofs + ehs].copy_from_slice(&entry.ekey);
        ofs += ehs;
        page[ofs..ofs + 4].copy_from_slice(&entry.spec_index.to_be_bytes());
        ofs += 4;
        page[ofs..ofs + 5].copy_from_slice(&entry.size);
        ofs += 5;
    }
}
