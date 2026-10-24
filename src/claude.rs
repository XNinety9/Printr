//! Client minimal de Claude, par deux chemins au choix :
//! - l'API (Messages API, HTTP direct : pas de SDK Rust officiel), avec `ANTHROPIC_API_KEY` ;
//! - la CLI Claude Code (`claude -p`), qui utilise sa propre connexion (abonnement Claude).

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::i18n::t;

const DEFAULT_API_URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-haiku-5-5";

/// URL des messages : `ANTHROPIC_BASE_URL` (compatible Claude Code) ou Anthropic par défaut.
fn api_url() -> String {
    match std::env::var("ANTHROPIC_BASE_URL").ok().filter(|u| !u.trim().is_empty()) {
        Some(mut u) => {
            u = u.trim().trim_end_matches('/').to_owned();
            if !u.ends_with("/v1/messages") {
                u.push_str("/v1/messages");
            }
            u
        }
        None => DEFAULT_API_URL.to_owned(),
    }
}

/// Délai d'un appel Claude : `PRINTR_CLAUDE_TIMEOUT_SECS` (défaut 120).
fn api_timeout() -> Duration {
    std::env::var("PRINTR_CLAUDE_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(120))
}

/// Une seule CLI à la fois : chaque appel lance un processus Node complet, gourmand en mémoire.
static CLI_LOCK: Mutex<()> = Mutex::new(());

pub struct Claude {
    model: String,
    backend: Backend,
}

enum Backend {
    Api { api_key: String, agent: ureq::Agent },
    Cli { program: String },
}

impl Claude {
    /// Choisit le chemin selon `PRINTR_CLAUDE_BACKEND` (`auto` par défaut, `api` ou `cli`) :
    /// en `auto`, l'API si `ANTHROPIC_API_KEY` est définie, sinon la CLI si elle est installée
    /// (`PRINTR_CLAUDE_CLI`, `claude` par défaut). `PRINTR_CLAUDE_MODEL` choisit le modèle.
    pub fn from_env() -> Option<Self> {
        let model = std::env::var("PRINTR_CLAUDE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty());
        let wanted = std::env::var("PRINTR_CLAUDE_BACKEND").unwrap_or_default().trim().to_ascii_lowercase();
        let backend = match (wanted.as_str(), api_key) {
            ("cli", _) | ("" | "auto", None) => {
                let program = cli_program();
                if !installed(&program) {
                    return None;
                }
                Backend::Cli { program }
            }
            (_, Some(api_key)) => Backend::Api { api_key, agent: api_agent() },
            (_, None) => return None,
        };
        Some(Self { model, backend })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Chemin utilisé : « API » ou « CLI ».
    pub fn backend(&self) -> &'static str {
        match self.backend {
            Backend::Api { .. } => "API",
            Backend::Cli { .. } => "CLI",
        }
    }

    /// Envoie une requête et décode la réponse selon le schéma JSON fourni (sorties structurées).
    pub fn generate<T: DeserializeOwned>(&self, system: &str, prompt: &str, schema: Value, effort: &str) -> Result<T> {
        let value = match &self.backend {
            Backend::Api { api_key, agent } => self.generate_api(agent, api_key, system, prompt, schema, effort)?,
            Backend::Cli { program } => self.generate_cli(program, system, prompt, &schema, effort)?,
        };
        serde_json::from_value(value.clone())
            .with_context(|| tr!("JSON inattendu de Claude : {value}", "unexpected JSON from Claude: {value}"))
    }

    fn generate_api(
        &self,
        agent: &ureq::Agent,
        api_key: &str,
        system: &str,
        prompt: &str,
        schema: Value,
        effort: &str,
    ) -> Result<Value> {
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
        let mut request = agent.post(api_url()).header("x-api-key", api_key).header("anthropic-version", "2023-06-01");
        // Relance automatique sur un autre modèle si le modèle principal refuse la requête.
        if supports_default_fallbacks(&self.model) {
            body["fallbacks"] = json!("default");
            request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }

        let mut response = request.send_json(&body).context(t("API Claude injoignable", "Claude API unreachable"))?;
        let status = response.status();
        let reply: Value = response
            .body_mut()
            .read_json()
            .context(t("réponse de l'API Claude illisible", "Claude API response unreadable"))?;
        debug(&reply);
        if !status.is_success() {
            let message = reply["error"]["message"].as_str().unwrap_or(t("erreur inconnue", "unknown error"));
            bail!("{}", tr!("API Claude ({status}) : {message}", "Claude API ({status}): {message}"));
        }
        match reply["stop_reason"].as_str() {
            Some("refusal") => bail!("{}", t("Claude a refusé la requête", "Claude refused the request")),
            Some("max_tokens") => bail!("{}", t("réponse de Claude tronquée", "Claude's response was cut short")),
            _ => {}
        }

        let text: String = reply["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect();
        serde_json::from_str(&text)
            .with_context(|| tr!("JSON inattendu de Claude : {text}", "unexpected JSON from Claude: {text}"))
    }

    fn generate_cli(&self, program: &str, system: &str, prompt: &str, schema: &Value, effort: &str) -> Result<Value> {
        let _one_at_a_time = CLI_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut cmd = Command::new(program);
        // Un simple appel au modèle : ni outils, ni réglages, ni CLAUDE.md, ni MCP, ni skills, ni
        // session gardée. Sans ça, le contexte embarqué multiplie le coût par cent.
        cmd.args(["-p", "--output-format", "json", "--no-session-persistence"])
            .args(["--tools", "", "--setting-sources", "", "--strict-mcp-config", "--disable-slash-commands"])
            .arg(format!("--model={}", self.model))
            .arg(format!("--effort={effort}"))
            .arg(format!("--json-schema={schema}"))
            .arg(format!("--system-prompt={system}"))
            .env("ENABLE_CLAUDEAI_MCP_SERVERS", "false")
            // La CLI doit utiliser sa connexion (abonnement), pas une clé API qui traînerait.
            .env_remove("ANTHROPIC_API_KEY")
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow::anyhow!(
                "{}",
                tr!(
                    "CLI Claude introuvable (« {program} ») : installe-la ou définis PRINTR_CLAUDE_CLI",
                    "Claude CLI not found (\"{program}\"): install it or set PRINTR_CLAUDE_CLI"
                )
            ),
            _ => anyhow::anyhow!(
                "{}",
                tr!("impossible de lancer la CLI Claude : {e}", "could not start the Claude CLI: {e}")
            ),
        })?;

        // Le prompt passe par l'entrée standard ; les sorties sont lues à part pour ne jamais bloquer.
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes())?;
        }
        let reader = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut text = String::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_string(&mut text);
                }
                text
            })
        };
        let stdout = reader(child.stdout.take().map(|s| Box::new(s) as _));
        let stderr = reader(child.stderr.take().map(|s| Box::new(s) as _));

        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if start.elapsed() > api_timeout() {
                let _ = child.kill();
                let _ = child.wait();
                bail!("{}", t("la CLI Claude ne répond pas", "the Claude CLI is not responding"));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let stdout = stdout.join().unwrap_or_default();
        let stderr = stderr.join().unwrap_or_default();

        let Ok(reply) = serde_json::from_str::<Value>(&stdout) else {
            let message = stderr.lines().chain(stdout.lines()).rfind(|l| !l.trim().is_empty());
            let message = message.unwrap_or(t("erreur inconnue", "unknown error"));
            bail!("{}", tr!("CLI Claude ({status}) : {message}", "Claude CLI ({status}): {message}"));
        };
        debug(&reply);
        if reply["is_error"].as_bool() == Some(true) || !status.success() {
            let message =
                reply["result"].as_str().or(reply["subtype"].as_str()).unwrap_or(t("erreur inconnue", "unknown error"));
            bail!("{}", tr!("CLI Claude : {message}", "Claude CLI: {message}"));
        }
        match reply.get("structured_output") {
            Some(value) if !value.is_null() => Ok(value.clone()),
            _ => {
                let text = reply["result"].as_str().unwrap_or_default();
                serde_json::from_str(text)
                    .with_context(|| tr!("JSON inattendu de Claude : {text}", "unexpected JSON from Claude: {text}"))
            }
        }
    }
}

