//! Nouvelles versions de printr.
//!
//! **Signalement** (par défaut) : régulièrement (une fois par jour, `PRINTR_UPDATE_INTERVAL`
//! minutes sinon), le serveur lit les étiquettes `vX.Y.Z` du dépôt GitHub et signale à
//! l'administrateur une version plus récente. Rien n'est téléchargé ni installé.
//!
//! **Installation automatique** (`PRINTR_AUTO_UPDATE=auto`, réglable dans l'appli) : la dernière
//! release est téléchargée, sa liste d'empreintes `SHA256SUMS` vérifiée par sa signature minisign
//! (clé publique compilée dans printr, ou `PRINTR_UPDATE_KEY`), le binaire par son empreinte ; il
//! doit s'exécuter et annoncer la version attendue. Dans la plage horaire permise
//! (`PRINTR_UPDATE_WINDOW`, 3 h à 5 h par défaut) et une fois l'imprimante libre, il remplace le
//! programme (l'ancien reste dans `printr.old`) et printr redémarre. La nouvelle version doit
//! ensuite répondre sur son port : sinon, le script de garde lancé avant chaque démarrage
//! (`deploy/printr-update-guard.sh`) remet l'ancienne et écarte cette version pour de bon.
//!
//! La signature se fait sur le PC du mainteneur (`scripts/signer-release.sh`), avec une clé qui
//! n'en sort jamais : un compte GitHub piraté ne suffit pas à pousser une mise à jour. Sans clé,
//! ou dans un conteneur (c'est alors l'image qui se met à jour), rien ne s'installe.
//!
//! `PRINTR_UPDATE_CHECK=off` coupe tout contact avec GitHub.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::i18n::t;

const REPO: &str = "XNinety9/Printr";
const GITHUB_API: &str = "https://api.github.com/repos/XNinety9/Printr";
/// Clé publique des releases (minisign, ligne en base64). Vide tant qu'aucune release n'est
/// signée : le mode automatique n'installe alors rien.
const RELEASE_KEY: &str = "";
/// Première vérification peu après le démarrage.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const DEFAULT_MINUTES: u64 = 24 * 60;
/// Sans compte, GitHub accepte 60 requêtes par heure et par adresse : on reste très en dessous.
const MIN_MINUTES: u64 = 5;
const DEFAULT_WINDOW: &str = "03:00-05:00";
const MAX_BINARY: u64 = 64 * 1024 * 1024;
/// Téléchargement du programme : de quoi passer sur un Wi-Fi lent.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
/// La nouvelle version confirme si elle répond sur son port dans ce délai.
const CONFIRM_AFTER: Duration = Duration::from_secs(20);
const CONFIRM_WITHIN: Duration = Duration::from_secs(120);

// Fichiers d'état, dans le dossier de données (partagés avec le script de garde).
const PENDING: &str = "update-pending";
const REFUSED: &str = "update-refused";
const LAST: &str = "update-last";

/// Version plus récente que celle qui tourne.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Available {
    pub version: String,
    /// Page de la version sur GitHub (notes, fichiers).
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Notify,
    Auto,
}

pub struct Settings {
    pub mode: Mode,
    /// API du dépôt (GitHub, ou un faux dépôt pour le laboratoire).
    base: String,
    key: Option<String>,
    pub interval: Duration,
    /// Plage horaire des installations ; `None` : à toute heure.
    window: Option<(NaiveTime, NaiveTime)>,
}

/// « v1.4.0 » ou « 1.4.0 » → (1, 4, 0) ; les préversions (« 1.4.0-rc1 ») sont ignorées.
fn parse(tag: &str) -> Option<(u32, u32, u32)> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let mut next = || parts.next()?.parse().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

fn current() -> (u32, u32, u32) {
    parse(env!("CARGO_PKG_VERSION")).expect("version de printr")
}

/// La plus haute des étiquettes, si elle dépasse `current`.
fn newer(tags: &[String], current: &str) -> Option<Available> {
    let current = parse(current)?;
    let (tag, version) = tags.iter().filter_map(|t| Some((t, parse(t)?))).max_by_key(|(_, v)| *v)?;
    (version > current).then(|| Available {
        version: format!("{}.{}.{}", version.0, version.1, version.2),
        url: format!("https://github.com/{REPO}/releases/tag/{tag}"),
    })
}

