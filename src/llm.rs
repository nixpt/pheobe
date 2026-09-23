//! Model layer — OpenAI-shaped chat-completions client (v0.2 of the endpoint
//! decision in DESIGN.md: own client now, uno as an opt-in feature later).
//!
//! The turn loop depends on the `Provider` trait, never on the network —
//! tests inject a scripted mock. The OpenAI-shaped impl covers opencode's
//! server, pipefish `/v1`, Ollama, flownet, anything chat-completions-shaped.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_TIMEOUT_SECS: u64 = 120;

fn endpoint_timeout_secs() -> u64 {
    std::env::var("PHEOBE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    /// system | user | assistant | tool
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Msg {
    pub fn system(content: impl Into<String>) -> Self {
        Msg {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Msg {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: None,
        }
    }
    pub fn tool_result(id: &str, content: impl Into<String>) -> Self {
        Msg {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: vec![],
            tool_call_id: Some(id.to_string()),
        }
    }
    pub fn assistant(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Msg {
            role: "assistant".into(),
            content: Some(content.into()),
            tool_calls,
            tool_call_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// Always `"function"` on the wire. The OpenAI spec requires it and
    /// llama.cpp's server rejects an assistant message whose echoed
    /// tool_calls omit it ("Failed to parse messages: Missing tool call
    /// type"); flownet/Zen-style gateways merely tolerated its absence
    /// (issue 12, found on a Kaggle-GPU llama-server, 2026-09-16).
    #[serde(rename = "type", default = "tool_call_type")]
    pub kind: String,
    pub function: ToolFn,
}

fn tool_call_type() -> String {
    "function".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFn {
    pub name: String,
    /// JSON object as a string (the OpenAI wire shape).
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSchema {
    #[serde(rename = "type")]
    kind: String,
    function: ToolSchemaFn,
}

#[derive(Debug, Clone, Serialize)]
struct ToolSchemaFn {
    name: String,
    description: String,
    parameters: Value,
}

impl ToolSchema {
    pub fn new(name: &str, description: &str, parameters: Value) -> Self {
        ToolSchema {
            kind: "function".into(),
            function: ToolSchemaFn {
                name: name.into(),
                description: description.into(),
                parameters,
            },
        }
    }
    pub fn name(&self) -> &str {
        &self.function.name
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub turns: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Cost the engine reported (worker route; PHEOBE-46). None = the engine
    /// doesn't report cost — never estimated here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
}

/// The LLM behind the loop is pluggable; the loop is the contract.
pub trait Provider {
    /// One generate: send the transcript, get one assistant message.
    fn chat(&self, messages: &[Msg], tools: &[ToolSchema]) -> Result<(Msg, Option<u64>)>;
}

pub struct OpenAi {
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAi {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("PHEOBE_BASE_URL")
            .context("PHEOBE_BASE_URL not set — pheobe needs a model endpoint (e.g. pipefish /v1, opencode server, Ollama)")?;
        let model = std::env::var("PHEOBE_MODEL")
            .context("PHEOBE_MODEL not set — name the model pheobe should run")?;
        let api_key = std::env::var("PHEOBE_API_KEY").unwrap_or_default();
        Ok(OpenAi {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        })
    }
}

impl Provider for OpenAi {
    fn chat(&self, messages: &[Msg], tools: &[ToolSchema]) -> Result<(Msg, Option<u64>)> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
        });
        if tools.is_empty() {
            body.as_object_mut().unwrap().remove("tools");
        }
        let url = format!("{}/chat/completions", self.base_url);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(endpoint_timeout_secs()))
            .build()
            .context("http client")?;
        let mut req = client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().context("endpoint unreachable")?;
        let status = resp.status();
        let text = resp.text().context("reading endpoint response")?;
        if !status.is_success() {
            bail!("endpoint {status}: {}", truncate(&text, 400));
        }
        let v: Value = serde_json::from_str(&text).context("endpoint returned non-JSON")?;
        let choice = v
            .pointer("/choices/0/message")
            .context("endpoint response had no choices[0].message")?
            .clone();
        let msg: Msg = serde_json::from_value(choice)?;
        let total = v.pointer("/usage/total_tokens").and_then(|t| t.as_u64());
        Ok((msg, total))
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…[{} bytes truncated]", &s[..max], s.len() - max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::tests::env_lock;

    #[test]
    fn timeout_secs_honors_override_and_default() {
        let _guard = env_lock();
        std::env::set_var("PHEOBE_TIMEOUT_SECS", "999");
        assert_eq!(endpoint_timeout_secs(), 999);
        std::env::set_var("PHEOBE_TIMEOUT_SECS", "not-a-number");
        assert_eq!(endpoint_timeout_secs(), DEFAULT_TIMEOUT_SECS);
        std::env::remove_var("PHEOBE_TIMEOUT_SECS");
        assert_eq!(endpoint_timeout_secs(), DEFAULT_TIMEOUT_SECS);
    }
}
