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
