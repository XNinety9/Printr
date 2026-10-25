//! Réglages modifiables depuis l'appli par l'administrateur : imprimante, langue, ville, clé Claude,
//! MQTT, quotas… Ils sont enregistrés avec les données (`printr.json`) et chargés au démarrage dans
//! l'environnement du processus, juste après `/etc/printr.env` et sans jamais l'écraser : une variable
//! fixée par le système (fichier d'environnement, systemd, Docker) garde la priorité, et l'appli la
//! montre comme verrouillée. Tout le reste du code continue de lire l'environnement.
//!
//! Un changement s'applique au redémarrage : sous systemd ou Docker, le serveur se relance tout seul
//! (voir `restart_soon`).

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::i18n::t;

/// Ce qu'accepte un réglage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    /// Jamais renvoyé à l'appli : seulement « défini ou non ».
    Secret,
    /// Une des valeurs listées (la chaîne vide : la valeur par défaut).
    Choice(&'static [&'static str]),
    /// Un entier entre ces bornes.
    Number(u32, u32),
    /// Chemin d'un périphérique, sous /dev.
    Device,
    /// `hôte:port`.
    Address,
}

/// Les réglages que l'appli peut modifier, dans l'ordre où elle les présente.
pub const SETTINGS: &[(&str, Kind)] = &[
    ("PRINTR_LANG", Kind::Choice(&["fr", "en"])),
    ("PRINTR_CITY", Kind::Text),
    ("PRINTR_DEVICE", Kind::Device),
    ("PRINTR_TCP", Kind::Address),
    ("PRINTR_CODEPAGE", Kind::Choice(&["850"])),
    ("ANTHROPIC_API_KEY", Kind::Secret),
    ("PRINTR_CLAUDE_MODEL", Kind::Text),
    ("PRINTR_TOKEN", Kind::Secret),
    ("PRINTR_MQTT_URL", Kind::Secret),
    ("PRINTR_MQTT_PREFIX", Kind::Text),
    ("PRINTR_MQTT_DISCOVERY", Kind::Text),
    ("PRINTR_MAX_TICKETS_PER_HOUR", Kind::Number(0, 1000)),
    ("PRINTR_QUEUE_HOURS", Kind::Number(1, 168)),
    ("PRINTR_UPDATE_CHECK", Kind::Choice(&["off"])),
];

fn kind(key: &str) -> Option<Kind> {
    SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, kind)| *kind)
}

/// Les réglages déjà fixés par le système au démarrage, avant ceux de l'appli.
static LOCKED: OnceLock<HashSet<&'static str>> = OnceLock::new();

fn set_in_env(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| !v.is_empty())
}

