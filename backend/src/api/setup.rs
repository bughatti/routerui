use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::sync::Arc;

use crate::AppState;

// ============ DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub is_complete: bool,
    pub current_step: u8,
    pub total_steps: u8,
}

#[derive(Debug, Serialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac: String,
    pub ip: Option<String>,
    pub is_up: bool,
    pub is_wireless: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ConfigureRouterRequest {
    pub wan_interface: String,
    pub lan_interface: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigStep {
    pub name: String,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConfigureRouterResponse {
    pub success: bool,
    pub steps: Vec<ConfigStep>,
}

// ============ API ENDPOINTS ============

/// Check if setup is complete
pub async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SetupStatus>, (StatusCode, String)> {
    let config_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='setup_config'"
    )
        .fetch_one(&state.db)
        .await
        .unwrap_or(0) > 0;

    if !config_exists {
        return Ok(Json(SetupStatus {
            is_complete: false,
            current_step: 1,
            total_steps: 4,
        }));
    }

    let setup_complete = sqlx::query_scalar::<_, String>(
        "SELECT value FROM setup_config WHERE key = 'setup_complete'"
    )
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);

    Ok(Json(SetupStatus {
        is_complete: setup_complete,
        current_step: if setup_complete { 4 } else { 1 },
        total_steps: 4,
    }))
}

/// Get available network interfaces
pub async fn get_interfaces() -> Result<Json<Vec<NetworkInterface>>, (StatusCode, String)> {
    let output = Command::new("ip")
        .args(["-j", "addr"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let interfaces: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .unwrap_or_default();

    let mut result = Vec::new();

    for iface in interfaces {
        let name = iface["ifname"].as_str().unwrap_or("").to_string();

        // Skip loopback and virtual interfaces
        if name == "lo" || name.starts_with("veth") || name.starts_with("br-") || name.starts_with("docker") {
            continue;
        }

        let mac = iface["address"].as_str().unwrap_or("").to_string();
        let is_up = iface["operstate"].as_str().unwrap_or("") == "UP";

        // Check if wireless
        let is_wireless = std::path::Path::new(&format!("/sys/class/net/{}/wireless", name)).exists();

        // Get IP address
        let ip = iface["addr_info"]
            .as_array()
            .and_then(|arr| arr.iter().find(|a| a["family"].as_str() == Some("inet")))
            .and_then(|a| a["local"].as_str())
            .map(|s| s.to_string());

        result.push(NetworkInterface {
            name,
            mac,
            ip,
            is_up,
            is_wireless,
        });
    }

    Ok(Json(result))
}

/// Create admin account during setup
pub async fn create_admin(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAdminRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if payload.username.len() < 3 {
        return Err((StatusCode::BAD_REQUEST, "Username must be at least 3 characters".to_string()));
    }
    if payload.password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "Password must be at least 6 characters".to_string()));
    }

    let password_hash = crate::auth::hash_password(&payload.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    if existing > 0 {
        return Err((StatusCode::CONFLICT, "Admin account already exists".to_string()));
    }

    sqlx::query(
        "INSERT INTO users (username, password_hash, role, enabled, created_at) VALUES (?, ?, 'admin', 1, datetime('now'))"
    )
        .bind(&payload.username)
        .bind(&password_hash)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Admin account created"
    })))
}

