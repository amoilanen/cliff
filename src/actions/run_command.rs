use anyhow::{Context, Result};
use std::process::{Command, Stdio};
use colored::*;

pub(crate) async fn execute(command: &String) -> Result<Option<String>> {
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
}
