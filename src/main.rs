use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use crate::config::{Config, Model};
use crate::llm::{ask_llm, ask_llm_for_plan, start_llm_ask_session};
use colored::*;
use reqwest::Client;

mod actions;
mod config;
mod executor;
mod llm;
mod fs;
mod json;

#[derive(Parser, Debug)]
#[command(author, version, about = "CLIFF: Command Line Interface Friendly & Facilitator", long_about = "CLIFF: Command Line Interface Friendly & Facilitator")]
struct Cli {
    /// Command to execute
    #[command(subcommand)]
    command: Commands,
    /// Configured LLM model to use to execute the command
    #[arg(short, long, global = true)]
    model: Option<String>,
    /// Files or URLs to provide as context
    #[arg(short, long, value_delimiter = ',')]
    context: Vec<String>
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start interactive session
    #[command(name = "session")]
    Session,
    /// Ask a question to the configured LLM
    Ask {
        /// The prompt/question to ask the LLM
        prompt: String
    },
    /// Ask the LLM to generate a plan and execute it
    Act {
        /// The instruction or goal for the LLM
        instruction: String,
        /// Automatically confirm and execute all actions in the plan
        #[arg(long, default_value = "false")]
        auto_confirm: bool
    },
    /// Manage LLM configurations
    Config(ConfigArgs),
}

#[derive(Parser, Debug)]
struct ConfigArgs {
    /// Configuration sub-command
    #[command(subcommand)]
    action: ConfigAction,
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Add a new model configuration
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(long)]
        model_identifier: Option<String>,
        #[arg(long)]
        api_url: String,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        api_key_header: Option<String>,
        #[arg(long)]
        request_format: String,
        #[arg(long)]
        response_json_path: String,
    },
    /// Set the default model
    SetDefault {
        /// Name of the model to set as default
        name: String,
    },
    /// Set the current model for this session (temporary override)
    SetCurrent {
         /// Name of the model to set as current
        name: String,
    },
    /// Clear the current model selection, falling back to default
    ClearCurrent,
    /// List all configured models
    List,
    /// Delete a configured model
    Delete {
        /// Name of the model to delete
        name: String,
    },
    /// Show the current configuration path
    Path,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut config = Config::load()?;
    let client = Client::new();

    if let Some(model_name) = &cli.model {
        if config.set_current_model(model_name).is_err() {
            eprintln!("Warning: Model '{}' not found, using default/active model.", model_name);
        }
    }
    let context = &cli.context;
    let active_model = get_active_model(&config)?;

    match cli.command {
        Commands::Ask { prompt } => {
            let answer = ask_llm(active_model, &prompt, &context, &client).await.context("Error during LLM call")?;
            println!("{}\n", answer.green());
        }
        Commands::Act { instruction, auto_confirm } => {
            let plan = ask_llm_for_plan(active_model, &instruction, &context, &Vec::new(), &client).await.context("Error during LLM call")?;
            plan.display();
            executor::execute_plan(&plan, active_model, &client, &mut Vec::new(), auto_confirm).await?;
        }
        Commands::Session => {
            start_llm_ask_session(active_model, context, &client).await?
        }
        Commands::Config(args) => {
            handle_config_action(args.action, &mut config)?;
        }
    }

    Ok(())
}

fn get_active_model(config: &Config) -> Result<&Model> {
    config.get_active_model().ok_or(anyhow!("Error: No active model configured. Use 'cognitor config add' and 'cognitor config set-default'."))
}

fn handle_config_action(action: ConfigAction, config: &mut Config) -> Result<()> {
    match action {
        ConfigAction::Add { name, api_url, api_key, api_key_header, model_identifier, request_format, response_json_path } => {
            let new_model = Model {
                name: name.clone(),
                api_url,
                api_key,
                api_key_header,
                model_identifier,
                request_format,
                response_json_path,
            };
            config.add_model(new_model);
            config.save()?;
            println!("Model '{}' added.", name);
        }
        ConfigAction::SetDefault { name } => {
            config.set_default_model(&name)?;
            config.save()?;
            println!("Default model set to '{}'.", name);
        }
        ConfigAction::SetCurrent { name } => {
            config.set_current_model(&name)?;
            println!("Current model for this session set to '{}'.", name);
        }
        ConfigAction::ClearCurrent => {
            config.clear_current_model();
            println!("Current model selection cleared. Using default model.");
        }
        ConfigAction::List => {
            println!("Configured Models:");
            if config.models.is_empty() {
                println!("No models configured.");
            } else {
                for (name, model) in &config.models {
                    let is_default = config.default_model.as_ref() == Some(name);
                    let is_current = config.current_model.as_ref() == Some(name);
                    let default_marker = if is_default { " (default)" } else { "" };
                    let current_marker = if is_current && Some(name) != config.default_model.as_ref() { " (current)" } else { "" };

                    println!(
                        "  - {}{}{}: URL={}, Key={}, Identifier={}",
                        name,
                        default_marker,
                        current_marker,
                        model.api_url,
                        model.api_key.as_deref().map_or("Not Set", |_|"Set"),
                        model.model_identifier.as_deref().unwrap_or("Not Set")
                    );
                }
            }
            println!(
                "\nActive model for next command (unless overridden): {}",
                config.get_active_model().map_or("None", |m| &m.name)
            );
        }
        ConfigAction::Delete { name } => {
            config.delete_model(&name)?;
            config.save()?;
            println!("Model '{}' deleted.", name);
        }
        ConfigAction::Path => {
            let path = Config::config_path().context("Error determining config path")?;
            println!("Config file path: {:?}", path)
        }
    }
    Ok(())
}
