use anyhow::{Context, Result};

pub(crate) async fn execute(pattern: &String) -> Result<Option<String>> {
    let mut paths: Vec<String> = Vec::new();
    for entry in glob::glob(pattern).with_context(|| format!("Failed to glob with pattern: {}", pattern))? {
        match entry {
            Ok(path) => {
                paths.push(path.display().to_string());
            }
            Err(e) => println!("glob error: {:?}", e),
        }
    }
    let result = paths.join("\n");
    Ok(Some(result))
} 