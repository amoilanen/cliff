use anyhow::{Context, Result};
use crate::config::Model;
use crate::llm::ask_llm_with_history;
use crate::json;
use crate::actions::replace_file_lines;
use crate::executor::Action;
use reqwest::Client;

pub(crate) async fn execute(
    path: &String,
    model_config: &Model,
    execution_history: &Vec<(Action, Option<String>)>,
    client: &Client,
) -> Result<Option<String>> {
    let prompt = format!("Generate a JSON object for a ReplaceFileLines action with path: '{}'. The JSON object should have 'action' = \"replace_file_lines\", 'action_idx', 'path', 'from_line_idx', 'until_line_idx', and 'replacement_lines' fields. Generated `replacement_lines` will be used LITERALLY and will not be parsed further.", path);
    let response = ask_llm_with_history(model_config, &prompt, execution_history, client)
        .await
        .context("Failed to get response from LLM")?;
    let replace_file_lines_action: Action = serde_json::from_str(json::strip_json_fence(&response))
        .context("Failed to parse LLM response as ReplaceFileLines action")?;

    if let Action::ReplaceFileLines { path, from_line_idx, until_line_idx, replacement_lines, .. } = replace_file_lines_action {
        replace_file_lines::execute(&path, from_line_idx, until_line_idx, &replacement_lines).await
    } else {
        anyhow::bail!("LLM did not return a ReplaceFileLines action, but instead: {:?}", replace_file_lines_action);
    }
} 