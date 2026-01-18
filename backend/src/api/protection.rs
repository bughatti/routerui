use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::collections::HashMap;

const BLOCKLISTS_DIR: &str = "/opt/routerui/blocklists";
const WHITELIST_FILE: &str = "/opt/routerui/protection-whitelist.json";
const GEOIP_DB: &str = "/opt/routerui/GeoLite2-Country.mmdb";

// ============ BLOCKLIST SOURCES ============

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocklistSource {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub enabled: bool,
    pub ip_count: u32,
    pub last_updated: Option<String>,
}

fn get_default_blocklists() -> Vec<BlocklistSource> {
    vec![
        BlocklistSource {
            id: "spamhaus-drop".to_string(),
            name: "Spamhaus DROP".to_string(),
            description: "Known hijacked/leased netblocks used for spam".to_string(),
            url: "https://www.spamhaus.org/drop/drop.txt".to_string(),
            enabled: false,
            ip_count: 0,
            last_updated: None,
        },
        BlocklistSource {
            id: "spamhaus-edrop".to_string(),
            name: "Spamhaus EDROP".to_string(),
            description: "Extended DROP list - additional hijacked blocks".to_string(),
            url: "https://www.spamhaus.org/drop/edrop.txt".to_string(),
            enabled: false,
            ip_count: 0,
            last_updated: None,
        },
        BlocklistSource {
            id: "emerging-threats".to_string(),
            name: "Emerging Threats".to_string(),
            description: "Known malicious IPs from intrusion detection".to_string(),
            url: "https://rules.emergingthreats.net/fwrules/emerging-Block-IPs.txt".to_string(),
            enabled: false,
            ip_count: 0,
            last_updated: None,
        },
        BlocklistSource {
            id: "firehol-level1".to_string(),
            name: "FireHOL Level 1".to_string(),
            description: "Basic protection - low false positive risk".to_string(),
            url: "https://iplists.firehol.org/files/firehol_level1.netset".to_string(),
            enabled: false,
            ip_count: 0,
            last_updated: None,
        },
        BlocklistSource {
            id: "abuse-ch-feodo".to_string(),
            name: "Feodo Tracker".to_string(),
            description: "Botnet C&C servers tracked by abuse.ch".to_string(),
            url: "https://feodotracker.abuse.ch/downloads/ipblocklist.txt".to_string(),
            enabled: false,
            ip_count: 0,
            last_updated: None,
        },
    ]
}

// ============ DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct ProtectionStatus {
    pub blocklists_active: u32,
    pub total_blocked_ips: u64,
    pub countries_blocked: u32,
    pub whitelist_count: u32,
    pub log_enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BlocklistsResponse {
    pub sources: Vec<BlocklistSource>,
    pub total_ips: u64,
}

