use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use colored::*;
use crate::config::Model;
use reqwest::Client;
use urlencoding::encode;
use std::future::Future;
use std::pin::Pin;
use crate::llm::{ask_llm_for_plan, ask_llm_with_history};
use crate::fs::expand_home;
use crate::json;
use crate::actions::create_file;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    //Create file on the machine of the user, `content` will be written out *literally*, no output
    CreateFile { action_idx: u32, path: String, content: String },
    //Ask Llm to reply with a CreateFile action for the file with `path`, output the result of CreateFile
    AskLlmToCreateFile {action_idx: u32, path: String},
    //Search the web using the provided `query`, output the results
    SearchWeb { action_idx: u32, query: String },
    //Read the content of the web page at the given `url`, output the result
    ReadWebPage { action_idx: u32, url: String },
    //Run command on the machine of the user, `command` is the command to execute, output the result
    RunCommand { action_idx: u32, command: String },
    //Ask the user the specified `question`, output the result
    AskUser { action_idx: u32, question: String },
    //Delete the file at the specified `path`, no output
    DeleteFile { action_idx: u32, path: String },
    // "content" will not be expanded and will be treated _literally_
    OverwriteFileContents { action_idx: u32, path: String, content: String },
    //Ask Llm to reply with a OverwriteFileContents action for the file with `path`, output the result of OverwriteFileContents
    AskLlmToOverwriteFileContents {action_idx: u32, path: String},
    // Ask LLM to output a response to the user (using the knowledge of previous actions and their outputs)
    AskLlm { action_idx: u32, prompt: String },
    // AskLlmForPlan provides the ability for the LLM to respond with a new subplan
    // 'instruction' guides the sub-plan generation.
    // 'context_sources' provides file paths or URLs for context.
    // the previously executed actions and their outputs are *always* provided to LLM in this action
    AskLlmForPlan {
        action_idx: u32,
        instruction: String,
        context_sources: Vec<String>
    },
    //Read the content of the file at the specified `path`, output the result
    ReadFile { action_idx: u32, path: String },
    //Find files matching the given `pattern`, output the result
    FindFiles { action_idx: u32, pattern: String },
    // "replacement_lines" will not be expanded and will be treated _literally_
    //Replace lines from `from_line_idx` to `until_line_idx` in the file at `path` with `replacement_lines`, output the result
    ReplaceFileLines {action_idx: u32, path: String, from_line_idx: usize, until_line_idx: usize, replacement_lines: String},
    //Ask LLM to output a ReplaceFileLines action for the file with `path`, output the result of ReplaceFileLines
    AskLlmToReplaceFileLines {action_idx: u32, path: String},
    // Append content to the file at the specified `path`, no output
    AppendToFile { action_idx: u32, path: String, content: String },
    // Move the file from `source` to `destination`, no output
    MoveFile { action_idx: u32, source: String, destination: String },
    // Copy the file from `source` to `destination`, no output
    CopyFile { action_idx: u32, source: String, destination: String },
    // List the contents of the directory at `path`, output the result
    ListDirectory { action_idx: u32, path: String },
    // Check if the path exists, output "true" or "false"
    CheckPathExists { action_idx: u32, path: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Plan {
    pub thought: Option<String>,
    pub steps: Vec<Action>,
}

async fn overwrite_file_contents(path: &String, content: &String) -> Result<Option<String>> {
    if !std::path::Path::new(path).exists() {
        if let Some(parent_dir) = std::path::Path::new(path).parent() {
            fs::create_dir_all(parent_dir)
                .with_context(|| format!("Failed to create parent directories for '{}'", path))?;
        }
    }
    fs::write(path, content)
        .with_context(|| format!("Failed to write file: {}", path))?;
    Ok(None)
}

async fn replace_file_lines(path: &String, from_line_idx: usize, until_line_idx: usize, new_contents: &String) -> Result<Option<String>> {
    let mut lines: Vec<String> = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file for replacement: {}", path))?
        .lines()
        .map(|s| s.to_string())
        .collect();

    if from_line_idx > lines.len() {
        let padding_needed = from_line_idx - lines.len();
        for _ in 0..padding_needed {
            lines.push(String::new());
        }
    }

    let range_start = from_line_idx;
    let range_end = std::cmp::min(until_line_idx + 1, lines.len());

    lines.drain(range_start..range_end);

    let new_lines: Vec<String> = new_contents.lines().map(|s| s.to_string()).collect();
    for line in new_lines.into_iter().rev() {
        lines.insert(range_start, line);
    }

    let modified_content = lines.join("\n");
    fs::write(path, modified_content)
        .with_context(|| format!("Failed to write modified file: {}", path))?;
    Ok(None)
}

impl Action {
    async fn execute(&self, execution_history: &mut Vec<(Action, Option<String>)>, model_config: &Model, client: &Client, current_auto_confirm: bool) -> Result<Option<String>> {
        match self {
            Action::CreateFile { path, content, .. } => {
                println!("Action: Create file '{}'", path);
                let output = create_file::execute(path, content).await?;
                println!("Success: File '{}' created.", path);
                Ok(output)
            },
            Action::AskLlmToCreateFile { path, .. } => {
                println!("Action: Asking LLM to generate CreateFile action for path: '{}'", path);
                let prompt = format!("Generate a JSON object for a CreateFile action with path: '{}'. The JSON object should have 'action' = \"create_file\", 'action_idx', 'path', and 'content' fields. Generated `content` will be used LITERALLY and will not be parsed further.", path);
                let response = ask_llm_with_history(model_config, &prompt, execution_history, client).await.context("Failed to get response from LLM")?;
                let action: Action = serde_json::from_str(json::strip_json_fence(&response)).context("Failed to parse LLM response as CreateFile action")?;

                if let Action::CreateFile { path, content, .. } = action {
                    let output = create_file::execute(&path, &content).await?;
                    println!("Success: File '{}' created.", path);
                    Ok(output)
                } else {
                    anyhow::bail!("LLM did not return a CreateFile action, but instead: {:?}", action);
                }
            },
            Action::AskLlmToOverwriteFileContents { path, .. } => {
                println!("Action: Asking LLM to generate OverwriteFileContents action for path: '{}'", path);
                let prompt = format!("Generate a JSON object for an OverwriteFileContents action with path: '{}'. The JSON object should have 'action' = \"overwrite_file_contents\", 'action_idx', 'path', and 'content' fields. Generated `content` will be used LITERALLY and will not be parsed further.", path);
                let response = ask_llm_with_history(model_config, &prompt, execution_history, client).await.context("Failed to get response from LLM")?;
                let action: Action = serde_json::from_str(json::strip_json_fence(&response)).context("Failed to parse LLM response as OverwriteFileContents action")?;

                if let Action::OverwriteFileContents { path, content, .. } = action {
                    let output = overwrite_file_contents(&path, &content).await?;
                    println!("Success: File '{}' overwritten.", path);
                    Ok(output)
                } else {
                    anyhow::bail!("LLM did not return an OverwriteFileContents action, but instead: {:?}", action);
                }
            },
            Action::OverwriteFileContents { path, content, .. } => {
                println!("Action: Overwrite contents file '{}'", path);
                let output = overwrite_file_contents(path, content).await?;
                println!("Success: File '{}' updated.", path);
                Ok(output)
            },
            Action::DeleteFile { path, .. } => {
                println!("Action: Delete file '{}'", path);
                if std::path::Path::new(path).exists() {
                    fs::remove_file(path)
                        .with_context(|| format!("Failed to delete file: {}", path))?;
                    println!("Success: File '{}' deleted.", path);
                } else {
                    println!("Warning: File '{}' does not exist, skipping deletion.", path);
                }
                Ok(None)
            },
            Action::RunCommand { command, .. } => {
                println!("Action: Run command `{}`", command);
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                let mut cmd = Command::new(shell);
                cmd.arg("-c");
                cmd.arg(command);

                cmd.stdin(Stdio::inherit());
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::inherit());

                let output = cmd.output() // Use output() to get status and streams
                    .with_context(|| format!("Failed to execute command: {}", command))?;

                if !output.stdout.is_empty() {
                    println!("{}", "--- Command Output ---".green());
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        println!("{}", line.green());
                    }
                    println!("{}", "----------------------".green());
                }
                 if !output.stderr.is_empty() {
                    eprintln!("{}", "--- Command Error Output ---".red());
                    for line in String::from_utf8_lossy(&output.stderr).lines() {
                        eprintln!("{}", line.red());
                    }
                    eprintln!("{}", "--------------------------".red());
                }

                if output.status.success() {
                    println!("Success: Command executed successfully.");
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    Ok(Some(output_str.trim().to_string()))
                } else {
                    anyhow::bail!("Command failed with status: {}", output.status);
                }
            },
            Action::AskLlm { prompt, .. } => {
                println!("Action: Asking LLM for response to prompt: '{}'", prompt);
                let response = crate::llm::ask_llm_with_history(model_config, prompt, &execution_history, client).await.context("Failed to get response from LLM")?;
                println!("Success: received LLM response:");
                println!("{}", response.green());
                Ok(Some(response))
            },
            Action::AskLlmForPlan { instruction, context_sources, .. } => {
                println!("Action: Asking LLM for sub-plan...");
                let sub_plan = ask_llm_for_plan(
                    model_config,
                    instruction,
                    context_sources,
                    &execution_history,
                    client,
                ).await.context("Failed to get sub-plan from LLM")?;
                println!("Success: received subplan from LLM:");
                sub_plan.display();
                println!("--- Starting Sub-Plan Execution ---");
                execute_plan(&sub_plan, model_config, client, execution_history, current_auto_confirm).await?;
                println!("--- Sub-Plan Execution Finished ---");
                Ok(None)
            },
            Action::AskLlmToReplaceFileLines { path, .. } => {
                println!("Action: Asking LLM to generate ReplaceFileLines action for path: '{}'", path);
                let prompt = format!("Generate a JSON object for a ReplaceFileLines action with path: '{}'. The JSON object should have 'action' = \"replace_file_lines\", 'action_idx', 'path', 'from_line_idx', 'until_line_idx', and 'replacement_lines' fields. Generated `replacement_lines` will be used LITERALLY and will not be parsed further.", path);
                let response = ask_llm_with_history(model_config, &prompt, execution_history, client).await.context("Failed to get response from LLM")?;
                let replace_file_lines_action: Action = serde_json::from_str(json::strip_json_fence(&response)).context("Failed to parse LLM response as ReplaceFileLines action")?;

                if let Action::ReplaceFileLines { path, from_line_idx, until_line_idx, replacement_lines, .. } = replace_file_lines_action {
                    let output = replace_file_lines(&path, from_line_idx, until_line_idx, &replacement_lines).await?;
                    println!("Success: Lines replaced in file {}.", path);
                    Ok(output)
                } else {
                    anyhow::bail!("LLM did not return a ReplaceFileLines action, but instead: {:?}", replace_file_lines_action);
                }
            },
            Action::SearchWeb { query, .. } => {
                println!("Action: Search web for '{}'", query);
                let url_encoded_query = encode(&query);
                let url = format!("https://api.duckduckgo.com/?q={}&format=json&pretty=1", url_encoded_query);
                let response = reqwest::get(&url).await?.text().await?;
                println!("Success: Web search completed. {}", response);
                Ok(Some(response))
            },
            Action::ReadWebPage { url, .. } => {
                println!("Action: Read web page at '{}'", url);
                let response = client.get(url).send().await?.text().await?;
                println!("Success: Web page read. {}", response);
                Ok(Some(response))
            },
            Action::AskUser { question, .. } => {
                println!("Action: Ask user");
                print!("{} ", question.green());
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                let response = input.trim().to_string();
                Ok(Some(response))
            },
            Action::ReadFile { path, .. } => {
                println!("Action: Read file '{}'", path);
                let content = fs::read_to_string(expand_home(path)?)
                    .with_context(|| format!("Failed to read file: {}", path))?;
                println!("Success: File '{}' read", path);
                Ok(Some(content))
            },
            Action::FindFiles { pattern, .. } => {
                println!("Action: Find files matching pattern '{}'", pattern);
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
                println!("Success: Files found matching pattern '{}'.", pattern);
                Ok(Some(result))
            },
            Action::ReplaceFileLines { path, from_line_idx, until_line_idx, replacement_lines: new_contents, .. } => {
                println!("Action: Replace lines {} to {} in file '{}'", from_line_idx, until_line_idx, path);
                let output = replace_file_lines(path, *from_line_idx, *until_line_idx, new_contents).await?;
                println!("Success: Lines {} to {} replaced in file '{}'.", from_line_idx, until_line_idx, path);
                Ok(output)
            },
            Action::AppendToFile { path, content, .. } => {
                println!("Action: Append to file '{}'", path);
                let expanded_path = expand_home(path)?;
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&expanded_path)
                    .with_context(|| format!("Failed to open file for appending: {}", expanded_path.display()))?;
                writeln!(file, "{}", content)
                    .with_context(|| format!("Failed to append content to file: {}", expanded_path.display()))?;
                println!("Success: Content appended to file '{}'.", path);
                Ok(None)
            },
            Action::MoveFile { source, destination, .. } => {
                println!("Action: Move file from '{}' to '{}'", source, destination);
                let expanded_source = expand_home(source)?;
                let expanded_destination = expand_home(destination)?;
                if let Some(parent_dir) = expanded_destination.parent() {
                    fs::create_dir_all(parent_dir)
                        .with_context(|| format!("Failed to create parent directories for destination '{}'", expanded_destination.display()))?;
                }
                fs::rename(&expanded_source, &expanded_destination)
                    .with_context(|| format!("Failed to move file from '{}' to '{}'", expanded_source.display(), expanded_destination.display()))?;
                println!("Success: File moved from '{}' to '{}'.", source, destination);
                Ok(None)
            },
            Action::CopyFile { source, destination, .. } => {
                println!("Action: Copy file from '{}' to '{}'", source, destination);
                let expanded_source = expand_home(source)?;
                let expanded_destination = expand_home(destination)?;
                 if let Some(parent_dir) = expanded_destination.parent() {
                    fs::create_dir_all(parent_dir)
                        .with_context(|| format!("Failed to create parent directories for destination '{}'", expanded_destination.display()))?;
                }
                fs::copy(&expanded_source, &expanded_destination)
                    .with_context(|| format!("Failed to copy file from '{}' to '{}'", expanded_source.display(), expanded_destination.display()))?;
                println!("Success: File copied from '{}' to '{}'.", source, destination);
                Ok(None)
            },
            Action::ListDirectory { path, .. } => {
                println!("Action: List directory '{}'", path);
                let expanded_path = expand_home(path)?;
                let mut entries = Vec::new();
                for entry in fs::read_dir(&expanded_path)
                    .with_context(|| format!("Failed to read directory: {}", expanded_path.display()))? {
                    let entry = entry.with_context(|| format!("Failed to read directory entry in {}", expanded_path.display()))?;
                    entries.push(entry.file_name().to_string_lossy().to_string());
                }
                let result = entries.join("\n");
                println!("Success: Directory '{}' listed.", path);
                Ok(Some(result))
            },
            Action::CheckPathExists { path, .. } => {
                println!("Action: Check if path exists '{}'", path);
                let expanded_path = expand_home(path)?;
                let exists = expanded_path.exists();
                println!("Success: Path '{}' exists: {}.", path, exists);
                Ok(Some(exists.to_string()))
            },
        }
    }
}

