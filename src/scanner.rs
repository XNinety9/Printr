//! Scanner de QR codes USB : chaque code lu imprime un ticket, sans réseau ni jeton.
//!
//! Activé par `PRINTR_SCANNER` (`/dev/ttyACM0`, ou mieux `/dev/serial/by-id/…`, qui ne change pas
//! d'un branchement à l'autre). Le scanner doit être en mode port série (USB-COM, CDC-ACM), pas en
//! émulation de clavier, et finir chaque code par un retour à la ligne. Un code contient :
//! - un ticket JSON sur une seule ligne (`{"blocks":[…]}`), comme `printr print` ;
//! - ou le nom (ou l'identifiant) d'un ticket enregistré et partagé : un code collé sur le frigo
//!   imprime toujours la dernière version du ticket.
//!
//! Mêmes règles que l'API HTTP et MQTT : 60 blocs au plus, quota de l'heure (« Scanner » dans
//! l'historique), file d'attente si l'imprimante est absente. Un code refusé est noté dans
//! l'historique, seul retour possible pour qui scanne. Débranché, le scanner est cherché toutes les
//! 5 secondes.

use std::io::{BufRead, BufReader, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::blocks::Ticket;
use crate::i18n::t;
use crate::server::{self, App};

/// Nom des impressions du scanner dans l'historique ; il compte pour le quota de l'heure.
const BY: &str = "Scanner";
/// Un QR code tient au plus environ 3 Ko ; au-delà, la ligne est refusée.
const MAX_LINE: usize = 8 * 1024;
/// Le même code relu dans ce délai est ignoré : un scanner lit souvent deux fois de suite.
const REPEAT: Duration = Duration::from_secs(3);
const RETRY: Duration = Duration::from_secs(5);

/// Ce que demande un code lu.
enum Scan {
    Ticket(Box<Ticket>),
    Preset(String),
}

fn decode(line: &[u8]) -> Result<Scan> {
    let text = std::str::from_utf8(line)
        .map_err(|_| {
            anyhow::anyhow!(
                "{}",
                t(
                    "code illisible (ASCII attendu, avec \\u00e9 pour les accents)",
                    "unreadable code (ASCII expected, with \\u00e9 for accents)"
                )
            )
        })?
        .trim();
    if text.is_empty() {
        bail!("{}", t("code vide", "empty code"));
    }
    if text.starts_with('{') {
        let ticket = serde_json::from_str(text).context(t("ticket JSON invalide", "invalid ticket JSON"))?;
        return Ok(Scan::Ticket(Box::new(ticket)));
    }
    Ok(Scan::Preset(text.to_owned()))
}

/// Imprime ce que demande un code, ou note le refus dans l'historique.
fn handle(app: &App, line: &[u8]) {
    let scanned = t("Ticket scanné", "Scanned ticket");
    let (ticket, label) = match decode(line) {
        Ok(Scan::Ticket(ticket)) => (*ticket, scanned.to_owned()),
        Ok(Scan::Preset(wanted)) => match app.shared_preset(&wanted) {
            None => {
                let error = tr!("aucun ticket partagé nommé « {wanted} »", "no shared ticket named \"{wanted}\"");
                return refuse(app, scanned, error);
            }
            Some(preset) => match serde_json::from_value::<Ticket>(preset.ticket) {
                Ok(ticket) => (ticket, preset.name),
                Err(e) => {
                    let error = tr!("ticket « {} » invalide : {e}", "invalid ticket \"{}\": {e}", preset.name);
                    return refuse(app, &preset.name, error);
                }
            },
        },
        Err(e) => return refuse(app, scanned, format!("{e:#}")),
    };
    if let Some(error) = server::ticket_too_long(&ticket).or_else(|| app.quota_message(BY)) {
        return refuse(app, &label, error);
    }
    let printed = app.print(&ticket, BY, &label);
    match printed.failure {
        Some(e) => {
            eprintln!("{}", tr!("scanner : « {label} » non imprimé : {e}", "scanner: \"{label}\" not printed: {e}"))
        }
        None => eprintln!("{}", tr!("scanner : « {label} » ({})", "scanner: \"{label}\" ({})", printed.state())),
    }
}

fn refuse(app: &App, label: &str, error: String) {
    eprintln!("{}", tr!("scanner : code refusé : {error}", "scanner: code refused: {error}"));
    app.record_refused(BY, label, error);
}

/// Port série en mode ligne : pas d'écho, CR changé en retour à la ligne, aucun caractère de contrôle
/// interprété. Sans effet sur ce qui n'est pas un terminal (un tube nommé, pour les essais).
fn line_mode(file: &std::fs::File) {
    let fd = file.as_raw_fd();
    // SAFETY : `fd` reste ouvert pendant l'appel, et `termios` est rempli par `tcgetattr`.
    unsafe {
        let mut tio: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut tio) != 0 {
            return;
        }
        tio.c_iflag = libc::ICRNL;
        tio.c_oflag = 0;
        tio.c_cflag = libc::CS8 | libc::CREAD | libc::CLOCAL;
        tio.c_lflag = libc::ICANON;
        libc::cfsetspeed(&mut tio, libc::B115200);
        libc::tcsetattr(fd, libc::TCSANOW, &tio);
    }
}

