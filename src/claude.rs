//! Client minimal de l'API Claude (Messages API, HTTP direct : pas de SDK Rust officiel).

use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-opus-5-5";

/// Nombre maximal de reprises après un `pause_turn` (boucle d'outils côté serveur).
const MAX_CONTINUATIONS: usize = 5;

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
            // Les requêtes avec recherche web peuvent prendre une minute ou plus.
            .timeout_global(Some(Duration::from_secs(300)))
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
        let (value, _) = self.run(system, prompt, schema, effort, None)?;
        Ok(value)
    }

    /// Comme `generate`, avec l'outil de recherche web côté serveur. Renvoie aussi les URL
    /// des résultats de recherche, pour vérifier que les liens cités existent vraiment.
    pub fn search<T: DeserializeOwned>(
        &self,
        system: &str,
        prompt: &str,
        schema: Value,
        effort: &str,
        max_searches: u32,
    ) -> Result<(T, Vec<String>)> {
        let tools = json!([{ "type": "web_search_20260209", "name": "web_search", "max_uses": max_searches }]);
        self.run(system, prompt, schema, effort, Some(tools))
    }

    fn run<T: DeserializeOwned>(
        &self,
        system: &str,
        prompt: &str,
        schema: Value,
        effort: &str,
        tools: Option<Value>,
    ) -> Result<(T, Vec<String>)> {
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
        if let Some(tools) = tools {
            body["tools"] = tools;
        }

        // Une longue recherche peut s'arrêter en `pause_turn` : on renvoie alors la
        // conversation telle quelle et le serveur reprend là où il en était.
        let mut transcript: Vec<Value> = Vec::new();
        let mut reply = self.send(&body)?;
        for _ in 0..MAX_CONTINUATIONS {
            if reply["stop_reason"] != "pause_turn" {
                break;
            }
            transcript.extend(reply["content"].as_array().cloned().unwrap_or_default());
            body["messages"] = json!([
                { "role": "user", "content": prompt },
                { "role": "assistant", "content": transcript },
            ]);
            reply = self.send(&body)?;
        }
        match reply["stop_reason"].as_str() {
            Some("refusal") => bail!("Claude a refusé la requête"),
            Some("max_tokens") => bail!("réponse de Claude tronquée"),
            Some("pause_turn") => bail!("recherche trop longue, abandonnée"),
            _ => {}
        }
        let content = reply["content"].as_array().cloned().unwrap_or_default();
        transcript.extend(content.iter().cloned());

        // Le JSON final est dans les blocs de texte qui suivent le dernier appel d'outil.
        let last_tool = content.iter().rposition(|block| block["type"] != "text");
        let text: String = content[last_tool.map_or(0, |i| i + 1)..]
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect();
        let value = serde_json::from_str(&text).with_context(|| format!("JSON inattendu de Claude : {text}"))?;

        let urls = transcript
            .iter()
            .filter(|block| block["type"] == "web_search_tool_result")
            .filter_map(|block| block["content"].as_array())
            .flatten()
            .filter_map(|result| result["url"].as_str().map(str::to_owned))
            .collect();
        Ok((value, urls))
    }

    fn send(&self, body: &Value) -> Result<Value> {
        let mut request = self
            .agent
            .post(API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01");
        let mut body = body.clone();
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
        Ok(reply)
    }
}

fn supports_default_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"].iter().any(|p| model.starts_with(p))
}
