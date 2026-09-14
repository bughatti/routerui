//! Guest network — an isolated network for visitors.
//!
//! A guest network is an isolated /24 (its own VLAN sub-interface + DHCP) whose
//! clients can reach the internet and the router's DNS/DHCP but not the primary
//! LAN, other VLANs, or each other. Wired isolation between guest devices can't
//! be enforced by the router alone, so when a Wi-Fi radio with hostapd is
//! present we also emit a guest SSID with `ap_isolate=1` (station-to-station
//! traffic blocked at the AP). The subnet-level isolation is done with the same
//! firewall approach as an isolated VLAN.

use crate::api::netutil;
use crate::api::vlan::Vlan;
use crate::mock;
use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::fs;

const GUEST_FILE: &str = "/opt/routerui/config/guest.json";
const GUEST_VLAN_ID: u16 = 90;
const GUEST_SUBNET: &str = "192.168.90";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestConfig {
    pub enabled: bool,
    /// Optional Wi-Fi SSID for the guest network (empty = wired/VLAN only).
    #[serde(default)]
    pub ssid: String,
    /// WPA2 passphrase for the guest SSID (8-63 chars). Never returned to the UI.
    #[serde(default)]
    pub passphrase: String,
}

impl Default for GuestConfig {
    fn default() -> Self {
        GuestConfig { enabled: false, ssid: String::new(), passphrase: String::new() }
    }
}

fn load() -> GuestConfig {
    fs::read_to_string(GUEST_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(cfg: &GuestConfig) -> Result<(), String> {
    fs::create_dir_all("/opt/routerui/config").map_err(|e| e.to_string())?;
    fs::write(GUEST_FILE, serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    // Config can hold a passphrase, so keep it private.
    let _ = std::process::Command::new("sudo")
        .args(["chmod", "600", GUEST_FILE])
        .output();
    Ok(())
}

fn guest_vlan() -> Vlan {
    Vlan {
        id: GUEST_VLAN_ID,
        name: "Guest".into(),
        subnet: GUEST_SUBNET.into(),
        dhcp_start: format!("{}.100", GUEST_SUBNET),
        dhcp_end: format!("{}.200", GUEST_SUBNET),
        isolated: true,
    }
}

/// Re-apply the guest network on boot if enabled.
pub fn reapply_on_boot() {
    let cfg = load();
    if !cfg.enabled {
        return;
    }
    let lan = netutil::lan_iface();
    if !lan.is_empty() {
        let _ = crate::api::vlan::apply_public(&lan, &guest_vlan());
        let _ = std::process::Command::new("sudo").args(["systemctl", "reload", "dnsmasq"]).output();
    }
}

// ============ HANDLERS ============

pub async fn status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "enabled": false, "ssid": "", "has_wifi": false })));
    }
    let cfg = load();
    let has_wifi = !netutil::lan_iface().is_empty()
        && which("hostapd");
    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "ssid": cfg.ssid,
        "subnet": format!("{}.0/24", GUEST_SUBNET),
        "has_passphrase": !cfg.passphrase.is_empty(),
        "has_wifi": has_wifi,
    })))
}

fn which(bin: &str) -> bool {
    std::process::Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Deserialize)]
pub struct SetGuest {
    pub enabled: bool,
    #[serde(default)]
    pub ssid: String,
    #[serde(default)]
    pub passphrase: String,
}

pub async fn set_config(
    _user: crate::api::AuthUser,
    Json(payload): Json<SetGuest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !payload.ssid.is_empty() {
        if payload.ssid.len() > 32 {
            return Err((StatusCode::BAD_REQUEST, "SSID too long".into()));
        }
        // A passphrase is required with an SSID unless one was already stored.
        let existing = load();
        let pass = if payload.passphrase.is_empty() { existing.passphrase.clone() } else { payload.passphrase.clone() };
        if pass.len() < 8 || pass.len() > 63 {
            return Err((StatusCode::BAD_REQUEST, "Wi-Fi passphrase must be 8-63 characters".into()));
        }
    }
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let mut cfg = load();
    cfg.enabled = payload.enabled;
    if !payload.ssid.is_empty() {
        cfg.ssid = payload.ssid.clone();
    }
    if !payload.passphrase.is_empty() {
        cfg.passphrase = payload.passphrase.clone();
    }

    let lan = netutil::lan_iface();
    if lan.is_empty() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "no LAN interface detected".into()));
    }

    if cfg.enabled {
        crate::api::vlan::apply_public(&lan, &guest_vlan())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    } else {
        crate::api::vlan::teardown_public(&lan, &guest_vlan());
    }
    let _ = std::process::Command::new("sudo").args(["systemctl", "reload", "dnsmasq"]).output();

    save(&cfg).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({ "success": true, "enabled": cfg.enabled })))
}
