//! Shared network-interface detection helpers.
//!
//! Several modules need to know which NIC faces the WAN (the uplink carrying
//! the default route) and which faces the LAN. The setup wizard persists the
//! operator's choice in the `setup_config` table, but most handlers run without
//! a database handle, so these helpers detect at runtime from the live routing
//! table and address layout. This keeps us from hardcoding a specific NIC name
//! (e.g. `enp1s0`), which only ever matched the author's own hardware.

use std::process::Command;

/// The interface carrying the default route (the WAN uplink). Falls back to an
/// empty string if no default route is present.
pub fn wan_iface() -> String {
    let out = Command::new("ip").args(["route", "show", "default"]).output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(dev) = s.split_whitespace().skip_while(|w| *w != "dev").nth(1) {
            return dev.to_string();
        }
    }
    String::new()
}

/// The IPv4 address bound to the WAN interface, or an empty string.
pub fn wan_ip() -> String {
    let iface = wan_iface();
    if iface.is_empty() {
        return String::new();
    }
    let out = Command::new("ip")
        .args(["-4", "-o", "addr", "show", "dev", &iface])
        .output();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(cidr) = s.split_whitespace().skip_while(|w| *w != "inet").nth(1) {
            return cidr.split('/').next().unwrap_or("").to_string();
        }
    }
    String::new()
}

/// Best-effort LAN interface: the first non-loopback, "up" interface that is
/// not the WAN uplink. A bridge (`br0`) is preferred when present, matching how
/// the setup wizard bridges the LAN + Wi-Fi.
pub fn lan_iface() -> String {
    let wan = wan_iface();
    let out = Command::new("ip").args(["-o", "link", "show"]).output();
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(o) = out {
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            // format: "3: enp2s0: <BROADCAST,...> mtu ..."
            if let Some(name) = line.split(':').nth(1).map(|s| s.trim().to_string()) {
                let name = name.split('@').next().unwrap_or(&name).to_string();
                if name == "lo" || name == wan || name.is_empty() {
                    continue;
                }
                if name.starts_with("wg")
                    || name.starts_with("ifb")
                    || name.starts_with("veth")
                    || name.starts_with("docker")
                    || name.starts_with("virbr")
                    || name.starts_with("tailscale")
                {
                    continue;
                }
                candidates.push(name);
            }
        }
    }
    if let Some(br) = candidates.iter().find(|c| c.starts_with("br")) {
        return br.clone();
    }
    candidates.into_iter().next().unwrap_or_default()
}