impl Plan {
    pub fn display(&self) {
        println!("\n--- Proposed Plan ---");
        if let Some(thought) = &self.thought {
            println!("Thought: {}", thought);
        }
        if self.steps.is_empty() {
            println!("No actions planned.");
            return;
        }
        for action in self.steps.iter() {
            match action {
                Action::CreateFile { action_idx, path, content } => println!("{}. Create file '{}' with content:\n{}", action_idx, path, content),
                Action::RunCommand { action_idx, command } => println!("{}. Run command: `{}`", action_idx, command),
                Action::SearchWeb { action_idx, query } => println!("{}. Search web for: '{}'", action_idx, query),
                Action::AskUser { action_idx, question } => println!("{}. Ask user: '{}'", action_idx, question),
                Action::AskLlmToReplaceFileLines { action_idx, path } => println!("{}. Ask LLM to generate ReplaceFileLines action for path: '{}'", action_idx, path),
                Action::DeleteFile { action_idx, path } => println!("{}. Delete file: '{}'", action_idx, path),
                Action::OverwriteFileContents { action_idx, path, content } => println!("{}. Edit file '{}' with content:\n{}", action_idx, path, content),
                Action::AskLlm { action_idx, prompt } => println!("{}. Ask LLM with prompt: '{}'", action_idx, prompt),
                Action::AskLlmForPlan { action_idx, instruction, context_sources } => { // Removed earlier_action_indices
                    println!(
                        "{}. Ask LLM for sub-plan:\n  Instruction: {}\n  Context Sources: {:?}",
                        action_idx, instruction, context_sources
                    );
                },
                Action::AskLlmToCreateFile { action_idx, path } => println!("{}. Ask LLM to generate CreateFile action for path: '{}'", action_idx, path),
                Action::ReadFile { action_idx, path } => println!("{}. Read file: '{}'", action_idx, path),
                Action::FindFiles { action_idx, pattern } => println!("{}. Find files matching pattern: '{}'", action_idx, pattern),
                Action::ReadWebPage { action_idx, url } => println!("{}. Read web page: '{}'", action_idx, url),
                Action::ReplaceFileLines { action_idx, path, from_line_idx, until_line_idx, replacement_lines: new_contents } => {
                    let content_snippet = if new_contents.len() > 50 {
                        format!("{}...", &new_contents[..50])
                    } else {
                        new_contents.clone()
                    };
                    println!("{}. Replace lines {} to {} in file '{}' with content: '{}'", action_idx, from_line_idx, until_line_idx, path, content_snippet);
                },
                Action::AskLlmToOverwriteFileContents { action_idx, path } => println!("{}. Ask LLM to generate OverwriteFileContents action for path: '{}'", action_idx, path),
                Action::AppendToFile { action_idx, path, content } => {
                     let content_snippet = if content.len() > 50 {
                        format!("{}...", &content[..50])
                    } else {
                        content.clone()
                    };
                    println!("{}. Append to file '{}' with content: '{}'", action_idx, path, content_snippet);
                },
                Action::MoveFile { action_idx, source, destination } => println!("{}. Move file from '{}' to '{}'", action_idx, source, destination),
                Action::CopyFile { action_idx, source, destination } => println!("{}. Copy file from '{}' to '{}'", action_idx, source, destination),
                Action::ListDirectory { action_idx, path } => println!("{}. List directory '{}'", action_idx, path),
                Action::CheckPathExists { action_idx, path } => println!("{}. Check if path exists '{}'", action_idx, path),
            }
        }
        println!("--------------------");
    }
}

