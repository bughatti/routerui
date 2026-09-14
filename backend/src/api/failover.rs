//! Dual-WAN failover.
//!
//! Monitors the primary WAN uplink and, when it goes down, moves the default
//! route (and NAT) to a backup uplink; it fails back when the primary recovers.
//! Reachability is tested by pinging a target *through a specific interface*, so
//! a link that is up but has no internet still counts as down.
//!
//! This needs two WAN interfaces (e.g. the built-in NIC + a USB/LTE dongle).
//! With a single uplink the feature stays idle. `run_monitor_once()` is driven
//! by a periodic task in main; `reapply_on_boot()` restores NAT for whichever
//! link is active.

use crate::mock;
use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::process::Command;

const FAILOVER_FILE: &str = "/opt/routerui/config/failover.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverConfig {
    pub enabled: bool,
    pub primary_iface: String,
    pub backup_iface: String,
    #[serde(default = "default_target")]
    pub ping_target: String,
}

fn default_target() -> String {
    "1.1.1.1".into()
}

impl Default for FailoverConfig {
    fn default() -> Self {
        FailoverConfig {
            enabled: false,
            primary_iface: String::new(),
            backup_iface: String::new(),
            ping_target: default_target(),
        }
    }
}

fn load() -> FailoverConfig {
    fs::read_to_string(FAILOVER_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(cfg: &FailoverConfig) -> Result<(), String> {
    fs::create_dir_all("/opt/routerui/config").map_err(|e| e.to_string())?;
    fs::write(FAILOVER_FILE, serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn sudo(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("sudo").args(args).output()
}

/// True if `target` is reachable via `iface` (2 pings, 1s timeout each).
fn reachable_via(iface: &str, target: &str) -> bool {
    Command::new("ping")
        .args(["-I", iface, "-c", "2", "-W", "1", "-n", target])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The gateway currently associated with an interface, learned from the routing
/// table (DHCP installs a per-link route). Empty if none is known.
fn gateway_of(iface: &str) -> String {
    if let Ok(o) = Command::new("ip").args(["route", "show", "dev", iface]).output() {
        let s = String::from_utf8_lossy(&o.stdout);
        for line in s.lines() {
            if line.starts_with("default") {
                if let Some(gw) = line.split_whitespace().skip_while(|w| *w != "via").nth(1) {
                    return gw.to_string();
                }
            }
        }
        // Fall back to the subnet's .1 if we can find the link's own address.
    }
    String::new()
}

/// The interface currently carrying the default route.
fn active_iface() -> String {
    crate::api::netutil::wan_iface()
}

/// Point the default route (and MASQUERADE) at `iface`. Idempotent.
fn switch_to(iface: &str) -> Result<(), String> {
    let gw = gateway_of(iface);
    if gw.is_empty() {
        // No via-gateway known; use a link-scoped default (works for PPP/USB).
        let out = sudo(&["ip", "route", "replace", "default", "dev", iface]).map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("route replace: {}", String::from_utf8_lossy(&out.stderr)));
        }
    } else {
        let out = sudo(&["ip", "route", "replace", "default", "via", &gw, "dev", iface]).map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(format!("route replace: {}", String::from_utf8_lossy(&out.stderr)));
        }
    }
    // Ensure NAT masquerades out the active uplink (harmless if already there).
    let _ = sudo(&["iptables", "-t", "nat", "-C", "POSTROUTING", "-o", iface, "-j", "MASQUERADE"])
        .map(|o| {
            if !o.status.success() {
                let _ = sudo(&["iptables", "-t", "nat", "-A", "POSTROUTING", "-o", iface, "-j", "MASQUERADE"]);
            }
        });
    Ok(())
}

/// One monitor pass. Called on a timer from main.
pub async fn run_monitor_once() -> Result<(), String> {
    let cfg = load();
    if !cfg.enabled || cfg.primary_iface.is_empty() || cfg.backup_iface.is_empty() {
        return Ok(());
    }
    let primary_ok = reachable_via(&cfg.primary_iface, &cfg.ping_target);
    let active = active_iface();

    if primary_ok {
        // Prefer the primary whenever it is healthy.
        if active != cfg.primary_iface {
            switch_to(&cfg.primary_iface)?;
        }
        return Ok(());
    }
    // Primary is down — fail over to backup if it can reach the internet.
    if reachable_via(&cfg.backup_iface, &cfg.ping_target) && active != cfg.backup_iface {
        switch_to(&cfg.backup_iface)?;
    }
    Ok(())
}

/// Restore NAT for the active uplink on boot.
pub fn reapply_on_boot() {
    let cfg = load();
    if !cfg.enabled {
        return;
    }
    let active = active_iface();
    if !active.is_empty() {
        let _ = switch_to(&active);
    }
}

// ============ HANDLERS ============

pub async fn status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "enabled": false, "primary_iface": "", "backup_iface": "",
            "active_iface": "eth0", "primary_up": true, "backup_up": false
        })));
    }
    let cfg = load();
    let primary_up = if cfg.primary_iface.is_empty() { false } else { reachable_via(&cfg.primary_iface, &cfg.ping_target) };
    let backup_up = if cfg.backup_iface.is_empty() { false } else { reachable_via(&cfg.backup_iface, &cfg.ping_target) };
    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "primary_iface": cfg.primary_iface,
        "backup_iface": cfg.backup_iface,
        "ping_target": cfg.ping_target,
        "active_iface": active_iface(),
        "primary_up": primary_up,
        "backup_up": backup_up,
    })))
}

pub async fn set_config(
    _user: crate::api::AuthUser,
    Json(cfg): Json<FailoverConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Interface names must look like real NICs (no shell metacharacters).
    let ok_iface = |s: &str| !s.is_empty() && s.len() <= 20
        && s.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@');
    if cfg.enabled {
        if !ok_iface(&cfg.primary_iface) || !ok_iface(&cfg.backup_iface) {
            return Err((StatusCode::BAD_REQUEST, "valid primary and backup interfaces are required".into()));
        }
        if cfg.primary_iface == cfg.backup_iface {
            return Err((StatusCode::BAD_REQUEST, "primary and backup must differ".into()));
        }
        if !crate::validate::is_ipv4(&cfg.ping_target) {
            return Err((StatusCode::BAD_REQUEST, "ping target must be an IPv4 address".into()));
        }
    }
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }
    save(&cfg).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    // Apply immediately so a freshly-enabled config takes effect now.
    let _ = run_monitor_once().await;
    Ok(Json(serde_json::json!({ "success": true })))
}
