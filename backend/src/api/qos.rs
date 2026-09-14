// Per-client QoS / bandwidth limiting (UniFi-style "rate limit") plus an
// optional global Smart Queue that fights bufferbloat with fq_codel.
//
// Model (the router forwards, so we shape on egress of the two interfaces):
//   * A LAN client's DOWNLOAD is traffic leaving the router toward the client,
//     i.e. egress on the LAN interface with dst = client IP.
//   * A LAN client's UPLOAD is traffic leaving the router toward the internet,
//     i.e. egress on the WAN interface with src = client IP.
// We therefore build an HTB tree on BOTH interfaces (download limits on LAN,
// upload limits on WAN) and attach fq_codel as the leaf qdisc so latency stays
// low even inside a rate-limited class. No IFB device is needed with this
// dual-egress model.
//
// Config source of truth: /opt/routerui/config/qos.json.

use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;
use crate::validate;

const QOS_CONF: &str = "/opt/routerui/config/qos.json";
const MAX_MBIT: u32 = 10000; // clamp ceiling (10 Gbit)

type ApiErr = (StatusCode, String);
fn err500<E: std::fmt::Display>(e: E) -> ApiErr {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn bad(msg: &str) -> ApiErr {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QosClient {
    pub ip: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub down_mbps: u32,
    #[serde(default)]
    pub up_mbps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QosConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub smart_queue: bool,
    #[serde(default)]
    pub clients: Vec<QosClient>,
}

// ── config persistence ──────────────────────────────────────────────────────

fn read_cfg() -> QosConfig {
    std::fs::read_to_string(QOS_CONF)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_cfg(cfg: &QosConfig) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(QOS_CONF).parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(QOS_CONF, json).map_err(|e| e.to_string())
}

// ── interface detection ─────────────────────────────────────────────────────

fn sh(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(cmd).args(args).output()
}

/// The WAN interface = the one carrying the default route.
fn wan_iface() -> String {
    sh("ip", &["route", "show", "default"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .and_then(|s| {
            s.split_whitespace()
                .skip_while(|w| *w != "dev")
                .nth(1)
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

/// The LAN interface = the first UP interface holding a private (RFC1918) IPv4
/// that is not the WAN, loopback, or a virtual/container interface. Falls back
/// to the conventional names used elsewhere in the codebase.
fn lan_iface() -> String {
    let wan = wan_iface();
    let out = match sh("ip", &["-j", "-4", "addr", "show"]) {
        Ok(o) => o,
        Err(_) => return "br0".to_string(),
    };
    let json_str = String::from_utf8_lossy(&out.stdout);
    if let Ok(ifaces) = serde_json::from_str::<Vec<serde_json::Value>>(&json_str) {
        for iface in &ifaces {
            let name = iface["ifname"].as_str().unwrap_or("");
            if name.is_empty()
                || name == wan
                || name == "lo"
                || name.starts_with("docker")
                || name.starts_with("br-")
                || name.starts_with("veth")
                || name.starts_with("wg")
                || name.starts_with("tailscale")
                || name.starts_with("ifb")
            {
                continue;
            }
            if let Some(addrs) = iface["addr_info"].as_array() {
                for a in addrs {
                    if let Some(ip) = a["local"].as_str() {
                        if ip.starts_with("10.")
                            || ip.starts_with("192.168.")
                            || (ip.starts_with("172.")
                                && ip
                                    .split('.')
                                    .nth(1)
                                    .and_then(|o| o.parse::<u8>().ok())
                                    .map(|o| (16..=31).contains(&o))
                                    .unwrap_or(false))
                        {
                            return name.to_string();
                        }
                    }
                }
            }
        }
    }
    "br0".to_string()
}

// ── tc plumbing ─────────────────────────────────────────────────────────────

fn tc_run(args: &[&str]) -> Result<(), String> {
    let out = Command::new("sudo")
        .arg("tc")
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(())
}

/// Delete a root qdisc, ignoring the "does not exist" error so clearing is
/// idempotent.
fn tc_del_root(iface: &str) {
    let _ = Command::new("sudo")
        .args(["tc", "qdisc", "del", "dev", iface, "root"])
        .output();
}

/// True if our shaping (htb, cake, or fq_codel) is currently attached to `iface`.
fn qdisc_applied(iface: &str) -> bool {
    Command::new("sudo")
        .args(["tc", "qdisc", "show", "dev", iface])
        .output()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            s.contains("htb") || s.contains("cake") || s.contains("fq_codel")
        })
        .unwrap_or(false)
}

fn clear_qos() {
    let lan = lan_iface();
    let wan = wan_iface();
    if !lan.is_empty() {
        tc_del_root(&lan);
    }
    if !wan.is_empty() && wan != lan {
        tc_del_root(&wan);
    }
}

/// Build the HTB tree on one interface. `entries` are (client_ip, rate_mbit)
/// pairs already validated and clamped. `field` is "dst" (download, on LAN) or
/// "src" (upload, on WAN).
fn build_iface(
    iface: &str,
    entries: &[(String, u32)],
    field: &str,
    smart_queue: bool,
) -> Result<(), String> {
    if entries.is_empty() {
        // No per-client limits on this direction. Still fight bufferbloat with a
        // CAKE root when Smart Queue is on (CAKE = shaping-aware fq + AQM, the
        // modern best-in-class home queue; unlimited mode still de-bloats).
        if smart_queue {
            tc_run(&["qdisc", "add", "dev", iface, "root", "handle", "1:", "cake"])?;
        }
        return Ok(());
    }

    tc_run(&[
        "qdisc", "add", "dev", iface, "root", "handle", "1:", "htb", "default", "999",
    ])?;
    tc_run(&[
        "class", "add", "dev", iface, "parent", "1:", "classid", "1:1", "htb", "rate", "10000mbit",
    ])?;
    // Unclassified traffic falls into 1:999 at full line rate, with CAKE as the
    // leaf so it also benefits from latency control.
    tc_run(&[
        "class", "add", "dev", iface, "parent", "1:1", "classid", "1:999", "htb", "rate",
        "10000mbit", "ceil", "10000mbit",
    ])?;
    tc_run(&[
        "qdisc", "add", "dev", iface, "parent", "1:999", "handle", "999:", "cake",
    ])?;

    for (i, (ip, rate)) in entries.iter().enumerate() {
        let cid = 10 + i as u32; // unique class minor per client
        let rate_s = format!("{}mbit", rate);
        let classid = format!("1:{}", cid);
        let handle = format!("{}:", cid);
        let match_ip = format!("{}/32", ip);

        tc_run(&[
            "class", "add", "dev", iface, "parent", "1:1", "classid", &classid, "htb", "rate",
            &rate_s, "ceil", &rate_s,
        ])?;
        tc_run(&[
            "qdisc", "add", "dev", iface, "parent", &classid, "handle", &handle, "cake",
        ])?;
        tc_run(&[
            "filter", "add", "dev", iface, "protocol", "ip", "parent", "1:", "prio", "1", "u32",
            "match", "ip", field, &match_ip, "flowid", &classid,
        ])?;
    }
    Ok(())
}

/// Tear down any existing shaping and rebuild from `cfg`.
fn apply_qos(cfg: &QosConfig) -> Result<(), String> {
    clear_qos();
    if !cfg.enabled {
        return Ok(());
    }
    let lan = lan_iface();
    let wan = wan_iface();
    if lan.is_empty() || wan.is_empty() {
        return Err("could not detect LAN/WAN interface".to_string());
    }

    // Download limits shape LAN egress (match dst = client).
    let down: Vec<(String, u32)> = cfg
        .clients
        .iter()
        .filter(|c| c.down_mbps > 0 && validate::is_ipv4(&c.ip))
        .map(|c| (c.ip.clone(), c.down_mbps.clamp(1, MAX_MBIT)))
        .collect();
    // Upload limits shape WAN egress (match src = client).
    let up: Vec<(String, u32)> = cfg
        .clients
        .iter()
        .filter(|c| c.up_mbps > 0 && validate::is_ipv4(&c.ip))
        .map(|c| (c.ip.clone(), c.up_mbps.clamp(1, MAX_MBIT)))
        .collect();

    if let Err(e) = build_iface(&lan, &down, "dst", cfg.smart_queue) {
        clear_qos();
        return Err(format!("LAN ({}) shaping failed: {}", lan, e));
    }
    if let Err(e) = build_iface(&wan, &up, "src", cfg.smart_queue) {
        clear_qos();
        return Err(format!("WAN ({}) shaping failed: {}", wan, e));
    }
    Ok(())
}

// ── handlers ────────────────────────────────────────────────────────────────

pub async fn status() -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "enabled": false, "smart_queue": false, "clients": [],
            "applied": false, "lan_iface": "br0", "wan_iface": "eth0", "mock": true
        })));
    }
    let cfg = read_cfg();
    let lan = lan_iface();
    let wan = wan_iface();
    let applied = qdisc_applied(&lan) || qdisc_applied(&wan);
    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "smart_queue": cfg.smart_queue,
        "clients": cfg.clients,
        "applied": applied,
        "lan_iface": lan,
        "wan_iface": wan,
    })))
}

