use anyhow::Result;
use std::io::{self, Write};
use colored::*;

pub(crate) async fn execute(question: &String) -> Result<Option<String>> {
    println!("Action: Ask user");
    print!("{} ", question.green());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let response = input.trim().to_string();
    Ok(Some(response))
}
