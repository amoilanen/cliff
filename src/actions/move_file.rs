use anyhow::{Context, Result};
use std::fs;
use crate::fs::expand_home;

pub(crate) async fn execute(source: &String, destination: &String) -> Result<Option<String>> {
    let expanded_source = expand_home(source)?;
    let expanded_destination = expand_home(destination)?;
    if let Some(parent_dir) = expanded_destination.parent() {
        fs::create_dir_all(parent_dir)
            .with_context(|| format!("Failed to create parent directories for destination '{}'", expanded_destination.display()))?;
    }
    fs::rename(&expanded_source, &expanded_destination)
        .with_context(|| format!("Failed to move file from '{}' to '{}'", expanded_source.display(), expanded_destination.display()))?;
    Ok(None)
} 