/// Configure the router - main configuration endpoint
pub async fn configure_router(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ConfigureRouterRequest>,
) -> Result<Json<ConfigureRouterResponse>, (StatusCode, String)> {
    let wan = &payload.wan_interface;
    let lan = &payload.lan_interface;

    let mut steps = Vec::new();
    let mut all_success = true;

    // Step 1: Set static IP on LAN interface
    let lan_ip_result = configure_lan_ip(lan);
    steps.push(ConfigStep {
        name: format!("Set LAN IP 192.168.1.1 on {}", lan),
        success: lan_ip_result.is_ok(),
        error: lan_ip_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Step 2: Enable IP forwarding
    let forward_result = enable_ip_forwarding();
    steps.push(ConfigStep {
        name: "Enable IP forwarding".to_string(),
        success: forward_result.is_ok(),
        error: forward_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Step 3: Configure NAT masquerade
    let nat_result = configure_nat(wan);
    steps.push(ConfigStep {
        name: format!("Configure NAT on {}", wan),
        success: nat_result.is_ok(),
        error: nat_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Step 4: Configure dnsmasq
    let dnsmasq_result = configure_dnsmasq(lan);
    steps.push(ConfigStep {
        name: "Configure DHCP/DNS (dnsmasq)".to_string(),
        success: dnsmasq_result.is_ok(),
        error: dnsmasq_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Step 5: Start dnsmasq
    let start_result = start_dnsmasq();
    steps.push(ConfigStep {
        name: "Start DHCP/DNS service".to_string(),
        success: start_result.is_ok(),
        error: start_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Step 6: Save iptables rules
    let save_result = save_iptables();
    steps.push(ConfigStep {
        name: "Save firewall rules".to_string(),
        success: save_result.is_ok(),
        error: save_result.err(),
    });
    if steps.last().map(|s| !s.success).unwrap_or(false) {
        all_success = false;
    }

    // Save configuration to database
    if all_success {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS setup_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )"
        )
            .execute(&state.db)
            .await
            .ok();

        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('wan_interface', ?)")
            .bind(wan)
            .execute(&state.db)
            .await
            .ok();

        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('lan_interface', ?)")
            .bind(lan)
            .execute(&state.db)
            .await
            .ok();

        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('lan_ip', '192.168.1.1')")
            .execute(&state.db)
            .await
            .ok();

        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('dhcp_start', '192.168.1.100')")
            .execute(&state.db)
            .await
            .ok();

        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('dhcp_end', '192.168.1.250')")
            .execute(&state.db)
            .await
            .ok();
    }

    Ok(Json(ConfigureRouterResponse {
        success: all_success,
        steps,
    }))
}

/// Complete setup
pub async fn complete(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS setup_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"
    )
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('setup_complete', 'true')")
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Setup complete! You can now log in."
    })))
}

// ============ CONFIGURATION FUNCTIONS ============

fn configure_lan_ip(interface: &str) -> Result<(), String> {
    // First, flush existing IP addresses on the interface
    Command::new("ip")
        .args(["addr", "flush", "dev", interface])
        .output()
        .map_err(|e| e.to_string())?;

    // Set the static IP
    let output = Command::new("ip")
        .args(["addr", "add", "192.168.1.1/24", "dev", interface])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ignore "File exists" error - IP might already be set
        if !stderr.contains("File exists") {
            return Err(stderr.to_string());
        }
    }

    // Bring interface up
    Command::new("ip")
        .args(["link", "set", interface, "up"])
        .output()
        .map_err(|e| e.to_string())?;

    // Make it persistent via netplan or interfaces file
    let netplan_config = format!(
        r#"network:
  version: 2
  ethernets:
    {}:
      addresses:
        - 192.168.1.1/24
"#,
        interface
    );

    // Try netplan first (Ubuntu 18.04+)
    if std::path::Path::new("/etc/netplan").exists() {
        std::fs::write(
            "/etc/netplan/99-routerui-lan.yaml",
            &netplan_config
        ).ok();
        Command::new("netplan")
            .args(["apply"])
            .output()
            .ok();
    } else {
        // Fallback to /etc/network/interfaces.d/
        let interfaces_config = format!(
            r#"auto {}
iface {} inet static
    address 192.168.1.1
    netmask 255.255.255.0
"#,
            interface, interface
        );
        std::fs::create_dir_all("/etc/network/interfaces.d").ok();
        std::fs::write(
            format!("/etc/network/interfaces.d/{}", interface),
            &interfaces_config
        ).ok();
    }

    Ok(())
}

fn enable_ip_forwarding() -> Result<(), String> {
    // Enable immediately
    std::fs::write("/proc/sys/net/ipv4/ip_forward", "1")
        .map_err(|e| e.to_string())?;

    // Make it persistent
    let sysctl_content = std::fs::read_to_string("/etc/sysctl.conf")
        .unwrap_or_default();

    if !sysctl_content.contains("net.ipv4.ip_forward=1") {
        let new_content = if sysctl_content.contains("net.ipv4.ip_forward") {
            sysctl_content.replace("#net.ipv4.ip_forward=1", "net.ipv4.ip_forward=1")
                .replace("net.ipv4.ip_forward=0", "net.ipv4.ip_forward=1")
        } else {
            format!("{}\nnet.ipv4.ip_forward=1\n", sysctl_content)
        };
        std::fs::write("/etc/sysctl.conf", new_content)
            .map_err(|e| e.to_string())?;
    }

    // Also write to sysctl.d for systemd systems
    std::fs::create_dir_all("/etc/sysctl.d").ok();
    std::fs::write("/etc/sysctl.d/99-routerui.conf", "net.ipv4.ip_forward=1\n").ok();

    Ok(())
}

fn configure_nat(wan_interface: &str) -> Result<(), String> {
    // Clear existing NAT rules for our interface
    Command::new("iptables")
        .args(["-t", "nat", "-D", "POSTROUTING", "-o", wan_interface, "-j", "MASQUERADE"])
        .output()
        .ok(); // Ignore error if rule doesn't exist

    // Add NAT masquerade rule
    let output = Command::new("iptables")
        .args(["-t", "nat", "-A", "POSTROUTING", "-o", wan_interface, "-j", "MASQUERADE"])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    // Allow forwarding
    Command::new("iptables")
        .args(["-A", "FORWARD", "-i", wan_interface, "-o", wan_interface, "-m", "state", "--state", "RELATED,ESTABLISHED", "-j", "ACCEPT"])
        .output()
        .ok();

    Command::new("iptables")
        .args(["-A", "FORWARD", "-j", "ACCEPT"])
        .output()
        .ok();

    Ok(())
}

fn configure_dnsmasq(lan_interface: &str) -> Result<(), String> {
    let config = format!(
        r#"# RouterUI dnsmasq configuration
# Do not modify - managed by RouterUI

# Interface to listen on
interface={}
bind-interfaces

# DHCP range and lease time
dhcp-range=192.168.1.100,192.168.1.250,255.255.255.0,12h

# Gateway (this router)
dhcp-option=option:router,192.168.1.1

# DNS server (this router)
dhcp-option=option:dns-server,192.168.1.1

# Domain
domain=lan
local=/lan/

# DNS settings
no-resolv
server=8.8.8.8
server=8.8.4.4
server=1.1.1.1

# Don't read /etc/hosts
no-hosts

# Log queries (for debugging)
# log-queries
# log-dhcp
"#,
        lan_interface
    );

    // Write configuration
    std::fs::create_dir_all("/etc/dnsmasq.d").ok();
    std::fs::write("/etc/dnsmasq.d/routerui.conf", &config)
        .map_err(|e| e.to_string())?;

    // Disable default dnsmasq config that might conflict
    let default_conf = "/etc/dnsmasq.conf";
    if std::path::Path::new(default_conf).exists() {
        let content = std::fs::read_to_string(default_conf).unwrap_or_default();
        if !content.contains("conf-dir=/etc/dnsmasq.d") {
            std::fs::write(default_conf, "conf-dir=/etc/dnsmasq.d/,*.conf\n")
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

fn start_dnsmasq() -> Result<(), String> {
    // Stop systemd-resolved if running (conflicts with dnsmasq on port 53)
    Command::new("systemctl")
        .args(["stop", "systemd-resolved"])
        .output()
        .ok();
    Command::new("systemctl")
        .args(["disable", "systemd-resolved"])
        .output()
        .ok();

    // Update /etc/resolv.conf to use our DNS
    std::fs::write("/etc/resolv.conf", "nameserver 127.0.0.1\n").ok();

    // Enable and start dnsmasq
    Command::new("systemctl")
        .args(["enable", "dnsmasq"])
        .output()
        .map_err(|e| e.to_string())?;

    let output = Command::new("systemctl")
        .args(["restart", "dnsmasq"])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        // Try to get more info about the failure
        let status = Command::new("systemctl")
            .args(["status", "dnsmasq"])
            .output()
            .ok();

        let error_info = status
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_else(|| "Unknown error".to_string());

        return Err(format!("Failed to start dnsmasq: {}", error_info));
    }

    Ok(())
}

fn save_iptables() -> Result<(), String> {
    // Save iptables rules
    let output = Command::new("bash")
        .args(["-c", "iptables-save > /etc/iptables/rules.v4"])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            // Try alternative location
            Command::new("bash")
                .args(["-c", "mkdir -p /etc/iptables && iptables-save > /etc/iptables/rules.v4"])
                .output()
                .ok();

            // Also try netfilter-persistent
            Command::new("netfilter-persistent")
                .args(["save"])
                .output()
                .ok();

            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

// ============ LEGACY ENDPOINTS (kept for compatibility) ============

#[derive(Debug, Deserialize)]
pub struct NetworkConfigRequest {
    pub wan_interface: String,
    pub lan_interface: String,
    pub wifi_interface: Option<String>,
}

/// Save network configuration (legacy endpoint)
pub async fn save_network_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NetworkConfigRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS setup_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"
    )
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('wan_interface', ?)")
        .bind(&payload.wan_interface)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('lan_interface', ?)")
        .bind(&payload.lan_interface)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(wifi) = &payload.wifi_interface {
        sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('wifi_interface', ?)")
            .bind(wifi)
            .execute(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Network configuration saved"
    })))
}
