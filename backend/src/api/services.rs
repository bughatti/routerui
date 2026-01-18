use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;

// Services we want to show in the UI
const MANAGED_SERVICES: &[(&str, &str)] = &[
    ("dnsmasq", "DHCP & DNS Server"),
    ("hostapd", "WiFi Access Point"),
    ("sshd", "SSH Server"),
    ("cloudflared", "Cloudflare Tunnel"),
    ("clamav-daemon", "ClamAV Antivirus"),
    ("clamav-freshclam", "ClamAV Updates"),
    ("docker", "Docker Engine"),
    ("AdGuardHome", "AdGuard Home"),
    ("NetworkManager", "Network Manager"),
    ("ufw", "Firewall (UFW)"),
    ("netfilter-persistent", "Firewall Rules"),
];

#[derive(Debug, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub status: String,        // active, inactive, failed, not-found
    pub is_running: bool,
    pub is_enabled: bool,      // starts on boot
    pub uptime: Option<String>,
    pub memory: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ServiceList {
    pub services: Vec<ServiceInfo>,
    pub total_running: u32,
    pub total_failed: u32,
}

#[derive(Debug, Deserialize)]
pub struct ServiceAction {
    pub name: String,
    pub action: String, // start, stop, restart, enable, disable
}

#[derive(Debug, Deserialize)]
pub struct ServiceLogsRequest {
    pub name: String,
    pub lines: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ServiceLogs {
    pub name: String,
    pub logs: String,
}

// ============ HELPER FUNCTIONS ============

fn get_service_status(name: &str) -> (String, bool) {
    let output = Command::new("systemctl")
        .args(["is-active", name])
        .output();

    match output {
        Ok(o) => {
            let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let is_running = status == "active";
            (status, is_running)
        }
        Err(_) => ("unknown".to_string(), false),
    }
}

fn is_service_enabled(name: &str) -> bool {
    Command::new("systemctl")
        .args(["is-enabled", name])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "enabled")
        .unwrap_or(false)
}

fn get_service_details(name: &str) -> (Option<String>, Option<String>, Option<u32>, String) {
    let output = Command::new("systemctl")
        .args(["show", name, "--property=ActiveEnterTimestamp,MemoryCurrent,MainPID,Description"])
        .output();

    let mut uptime = None;
    let mut memory = None;
    let mut pid = None;
    let mut description = String::new();

    if let Ok(o) = output {
        let text = String::from_utf8_lossy(&o.stdout);
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "ActiveEnterTimestamp" if !value.is_empty() => {
                        // Parse timestamp and calculate uptime
                        uptime = Some(value.to_string());
                    }
                    "MemoryCurrent" => {
                        if let Ok(bytes) = value.parse::<u64>() {
                            if bytes > 0 && bytes < u64::MAX {
                                memory = Some(format_bytes(bytes));
                            }
                        }
                    }
                    "MainPID" => {
                        if let Ok(p) = value.parse::<u32>() {
                            if p > 0 {
                                pid = Some(p);
                            }
                        }
                    }
                    "Description" => {
                        description = value.to_string();
                    }
                    _ => {}
                }
            }
        }
    }

    (uptime, memory, pid, description)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

// ============ API ENDPOINTS ============

pub async fn list() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::services::list()));
    }

    let mut services = Vec::new();
    let mut total_running = 0;
    let mut total_failed = 0;

    for (name, display_name) in MANAGED_SERVICES {
        let (status, is_running) = get_service_status(name);

        // Skip services that don't exist
        if status == "inactive" || status == "active" || status == "failed" {
            let is_enabled = is_service_enabled(name);
            let (uptime, memory, pid, description) = get_service_details(name);

            if is_running {
                total_running += 1;
            }
            if status == "failed" {
                total_failed += 1;
            }

            services.push(ServiceInfo {
                name: name.to_string(),
                display_name: display_name.to_string(),
                description,
                status: status.clone(),
                is_running,
                is_enabled,
                uptime,
                memory,
                pid,
            });
        }
    }

    Ok(Json(serde_json::to_value(ServiceList {
        services,
        total_running,
        total_failed,
    }).unwrap()))
}