pub fn execute_plan<'a>(
    plan: &'a Plan,
    model_config: &'a Model,
    client: &'a Client,
    execution_history: &'a mut Vec<(Action, Option<String>)>,
    auto_confirm: bool,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        println!("\n--- Executing Plan ---");
        if plan.steps.is_empty() {
            println!("No actions to execute.");
            return Ok(());
        }
        let mut current_auto_confirm = auto_confirm;

        for (i, action) in plan.steps.iter().enumerate() {
            println!("\n--- Step {}/{}: {:?} ---", i + 1, plan.steps.len(), action);

            let (new_auto_confirm, confirmed) = ask_for_confirmation(current_auto_confirm).await?;
            current_auto_confirm = new_auto_confirm;
            if confirmed {
                match action.execute(execution_history, &model_config, &client, current_auto_confirm).await {
                    Ok(output) => {
                        execution_history.push((action.clone(), output));
                    }
                    Err(e) => {
                        eprintln!("Action {:?} failed: {}", action, e);
                        let instruction = format!(
                            "Action {:?} failed with error: {}. The history of previous actions is provided. Generate a new plan to achieve the original objective, taking this failure into account.",
                            action, e
                        );
                        execution_history.push((action.clone(), Some(format!("ERROR: {}", e))));
                        println!("Asking LLM for a new plan due to error...");
                        // Ask LLM for a new plan
                        match ask_llm_for_plan(
                            model_config,
                            &instruction,
                            &Vec::new(), // No extra context sources for now
                            &execution_history,
                            client,
                        ).await {
                            Ok(new_plan) => {
                                println!("Received new plan from LLM.");
                                new_plan.display();
                                return execute_plan(&new_plan, model_config, client, execution_history, current_auto_confirm).await;
                            }
                            Err(llm_err) => {
                                eprintln!("Failed to get a new plan from LLM: {}", llm_err);
                                return Err(llm_err.context("Failed to get recovery plan from LLM after action failure"));
                            }
                        }
                    }
                }
            } else {
                println!("Skipping step {}.", i + 1);
            }
        }
        println!("\n--- Plan Execution Finished ---");
        Ok(())
    })
}