#[derive(Debug, Deserialize)]
pub struct ToggleBlocklist {
    pub id: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct BlockedEntry {
    pub timestamp: String,
    pub direction: String,  // "inbound" or "outbound"
    pub src_ip: String,
    pub dst_ip: String,
    pub src_port: u16,
    pub dst_port: u16,
    pub protocol: String,
    pub interface: String,
    pub reason: String,     // which blocklist or rule blocked it
    pub country: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BlockedLogResponse {
    pub entries: Vec<BlockedEntry>,
    pub total_blocked_24h: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WhitelistEntry {
    pub ip: String,
    pub description: String,
    pub added_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AddWhitelist {
    pub ip: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveWhitelist {
    pub ip: String,
}

#[derive(Debug, Serialize)]
pub struct CountryBlock {
    pub code: String,
    pub name: String,
    pub blocked: bool,
}

#[derive(Debug, Deserialize)]
pub struct ToggleCountry {
    pub code: String,
    pub blocked: bool,
}

// ============ HELPER FUNCTIONS ============

fn ensure_dirs() {
    let _ = fs::create_dir_all(BLOCKLISTS_DIR);
}

fn get_ipset_count(name: &str) -> u32 {
    let output = Command::new("sudo")
        .args(["ipset", "list", name, "-t"])
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.starts_with("Number of entries:") {
                if let Some(num) = line.split(':').nth(1) {
                    return num.trim().parse().unwrap_or(0);
                }
            }
        }
    }
    0
}

fn ipset_exists(name: &str) -> bool {
    Command::new("sudo")
        .args(["ipset", "list", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn create_ipset(name: &str) -> Result<(), (StatusCode, String)> {
    if !ipset_exists(name) {
        Command::new("sudo")
            .args(["ipset", "create", name, "hash:net", "maxelem", "1000000"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    Ok(())
}

fn add_ipset_rule(set_name: &str) -> Result<(), (StatusCode, String)> {
    // Check if rule already exists
    let check = Command::new("sudo")
        .args(["iptables", "-C", "INPUT", "-m", "set", "--match-set", set_name, "src", "-j", "DROP"])
        .output();

    if check.map(|o| o.status.success()).unwrap_or(false) {
        return Ok(()); // Rule already exists
    }

    // Add the rule - log then drop
    Command::new("sudo")
        .args(["iptables", "-I", "INPUT", "1", "-m", "set", "--match-set", set_name, "src", "-j", "LOG",
               "--log-prefix", &format!("BLOCKED:{}: ", set_name), "--log-level", "4"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Command::new("sudo")
        .args(["iptables", "-I", "INPUT", "2", "-m", "set", "--match-set", set_name, "src", "-j", "DROP"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn remove_ipset_rule(set_name: &str) -> Result<(), (StatusCode, String)> {
    // Remove LOG rule
    let _ = Command::new("sudo")
        .args(["iptables", "-D", "INPUT", "-m", "set", "--match-set", set_name, "src", "-j", "LOG",
               "--log-prefix", &format!("BLOCKED:{}: ", set_name), "--log-level", "4"])
        .output();

    // Remove DROP rule
    let _ = Command::new("sudo")
        .args(["iptables", "-D", "INPUT", "-m", "set", "--match-set", set_name, "src", "-j", "DROP"])
        .output();

    Ok(())
}

fn load_whitelist() -> Vec<WhitelistEntry> {
    fs::read_to_string(WHITELIST_FILE)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_whitelist(entries: &[WhitelistEntry]) -> Result<(), (StatusCode, String)> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(WHITELIST_FILE, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

fn get_blocklist_state() -> HashMap<String, bool> {
    let state_file = format!("{}/state.json", BLOCKLISTS_DIR);
    fs::read_to_string(state_file)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_blocklist_state(state: &HashMap<String, bool>) -> Result<(), (StatusCode, String)> {
    ensure_dirs();
    let state_file = format!("{}/state.json", BLOCKLISTS_DIR);
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(state_file, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

// ============ API ENDPOINTS ============

use crate::mock;

// Get overall protection status
pub async fn status() -> Result<Json<ProtectionStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(ProtectionStatus {
            blocklists_active: 2,
            total_blocked_ips: 50000,
            countries_blocked: 0,
            whitelist_count: 3,
            log_enabled: true,
        }));
    }

    let state = get_blocklist_state();
    let active_lists = state.values().filter(|&&v| v).count() as u32;

    let mut total_ips: u64 = 0;
    for (id, &enabled) in &state {
        if enabled {
            total_ips += get_ipset_count(id) as u64;
        }
    }

    let whitelist = load_whitelist();

    // Check if logging is enabled (look for LOG rules)
    let log_check = Command::new("sudo")
        .args(["iptables", "-L", "INPUT", "-n"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("LOG"))
        .unwrap_or(false);

    Ok(Json(ProtectionStatus {
        blocklists_active: active_lists,
        total_blocked_ips: total_ips,
        countries_blocked: 0, // TODO: implement country counting
        whitelist_count: whitelist.len() as u32,
        log_enabled: log_check,
    }))
}

// Get all blocklist sources and their status
pub async fn blocklists() -> Result<Json<BlocklistsResponse>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        let sources = get_default_blocklists().into_iter().enumerate().map(|(i, mut s)| {
            s.enabled = i < 2;
            s.ip_count = if s.enabled { 25000 } else { 0 };
            s.last_updated = if s.enabled { Some("2026-01-18 10:00".to_string()) } else { None };
            s
        }).collect();
        return Ok(Json(BlocklistsResponse { sources, total_ips: 50000 }));
    }

    let state = get_blocklist_state();
    let mut sources = get_default_blocklists();
    let mut total: u64 = 0;

    for source in &mut sources {
        source.enabled = *state.get(&source.id).unwrap_or(&false);
        if source.enabled {
            source.ip_count = get_ipset_count(&source.id);
            total += source.ip_count as u64;

            // Check last update time from file
            let list_file = format!("{}/{}.txt", BLOCKLISTS_DIR, source.id);
            if let Ok(metadata) = fs::metadata(&list_file) {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let secs = duration.as_secs();
                        let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
                            .map(|d| d.format("%Y-%m-%d %H:%M").to_string());
                        source.last_updated = dt;
                    }
                }
            }
        }
    }

    Ok(Json(BlocklistsResponse {
        sources,
        total_ips: total,
    }))
}

// Toggle a blocklist on/off
pub async fn toggle_blocklist(
    Json(payload): Json<ToggleBlocklist>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    ensure_dirs();
    let mut state = get_blocklist_state();

    if payload.enabled {
        // Enable blocklist
        // 1. Create ipset
        create_ipset(&payload.id)?;

        // 2. Download and populate ipset
        let sources = get_default_blocklists();
        if let Some(source) = sources.iter().find(|s| s.id == payload.id) {
            let list_file = format!("{}/{}.txt", BLOCKLISTS_DIR, payload.id);

            // Download list
            let download = Command::new("curl")
                .args(["-s", "-o", &list_file, &source.url])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if !download.status.success() {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to download blocklist".to_string()));
            }

            // Parse and add IPs to ipset
            if let Ok(content) = fs::read_to_string(&list_file) {
                // Flush existing entries
                let _ = Command::new("sudo")
                    .args(["ipset", "flush", &payload.id])
                    .output();

                for line in content.lines() {
                    let line = line.trim();
                    // Skip comments and empty lines
                    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                        continue;
                    }
                    // Extract IP/CIDR (first field before any whitespace or semicolon)
                    if let Some(ip) = line.split(|c| c == ' ' || c == '\t' || c == ';').next() {
                        let ip = ip.trim();
                        if !ip.is_empty() && (ip.contains('.') || ip.contains(':')) {
                            let _ = Command::new("sudo")
                                .args(["ipset", "add", &payload.id, ip, "-exist"])
                                .output();
                        }
                    }
                }
            }
        }

        // 3. Add iptables rule
        add_ipset_rule(&payload.id)?;

        state.insert(payload.id.clone(), true);
    } else {
        // Disable blocklist
        remove_ipset_rule(&payload.id)?;

        // Destroy ipset
        let _ = Command::new("sudo")
            .args(["ipset", "destroy", &payload.id])
            .output();

        state.insert(payload.id.clone(), false);
    }

    save_blocklist_state(&state)?;

    // Save iptables rules
    let _ = Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output();

    Ok(Json(serde_json::json!({"success": true})))
}

// Update all enabled blocklists
pub async fn update_blocklists() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "updated": 2, "mock": true})));
    }

    let state = get_blocklist_state();
    let sources = get_default_blocklists();
    let mut updated = 0;

    for (id, &enabled) in &state {
        if enabled {
            if let Some(source) = sources.iter().find(|s| &s.id == id) {
                let list_file = format!("{}/{}.txt", BLOCKLISTS_DIR, id);

                // Download
                let _ = Command::new("curl")
                    .args(["-s", "-o", &list_file, &source.url])
                    .output();

                // Flush and repopulate
                let _ = Command::new("sudo")
                    .args(["ipset", "flush", id])
                    .output();

                if let Ok(content) = fs::read_to_string(&list_file) {
                    for line in content.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                            continue;
                        }
                        if let Some(ip) = line.split(|c| c == ' ' || c == '\t' || c == ';').next() {
                            let ip = ip.trim();
                            if !ip.is_empty() && (ip.contains('.') || ip.contains(':')) {
                                let _ = Command::new("sudo")
                                    .args(["ipset", "add", id, ip, "-exist"])
                                    .output();
                            }
                        }
                    }
                }
                updated += 1;
            }
        }
    }

    Ok(Json(serde_json::json!({"success": true, "updated": updated})))
}

// Get blocked traffic log
pub async fn blocked_log() -> Result<Json<BlockedLogResponse>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(BlockedLogResponse {
            entries: vec![
                BlockedEntry { timestamp: "2026-01-18T10:30:00".to_string(), direction: "inbound".to_string(), src_ip: "45.155.205.100".to_string(), dst_ip: "10.22.22.1".to_string(), src_port: 45678, dst_port: 22, protocol: "TCP".to_string(), interface: "enp1s0".to_string(), reason: "spamhaus-drop".to_string(), country: Some("RU".to_string()) },
                BlockedEntry { timestamp: "2026-01-18T10:29:00".to_string(), direction: "inbound".to_string(), src_ip: "192.168.1.100".to_string(), dst_ip: "10.22.22.1".to_string(), src_port: 12345, dst_port: 80, protocol: "TCP".to_string(), interface: "enp1s0".to_string(), reason: "emerging-threats".to_string(), country: Some("CN".to_string()) },
            ],
            total_blocked_24h: 156,
        }));
    }

    // Parse kernel log for blocked entries
    let output = Command::new("sudo")
        .args(["journalctl", "-k", "--since", "24 hours ago", "--no-pager", "-o", "short-iso"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let log = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in log.lines() {
        if !line.contains("BLOCKED:") {
            continue;
        }

        // Parse: timestamp ... BLOCKED:listname: ... SRC=x DST=y SPT=z DPT=w PROTO=p
        let mut entry = BlockedEntry {
            timestamp: String::new(),
            direction: "inbound".to_string(),
            src_ip: String::new(),
            dst_ip: String::new(),
            src_port: 0,
            dst_port: 0,
            protocol: String::new(),
            interface: String::new(),
            reason: String::new(),
            country: None,
        };

        // Extract timestamp (first part of line)
        if let Some(ts) = line.split_whitespace().next() {
            entry.timestamp = ts.to_string();
        }

        // Extract reason (blocklist name)
        if let Some(start) = line.find("BLOCKED:") {
            if let Some(end) = line[start..].find(':') {
                if let Some(end2) = line[start + end + 1..].find(':') {
                    entry.reason = line[start + end + 1..start + end + 1 + end2].to_string();
                }
            }
        }

        // Extract fields
        for part in line.split_whitespace() {
            if part.starts_with("SRC=") {
                entry.src_ip = part[4..].to_string();
            } else if part.starts_with("DST=") {
                entry.dst_ip = part[4..].to_string();
            } else if part.starts_with("SPT=") {
                entry.src_port = part[4..].parse().unwrap_or(0);
            } else if part.starts_with("DPT=") {
                entry.dst_port = part[4..].parse().unwrap_or(0);
            } else if part.starts_with("PROTO=") {
                entry.protocol = part[6..].to_string();
            } else if part.starts_with("IN=") {
                entry.interface = part[3..].to_string();
            }
        }

        // Determine direction based on interface
        if entry.interface == "enp1s0" {
            entry.direction = "inbound".to_string();
        } else {
            entry.direction = "outbound".to_string();
        }

        if !entry.src_ip.is_empty() {
            entries.push(entry);
        }
    }

    // Limit to most recent 100
    entries.reverse();
    entries.truncate(100);

    let total = entries.len() as u64;

    Ok(Json(BlockedLogResponse {
        entries,
        total_blocked_24h: total,
    }))
}

// Get whitelist
pub async fn whitelist() -> Result<Json<Vec<WhitelistEntry>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            WhitelistEntry { ip: "8.8.8.8".to_string(), description: "Google DNS".to_string(), added_at: "2026-01-15 12:00:00".to_string() },
            WhitelistEntry { ip: "1.1.1.1".to_string(), description: "Cloudflare DNS".to_string(), added_at: "2026-01-16 14:00:00".to_string() },
        ]));
    }

    Ok(Json(load_whitelist()))
}