/// « 30 » (minutes) ou « 20s » (secondes, pour le laboratoire). Le plancher de 5 minutes ne
/// vaut que pour GitHub.
fn interval(setting: Option<&str>, github: bool) -> Duration {
    let setting = setting.map(str::trim).filter(|s| !s.is_empty());
    let secs = match setting {
        Some(s) if s.ends_with('s') => s.trim_end_matches('s').parse().ok(),
        Some(s) => s.parse::<u64>().ok().map(|m| m * 60),
        None => None,
    }
    .unwrap_or(DEFAULT_MINUTES * 60);
    Duration::from_secs(if github { secs.max(MIN_MINUTES * 60) } else { secs.max(1) })
}

/// « 03:00-05:00 » (éventuellement à cheval sur minuit), ou « toujours ».
fn window(setting: &str) -> Option<(NaiveTime, NaiveTime)> {
    let (from, to) = setting.trim().split_once('-')?;
    Some((NaiveTime::parse_from_str(from.trim(), "%H:%M").ok()?, NaiveTime::parse_from_str(to.trim(), "%H:%M").ok()?))
}

fn in_window(now: NaiveTime, window: Option<(NaiveTime, NaiveTime)>) -> bool {
    match window {
        None => true,
        Some((from, to)) if from <= to => now >= from && now < to,
        Some((from, to)) => now >= from || now < to,
    }
}

/// Dans un conteneur, le programme fait partie de l'image : c'est elle qu'on met à jour.
fn in_container() -> bool {
    Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists()
}

pub fn settings() -> Option<Settings> {
    let env = |name| std::env::var(name).ok().filter(|v: &String| !v.trim().is_empty());
    if env("PRINTR_UPDATE_CHECK").is_some_and(|v| matches!(v.trim(), "off" | "0" | "false" | "non")) {
        return None;
    }
    let base = env("PRINTR_UPDATE_URL").unwrap_or_else(|| GITHUB_API.to_owned());
    let github = base == GITHUB_API;
    let window_setting = env("PRINTR_UPDATE_WINDOW").unwrap_or_else(|| DEFAULT_WINDOW.to_owned());
    let auto = env("PRINTR_AUTO_UPDATE").is_some_and(|v| v.trim() == "auto") && !in_container();
    Some(Settings {
        mode: if auto { Mode::Auto } else { Mode::Notify },
        key: env("PRINTR_UPDATE_KEY").or_else(|| Some(RELEASE_KEY.to_owned()).filter(|k| !k.is_empty())),
        interval: interval(env("PRINTR_UPDATE_INTERVAL").as_deref(), github),
        window: if matches!(window_setting.trim(), "toujours" | "always") { None } else { window(&window_setting) },
        base,
    })
}

/// L'installation automatique est-elle demandée, et possible (clé de signature présente) ?
pub fn auto_installs() -> bool {
    settings().is_some_and(|s| s.mode == Mode::Auto && s.key.is_some())
}

/// Ligne de la bannière de démarrage.
pub fn summary() -> String {
    let Some(s) = settings() else {
        return format!("{} (PRINTR_UPDATE_CHECK)", t("vérification désactivée", "update check disabled"));
    };
    let every = match s.interval.as_secs() {
        86_400 => t("chaque jour", "daily").to_owned(),
        n if n % 60 == 0 => tr!("toutes les {} min", "every {} min", n / 60),
        n => tr!("toutes les {n} s", "every {n} s"),
    };
    match (s.mode, &s.key) {
        (Mode::Notify, _) => tr!(
            "vérification {every} sur GitHub, rien n'est installé tout seul",
            "checking {every} on GitHub, nothing is installed automatically"
        ),
        (Mode::Auto, None) => tr!(
            "vérification {every} ; installation automatique demandée, mais sans clé de signature rien ne s'installe",
            "checking {every}; automatic install requested, but without a signing key nothing is installed"
        ),
        (Mode::Auto, Some(_)) => {
            let when = s.window.map_or(t("à toute heure", "at any time").to_owned(), |(a, b)| {
                tr!("entre {} et {}", "between {} and {}", a.format("%H:%M"), b.format("%H:%M"))
            });
            tr!(
                "vérification {every}, installation automatique signée, {when}",
                "checking {every}, signed automatic install, {when}"
            )
        }
    }
}

