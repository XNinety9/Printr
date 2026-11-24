//! Données de l'application web : utilisateurs, sessions, presets, planifications, historique.
//! Tout tient dans un fichier JSON du répertoire de données, réécrit de façon atomique.
//!
//! Répertoire : `$PRINTR_DATA_DIR`, sinon `$STATE_DIRECTORY` (systemd `StateDirectory=`),
//! sinon `/var/lib/printr` s'il existe (installation sur le Pi), sinon `$XDG_DATA_HOME/printr`,
//! sinon `~/.local/share/printr`.
//!
//! Sur le Pi, le dossier appartient au groupe `printr` (setgid) : le service et les membres du
//! groupe le partagent, d'où des fichiers lisibles et modifiables par le groupe.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::i18n::t;

/// Nombre d'entrées conservées dans l'historique.
const HISTORY_MAX: usize = 300;
/// Durée de validité d'une session.
const SESSION_DAYS: i64 = 180;
/// Couleurs attribuées aux utilisateurs, dans l'ordre de création.
#[rustfmt::skip]
const COLORS: &[&str] = &["#e4572e", "#2e86ab", "#7a9e3f", "#a259c4", "#f2a541", "#3fa7a0"];

/// Données du service installé sur le Pi.
const SYSTEM_DIR: &str = "/var/lib/printr";

pub fn data_dir() -> Option<PathBuf> {
    let env = |name| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    env("PRINTR_DATA_DIR")
        .or_else(|| env("STATE_DIRECTORY"))
        .or_else(|| Some(PathBuf::from(SYSTEM_DIR)).filter(|d| d.is_dir()))
        .or_else(|| env("XDG_DATA_HOME").map(|d| d.join("printr")))
        .or_else(|| env("HOME").map(|d| d.join(".local/share/printr")))
}

/// Dossier des photos envoyées depuis l'interface.
pub fn uploads_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("images"))
}

/// Octets ESC/POS des tickets en file d'attente.
fn queue_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("queue"))
}

fn queue_file(id: &str) -> Option<PathBuf> {
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| queue_dir().map(|d| d.join(format!("{id}.bin"))))
        .flatten()
}

