use axum::{http::StatusCode, Json};
use serde::Serialize;
use std::process::Command;
use std::fs;
use std::collections::HashMap;

use crate::mock;
use super::AuthUser;

#[derive(Debug, Serialize)]
pub struct SecurityOverview {
    pub firewall_drops_24h: u64,
    pub blocklist_hits: BlocklistHits,
    pub failed_ssh_attempts_24h: u64,
    pub active_connections: u64,
    pub recent_events: Vec<SecurityEvent>,
    pub top_blocked_ips: Vec<BlockedIP>,
    pub ssh_sessions: Vec<SshSession>,
}

#[derive(Debug, Serialize)]
pub struct BlocklistHits {
    pub spamhaus_drop: u64,
    pub emerging_threats: u64,
}

#[derive(Debug, Serialize)]
pub struct SecurityEvent {
    pub timestamp: String,
    pub event_type: String,
    pub source_ip: String,
    pub details: String,
    pub severity: String,
    pub is_external: bool,  // true = WAN side (192.168.12.x or internet), false = LAN (10.22.22.x)
}

#[derive(Debug, Serialize)]
pub struct BlockedIP {
    pub ip: String,
    pub hits: u64,
    pub last_seen: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SshSession {
    pub user: String,
    pub source_ip: String,
    pub timestamp: String,
    pub status: String,
}

pub async fn overview(
    AuthUser(_user): AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::security::overview()));
    }

    // Get firewall drop counts from iptables
    let firewall_drops = get_firewall_drops();

    // Get blocklist hit counts
    let blocklist_hits = get_blocklist_hits();

    // Get failed SSH attempts from auth.log
    let failed_ssh = get_failed_ssh_attempts();

    // Get active connection count
    let active_connections = get_active_connections();

    // Get recent security events
    let recent_events = get_recent_events();

    // Get top blocked IPs (from iptables logs if available)
    let top_blocked = get_top_blocked_ips();

    // Get recent SSH sessions
    let ssh_sessions = get_ssh_sessions();

    Ok(Json(serde_json::to_value(SecurityOverview {
        firewall_drops_24h: firewall_drops,
        blocklist_hits,
        failed_ssh_attempts_24h: failed_ssh,
        active_connections,
        recent_events,
        top_blocked_ips: top_blocked,
        ssh_sessions,
    }).unwrap()))
}

