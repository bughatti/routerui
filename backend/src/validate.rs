//! Validators for values that end up in system config files (dnsmasq, hostapd)
//! or command arguments. A router config file is line- and comma-delimited, so
//! an unvalidated value containing a newline or comma could inject additional
//! directives — e.g. a dnsmasq `dhcp-script=` that runs as root. Every network
//! field taken from a request is checked here before use.

use std::net::Ipv4Addr;
use std::str::FromStr;

/// A single IPv4 address, e.g. 192.168.1.10.
pub fn is_ipv4(s: &str) -> bool {
    Ipv4Addr::from_str(s).is_ok()
}

/// IPv4 CIDR, e.g. 192.168.1.0/24. Also accepts a bare address.
pub fn is_ipv4_cidr(s: &str) -> bool {
    match s.split_once('/') {
        Some((ip, pfx)) => is_ipv4(ip) && pfx.parse::<u8>().map(|p| p <= 32).unwrap_or(false),
        None => is_ipv4(s),
    }
}

/// A MAC address: six colon- or dash-separated hex octets.
pub fn is_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(|c| c == ':' || c == '-').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A DNS hostname/label: letters, digits, hyphen and dot; 1..=253 chars; no
/// leading/trailing dot or hyphen. Critically rejects commas, spaces and any
/// control character, which is what prevents config injection.
pub fn is_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    if s.starts_with('.') || s.ends_with('.') || s.starts_with('-') || s.ends_with('-') {
        return false;
    }
    s.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

/// A dnsmasq lease time: a number optionally suffixed s/m/h/d, or "infinite".
pub fn is_lease_time(s: &str) -> bool {
    if s == "infinite" {
        return true;
    }
    let (num, unit) = match s.chars().last() {
        Some(u) if matches!(u, 's' | 'm' | 'h' | 'd') => (&s[..s.len() - 1], true),
        _ => (s, false),
    };
    let _ = unit;
    !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
}

/// A network interface name: short, alphanumeric plus . _ - @ (VLAN/alt names).
pub fn is_iface(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'))
}

/// A Wi-Fi SSID: 1..=32 chars, printable, no control characters or newlines.
pub fn is_ssid(s: &str) -> bool {
    let n = s.chars().count();
    n >= 1 && n <= 32 && s.chars().all(|c| !c.is_control())
}

/// A WPA passphrase: 8..=63 printable ASCII characters (the WPA-PSK range).
pub fn is_wifi_passphrase(s: &str) -> bool {
    let n = s.len();
    n >= 8 && n <= 63 && s.chars().all(|c| c.is_ascii_graphic() || c == ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4() {
        assert!(is_ipv4("192.168.1.10"));
        assert!(!is_ipv4("192.168.1.256"));
        assert!(!is_ipv4("1.2.3"));
        assert!(!is_ipv4("192.168.1.10 evil"));
    }

    #[test]
    fn mac() {
        assert!(is_mac("aa:bb:cc:dd:ee:ff"));
        assert!(is_mac("AA-BB-CC-DD-EE-FF"));
        assert!(!is_mac("aa:bb:cc:dd:ee"));
        assert!(!is_mac("zz:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn hostname_rejects_injection() {
        assert!(is_hostname("printer"));
        assert!(is_hostname("nas.local"));
        // The attack this guards against: a newline/comma smuggling a dnsmasq
        // directive into the config file.
        assert!(!is_hostname("x\ndhcp-script=/tmp/evil"));
        assert!(!is_hostname("a,b"));
        assert!(!is_hostname("has space"));
        assert!(!is_hostname(""));
    }

    #[test]
    fn lease_time() {
        for ok in ["12h", "3600", "30m", "infinite", "1d"] {
            assert!(is_lease_time(ok), "{ok}");
        }
        for bad in ["12h\nx", "abc", ""] {
            assert!(!is_lease_time(bad), "{bad}");
        }
    }

    #[test]
    fn wifi_and_ssid() {
        assert!(is_ssid("HomeNet"));
        assert!(!is_ssid("bad\nssid"));
        assert!(!is_ssid(&"x".repeat(33)));
        assert!(is_wifi_passphrase("goodpass1"));
        assert!(!is_wifi_passphrase("short"));
        assert!(!is_wifi_passphrase("has\nnewline-in-it-padding"));
    }

    #[test]
    fn cidr_and_iface() {
        assert!(is_ipv4_cidr("192.168.10.0/24"));
        assert!(is_ipv4_cidr("10.0.0.1"));
        assert!(!is_ipv4_cidr("192.168.10.0/40"));
        assert!(is_iface("enp2s0"));
        assert!(is_iface("eth0.20"));
        assert!(!is_iface("eth0; rm -rf"));
    }
}