pub async fn get_config() -> Result<Json<QosConfig>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(QosConfig::default()));
    }
    Ok(Json(read_cfg()))
}

pub async fn set_config(Json(mut cfg): Json<QosConfig>) -> Result<Json<serde_json::Value>, ApiErr> {
    // Validate and clamp before anything touches tc.
    for c in &mut cfg.clients {
        if !validate::is_ipv4(&c.ip) {
            return Err(bad(&format!("invalid client IP: {}", c.ip)));
        }
        if c.name.len() > 64 {
            c.name.truncate(64);
        }
        if c.down_mbps > MAX_MBIT {
            c.down_mbps = MAX_MBIT;
        }
        if c.up_mbps > MAX_MBIT {
            c.up_mbps = MAX_MBIT;
        }
    }

    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    write_cfg(&cfg).map_err(err500)?;
    apply_qos(&cfg).map_err(err500)?;
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn apply() -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }
    let cfg = read_cfg();
    apply_qos(&cfg).map_err(err500)?;
    Ok(Json(serde_json::json!({"success": true, "applied": cfg.enabled})))
}

pub async fn clear() -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }
    clear_qos();
    let mut cfg = read_cfg();
    cfg.enabled = false;
    write_cfg(&cfg).map_err(err500)?;
    Ok(Json(serde_json::json!({"success": true})))
}

/// Re-apply saved QoS shaping on service start so limits survive a reboot.
pub fn reapply_on_boot() {
    let cfg = read_cfg();
    if cfg.enabled {
        let _ = apply_qos(&cfg);
    }
}
