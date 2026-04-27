//! Agent management commands.

use anyhow::Result;
use crate::config::Config;

pub fn list() -> Result<()> {
    let config = Config::load()?;
    if config.agents.is_empty() {
        eprintln!("No agents configured. Run 'hindsight-agent setup' to add one.");
        return Ok(());
    }
    let output = serde_json::to_string_pretty(&config.agents)?;
    println!("{}", output);
    Ok(())
}

pub fn show(agent_id: &str) -> Result<()> {
    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let output = serde_json::to_string_pretty(agent)?;
    println!("{}", output);
    Ok(())
}
