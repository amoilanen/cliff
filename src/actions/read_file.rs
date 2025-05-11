use anyhow::{Context, Result};
use std::fs;
use crate::fs::expand_home;

pub(crate) async fn execute(path: &String) -> Result<Option<String>> {
    let content = fs::read_to_string(expand_home(path)?)
        .with_context(|| format!("Failed to read file: {}", path))?;
    Ok(Some(content))
}