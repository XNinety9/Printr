//! Découverte automatique Home Assistant (MQTT discovery) : Printr y apparaît tout seul comme un
//! appareil, avec ses capteurs (état, papier, file d'attente), un bouton « Nouveau rouleau », un
//! champ pour imprimer un texte et un bouton par ticket enregistré et partagé.
//!
//! Chaque entité a sa configuration, retenue, sous `<découverte>/<type>/<nœud>/<objet>/config`
//! (`homeassistant` par défaut, `PRINTR_MQTT_DISCOVERY=off` pour s'en passer). Les capteurs lisent
//! `printr/status` ; les boutons publient sur les sujets de commande habituels.

use serde_json::{Value, json};

use crate::i18n::t;

/// Préfixe de découverte de Home Assistant, par défaut.
pub const DEFAULT_PREFIX: &str = "homeassistant";

/// Le préfixe de découverte, ou `None` si elle est désactivée.
pub fn discovery_prefix() -> Option<String> {
    let value = std::env::var("PRINTR_MQTT_DISCOVERY").unwrap_or_default();
    let value = value.trim().trim_matches('/');
    match value {
        "" => Some(DEFAULT_PREFIX.to_owned()),
        "off" | "false" | "0" | "no" | "non" => None,
        other => Some(other.to_owned()),
    }
}

/// Nœud de l'appareil, tiré du préfixe MQTT : deux Printr sur un même broker ont des préfixes
/// différents, donc deux appareils distincts.
pub fn node_id(prefix: &str) -> String {
    prefix.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
}

/// Un ticket partagé, imprimable d'un bouton.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetButton {
    pub id: String,
    pub name: String,
}

/// Sujet de configuration d'une entité.
pub fn config_topic(discovery: &str, component: &str, node: &str, object: &str) -> String {
    format!("{discovery}/{component}/{node}/{object}/config")
}

/// Sujet de configuration du bouton d'un ticket partagé.
pub fn preset_topic(discovery: &str, node: &str, id: &str) -> String {
    config_topic(discovery, "button", node, &format!("preset_{}", node_id(id)))
}

/// Le sujet est-il la configuration d'une entité de cet appareil (`<découverte>/<type>/<nœud>/<objet>/config`) ?
pub fn is_own_config(discovery: &str, node: &str, topic: &str) -> bool {
    let Some(rest) = topic.strip_prefix(discovery).and_then(|r| r.strip_prefix('/')) else { return false };
    let parts: Vec<&str> = rest.split('/').collect();
    parts.len() == 4 && parts[1] == node && parts[3] == "config"
}

/// L'identifiant d'un ticket partagé d'après le sujet de son bouton, pour retirer ceux qui ne le
/// sont plus.
pub fn preset_of_topic<'a>(discovery: &str, node: &str, topic: &'a str) -> Option<&'a str> {
    topic
        .strip_prefix(discovery)?
        .strip_prefix("/button/")?
        .strip_prefix(node)?
        .strip_prefix("/preset_")?
        .strip_suffix("/config")
}