// ---- Lecture du dépôt ----

#[derive(Deserialize)]
struct Tag {
    name: String,
}

#[derive(Deserialize, Clone, Debug)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone, Debug)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn get(http: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut response = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("printr/", env!("CARGO_PKG_VERSION")))
        .call()
        .with_context(|| tr!("{url} injoignable", "{url} unreachable"))?;
    Ok(response.body_mut().with_config().limit(limit).read_to_vec()?)
}

fn latest_tags(http: &ureq::Agent, s: &Settings) -> Result<Option<Available>> {
    let tags: Vec<Tag> = serde_json::from_slice(&get(http, &format!("{}/tags?per_page=100", s.base), 1 << 20)?)
        .context(t("réponse du dépôt illisible", "repository response unreadable"))?;
    let names: Vec<String> = tags.into_iter().map(|t| t.name).collect();
    Ok(newer(&names, env!("CARGO_PKG_VERSION")))
}

fn latest_release(http: &ureq::Agent, s: &Settings) -> Result<Release> {
    let release: Release = serde_json::from_slice(&get(http, &format!("{}/releases/latest", s.base), 1 << 20)?)
        .context(t("release illisible", "release unreadable"))?;
    ensure!(
        !release.draft && !release.prerelease,
        "{}",
        t("la dernière release est un brouillon ou une préversion", "the latest release is a draft or a prerelease")
    );
    Ok(release)
}

// ---- Installation ----

/// Nom du binaire de cette machine dans une release : `printr-aarch64`, `printr-arm`,
/// `printr-x86_64` (le binaire ARM 32 bits vise ARMv6, et tourne donc aussi en ARMv7).
fn asset_name() -> String {
    format!("printr-{}", std::env::consts::ARCH)
}

fn state_dir() -> Result<PathBuf> {
    crate::store::data_dir().context(t("dossier de données introuvable", "data directory not found"))
}

fn refused(state: &Path) -> Vec<String> {
    fs::read_to_string(state.join(REFUSED)).unwrap_or_default().lines().map(|l| l.trim().to_owned()).collect()
}

/// Empreinte attendue d'un fichier dans une liste `SHA256SUMS` (« empreinte  nom »).
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.trim().to_lowercase())
    })
}

fn verify_signature(key: &str, data: &[u8], signature: &str) -> Result<()> {
    let key = minisign_verify::PublicKey::from_base64(key.trim())
        .map_err(|e| anyhow::anyhow!("{}", tr!("clé publique invalide : {e}", "invalid public key: {e}")))?;
    let signature = minisign_verify::Signature::decode(signature)
        .map_err(|e| anyhow::anyhow!("{}", tr!("signature illisible : {e}", "unreadable signature: {e}")))?;
    key.verify(data, &signature, false)
        .map_err(|e| anyhow::anyhow!("{}", tr!("signature refusée : {e}", "signature refused: {e}")))
}

/// Nouveau programme vérifié, prêt à remplacer l'actuel.
pub struct Prepared {
    pub from: String,
    pub to: String,
    new: PathBuf,
    exe: PathBuf,
    state: PathBuf,
}

