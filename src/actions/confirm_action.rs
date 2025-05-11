use anyhow::Result;
use std::io::{self, Write};

pub(crate) async fn execute(current_auto_confirm: bool) -> Result<(bool, bool)> {
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