fn get_firewall_drops() -> u64 {
    // Get drop count from iptables INPUT chain
    let output = Command::new("sudo")
        .args(["iptables", "-L", "INPUT", "-v", "-n"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        // Parse the policy line for DROP count
        for line in text.lines() {
            if line.contains("policy DROP") {
                // Format: Chain INPUT (policy DROP X packets, Y bytes)
                if let Some(start) = line.find("DROP ") {
                    let rest = &line[start + 5..];
                    if let Some(end) = rest.find(" packets") {
                        if let Ok(count) = rest[..end].parse::<u64>() {
                            return count;
                        }
                    }
                }
            }
        }
    }
    0
}

fn get_blocklist_hits() -> BlocklistHits {
    let mut hits = BlocklistHits {
        spamhaus_drop: 0,
        emerging_threats: 0,
    };

    // Get iptables rule counters for blocklists
    let output = Command::new("sudo")
        .args(["iptables", "-L", "INPUT", "-v", "-n"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("spamhaus-drop") {
                hits.spamhaus_drop = parse_packet_count(line);
            } else if line.contains("emerging-threats") {
                hits.emerging_threats = parse_packet_count(line);
            }
        }
    }

    hits
}

fn parse_packet_count(line: &str) -> u64 {
    // iptables -v format: pkts bytes target ...
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() > 0 {
        // First field is packet count (may have K/M/G suffix)
        let count_str = parts[0];
        if let Ok(count) = count_str.parse::<u64>() {
            return count;
        }
        // Handle K/M/G suffixes
        if count_str.ends_with('K') {
            if let Ok(n) = count_str.trim_end_matches('K').parse::<f64>() {
                return (n * 1000.0) as u64;
            }
        } else if count_str.ends_with('M') {
            if let Ok(n) = count_str.trim_end_matches('M').parse::<f64>() {
                return (n * 1_000_000.0) as u64;
            }
        }
    }
    0
}

fn get_failed_ssh_attempts() -> u64 {
    // Parse auth.log for actual failed SSH attempts (not sudo failures)
    // Exclude: localhost (127.0.0.1), our own grep commands
    let output = Command::new("sudo")
        .args(["grep", "-E", "sshd.*(Failed|Invalid user)", "/var/log/auth.log"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        // Count lines that are actual external failures (not localhost)
        text.lines()
            .filter(|line| !line.contains("127.0.0.1"))
            .count() as u64
    } else {
        0
    }
}

fn get_active_connections() -> u64 {
    // Try ss first (faster)
    let output = Command::new("ss")
        .args(["-t", "-n", "state", "established"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        // Count lines minus header
        let count = text.lines().count();
        if count > 0 {
            return (count - 1) as u64;
        }
    }
    0
}

fn get_recent_events() -> Vec<SecurityEvent> {
    let mut events = Vec::new();

    // Get recent auth events
    let output = Command::new("sudo")
        .args(["tail", "-100", "/var/log/auth.log"])
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines().rev().take(20) {
            if let Some(event) = parse_auth_event(line) {
                events.push(event);
            }
        }
    }

    events
}

fn is_internal_ip(ip: &str) -> bool {
    // LAN network is 10.22.22.x - this is trusted internal
    ip.starts_with("10.22.22.") || ip == "127.0.0.1" || ip == "N/A"
}

fn is_routerui_noise(line: &str) -> bool {
    // Filter out RouterUI's own monitoring commands
    line.contains("PWD=/opt/routerui") ||
    line.contains("/usr/sbin/iptables") ||
    line.contains("/usr/sbin/ipset") ||
    line.contains("/var/log/auth.log") ||
    line.contains("journalctl")
}

fn parse_auth_event(line: &str) -> Option<SecurityEvent> {
    // Parse syslog format: timestamp hostname service[pid]: message
    let lower = line.to_lowercase();

    // Skip RouterUI's own monitoring activity
    if is_routerui_noise(line) {
        return None;
    }

    // Skip all sudo session noise (these are internal system operations)
    if lower.contains("sudo:") && (lower.contains("session opened") || lower.contains("session closed")) {
        return None;
    }

    // Skip localhost session noise (Cloudflare tunnel creates lots of these)
    if lower.contains("127.0.0.1") && (lower.contains("session opened") || lower.contains("session closed")) {
        return None;
    }

    // Skip sudo commands without external context (internal system operations)
    if lower.contains("sudo") && lower.contains("command") && !lower.contains("192.168.") {
        return None;
    }

    let (event_type, severity) = if lower.contains("failed") || lower.contains("invalid user") {
        ("Failed Login", "high")
    } else if lower.contains("accepted") {
        ("Successful Login", "info")
    } else if lower.contains("session opened") && lower.contains("sshd") {
        ("SSH Session Opened", "info")
    } else if lower.contains("session closed") && lower.contains("sshd") {
        ("SSH Session Closed", "info")
    } else {
        return None;
    };

    // Extract timestamp (first part of line)
    let parts: Vec<&str> = line.splitn(4, ' ').collect();
    let timestamp = if parts.len() >= 1 {
        parts[0].to_string()
    } else {
        "Unknown".to_string()
    };

    // Try to extract IP
    let source_ip = extract_ip(line).unwrap_or_else(|| "N/A".to_string());

    // Determine if this is external (WAN side) or internal (LAN)
    // 192.168.12.x = WAN (T-Mobile router side)
    // 10.22.22.x = LAN (internal trusted network)
    let is_external = !is_internal_ip(&source_ip);

    // Get the message part
    let details = if let Some(idx) = line.find("]: ") {
        line[idx + 3..].to_string()
    } else {
        line.to_string()
    };

    Some(SecurityEvent {
        timestamp,
        event_type: event_type.to_string(),
        source_ip,
        details: details.chars().take(100).collect(),
        severity: severity.to_string(),
        is_external,
    })
}

fn extract_ip(line: &str) -> Option<String> {
    // Look for "from X.X.X.X" or "src=X.X.X.X" patterns
    if let Some(idx) = line.find("from ") {
        let rest = &line[idx + 5..];
        let ip: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if ip.contains('.') {
            return Some(ip);
        }
    }
    if let Some(idx) = line.find("src=") {
        let rest = &line[idx + 4..];
        let ip: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if ip.contains('.') {
            return Some(ip);
        }
    }
    None
}

fn get_top_blocked_ips() -> Vec<BlockedIP> {
    // Since we don't have detailed iptables logging enabled,
    // return empty for now. This could be populated if LOG rules are added.
    Vec::new()
}

fn get_ssh_sessions() -> Vec<SshSession> {
    let mut sessions = Vec::new();

    // Get who is currently logged in
    let output = Command::new("who")
        .output()
        .ok();

    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                let user = parts[0].to_string();
                let source = if parts.len() >= 5 {
                    parts[4].trim_matches(|c| c == '(' || c == ')').to_string()
                } else {
                    "local".to_string()
                };
                let timestamp = format!("{} {}", parts.get(2).unwrap_or(&""), parts.get(3).unwrap_or(&""));

                sessions.push(SshSession {
                    user,
                    source_ip: source,
                    timestamp,
                    status: "Active".to_string(),
                });
            }
        }
    }

    sessions
}

// Additional endpoint for live feed
pub async fn live_feed(
    AuthUser(_user): AuthUser,
) -> Result<Json<Vec<SecurityEvent>>, (StatusCode, String)> {
    let events = get_recent_events();
    Ok(Json(events))
}

// Endpoint for connection details
#[derive(Debug, Serialize)]
pub struct ConnectionInfo {
    pub local_addr: String,
    pub remote_addr: String,
    pub state: String,
    pub process: String,
}

pub async fn connections(
    AuthUser(_user): AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::security::connections()));
    }

    let mut connections = Vec::new();

    let output = Command::new("ss")
        .args(["-t", "-n", "-p", "state", "established"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let local = parts[3].to_string();
            let remote = parts[4].to_string();
            let process = parts.get(5).unwrap_or(&"").to_string();

            connections.push(ConnectionInfo {
                local_addr: local,
                remote_addr: remote,
                state: "ESTABLISHED".to_string(),
                process: process.trim_matches(|c| c == '"').to_string(),
            });
        }
    }

    Ok(Json(serde_json::to_value(connections).unwrap()))
}