fn prepare(http: &ureq::Agent, s: &Settings, release: &Release) -> Result<Option<Prepared>> {
    let Some(version) = parse(&release.tag_name) else {
        bail!("{}", tr!("étiquette de release illisible : {}", "unreadable release tag: {}", release.tag_name))
    };
    if version <= current() {
        return Ok(None);
    }
    let to = format!("{}.{}.{}", version.0, version.1, version.2);
    let state = state_dir()?;
    if refused(&state).contains(&to) {
        return Ok(None); // déjà essayée, et ramenée en arrière : on ne réessaie pas
    }
    let key = s
        .key
        .as_deref()
        .context(t("aucune clé de signature : installation impossible", "no signing key: cannot install"))?;
    let asset = |name: &str| {
        release
            .assets
            .iter()
            .find(|a| a.name == name)
            .with_context(|| tr!("release {to} sans fichier {name}", "release {to} has no {name} file"))
    };
    let binary_name = asset_name();
    let sums = get(http, &asset("SHA256SUMS")?.browser_download_url, 64 * 1024)?;
    let signature = get(http, &asset("SHA256SUMS.minisig")?.browser_download_url, 8 * 1024)?;
    verify_signature(key, &sums, &String::from_utf8_lossy(&signature))?;
    let expected = expected_hash(&String::from_utf8_lossy(&sums), &binary_name)
        .with_context(|| tr!("{binary_name} absent de SHA256SUMS", "{binary_name} missing from SHA256SUMS"))?;
    let binary = get(http, &asset(&binary_name)?.browser_download_url, MAX_BINARY)?;
    let actual: String = Sha256::digest(&binary).iter().map(|b| format!("{b:02x}")).collect();
    ensure!(
        actual == expected,
        "{}",
        tr!(
            "empreinte de {binary_name} incorrecte : fichier corrompu ou modifié",
            "wrong checksum for {binary_name}: corrupted or altered file"
        )
    );

    let exe = std::env::current_exe().context(t("emplacement du programme inconnu", "program location unknown"))?;
    let new = exe.with_file_name("printr.new");
    fs::write(&new, &binary).with_context(|| tr!("impossible d'écrire {}", "cannot write {}", new.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&new, fs::Permissions::from_mode(0o775))?;
    }
    // Le programme doit s'exécuter ici (bonne architecture) et annoncer la version attendue.
    let output = Command::new(&new)
        .arg("--version")
        .output()
        .context(t("le nouveau programme ne se lance pas", "the new program does not start"))?;
    let announced = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if announced_version(&announced) != Some(to.as_str()) {
        let _ = fs::remove_file(&new);
        bail!(
            "{}",
            tr!(
                "le nouveau programme annonce « {announced} » au lieu de printr {to}",
                "the new program says “{announced}” instead of printr {to}"
            )
        );
    }
    Ok(Some(Prepared { from: env!("CARGO_PKG_VERSION").to_owned(), to, new, exe, state }))
}

/// La version dans la sortie de `printr --version` : « printr 2.0.1 “Mergenthaler” » → « 2.0.1 ».
fn announced_version(output: &str) -> Option<&str> {
    let mut words = output.split_whitespace();
    (words.next()? == "printr").then(|| words.next()).flatten()
}

impl Prepared {
    /// Met le nouveau programme en place ; l'ancien reste dans `printr.old`.
    pub fn swap(&self) -> Result<()> {
        let old = self.exe.with_file_name("printr.old");
        fs::copy(&self.exe, &old).with_context(|| tr!("impossible de garder {}", "cannot keep {}", old.display()))?;
        fs::write(self.state.join(PENDING), format!("from={}\nto={}\n", self.from, self.to))?;
        let _ = fs::remove_file(self.state.join(format!("{PENDING}.starts")));
        fs::rename(&self.new, &self.exe)
            .with_context(|| tr!("impossible de remplacer {}", "cannot replace {}", self.exe.display()))?;
        Ok(())
    }
}

/// Ce que le serveur fait d'une mise à jour prête : attendre l'imprimante, remplacer, redémarrer.
pub trait Host: Send + Sync + 'static {
    fn restart_for_update(&self, update: &Prepared);
}

/// Lance la vérification périodique ; une version plus récente est rangée dans `latest`.
pub fn spawn(latest: Arc<Mutex<Option<Available>>>, host: Arc<dyn Host>) {
    let Some(s) = settings() else { return };
    std::thread::spawn(move || {
        // GitHub : adresses publiques seulement. Un autre dépôt vient de la configuration (le
        // laboratoire sert le sien en local).
        let http = crate::net::agent_with_timeout(s.base == GITHUB_API, DOWNLOAD_TIMEOUT);
        std::thread::sleep(FIRST_CHECK.min(s.interval));
        loop {
            if let Err(e) = round(&http, &s, &latest, host.as_ref()) {
                // Hors ligne, dépôt en panne, release refusée : on réessaiera à la prochaine échéance.
                eprintln!("{}", tr!("mise à jour : {e:#}", "update: {e:#}"));
            }
            std::thread::sleep(s.interval);
        }
    });
}

