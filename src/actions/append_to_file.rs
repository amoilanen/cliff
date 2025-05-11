use anyhow::{Context, Result};
use std::fs;
use std::io::Write;
use crate::fs::expand_home;

pub(crate) async fn execute(path: &String, content: &String) -> Result<Option<String>> {
    let expanded_path = expand_home(path)?;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&expanded_path)
        .with_context(|| format!("Failed to open file for appending: {}", expanded_path.display()))?;
    writeln!(file, "{}", content)
        .with_context(|| format!("Failed to append content to file: {}", expanded_path.display()))?;
    Ok(None)
} 