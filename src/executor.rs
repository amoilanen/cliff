use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use crate::config::Model;
use reqwest::Client;
use crate::llm::ask_llm_for_plan;
use std::future::Future;
use std::pin::Pin;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    CreateFile { action_idx: u32, path: String, content: String },
    SearchWeb { action_idx: u32, query: String },
    RunCommand { action_idx: u32, command: String },
    AskUser { action_idx: u32, question: String },
    DeleteFile { action_idx: u32, path: String },
    EditFile { action_idx: u32, path: String, content: String },
    // Ask LLM to output a response to the user (using the knowledge of previous actions and their outputs)
    AskLlm { action_idx: u32, prompt: String },
    // AskLlmForPlan provides the ability for the LLM to respond with a new subplan
    // based on the results of the execution of the previous actions.
    // 'instruction' guides the sub-plan generation.
    // 'context_sources' provides file paths or URLs for context.
    AskLlmForPlan {
        action_idx: u32,
        instruction: String,
        context_sources: Vec<String>
    },
    ReadFile { action_idx: u32, path: String },
    FindFiles { action_idx: u32, pattern: String },
    /*
    //TODO: Implement also the following commands
    ReadWebPage { url: String },
    AppendToFile { path: String, content: String },
    MoveFile { source: String, destination: String },
    CopyFile { source: String, destination: String },
    ListDirectory { path: String },
    CheckPathExists { path: String },
    */
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Plan {
    pub thought: Option<String>,
    pub steps: Vec<Action>,
}

impl Action {
    async fn execute(&self, execution_history: &mut Vec<(Action, Option<String>)>, model_config: &Model, client: &Client, current_auto_confirm: bool) -> Result<Option<String>> {
        match self {
            Action::CreateFile { path, content, .. } => {
                println!("Action: Create file '{}'", path);
                if let Some(parent_dir) = std::path::Path::new(path).parent() {
                    fs::create_dir_all(parent_dir)
                        .with_context(|| format!("Failed to create parent directories for '{}'", path))?;
                }
                fs::write(path, content)
                    .with_context(|| format!("Failed to write file: {}", path))?;
                println!("Success: File '{}' created.", path);
                Ok(None)
            },
            Action::EditFile { path, content, .. } => {
                println!("Action: Edit/Overwrite file '{}'", path);
                if !std::path::Path::new(path).exists() {
                    println!("Warning: File '{}' does not exist, creating it.", path);
                    if let Some(parent_dir) = std::path::Path::new(path).parent() {
                        fs::create_dir_all(parent_dir)
                            .with_context(|| format!("Failed to create parent directories for '{}'", path))?;
                    }
                }
                fs::write(path, content)
                    .with_context(|| format!("Failed to write file: {}", path))?;
                println!("Success: File '{}' updated.", path);
                Ok(None)
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
                    println!("--- Command Output ---");
                    io::stdout().write_all(&output.stdout)?;
                    println!("----------------------");
                }
                 if !output.stderr.is_empty() {
                    eprintln!("--- Command Error Output ---");
                    io::stderr().write_all(&output.stderr)?;
                    eprintln!("--------------------------");
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
                println!("LLM response: '{}'", response);
                Ok(Some(response))
            },
            Action::AskLlmForPlan { instruction, context_sources, .. } => {
                // This action is handled directly in execute_plan for recursion.
                // Execution logic (calling LLM, recursive call) happens there.
                // This function shouldn't be called directly for AskLlmForPlan.
                // However, to satisfy the match, we print a message.
                println!("Action: Asking LLM for sub-plan...");
                let sub_plan = ask_llm_for_plan(
                    model_config,
                    instruction,
                    context_sources,
                    &execution_history,
                    client,
                ).await.context("Failed to get sub-plan from LLM")?;
                sub_plan.display();
                println!("--- Starting Sub-Plan Execution ---");
                execute_plan(&sub_plan, model_config, client, execution_history, current_auto_confirm).await?; // .await the pinned future
                println!("--- Sub-Plan Execution Finished ---");
                Ok(None)
            },
            Action::SearchWeb { query, .. } => {
                //TODO: Implement and potentially return search results
                println!("Action: Search web for '{}'", query);
                println!("  (Action: Search web not yet implemented)");
                println!("  Skipping: Search web functionality is not available.");
                Ok(None)
            },
            Action::AskUser { question, .. } => {
                println!("Action: Ask user '{}'", question);
                print!("{} ", question);
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                let response = input.trim().to_string();
                Ok(Some(response))
            },
            Action::ReadFile { path, .. } => {
                println!("Action: Read file '{}'", path);
                let content = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read file: {}", path))?;
                println!("Success: File '{}' read.", path);
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
            }
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
                Action::DeleteFile { action_idx, path } => println!("{}. Delete file: '{}'", action_idx, path),
                Action::EditFile { action_idx, path, content } => println!("{}. Edit file '{}' with content:\n{}", action_idx, path, content),
                Action::AskLlm { action_idx, prompt } => println!("{}. Ask LLM with prompt: '{}'", action_idx, prompt),
                Action::AskLlmForPlan { action_idx, instruction, context_sources } => { // Removed earlier_action_indices
                    println!(
                        "{}. Ask LLM for sub-plan:\n  Instruction: {}\n  Context Sources: {:?}",
                        action_idx, instruction, context_sources
                    );
                },
                Action::ReadFile { action_idx, path } => println!("{}. Read file: '{}'", action_idx, path),
                Action::FindFiles { action_idx, pattern } => println!("{}. Find files matching pattern: '{}'", action_idx, pattern),
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
                let output = action.execute(execution_history, &model_config, &client, current_auto_confirm).await?;
                execution_history.push((action.clone(), output));
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
            ],
        };

        let serialized_plan = serde_json::to_string_pretty(&plan)?;
        let deserialized_plan: Plan = serde_json::from_str(&serialized_plan)?;
        assert_eq!(plan, deserialized_plan);
        Ok(())
    }
}
