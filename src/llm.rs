use crate::config::Model;
use anyhow::{Context, Result};
use reqwest::Client;
use crate::executor::{Action, Plan};
use serde_json::{self, Value};
use std::fs;
use url::Url;
use jsonpath_lib::select as jsonpath_select;
use std::io::{self, Write};
use crate::json;

#[derive(Debug, PartialEq)]
struct ContextContent {
    source: String,
    content: String,
}

pub async fn start_llm_ask_session(
    model_config: &Model,
    context_sources: &[String],
    client: &Client
) -> Result<()> {
    println!("Ask your questions (or type 'exit' to end):");
    io::stdout().flush()?;
    let mut conversation_history: Vec<String> = Vec::new();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut user_question = String::new();
        io::stdin().read_line(&mut user_question)?;
        let user_question = user_question.trim();

        if user_question.to_lowercase() == "exit" {
            println!("Ending session.");
            break;
        }

        let prompt_with_history = format!(
            "{}\\nConversation History:\\n{}",
            user_question,
            conversation_history.join("\\n")
        );

        let answer = ask_llm(model_config, &prompt_with_history, context_sources, client)
            .await
            .context("Error during LLM call")?;

        println!("{}\\n", answer);

        conversation_history.push(format!("User: {}\\nLLM: {}", user_question, answer));
    }
    Ok(())
}

