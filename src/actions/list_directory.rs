use anyhow::{Context, Result};
use std::fs;
use crate::fs::expand_home;

pub(crate) async fn execute(path: &String) -> Result<Option<String>> {
    let expanded_path = expand_home(path)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&expanded_path)
        .with_context(|| format!("Failed to read directory: {}", expanded_path.display()))? {
        let entry = entry.with_context(|| format!("Failed to read directory entry in {}", expanded_path.display()))?;
        entries.push(entry.file_name().to_string_lossy().to_string());
    }
    let result = entries.join("\n");
    Ok(Some(result))
} 