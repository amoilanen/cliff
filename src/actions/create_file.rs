use anyhow::{Context, Result};
use std::fs;

pub(crate) async fn execute(path: &String, content: &String) -> Result<Option<String>> {
    if let Some(parent_dir) = std::path::Path::new(path).parent() {
        fs::create_dir_all(parent_dir)
            .with_context(|| format!("Failed to create parent directories for '{}'", path))?;
    }
    fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path))?;
    Ok(None)
}