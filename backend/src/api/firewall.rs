use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mock;

const BACKUP_FILE: &str = "/tmp/iptables-backup";
const PENDING_FILE: &str = "/tmp/firewall-pending";
const ROLLBACK_TIMEOUT: u64 = 300; // 5 minutes in seconds

#[derive(Debug, Serialize)]
pub struct FirewallStatus {
    pub enabled: bool,
    pub input_policy: String,
    pub forward_policy: String,
    pub output_policy: String,
    pub pending_changes: bool,
    pub pending_timeout: Option<u64>, // seconds remaining
}

#[derive(Debug, Serialize)]
pub struct PendingStatus {
    pub pending: bool,
    pub seconds_remaining: Option<u64>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct PortForward {
    pub id: u32,
    pub enabled: bool,
    pub protocol: String,
    pub external_port: u16,
    pub internal_ip: String,
    pub internal_port: u16,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct AddPortForward {
    pub protocol: String,
    pub external_port: u16,
    pub internal_ip: String,
    pub internal_port: u16,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemovePortForward {
    pub protocol: String,
    pub external_port: u16,
    pub internal_ip: String,
    pub internal_port: u16,
}

#[derive(Debug, Serialize)]
pub struct BlockedIP {
    pub ip: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct AddBlockedIP {
    pub ip: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveBlockedIP {
    pub ip: String,
}

#[derive(Debug, Serialize)]
pub struct RawRules {
    pub filter: String,
    pub nat: String,
}

#[derive(Debug, Serialize)]
pub struct DMZStatus {
    pub enabled: bool,
    pub target_ip: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetDMZ {
    pub enabled: bool,
    pub target_ip: Option<String>,
}

// ============ ROLLBACK/CONFIRM SYSTEM ============

fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn check_pending_status() -> (bool, Option<u64>) {
    if let Ok(content) = fs::read_to_string(PENDING_FILE) {
        if let Ok(deadline) = content.trim().parse::<u64>() {
            let now = get_current_timestamp();
            if now < deadline {
                return (true, Some(deadline - now));
            } else {
                // Timer expired - trigger rollback
                let _ = do_rollback();
            }
        }
    }
    (false, None)
}

fn save_backup() -> Result<(), (StatusCode, String)> {
    // Only save backup if there isn't already a pending change
    let (pending, _) = check_pending_status();
    if pending {
        return Ok(()); // Don't overwrite backup during pending state
    }

    let output = Command::new("sudo")
        .args(["iptables-save"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fs::write(BACKUP_FILE, &output.stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Also save NAT table
    let nat_output = Command::new("sudo")
        .args(["iptables-save", "-t", "nat"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fs::write(format!("{}-nat", BACKUP_FILE), &nat_output.stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn start_rollback_timer() -> Result<(), (StatusCode, String)> {
    let deadline = get_current_timestamp() + ROLLBACK_TIMEOUT;
    fs::write(PENDING_FILE, deadline.to_string())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Start background rollback process
    Command::new("bash")
        .args(["-c", &format!(
            "sleep {} && [ -f {} ] && sudo iptables-restore < {} && rm -f {} {} 2>/dev/null &",
            ROLLBACK_TIMEOUT, PENDING_FILE, BACKUP_FILE, PENDING_FILE, BACKUP_FILE
        )])
        .spawn()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn do_rollback() -> Result<(), (StatusCode, String)> {
    if fs::metadata(BACKUP_FILE).is_ok() {
        Command::new("sudo")
            .args(["iptables-restore"])
            .stdin(std::process::Stdio::from(
                std::fs::File::open(BACKUP_FILE)
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            ))
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Clean up
    let _ = fs::remove_file(PENDING_FILE);
    let _ = fs::remove_file(BACKUP_FILE);
    let _ = fs::remove_file(format!("{}-nat", BACKUP_FILE));

    Ok(())
}

fn do_confirm() -> Result<(), (StatusCode, String)> {
    // Remove pending file to cancel rollback
    let _ = fs::remove_file(PENDING_FILE);
    let _ = fs::remove_file(BACKUP_FILE);
    let _ = fs::remove_file(format!("{}-nat", BACKUP_FILE));

    // Persist rules
    save_rules_permanent()?;

    Ok(())
}

fn save_rules_permanent() -> Result<(), (StatusCode, String)> {
    Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

// Apply change with rollback protection
fn apply_with_rollback<F>(change_fn: F) -> Result<(), (StatusCode, String)>
where
    F: FnOnce() -> Result<(), (StatusCode, String)>,
{
    save_backup()?;
    change_fn()?;
    start_rollback_timer()?;
    Ok(())
}

// ============ API ENDPOINTS ============

// Check pending status
pub async fn pending() -> Result<Json<PendingStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(PendingStatus {
            pending: false,
            seconds_remaining: None,
            message: "No pending changes (mock).".to_string(),
        }));
    }

    let (pending, seconds) = check_pending_status();

    Ok(Json(PendingStatus {
        pending,
        seconds_remaining: seconds,
        message: if pending {
            format!("Changes pending confirmation. Auto-revert in {} seconds.", seconds.unwrap_or(0))
        } else {
            "No pending changes.".to_string()
        },
    }))
}

// Confirm pending changes
pub async fn confirm() -> Result<Json<PendingStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(PendingStatus {
            pending: false,
            seconds_remaining: None,
            message: "Changes confirmed and saved (mock).".to_string(),
        }));
    }

    do_confirm()?;

    Ok(Json(PendingStatus {
        pending: false,
        seconds_remaining: None,
        message: "Changes confirmed and saved.".to_string(),
    }))
}

// Revert pending changes
pub async fn revert() -> Result<Json<PendingStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(PendingStatus {
            pending: false,
            seconds_remaining: None,
            message: "Changes reverted to previous state (mock).".to_string(),
        }));
    }

    do_rollback()?;

    Ok(Json(PendingStatus {
        pending: false,
        seconds_remaining: None,
        message: "Changes reverted to previous state.".to_string(),
    }))
}

// Get firewall status
pub async fn status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::firewall::status()));
    }

    // Check for pending changes and possibly trigger rollback
    let (pending, seconds) = check_pending_status();

    let output = Command::new("sudo")
        .args(["iptables", "-L", "-n"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rules = String::from_utf8_lossy(&output.stdout);

    let input_policy = parse_chain_policy(&rules, "INPUT");
    let forward_policy = parse_chain_policy(&rules, "FORWARD");
    let output_policy = parse_chain_policy(&rules, "OUTPUT");

    let enabled = input_policy == "DROP";

    Ok(Json(serde_json::to_value(FirewallStatus {
        enabled,
        input_policy,
        forward_policy,
        output_policy,
        pending_changes: pending,
        pending_timeout: seconds,
    }).unwrap()))
}

fn parse_chain_policy(rules: &str, chain: &str) -> String {
    for line in rules.lines() {
        if line.starts_with(&format!("Chain {}", chain)) {
            if line.contains("policy ACCEPT") {
                return "ACCEPT".to_string();
            } else if line.contains("policy DROP") {
                return "DROP".to_string();
            }
        }
    }
    "UNKNOWN".to_string()
}

// Toggle firewall
#[derive(Debug, Deserialize)]
pub struct ToggleFirewall {
    pub enabled: bool,
}

pub async fn toggle(
    Json(payload): Json<ToggleFirewall>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "enabled": payload.enabled,
            "input_policy": if payload.enabled { "DROP" } else { "ACCEPT" },
            "forward_policy": "ACCEPT",
            "output_policy": "ACCEPT",
            "pending_changes": false,
            "pending_timeout": null,
            "mock": true
        })));
    }

