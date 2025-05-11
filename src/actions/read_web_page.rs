use anyhow::Result;
use reqwest::Client;

pub(crate) async fn execute(client: &Client, url: &String) -> Result<Option<String>> {
    println!("Action: Read web page at '{}'", url);
    let response = client.get(url).send().await?.text().await?;
    println!("Success: Web page read. {}", response);
    Ok(Some(response))
}
