//! Recall command — search agent memories.

use anyhow::Result;
use crate::api::HindsightClient;
use crate::config::Config;

pub fn recall(
    agent_id: &str,
    query: &str,
    max_results: u32,
    types: &[String],
) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.recall(query, max_results, types)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