/// Les réglages enregistrés par l'appli, lus dans le fichier de données (sans le créer).
pub fn saved(data_file: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(data_file) else { return BTreeMap::new() };
    let Ok(Value::Object(data)) = serde_json::from_str::<Value>(&text) else { return BTreeMap::new() };
    match data.get("settings") {
        Some(Value::Object(map)) => map
            .iter()
            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
            .filter(|(k, v)| kind(k).is_some() && !v.is_empty())
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// Au démarrage, une seule fois : note ce que le système fixe, puis charge les réglages de l'appli
/// pour les clés libres. À appeler avant le moindre fil d'exécution.
pub fn load(data_file: Option<&Path>) {
    let locked: HashSet<&'static str> = SETTINGS.iter().map(|(k, _)| *k).filter(|k| set_in_env(k)).collect();
    if let Some(file) = data_file {
        for (key, value) in saved(file) {
            if !locked.contains(key.as_str()) {
                // SAFETY : appelé au tout début de `main`, avant le moindre fil d'exécution.
                unsafe { std::env::set_var(&key, &value) };
            }
        }
    }
    let _ = LOCKED.set(locked);
}

pub fn is_locked(key: &str) -> bool {
    LOCKED.get().is_some_and(|l| l.contains(key))
}

/// Une valeur saisie dans l'appli, vérifiée et nettoyée ; la chaîne vide efface le réglage.
pub fn check(key: &str, value: &str) -> Result<String> {
    let Some(kind) = kind(key) else { bail!("{}", tr!("réglage inconnu : {key}", "unknown setting: {key}")) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    if value.chars().any(char::is_control) || value.len() > 500 {
        bail!("{}", tr!("valeur invalide pour {key}", "invalid value for {key}"));
    }
    match kind {
        // Un jeton trop court empêcherait le serveur de démarrer (voir `main`).
        Kind::Secret if key == "PRINTR_TOKEN" && value.chars().count() < 16 => {
            bail!("{}", t("le jeton doit faire au moins 16 caractères", "the token must be at least 16 characters"));
        }
        Kind::Text | Kind::Secret => {}
        Kind::Choice(allowed) => {
            if !allowed.contains(&value) {
                bail!("{}", tr!("valeur invalide pour {key}", "invalid value for {key}"));
            }
        }
        Kind::Number(min, max) => match value.parse::<u32>() {
            Ok(n) if (min..=max).contains(&n) => {}
            _ => bail!("{}", tr!("{key} : un nombre de {min} à {max}", "{key}: a number from {min} to {max}")),
        },
        Kind::Device => {
            if !value.starts_with("/dev/") || value.contains("..") {
                bail!("{}", t("l'imprimante USB doit être sous /dev", "the USB printer must be under /dev"));
            }
        }
        Kind::Address => {
            let port = value.rsplit_once(':').and_then(|(h, p)| (!h.is_empty()).then_some(p));
            if port.and_then(|p| p.parse::<u16>().ok()).is_none() {
                bail!("{}", t("adresse attendue : hôte:port", "expected address: host:port"));
            }
        }
    }
    Ok(value.to_owned())
}

/// Ce que voit l'appli : pour chaque réglage, sa valeur en vigueur (les secrets seulement « définis »),
/// et s'il est fixé par le système.
pub fn describe() -> Value {
    let items: Vec<Value> = SETTINGS
        .iter()
        .map(|(key, kind)| {
            let current = std::env::var(key).ok().filter(|v| !v.is_empty());
            let value = if *kind == Kind::Secret { None } else { current.clone() };
            json!({ "key": key, "value": value, "set": current.is_some(), "locked": is_locked(key) })
        })
        .collect();
    json!({ "settings": items, "restart": restart_supported() })
}

/// Le serveur peut-il se relancer tout seul ? Sous systemd (`INVOCATION_ID`) ou dans un conteneur.
pub fn restart_supported() -> bool {
    std::env::var_os("INVOCATION_ID").is_some()
        || Path::new("/.dockerenv").exists()
        || Path::new("/run/.containerenv").exists()
}

/// Code de sortie d'un redémarrage voulu : systemd (`Restart=on-failure`) et Docker relancent.
pub const RESTART_CODE: i32 = 75;

/// Quitte le serveur peu après (le temps de répondre à l'appli), pour qu'il soit relancé avec les
/// nouveaux réglages.
pub fn restart_soon() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(800));
        eprintln!("{}", t("redémarrage pour appliquer les réglages", "restarting to apply the settings"));
        std::process::exit(RESTART_CODE);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_checked() {
        assert_eq!(check("PRINTR_LANG", " en ").unwrap(), "en");
        assert!(check("PRINTR_LANG", "de").is_err());
        assert_eq!(check("PRINTR_LANG", "").unwrap(), "");
        assert!(check("PRINTR_DEVICE", "/dev/usb/lp0").is_ok());
        assert!(check("PRINTR_DEVICE", "/etc/passwd").is_err());
        assert!(check("PRINTR_DEVICE", "/dev/../etc/passwd").is_err());
        assert!(check("PRINTR_TCP", "192.168.1.50:9100").is_ok());
        assert!(check("PRINTR_TCP", "192.168.1.50").is_err());
        assert!(check("PRINTR_QUEUE_HOURS", "12").is_ok());
        assert!(check("PRINTR_QUEUE_HOURS", "0").is_err());
        assert!(check("ANTHROPIC_API_KEY", "a\nb").is_err());
        assert!(check("PRINTR_TOKEN", "court").is_err(), "le serveur refuserait de démarrer");
        assert!(check("PRINTR_TOKEN", "0123456789abcdef").is_ok());
        assert!(check("PRINTR_DATA_DIR", "/tmp").is_err(), "pas modifiable depuis l'appli");
    }

    #[test]
    fn saved_settings_ignore_unknown_keys() {
        let path = std::env::temp_dir().join(format!("printr-settings-{}.json", std::process::id()));
        std::fs::write(
            &path,
            r#"{"users":[],"settings":{"PRINTR_LANG":"en","PRINTR_DATA_DIR":"/x","PRINTR_CITY":""}}"#,
        )
        .unwrap();
        let saved = saved(&path);
        assert_eq!(saved.into_iter().collect::<Vec<_>>(), vec![("PRINTR_LANG".to_owned(), "en".to_owned())]);
        let _ = std::fs::remove_file(path);
    }
}
