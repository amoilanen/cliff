use anyhow::{Context, Result};
use std::fs;

pub(crate) async fn execute(path: &String) -> Result<Option<String>> {
    if std::path::Path::new(path).exists() {
        fs::remove_file(path)
            .with_context(|| format!("Failed to delete file: {}", path))?;
    }
    Ok(None)
} 