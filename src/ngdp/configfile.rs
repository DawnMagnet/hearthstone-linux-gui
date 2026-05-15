use super::blizini;
use anyhow::Result;

#[derive(Clone, Debug, Default)]
pub struct KeyPair {
    pub content_key: String,
    pub encoding_key: String,
}

impl KeyPair {
    pub fn parse(value: &str) -> Result<Self> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        anyhow::ensure!(parts.len() <= 2, "invalid key pair `{value}`");
        Ok(Self {
            content_key: parts.first().copied().unwrap_or_default().to_string(),
            encoding_key: parts.get(1).copied().unwrap_or_default().to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct BuildConfig {
    pub root: String,
    pub install: KeyPair,
    pub encoding: KeyPair,
    pub build_name: String,
}

impl BuildConfig {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let values = blizini::parse(std::str::from_utf8(data)?);
        Ok(Self {
            root: values.get("root").cloned().unwrap_or_default(),
            install: KeyPair::parse(values.get("install").map(String::as_str).unwrap_or(""))?,
            encoding: KeyPair::parse(values.get("encoding").map(String::as_str).unwrap_or(""))?,
            build_name: values.get("build-name").cloned().unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct CdnConfig {
    pub archive_group: String,
    pub archives: Vec<String>,
}

impl CdnConfig {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let values = blizini::parse(std::str::from_utf8(data)?);
        Ok(Self {
            archive_group: values.get("archive-group").cloned().unwrap_or_default(),
            archives: values
                .get("archives")
                .map(|value| value.split_whitespace().map(ToOwned::to_owned).collect())
                .unwrap_or_default(),
        })
    }
}