async fn ask_for_confirmation(current_auto_confirm: bool) -> Result<(bool, bool)> {
    let mut current_auto_confirm = current_auto_confirm;
    let mut confirmed = current_auto_confirm;
    if !current_auto_confirm {
        print!("Execute this step? (y/N/all): ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim().to_lowercase();
        if choice == "y" || choice == "yes" {
            confirmed = true;
        } else if choice == "a" || choice == "all" {
            confirmed = true;
            current_auto_confirm = true;
        }
    }
    Ok((current_auto_confirm, confirmed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;
    use tokio; // Import tokio for the test attribute

    #[test]
    fn test_plan_serialization() -> Result<()> {
        let plan = Plan {
            thought: Some("Create a hello world script and run it".to_string()),
            steps: vec![
                Action::CreateFile {
                    action_idx: 0,
                    path: "hello.sh".to_string(),
                    content: "#!/bin/bash\necho 'Hello World!'".to_string(),
                },
                Action::RunCommand {
                    action_idx: 1,
                    command: "bash hello.sh".to_string(),
                },
                Action::AskUser {
                    action_idx: 2,
                    question: "Script executed.".to_string()
                },
                Action::ReadWebPage {
                    action_idx: 1,
                    url: "https://example.com".to_string(),
                },
            ],
        };

        let serialized_plan = serde_json::to_string_pretty(&plan)?;
        let deserialized_plan: Plan = serde_json::from_str(&serialized_plan)?;
        assert_eq!(plan, deserialized_plan);
        Ok(())
    }

    fn create_temp_file(content: &str) -> Result<NamedTempFile> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "{}", content)?;
        Ok(temp_file)
    }

    fn read_file_content(path: &std::path::Path) -> Result<String> {
        fs::read_to_string(path).context("Failed to read temp file")
    }

    async fn test_replace_lines_action(
        file_content: &str,
        from_line_idx: usize,
        until_line_idx: usize,
        replacement_lines: String,
        expected_content: &str,
    ) -> Result<()> {
        let temp_file = create_temp_file(file_content)?;
        let path = temp_file.path().to_str().unwrap().to_string();

        let action = Action::ReplaceFileLines {
            action_idx: 1,
            path: path.clone(),
            from_line_idx,
            until_line_idx,
            replacement_lines,
        };

        let mut history = Vec::new();
        let model_config = Model {
            name: "default".to_string(),
            api_url: "http://localhost:8000".to_string(),
            api_key: None,
            api_key_header: None,
            model_identifier: None,
            request_format: "".to_string(),
            response_json_path: "".to_string(),
        };
        let client = Client::new();

        action.execute(&mut history, &model_config, &client, true).await?;

        let content = read_file_content(temp_file.path())?;
        assert_eq!(content.trim(), expected_content);
        Ok(())
    }

    #[tokio::test]
    async fn test_replace_lines_middle() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2\nline3\nline4\nline5",
            1,
            2,
            "new_line_a\nnew_line_b".to_string(),
            "line1\nnew_line_a\nnew_line_b\nline4\nline5",
        ).await
    }

    #[tokio::test]
    async fn test_replace_lines_start() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2\nline3",
            0,
            0,
            "replacement".to_string(),
            "replacement\nline2\nline3",
        ).await
    }

    #[tokio::test]
    async fn test_replace_lines_end() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2\nline3",
            2,
            2,
            "new_end".to_string(),
            "line1\nline2\nnew_end",
        ).await
    }

     #[tokio::test]
    async fn test_replace_lines_delete() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2\nline3\nline4",
            1,
            2,
            "".to_string(), // Empty replacement means deletion
            "line1\nline4",
        ).await
    }

    #[tokio::test]
    async fn test_replace_lines_insert_beyond_end() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2",
            4, // Start replacing from line 4 (index)
            4,
            "new_line_far_away".to_string(),
            "line1\nline2\n\n\nnew_line_far_away",
        ).await
    }

     #[tokio::test]
    async fn test_replace_lines_replace_all() -> Result<()> {
        test_replace_lines_action(
            "line1\nline2\nline3",
            0,
            2,
            "completely\nnew\ncontent".to_string(),
            "completely\nnew\ncontent",
        ).await
    }
}
