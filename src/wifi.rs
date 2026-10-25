//! Première connexion au réseau, pour l'image Raspberry Pi (`PRINTR_WIFI_SETUP=1`) : si la machine
//! n'est reliée à aucun réseau au démarrage, Printr ouvre un point d'accès Wi-Fi « Printr-XXXX » et
//! imprime un ticket pour s'y connecter. Depuis le téléphone, on choisit le Wi-Fi de la maison ;
//! Printr le rejoint et imprime l'adresse de l'appli (ou, en cas d'échec, rouvre le point d'accès).
//!
//! Tout passe par NetworkManager (`nmcli`), lancé sans shell, chaque valeur dans son propre argument.

use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Result, bail};
use rand::seq::IndexedRandom;
use serde::Serialize;

use crate::doc::{Doc, Style};
use crate::i18n::t;
use crate::server::App;

/// Nom de la connexion du point d'accès dans NetworkManager.
const HOTSPOT: &str = "printr-hotspot";
/// Adresse de la machine sur son point d'accès (mode « partagé » de NetworkManager).
pub const HOTSPOT_ADDRESS: &str = "10.42.0.1";
/// Le temps laissé au réseau pour apparaître au démarrage, avant d'ouvrir le point d'accès.
const BOOT_WAIT: Duration = Duration::from_secs(60);
/// Lettres du mot de passe : sans les caractères qu'on confond sur un ticket (l, 1, o, 0).
const PASSWORD_LETTERS: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

/// Un réseau Wi-Fi vu depuis la machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Network {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
}

#[derive(Clone, Debug, Default)]
enum State {
    /// Rien à faire : réseau présent, ou mode désactivé.
    #[default]
    Off,
    /// Point d'accès ouvert, en attente du choix du Wi-Fi.
    Hotspot { ssid: String, password: String, networks: Vec<Network>, error: Option<String> },
    /// Tentative de connexion au Wi-Fi choisi.
    Joining { ssid: String },
}

static STATE: Mutex<State> = Mutex::new(State::Off);
/// Le serveur (pour imprimer) et son port, fixés au démarrage.
static SERVER: OnceLock<(Arc<App>, u16)> = OnceLock::new();

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Le mode est-il demandé (image Raspberry Pi) ?
pub fn enabled() -> bool {
    std::env::var("PRINTR_WIFI_SETUP").is_ok_and(|v| matches!(v.trim(), "1" | "on" | "true" | "yes" | "oui"))
}

/// Le point d'accès est-il ouvert (ou une connexion en cours) ?
pub fn active() -> bool {
    !matches!(*state(), State::Off)
}

/// Pendant la configuration, l'adresse de la page à ouvrir pour une requête adressée à un autre
/// hôte : c'est ce qui fait apparaître la page tout seul sur un téléphone (portail captif).
pub fn portal_redirect(host: Option<&str>) -> Option<String> {
    if !active() {
        return None;
    }
    let host = host.unwrap_or("");
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if name == HOTSPOT_ADDRESS || name == "localhost" || name.ends_with(".local") {
        return None;
    }
    SERVER.get().map(|(_, port)| format!("{}/", app_url(HOTSPOT_ADDRESS, *port)))
}

/// Ce que montre l'appli pendant la configuration (rien hors de ce mode).
pub fn describe() -> Option<serde_json::Value> {
    match &*state() {
        State::Off => None,
        State::Hotspot { ssid, networks, error, .. } => {
            Some(serde_json::json!({ "hotspot": ssid, "networks": networks, "error": error, "joining": null }))
        }
        State::Joining { ssid } => Some(serde_json::json!({ "joining": ssid })),
    }
}

/// `nmcli` en anglais (sortie stable), sans hériter de l'environnement du serveur.
fn nmcli(args: &[&str]) -> Result<String> {
    let out = Command::new("nmcli")
        .args(args)
        .env_clear()
        .env("LANG", "C")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.trim().trim_start_matches("Error: "));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Découpe une ligne de `nmcli -t` : champs séparés par « : », avec « \: » et « \\ » échappés.
fn terse_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => fields.last_mut().unwrap().extend(chars.next()),
            ':' => fields.push(String::new()),
            c => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

