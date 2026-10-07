//! Cache disque : réponses de Claude du jour, historique des mots du jour.
//!
//! Emplacement : `$PRINTR_CACHE_DIR`, sinon `$CACHE_DIRECTORY` (fourni par systemd avec
//! `CacheDirectory=`), sinon `$XDG_CACHE_HOME/printr`, sinon `~/.cache/printr`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Les réponses plus anciennes sont supprimées à l'ouverture du cache.
const MAX_AGE: Duration = Duration::from_secs(14 * 24 * 3600);

pub struct Cache {
    dir: PathBuf,
}

fn default_dir() -> Option<PathBuf> {
    let env = |name| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    env("PRINTR_CACHE_DIR")
        .or_else(|| env("CACHE_DIRECTORY"))
        .or_else(|| env("XDG_CACHE_HOME").map(|d| d.join("printr")))
        .or_else(|| env("HOME").map(|d| d.join(".cache/printr")))
}

/// Nom de fichier sûr à partir d'une clé libre.
fn file_name(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

impl Cache {
    pub fn open() -> Option<Self> {
        let dir = default_dir()?;
        fs::create_dir_all(dir.join("claude")).ok()?;
        let cache = Self { dir };
        cache.prune();
        Some(cache)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn prune(&self) {
        let Ok(entries) = fs::read_dir(self.dir.join("claude")) else { return };
        let now = SystemTime::now();
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| now.duration_since(t).unwrap_or_default() > MAX_AGE);
            if old {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn entry(&self, key: &str) -> PathBuf {
        self.dir.join("claude").join(format!("{}.json", file_name(key)))
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_str(&fs::read_to_string(self.entry(key)).ok()?).ok()
    }

    /// Écriture au mieux : un cache qui échoue ne doit pas empêcher d'imprimer.
    pub fn put<T: Serialize>(&self, key: &str, value: &T) {
        if let Ok(json) = serde_json::to_string_pretty(value) {
            let _ = fs::write(self.entry(key), json);
        }
    }

    /// Dernières lignes d'un fichier d'historique (les plus récentes à la fin).
    pub fn history(&self, name: &str, keep: usize) -> Vec<String> {
        let text = fs::read_to_string(self.dir.join(name)).unwrap_or_default();
        let lines: Vec<String> = text.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect();
        lines[lines.len().saturating_sub(keep)..].to_vec()
    }

    pub fn remember(&self, name: &str, line: &str, keep: usize) {
        let mut lines = self.history(name, keep.saturating_sub(1));
        lines.push(line.to_owned());
        let _ = fs::write(self.dir.join(name), lines.join("\n") + "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache(name: &str) -> Cache {
        let dir = std::env::temp_dir().join(format!("printr-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("claude")).unwrap();
        Cache { dir }
    }

    #[test]
    fn roundtrip_and_history() {
        let cache = temp_cache("roundtrip");
        assert_eq!(cache.get::<String>("2026-10-07/horoscope lion"), None);
        cache.put("2026-10-07/horoscope lion", &"bonjour".to_owned());
        assert_eq!(cache.get::<String>("2026-10-07/horoscope lion").as_deref(), Some("bonjour"));

        for word in ["a", "b", "c", "d"] {
            cache.remember("mots.txt", word, 3);
        }
        assert_eq!(cache.history("mots.txt", 10), vec!["b", "c", "d"]);
        let _ = fs::remove_dir_all(cache.dir());
    }
}
