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

#[cfg(test)]
mod tests {
    use super::{BuildConfig, CdnConfig, KeyPair};

    const BUILD_CONFIG: &str = "\
# Build Configuration

root = 00112233445566778899aabbccddeeff
install = 11111111111111111111111111111111 22222222222222222222222222222222
install-size = 1234 567
encoding = 33333333333333333333333333333333 44444444444444444444444444444444
encoding-size = 111 222
build-name = 1.2.3.4567
";

    #[test]
    fn key_pair_two_parts() {
        let pair = KeyPair::parse("c e").unwrap();
        assert_eq!(pair.content_key, "c");
        assert_eq!(pair.encoding_key, "e");
    }

    #[test]
    fn key_pair_single_part() {
        let pair = KeyPair::parse("c").unwrap();
        assert_eq!(pair.content_key, "c");
        assert_eq!(pair.encoding_key, "");
    }

    #[test]
    fn key_pair_empty() {
        let pair = KeyPair::parse("").unwrap();
        assert_eq!(pair.content_key, "");
        assert_eq!(pair.encoding_key, "");
    }

    #[test]
    fn key_pair_three_parts_is_error() {
        let error = KeyPair::parse("a b c").unwrap_err();
        assert!(format!("{error:#}").contains("invalid key pair"));
    }

    #[test]
    fn key_pair_tabs_and_extra_spaces() {
        let pair = KeyPair::parse("  a\t\tb ").unwrap();
        assert_eq!(pair.content_key, "a");
        assert_eq!(pair.encoding_key, "b");
    }

    #[test]
    fn build_config_parses_fields_ignores_unknown() {
        let config = BuildConfig::parse(BUILD_CONFIG.as_bytes()).unwrap();
        assert_eq!(config.root, "00112233445566778899aabbccddeeff");
        assert_eq!(
            config.install.content_key,
            "11111111111111111111111111111111"
        );
        assert_eq!(
            config.install.encoding_key,
            "22222222222222222222222222222222"
        );
        assert_eq!(
            config.encoding.content_key,
            "33333333333333333333333333333333"
        );
        assert_eq!(
            config.encoding.encoding_key,
            "44444444444444444444444444444444"
        );
        assert_eq!(config.build_name, "1.2.3.4567");
    }

    #[test]
    fn build_config_missing_keys_default_empty() {
        // B14: a missing encoding pair parses as empty strings. The failure
        // shows up later, in partition_hash, as an invalid hash.
        let config = BuildConfig::parse(b"build-name = x").unwrap();
        assert_eq!(config.build_name, "x");
        assert_eq!(config.encoding.content_key, "");
        assert_eq!(config.encoding.encoding_key, "");
        assert_eq!(config.install.content_key, "");
        assert_eq!(config.root, "");
    }

    #[test]
    fn build_config_invalid_utf8_is_error() {
        assert!(BuildConfig::parse(&[0xFF, 0xFE]).is_err());
    }

    #[test]
    fn build_config_duplicate_encoding_line_is_error() {
        let text = "encoding = c e\nencoding = c e\n";
        let error = BuildConfig::parse(text.as_bytes()).unwrap_err();
        assert!(format!("{error:#}").contains("invalid key pair"));
    }

    #[test]
    fn cdn_config_parses_archives() {
        let text = "archives = aa bb cc\narchive-group = gg\npatch-archives = pp\n";
        let config = CdnConfig::parse(text.as_bytes()).unwrap();
        assert_eq!(config.archives, ["aa", "bb", "cc"]);
        assert_eq!(config.archive_group, "gg");
    }

    #[test]
    fn cdn_config_missing_archives_is_empty() {
        let config = CdnConfig::parse(b"archive-group = gg").unwrap();
        assert!(config.archives.is_empty());
        assert_eq!(config.archive_group, "gg");
    }

    #[test]
    fn cdn_config_repeated_archives_lines_concatenate() {
        let text = "archives = aa\narchives = bb\n";
        let config = CdnConfig::parse(text.as_bytes()).unwrap();
        assert_eq!(config.archives, ["aa", "bb"]);
    }
}
