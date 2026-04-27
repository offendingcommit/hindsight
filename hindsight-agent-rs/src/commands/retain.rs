//! Retain command — raw content retention (used by harness plugins).

use anyhow::{Context, Result};
use std::io::Read;
use crate::api::HindsightClient;
use crate::config::Config;

pub fn retain(
    agent_id: &str,
    input_file: Option<&str>,
    document_id: Option<&str>,
) -> Result<()> {
    let content = if let Some(path) = input_file {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))?
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read from stdin")?;
        buf
    };

    if content.trim().is_empty() {
        anyhow::bail!("No content to retain.");
    }

    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;
    let result = client.retain(&content, document_id)?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