    let change_fn = || {
        if payload.enabled {
            // Enable firewall with safe rules

            // First, add rules to allow LAN and established connections BEFORE changing policy
            // Allow LAN
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "1", "-i", "enp2s0", "-j", "ACCEPT"])
                .output();

            // Allow WiFi
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "2", "-i", "wlo1", "-j", "ACCEPT"])
                .output();

            // Allow br0 bridge (LAN traffic goes through here)
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "3", "-i", "br0", "-j", "ACCEPT"])
                .output();

            // Allow loopback
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "4", "-i", "lo", "-j", "ACCEPT"])
                .output();

            // Allow established/related
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "5", "-m", "state", "--state", "ESTABLISHED,RELATED", "-j", "ACCEPT"])
                .output();

            // Allow DHCP on WAN (for IP renewal) - UDP port 68
            let _ = Command::new("sudo")
                .args(["iptables", "-I", "INPUT", "6", "-i", "enp1s0", "-p", "udp", "--dport", "68", "-j", "ACCEPT"])
                .output();

            // Now set INPUT policy to DROP
            Command::new("sudo")
                .args(["iptables", "-P", "INPUT", "DROP"])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        } else {
            // Disable firewall - set to ACCEPT
            Command::new("sudo")
                .args(["iptables", "-P", "INPUT", "ACCEPT"])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        Ok(())
    };

    apply_with_rollback(change_fn)?;

    status().await
}

