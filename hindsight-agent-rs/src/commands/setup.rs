//! Setup command — one-shot agent onboarding.

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::api::HindsightClient;
use crate::config::{AgentConfig, Config};

pub fn setup(
    agent_id: &str,
    bank_id: &str,
    api_url: &str,
    api_token: Option<&str>,
    harness: &str,
    template: Option<&str>,
    content_dir: Option<&str>,
) -> Result<()> {
    eprintln!("Setting up agent '{}'", agent_id);
    eprintln!("  Bank:    {}", bank_id);
    eprintln!("  API:     {}", api_url);
    eprintln!("  Harness: {}", harness);
    eprintln!();

    let agent_config = AgentConfig {
        bank_id: bank_id.to_string(),
        api_url: api_url.to_string(),
        api_token: api_token.map(|s| s.to_string()),
        harness: harness.to_string(),
        workspace: None,
    };
    let client = HindsightClient::from_agent(&agent_config)?;

    // Health check
    client
        .health()
        .context(format!(
            "Cannot reach Hindsight at {}. Make sure the server is running.",
            api_url
        ))?;

    // Create bank (via template or ensure)
    eprintln!("Creating Hindsight bank...");
    if let Some(template_path) = template {
        let template_str = fs::read_to_string(template_path)
            .with_context(|| format!("Failed to read template: {}", template_path))?;
        let template_value: serde_json::Value = serde_json::from_str(&template_str)
            .with_context(|| format!("Invalid JSON in template: {}", template_path))?;
        client.import_template(&template_value)?;
        eprintln!("  Template imported.");
    } else {
        client.ensure_bank()?;
    }
    eprintln!("  Done.");

    // Ingest content directory
    if let Some(dir) = content_dir {
        ingest_content_dir(&client, dir)?;
    }

    // Save config
    eprintln!("Saving agent config...");
    let mut config = Config::load()?;
    config
        .agents
        .insert(agent_id.to_string(), agent_config);
    config.save()?;
    eprintln!("  Done.");

    // Harness-specific setup
    match harness {
        "hermes" => setup_hermes(agent_id)?,
        "openclaw" => setup_openclaw(agent_id)?,
        _ => eprintln!("  Unknown harness '{}', skipping harness setup.", harness),
    }

    eprintln!();
    eprintln!("Agent '{}' is ready.", agent_id);
    match harness {
        "hermes" => {
            if agent_id == "default" {
                eprintln!("  Start chatting: hermes");
            } else {
                eprintln!("  Start chatting: hermes --profile {}", agent_id);
            }
        }
        "openclaw" => {
            eprintln!("  Restart your openclaw gateway to pick up the new agent.");
        }
        _ => {}
    }

    Ok(())
}

const CONTENT_EXTENSIONS: &[&str] = &[".md", ".txt", ".html", ".json", ".csv", ".xml"];

fn ingest_content_dir(client: &HindsightClient, dir: &str) -> Result<()> {
    let path = Path::new(dir);
    if !path.is_dir() {
        anyhow::bail!("Content path is not a directory: {}", dir);
    }

    let mut entries: Vec<_> = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| CONTENT_EXTENSIONS.contains(&format!(".{}", ext).as_str()))
                    .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|e| e.path());

    if entries.is_empty() {
        eprintln!("  No files to ingest in {}", dir);
        return Ok(());
    }

    eprintln!("Ingesting {} file(s) from {}...", entries.len(), dir);
    for entry in &entries {
        let file_path = entry.path();
        let text = fs::read_to_string(&file_path)?;
        if text.trim().is_empty() {
            continue;
        }
        let stem = file_path.file_stem().unwrap().to_string_lossy();
        let result = client.retain(&text, Some(&stem))?;
        let op_id = result["operation_id"]
            .as_str()
            .unwrap_or("queued");
        eprintln!(
            "  {} → {}",
            file_path.file_name().unwrap().to_string_lossy(),
            op_id,
        );
    }
    eprintln!("  Content ingestion queued (async).");
    Ok(())
}

fn setup_hermes(agent_id: &str) -> Result<()> {
    eprintln!("Configuring Hermes...");

    // Create profile if not default
    if agent_id != "default" {
        let output = Command::new("hermes")
            .args(["profile", "create", agent_id, "--clone", "--no-alias"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                eprintln!("  Created Hermes profile '{}'.", agent_id);
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                if stderr.to_lowercase().contains("already exists") {
                    eprintln!("  Profile '{}' already exists.", agent_id);
                } else {
                    eprintln!(
                        "  Note: Create profile manually: hermes profile create {}",
                        agent_id
                    );
                }
            }
            Err(_) => {
                eprintln!("  Note: hermes CLI not found. Create profile manually.");
            }
        }
    }

    // Set memory provider
    let mut config_cmd = vec!["hermes"];
    if agent_id != "default" {
        config_cmd.extend(["--profile", agent_id]);
    }
    config_cmd.extend(["config", "set", "memory.provider", "hindsight_agent"]);

    let output = Command::new(config_cmd[0]).args(&config_cmd[1..]).output();
    match output {
        Ok(o) if o.status.success() => {
            eprintln!("  Memory provider set to hindsight_agent.");
        }
        _ => {
            eprintln!("  Note: Set memory provider with: hermes config set memory.provider hindsight_agent");
        }
    }

    eprintln!("  Done.");
    Ok(())
}

fn setup_openclaw(agent_id: &str) -> Result<()> {
    eprintln!("Configuring OpenClaw...");

    // Check if agent exists
    let output = Command::new("openclaw")
        .args(["agents", "list", "--json"])
        .output();

    let exists = match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            serde_json::from_str::<serde_json::Value>(&stdout)
                .ok()
                .and_then(|v| v["agents"].as_array().cloned())
                .map(|agents| agents.iter().any(|a| a["name"].as_str() == Some(agent_id)))
                .unwrap_or(false)
        }
        _ => false,
    };

    if exists {
        eprintln!("  Agent '{}' already exists in OpenClaw.", agent_id);
    } else {
        let output = Command::new("openclaw")
            .args(["agents", "add", agent_id, "--non-interactive"])
            .output();
        match output {
            Ok(o) if o.status.success() => {
                eprintln!("  Created OpenClaw agent '{}'.", agent_id);
            }
            _ => {
                eprintln!(
                    "  Note: Create agent manually: openclaw agents add {}",
                    agent_id
                );
            }
        }
    }

    eprintln!("  Done.");
    Ok(())
}
