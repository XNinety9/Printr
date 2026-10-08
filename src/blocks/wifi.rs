//! Wi-Fi invités : un QR code qui connecte directement au réseau, le nom et le mot de passe en clair.

use serde::Deserialize;

use crate::doc::{Doc, Style};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    #[default]
    #[serde(alias = "wpa2", alias = "wpa3")]
    Wpa,
    Wep,
    /// Réseau ouvert, sans mot de passe.
    #[serde(alias = "aucune", alias = "open", alias = "ouvert")]
    None,
}

/// Échappe `\ ; , : "` comme le demande le format `WIFI:` des QR codes.
fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| match c {
            '\\' | ';' | ',' | ':' | '"' => vec!['\\', c],
            c => vec![c],
        })
        .collect()
}

/// Contenu du QR code, compris par l'appareil photo d'iOS et d'Android.
fn payload(ssid: &str, password: Option<&str>, security: Security, hidden: bool) -> String {
    let (kind, password) = match (security, password) {
        (Security::None, _) | (_, None) => ("nopass", String::new()),
        (Security::Wpa, Some(p)) => ("WPA", escape(p)),
        (Security::Wep, Some(p)) => ("WEP", escape(p)),
    };
    let mut out = format!("WIFI:T:{kind};S:{};", escape(ssid));
    if kind != "nopass" {
        out.push_str(&format!("P:{password};"));
    }
    if hidden {
        out.push_str("H:true;");
    }
    out.push(';');
    out
}

pub fn build(ssid: &str, password: Option<&str>, security: Security, hidden: bool, show_password: bool) -> anyhow::Result<Doc> {
    anyhow::ensure!(!ssid.trim().is_empty(), "indiquer le nom du réseau (`ssid`)");
    let password = password.filter(|p| !p.is_empty());
    anyhow::ensure!(
        security == Security::None || password.is_some(),
        "indiquer le mot de passe (`password`), ou `\"security\": \"none\"` pour un réseau ouvert"
    );

    let mut doc = Doc::new();
    doc.header("Wi-Fi invités");
    doc.feed(1);
    doc.qr(&payload(ssid, password, security, hidden), 8);
    doc.feed(1);
    doc.text("Scanne avec l'appareil photo pour te connecter", Style::default().small().center());
    doc.feed(1);
    doc.hanging("Réseau : ", ssid, Style::default().bold());
    if let (Some(password), true) = (password, show_password) {
        doc.hanging("Mot de passe : ", password, Style::default().bold());
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_escaped() {
        assert_eq!(payload("Maison", Some("a;b:c"), Security::Wpa, false), r"WIFI:T:WPA;S:Maison;P:a\;b\:c;;");
        assert_eq!(payload("Café", None, Security::None, true), "WIFI:T:nopass;S:Café;H:true;;");
    }

    #[test]
    fn password_is_required_unless_open() {
        assert!(build("Maison", None, Security::Wpa, false, true).is_err());
        assert!(build("Maison", None, Security::None, false, true).is_ok());
    }
}
