//! Client minimal de l'API Claude (Messages API, HTTP direct : pas de SDK Rust officiel).

use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-5-5";

pub struct Claude {
    api_key: String,
    model: String,
    agent: ureq::Agent,
}

impl Claude {
    /// Lit `ANTHROPIC_API_KEY` (et `PRINTR_CLAUDE_MODEL`, optionnel).
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty())?;
        let model = std::env::var("PRINTR_CLAUDE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .build()
            .into();
        Some(Self { api_key, model, agent })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Envoie une requête et décode la réponse selon le schéma JSON fourni (sorties structurées).
    pub fn generate<T: DeserializeOwned>(&self, system: &str, prompt: &str, schema: Value, effort: &str) -> Result<T> {
        let mut body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": system,
            "output_config": {
                "effort": effort,
                "format": { "type": "json_schema", "schema": schema },
            },
            "messages": [{ "role": "user", "content": prompt }],
        });
        let mut request = self
            .agent
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01");
        // Relance automatique sur un autre modèle si le modèle principal refuse la requête.
        if supports_default_fallbacks(&self.model) {
            body["fallbacks"] = json!("default");
            request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }

        let mut response = request.send_json(&body).context("API Claude injoignable")?;
        let status = response.status();
        let reply: Value = response.body_mut().read_json().context("réponse de l'API Claude illisible")?;
        if std::env::var_os("PRINTR_DEBUG").is_some() {
            eprintln!("[claude] {reply}");
        }
        if !status.is_success() {
            let message = reply["error"]["message"].as_str().unwrap_or("erreur inconnue");
            bail!("API Claude ({status}) : {message}");
        }
        match reply["stop_reason"].as_str() {
            Some("refusal") => bail!("Claude a refusé la requête"),
            Some("max_tokens") => bail!("réponse de Claude tronquée"),
            _ => {}
        }

        let text: String = reply["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect();
        serde_json::from_str(&text).with_context(|| format!("JSON inattendu de Claude : {text}"))
    }
}

fn supports_default_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"].iter().any(|p| model.starts_with(p))
}