fn round(http: &ureq::Agent, s: &Settings, latest: &Mutex<Option<Available>>, host: &dyn Host) -> Result<()> {
    let release = match s.mode {
        Mode::Notify => {
            publish(latest, latest_tags(http, s)?);
            return Ok(());
        }
        Mode::Auto => latest_release(http, s)?,
    };
    let found = newer(std::slice::from_ref(&release.tag_name), env!("CARGO_PKG_VERSION"))
        .map(|a| Available { url: release.html_url.clone(), ..a });
    publish(latest, found.clone());
    if found.is_none() || !in_window(Local::now().time(), s.window) {
        return Ok(());
    }
    if let Some(prepared) = prepare(http, s, &release)? {
        eprintln!(
            "{}",
            tr!(
                "mise à jour : printr {} vérifiée (signature et empreinte), installation",
                "update: printr {} verified (signature and checksum), installing",
                prepared.to
            )
        );
        host.restart_for_update(&prepared);
    }
    Ok(())
}

fn publish(latest: &Mutex<Option<Available>>, found: Option<Available>) {
    let mut slot = latest.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(new) = found.as_ref().filter(|n| slot.as_ref() != Some(*n)) {
        eprintln!(
            "{}",
            tr!(
                "nouvelle version disponible : printr {} ({})",
                "new version available: printr {} ({})",
                new.version,
                new.url
            )
        );
    }
    *slot = found;
}

// ---- Confirmation après redémarrage ----

/// Dernière mise à jour réussie : (ancienne version, nouvelle, date).
pub fn last_update() -> Option<(String, String, String)> {
    let text = fs::read_to_string(state_dir().ok()?.join(LAST)).ok()?;
    let mut parts = text.split_whitespace();
    Some((parts.next()?.to_owned(), parts.next()?.to_owned(), parts.next()?.to_owned()))
}

