//! VLAN / multiple-network support (UniFi-style "networks").
//!
//! Each VLAN is an 802.1q sub-interface on the LAN NIC (`<lan>.<id>`) with its
//! own /24 subnet, the router at `.1`, a dedicated dnsmasq DHCP range, and
//! firewall rules. Forwarded traffic reaches the internet through the existing
//! WAN MASQUERADE rule, so no per-VLAN NAT is needed. An "isolated" VLAN can
//! reach the internet and the router's DNS/DHCP but not the primary LAN or any
//! other VLAN (guest-style segmentation).
//!
//! Config is persisted to VLANS_FILE and re-applied on boot by
//! `reapply_on_boot()` (called from main), so VLANs survive a reboot without a
//! separate netplan file.

use crate::api::netutil;
use crate::mock;
use crate::validate;
use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

const VLANS_FILE: &str = "/opt/routerui/config/vlans.json";
const DNSMASQ_DIR: &str = "/etc/dnsmasq.d";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vlan {
    pub id: u16,
    pub name: String,
    /// First three octets of the /24, e.g. "192.168.20".
    pub subnet: String,
    pub dhcp_start: String,
    pub dhcp_end: String,
    /// When true the VLAN is walled off from the primary LAN and other VLANs.
    pub isolated: bool,
}

fn load_vlans() -> Vec<Vlan> {
    fs::read_to_string(VLANS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_vlans(vlans: &[Vlan]) -> Result<(), String> {
    fs::create_dir_all("/opt/routerui/config").map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(vlans).map_err(|e| e.to_string())?;
    fs::write(VLANS_FILE, json).map_err(|e| e.to_string())
}

fn sudo(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("sudo").args(args).output()
}

/// Reject a VLAN whose id/name/subnet is malformed. Returns Ok on success.
fn validate_vlan(v: &Vlan) -> Result<(), String> {
    if v.id < 1 || v.id > 4094 {
        return Err("VLAN id must be 1-4094".into());
    }
    if v.name.is_empty() || v.name.len() > 32 || !v.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ') {
        return Err("invalid VLAN name".into());
    }
    // subnet is the first three octets; validate as an address ending in .0
    if !validate::is_ipv4(&format!("{}.0", v.subnet)) {
        return Err("invalid subnet (expected three octets like 192.168.20)".into());
    }
    if !validate::is_ipv4(&v.dhcp_start) || !validate::is_ipv4(&v.dhcp_end) {
        return Err("invalid DHCP range".into());
    }
    Ok(())
}

fn dnsmasq_conf_path(id: u16) -> String {
    format!("{}/vlan-{}.conf", DNSMASQ_DIR, id)
}

/// Bring one VLAN interface up and write its DHCP config. Idempotent: deleting a
/// pre-existing link first keeps re-apply clean.
fn apply_vlan(lan: &str, v: &Vlan) -> Result<(), String> {
    let ifname = format!("{}.{}", lan, v.id);
    let router_ip = format!("{}.1", v.subnet);

    // (Re)create the 802.1q sub-interface.
    let _ = sudo(&["ip", "link", "del", &ifname]); // ignore if absent
    let out = sudo(&["ip", "link", "add", "link", lan, "name", &ifname, "type", "vlan", "id", &v.id.to_string()])
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("ip link add: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let _ = sudo(&["ip", "addr", "add", &format!("{}/24", router_ip), "dev", &ifname]);
    let _ = sudo(&["ip", "link", "set", &ifname, "up"]);

    // Per-VLAN dnsmasq DHCP/DNS.
    let conf = format!(
        "# RouterUI VLAN {id} ({name}) - managed, do not edit\n\
         interface={ifname}\n\
         dhcp-range={start},{end},255.255.255.0,12h\n\
         dhcp-option={ifname},option:router,{router}\n\
         dhcp-option={ifname},option:dns-server,{router}\n",
        id = v.id, name = v.name, ifname = ifname,
        start = v.dhcp_start, end = v.dhcp_end, router = router_ip,
    );
    fs::write(dnsmasq_conf_path(v.id), conf).map_err(|e| e.to_string())?;

    apply_vlan_firewall(lan, v)?;
    Ok(())
}

/// Firewall rules for a VLAN. Tagged in the comment so removal is precise.
fn apply_vlan_firewall(lan: &str, v: &Vlan) -> Result<(), String> {
    let ifname = format!("{}.{}", lan, v.id);
    let wan = netutil::wan_iface();
    // Clean any stale rules for this iface first (idempotent re-apply).
    remove_vlan_firewall(lan, v);

    // Always: this VLAN may reach the internet.
    if !wan.is_empty() {
        let _ = sudo(&["iptables", "-A", "FORWARD", "-i", &ifname, "-o", &wan, "-j", "ACCEPT"]);
    }
    // Router services this VLAN's clients need: DNS + DHCP only.
    let _ = sudo(&["iptables", "-A", "INPUT", "-i", &ifname, "-p", "udp", "-m", "multiport", "--dports", "53,67", "-j", "ACCEPT"]);
    let _ = sudo(&["iptables", "-A", "INPUT", "-i", &ifname, "-p", "tcp", "--dport", "53", "-j", "ACCEPT"]);

    if v.isolated {
        // Block this VLAN from every RFC1918 destination (primary LAN + other
        // VLANs); established replies are already accepted by the global rule.
        for net in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
            let _ = sudo(&["iptables", "-A", "FORWARD", "-i", &ifname, "-d", net, "-j", "DROP"]);
        }
    } else {
        // Non-isolated VLAN may also talk to the primary LAN.
        let _ = sudo(&["iptables", "-A", "FORWARD", "-i", &ifname, "-o", lan, "-j", "ACCEPT"]);
    }
    Ok(())
}

fn remove_vlan_firewall(lan: &str, v: &Vlan) {
    let ifname = format!("{}.{}", lan, v.id);
    let wan = netutil::wan_iface();
    if !wan.is_empty() {
        let _ = sudo(&["iptables", "-D", "FORWARD", "-i", &ifname, "-o", &wan, "-j", "ACCEPT"]);
    }
    let _ = sudo(&["iptables", "-D", "INPUT", "-i", &ifname, "-p", "udp", "-m", "multiport", "--dports", "53,67", "-j", "ACCEPT"]);
    let _ = sudo(&["iptables", "-D", "INPUT", "-i", &ifname, "-p", "tcp", "--dport", "53", "-j", "ACCEPT"]);
    for net in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
        let _ = sudo(&["iptables", "-D", "FORWARD", "-i", &ifname, "-d", net, "-j", "DROP"]);
    }
    let _ = sudo(&["iptables", "-D", "FORWARD", "-i", &ifname, "-o", lan, "-j", "ACCEPT"]);
}

