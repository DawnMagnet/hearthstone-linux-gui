use std::collections::HashMap;

pub fn parse(input: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for line in input.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        values
            .entry(key.trim().to_string())
            .and_modify(|existing: &mut String| {
                existing.push('\n');
                existing.push_str(value.trim());
            })
            .or_insert_with(|| value.trim().to_string());
    }
    values
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_key_values() {
        let values = parse("root = aa\nencoding = c1 e1\n");
        assert_eq!(values.get("root").map(String::as_str), Some("aa"));
        assert_eq!(values.get("encoding").map(String::as_str), Some("c1 e1"));
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let values = parse("# Build Configuration\n\n   \nroot = x");
        assert_eq!(values.len(), 1);
        assert_eq!(values.get("root").map(String::as_str), Some("x"));
    }

    #[test]
    fn trims_whitespace_and_crlf() {
        let values = parse("  root  =  x  \r\n");
        assert_eq!(values.get("root").map(String::as_str), Some("x"));
    }

    #[test]
    fn splits_on_first_equals_only() {
        let values = parse("k = a=b");
        assert_eq!(values.get("k").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn line_without_equals_has_empty_value() {
        let values = parse("flag");
        assert_eq!(values.get("flag").map(String::as_str), Some(""));
    }

    #[test]
    fn duplicate_keys_join_with_newline() {
        let values = parse("a = 1\na = 2");
        assert_eq!(values.get("a").map(String::as_str), Some("1\n2"));
    }

    #[test]
    fn empty_input_is_empty_map() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn hash_inside_value_is_kept() {
        let values = parse("k = a # b");
        assert_eq!(values.get("k").map(String::as_str), Some("a # b"));
    }

    #[test]
    fn utf8_bom_on_first_line() {
        // B15: the BOM is not stripped, so the comment line is stored as a key.
        let values = parse("\u{feff}# Build Configuration\nroot = x");
        assert_eq!(values.get("root").map(String::as_str), Some("x"));
        assert!(values.keys().any(|key| key.starts_with('\u{feff}')));
    }
}
