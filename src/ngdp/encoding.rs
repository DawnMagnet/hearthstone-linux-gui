use super::verify_md5;
use anyhow::Result;
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
        let content_page_table_page_size =
            u16::from_be_bytes(data[5..7].try_into().unwrap()) as usize;
        let encoding_page_table_page_size =
            u16::from_be_bytes(data[7..9].try_into().unwrap()) as usize;
        let content_page_table_page_count =
            u32::from_be_bytes(data[9..13].try_into().unwrap()) as usize;
        let encoding_page_table_page_count =
            u32::from_be_bytes(data[13..17].try_into().unwrap()) as usize;
        let encoding_spec_block_size =
            u32::from_be_bytes(data[18..22].try_into().unwrap()) as usize;

        let mut offset = 22;
        let spec_data = &data[offset..offset + encoding_spec_block_size];
        let specs: Vec<String> = spec_data
            .split(|byte| *byte == 0)
            .filter(|chunk| !chunk.is_empty())
            .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
            .collect();
        offset += encoding_spec_block_size;

        let content_index_size = content_page_table_page_count * (content_hash_size * 2);
        offset += content_index_size;
        let content_table_size =
            content_page_table_page_count * 1024 * content_page_table_page_size;
        let content_table = &data[offset..offset + content_table_size];
        offset += content_table_size;

        let encoding_index_size = encoding_page_table_page_count * (encoding_hash_size * 2);
        offset += encoding_index_size;
        let encoding_table_size =
            encoding_page_table_page_count * 1024 * encoding_page_table_page_size;
        let encoding_table = &data[offset..offset + encoding_table_size];

        let mut content_to_encoding = HashMap::new();
        let page_size = 1024 * content_page_table_page_size;
        for page in content_table
            .chunks(page_size)
            .take(content_page_table_page_count)
        {
            let mut ofs = 0;
            while ofs + 6 + content_hash_size + encoding_hash_size <= page.len() {
                let key_count = page[ofs] as usize;
                let file_size_hi = page[ofs + 1] as u64;
                let file_size =
                    u32::from_be_bytes(page[ofs + 2..ofs + 6].try_into().unwrap()) as u64;
                let _full_size = file_size | (file_size_hi << 32);
                ofs += 6;
                if key_count == 0 {
                    break;
                }
                let content_key = hex::encode(&page[ofs..ofs + content_hash_size]);
                ofs += content_hash_size;
                if key_count > 0 {
                    let encoding_key = hex::encode(&page[ofs..ofs + encoding_hash_size]);
                    content_to_encoding.insert(content_key, encoding_key);
                }
                ofs += key_count * encoding_hash_size;
            }
        }

        let mut encoding_specs = HashMap::new();
        let page_size = 1024 * encoding_page_table_page_size;
        for page in encoding_table
            .chunks(page_size)
            .take(encoding_page_table_page_count)
        {
            let mut ofs = 0;
            while ofs + encoding_hash_size + 9 < page.len() {
                let spec_index = i32::from_be_bytes(
                    page[ofs + encoding_hash_size..ofs + encoding_hash_size + 4]
                        .try_into()
                        .unwrap(),
                );
                if spec_index == -1 {
                    break;
                }
                let encoding_key = hex::encode(&page[ofs..ofs + encoding_hash_size]);
                let spec = specs.get(spec_index as usize).cloned().unwrap_or_default();
                encoding_specs.insert(encoding_key, spec);
                ofs += encoding_hash_size + 9;
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