/// Garde les octets d'un ticket qui attend l'imprimante.
pub fn save_job(id: &str, bytes: &[u8]) -> Result<()> {
    let path = queue_file(id).context(t("répertoire de données introuvable", "data directory not found"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, bytes)?;
    Ok(())
}

pub fn load_job(id: &str) -> Option<Vec<u8>> {
    fs::read(queue_file(id)?).ok()
}

pub fn remove_job(id: &str) {
    if let Some(path) = queue_file(id) {
        let _ = fs::remove_file(path);
    }
}

/// Tickets imprimés gardés pour l'aperçu de l'historique (réservé à l'administrateur).
fn previews_dir() -> Option<PathBuf> {
    data_dir().map(|d| d.join("previews"))
}

/// Durée de conservation des aperçus : un petit mot avec photo pèse une centaine de Ko.
const PREVIEW_DAYS: u64 = 30;

/// Garde le ticket tel qu'imprimé (au format de l'aperçu de l'appli) et renvoie son identifiant.
/// Les aperçus de plus de `PREVIEW_DAYS` jours sont supprimés au passage.
pub fn save_preview(preview: &serde_json::Value) -> Option<String> {
    let dir = previews_dir()?;
    fs::create_dir_all(&dir).ok()?;
    prune_previews(&dir);
    let id = new_id(12);
    fs::write(dir.join(format!("{id}.json")), serde_json::to_vec(preview).ok()?).ok()?;
    Some(id)
}

/// L'aperçu `id`, s'il existe encore.
pub fn load_preview(id: &str) -> Option<serde_json::Value> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let bytes = fs::read(previews_dir()?.join(format!("{id}.json"))).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn prune_previews(dir: &Path) {
    let limit = std::time::Duration::from_secs(PREVIEW_DAYS * 24 * 3600);
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Identifiant aléatoire (hexadécimal).
pub fn new_id(bytes: usize) -> String {
    let mut rng = rand::rng();
    (0..bytes).map(|_| format!("{:02x}", rng.random::<u8>())).collect()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub name: String,
    pub color: String,
    password_hash: String,
    /// Peut retirer des comptes. Le premier compte l'est ; pour des données plus anciennes sans
    /// administrateur, le plus ancien compte (voir `Store::is_admin`).
    #[serde(default)]
    pub admin: bool,
    /// Blocs favoris du catalogue, dans l'ordre où ils ont été choisis.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub favorites: Vec<String>,
    /// Derniers blocs ajoutés à un ticket, du plus récent au plus ancien.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Session {
    token: String,
    user_id: String,
    expires: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Schedule {
    pub id: String,
    /// Jours de la semaine, 1 = lundi … 7 = dimanche.
    pub days: Vec<u8>,
    /// Heure locale « HH:MM ».
    pub time: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
    /// Périodicité ; absente (planifications d'avant) : chaque semaine, les jours cochés.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<Repeat>,
    /// Date de référence des périodicités qui en ont besoin (voir `Repeat::needs_start`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<NaiveDate>,
}

/// Périodicité d'une planification.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Repeat {
    /// Une seule fois, à la date de référence.
    Never,
    /// Chaque heure, de l'heure choisie jusqu'à minuit, les jours cochés.
    Hourly,
    Daily,
    Weekdays,
    Weekends,
    /// Les jours cochés, chaque semaine (comportement d'origine).
    Weekly,
    /// Les jours cochés, une semaine sur deux depuis la date de référence.
    Biweekly,
    /// Le jour du mois de la date de référence, tous les 1, 3, 6 ou 12 mois.
    Monthly,
    Quarterly,
    Semiannual,
    Yearly,
}

fn enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Preset {
    pub id: String,
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub shared: bool,
    /// Ticket JSON, tel qu'accepté par `printr print`.
    pub ticket: serde_json::Value,
    #[serde(default)]
    pub schedules: Vec<Schedule>,
    pub updated: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    /// Ticket en file d'attente (imprimante absente) : identifiant de la tâche, tant qu'il attend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued: Option<String>,
    /// Imprimé plus tard que demandé, sorti de la file d'attente à cette heure-là.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printed_at: Option<DateTime<Utc>>,
    /// Identifiant de l'aperçu gardé (voir `save_preview`), pour l'administrateur.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    pub at: DateTime<Utc>,
    /// Nom de l'utilisateur, ou « planification ».
    pub by: String,
    pub label: String,
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    /// Types des blocs imprimés (« sudoku », « météo »…), pour le bilan du mois.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<String>,
    /// Longueur de papier, en millimètres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paper_mm: Option<u32>,
}

/// Un article de la liste de courses partagée.
#[derive(Serialize, Deserialize, Clone)]
pub struct ShoppingItem {
    pub id: String,
    pub text: String,
    /// Prénom de qui l'a ajouté.
    pub by: String,
    pub added: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Data {
    #[serde(default)]
    users: Vec<User>,
    #[serde(default)]
    sessions: Vec<Session>,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    #[serde(default)]
    pub shopping: Vec<ShoppingItem>,
    /// Anniversaires communs à la famille, que chacun complète : une personne par ligne, comme
    /// dans le bloc Anniversaires, qui les ajoute aux siens.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub family_birthdays: String,
    /// Tickets qui attendent le retour de l'imprimante, du plus ancien au plus récent ; leurs
    /// octets ESC/POS sont dans `queue/`.
    #[serde(default)]
    pub queue: Vec<QueuedJob>,
    /// Blocs retirés du catalogue par l'administrateur (ceux dont la famille ne se sert pas). Les
    /// tickets qui les utilisent déjà continuent de fonctionner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_blocks: Vec<String>,
    #[serde(default)]
    pub paper: Paper,
    /// Réglages faits dans l'appli par l'administrateur (voir `settings`), par nom de variable.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub settings: std::collections::BTreeMap<String, String>,
    /// Assistant de premier lancement à faire : posé à la création du premier compte, retiré quand
    /// l'administrateur le termine (les installations existantes ne le voient pas).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wizard_pending: bool,
}

impl Data {
    pub fn users_count(&self) -> usize {
        self.users.len()
    }
}

/// Le rouleau en place, pour estimer le papier qui reste : l'imprimante ne dit « plus de papier »
/// que trop tard. Longueurs en mm, comptées comme `HistoryEntry::paper_mm`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Paper {
    /// Longueur d'un rouleau neuf : réglée à la pose, ou mesurée sur un rouleau entier.
    pub roll_mm: u32,
    /// Papier sorti depuis la pose du rouleau.
    pub used_mm: u32,
    /// Pose du rouleau ; aucune tant qu'on n'en a pas vu poser un (le compte part alors d'un
    /// rouleau supposé neuf, et ne sert pas à mesurer sa longueur).
    pub since: Option<DateTime<Utc>>,
}

impl Default for Paper {
    fn default() -> Self {
        Paper { roll_mm: Paper::DEFAULT_ROLL_MM, used_mm: 0, since: None }
    }
}

impl Paper {
    /// Un rouleau de 70 mm de diamètre : environ 70 m.
    pub const DEFAULT_ROLL_MM: u32 = 70_000;
    /// En dessous, « papier presque fini ».
    const LOW_PERCENT: u32 = 10;
    /// Longueurs de rouleau plausibles, pour la saisie et pour la mesure.
    pub const ROLL_RANGE_MM: std::ops::RangeInclusive<u32> = 5_000..=500_000;

    pub fn remaining_mm(&self) -> u32 {
        self.roll_mm.saturating_sub(self.used_mm)
    }

    pub fn percent(&self) -> u32 {
        (u64::from(self.remaining_mm()) * 100 / u64::from(self.roll_mm.max(1))) as u32
    }

    pub fn low(&self) -> bool {
        self.percent() < Self::LOW_PERCENT
    }

    /// Un ticket est sorti.
    pub fn consume(&mut self, mm: u32) {
        self.used_mm = self.used_mm.saturating_add(mm);
    }

    /// Un rouleau neuf est posé, d'une longueur donnée ou de la même que le précédent.
    pub fn new_roll(&mut self, roll_mm: Option<u32>) {
        if let Some(mm) = roll_mm {
            self.roll_mm = mm;
        }
        self.used_mm = 0;
        self.since = Some(Utc::now());
    }

    /// Le rouleau est arrivé au bout : s'il avait été posé neuf sous nos yeux, ce qu'il a donné
    /// est sa vraie longueur, qui servira pour les suivants (le compte en tient les écarts :
    /// avance avant la coupe, rouleaux plus courts qu'annoncé).
    pub fn ran_out(&mut self) {
        if self.since.is_some() && Self::ROLL_RANGE_MM.contains(&self.used_mm) {
            self.roll_mm = self.used_mm;
        }
    }
}

/// Un ticket prêt, qui attend l'imprimante.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QueuedJob {
    pub id: String,
    pub at: DateTime<Utc>,
    pub by: String,
    pub label: String,
}

/// Lit les données sur le disque, sans rien créer : pour les blocs (liste de courses, bilan).
pub fn read_data() -> Result<Data> {
    let dir = data_dir().context(t(
        "répertoire de données introuvable (définir PRINTR_DATA_DIR)",
        "data directory not found (set PRINTR_DATA_DIR)",
    ))?;
    Ok(Store::open_at(&dir.join("printr.json"))?.data)
}

pub struct Store {
    path: PathBuf,
    pub data: Data,
    /// Date de modification du fichier lu ou écrit en dernier : une modification venue d'ailleurs
    /// (`printr user add` pendant que le serveur tourne) est rechargée au lieu d'être écrasée.
    modified: Option<SystemTime>,
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Droits partagés avec le groupe (dossier setgid sur le Pi) ; sans effet ailleurs.
#[cfg(unix)]
fn share_with_group(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}

impl Store {
    pub fn open() -> Result<Self> {
        let dir = data_dir().context(t(
            "répertoire de données introuvable (définir PRINTR_DATA_DIR)",
            "data directory not found (set PRINTR_DATA_DIR)",
        ))?;
        let images = dir.join("images");
        if !images.is_dir() {
            fs::create_dir_all(&images)
                .with_context(|| tr!("impossible de créer {}", "cannot create {}", images.display()))?;
            #[cfg(unix)]
            share_with_group(&images, 0o2770);
        }
        Self::open_at(&dir.join("printr.json"))
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let data = match fs::read_to_string(path) {
            Ok(text) => {
                serde_json::from_str(&text).with_context(|| tr!("{} illisible", "{} is unreadable", path.display()))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e).with_context(|| tr!("impossible de lire {}", "cannot read {}", path.display())),
        };
        Ok(Self { path: path.to_owned(), data, modified: modified(path) })
    }

    /// Relit le fichier s'il a été modifié depuis la dernière lecture ou écriture.
    pub fn reload_if_changed(&mut self) -> Result<()> {
        let now = modified(&self.path);
        if now.is_some() && now != self.modified {
            *self = Self::open_at(&self.path)?;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Écrit dans un fichier temporaire puis renomme : jamais de fichier à moitié écrit.
    pub fn save(&mut self) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.data)?)?;
        #[cfg(unix)]
        share_with_group(&tmp, 0o660);
        fs::rename(&tmp, &self.path)?;
        self.modified = modified(&self.path);
        Ok(())
    }

    // ---- Sauvegarde ----

    /// Les données à sauvegarder : tout, sauf les sessions et la file d'attente (propres à la machine).
    pub fn backup_data(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(&self.data)?;
        if let Some(map) = value.as_object_mut() {
            map.remove("sessions");
            map.remove("queue");
        }
        Ok(value)
    }

    /// Remplace les données par une sauvegarde : tout le monde se reconnecte, et les tickets qui
    /// attendent l'imprimante sont gardés.
    pub fn restore(&mut self, mut data: Data) {
        data.sessions.clear();
        data.queue = std::mem::take(&mut self.data.queue);
        self.data = data;
    }

    // ---- Utilisateurs ----

    pub fn users(&self) -> &[User] {
        &self.data.users
    }

    pub fn user(&self, id: &str) -> Option<&User> {
        self.data.users.iter().find(|u| u.id == id)
    }

    pub fn user_mut(&mut self, id: &str) -> Option<&mut User> {
        self.data.users.iter_mut().find(|u| u.id == id)
    }

    fn find_by_name(&self, name: &str) -> Option<&User> {
        self.data.users.iter().find(|u| u.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Crée un compte ; renvoie son identifiant.
    pub fn add_user(&mut self, name: &str, password: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            bail!("{}", t("le prénom est vide", "the name is empty"));
        }
        if self.find_by_name(name).is_some() {
            bail!("{}", tr!("{name} existe déjà", "{name} already exists"));
        }
        let color = COLORS[self.data.users.len() % COLORS.len()].to_owned();
        let password_hash = hash(password)?;
        let id = new_id(8);
        let admin = self.data.users.is_empty();
        self.data.users.push(User {
            id: id.clone(),
            name: name.to_owned(),
            color,
            password_hash,
            admin,
            favorites: Vec::new(),
            recent: Vec::new(),
        });
        Ok(id)
    }

    /// Administrateur : le compte marqué comme tel, ou à défaut le plus ancien.
    pub fn is_admin(&self, id: &str) -> bool {
        match self.data.users.iter().find(|u| u.admin) {
            Some(_) => self.user(id).is_some_and(|u| u.admin),
            None => self.data.users.first().is_some_and(|u| u.id == id),
        }
    }

    pub fn set_password(&mut self, name: &str, password: &str) -> Result<()> {
        let hash = hash(password)?;
        let user = self.data.users.iter_mut().find(|u| u.name.eq_ignore_ascii_case(name.trim()));
        let user = user.with_context(|| tr!("utilisateur inconnu : {name}", "unknown user: {name}"))?;
        user.password_hash = hash;
        let id = user.id.clone();
        self.data.sessions.retain(|s| s.user_id != id);
        Ok(())
    }

    pub fn remove_user(&mut self, name: &str) -> Result<()> {
        let id = self
            .find_by_name(name)
            .map(|u| u.id.clone())
            .with_context(|| tr!("utilisateur inconnu : {name}", "unknown user: {name}"))?;
        self.remove_user_id(&id);
        Ok(())
    }

    /// Supprime un compte, ses sessions et ses presets.
    pub fn remove_user_id(&mut self, id: &str) {
        self.data.users.retain(|u| u.id != id);
        self.data.sessions.retain(|s| s.user_id != id);
        self.data.presets.retain(|p| p.owner != id);
    }

    // ---- Sessions ----

    /// Empreinte du mot de passe, pour la vérifier hors du verrou des données (Argon2 est lent).
    pub fn password_hash(&self, user_id: &str) -> Option<String> {
        self.user(user_id).map(|u| u.password_hash.clone())
    }

    /// Ouvre une session pour ce compte ; renvoie le jeton.
    pub fn open_session(&mut self, user_id: &str) -> String {
        let token = new_id(32);
        let now = Utc::now();
        self.data.sessions.retain(|s| s.expires > now);
        self.data.sessions.push(Session {
            token: token.clone(),
            user_id: user_id.to_owned(),
            expires: now + Duration::days(SESSION_DAYS),
        });
        token
    }

    pub fn session_user(&self, token: &str) -> Option<&User> {
        let now = Utc::now();
        let session = self.data.sessions.iter().find(|s| s.token == token && s.expires > now)?;
        self.user(&session.user_id)
    }

    pub fn logout(&mut self, token: &str) {
        self.data.sessions.retain(|s| s.token != token);
    }

    // ---- Historique ----

    pub fn record(&mut self, entry: HistoryEntry) {
        self.data.history.push(entry);
        let excess = self.data.history.len().saturating_sub(HISTORY_MAX);
        self.data.history.drain(..excess);
    }
}

/// Longueur minimale d'un nouveau mot de passe (l'appli est joignable depuis Internet).
pub const MIN_PASSWORD: usize = 8;

/// Vérifie un mot de passe contre son empreinte Argon2.
pub fn verify_password(hash: &str, password: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

fn hash(password: &str) -> Result<String> {
    if password.chars().count() < MIN_PASSWORD {
        bail!(
            "{}",
            tr!(
                "mot de passe trop court ({MIN_PASSWORD} caractères minimum)",
                "password too short (at least {MIN_PASSWORD} characters)"
            )
        );
    }
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("{}", tr!("hachage impossible : {e}", "hashing failed: {e}")))
}

impl Repeat {
    /// Écart en mois entre deux impressions, pour les périodicités calendaires.
    fn months(self) -> Option<u32> {
        match self {
            Repeat::Monthly => Some(1),
            Repeat::Quarterly => Some(3),
            Repeat::Semiannual => Some(6),
            Repeat::Yearly => Some(12),
            _ => None,
        }
    }

    /// La périodicité se compte à partir d'une date de référence (`Schedule::start`).
    pub fn needs_start(self) -> bool {
        matches!(self, Repeat::Never | Repeat::Biweekly) || self.months().is_some()
    }

    /// Jours imposés par la périodicité, quels que soient les jours cochés.
    pub fn fixed_days(self) -> Option<&'static [u8]> {
        match self {
            Repeat::Daily => Some(&[1, 2, 3, 4, 5, 6, 7]),
            Repeat::Weekdays => Some(&[1, 2, 3, 4, 5]),
            Repeat::Weekends => Some(&[6, 7]),
            _ => None,
        }
    }
}

impl Schedule {
    /// Prochaine échéance passée non encore exécutée, à moins de `grace` minutes.
    /// Renvoie l'heure d'échéance si la planification doit partir maintenant.
    pub fn due(&self, now: DateTime<Local>, grace: Duration) -> Option<DateTime<Utc>> {
        self.due_in(&now, grace)
    }

    /// Comme `due`, dans un fuseau horaire quelconque (les tests en fixent un).
    pub fn due_in<Tz: TimeZone>(&self, now: &DateTime<Tz>, grace: Duration) -> Option<DateTime<Utc>> {
        if !self.enabled {
            return None;
        }
        let (h, m) = self.time.split_once(':')?;
        let time = NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)?;
        let tz = &now.timezone();
        let utc_now = now.with_timezone(&Utc);
        let today = now.date_naive();
        // La veille aussi : une échéance de 23 h 55 part encore à 0 h 05.
        [today.pred_opt()?, today]
            .into_iter()
            .filter(|&date| self.occurs_on(date))
            .flat_map(|date| self.times(time).into_iter().filter_map(move |t| local_instant(tz, date, t)))
            .filter(|&due| utc_now >= due && utc_now - due <= grace && self.last_run.is_none_or(|last| last < due))
            .max()
    }

    /// Heures d'impression d'une journée : l'heure choisie, ou chaque heure jusqu'à minuit.
    fn times(&self, first: NaiveTime) -> Vec<NaiveTime> {
        if self.repeat != Some(Repeat::Hourly) {
            return vec![first];
        }
        (first.hour()..24).filter_map(|h| NaiveTime::from_hms_opt(h, first.minute(), 0)).collect()
    }

    /// La planification imprime-t-elle ce jour-là ?
    fn occurs_on(&self, date: NaiveDate) -> bool {
        let weekday = date.weekday().number_from_monday() as u8;
        let on_day = self.days.contains(&weekday);
        let repeat = self.repeat.unwrap_or(Repeat::Weekly);
        if !repeat.needs_start() {
            return on_day;
        }
        let Some(start) = self.start.filter(|&start| date >= start) else {
            return false;
        };
        match repeat {
            Repeat::Never => date == start,
            // Une semaine sur deux, comptée depuis le lundi de la semaine de référence.
            Repeat::Biweekly => {
                let monday = |d: NaiveDate| d - Duration::days(i64::from(d.weekday().num_days_from_monday()));
                on_day && (monday(date) - monday(start)).num_weeks() % 2 == 0
            }
            _ => {
                let every = repeat.months().unwrap_or(1);
                let elapsed = (date.year() - start.year()) * 12 + date.month() as i32 - start.month() as i32;
                // Le 31 devient le dernier jour des mois plus courts, le 29 février le 28 hors bissextiles.
                elapsed % every as i32 == 0 && date.day() == start.day().min(days_in_month(date))
            }
        }
    }
}

