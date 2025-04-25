use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    CreateFile { action_idx: u32, path: String, content: String },
    RunCommand { action_idx: u32, command: String },
    SearchWeb { action_idx: u32, query: String },
    AskUser { action_idx: u32, question: String },
    DeleteFile { action_idx: u32, path: String },
    EditFile { action_idx: u32, path: String, content: String },
    AskLlmForPlan { action_idx: u32, prev_commands_to_provide_llm_with_outputs_of: Vec<u32>},
    Respond { action_idx: u32, message: String },
    /*
    //TODO: Implement also the following commands
    ReadFile { path: String },
    FindFiles { pattern: String },
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
    async fn execute(&self) -> Result<()> {
        match self {
            Action::CreateFile { action_idx, path, content } => {
                println!("Action: Create file '{}'", path);
                if let Some(parent_dir) = std::path::Path::new(path).parent() {
                    fs::create_dir_all(parent_dir)
                        .with_context(|| format!("Failed to create parent directories for '{}'", path))?;
                }
                fs::write(path, content)
                    .with_context(|| format!("Failed to write file: {}", path))?;
                println!("Success: File '{}' created.", path);
            },
            Action::EditFile { action_idx, path, content } => {
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
            },
            Action::DeleteFile { action_idx, path } => {
                println!("Action: Delete file '{}'", path);
                if std::path::Path::new(path).exists() {
                    fs::remove_file(path)
                        .with_context(|| format!("Failed to delete file: {}", path))?;
                    println!("Success: File '{}' deleted.", path);
                } else {
                    println!("Warning: File '{}' does not exist, skipping deletion.", path);
                }
            },
            Action::RunCommand { action_idx, command } => {
                println!("Action: Run command `{}`", command);
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                let mut cmd = Command::new(shell);
                cmd.arg("-c");
                cmd.arg(command);

                cmd.stdin(Stdio::inherit());
                cmd.stdout(Stdio::inherit());
                cmd.stderr(Stdio::inherit());

                let status = cmd.status()
                    .with_context(|| format!("Failed to execute command: {}", command))?;

                if status.success() {
                    println!("Success: Command executed successfully.");
                } else {
                    anyhow::bail!("Command failed with status: {}", status);
                }
            },
            Action::AskLlmForPlan { action_idx, prev_commands_to_provide_llm_with_outputs_of } => {
                //TODO:
            },
            Action::SearchWeb { action_idx,query } => {
                //TODO:
                println!("Action: Search web for '{}'", query);
                println!("  (Action: Search web not yet implemented)");
                println!("  Skipping: Search web functionality is not available.");
            },
            Action::AskUser { action_idx,question } => {
                //TODO:
                println!("  Action: Ask user '{}'", question);
                println!("  (Asking user not yet implemented)");
                println!("  Skipping: Asking user functionality is not available.");
            },
            Action::Respond { action_idx,message } => {
                //TODO:
                println!("--- Final Response ---");
                println!("{}", message);
                println!("----------------------");
            }
        }
        Ok(())
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
                Action::Respond { action_idx, message } => println!("{}. Respond: '{}'", action_idx, message),
                Action::AskLlmForPlan { action_idx, prev_commands_to_provide_llm_with_outputs_of } => println!("{}. Ask LLM for plan depending on output of commands '{:?}'", action_idx, prev_commands_to_provide_llm_with_outputs_of),
            }
        }
        println!("--------------------");
    }
}

pub async fn execute_plan(plan: &Plan, auto_confirm: bool) -> Result<()> {
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
            action.execute().await?;
        } else {
            println!("Skipping step {}.", i + 1);
        }
    }
    println!("\n--- Plan Execution Finished ---");
    Ok(())
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
                Action::Respond {
                    action_idx: 2,
                    message: "Script executed.".to_string()
                },
            ],
        };

        let serialized_plan = serde_json::to_string_pretty(&plan)?;
        let deserialized_plan: Plan = serde_json::from_str(&serialized_plan)?;
        assert_eq!(plan, deserialized_plan);
        Ok(())
    }
}
