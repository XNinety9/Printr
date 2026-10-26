//! Recherche des imprimantes pour l'assistant de premier lancement : les imprimantes USB (pilote
//! usblp, et le lien `/dev/tm88` de la règle udev), et les imprimantes réseau qui écoutent sur le
//! port 9100 (impression brute) dans le réseau local de la machine.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};

use crate::output::Target;

/// Port de l'impression brute (« JetDirect »), celui des imprimantes de tickets en réseau.
const RAW_PORT: u16 = 9100;
/// Délai de connexion par adresse, et nombre d'essais menés en même temps.
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);
const PROBES_AT_ONCE: usize = 64;

/// Les imprimantes USB présentes, le lien stable `/dev/tm88` d'abord.
pub fn usb() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let link = PathBuf::from("/dev/tm88");
    if link.exists() {
        found.push(link);
    }
    if let Ok(entries) = std::fs::read_dir("/dev/usb") {
        let mut lp: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("lp")))
            .collect();
        lp.sort();
        found.extend(lp);
    }
    found
}

/// L'adresse IPv4 de la machine sur le réseau local (celle qui sortirait vers Internet), sans rien
/// envoyer : une socket UDP « connectée » choisit seulement son adresse.
pub fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?; // TEST-NET-1 : aucune donnée n'est envoyée
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if ip.is_private() => Some(ip),
        _ => None,
    }
}

/// Les adresses du réseau local qui répondent sur le port 9100 (un /24 autour de la machine).
pub fn network() -> Vec<String> {
    let Some(me) = local_ipv4() else { return Vec::new() };
    let [a, b, c, _] = me.octets();
    let candidates: Vec<Ipv4Addr> = (1..=254).map(|d| Ipv4Addr::new(a, b, c, d)).filter(|ip| *ip != me).collect();
    let mut found = Vec::new();
    for chunk in candidates.chunks(PROBES_AT_ONCE) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|&ip| {
                std::thread::spawn(move || {
                    TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(ip), RAW_PORT), PROBE_TIMEOUT)
                        .is_ok()
                        .then(|| format!("{ip}:{RAW_PORT}"))
                })
            })
            .collect();
        found.extend(handles.into_iter().filter_map(|h| h.join().ok().flatten()));
    }
    found
}

/// Ce que l'assistant affiche : les imprimantes USB avec leur état, puis celles du réseau.
pub fn printers() -> Value {
    let usb: Vec<Value> = usb()
        .into_iter()
        .map(|path| {
            let status = Target::Device(path.clone()).status().map(|s| s.code());
            json!({ "device": path.display().to_string(), "status": status })
        })
        .collect();
    let network: Vec<Value> = network().into_iter().map(|addr| json!({ "tcp": addr })).collect();
    json!({ "usb": usb, "network": network })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_address_is_private_or_absent() {
        // Sur une machine sans réseau, rien ; sinon une adresse privée.
        if let Some(ip) = local_ipv4() {
            assert!(ip.is_private());
        }
    }
}