fn teardown_vlan(lan: &str, v: &Vlan) {
    remove_vlan_firewall(lan, v);
    let ifname = format!("{}.{}", lan, v.id);
    let _ = sudo(&["ip", "link", "del", &ifname]);
    let _ = fs::remove_file(dnsmasq_conf_path(v.id));
}

fn reload_dnsmasq() {
    let _ = sudo(&["systemctl", "reload", "dnsmasq"]);
}

/// Public wrappers so the guest-network module can reuse the VLAN primitives
/// without duplicating the interface/DHCP/firewall logic.
pub fn apply_public(lan: &str, v: &Vlan) -> Result<(), String> {
    apply_vlan(lan, v)
}
pub fn teardown_public(lan: &str, v: &Vlan) {
    teardown_vlan(lan, v)
}

/// Re-create every configured VLAN. Called on service startup so VLANs persist
/// across reboots without a netplan file.
pub fn reapply_on_boot() {
    let vlans = load_vlans();
    if vlans.is_empty() {
        return;
    }
    let lan = netutil::lan_iface();
    if lan.is_empty() {
        return;
    }
    for v in &vlans {
        let _ = apply_vlan(&lan, v);
    }
    reload_dnsmasq();
}

// ============ HANDLERS ============

pub async fn list() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "vlans": [], "lan_interface": "eth1" })));
    }
    Ok(Json(serde_json::json!({
        "vlans": load_vlans(),
        "lan_interface": netutil::lan_iface(),
    })))
}

pub async fn add(
    _user: crate::api::AuthUser,
    Json(v): Json<Vlan>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    validate_vlan(&v).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let mut vlans = load_vlans();
    if vlans.iter().any(|x| x.id == v.id) {
        return Err((StatusCode::CONFLICT, format!("VLAN {} already exists", v.id)));
    }
    if vlans.iter().any(|x| x.subnet == v.subnet) {
        return Err((StatusCode::CONFLICT, format!("subnet {}.0/24 already in use", v.subnet)));
    }

    let lan = netutil::lan_iface();
    if lan.is_empty() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "no LAN interface detected".into()));
    }
    apply_vlan(&lan, &v).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    vlans.push(v);
    save_vlans(&vlans).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    reload_dnsmasq();
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Deserialize)]
pub struct RemoveVlan {
    pub id: u16,
}

pub async fn remove(
    _user: crate::api::AuthUser,
    Json(payload): Json<RemoveVlan>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }
    let mut vlans = load_vlans();
    let Some(pos) = vlans.iter().position(|x| x.id == payload.id) else {
        return Err((StatusCode::NOT_FOUND, "VLAN not found".into()));
    };
    let lan = netutil::lan_iface();
    let v = vlans.remove(pos);
    teardown_vlan(&lan, &v);
    save_vlans(&vlans).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    reload_dnsmasq();
    Ok(Json(serde_json::json!({ "success": true })))
}
