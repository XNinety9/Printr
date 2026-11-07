//! Vieux parler et gothique, par Vetuste Scripteur (https://github.com/XNinety9/vetuste_scripteur),
//! appelé en ligne de commande, comme Barnum : il renvoie un PNG noir et blanc de la largeur du papier.
//!
//! Commande : `$PRINTR_VETUSTE` (par exemple `python3 /opt/vetuste/vetuste-scripteur.pyz`), sinon
//! `vetuste-scripteur` s'il est dans le PATH, sinon `python3 /opt/vetuste/vetuste-scripteur.pyz`
//! s'il est installé là (install.sh, image Raspberry Pi, scripts/deploy.sh).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Datelike, NaiveDate};

use crate::doc::Doc;
use crate::{i18n, raster};

/// Sur un Pi Zero, Python démarre lentement : bien plus de marge que pour Barnum.
const TIMEOUT: Duration = Duration::from_secs(30);

/// Au-delà, le texte rétrécirait jusqu'à l'illisible (l'image est limitée à 10 000 px de haut).
pub const MAX_CHARS: usize = 2000;

/// Niveau le plus fort : grimoire illisible.
pub const MAX_LEVEL: u8 = 3;

/// Là où l'installation le dépose.
const INSTALLED: &str = "/opt/vetuste/vetuste-scripteur.pyz";

/// Programme trouvé : chemin existant, ou nom présent dans le PATH.
fn found(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// Commande de Vetuste Scripteur, découpée en programme et arguments.
pub fn command() -> Vec<String> {
    let line = match std::env::var("PRINTR_VETUSTE").ok().filter(|c| !c.trim().is_empty()) {
        Some(line) => line,
        None if !found("vetuste-scripteur") && Path::new(INSTALLED).is_file() => format!("python3 {INSTALLED}"),
        None => "vetuste-scripteur".to_owned(),
    };
    line.split_whitespace().map(str::to_owned).collect()
}

/// Vetuste Scripteur est-il installé ? Pour `python3 /opt/…/vetuste-scripteur.pyz`, on vérifie
/// aussi le fichier : sans lui, python3 seul ne suffit pas.
pub fn available() -> bool {
    let command = command();
    let Some(program) = command.first() else { return false };
    found(program) && command.iter().skip(1).filter(|a| a.ends_with(".pyz")).all(|a| found(a))
}

/// Seed du jour : la même toute la journée, pour que l'aperçu montre le ticket qui sortira.
pub fn daily_seed(date: NaiveDate) -> u64 {
    (date.year() as u64) * 10_000 + u64::from(date.month()) * 100 + u64::from(date.day())
}

/// Lance la gravure : `text` transmogrifié, en gothique, dans un PNG noir et blanc de `width` pixels.
/// Avec une `seed`, le même texte redonne la même image. `level` va de 0 (vocabulaire seul) à 3
/// (grimoire illisible) ; 2 par défaut.
pub fn engrave(text: &str, width: u32, seed: Option<u64>, level: Option<u8>) -> Result<Vec<u8>> {
    run(&command(), text, width, seed, level)
}

fn run(command: &[String], text: &str, width: u32, seed: Option<u64>, level: Option<u8>) -> Result<Vec<u8>> {
    let (program, base) =
        command.split_first().context(i18n::t("commande Vetuste Scripteur vide", "empty Vetuste Scripteur command"))?;
    let mut cmd = Command::new(program);
    cmd.args(base).arg("--png").arg(format!("--largeur={width}"));
    if let Some(seed) = seed {
        cmd.arg(format!("--seed={seed}"));
    }
    if let Some(level) = level {
        cmd.arg(format!("--niveau={level}"));
    }
    // Le texte après `--` : venu de l'appli, il ne peut jamais être pris pour une option.
    cmd.arg("--").arg(text);

    let mut child =
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => anyhow!(
                "{}",
                tr!(
                    "Vetuste Scripteur introuvable (« {program} ») : installe-le ou définis PRINTR_VETUSTE",
                    "Vetuste Scripteur not found (\"{program}\"): install it or set PRINTR_VETUSTE"
                )
            ),
            _ => anyhow!(
                "{}",
                tr!("impossible de lancer Vetuste Scripteur : {e}", "could not start Vetuste Scripteur: {e}")
            ),
        })?;

    // Lus dans des fils à part : une image dépasse vite la capacité du tuyau, et le programme
    // resterait bloqué en écriture si on attendait d'abord qu'il se termine.
    let mut stdout = child.stdout.take().context("stdout")?;
    let mut stderr = child.stderr.take().context("stderr")?;
    let out = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{}", i18n::t("Vetuste Scripteur ne répond pas", "Vetuste Scripteur is not responding"));
        }
        thread::sleep(Duration::from_millis(20));
    };

    let interrupted = || anyhow!("{}", i18n::t("lecture interrompue", "read interrupted"));
    let bytes = out.join().map_err(|_| interrupted())??;
    let message = err.join().map_err(|_| interrupted())?;
    if !status.success() {
        let message = String::from_utf8_lossy(&message);
        let last =
            message.lines().rfind(|l| !l.trim().is_empty()).unwrap_or(i18n::t("erreur inconnue", "unknown error"));
        bail!("{}", tr!("Vetuste Scripteur a échoué : {}", "Vetuste Scripteur failed: {}", last.trim()));
    }
    if !bytes.starts_with(b"\x89PNG") {
        bail!("{}", i18n::t("Vetuste Scripteur n'a pas renvoyé d'image", "Vetuste Scripteur returned no image"));
    }
    Ok(bytes)
}