/// Les entités fixes : (type, objet, configuration).
pub fn entities(prefix: &str) -> Vec<(&'static str, &'static str, Value)> {
    let node = node_id(prefix);
    let status = format!("{prefix}/status");
    let base = |object: &str, name: &str| {
        json!({
            "name": name,
            "unique_id": format!("{node}_{object}"),
            "availability_topic": format!("{prefix}/availability"),
            "device": device(prefix),
        })
    };
    let with = |mut config: Value, extra: Value| {
        if let (Some(config), Some(extra)) = (config.as_object_mut(), extra.as_object()) {
            config.extend(extra.clone());
        }
        config
    };
    vec![
        (
            "sensor",
            "status",
            with(
                base("status", t("État", "Status")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ value_json.printer_status }}",
                    "device_class": "enum",
                    "options": ["ready", "cover_open", "paper_out", "missing"],
                    "icon": "mdi:printer-pos",
                }),
            ),
        ),
        (
            "binary_sensor",
            "problem",
            with(
                base("problem", t("Problème", "Problem")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ 'ON' if value_json.printer_status not in ['ready', None] else 'OFF' }}",
                    "device_class": "problem",
                }),
            ),
        ),
        (
            "sensor",
            "paper",
            with(
                base("paper", t("Papier restant", "Paper left")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ value_json.paper.percent }}",
                    "unit_of_measurement": "%",
                    "state_class": "measurement",
                    "icon": "mdi:paper-roll",
                }),
            ),
        ),
        (
            "sensor",
            "paper_length",
            with(
                base("paper_length", t("Papier restant (longueur)", "Paper left (length)")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ (value_json.paper.remaining_mm / 1000) | round(1) }}",
                    "unit_of_measurement": "m",
                    "device_class": "distance",
                    "state_class": "measurement",
                    "icon": "mdi:paper-roll",
                }),
            ),
        ),
        (
            "binary_sensor",
            "paper_low",
            with(
                base("paper_low", t("Papier presque fini", "Paper almost out")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ 'ON' if value_json.paper.low else 'OFF' }}",
                    "device_class": "problem",
                    "icon": "mdi:paper-roll-outline",
                }),
            ),
        ),
        (
            "sensor",
            "queued",
            with(
                base("queued", t("Tickets en attente", "Queued tickets")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ value_json.queued }}",
                    "state_class": "measurement",
                    "icon": "mdi:tray-full",
                }),
            ),
        ),
        (
            "sensor",
            "last_ticket",
            with(
                base("last_ticket", t("Dernier ticket", "Last ticket")),
                json!({
                    "state_topic": status,
                    "value_template": "{{ value_json.last_ticket.label if value_json.last_ticket else '' }}",
                    "json_attributes_topic": status,
                    "json_attributes_template": "{{ (value_json.last_ticket or {}) | tojson }}",
                    "icon": "mdi:receipt-text-outline",
                }),
            ),
        ),
        (
            "button",
            "new_roll",
            with(
                base("new_roll", t("Nouveau rouleau", "New roll")),
                json!({
                    "command_topic": format!("{prefix}/paper/roll"),
                    "payload_press": "new",
                    "icon": "mdi:paper-roll-outline",
                }),
            ),
        ),
        (
            "text",
            "message",
            with(
                base("message", t("Imprimer un texte", "Print a text")),
                json!({
                    "command_topic": format!("{prefix}/print"),
                    "max": 255,
                    "icon": "mdi:message-text-outline",
                }),
            ),
        ),
    ]
}

/// Le bouton d'un ticket partagé.
pub fn preset_entity(prefix: &str, preset: &PresetButton) -> Value {
    let node = node_id(prefix);
    json!({
        "name": preset.name,
        "unique_id": format!("{node}_preset_{}", node_id(&preset.id)),
        "availability_topic": format!("{prefix}/availability"),
        "device": device(prefix),
        "command_topic": format!("{prefix}/print/ticket"),
        "payload_press": preset.id,
        "icon": "mdi:receipt-text",
    })
}

fn device(prefix: &str) -> Value {
    json!({
        "identifiers": [format!("printr_{}", node_id(prefix))],
        "name": if prefix == "printr" { "Printr".to_owned() } else { format!("Printr ({prefix})") },
        "manufacturer": "Printr",
        "model": t("Imprimante à tickets", "Receipt printer"),
        "sw_version": env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_share_one_device_and_read_the_status_topic() {
        let entities = entities("maison/printr");
        assert!(entities.len() >= 9);
        let mut ids = std::collections::HashSet::new();
        for (_, object, config) in &entities {
            assert!(ids.insert(config["unique_id"].as_str().unwrap().to_owned()), "{object} : identifiant unique");
            assert_eq!(config["device"]["identifiers"][0], "printr_maison_printr");
            assert_eq!(config["availability_topic"], "maison/printr/availability");
            if let Some(topic) = config.get("state_topic") {
                assert_eq!(topic, "maison/printr/status");
            }
        }
        assert_eq!(
            config_topic("homeassistant", "sensor", "printr", "paper"),
            "homeassistant/sensor/printr/paper/config"
        );
    }

    #[test]
    fn own_configs_are_recognised() {
        assert!(is_own_config("homeassistant", "printr", "homeassistant/sensor/printr/paper/config"));
        assert!(is_own_config("homeassistant", "printr", "homeassistant/button/printr/preset_a1/config"));
        assert!(!is_own_config("homeassistant", "printr", "homeassistant/sensor/autre/paper/config"));
        assert!(!is_own_config("homeassistant", "printr", "homeassistant/status"));
        assert!(!is_own_config("homeassistant", "printr", "printr/status"));
    }

    #[test]
    fn preset_topics_round_trip() {
        let topic = preset_topic("homeassistant", "printr", "a1b2c3");
        assert_eq!(topic, "homeassistant/button/printr/preset_a1b2c3/config");
        assert_eq!(preset_of_topic("homeassistant", "printr", &topic), Some("a1b2c3"));
        assert_eq!(preset_of_topic("homeassistant", "printr", "homeassistant/button/printr/new_roll/config"), None);
        let config = preset_entity("printr", &PresetButton { id: "a1b2c3".into(), name: "Brief du matin".into() });
        assert_eq!(
            (config["command_topic"].as_str(), config["payload_press"].as_str()),
            (Some("printr/print/ticket"), Some("a1b2c3"))
        );
    }
}
