//! HTTP client for Hindsight API.
//!
//! Thin wrapper over reqwest with agent config resolution.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value;

use crate::config::AgentConfig;

pub struct HindsightClient {
    client: Client,
    base_url: String,
    bank_id: String,
}

impl HindsightClient {
    pub fn from_agent(config: &AgentConfig) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if let Some(token) = &config.api_token {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .context("Invalid API token")?,
            );
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self {
            client,
            base_url: config.api_url.trim_end_matches('/').to_string(),
            bank_id: config.bank_id.clone(),
        })
    }

    fn bank_url(&self, path: &str) -> String {
        format!(
            "{}/v1/default/banks/{}{}",
            self.base_url, self.bank_id, path
        )
    }

    // ── Health ──────────────────────────────────────────

    pub fn health(&self) -> Result<()> {
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .context("Cannot reach Hindsight API")?;
        if !resp.status().is_success() {
            anyhow::bail!("Hindsight API unhealthy ({})", resp.status());
        }
        Ok(())
    }

    // ── Bank ────────────────────────────────────────────

    pub fn ensure_bank(&self) -> Result<()> {
        let resp = self
            .client
            .get(format!("{}/v1/default/banks", self.base_url))
            .send()?;
        if resp.status().is_success() {
            let body: Value = resp.json()?;
            if let Some(banks) = body["banks"].as_array() {
                for bank in banks {
                    if bank["bank_id"].as_str() == Some(&self.bank_id) {
                        return Ok(());
                    }
                }
            }
        }
        // Create via empty retain
        self.client
            .post(self.bank_url("/memories"))
            .json(&serde_json::json!({"items": [], "async": true}))
            .send()?;
        Ok(())
    }

    pub fn import_template(&self, template: &Value) -> Result<Value> {
        let resp = self
            .client
            .post(self.bank_url("/import"))
            .json(template)
            .send()?;
        if !resp.status().is_success() {
            let body = resp.text().unwrap_or_default();
            anyhow::bail!("Template import failed: {}", body);
        }
        Ok(resp.json()?)
    }

    // ── Wiki (Mental Models) ────────────────────────────

    pub fn wiki_list(&self) -> Result<Value> {
        let resp = self.client.get(self.bank_url("/mental-models")).send()?;
        resp.error_for_status_ref()
            .context("Failed to list wiki pages")?;
        Ok(resp.json()?)
    }

    pub fn wiki_get(&self, page_id: &str) -> Result<Value> {
        let resp = self
            .client
            .get(self.bank_url(&format!("/mental-models/{}", page_id)))
            .send()?;
        resp.error_for_status_ref()
            .context(format!("Failed to get page '{}'", page_id))?;
        Ok(resp.json()?)
    }

    pub fn wiki_create(
        &self,
        page_id: &str,
        name: &str,
        source_query: &str,
    ) -> Result<Value> {
        let body = serde_json::json!({
            "id": page_id,
            "name": name,
            "source_query": source_query,
            "trigger": {
                "mode": "delta",
                "refresh_after_consolidation": true,
                "exclude_mental_models": true,
                "fact_types": ["observation"],
            }
        });
        let resp = self
            .client
            .post(self.bank_url("/mental-models"))
            .json(&body)
            .send()?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            anyhow::bail!("Failed to create page ({}): {}", status, text);
        }
        Ok(resp.json()?)
    }

    pub fn wiki_update(
        &self,
        page_id: &str,
        name: Option<&str>,
        source_query: Option<&str>,
    ) -> Result<Value> {
        let mut body = serde_json::Map::new();
        if let Some(n) = name {
            body.insert("name".to_string(), Value::String(n.to_string()));
        }
        if let Some(sq) = source_query {
            body.insert("source_query".to_string(), Value::String(sq.to_string()));
        }
        let resp = self
            .client
            .patch(self.bank_url(&format!("/mental-models/{}", page_id)))
            .json(&Value::Object(body))
            .send()?;
        resp.error_for_status_ref()
            .context(format!("Failed to update page '{}'", page_id))?;
        Ok(resp.json()?)
    }

    pub fn wiki_delete(&self, page_id: &str) -> Result<()> {
        let resp = self
            .client
            .delete(self.bank_url(&format!("/mental-models/{}", page_id)))
            .send()?;
        resp.error_for_status_ref()
            .context(format!("Failed to delete page '{}'", page_id))?;
        Ok(())
    }

    // ── Recall ──────────────────────────────────────────

    pub fn recall(
        &self,
        query: &str,
        max_results: u32,
        types: &[String],
    ) -> Result<Value> {
        let mut body = serde_json::json!({
            "query": query,
            "max_results": max_results,
        });
        if !types.is_empty() {
            body["types"] = Value::Array(types.iter().map(|t| Value::String(t.clone())).collect());
        }
        let resp = self
            .client
            .post(self.bank_url("/memories/recall"))
            .json(&body)
            .send()?;
        resp.error_for_status_ref().context("Recall failed")?;
        Ok(resp.json()?)
    }

    // ── Ingest / Retain ─────────────────────────────────

    pub fn retain(&self, content: &str, document_id: Option<&str>) -> Result<Value> {
        let mut item = serde_json::json!({"content": content});
        if let Some(doc_id) = document_id {
            item["document_id"] = Value::String(doc_id.to_string());
        }
        let resp = self
            .client
            .post(self.bank_url("/memories"))
            .json(&serde_json::json!({"items": [item], "async": true}))
            .send()?;
        resp.error_for_status_ref().context("Retain failed")?;
        Ok(resp.json()?)
    }

    // ── Documents ───────────────────────────────────────

    pub fn documents_list(&self) -> Result<Value> {
        let resp = self.client.get(self.bank_url("/documents")).send()?;
        resp.error_for_status_ref()
            .context("Failed to list documents")?;
        Ok(resp.json()?)
    }

    // ── Consolidation ───────────────────────────────────

    pub fn consolidate(&self) -> Result<Value> {
        let resp = self
            .client
            .post(self.bank_url("/consolidate"))
            .send()?;
        resp.error_for_status_ref()
            .context("Failed to trigger consolidation")?;
        Ok(resp.json()?)
    }
}
