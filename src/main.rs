use bsky_sdk::BskyAgent;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv()?;

    let handle = env::var("handle")?;
    let password = env::var("password")?;

    let agent = BskyAgent::builder().build().await?;
    let session = agent.login(handle,password).await?;
    Ok(())
}
