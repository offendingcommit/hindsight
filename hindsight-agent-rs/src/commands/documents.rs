//! Documents command — list retained documents.

use anyhow::Result;
use crate::api::HindsightClient;
use crate::config::Config;

pub fn list(agent_id: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.documents_list()?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
