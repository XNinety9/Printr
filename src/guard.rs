//! Garde-fous du serveur exposé sur Internet : essais de mot de passe limités, adresse réelle du
//! client derrière le reverse proxy, requêtes venues d'un autre site refusées.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::net;

/// Essais ratés tolérés par adresse IP, sur la fenêtre `WINDOW`.
const MAX_FAILURES: usize = 5;
const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Pause après un essai raté : ralentit les essais en série.
const PENALTY: Duration = Duration::from_millis(600);

#[derive(Default)]
pub struct LoginGuard {
    failures: Mutex<HashMap<IpAddr, Vec<Instant>>>,
    /// Une vérification Argon2 à la fois : chacune prend ~19 Mo, et des essais en parallèle
    /// suffiraient sinon à épuiser la mémoire du Pi.
    gate: Mutex<()>,
}

impl LoginGuard {
    /// `Err(attente)` si cette adresse a trop raté récemment.
    pub fn check(&self, ip: IpAddr) -> Result<(), Duration> {
        let mut failures = self.failures.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        // On oublie les essais trop anciens (et les adresses qui n'ont plus rien en cours).
        failures.retain(|_, list| {
            list.retain(|t| now.duration_since(*t) < WINDOW);
            !list.is_empty()
        });
        match failures.get(&ip) {
            Some(list) if list.len() >= MAX_FAILURES => Err(WINDOW.saturating_sub(now.duration_since(list[0]))),
            _ => Ok(()),
        }
    }

    pub fn verify(&self, hash: &str, password: &str) -> bool {
        let _one_at_a_time = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        crate::store::verify_password(hash, password)
    }

    pub fn failed(&self, ip: IpAddr) {
        self.failures.lock().unwrap_or_else(|e| e.into_inner()).entry(ip).or_default().push(Instant::now());
        std::thread::sleep(PENALTY);
    }

    pub fn succeeded(&self, ip: IpAddr) {
        self.failures.lock().unwrap_or_else(|e| e.into_inner()).remove(&ip);
    }
}

/// Adresse du client : celle de la connexion, ou, si la connexion vient du réseau local (le
/// reverse proxy), la dernière adresse de `X-Forwarded-For`, celle qu'a ajoutée le proxy.
pub fn client_ip(remote: Option<IpAddr>, forwarded_for: Option<&str>) -> Option<IpAddr> {
    let remote = remote?;
    if net::is_public(remote) {
        return Some(remote);
    }
    let forwarded = forwarded_for.and_then(|h| h.rsplit(',').next()).and_then(|ip| ip.trim().parse().ok());
    Some(forwarded.unwrap_or(remote))
}

/// La requête vient-elle d'une page de l'appli elle-même ? Un navigateur envoie `Origin` sur
/// toute requête qui modifie quelque chose ; sans `Origin` (curl, script), rien à vérifier.
/// Protège des sous-domaines voisins (*.x99.fr) que le cookie `SameSite=Lax` laisse passer.
pub fn same_origin(origin: Option<&str>, host: Option<&str>) -> bool {
    let Some(origin) = origin else { return true };
    let origin_host = origin.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    host.is_some_and(|host| origin_host.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_block_after_the_limit() {
        let guard = LoginGuard::default();
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        for _ in 0..MAX_FAILURES {
            assert!(guard.check(ip).is_ok());
            guard.failures.lock().unwrap().entry(ip).or_default().push(Instant::now());
        }
        assert!(guard.check(ip).is_err());
        assert!(guard.check("203.0.113.8".parse().unwrap()).is_ok());
        guard.succeeded(ip);
        assert!(guard.check(ip).is_ok());
    }

    #[test]
    fn forwarded_for_is_trusted_only_from_the_local_network() {
        let proxy: IpAddr = "192.168.1.200".parse().unwrap();
        let outside: IpAddr = "8.8.8.8".parse().unwrap();
        assert_eq!(client_ip(Some(proxy), Some("1.2.3.4, 9.9.9.9")), Some("9.9.9.9".parse().unwrap()));
        assert_eq!(client_ip(Some(outside), Some("10.0.0.1")), Some(outside));
        assert_eq!(client_ip(Some(proxy), None), Some(proxy));
    }

    #[test]
    fn origin_must_match_host() {
        assert!(same_origin(None, Some("printr.x99.fr")));
        assert!(same_origin(Some("https://printr.x99.fr"), Some("printr.x99.fr")));
        assert!(same_origin(Some("http://printer.local:8080"), Some("printer.local:8080")));
        assert!(!same_origin(Some("https://files.x99.fr"), Some("printr.x99.fr")));
        assert!(!same_origin(Some("null"), Some("printr.x99.fr")));
    }
}