/// Les interfaces Wi-Fi, et une connexion réseau active hors point d'accès.
fn devices() -> Result<(Option<String>, bool)> {
    let mut wifi = None;
    let mut online = false;
    for line in nmcli(&["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"])?.lines() {
        let f = terse_fields(line);
        let [device, kind, status, connection] = f.as_slice() else { continue };
        if kind == "wifi" && wifi.is_none() {
            wifi = Some(device.clone());
        }
        if matches!(kind.as_str(), "ethernet" | "wifi") && status == "connected" && connection != HOTSPOT {
            online = true;
        }
    }
    Ok((wifi, online))
}

/// Les réseaux à portée, du plus fort au plus faible, une fois chacun.
fn parse_scan(text: &str) -> Vec<Network> {
    let mut found: Vec<Network> = Vec::new();
    for line in text.lines() {
        let f = terse_fields(line);
        let [ssid, signal, security] = f.as_slice() else { continue };
        if ssid.is_empty() {
            continue; // réseau masqué
        }
        let network = Network {
            ssid: ssid.clone(),
            signal: signal.parse().unwrap_or(0),
            secure: !security.trim().is_empty() && security != "--",
        };
        match found.iter_mut().find(|n| n.ssid == network.ssid) {
            Some(n) if n.signal < network.signal => *n = network,
            Some(_) => {}
            None => found.push(network),
        }
    }
    found.sort_by(|a, b| b.signal.cmp(&a.signal).then_with(|| a.ssid.cmp(&b.ssid)));
    found.truncate(30);
    found
}