fn check(text: &str, level: u8) -> Result<&str> {
    let text = text.trim();
    anyhow::ensure!(!text.is_empty(), i18n::t("texte vide", "empty text"));
    anyhow::ensure!(
        text.chars().count() <= MAX_CHARS,
        tr!("texte trop long ({MAX_CHARS} caractères au plus)", "text too long ({MAX_CHARS} characters at most)")
    );
    anyhow::ensure!(level <= MAX_LEVEL, tr!("niveau {level} inconnu (0 à 3)", "unknown level {level} (0 to 3)"));
    Ok(text)
}

/// Le texte en vieux parler, gravé en gothique sur toute la largeur imprimable.
pub fn build(text: &str, level: u8, seed: u64) -> Result<Doc> {
    let text = check(text, level)?;
    let png = engrave(text, raster::PRINT_WIDTH, Some(seed), Some(level))?;
    // Déjà en noir et blanc, à la bonne largeur : un simple seuil, sans tramage.
    let image = raster::prepare(&raster::decode(&png)?, raster::Dither::None);
    let mut doc = Doc::new();
    doc.image(image);
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_is_explained() {
        let cmd = vec!["printr-vetuste-introuvable".to_owned()];
        let err = run(&cmd, "Bonjour", 512, None, None).unwrap_err();
        assert!(err.to_string().contains("Vetuste Scripteur introuvable"));
    }

    #[test]
    fn failure_keeps_the_last_error_line() {
        let cmd = ["sh", "-c", "echo 'usage: …' >&2; echo 'erreur : largeur hors limites' >&2; exit 2", "sh"];
        let cmd: Vec<String> = cmd.iter().map(|s| (*s).to_owned()).collect();
        let err = run(&cmd, "Bonjour", 512, None, None).unwrap_err().to_string();
        assert!(err.ends_with("erreur : largeur hors limites"), "{err}");
    }

    #[test]
    fn non_png_output_is_refused() {
        let cmd: Vec<String> = ["sh", "-c", "echo pas une image", "sh"].iter().map(|s| (*s).to_owned()).collect();
        let err = run(&cmd, "Bonjour", 512, None, None).unwrap_err();
        assert!(err.to_string().contains("pas renvoyé d'image"));
    }

    /// Les options passent avant `--`, et le texte, même s'il ressemble à une option, après.
    #[test]
    fn text_always_comes_after_double_dash() {
        // Un faux programme qui renvoie un en-tête PNG suivi de ses arguments.
        let echo = "printf '\\211PNG'; printf '%s|' \"$@\"";
        let cmd: Vec<String> = ["sh", "-c", echo, "sh"].iter().map(|s| (*s).to_owned()).collect();
        let bytes = run(&cmd, "--niveau=0", 512, Some(42), Some(3)).unwrap();
        assert_eq!(&bytes[4..], b"--png|--largeur=512|--seed=42|--niveau=3|--|--niveau=0|");
    }

    #[test]
    fn large_output_does_not_block() {
        // Plus que la capacité d'un tuyau (64 Ko) : lu au fil de l'eau, sinon le programme bloquerait.
        let big = "printf '\\211PNG'; head -c 600000 /dev/zero";
        let cmd: Vec<String> = ["sh", "-c", big, "sh"].iter().map(|s| (*s).to_owned()).collect();
        assert_eq!(run(&cmd, "Bonjour", 512, None, None).unwrap().len(), 600_004);
    }

    #[test]
    fn checks_text_and_level() {
        assert!(check("  ", 2).is_err());
        assert!(check("Bonjour", 4).is_err());
        assert!(check(&"a".repeat(MAX_CHARS + 1), 2).is_err());
        assert_eq!(check("  Bonjour\n", 2).unwrap(), "Bonjour");
    }

    /// La version figée dans install.sh est celle que reprennent deploy.sh et image/build.sh :
    /// le Dockerfile, qui ne peut pas la relire, doit donner la même.
    #[test]
    fn pinned_version_is_the_same_everywhere() {
        let value = |text: &str, key: &str| {
            text.lines().find_map(|l| l.trim().strip_prefix(key)).map(str::to_owned).unwrap_or_default()
        };
        let install = include_str!("../../deploy/install.sh");
        let docker = include_str!("../../deploy/Dockerfile");
        for key in ["VETUSTE_VERSION=", "VETUSTE_SHA256="] {
            let pinned = value(install, key);
            assert!(!pinned.is_empty(), "{key} absent de install.sh");
            assert_eq!(value(docker, &format!("ARG {key}")), pinned, "{key} du Dockerfile");
        }
    }

    #[test]
    fn daily_seed_is_the_date() {
        assert_eq!(daily_seed(NaiveDate::from_ymd_opt(2026, 11, 7).unwrap()), 20261107);
    }
}
