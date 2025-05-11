use anyhow::{Context, Result};
use crate::config::Model;
use crate::llm::ask_llm_with_history;
use crate::json;
use crate::actions::overwrite_file;
use crate::executor::Action;
use reqwest::Client;

pub(crate) async fn execute(
    path: &String,
    model_config: &Model,
    execution_history: &Vec<(Action, Option<String>)>,
    client: &Client,
) -> Result<Option<String>> {
    let prompt = format!("Generate a JSON object for an OverwriteFileContents action with path: '{}'. The JSON object should have 'action' = \"overwrite_file_contents\", 'action_idx', 'path', and 'content' fields. Generated `content` will be used LITERALLY and will not be parsed further.", path);
    let response = ask_llm_with_history(model_config, &prompt, execution_history, client)
        .await
        .context("Failed to get response from LLM")?;
    let action: Action = serde_json::from_str(json::strip_json_fence(&response))
        .context("Failed to parse LLM response as OverwriteFileContents action")?;

    if let Action::OverwriteFileContents { path, content, .. } = action {
        overwrite_file::execute(&path, &content).await
    } else {
        anyhow::bail!("LLM did not return an OverwriteFileContents action, but instead: {:?}", action);
    }
} 