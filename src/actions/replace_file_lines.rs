use anyhow::{Context, Result};
use std::fs;

pub(crate) async fn execute(path: &String, from_line_idx: usize, until_line_idx: usize, new_contents: &String) -> Result<Option<String>> {
    let mut lines: Vec<String> = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file for replacement: {}", path))?
        .lines()
        .map(|s| s.to_string())
        .collect();

    if from_line_idx > lines.len() {
        let padding_needed = from_line_idx - lines.len();
        for _ in 0..padding_needed {
            lines.push(String::new());
        }
    }

    let range_start = from_line_idx;
    let range_end = std::cmp::min(until_line_idx + 1, lines.len());

    lines.drain(range_start..range_end);

    let new_lines: Vec<String> = new_contents.lines().map(|s| s.to_string()).collect();
    for line in new_lines.into_iter().rev() {
        lines.insert(range_start, line);
    }

    let modified_content = lines.join("\n");
    fs::write(path, modified_content)
        .with_context(|| format!("Failed to write modified file: {}", path))?;
    Ok(None)
} 