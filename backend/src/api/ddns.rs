use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;
use crate::validate;
use super::AuthUser;

const DDNS_CONF: &str = "/opt/routerui/config/ddns.json";

// ── stored config ───────────────────────────────────────────────────────────
//
// Persisted at DDNS_CONF (mode 0600). Secret fields (token/password/api creds)
// are NEVER returned to the client: get_config() masks them behind
// `has_credentials`. No secret is ever baked into the binary or logged.

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DdnsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String, // cloudflare | duckdns | noip | custom
    #[serde(default)]
    pub hostname: String,
    // provider-specific credentials
    #[serde(default)]
    pub token: String, // duckdns token, or cloudflare api_token
    #[serde(default)]
    pub username: String, // noip username
    #[serde(default)]
    pub password: String, // noip password
    #[serde(default)]
    pub zone_id: String, // cloudflare
    #[serde(default)]
    pub record_id: String, // cloudflare
    #[serde(default)]
    pub custom_url: String, // custom template containing {ip}
    // status
    #[serde(default)]
    pub last_ip: String,
    #[serde(default)]
    pub last_update: String,
    #[serde(default)]
    pub last_status: String,
}

fn load_config() -> DdnsConfig {
    std::fs::read_to_string(DDNS_CONF)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_config(cfg: &DdnsConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    // Written through a root helper so it works regardless of file ownership,
    // then locked to 0600 because it holds provider credentials.
    use std::io::Write;
    let mut child = Command::new("sudo")
        .args(["tee", DDNS_CONF])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .as_mut()
        .map(|i| i.write_all(json.as_bytes()))
        .transpose()
        .map_err(|e| e.to_string())?;
    child.wait().map_err(|e| e.to_string())?;
    let _ = Command::new("sudo").args(["chmod", "600", DDNS_CONF]).status();
    Ok(())
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .connect_timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Best-effort current public IP: ask an external echo service, and if that is
/// unreachable fall back to the WAN interface's address.
async fn current_public_ip() -> Option<String> {
    let c = http();
    for url in ["https://api.ipify.org", "https://ifconfig.me/ip"] {
        if let Ok(resp) = c.get(url).send().await {
            if let Ok(text) = resp.text().await {
                let ip = text.trim().to_string();
                if validate::is_ipv4(&ip) {
                    return Some(ip);
                }
            }
        }
    }
    wan_ip()
}

fn wan_ip() -> Option<String> {
    let route = Command::new("ip").args(["route", "show", "default"]).output().ok()?;
    let route = String::from_utf8_lossy(&route.stdout);
    let iface = route
        .split_whitespace()
        .skip_while(|w| *w != "dev")
        .nth(1)?;
    let addr = Command::new("ip").args(["-4", "-o", "addr", "show", "dev", iface]).output().ok()?;
    let addr = String::from_utf8_lossy(&addr.stdout);
    addr.split_whitespace()
        .skip_while(|w| *w != "inet")
        .nth(1)
        .and_then(|c| c.split('/').next())
        .map(|s| s.to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── provider calls ──────────────────────────────────────────────────────────

async fn push_update(cfg: &DdnsConfig, ip: &str) -> Result<String, String> {
    let c = http();
    match cfg.provider.as_str() {
        "duckdns" => {
            // hostname is the subdomain label for duckdns (e.g. "myhome")
            let sub = cfg.hostname.split('.').next().unwrap_or(&cfg.hostname);
            let url = format!(
                "https://www.duckdns.org/update?domains={}&token={}&ip={}",
                sub, cfg.token, ip
            );
            let body = c.get(url).send().await.map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
            if body.trim() == "OK" {
                Ok("OK".into())
            } else {
                Err(format!("duckdns rejected update: {}", body.trim()))
            }
        }
        "noip" => {
            let url = format!(
                "https://dynupdate.no-ip.com/nic/update?hostname={}&myip={}",
                cfg.hostname, ip
            );
            let body = c
                .get(url)
                .basic_auth(&cfg.username, Some(&cfg.password))
                .header("User-Agent", "RouterUI-DDNS/1.0")
                .send()
                .await
                .map_err(|e| e.to_string())?
                .text()
                .await
                .map_err(|e| e.to_string())?;
            let first = body.split_whitespace().next().unwrap_or("");
            if first == "good" || first == "nochg" {
                Ok(first.to_string())
            } else {
                Err(format!("no-ip rejected update: {}", body.trim()))
            }
        }
        "cloudflare" => {
            let url = format!(
                "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                cfg.zone_id, cfg.record_id
            );
            let resp = c
                .patch(url)
                .bearer_auth(&cfg.token)
                .json(&serde_json::json!({
                    "type": "A",
                    "name": cfg.hostname,
                    "content": ip,
                    "ttl": 120,
                }))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            let ok = resp.status().is_success();
            let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            if ok && v.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
                Ok("OK".into())
            } else {
                // surface the API error message but never the token
                let msg = v
                    .get("errors")
                    .and_then(|e| e.get(0))
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("cloudflare update failed");
                Err(msg.to_string())
            }
        }
        "custom" => {
            if !cfg.custom_url.contains("{ip}") {
                return Err("custom URL must contain the {ip} placeholder".into());
            }
            let url = cfg.custom_url.replace("{ip}", ip);
            let resp = c.get(url).send().await.map_err(|e| e.to_string())?;
            if resp.status().is_success() {
                Ok("OK".into())
            } else {
                Err(format!("custom endpoint returned {}", resp.status()))
            }
        }
        other => Err(format!("unknown provider '{}'", other)),
    }
}

/// Run one update cycle. Safe to call on a timer: resolves the public IP and,
/// if it differs from the last successful update, pushes it to the provider.
/// Returns a short human-readable status string.
pub async fn run_updater_once() -> Result<String, String> {
    let mut cfg = load_config();
    if !cfg.enabled {
        return Ok("disabled".into());
    }
    if cfg.hostname.is_empty() || cfg.provider.is_empty() {
        return Err("not configured".into());
    }
    let ip = current_public_ip().await.ok_or("could not determine public IP")?;
    if ip == cfg.last_ip && cfg.last_status == "ok" {
        return Ok(format!("no change ({})", ip));
    }
    match push_update(&cfg, &ip).await {
        Ok(_) => {
            cfg.last_ip = ip.clone();
            cfg.last_update = now_iso();
            cfg.last_status = "ok".into();
            let _ = save_config(&cfg);
            Ok(format!("updated to {}", ip))
        }
        Err(e) => {
            cfg.last_update = now_iso();
            cfg.last_status = format!("error: {}", e);
            let _ = save_config(&cfg);
            Err(e)
        }
    }
}

// ── HTTP handlers ───────────────────────────────────────────────────────────

/// Config with all secrets stripped, plus a `has_credentials` flag per the
/// active provider so the UI can show "configured" without exposing anything.
pub async fn get_config() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "enabled": false, "provider": "duckdns", "hostname": "",
            "has_credentials": false, "last_ip": "", "last_update": "", "last_status": ""
        })));
    }
    let cfg = load_config();
    let has_credentials = match cfg.provider.as_str() {
        "duckdns" => !cfg.token.is_empty(),
        "noip" => !cfg.username.is_empty() && !cfg.password.is_empty(),
        "cloudflare" => !cfg.token.is_empty() && !cfg.zone_id.is_empty() && !cfg.record_id.is_empty(),
        "custom" => !cfg.custom_url.is_empty(),
        _ => false,
    };
    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "provider": cfg.provider,
        "hostname": cfg.hostname,
        "zone_id": cfg.zone_id,
        "record_id": cfg.record_id,
        "custom_url": cfg.custom_url,
        "has_credentials": has_credentials,
        "last_ip": cfg.last_ip,
        "last_update": cfg.last_update,
        "last_status": cfg.last_status,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SetConfig {
    pub enabled: bool,
    pub provider: String,
    pub hostname: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub zone_id: Option<String>,
    #[serde(default)]
    pub record_id: Option<String>,
    #[serde(default)]
    pub custom_url: Option<String>,
}

