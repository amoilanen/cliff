use anyhow::Result;
use crate::fs::expand_home;

pub(crate) async fn execute(path: &String) -> Result<Option<String>> {
    let expanded_path = expand_home(path)?;
    let exists = expanded_path.exists();
    Ok(Some(exists.to_string()))
} 