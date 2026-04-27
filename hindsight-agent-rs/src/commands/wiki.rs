//! Wiki (knowledge pages) commands.

use anyhow::Result;
use crate::api::HindsightClient;
use crate::config::Config;

pub fn list(agent_id: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.wiki_list()?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub fn get(agent_id: &str, page_id: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.wiki_get(page_id)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub fn create(agent_id: &str, page_id: &str, name: &str, source_query: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.wiki_create(page_id, name, source_query)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub fn update(
    agent_id: &str,
    page_id: &str,
    name: Option<&str>,
    source_query: Option<&str>,
) -> Result<()> {
    if name.is_none() && source_query.is_none() {
        anyhow::bail!("At least one of --name or --source-query must be provided");
    }
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.wiki_update(page_id, name, source_query)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub fn delete(agent_id: &str, page_id: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    client.wiki_delete(page_id)?;
    println!("{{\"success\": true}}");
    Ok(())
}
