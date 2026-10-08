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

use anyhow::{bail, Context, Result};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Duration, Local, Utc};
use rand::RngExt;
use serde::{Deserialize, Serialize};

/// Nombre d'entrées conservées dans l'historique.
const HISTORY_MAX: usize = 300;
/// Durée de validité d'une session.
const SESSION_DAYS: i64 = 180;
/// Couleurs attribuées aux utilisateurs, dans l'ordre de création.
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
}

/// Lit les données sur le disque, sans rien créer : pour les blocs (liste de courses, bilan).
pub fn read_data() -> Result<Data> {
    let dir = data_dir().context("répertoire de données introuvable (définir PRINTR_DATA_DIR)")?;
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
        let dir = data_dir().context("répertoire de données introuvable (définir PRINTR_DATA_DIR)")?;
        let images = dir.join("images");
        if !images.is_dir() {
            fs::create_dir_all(&images).with_context(|| format!("impossible de créer {}", images.display()))?;
            #[cfg(unix)]
            share_with_group(&images, 0o2770);
        }
        Self::open_at(&dir.join("printr.json"))
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let data = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).with_context(|| format!("{} illisible", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(e).with_context(|| format!("impossible de lire {}", path.display())),
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

    // ---- Utilisateurs ----

    pub fn users(&self) -> &[User] {
        &self.data.users
    }

    pub fn user(&self, id: &str) -> Option<&User> {
        self.data.users.iter().find(|u| u.id == id)
    }

    fn find_by_name(&self, name: &str) -> Option<&User> {
        self.data.users.iter().find(|u| u.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Crée un compte ; renvoie son identifiant.
    pub fn add_user(&mut self, name: &str, password: &str) -> Result<String> {
        let name = name.trim();
        if name.is_empty() {
            bail!("le prénom est vide");
        }
        if self.find_by_name(name).is_some() {
            bail!("{name} existe déjà");
        }
        let color = COLORS[self.data.users.len() % COLORS.len()].to_owned();
        let password_hash = hash(password)?;
        let id = new_id(8);
        let admin = self.data.users.is_empty();
        self.data.users.push(User { id: id.clone(), name: name.to_owned(), color, password_hash, admin });
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
        let user = user.with_context(|| format!("utilisateur inconnu : {name}"))?;
        user.password_hash = hash;
        let id = user.id.clone();
        self.data.sessions.retain(|s| s.user_id != id);
        Ok(())
    }

    pub fn remove_user(&mut self, name: &str) -> Result<()> {
        let id = self.find_by_name(name).map(|u| u.id.clone()).with_context(|| format!("utilisateur inconnu : {name}"))?;
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
        bail!("mot de passe trop court ({MIN_PASSWORD} caractères minimum)");
    }
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hachage impossible : {e}"))
}

impl Schedule {
    /// Prochaine échéance passée non encore exécutée, à moins de `grace` minutes.
    /// Renvoie l'heure d'échéance si la planification doit partir maintenant.
    pub fn due(&self, now: DateTime<Local>, grace: Duration) -> Option<DateTime<Utc>> {
        if !self.enabled {
            return None;
        }
        let (h, m) = self.time.split_once(':')?;
        let time = chrono::NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)?;
        let today = now.date_naive();
        let weekday = chrono::Datelike::weekday(&today).number_from_monday() as u8;
        if !self.days.contains(&weekday) {
            return None;
        }
        let due = today.and_time(time).and_local_timezone(Local).single()?.with_timezone(&Utc);
        let now = now.with_timezone(&Utc);
        let pending = self.last_run.is_none_or(|last| last < due);
        (now >= due && now - due <= grace && pending).then_some(due)
    }
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
        let mut s = Schedule { id: "s".into(), days: vec![3], time: "08:00".into(), enabled: true, last_run: None };
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
}