fn scan() -> Vec<Network> {
    nmcli(&["-t", "-f", "SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "yes"])
        .map(|text| parse_scan(&text))
        .unwrap_or_default()
}

/// Le nom du point d'accès, stable d'un démarrage à l'autre (tiré de /etc/machine-id).
fn hotspot_name() -> String {
    let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
    let suffix: String = id.trim().chars().filter(char::is_ascii_hexdigit).take(4).collect();
    if suffix.len() == 4 { format!("Printr-{}", suffix.to_uppercase()) } else { "Printr".to_owned() }
}

fn new_password() -> String {
    let mut rng = rand::rng();
    (0..10).map(|_| *PASSWORD_LETTERS.choose(&mut rng).unwrap() as char).collect()
}

fn open_hotspot(ifname: &str, ssid: &str, password: &str) -> Result<()> {
    let _ = nmcli(&["connection", "delete", HOTSPOT]);
    nmcli(&["device", "wifi", "hotspot", "ifname", ifname, "con-name", HOTSPOT, "ssid", ssid, "password", password])?;
    Ok(())
}

/// Un nom de réseau et un mot de passe acceptables (WPA : 8 à 63 caractères, ou rien si ouvert).
pub fn check(ssid: &str, password: &str) -> Result<()> {
    if ssid.is_empty() || ssid.len() > 32 || ssid.starts_with('-') || ssid.chars().any(char::is_control) {
        bail!("{}", t("nom de réseau invalide", "invalid network name"));
    }
    let n = password.chars().count();
    if !(password.is_empty() || (8..=63).contains(&n)) || password.chars().any(char::is_control) {
        bail!("{}", t("le mot de passe Wi-Fi fait de 8 à 63 caractères", "the Wi-Fi password is 8 to 63 characters"));
    }
    Ok(())
}

/// L'adresse de l'appli pour un hôte donné (sans « :80 »).
fn app_url(host: &str, port: u16) -> String {
    if port == 80 { format!("http://{host}") } else { format!("http://{host}:{port}") }
}

/// Le texte du QR code qui connecte un téléphone au point d'accès.
fn wifi_qr(ssid: &str, password: &str) -> String {
    let escape = |s: &str| {
        s.chars().fold(String::new(), |mut out, c| {
            if matches!(c, '\\' | ';' | ',' | ':' | '"') {
                out.push('\\');
            }
            out.push(c);
            out
        })
    };
    format!("WIFI:T:WPA;S:{};P:{};;", escape(ssid), escape(password))
}

/// Ticket 1 : rejoindre le point d'accès, puis ouvrir la page de configuration.
fn hotspot_ticket(ssid: &str, password: &str, port: u16, error: Option<&str>) -> Doc {
    let mut doc = Doc::new();
    doc.text("Printr", Style::default().bold().center().size(2));
    if let Some(error) = error {
        doc.text(error, Style::default().center().bold());
        doc.feed(1);
    } else {
        doc.text(t("Bienvenue !", "Welcome!"), Style::default().center());
        doc.feed(1);
    }
    doc.text(
        t("1. Connecte ton téléphone à ce Wi-Fi :", "1. Connect your phone to this Wi-Fi:"),
        Style::default().bold(),
    );
    doc.qr(&wifi_qr(ssid, password), 6);
    doc.text(&format!("{}{ssid}", t("Réseau : ", "Network: ")), Style::default().center());
    doc.text(&format!("{}{password}", t("Mot de passe : ", "Password: ")), Style::default().center());
    doc.feed(1);
    doc.text(t("2. Puis ouvre cette page :", "2. Then open this page:"), Style::default().bold());
    let url = app_url(HOTSPOT_ADDRESS, port);
    doc.qr(&url, 6);
    doc.text(&url, Style::default().center());
    doc
}

/// Ticket 2 : connecté, voici l'adresse de l'appli.
fn joined_ticket(ssid: &str, urls: &[String]) -> Doc {
    let mut doc = Doc::new();
    doc.text("Printr", Style::default().bold().center().size(2));
    doc.text(&tr!("Connecté à « {ssid} » !", "Connected to “{ssid}”!"), Style::default().center());
    doc.feed(1);
    doc.text(
        t(
            "Reconnecte ton téléphone au Wi-Fi de la maison, puis ouvre Printr :",
            "Put your phone back on the home Wi-Fi, then open Printr:",
        ),
        Style::default(),
    );
    if let Some(first) = urls.first() {
        doc.qr(first, 6);
    }
    for url in urls {
        doc.text(url, Style::default().center().bold());
    }
    doc
}

/// Imprime un ticket de configuration ; à l'allumage l'imprimante peut ne pas être encore prête.
fn print_soon(app: &App, doc: &Doc) {
    for _ in 0..24 {
        match app.print_doc(doc) {
            Ok(()) => return,
            Err(_) => std::thread::sleep(Duration::from_secs(5)),
        }
    }
    eprintln!("{}", t("ticket de configuration Wi-Fi non imprimé", "Wi-Fi setup ticket not printed"));
}

/// Au démarrage, en arrière-plan : attend le réseau, sinon ouvre le point d'accès.
pub fn spawn(app: Arc<App>, port: u16) {
    if !enabled() {
        return;
    }
    let _ = SERVER.set((Arc::clone(&app), port));
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let ifname = loop {
            match devices() {
                Ok((_, true)) => return, // déjà en ligne
                Ok((Some(ifname), false)) if start.elapsed() >= BOOT_WAIT => break ifname,
                Ok((None, false)) if start.elapsed() >= BOOT_WAIT => {
                    eprintln!(
                        "{}",
                        t("pas de réseau, et pas de Wi-Fi pour le configurer", "no network, and no Wi-Fi to set it up")
                    );
                    return;
                }
                Err(e) if start.elapsed() >= BOOT_WAIT => {
                    eprintln!("{}", tr!("NetworkManager injoignable : {e}", "NetworkManager unreachable: {e}"));
                    return;
                }
                _ => std::thread::sleep(Duration::from_secs(3)),
            }
        };
        let ssid = hotspot_name();
        let password = new_password();
        // On regarde les réseaux avant : la puce Wi-Fi ne cherche plus guère une fois en point d'accès.
        let networks = scan();
        if let Err(e) = open_hotspot(&ifname, &ssid, &password) {
            eprintln!("{}", tr!("point d'accès impossible : {e}", "cannot open the hotspot: {e}"));
            return;
        }
        eprintln!("{}", tr!("point d'accès « {ssid} » ouvert", "hotspot “{ssid}” open"));
        *state() = State::Hotspot { ssid: ssid.clone(), password: password.clone(), networks, error: None };
        print_soon(&app, &hotspot_ticket(&ssid, &password, port, None));
    });
}