// Add to whitelist
pub async fn add_whitelist(
    Json(payload): Json<AddWhitelist>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut entries = load_whitelist();

    // Check if already exists
    if entries.iter().any(|e| e.ip == payload.ip) {
        return Err((StatusCode::BAD_REQUEST, "IP already in whitelist".to_string()));
    }

    // Add to whitelist
    entries.push(WhitelistEntry {
        ip: payload.ip.clone(),
        description: payload.description.unwrap_or_default(),
        added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });

    save_whitelist(&entries)?;

    // Create whitelist ipset if doesn't exist
    create_ipset("protection-whitelist")?;

    // Add to ipset
    Command::new("sudo")
        .args(["ipset", "add", "protection-whitelist", &payload.ip, "-exist"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Ensure whitelist rule is at top of INPUT chain (ACCEPT before any DROP)
    let check = Command::new("sudo")
        .args(["iptables", "-C", "INPUT", "-m", "set", "--match-set", "protection-whitelist", "src", "-j", "ACCEPT"])
        .output();

    if !check.map(|o| o.status.success()).unwrap_or(false) {
        Command::new("sudo")
            .args(["iptables", "-I", "INPUT", "1", "-m", "set", "--match-set", "protection-whitelist", "src", "-j", "ACCEPT"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Save rules
    let _ = Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output();

    Ok(Json(serde_json::json!({"success": true})))
}

// Remove from whitelist
pub async fn remove_whitelist(
    Json(payload): Json<RemoveWhitelist>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut entries = load_whitelist();
    entries.retain(|e| e.ip != payload.ip);
    save_whitelist(&entries)?;

    // Remove from ipset
    let _ = Command::new("sudo")
        .args(["ipset", "del", "protection-whitelist", &payload.ip])
        .output();

    // Save rules
    let _ = Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output();

    Ok(Json(serde_json::json!({"success": true})))
}

// Quick-allow an IP from blocked log (adds to whitelist and removes from current session blocks)
pub async fn quick_allow(
    Json(payload): Json<AddWhitelist>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Add to whitelist
    add_whitelist(Json(payload)).await
}

// ============ COUNTRY BLOCKING ============

fn get_common_countries() -> Vec<CountryBlock> {
    vec![
        CountryBlock { code: "CN".to_string(), name: "China".to_string(), blocked: false },
        CountryBlock { code: "RU".to_string(), name: "Russia".to_string(), blocked: false },
        CountryBlock { code: "KP".to_string(), name: "North Korea".to_string(), blocked: false },
        CountryBlock { code: "IR".to_string(), name: "Iran".to_string(), blocked: false },
        CountryBlock { code: "BY".to_string(), name: "Belarus".to_string(), blocked: false },
        CountryBlock { code: "VN".to_string(), name: "Vietnam".to_string(), blocked: false },
        CountryBlock { code: "IN".to_string(), name: "India".to_string(), blocked: false },
        CountryBlock { code: "BR".to_string(), name: "Brazil".to_string(), blocked: false },
        CountryBlock { code: "NL".to_string(), name: "Netherlands".to_string(), blocked: false },
        CountryBlock { code: "DE".to_string(), name: "Germany".to_string(), blocked: false },
        CountryBlock { code: "FR".to_string(), name: "France".to_string(), blocked: false },
        CountryBlock { code: "GB".to_string(), name: "United Kingdom".to_string(), blocked: false },
        CountryBlock { code: "UA".to_string(), name: "Ukraine".to_string(), blocked: false },
        CountryBlock { code: "PK".to_string(), name: "Pakistan".to_string(), blocked: false },
        CountryBlock { code: "BD".to_string(), name: "Bangladesh".to_string(), blocked: false },
    ]
}

fn get_country_state() -> HashMap<String, bool> {
    let state_file = format!("{}/countries.json", BLOCKLISTS_DIR);
    fs::read_to_string(state_file)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_country_state(state: &HashMap<String, bool>) -> Result<(), (StatusCode, String)> {
    ensure_dirs();
    let state_file = format!("{}/countries.json", BLOCKLISTS_DIR);
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(state_file, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

// Get country block status
pub async fn countries() -> Result<Json<Vec<CountryBlock>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(get_common_countries()));
    }

    let state = get_country_state();
    let mut countries = get_common_countries();

    for country in &mut countries {
        country.blocked = *state.get(&country.code).unwrap_or(&false);
    }

    Ok(Json(countries))
}

// Toggle country blocking
pub async fn toggle_country(
    Json(payload): Json<ToggleCountry>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut state = get_country_state();
    let set_name = format!("country-{}", payload.code.to_lowercase());

    if payload.blocked {
        // Download country IP ranges from ipdeny.com
        let zone_url = format!("https://www.ipdeny.com/ipblocks/data/countries/{}.zone", payload.code.to_lowercase());
        let zone_file = format!("{}/{}.zone", BLOCKLISTS_DIR, payload.code.to_lowercase());

        // Download
        let download = Command::new("curl")
            .args(["-s", "-o", &zone_file, &zone_url])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if !download.status.success() {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to download country IP list".to_string()));
        }

        // Create ipset
        create_ipset(&set_name)?;

        // Flush and populate
        let _ = Command::new("sudo")
            .args(["ipset", "flush", &set_name])
            .output();

        if let Ok(content) = fs::read_to_string(&zone_file) {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    let _ = Command::new("sudo")
                        .args(["ipset", "add", &set_name, line, "-exist"])
                        .output();
                }
            }
        }

        // Add iptables rule
        add_ipset_rule(&set_name)?;
        state.insert(payload.code.clone(), true);
    } else {
        // Remove blocking
        remove_ipset_rule(&set_name)?;
        let _ = Command::new("sudo")
            .args(["ipset", "destroy", &set_name])
            .output();
        state.insert(payload.code.clone(), false);
    }

    save_country_state(&state)?;

    // Save iptables
    let _ = Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output();

    Ok(Json(serde_json::json!({"success": true})))
}

// Enable logging for blocked traffic (adds LOG rules before DROP rules)
pub async fn enable_logging() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    // Add a catch-all LOG rule for dropped packets
    // This will log any packet that's about to be dropped by the default policy

    // First check if we already have a general LOG rule
    let check = Command::new("sudo")
        .args(["iptables", "-C", "INPUT", "-j", "LOG", "--log-prefix", "BLOCKED:firewall: ", "--log-level", "4"])
        .output();

    if !check.map(|o| o.status.success()).unwrap_or(false) {
        // Add LOG rule before the end of INPUT chain (right before policy kicks in)
        // Get the rule count first
        let list = Command::new("sudo")
            .args(["iptables", "-L", "INPUT", "--line-numbers", "-n"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let lines = String::from_utf8_lossy(&list.stdout);
        let rule_count = lines.lines().count().saturating_sub(2) as u32;

        // Append LOG rule at the end (will trigger before default DROP policy)
        Command::new("sudo")
            .args(["iptables", "-A", "INPUT", "-j", "LOG", "--log-prefix", "BLOCKED:firewall: ", "--log-level", "4"])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // Save rules
    let _ = Command::new("sudo")
        .args(["netfilter-persistent", "save"])
        .output();

    Ok(Json(serde_json::json!({"success": true})))
}
