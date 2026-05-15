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
