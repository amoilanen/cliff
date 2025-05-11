use anyhow::Result;
use urlencoding::encode;

pub(crate) async fn execute(query: &String) -> Result<Option<String>> {
    println!("Action: Search web for '{}'", query);
    let url_encoded_query = encode(&query);
    let url = format!("https://api.duckduckgo.com/?q={}&format=json&pretty=1", url_encoded_query);
    let response = reqwest::get(&url).await?.text().await?;
    println!("Success: Web search completed. {}", response);
    Ok(Some(response))
}
