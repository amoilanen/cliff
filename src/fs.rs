use std::env;
use std::path::PathBuf;
use anyhow::Result;

pub(crate) fn expand_home(path: &str) -> Result<PathBuf> {
    let expanded_path = if path.starts_with("~/") {
        let home = env::var("HOME")?;
        PathBuf::from(home).join(&path[2..])
    } else {
        PathBuf::from(path)
    };
    Ok(expanded_path)
}