/// Nombre de jours du mois de `date`.
fn days_in_month(date: NaiveDate) -> u32 {
    let next = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
    };
    next.and_then(|d| d.pred_opt()).map_or(31, |d| d.day())
}

/// Instant d'une heure locale, changements d'heure compris : une heure sautée au passage à l'heure
/// d'été part une heure plus tard, une heure doublée au passage à l'heure d'hiver part la première fois.
fn local_instant<Tz: TimeZone>(tz: &Tz, date: NaiveDate, time: NaiveTime) -> Option<DateTime<Utc>> {
    let naive = date.and_time(time);
    tz.from_local_datetime(&naive)
        .earliest()
        .or_else(|| tz.from_local_datetime(&(naive + Duration::hours(1))).earliest())
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn temp_store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("printr-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Store::open_at(&dir.join("printr.json")).unwrap()
    }

    #[test]
    fn users_sessions_and_persistence() {
        let mut store = temp_store("users");
        store.add_user("Camille", "secret123").unwrap();
        assert!(store.add_user("camille", "autre1234").is_err());
        let id = store.users()[0].id.clone();
        let hash = store.password_hash(&id).unwrap();
        assert!(!verify_password(&hash, "mauvais"));
        assert!(verify_password(&hash, "secret123"));
        let token = store.open_session(&id);
        assert_eq!(store.session_user(&token).unwrap().name, "Camille");
        store.save().unwrap();

        let reopened = Store::open_at(store.path()).unwrap();
        assert_eq!(reopened.session_user(&token).unwrap().name, "Camille");
        let _ = fs::remove_dir_all(store.path().parent().unwrap());
    }

    #[test]
    fn schedule_fires_once_within_grace() {
        // Mercredi 7 octobre 2026.
        let at = |h, m| Local.with_ymd_and_hms(2026, 10, 7, h, m, 0).unwrap();
        let mut s = Schedule {
            id: "s".into(),
            days: vec![3],
            time: "08:00".into(),
            enabled: true,
            last_run: None,
            repeat: None,
            start: None,
        };
        let grace = Duration::minutes(15);
        assert!(s.due(at(7, 59), grace).is_none());
        assert!(s.due(at(8, 5), grace).is_some());
        assert!(s.due(at(9, 0), grace).is_none(), "trop tard : pas de rattrapage");
        s.last_run = Some(at(8, 1).with_timezone(&Utc));
        assert!(s.due(at(8, 5), grace).is_none(), "déjà imprimé");
        s.days = vec![1, 2];
        s.last_run = None;
        assert!(s.due(at(8, 5), grace).is_none(), "pas le bon jour");
    }

    // --- Périodicités, dans un fuseau fixé (Paris, avec ses changements d'heure). ---

    use chrono_tz::Europe::Paris;

    const GRACE: Duration = Duration::minutes(15);

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn repeating(repeat: Repeat, days: &[u8], time: &str, start: Option<NaiveDate>) -> Schedule {
        Schedule {
            id: "s".into(),
            days: days.to_vec(),
            time: time.into(),
            enabled: true,
            last_run: None,
            repeat: Some(repeat),
            start,
        }
    }

    /// Fait tourner le planificateur minute par minute entre deux dates (incluses) et renvoie
    /// les instants d'impression, en heure de Paris.
    fn run(mut s: Schedule, from: NaiveDate, to: NaiveDate) -> Vec<chrono::NaiveDateTime> {
        let mut printed = Vec::new();
        let mut now = Paris.from_local_datetime(&from.and_hms_opt(0, 0, 0).unwrap()).unwrap().with_timezone(&Utc);
        let end = Paris.from_local_datetime(&to.and_hms_opt(23, 59, 0).unwrap()).unwrap().with_timezone(&Utc);
        while now <= end {
            if let Some(due) = s.due_in(&now.with_timezone(&Paris), GRACE) {
                s.last_run = Some(now);
                printed.push(due.with_timezone(&Paris).naive_local());
            }
            now += Duration::minutes(1);
        }
        printed
    }

    fn days_printed(s: Schedule, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        run(s, from, to).into_iter().map(|t| t.date()).collect()
    }

    #[test]
    fn schedules_without_repeat_keep_working() {
        // Planification enregistrée avant les périodicités : même format, même comportement.
        let json = r#"{"id":"s","days":[1,3],"time":"08:00","enabled":true,"last_run":null}"#;
        let old: Schedule = serde_json::from_str(json).unwrap();
        assert_eq!((old.repeat, old.start), (None, None));
        let weekly = repeating(Repeat::Weekly, &[1, 3], "08:00", None);
        let (from, to) = (date(2026, 10, 1), date(2026, 10, 31));
        assert_eq!(days_printed(old.clone(), from, to), days_printed(weekly, from, to));
        assert_eq!(days_printed(old.clone(), from, to).len(), 8);
        let saved = serde_json::to_value(&old).unwrap();
        assert!(saved.get("repeat").is_none() && saved.get("start").is_none(), "format inchangé");
    }

    #[test]
    fn hourly_prints_every_hour_until_midnight() {
        let s = repeating(Repeat::Hourly, &[3], "20:30", None);
        let hours: Vec<_> = run(s, date(2026, 10, 6), date(2026, 10, 8)).iter().map(|t| t.to_string()).collect();
        assert_eq!(hours, ["2026-10-07 20:30:00", "2026-10-07 21:30:00", "2026-10-07 22:30:00", "2026-10-07 23:30:00"]);
    }

    #[test]
    fn daily_weekdays_and_weekends_follow_their_days() {
        let week = (date(2026, 10, 5), date(2026, 10, 11)); // lundi → dimanche
        let count = |repeat: Repeat| {
            days_printed(repeating(repeat, repeat.fixed_days().unwrap(), "08:00", None), week.0, week.1)
        };
        assert_eq!(count(Repeat::Daily).len(), 7);
        assert_eq!(count(Repeat::Weekdays), (5..=9).map(|d| date(2026, 10, d)).collect::<Vec<_>>());
        assert_eq!(count(Repeat::Weekends), vec![date(2026, 10, 10), date(2026, 10, 11)]);
    }

    #[test]
    fn weekly_prints_on_checked_days() {
        let s = repeating(Repeat::Weekly, &[2], "08:00", None);
        let expected: Vec<_> = [6, 13, 20, 27].map(|d| date(2026, 10, d)).into();
        assert_eq!(days_printed(s, date(2026, 10, 1), date(2026, 10, 31)), expected);
    }

    #[test]
    fn biweekly_counts_weeks_from_the_start_date() {
        // Référence le mercredi 7 octobre : lundis et mercredis, une semaine sur deux, pas avant.
        let s = repeating(Repeat::Biweekly, &[1, 3], "08:00", Some(date(2026, 10, 7)));
        let expected: Vec<_> = [(10, 7), (10, 19), (10, 21), (11, 2), (11, 4)].map(|(m, d)| date(2026, m, d)).into();
        assert_eq!(days_printed(s, date(2026, 10, 1), date(2026, 11, 8)), expected);
    }

    #[test]
    fn monthly_keeps_the_end_of_month() {
        let s = repeating(Repeat::Monthly, &[1], "08:00", Some(date(2026, 1, 31)));
        let expected: Vec<_> =
            [(1, 31), (2, 28), (3, 31), (4, 30), (5, 31), (6, 30)].map(|(m, d)| date(2026, m, d)).into();
        assert_eq!(days_printed(s, date(2026, 1, 1), date(2026, 6, 30)), expected);
    }

    #[test]
    fn quarterly_and_semiannual_keep_their_calendar_reference() {
        let quarterly = repeating(Repeat::Quarterly, &[1], "08:00", Some(date(2026, 11, 30)));
        let expected: Vec<_> = [(2026, 11, 30), (2027, 2, 28), (2027, 5, 30), (2027, 8, 30), (2027, 11, 30)]
            .map(|(y, m, d)| date(y, m, d))
            .into();
        assert_eq!(days_printed(quarterly, date(2026, 11, 1), date(2027, 12, 31)), expected);

        let semiannual = repeating(Repeat::Semiannual, &[1], "08:00", Some(date(2026, 8, 31)));
        let expected: Vec<_> =
            [(2026, 8, 31), (2027, 2, 28), (2027, 8, 31), (2028, 2, 29)].map(|(y, m, d)| date(y, m, d)).into();
        assert_eq!(days_printed(semiannual, date(2026, 8, 1), date(2028, 3, 31)), expected);
    }

    #[test]
    fn yearly_handles_leap_years() {
        let s = repeating(Repeat::Yearly, &[1], "08:00", Some(date(2024, 2, 29)));
        let expected: Vec<_> = [(2024, 2, 29), (2025, 2, 28), (2026, 2, 28), (2027, 2, 28), (2028, 2, 29)]
            .map(|(y, m, d)| date(y, m, d))
            .into();
        assert_eq!(days_printed(s, date(2024, 1, 1), date(2028, 12, 31)), expected);
    }

    #[test]
    fn never_prints_once_on_the_start_date() {
        let s = repeating(Repeat::Never, &[1, 2, 3, 4, 5, 6, 7], "08:00", Some(date(2026, 10, 7)));
        assert_eq!(days_printed(s, date(2026, 10, 1), date(2026, 10, 31)), vec![date(2026, 10, 7)]);
    }

    #[test]
    fn calendar_repeats_need_a_start_date() {
        let s = repeating(Repeat::Monthly, &[1], "08:00", None);
        assert!(days_printed(s, date(2026, 1, 1), date(2026, 3, 31)).is_empty());
    }

    #[test]
    fn daylight_saving_time_does_not_shift_or_skip_prints() {
        // 29 mars 2026 : 2 h 30 n'existe pas, l'impression part à 3 h 30.
        let s = repeating(Repeat::Daily, &[1, 2, 3, 4, 5, 6, 7], "02:30", None);
        let spring: Vec<_> =
            run(s.clone(), date(2026, 3, 28), date(2026, 3, 30)).iter().map(|t| t.to_string()).collect();
        assert_eq!(spring, ["2026-03-28 02:30:00", "2026-03-29 03:30:00", "2026-03-30 02:30:00"]);
        // 25 octobre 2026 : 2 h 30 existe deux fois, une seule impression.
        assert_eq!(run(s, date(2026, 10, 24), date(2026, 10, 26)).len(), 3);
        // Une heure ordinaire reste la même heure locale de part et d'autre du changement.
        let s = repeating(Repeat::Daily, &[1, 2, 3, 4, 5, 6, 7], "08:00", None);
        let times: Vec<_> =
            run(s, date(2026, 10, 24), date(2026, 10, 26)).iter().map(|t| t.time().to_string()).collect();
        assert_eq!(times, ["08:00:00"; 3]);
        // Toutes les heures, le jour du passage à l'heure d'hiver : 1 h 30, 2 h 30 (une fois), 3 h 30.
        let s = repeating(Repeat::Hourly, &[7], "01:30", None);
        assert_eq!(run(s, date(2026, 10, 25), date(2026, 10, 25)).len(), 23);
    }

    #[test]
    fn due_times_follow_the_time_zone() {
        let s = repeating(Repeat::Weekly, &[3], "08:00", None);
        let at = |tz: chrono_tz::Tz, h| tz.with_ymd_and_hms(2026, 10, 7, h, 1, 0).unwrap();
        let paris = s.due_in(&at(Paris, 8), GRACE).unwrap();
        let new_york = s.due_in(&at(chrono_tz::America::New_York, 8), GRACE).unwrap();
        assert_eq!(paris.to_rfc3339(), "2026-10-07T06:00:00+00:00");
        assert_eq!(new_york.to_rfc3339(), "2026-10-07T12:00:00+00:00");
    }

    #[test]
    fn a_print_just_before_midnight_still_leaves_after_midnight() {
        let s = repeating(Repeat::Weekly, &[3], "23:55", None);
        let after = Paris.with_ymd_and_hms(2026, 10, 8, 0, 5, 0).unwrap();
        assert!(s.due_in(&after, GRACE).is_some());
    }

    #[test]
    fn paper_gauge_counts_down_and_warns_when_low() {
        let mut paper = Paper::default();
        assert_eq!((paper.remaining_mm(), paper.percent(), paper.low()), (70_000, 100, false));
        paper.consume(63_500);
        assert_eq!((paper.remaining_mm(), paper.percent(), paper.low()), (6_500, 9, true));
        paper.consume(10_000);
        assert_eq!((paper.remaining_mm(), paper.percent()), (0, 0), "jamais en dessous de zéro");
        paper.new_roll(Some(80_000));
        assert_eq!((paper.roll_mm, paper.used_mm, paper.low()), (80_000, 0, false));
    }

    #[test]
    fn a_whole_roll_measures_the_roll_length() {
        // Rouleau en place à l'installation : on ne sait pas d'où il part, pas de mesure.
        let mut paper = Paper::default();
        paper.consume(30_000);
        paper.ran_out();
        assert_eq!(paper.roll_mm, 70_000);
        // Rouleau posé neuf, vidé jusqu'au bout : sa longueur devient la référence.
        paper.new_roll(None);
        paper.consume(64_200);
        paper.ran_out();
        assert_eq!(paper.roll_mm, 64_200);
        // Mesure invraisemblable (compteur perdu, rouleau entamé) : ignorée.
        paper.new_roll(None);
        paper.consume(1_000);
        paper.ran_out();
        assert_eq!(paper.roll_mm, 64_200);
    }
}
