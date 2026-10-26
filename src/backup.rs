//! Sauvegarde et restauration de tout ce que la famille a fait dans l'appli : comptes, tickets,
//! planifications, historique, courses, anniversaires, réglages et photos, dans un seul fichier
//! JSON. Les sessions et la file d'attente de l'imprimante restent propres à la machine.
//!
//! Avant une restauration, l'état actuel est gardé dans `backups/` (les cinq derniers) : une
//! restauration ratée se rattrape.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::i18n::t;
use crate::store::{Data, Store};

/// Version du format, pour pouvoir le faire évoluer.
const FORMAT: u32 = 1;
/// Copies gardées avant restauration.
const KEEP: usize = 5;

#[derive(Serialize, Deserialize)]
struct Backup {
    printr_backup: u32,
    version: String,
    created: chrono::DateTime<chrono::Utc>,
    data: Value,
    /// Photos envoyées depuis l'appli : nom du fichier (`<id>.png`) → contenu en base64.
    #[serde(default)]
    images: std::collections::BTreeMap<String, String>,
}

/// Ce qu'apporte une restauration, pour le dire à l'appli.
pub struct Restored {
    pub users: usize,
    pub presets: usize,
    pub images: usize,
    /// Les réglages changent : un redémarrage les applique.
    pub settings_changed: bool,
}

/// Un nom de photo tel que Printr les crée : des lettres et des chiffres, puis `.png`.
fn image_name_ok(name: &str) -> bool {
    name.strip_suffix(".png")
        .is_some_and(|id| !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Les photos, à côté du fichier de données (comme `store::uploads_dir`).
fn uploads(store: &Store) -> PathBuf {
    store.path().parent().unwrap_or(Path::new(".")).join("images")
}

/// Le fichier de sauvegarde de l'état actuel.
pub fn export(store: &Store) -> Result<Vec<u8>> {
    let mut images = std::collections::BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(uploads(store)) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if image_name_ok(&name)
                && let Ok(bytes) = std::fs::read(entry.path())
            {
                images.insert(name, STANDARD.encode(bytes));
            }
        }
    }
    let backup = Backup {
        printr_backup: FORMAT,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        created: chrono::Utc::now(),
        data: store.backup_data()?,
        images,
    };
    Ok(serde_json::to_vec(&backup)?)
}

/// Une photo de la sauvegarde : son nom de fichier et son contenu.
type Image = (String, Vec<u8>);

/// Lit et vérifie un fichier de sauvegarde, sans rien toucher.
fn parse(bytes: &[u8]) -> Result<(Data, Vec<Image>)> {
    let backup: Backup = serde_json::from_slice(bytes).map_err(|_| {
        anyhow::anyhow!("{}", t("ce fichier n'est pas une sauvegarde de Printr", "this file is not a Printr backup"))
    })?;
    if backup.printr_backup != FORMAT {
        bail!(
            "{}",
            t("sauvegarde d'un format inconnu (Printr plus récent ?)", "backup in an unknown format (newer Printr?)")
        );
    }
    let data: Data = serde_json::from_value(backup.data)
        .map_err(|e| anyhow::anyhow!("{}", tr!("sauvegarde illisible : {e}", "unreadable backup: {e}")))?;
    if data.users_count() == 0 {
        bail!("{}", t("la sauvegarde ne contient aucun compte", "the backup has no account"));
    }
    let mut images = Vec::new();
    for (name, content) in backup.images {
        if !image_name_ok(&name) {
            bail!("{}", tr!("photo au nom invalide : {name}", "photo with an invalid name: {name}"));
        }
        let bytes = STANDARD
            .decode(content.as_bytes())
            .map_err(|_| anyhow::anyhow!("{}", tr!("photo illisible : {name}", "unreadable photo: {name}")))?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            bail!("{}", tr!("photo illisible : {name}", "unreadable photo: {name}"));
        }
        images.push((name, bytes));
    }
    Ok((data, images))
}

/// Garde une copie de l'état actuel dans `backups/`, et seulement les plus récentes.
fn keep_current(store: &Store) -> Result<PathBuf> {
    let dir = store.path().parent().unwrap_or(Path::new(".")).join("backups");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("avant-restauration-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&path, export(store)?)?;
    let mut old: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("avant-restauration-")))
        .collect();
    old.sort();
    for path in old.iter().rev().skip(KEEP) {
        let _ = std::fs::remove_file(path);
    }
    Ok(path)
}

/// Remplace les données par celles de la sauvegarde (l'état actuel est gardé avant).
pub fn restore(store: &mut Store, bytes: &[u8]) -> Result<Restored> {
    let (data, images) = parse(bytes)?;
    keep_current(store)?;
    let dir = uploads(store);
    std::fs::create_dir_all(&dir)?;
    for (name, bytes) in &images {
        std::fs::write(dir.join(name), bytes)?;
    }
    let settings_changed = store.data.settings != data.settings;
    let restored =
        Restored { users: data.users_count(), presets: data.presets.len(), images: images.len(), settings_changed };
    store.restore(data);
    store.save()?;
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_names_are_checked() {
        assert!(image_name_ok("a1B2c3.png"));
        assert!(!image_name_ok("../printr.json"));
        assert!(!image_name_ok("a/b.png"));
        assert!(!image_name_ok(".png"));
        assert!(!image_name_ok("photo.jpg"));
    }

    #[test]
    fn backups_are_checked() {
        let empty = serde_json::json!({
            "printr_backup": 1, "version": "2.0.0", "created": "2026-10-26T10:00:00Z", "data": {}
        });
        assert!(parse(empty.to_string().as_bytes()).is_err(), "sans compte");
        assert!(parse(b"{\"users\": []}").is_err(), "pas une sauvegarde");
        let future = serde_json::json!({
            "printr_backup": 99, "version": "9.0.0", "created": "2026-10-26T10:00:00Z", "data": {}
        });
        assert!(parse(future.to_string().as_bytes()).is_err(), "format inconnu");
    }

    #[test]
    fn export_then_restore_keeps_everything() {
        let dir = std::env::temp_dir().join(format!("printr-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = Store::open_at(&dir.join("printr.json")).unwrap();
        store.add_user("Camille", "demo1234").unwrap();
        store.data.family_birthdays = "Mamie, 02/01/1950".into();
        let bytes = export(&store).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("\"sessions\""), "pas de sessions");

        let mut other = Store::open_at(&dir.join("autre.json")).unwrap();
        let restored = restore(&mut other, &bytes).unwrap();
        assert_eq!(restored.users, 1);
        assert_eq!(other.users()[0].name, "Camille");
        assert_eq!(other.data.family_birthdays, "Mamie, 02/01/1950");
        assert!(dir.join("backups").read_dir().unwrap().count() == 1, "l'état d'avant est gardé");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