pub async fn set_config(
    _user: AuthUser,
    Json(payload): Json<SetConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if !matches!(payload.provider.as_str(), "duckdns" | "noip" | "cloudflare" | "custom") {
        return Err((StatusCode::BAD_REQUEST, "invalid provider".into()));
    }
    if !payload.hostname.is_empty() && !validate::is_hostname(&payload.hostname) {
        return Err((StatusCode::BAD_REQUEST, "invalid hostname".into()));
    }
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    // Merge onto the stored config so blank secret fields (the UI never echoes
    // secrets back) preserve what was already saved instead of wiping it.
    let mut cfg = load_config();
    cfg.enabled = payload.enabled;
    cfg.provider = payload.provider;
    cfg.hostname = payload.hostname;
    cfg.zone_id = payload.zone_id.unwrap_or(cfg.zone_id);
    cfg.record_id = payload.record_id.unwrap_or(cfg.record_id);
    if let Some(v) = payload.token { if !v.is_empty() { cfg.token = v; } }
    if let Some(v) = payload.username { if !v.is_empty() { cfg.username = v; } }
    if let Some(v) = payload.password { if !v.is_empty() { cfg.password = v; } }
    if let Some(v) = payload.custom_url { cfg.custom_url = v; }

    save_config(&cfg).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn update_now(
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "ip": "203.0.113.10", "status": "updated", "mock": true })));
    }
    match run_updater_once().await {
        Ok(status) => {
            let cfg = load_config();
            Ok(Json(serde_json::json!({ "success": true, "ip": cfg.last_ip, "status": status })))
        }
        Err(e) => Err((StatusCode::BAD_GATEWAY, e)),
    }
}

pub async fn status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "enabled": false, "last_ip": "", "last_update": "", "last_status": ""
        })));
    }
    let cfg = load_config();
    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "provider": cfg.provider,
        "hostname": cfg.hostname,
        "last_ip": cfg.last_ip,
        "last_update": cfg.last_update,
        "last_status": cfg.last_status,
    })))
}