// List port forwards
pub async fn port_forwards() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::firewall::port_forwards()));
    }

    let output = Command::new("sudo")
        .args(["iptables", "-t", "nat", "-L", "PREROUTING", "-n", "--line-numbers"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rules = String::from_utf8_lossy(&output.stdout);
    let mut forwards = Vec::new();

    for line in rules.lines().skip(2) {
        if let Some(forward) = parse_port_forward(line) {
            forwards.push(forward);
        }
    }

    Ok(Json(serde_json::to_value(forwards).unwrap()))
}

fn parse_port_forward(line: &str) -> Option<PortForward> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 6 || parts[1] != "DNAT" {
        return None;
    }

    let id: u32 = parts[0].parse().ok()?;
    let protocol = parts[2].to_string();

    let mut external_port: u16 = 0;
    let mut internal_ip = String::new();
    let mut internal_port: u16 = 0;

    for part in &parts {
        if part.starts_with("dpt:") {
            external_port = part.trim_start_matches("dpt:").parse().ok()?;
        }
        if part.starts_with("to:") {
            let dest = part.trim_start_matches("to:");
            let dest_parts: Vec<&str> = dest.split(':').collect();
            if dest_parts.len() == 2 {
                internal_ip = dest_parts[0].to_string();
                internal_port = dest_parts[1].parse().ok()?;
            }
        }
    }

    if external_port == 0 || internal_ip.is_empty() {
        return None;
    }

    Some(PortForward {
        id,
        enabled: true,
        protocol,
        external_port,
        internal_ip,
        internal_port,
        description: String::new(),
    })
}

// Add port forward
pub async fn add_port_forward(
    Json(payload): Json<AddPortForward>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "pending": true, "mock": true})));
    }

    let protocol = payload.protocol.to_lowercase();
    if protocol != "tcp" && protocol != "udp" && protocol != "both" {
        return Err((StatusCode::BAD_REQUEST, "Invalid protocol".to_string()));
    }

    let protocols: Vec<&str> = if protocol == "both" {
        vec!["tcp", "udp"]
    } else {
        vec![protocol.as_str()]
    };

    let ext_port = payload.external_port;
    let int_ip = payload.internal_ip.clone();
    let int_port = payload.internal_port;

    let change_fn = move || {
        for proto in &protocols {
            let dnat_result = Command::new("sudo")
                .args([
                    "iptables", "-t", "nat", "-A", "PREROUTING",
                    "-i", "enp1s0",
                    "-p", proto,
                    "--dport", &ext_port.to_string(),
                    "-j", "DNAT",
                    "--to-destination", &format!("{}:{}", int_ip, int_port),
                ])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if !dnat_result.status.success() {
                return Err((StatusCode::INTERNAL_SERVER_ERROR,
                    String::from_utf8_lossy(&dnat_result.stderr).to_string()));
            }

            let forward_result = Command::new("sudo")
                .args([
                    "iptables", "-A", "FORWARD",
                    "-p", proto,
                    "-d", &int_ip,
                    "--dport", &int_port.to_string(),
                    "-j", "ACCEPT",
                ])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if !forward_result.status.success() {
                return Err((StatusCode::INTERNAL_SERVER_ERROR,
                    String::from_utf8_lossy(&forward_result.stderr).to_string()));
            }
        }
        Ok(())
    };

    apply_with_rollback(change_fn)?;

    Ok(Json(serde_json::json!({"success": true, "pending": true})))
}

// Remove port forward
pub async fn remove_port_forward(
    Json(payload): Json<RemovePortForward>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "pending": true, "mock": true})));
    }

    let protocol = payload.protocol.to_lowercase();
    let protocols: Vec<&str> = if protocol == "both" {
        vec!["tcp", "udp"]
    } else {
        vec![protocol.as_str()]
    };

    let ext_port = payload.external_port;
    let int_ip = payload.internal_ip.clone();
    let int_port = payload.internal_port;

    let change_fn = move || {
        for proto in &protocols {
            let _ = Command::new("sudo")
                .args([
                    "iptables", "-t", "nat", "-D", "PREROUTING",
                    "-i", "enp1s0",
                    "-p", proto,
                    "--dport", &ext_port.to_string(),
                    "-j", "DNAT",
                    "--to-destination", &format!("{}:{}", int_ip, int_port),
                ])
                .output();

            let _ = Command::new("sudo")
                .args([
                    "iptables", "-D", "FORWARD",
                    "-p", proto,
                    "-d", &int_ip,
                    "--dport", &int_port.to_string(),
                    "-j", "ACCEPT",
                ])
                .output();
        }
        Ok(())
    };

    apply_with_rollback(change_fn)?;

    Ok(Json(serde_json::json!({"success": true, "pending": true})))
}