pub async fn ask_llm_with_history(
    model_config: &Model,
    question: &str,
    execution_history: &[(Action, Option<String>)],
    client: &Client
) -> Result<String> {
    let executed_actions: Vec<String> = execution_history
        .iter()
        .map(|(action, output)| {
            let action_string = format!("{:?}", action);
            if let Some(output) = output {
                format!("action: {}, output: {}", action_string, output)
            } else {
                action_string
            }
        })
        .collect();
    let executed_actions_context = executed_actions.join("\\n");

    let prompt_with_executed_actions_context = format!("
        Question: {}

        Previous executed actions (action and its output): {}
    ", question, executed_actions_context);
    fetch_llm_response(&prompt_with_executed_actions_context, model_config, client).await
}

pub async fn ask_llm(
    model_config: &Model,
    prompt: &str,
    context_sources: &[String],
    client: &Client
) -> Result<String> {
    let combined_context = get_combined_context(context_sources, client).await?;
    let prompt_with_context = format!("
    Question: {}

    Context: {}
", prompt, combined_context.unwrap_or("".to_string()));
    fetch_llm_response(&prompt_with_context, model_config, client).await
}

pub async fn ask_llm_for_plan(
    model_config: &Model,
    instruction: &str,
    context_sources: &[String],
    execution_history: &[(Action, Option<String>)],
    client: &Client
) -> Result<Plan> {
    let combined_context = get_combined_context(context_sources, client).await?;

    let plan_prompt = format!(
        "Based on the following instruction and context, create a step-by-step plan to achieve the goal.
        NEVER directly reply with actions CreateFile, OverwriteFileContents, ReplaceFileLines unless asked to.
        Output the plan ONLY as a JSON object matching the following Rust interface (\"action\" tag MUST BE snake_case):

        ```rust
    #[derive(Serialize, Deserialize, Debug, Clone)]
    #[serde(tag = \"action\", rename_all = \"snake_case\")]
    pub enum Action {{
        //Ask Llm to reply with a one action subplan consisting of CreateFile action for the file with `path`
        AskLlmToCreateFile {{ action_idx: u32, path: String }},
        //Create file on the machine of the user, `content` WILL NOT BE EXPANDED OR PARSED AND WILL BE TREATED LITERALLY, no output
        CreateFile {{ action_idx: u32, path: String, content: String }},
        //Run command on the machine of the user, `command` is the command to execute, output the result
        RunCommand {{ action_idx: u32, command: String }},
        //Search the web using the provided `query`, output the results
        SearchWeb {{ action_idx: u32, query: String }}, output the results
        //Read the content of the web page at the given `url`, output the result
        ReadWebPage {{ action_idx: u32, url: String }},
        //Ask the user the specified `question`, output the result
        AskUser {{ action_idx: u32, question: String }},
        //Delete the file at the specified `path`, no output
        DeleteFile {{ action_idx: u32, path: String }},
        //Ask Llm to reply with  a one action subplan consisting of a OverwriteFileContents action for the file with `path`, output the result of OverwriteFileContents
        AskLlmToOverwriteFileContents {{action_idx: u32, path: String}},
        // \"content \" WILL NOT BE EXPANDED OR PARSED AND WILL BE TREATED LITERALLY, no output
        OverwriteFileContents {{ action_idx: u32, path: String, content: String }},
        // Ask LLM to output a response to the user (using the knowledge of previous actions and their outputs)
        AskLlm {{ action_idx: u32, prompt: String }},
        // AskLlmForPlan provides the ability for the LLM to respond with a new subplan
        // 'instruction' guides the sub-plan generation.
        // 'context_sources' provides file paths or URLs for context.
        // the previously executed actions and their outputs are *always* provided to LLM in this action
        AskLlmForPlan {{
            action_idx: u32,
            instruction: String,
            context_sources: Vec<String>
        }},
        //Read the content of the file at the specified `path`, output the result
        ReadFile {{ action_idx: u32, path: String }},
        //Find files matching the given `pattern`, output the result
        FindFiles {{ action_idx: u32, pattern: String }},
        //Ask LLM to reply with a one action subplan consisting of a ReplaceFileLines action for the file with `path`, , output the result of ReplaceFileLines
        AskLlmToReplaceFileLines {{action_idx: u32, path: String}},
        //Replace lines from `from_line_idx` to `until_line_idx` in the file at `path` with `replacement_lines`, `replacement_lines` WILL NOT BE EXPANDED OR PARSED AND WILL BE TREATED LITERALLY, no output
        ReplaceFileLines {{action_idx: u32, path: String, from_line_idx: u32, until_line_idx: u32, replacement_lines: String}},
        // Append content to the file at the specified `path`, no output
        AppendToFile {{ action_idx: u32, path: String, content: String }},
        // Move the file from `source` to `destination`, no output
        MoveFile {{ action_idx: u32, source: String, destination: String }},
        // Copy the file from `source` to `destination`, no output
        CopyFile {{ action_idx: u32, source: String, destination: String }},
        // List the contents of the directory at `path`, output the result
        ListDirectory {{ action_idx: u32, path: String }},
        // Check if the path exists, output \"true\" or \"false\"
        CheckPathExists {{ action_idx: u32, path: String }},
    }}

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct Plan {{
        pub thought: Option<String>,
        pub steps: Vec<Action>,
    }}
        ```

        \"Previous executed actions (action and its output):\" {}

        \"Instruction:\" {}

        \"Context:\" {}

        Respond ONLY with a valid JSON object",
        serde_json::to_string_pretty(&execution_history).unwrap_or_else(|e| format!("Error serializing history: {}", e)),
        instruction,
        combined_context.as_deref().unwrap_or("No context provided.")
    );

    let plan_response = fetch_llm_response(&plan_prompt, model_config, client).await?;
    let response_json = json::strip_json_fence(&plan_response);
    println!("Response = '{}'", response_json);
    let plan: Plan = serde_json::from_str(response_json)
        .with_context(|| format!("Failed to parse extracted plan JSON string. Extracted string:\\n{}", plan_response))?;
    Ok(plan)
}

async fn get_combined_context(context_sources: &[String], client: &Client) -> Result<Option<String>> {
    let fetched_context = fetch_context(context_sources, client).await?;
    let combined_context = if !fetched_context.is_empty() {
        Some(
            fetched_context
                .iter()
                .map(|c| format!("Context from {}:\n{}\n", c.source, c.content))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    } else {
        None
    };
    Ok(combined_context)
}

async fn fetch_context(context_sources: &[String], client: &Client) -> Result<Vec<ContextContent>> {
    let mut fetched_contents = Vec::new();

    for source in context_sources {
        let content = if source.starts_with("http://") || source.starts_with("https://") {
            let url = Url::parse(source)?;
            let response = client.get(url.clone()).send().await
                .with_context(|| format!("Failed to fetch URL: {}", source))?;
            if !response.status().is_success() {
                anyhow::bail!("Failed to fetch URL: {} - Status: {}", source, response.status());
            }
            response.text().await
                .with_context(|| format!("Failed to read content from URL: {}", source))?
        } else {
            fs::read_to_string(source)
                .with_context(|| format!("Failed to read file: {}", source))?
        };
        fetched_contents.push(ContextContent {
            source: source.clone(),
            content
        });
    }
    //println!("Fetched context: {:?}", &fetched_contents);
    Ok(fetched_contents)
}

async fn fetch_llm_response(
    prompt: &str,
    model_config: &Model,
    client: &Client
) -> Result<String> {
    let request_body = &model_config.request_format
        .replace("{{prompt}}", &prompt.replace("\\", "\\\\").replace("\"", "\\\""))
        .replace("{{model}}", &model_config.model_identifier.clone().unwrap_or("?".to_string()));

    //println!("Prompt: {}", prompt);
    let mut request_builder = client.post(&model_config.api_url).body(request_body.to_string());

    if let Some(api_key) = &model_config.api_key {
        if let Some(api_key_header) = &model_config.api_key_header {
            if let Some((header_name, header_value)) = api_key_header.split_once(":") {
                let header_name = header_name.trim();
                let header_value = header_value.replace("{{api_key}}", api_key);
                request_builder = request_builder.header(header_name, header_value);
            } else {
                eprintln!("Warning: Invalid api_key_header format. Expected 'Header-Name: Header-Value': '{}'", api_key_header);
                request_builder = request_builder.bearer_auth(api_key);
            }
        } else {
            request_builder = request_builder.bearer_auth(api_key);
        }
    }

    let response = request_builder.send().await
        .with_context(|| format!("Failed to send request to {}", model_config.api_url))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response.text().await.unwrap_or_else(|_| "Could not read error body".to_string());
        anyhow::bail!(
            "LLM API request failed for model '{}' with status: {}. Response: {}",
            model_config.name,
            status,
            error_body
        );
    }
    let response_text = response.text().await
        .with_context(|| "Failed to read LLM response text")?;
    let response_json: Value = serde_json::from_str(&response_text)
        .with_context(|| format!("Failed to parse LLM response as JSON. Raw response:\\n{}", response_text))?;

    let selected_values = jsonpath_select(&response_json, &model_config.response_json_path)
        .map_err(|e| anyhow::anyhow!("JSONPath selection error: {}", e))?;

    match selected_values.first() {
        Some(Value::String(answer)) => Ok(answer.clone()),
        Some(other) => anyhow::bail!(
            "Expected a string at JSONPath '{}', but found: {:?}",
            &model_config.response_json_path,
            other
        ),
        None => {
            anyhow::bail!("Could not extract the value using the defined path, response='{}', path = '{}'", &response_json, &model_config.response_json_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::prelude::*;
    use tokio;

    #[test]
    fn test_strip_json_fence() {
        let input = r#"
        ```json
        {
          "steps": [
            {
              "action": "create_file",
              "path": "hello.py",
              "content": "print('Hello, world!')"
            }
          ]
        }
        ```"#;
       let expected = r#"{
          "steps": [
            {
              "action": "create_file",
              "path": "hello.py",
              "content": "print('Hello, world!')"
            }
          ]
        }"#;
       let result = json::strip_json_fence(input);
       assert_eq!(result, expected)
    }

    #[tokio::test]
    async fn test_fetch_context_file_not_found() {
        let client = Client::new();
        let sources = vec!["nonexistent_file.txt".to_string()];
        let result = fetch_context(&sources, &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_fetch_context_url_success() -> Result<()> {
        let server = MockServer::start();
        let mock_url = server.url("/test-page");
        let content = "<html>Hello World</html>";
        let mock = server.mock(|when, then| {
            when.method(GET).path("/test-page");
            then.status(200).body(content);
        });

        let client = Client::new();

        let sources = vec![mock_url.clone()];
        let result = fetch_context(&sources, &client).await?;

        mock.assert();
        assert_eq!(result, vec![ContextContent {
            source: mock_url.to_string(),
            content: content.to_string()
        }]);
        Ok(())
    }

     #[tokio::test]
    async fn test_fetch_context_url_error() {
        let server = MockServer::start();
        let client = Client::new();

        let mock_url = server.url("/error-page");
         let mock = server.mock(|when, then| {
            when.method(GET).path("/error-page");
            then.status(404);
        });

        let sources = vec![mock_url.clone()];
        let result = fetch_context(&sources, &client).await;

        mock.assert();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Status: 404"));
    }

    #[tokio::test]
    async fn test_ask_llm_with_format_string() {
        let server = MockServer::start();
        let client = Client::new();

        let mock_url = server.url("/formatted-test");
       let mock = server.mock(|when, then| {
            when.method(POST)
                .path("/formatted-test")
                .body("{\"model\": \"test_model\", \"input\": \"\n    Question: test prompt\n\n    Context: Context from test_context_file:\ntest context\n\n\"}");
            then.status(200)
                .header("Content-Type", "application/json")
                .body(r#"{"answer": "test answer"}"#);
        });

        let model_config = Model {
            name: "Test Model".to_string(),
            api_url: mock_url.clone(),
            api_key: None,
            api_key_header: None,
            model_identifier: Some("test_model".to_string()),
            request_format: r#"{"model": "{{model}}", "input": "{{prompt}}"}"#.to_string(),
            response_json_path: "$.answer".to_string(),
        };

        let prompt = "test prompt";
        let context_sources = vec!["test_context_file".to_string()];

        fs::write("test_context_file", "test context").unwrap();

        let result = ask_llm(&model_config, prompt, &context_sources, &client).await;

        fs::remove_file("test_context_file").unwrap();

        mock.assert();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test answer");
    }
}