pub async fn list_all() -> Result<Json<ServiceList>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(ServiceList {
            services: vec![
                ServiceInfo { name: "sshd".to_string(), display_name: "SSH Server".to_string(), description: "OpenSSH Server".to_string(), status: "active".to_string(), is_running: true, is_enabled: true, uptime: Some("2 days".to_string()), memory: Some("12.5 MB".to_string()), pid: Some(1234) },
                ServiceInfo { name: "dnsmasq".to_string(), display_name: "DHCP/DNS".to_string(), description: "dnsmasq - DHCP and DNS server".to_string(), status: "active".to_string(), is_running: true, is_enabled: true, uptime: Some("2 days".to_string()), memory: Some("8.2 MB".to_string()), pid: Some(1235) },
                ServiceInfo { name: "docker".to_string(), display_name: "Docker".to_string(), description: "Docker Application Container Engine".to_string(), status: "active".to_string(), is_running: true, is_enabled: true, uptime: Some("2 days".to_string()), memory: Some("156.8 MB".to_string()), pid: Some(1236) },
            ],
            total_running: 3,
            total_failed: 0,
        }));
    }

    // List all running services
    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--state=running,failed", "--no-pager", "--plain"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut services = Vec::new();
    let mut total_running = 0;
    let mut total_failed = 0;

    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let unit = parts[0].trim_end_matches(".service");
            let load = parts[1];
            let active = parts[2];
            let sub = parts[3];

            if load != "loaded" {
                continue;
            }

            let is_running = active == "active";
            let status = if sub == "running" {
                "active"
            } else if sub == "failed" {
                "failed"
            } else {
                sub
            }.to_string();

            if is_running {
                total_running += 1;
            }
            if status == "failed" {
                total_failed += 1;
            }

            let is_enabled = is_service_enabled(unit);
            let (uptime, memory, pid, description) = get_service_details(unit);

            services.push(ServiceInfo {
                name: unit.to_string(),
                display_name: unit.to_string(),
                description,
                status,
                is_running,
                is_enabled,
                uptime,
                memory,
                pid,
            });
        }
    }

    Ok(Json(ServiceList {
        services,
        total_running,
        total_failed,
    }))
}

pub async fn action(
    Json(payload): Json<ServiceAction>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let action = match payload.action.as_str() {
        "start" | "stop" | "restart" | "enable" | "disable" => payload.action.as_str(),
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid action".to_string())),
    };

    // Validate service name (prevent command injection)
    if !payload.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Invalid service name".to_string()));
    }

    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "action": action,
            "service": payload.name,
            "mock": true
        })));
    }

    let output = Command::new("sudo")
        .args(["systemctl", action, &payload.name])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "action": action,
        "service": payload.name
    })))
}

pub async fn logs(
    Json(payload): Json<ServiceLogsRequest>,
) -> Result<Json<ServiceLogs>, (StatusCode, String)> {
    // Validate service name
    if !payload.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Invalid service name".to_string()));
    }

    if mock::is_mock_mode() {
        return Ok(Json(ServiceLogs {
            name: payload.name,
            logs: "2026-01-18 10:00:00 Mock service started\n2026-01-18 10:00:01 Mock service running normally\n2026-01-18 10:01:00 Mock service heartbeat\n".to_string(),
        }));
    }

    let lines = payload.lines.unwrap_or(50);
    let lines_str = lines.to_string();

    let output = Command::new("sudo")
        .args(["journalctl", "-u", &payload.name, "-n", &lines_str, "--no-pager", "-o", "short-iso"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let logs = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(Json(ServiceLogs {
        name: payload.name,
        logs,
    }))
}

// Get status of a single service
pub async fn status(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<ServiceInfo>, (StatusCode, String)> {
    let name = payload.get("name")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing service name".to_string()))?;

    // Validate service name
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
        return Err((StatusCode::BAD_REQUEST, "Invalid service name".to_string()));
    }

    if mock::is_mock_mode() {
        return Ok(Json(ServiceInfo {
            name: name.to_string(),
            display_name: name.to_string(),
            description: "Mock service".to_string(),
            status: "active".to_string(),
            is_running: true,
            is_enabled: true,
            uptime: Some("2 days".to_string()),
            memory: Some("10.0 MB".to_string()),
            pid: Some(1234),
        }));
    }

    let (status, is_running) = get_service_status(name);
    let is_enabled = is_service_enabled(name);
    let (uptime, memory, pid, description) = get_service_details(name);

    // Find display name
    let display_name = MANAGED_SERVICES.iter()
        .find(|(n, _)| *n == name)
        .map(|(_, d)| d.to_string())
        .unwrap_or_else(|| name.to_string());

    Ok(Json(ServiceInfo {
        name: name.to_string(),
        display_name,
        description,
        status,
        is_running,
        is_enabled,
        uptime,
        memory,
        pid,
    }))
}
