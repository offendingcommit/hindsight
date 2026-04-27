//! Agent configuration.
//!
//! Stores agent → Hindsight mapping in ~/.hindsight-agent/config.json.
//! Each agent has a bank_id, api_url, optional api_token, and harness.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub bank_id: String,
    pub api_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    pub harness: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub agents: HashMap<String, AgentConfig>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Config {
                agents: HashMap::new(),
            });
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config at {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, format!("{}\n", content))?;
        Ok(())
    }

    pub fn get_agent(&self, agent_id: &str) -> Result<&AgentConfig> {
        self.agents.get(agent_id).ok_or_else(|| {
            anyhow::anyhow!(
                "Agent '{}' not found. Run 'hindsight-agent setup {}' first.\n\
                 Available agents: {}",
                agent_id,
                agent_id,
                if self.agents.is_empty() {
                    "(none)".to_string()
                } else {
                    self.agents.keys().cloned().collect::<Vec<_>>().join(", ")
                }
            )
        })
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".hindsight-agent")
        .join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let mut config = Config {
            agents: HashMap::new(),
        };
        config.agents.insert(
            "test-agent".to_string(),
            AgentConfig {
                bank_id: "test-bank".to_string(),
                api_url: "http://localhost:8888".to_string(),
                api_token: None,
                harness: "hermes".to_string(),
                workspace: None,
            },
        );
        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.agents.len(), 1);
        assert_eq!(parsed.agents["test-agent"].bank_id, "test-bank");
    }

    #[test]
    fn test_get_agent_not_found() {
        let config = Config {
            agents: HashMap::new(),
        };
        let result = config.get_agent("missing");
        assert!(result.is_err());
    }

    #[test]
    fn test_api_token_omitted_when_none() {
        let agent = AgentConfig {
            bank_id: "b".to_string(),
            api_url: "http://localhost".to_string(),
            api_token: None,
            harness: "hermes".to_string(),
            workspace: None,
        };
        let json = serde_json::to_string(&agent).unwrap();
        assert!(!json.contains("api_token"));
    }
}
