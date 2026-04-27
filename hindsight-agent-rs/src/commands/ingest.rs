//! Ingest command — upload documents into agent memory.

use anyhow::{Context, Result};
use std::io::Read;
use crate::api::HindsightClient;
use crate::config::Config;

pub fn ingest(
    agent_id: &str,
    title: &str,
    file_path: Option<&str>,
    inline_content: Option<&str>,
) -> Result<()> {
    let content = if let Some(path) = file_path {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))?
    } else if let Some(c) = inline_content {
        c.to_string()
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read from stdin")?;
        buf
    };

    if content.trim().is_empty() {
        anyhow::bail!("No content provided. Use --file, --content, or pipe to stdin.");
    }

    let config = Config::load()?;
    let agent = config.get_agent(agent_id)?;
    let client = HindsightClient::from_agent(agent)?;

    // Use title slug as document_id for upsert
    let doc_id = title.to_lowercase().replace(' ', "-");
    let result = client.retain(&content, Some(&doc_id))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
