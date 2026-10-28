use anyhow::{Context, Result};
use std::collections::HashMap;

#[derive(Debug)]
pub struct PsvFile {
    pub header: Vec<String>,
    pub rows: Vec<HashMap<String, String>>,
}

impl PsvFile {
    pub fn parse(input: &str) -> Result<Self> {
        let mut lines = input
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty());
        let raw_header = lines.next().context("PSV file has no header")?;
        let header: Vec<String> = raw_header
            .split('|')
            .map(|field| field.split('!').next().unwrap_or(field).to_string())
            .collect();

        let mut rows = Vec::new();
        for line in lines {
            let values: Vec<&str> = line.split('|').collect();
            let mut row = HashMap::new();
            for (idx, key) in header.iter().enumerate() {
                row.insert(
                    key.clone(),
                    values.get(idx).copied().unwrap_or_default().to_string(),
                );
            }
            rows.push(row);
        }

        Ok(Self { header, rows })
    }
}

#[cfg(test)]
mod tests {
    use super::PsvFile;

    const VERSIONS: &str = "\
Region!STRING:0|BuildConfig!HEX:16|CDNConfig!HEX:16|BuildId!DEC:4|VersionsName!String:0
## seqn = 12345
us|aaaa|bbbb|123|1.2.3
eu|cccc|dddd|124|1.2.4
";

    #[test]
    fn parses_header_strips_type_suffix() {
        let psv = PsvFile::parse(VERSIONS).unwrap();
        assert_eq!(
            psv.header,
            [
                "Region",
                "BuildConfig",
                "CDNConfig",
                "BuildId",
                "VersionsName"
            ]
        );
        assert_eq!(psv.rows.len(), 2);
        assert_eq!(psv.rows[1].get("Region").map(String::as_str), Some("eu"));
        assert_eq!(psv.rows[0].get("BuildId").map(String::as_str), Some("123"));
    }

    #[test]
    fn skips_seqn_and_blank_lines() {
        let text = "\
Region!STRING:0|BuildId!DEC:4

## seqn = 12345

us|123

eu|124
";
        let psv = PsvFile::parse(text).unwrap();
        assert_eq!(psv.rows.len(), 2);
        assert_eq!(psv.rows[1].get("BuildId").map(String::as_str), Some("124"));
    }

    #[test]
    fn no_header_is_error() {
        for input in ["", "## seqn = 1\n\n"] {
            let error = PsvFile::parse(input).unwrap_err();
            assert!(
                format!("{error:#}").contains("no header"),
                "input {input:?}"
            );
        }
    }

    #[test]
    fn short_row_fills_empty_strings() {
        let psv = PsvFile::parse("A|B|C\n1|2").unwrap();
        assert_eq!(psv.rows[0].get("C").map(String::as_str), Some(""));
    }

    #[test]
    fn long_row_ignores_extra_columns() {
        let psv = PsvFile::parse("A|B\n1|2|3").unwrap();
        assert_eq!(psv.header, ["A", "B"]);
        assert_eq!(psv.rows[0].get("A").map(String::as_str), Some("1"));
        assert_eq!(psv.rows[0].get("B").map(String::as_str), Some("2"));
        assert!(!psv.rows[0].values().any(|value| value == "3"));
    }

    #[test]
    fn crlf_line_endings() {
        let psv = PsvFile::parse("A!STRING:0|B\r\n1|2\r\n").unwrap();
        assert_eq!(psv.rows[0].get("B").map(String::as_str), Some("2"));
        assert!(!psv.rows[0]["B"].contains('\r'));
    }

    #[test]
    fn header_without_type_suffix() {
        let psv = PsvFile::parse("A|B\n1|2").unwrap();
        assert_eq!(psv.header, ["A", "B"]);
    }

    #[test]
    fn empty_field_values() {
        let psv = PsvFile::parse("A|B\n|x").unwrap();
        assert_eq!(psv.rows[0].get("A").map(String::as_str), Some(""));
        assert_eq!(psv.rows[0].get("B").map(String::as_str), Some("x"));
    }

    #[test]
    fn indented_comment_is_not_filtered() {
        // B15: the comment check does not trim, so an indented "##" line
        // becomes the header.
        let psv = PsvFile::parse("  ## seqn = 1\nA|B\n1|2").unwrap();
        assert_eq!(psv.header, ["  ## seqn = 1"]);
        assert_eq!(psv.rows.len(), 2);
    }
}