/// Si printr vient d'être mis à jour : vérifie qu'il répond sur son port, puis le confirme. Sinon,
/// il s'arrête en erreur, et le script de garde remet l'ancienne version au démarrage suivant.
pub fn confirm_pending(listen: &str) {
    let Ok(state) = state_dir() else { return };
    let pending = state.join(PENDING);
    let Ok(text) = fs::read_to_string(&pending) else { return };
    let field = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).unwrap_or("?").to_owned();
    let (from, to) = (field("from="), field("to="));
    if to != env!("CARGO_PKG_VERSION") {
        return; // ce n'est pas la version attendue : le script de garde s'en charge
    }
    let address = listen.replace("0.0.0.0", "127.0.0.1");
    std::thread::spawn(move || {
        std::thread::sleep(CONFIRM_AFTER);
        let start = Instant::now();
        while start.elapsed() < CONFIRM_WITHIN {
            if answers(&address) {
                let _ = fs::remove_file(&pending);
                let _ = fs::remove_file(state.join(format!("{PENDING}.starts")));
                let _ = fs::write(state.join(LAST), format!("{from} {to} {}\n", Local::now().format("%Y-%m-%dT%H:%M")));
                eprintln!(
                    "{}",
                    tr!("mise à jour confirmée : printr {from} → {to}", "update confirmed: printr {from} → {to}")
                );
                return;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
        eprintln!(
            "{}",
            tr!(
                "printr {to} ne répond pas : arrêt, l'ancienne version {from} sera remise en place",
                "printr {to} does not answer: stopping, the previous version {from} will be put back"
            )
        );
        std::process::exit(1);
    });
}

/// Le serveur répond-il à une requête HTTP sur son port ?
fn answers(address: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect(address) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    if stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n").is_err() {
        return false;
    }
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).is_ok() && head.starts_with(b"HTTP/1.") && &head[9..12] == b"200"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn versions_are_compared_as_numbers() {
        assert_eq!(parse("v1.10.0"), Some((1, 10, 0)));
        assert_eq!(parse("1.4.0-rc1"), None);
        assert_eq!(parse("v2"), None);
        let found = newer(&tags(&["v1.3.0", "v1.10.0", "v1.9.2", "nightly"]), "1.9.9").unwrap();
        assert_eq!(found.version, "1.10.0");
        assert!(found.url.ends_with("/releases/tag/v1.10.0"));
    }

    #[test]
    fn nothing_when_up_to_date_or_ahead() {
        assert_eq!(newer(&tags(&["v1.2.0", "v1.3.0"]), "1.3.0"), None);
        assert_eq!(newer(&tags(&["v1.2.0"]), "1.3.0"), None);
        assert_eq!(newer(&tags(&[]), "1.3.0"), None);
    }

    #[test]
    fn interval_has_a_default_and_a_floor_for_github() {
        assert_eq!(interval(None, true), Duration::from_secs(24 * 3600));
        assert_eq!(interval(Some("10"), true), Duration::from_secs(600));
        assert_eq!(interval(Some("1"), true), Duration::from_secs(MIN_MINUTES * 60));
        assert_eq!(interval(Some("20s"), false), Duration::from_secs(20));
        assert_eq!(interval(Some("bof"), true), Duration::from_secs(24 * 3600));
    }

    #[test]
    fn install_window_wraps_around_midnight() {
        let t = |s| NaiveTime::parse_from_str(s, "%H:%M").unwrap();
        let night = window("03:00-05:00");
        assert!(in_window(t("03:30"), night) && !in_window(t("05:00"), night) && !in_window(t("12:00"), night));
        let late = window("22:00-02:00");
        assert!(in_window(t("23:00"), late) && in_window(t("01:00"), late) && !in_window(t("03:00"), late));
        assert!(in_window(t("12:00"), None));
        assert_eq!(window("n'importe quoi"), None);
    }

    #[test]
    fn announced_version_ignores_the_codename() {
        assert_eq!(announced_version("printr 2.0.1 “Mergenthaler”"), Some("2.0.1"));
        assert_eq!(announced_version("printr 2.0.1"), Some("2.0.1"));
        assert_eq!(announced_version("autre 2.0.1"), None);
        assert_eq!(announced_version(""), None);
    }

    #[test]
    fn checksum_list_is_read_like_sha256sum() {
        let sums = "aa11  printr-aarch64\nBB22 *printr-x86_64\n";
        assert_eq!(expected_hash(sums, "printr-aarch64").as_deref(), Some("aa11"));
        assert_eq!(expected_hash(sums, "printr-x86_64").as_deref(), Some("bb22"));
        assert_eq!(expected_hash(sums, "printr-arm"), None);
    }

    #[test]
    fn signatures_from_another_key_are_refused() {
        // Paire de test : clé publique et signature de « bonjour\n » produites par examples/signer.
        let key = "RWRMIqL5Nm0mMNBZfIIJeCEoY6y9yogvg19qqV0re3ns17WRgfrtktAi";
        let signature = "untrusted comment: signature from rsign secret key
RURMIqL5Nm0mMOXO5iWBwO8S/QrpJT3YfATJR2XymxnA2uf5yAU/rYaO9aMUB73ni2d2smh1+BiOsA5SKVjVFS4PPhhSeDgbdg0=
trusted comment: file:essai.txt
BQcuKLvRd5wodCubHFheHqNpXStqRmjTYii3guALe1l9ai9vGTWWDy7ELF0aUXfPQMQLDSIpRlg2nQzxqVOODg==
";
        assert!(verify_signature(key, b"bonjour\n", signature).is_ok());
        assert!(verify_signature(key, b"bonjour !\n", signature).is_err(), "contenu modifié");
        let other = "RWQQ94JtAPgIQ+/cgIAXOSRqXR5GcJ2DidCvvKq73pTLErY7h4DyslGJ";
        assert!(verify_signature(other, b"bonjour\n", signature).is_err(), "autre clé");
    }
}