/// Rejoint le Wi-Fi choisi dans l'appli. La réponse part avant : le téléphone perd le point d'accès.
pub fn join(wanted: String, key: String) -> Result<()> {
    check(&wanted, &key)?;
    let Some((app, port)) = SERVER.get().cloned() else {
        bail!("{}", t("pas de configuration Wi-Fi en cours", "no Wi-Fi setup in progress"))
    };
    let (ssid, password, networks) = match &*state() {
        State::Hotspot { ssid, password, networks, .. } => (ssid.clone(), password.clone(), networks.clone()),
        _ => bail!("{}", t("pas de configuration Wi-Fi en cours", "no Wi-Fi setup in progress")),
    };
    *state() = State::Joining { ssid: wanted.clone() };
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(2));
        let ifname = devices().ok().and_then(|(wifi, _)| wifi).unwrap_or_else(|| "wlan0".to_owned());
        let _ = nmcli(&["connection", "down", HOTSPOT]);
        let mut args = vec!["--wait", "45", "device", "wifi", "connect", wanted.as_str()];
        if !key.is_empty() {
            args.extend(["password", key.as_str()]);
        }
        args.extend(["ifname", ifname.as_str()]);
        match nmcli(&args) {
            Ok(_) => {
                let _ = nmcli(&["connection", "delete", HOTSPOT]);
                *state() = State::Off;
                eprintln!("{}", tr!("connecté à « {wanted} »", "connected to “{wanted}”"));
                // Le temps d'obtenir une adresse.
                let mut ip = None;
                for _ in 0..20 {
                    ip = crate::detect::local_ipv4();
                    if ip.is_some() {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
                let mut urls = Vec::new();
                if let Some(host) =
                    std::fs::read_to_string("/etc/hostname").ok().map(|h| h.trim().to_owned()).filter(|h| !h.is_empty())
                {
                    urls.push(app_url(&format!("{host}.local"), port));
                }
                if let Some(ip) = ip {
                    urls.insert(0, app_url(&ip.to_string(), port));
                }
                print_soon(&app, &joined_ticket(&wanted, &urls));
            }
            Err(e) => {
                eprintln!("{}", tr!("connexion à « {wanted} » impossible : {e}", "cannot connect to “{wanted}”: {e}"));
                // NetworkManager garde un profil pour ce réseau : on l'enlève, pour réessayer proprement.
                let _ = nmcli(&["connection", "delete", "id", wanted.as_str()]);
                let error = tr!(
                    "Impossible de rejoindre « {wanted} ». Vérifie le mot de passe et réessaie.",
                    "Could not join “{wanted}”. Check the password and try again."
                );
                if let Err(e) = open_hotspot(&ifname, &ssid, &password) {
                    eprintln!("{}", tr!("point d'accès impossible : {e}", "cannot open the hotspot: {e}"));
                }
                *state() = State::Hotspot {
                    ssid: ssid.clone(),
                    password: password.clone(),
                    networks,
                    error: Some(error.clone()),
                };
                print_soon(&app, &hotspot_ticket(&ssid, &password, port, Some(&error)));
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_lines_are_split_with_escapes() {
        assert_eq!(terse_fields(r"Box\:5G:72:WPA2"), vec!["Box:5G", "72", "WPA2"]);
        assert_eq!(terse_fields(r"a\\b::"), vec![r"a\b", "", ""]);
    }

    #[test]
    fn scan_keeps_the_strongest_of_each_network() {
        let text = "Maison:40:WPA2\n:90:WPA2\nMaison:75:WPA2\nCafé:60:\nVoisin:60:WPA1 WPA2\n";
        let found = parse_scan(text);
        let names: Vec<_> = found.iter().map(|n| (n.ssid.as_str(), n.signal, n.secure)).collect();
        assert_eq!(names, vec![("Maison", 75, true), ("Café", 60, false), ("Voisin", 60, true)]);
    }

    #[test]
    fn wifi_values_are_checked() {
        assert!(check("Maison", "motdepasse").is_ok());
        assert!(check("Café", "").is_ok(), "réseau ouvert");
        assert!(check("Maison", "court").is_err());
        assert!(check("", "motdepasse").is_err());
        assert!(check("--help", "motdepasse").is_err(), "pris pour une option");
        assert!(check("a\nb", "motdepasse").is_err());
        assert!(check(&"x".repeat(33), "motdepasse").is_err());
    }

    #[test]
    fn qr_escapes_special_characters() {
        assert_eq!(wifi_qr("Printr-AB12", "abc;def"), r"WIFI:T:WPA;S:Printr-AB12;P:abc\;def;;");
    }

    #[test]
    fn urls_skip_the_default_port() {
        assert_eq!(app_url("printr.local", 80), "http://printr.local");
        assert_eq!(app_url("10.42.0.1", 8080), "http://10.42.0.1:8080");
    }

    #[test]
    fn hotspot_password_is_readable() {
        let p = new_password();
        assert_eq!(p.len(), 10);
        assert!(p.bytes().all(|b| PASSWORD_LETTERS.contains(&b)));
    }
}
