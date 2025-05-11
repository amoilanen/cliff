use anyhow::{Context, Result};
use crate::config::Model;
use crate::llm::ask_llm_with_history;
use crate::json;
use crate::actions::create_file;
use crate::executor::Action;
use reqwest::Client;

pub(crate) async fn execute(
    path: &String,
    model_config: &Model,
    execution_history: &Vec<(Action, Option<String>)>,
    client: &Client,
) -> Result<Option<String>> {
    let prompt = format!("Generate a JSON object for a CreateFile action with path: '{}'. The JSON object should have 'action' = \"create_file\", 'action_idx', 'path', and 'content' fields. Generated `content` will be used LITERALLY and will not be parsed further.", path);
    let response = ask_llm_with_history(model_config, &prompt, execution_history, client)
        .await
        .context("Failed to get response from LLM")?;
    let action: Action = serde_json::from_str(json::strip_json_fence(&response))
        .context("Failed to parse LLM response as CreateFile action")?;

    if let Action::CreateFile { path, content, .. } = action {
        create_file::execute(&path, &content).await
    } else {
        anyhow::bail!("LLM did not return a CreateFile action, but instead: {:?}", action);
    }
} 