fn api_agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(api_timeout())).http_status_as_error(false).build().into()
}

fn cli_program() -> String {
    std::env::var("PRINTR_CLAUDE_CLI").ok().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| "claude".to_owned())
}

/// Programme trouvé dans le PATH, ou chemin existant.
fn installed(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

fn debug(reply: &Value) {
    if std::env::var_os("PRINTR_DEBUG").is_some() {
        eprintln!("[claude] {reply}");
    }
}

fn supports_default_fallbacks(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"].iter().any(|p| model.starts_with(p))
}

/// Message quand Claude n'est pas disponible (ni clé API, ni CLI).
pub fn missing() -> &'static str {
    t(
        "Claude indisponible : définis ANTHROPIC_API_KEY ou installe la CLI Claude",
        "Claude unavailable: set ANTHROPIC_API_KEY or install the Claude CLI",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_url_resolution() {
        unsafe { std::env::remove_var("ANTHROPIC_BASE_URL") };
        assert_eq!(api_url(), DEFAULT_API_URL);

        unsafe { std::env::set_var("ANTHROPIC_BASE_URL", "http://127.0.0.1:8000") };
        assert_eq!(api_url(), "http://127.0.0.1:8000/v1/messages");

        unsafe { std::env::set_var("ANTHROPIC_BASE_URL", "http://127.0.0.1:8000/") };
        assert_eq!(api_url(), "http://127.0.0.1:8000/v1/messages");

        unsafe { std::env::set_var("ANTHROPIC_BASE_URL", "http://127.0.0.1:8000/v1/messages") };
        assert_eq!(api_url(), "http://127.0.0.1:8000/v1/messages");

        unsafe { std::env::set_var("ANTHROPIC_BASE_URL", "   ") };
        assert_eq!(api_url(), DEFAULT_API_URL);

        unsafe { std::env::remove_var("ANTHROPIC_BASE_URL") };
    }

    #[test]
    fn api_timeout_resolution() {
        unsafe { std::env::remove_var("PRINTR_CLAUDE_TIMEOUT_SECS") };
        assert_eq!(api_timeout(), Duration::from_secs(120));

        unsafe { std::env::set_var("PRINTR_CLAUDE_TIMEOUT_SECS", "600") };
        assert_eq!(api_timeout(), Duration::from_secs(600));

        unsafe { std::env::set_var("PRINTR_CLAUDE_TIMEOUT_SECS", "soon") };
        assert_eq!(api_timeout(), Duration::from_secs(120));

        unsafe { std::env::remove_var("PRINTR_CLAUDE_TIMEOUT_SECS") };
    }
}