/// Lit le scanner jusqu'à ce qu'il soit débranché. Seule l'ouverture peut échouer : une erreur de
/// lecture (souvent EIO) veut dire que le scanner a été débranché.
fn listen(app: &App, path: &Path) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOCTTY).open(path)?;
    line_mode(&file);
    eprintln!("{}", tr!("scanner prêt : {}", "scanner ready: {}", path.display()));
    let _ = read_codes(app, BufReader::new(file));
    Ok(())
}

fn read_codes(app: &App, mut reader: BufReader<std::fs::File>) -> std::io::Result<()> {
    let mut last: Option<(Vec<u8>, Instant)> = None;
    loop {
        let mut line = Vec::new();
        if reader.by_ref().take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if line.len() > MAX_LINE {
            // La fin de la ligne trop longue est jetée.
            let mut rest = Vec::new();
            reader.read_until(b'\n', &mut rest)?;
            refuse(app, t("Ticket scanné", "Scanned ticket"), t("code trop long", "code too long").to_owned());
            continue;
        }
        let line = line.trim_ascii().to_vec();
        // Un CR suivi d'un LF donne une ligne vide.
        if line.is_empty() {
            continue;
        }
        if last.as_ref().is_some_and(|(previous, at)| *previous == line && at.elapsed() < REPEAT) {
            continue;
        }
        handle(app, &line);
        last = Some((line, Instant::now()));
    }
}

/// Lance la lecture du scanner si `PRINTR_SCANNER` est défini.
pub fn spawn(app: Arc<App>) {
    let Some(path) = std::env::var("PRINTR_SCANNER").ok().map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()) else {
        return;
    };
    std::thread::spawn(move || {
        // Le journal ne dit qu'une fois que le scanner manque, pas toutes les 5 secondes.
        let mut reported = false;
        loop {
            match listen(&app, Path::new(&path)) {
                Ok(()) => {
                    eprintln!("{}", tr!("scanner débranché : {path}", "scanner unplugged: {path}"));
                    reported = false;
                }
                Err(e) if !reported => {
                    let hint = if e.kind() == std::io::ErrorKind::PermissionDenied {
                        t(
                            " (le compte printr doit être dans le groupe dialout)",
                            " (the printr account must be in the dialout group)",
                        )
                    } else {
                        ""
                    };
                    eprintln!("{}", tr!("scanner absent : {path} : {e}{hint}", "scanner not found: {path}: {e}{hint}"));
                    reported = true;
                }
                Err(_) => {}
            }
            std::thread::sleep(RETRY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_a_ticket_or_a_shared_ticket() {
        let scan = decode(br#"{"cut":true,"blocks":[{"type":"title","text":"Hello"},{"type":"date"}]}"#).unwrap();
        assert!(matches!(scan, Scan::Ticket(t) if t.blocks.len() == 2));
        assert!(matches!(decode(b"  Matin\r").unwrap(), Scan::Preset(name) if name == "Matin"));
        assert!(decode(br#"{"blocks":"#).is_err());
        assert!(decode(b" \r").is_err());
        assert!(decode(b"gr\xfcn").is_err(), "pas de l'UTF-8");
    }
}
