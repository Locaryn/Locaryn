//! L'adresse par laquelle un appareil du même réseau joint cette machine.
//!
//! Le procédé d'avant — l'adresse de la route vers Internet — tombe sur le
//! VPN dès qu'un VPN plein tunnel est actif : le QR donnait `172.16.0.42`
//! (OpenVPN), et un téléphone sur le Wi-Fi de la maison n'y arrivait jamais.
//! Les cartes de Hyper-V, WSL ou VMware ont le même défaut. On choisit donc
//! parmi les cartes de la machine celle du vrai réseau local : adresse
//! privée, passerelle, Wi-Fi ou Ethernet, et rien de virtuel.

use std::net::Ipv4Addr;

/// Ce qui compte d'une carte réseau pour ce choix.
#[derive(Debug, Clone)]
pub struct Carte {
    /// Nom, nom lisible et description, mis bout à bout.
    pub noms: String,
    pub ip: Ipv4Addr,
    /// Une passerelle par défaut : la carte mène quelque part.
    pub passerelle: bool,
    /// Wi-Fi ou Ethernet, d'après le système.
    pub wifi_ou_ethernet: bool,
}

/// Mots qui trahissent une carte virtuelle ou un tunnel.
const VIRTUELLES: &[&str] = &[
    "vpn",
    "openvpn",
    "tap-",
    "tap ",
    "tun",
    "wireguard",
    "wintun",
    "tailscale",
    "zerotier",
    "hamachi",
    "vethernet",
    "hyper-v",
    "wsl",
    "vmware",
    "vmnet",
    "virtualbox",
    "vboxnet",
    "docker",
    "bluetooth",
    "loopback",
    "utun",
    "bridge",
];

fn virtuelle(noms: &str) -> bool {
    let n = noms.to_lowercase();
    VIRTUELLES.iter().any(|m| n.contains(m))
}

/// La note d'une carte, ou `None` si elle ne convient pas du tout.
fn note(c: &Carte) -> Option<u32> {
    if !c.ip.is_private() || c.ip.is_loopback() || c.ip.is_link_local() || virtuelle(&c.noms) {
        return None;
    }
    let mut n = 0;
    if c.passerelle {
        n += 8;
    }
    if c.wifi_ou_ethernet {
        n += 4;
    }
    // À égalité, les plages des box domestiques d'abord.
    n += match c.ip.octets() {
        [192, 168, ..] => 2,
        [10, ..] => 1,
        _ => 0,
    };
    Some(n)
}

/// La meilleure carte, à note égale la première rencontrée.
pub fn choisir(cartes: &[Carte]) -> Option<Ipv4Addr> {
    let mut meilleure: Option<(u32, Ipv4Addr)> = None;
    for c in cartes {
        if let Some(n) = note(c) {
            if meilleure.is_none_or(|(m, _)| n > m) {
                meilleure = Some((n, c.ip));
            }
        }
    }
    meilleure.map(|(_, ip)| ip)
}

/// Les cartes actives de cette machine.
fn cartes() -> Vec<Carte> {
    netdev::get_interfaces()
        .into_iter()
        .filter(|i| i.is_up())
        .flat_map(|i| {
            let noms = format!(
                "{} {} {}",
                i.name,
                i.friendly_name.clone().unwrap_or_default(),
                i.description.clone().unwrap_or_default()
            );
            let passerelle = i.gateway.is_some();
            let wifi_ou_ethernet = matches!(
                i.if_type,
                netdev::interface::InterfaceType::Ethernet
                    | netdev::interface::InterfaceType::Wireless80211
            );
            i.ipv4
                .iter()
                .map(|net| Carte {
                    noms: noms.clone(),
                    ip: net.addr(),
                    passerelle,
                    wifi_ou_ethernet,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// L'adresse du réseau local de cette machine, si une carte convient.
pub fn adresse() -> Option<Ipv4Addr> {
    choisir(&cartes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carte(noms: &str, ip: [u8; 4], passerelle: bool, wifi: bool) -> Carte {
        Carte {
            noms: noms.into(),
            ip: Ipv4Addr::from(ip),
            passerelle,
            wifi_ou_ethernet: wifi,
        }
    }

    /// Le cas du 12/11 : OpenVPN tient la route, le Wi-Fi doit gagner.
    #[test]
    fn le_wifi_passe_devant_le_vpn_et_les_cartes_virtuelles() {
        let cartes = [
            carte("OpenVPN Connect DCO Adapter", [172, 16, 0, 42], true, false),
            carte(
                "vEthernet (WSL (Hyper-V firewall))",
                [172, 24, 192, 1],
                false,
                true,
            ),
            carte(
                "VMware Network Adapter VMnet8",
                [192, 168, 233, 1],
                false,
                true,
            ),
            carte("Wi-Fi MediaTek Wi-Fi 6", [192, 168, 0, 32], true, true),
        ];
        assert_eq!(choisir(&cartes), Some(Ipv4Addr::new(192, 168, 0, 32)));
    }

    #[test]
    fn une_carte_avec_passerelle_passe_devant_une_carte_sans() {
        let cartes = [
            carte("Ethernet 2", [10, 0, 0, 5], false, true),
            carte("Ethernet", [10, 1, 0, 7], true, true),
        ];
        assert_eq!(choisir(&cartes), Some(Ipv4Addr::new(10, 1, 0, 7)));
    }

    #[test]
    fn rien_de_convenable_ne_rend_rien() {
        let cartes = [
            carte("Loopback", [127, 0, 0, 1], false, false),
            carte("Wi-Fi", [8, 8, 8, 8], true, true),
            carte("tailscale0", [100, 64, 0, 1], true, false),
        ];
        assert_eq!(choisir(&cartes), None);
    }
}