// List blocked IPs
pub async fn blocked_ips() -> Result<Json<Vec<BlockedIP>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            BlockedIP { ip: "45.155.205.100".to_string(), description: "Known scanner".to_string() },
            BlockedIP { ip: "192.168.1.100".to_string(), description: "Test block".to_string() },
        ]));
    }

    let output = Command::new("sudo")
        .args(["iptables", "-L", "INPUT", "-n", "--line-numbers"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rules = String::from_utf8_lossy(&output.stdout);
    let mut blocked = Vec::new();

    for line in rules.lines().skip(2) {
        if let Some(ip) = parse_blocked_ip(line) {
            blocked.push(ip);
        }
    }

    Ok(Json(blocked))
}

fn parse_blocked_ip(line: &str) -> Option<BlockedIP> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    if parts.len() < 5 || parts[1] != "DROP" {
        return None;
    }

    let source = parts[4];
    if source == "0.0.0.0/0" {
        return None;
    }

    Some(BlockedIP {
        ip: source.to_string(),
        description: String::new(),
    })
}

// Add blocked IP
pub async fn add_blocked_ip(
    Json(payload): Json<AddBlockedIP>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "pending": true, "mock": true})));
    }

    let ip = payload.ip.clone();

    let change_fn = move || {
        Command::new("sudo")
            .args(["iptables", "-I", "INPUT", "1", "-s", &ip, "-j", "DROP"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Command::new("sudo")
            .args(["iptables", "-I", "FORWARD", "1", "-s", &ip, "-j", "DROP"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        Ok(())
    };

    apply_with_rollback(change_fn)?;

    Ok(Json(serde_json::json!({"success": true, "pending": true})))
}

// Remove blocked IP
pub async fn remove_blocked_ip(
    Json(payload): Json<RemoveBlockedIP>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "pending": true, "mock": true})));
    }

    let ip = payload.ip.clone();

    let change_fn = move || {
        let _ = Command::new("sudo")
            .args(["iptables", "-D", "INPUT", "-s", &ip, "-j", "DROP"])
            .output();

        let _ = Command::new("sudo")
            .args(["iptables", "-D", "FORWARD", "-s", &ip, "-j", "DROP"])
            .output();

        Ok(())
    };

    apply_with_rollback(change_fn)?;

    Ok(Json(serde_json::json!({"success": true, "pending": true})))
}

// Get raw iptables rules
pub async fn raw_rules() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::firewall::rules()));
    }

    let filter = Command::new("sudo")
        .args(["iptables", "-L", "-n", "-v"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let nat = Command::new("sudo")
        .args(["iptables", "-t", "nat", "-L", "-n", "-v"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::to_value(RawRules {
        filter: String::from_utf8_lossy(&filter.stdout).to_string(),
        nat: String::from_utf8_lossy(&nat.stdout).to_string(),
    }).unwrap()))
}

// Get DMZ status
pub async fn dmz_status() -> Result<Json<DMZStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(DMZStatus {
            enabled: false,
            target_ip: None,
        }));
    }

    let output = Command::new("sudo")
        .args(["iptables", "-t", "nat", "-L", "PREROUTING", "-n"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rules = String::from_utf8_lossy(&output.stdout);

    for line in rules.lines() {
        if line.contains("DNAT") && line.contains("0.0.0.0/0") && !line.contains("dpt:") {
            if let Some(pos) = line.find("to:") {
                let target = line[pos + 3..].split_whitespace().next().unwrap_or("");
                let ip = target.split(':').next().unwrap_or(target);
                return Ok(Json(DMZStatus {
                    enabled: true,
                    target_ip: Some(ip.to_string()),
                }));
            }
        }
    }

    Ok(Json(DMZStatus {
        enabled: false,
        target_ip: None,
    }))
}

// Set DMZ
pub async fn set_dmz(
    Json(payload): Json<SetDMZ>,
) -> Result<Json<DMZStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(DMZStatus {
            enabled: payload.enabled,
            target_ip: payload.target_ip.clone(),
        }));
    }

    let enabled = payload.enabled;
    let target_ip = payload.target_ip.clone();

    let change_fn = move || {
        // Remove any existing DMZ rules
        let _ = Command::new("sudo")
            .args(["iptables", "-t", "nat", "-D", "PREROUTING", "-i", "enp1s0", "-j", "DNAT", "--to-destination", "0.0.0.0"])
            .output();

        if enabled {
            if let Some(ref ip) = target_ip {
                Command::new("sudo")
                    .args([
                        "iptables", "-t", "nat", "-A", "PREROUTING",
                        "-i", "enp1s0",
                        "-j", "DNAT",
                        "--to-destination", ip,
                    ])
                    .output()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

                Command::new("sudo")
                    .args([
                        "iptables", "-A", "FORWARD",
                        "-d", ip,
                        "-j", "ACCEPT",
                    ])
                    .output()
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            }
        }
        Ok(())
    };

    apply_with_rollback(change_fn)?;

    dmz_status().await
}
