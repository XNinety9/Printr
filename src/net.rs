//! Accès réseau des blocs (météo, actualités, agenda, images…).
//!
//! Depuis l'appli web ou un script, on ne fait confiance qu'aux adresses publiques : un compte ne
//! doit pas pouvoir faire lire à la machine la box, le homelab ou un service local (SSRF). Le
//! filtre est dans le résolveur DNS, donc il s'applique à chaque connexion, redirections et
//! changements de DNS compris. En ligne de commande, tout reste permis.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

const TIMEOUT: Duration = Duration::from_secs(20);

/// Client HTTP des blocs ; `public_only` refuse toute adresse non publique.
pub fn agent(public_only: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).build();
    if public_only {
        ureq::Agent::with_parts(config, DefaultConnector::new(), PublicOnly { local: local_prefixes() })
    } else {
        config.into()
    }
}

/// Préfixes IPv6 publics de la machine (/64) : ses voisins du réseau local ont des adresses
/// « publiques » dans ce préfixe, à refuser comme les adresses privées.
fn local_prefixes() -> Vec<Ipv6Addr> {
    let Ok(text) = std::fs::read_to_string("/proc/net/if_inet6") else { return Vec::new() };
    text.lines()
        .filter_map(|line| u128::from_str_radix(line.split_whitespace().next()?, 16).ok())
        .map(Ipv6Addr::from)
        .filter(|ip| is_public(IpAddr::V6(*ip)))
        .map(prefix64)
        .collect()
}

fn prefix64(ip: Ipv6Addr) -> Ipv6Addr {
    Ipv6Addr::from(u128::from(ip) & !((1u128 << 64) - 1))
}

/// Adresse joignable sur Internet, hors plages privées, locales, réservées ou de documentation.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            let s = v6.segments();
            let nat64 = s[0] == 0x64 && s[1] == 0xff9b; // 64:ff9b::/96 : une adresse IPv4 déguisée
            !(v6.is_unspecified()
                || v6.is_loopback()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // fc00::/7, adresses locales uniques
                || (s[0] & 0xffc0) == 0xfe80 // fe80::/10, lien local
                || (s[0] & 0xffc0) == 0xfec0 // fec0::/10, site local (obsolète)
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (nat64 && !is_public_v4(Ipv4Addr::from((u128::from(v6) & 0xffff_ffff) as u32))))
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || (a == 100 && (64..128).contains(&b)) // 100.64.0.0/10, NAT des opérateurs
        || (a == 192 && b == 0 && c == 0) // 192.0.0.0/24, réservé
        || (a == 198 && (b == 18 || b == 19)) // 198.18.0.0/15, tests de performance
        || a >= 240) // 240.0.0.0/4, réservé
}

#[derive(Debug)]
struct PublicOnly {
    local: Vec<Ipv6Addr>,
}

impl PublicOnly {
    fn allows(&self, ip: IpAddr) -> bool {
        is_public(ip)
            && match ip {
                IpAddr::V6(v6) => !self.local.contains(&prefix64(v6)),
                IpAddr::V4(_) => true,
            }
    }
}

impl Resolver for PublicOnly {
    fn resolve(&self, uri: &Uri, config: &ureq::config::Config, timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let all = DefaultResolver::default().resolve(uri, config, timeout)?;
        let mut allowed = self.empty();
        allowed.truncate(0);
        for addr in all.iter().filter(|a| self.allows(a.ip())) {
            allowed.push(*addr);
        }
        if allowed.is_empty() {
            let host = uri.host().unwrap_or("?");
            return Err(ureq::Error::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{host} : adresse du réseau local, refusée depuis l'appli"),
            )));
        }
        Ok(allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn private_and_reserved_addresses_are_refused() {
        for s in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.200", "169.254.169.254", "0.0.0.0", "100.64.0.1",
            "::1", "fe80::1", "fd00::1", "::ffff:192.168.1.1", "64:ff9b::c0a8:0101", "2001:db8::1",
        ] {
            assert!(!is_public(ip(s)), "{s} devrait être refusée");
        }
        for s in ["82.67.33.234", "1.1.1.1", "2a01:e0a:eaf:8950::1", "64:ff9b::0101:0101"] {
            assert!(is_public(ip(s)), "{s} devrait être acceptée");
        }
    }

    #[test]
    fn neighbours_in_the_local_ipv6_prefix_are_refused() {
        let r = PublicOnly { local: vec![prefix64("2a01:e0a:eaf:8950::".parse().unwrap())] };
        assert!(!r.allows(ip("2a01:e0a:eaf:8950:96c6:91ff:fea2:c610")));
        assert!(r.allows(ip("2a01:e0a:eaf:9999::1")));
    }

    #[test]
    fn public_only_agent_refuses_local_urls() {
        let err = agent(true).get("http://127.0.0.1:9/").call().unwrap_err();
        assert!(err.to_string().contains("refusée"), "{err}");
    }
}
