use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::io::Write;
use chrono::Utc;

// ============ TRAFFIC MONITOR STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct TrafficStats {
    pub interfaces: Vec<InterfaceTraffic>,
}

#[derive(Debug, Serialize)]
pub struct InterfaceTraffic {
    pub name: String,
    pub total_rx: u64,
    pub total_tx: u64,
    pub hourly: Vec<TrafficPoint>,
    pub daily: Vec<TrafficPoint>,
    pub monthly: Vec<TrafficPoint>,
}

#[derive(Debug, Serialize)]
pub struct TrafficPoint {
    pub timestamp: String,
    pub rx: u64,
    pub tx: u64,
}

// ============ DIAGNOSTICS STRUCTURES ============

#[derive(Debug, Deserialize)]
pub struct PingRequest {
    pub host: String,
    pub count: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PingResult {
    pub host: String,
    pub success: bool,
    pub output: String,
    pub packets_sent: u32,
    pub packets_received: u32,
    pub packet_loss: f32,
    pub avg_latency: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct TracerouteRequest {
    pub host: String,
}

#[derive(Debug, Serialize)]
pub struct TracerouteResult {
    pub host: String,
    pub output: String,
    pub hops: Vec<TracerouteHop>,
}

#[derive(Debug, Serialize)]
pub struct TracerouteHop {
    pub hop: u32,
    pub host: String,
    pub ip: Option<String>,
    pub latency: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DnsLookupRequest {
    pub hostname: String,
    pub record_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DnsLookupResult {
    pub hostname: String,
    pub record_type: String,
    pub results: Vec<String>,
    pub output: String,
}

#[derive(Debug, Serialize)]
pub struct SpeedTestResult {
    pub running: bool,
    pub completed: bool,
    pub download_mbps: Option<f64>,
    pub upload_mbps: Option<f64>,
    pub ping_ms: Option<f64>,
    pub server: Option<String>,
    pub output: String,
}

// ============ SYSTEM LOGS STRUCTURES ============

#[derive(Debug, Deserialize)]
pub struct LogsRequest {
    pub unit: Option<String>,
    pub priority: Option<String>,
    pub lines: Option<u32>,
    pub since: Option<String>,
    pub grep: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogsResult {
    pub logs: String,
    pub line_count: usize,
}

#[derive(Debug, Serialize)]
pub struct LogUnit {
    pub name: String,
    pub description: String,
}

// ============ BACKUP/RESTORE STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub filename: String,
    pub created: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupData {
    pub version: String,
    pub created: String,
    pub hostname: String,
    pub configs: BackupConfigs,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupConfigs {
    pub dnsmasq: Option<String>,
    pub hostapd: Option<String>,
    pub iptables: Option<String>,
    pub static_leases: Option<String>,
    pub wol_devices: Option<String>,
    pub protection_whitelist: Option<String>,
}

// ============ TRAFFIC MONITOR ENDPOINTS ============

pub async fn traffic_stats() -> Result<Json<TrafficStats>, (StatusCode, String)> {
    let output = Command::new("vnstat")
        .args(["--json"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Ok(Json(TrafficStats { interfaces: vec![] }));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut interfaces = Vec::new();

    if let Some(ifaces) = json.get("interfaces").and_then(|v| v.as_array()) {
        for iface in ifaces {
            let name = iface.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();

            // Skip docker and tailscale interfaces for cleaner view
            if name.starts_with("docker") || name.starts_with("veth") || name == "lo" {
                continue;
            }

            let traffic = iface.get("traffic").unwrap_or(&serde_json::Value::Null);

            let total = traffic.get("total").unwrap_or(&serde_json::Value::Null);
            let total_rx = total.get("rx").and_then(|v| v.as_u64()).unwrap_or(0);
            let total_tx = total.get("tx").and_then(|v| v.as_u64()).unwrap_or(0);

            // Parse hourly data
            let hourly = parse_traffic_array(traffic.get("hour"));
            let daily = parse_traffic_array(traffic.get("day"));
            let monthly = parse_traffic_array(traffic.get("month"));

            interfaces.push(InterfaceTraffic {
                name,
                total_rx,
                total_tx,
                hourly,
                daily,
                monthly,
            });
        }
    }

    Ok(Json(TrafficStats { interfaces }))
}

fn parse_traffic_array(arr: Option<&serde_json::Value>) -> Vec<TrafficPoint> {
    let mut points = Vec::new();

    if let Some(arr) = arr.and_then(|v| v.as_array()) {
        for item in arr.iter().take(24) { // Limit to 24 entries
            let date = item.get("date").unwrap_or(&serde_json::Value::Null);
            let time = item.get("time").unwrap_or(&serde_json::Value::Null);

            let year = date.get("year").and_then(|v| v.as_u64()).unwrap_or(0);
            let month = date.get("month").and_then(|v| v.as_u64()).unwrap_or(0);
            let day = date.get("day").and_then(|v| v.as_u64()).unwrap_or(0);
            let hour = time.get("hour").and_then(|v| v.as_u64()).unwrap_or(0);
            let minute = time.get("minute").and_then(|v| v.as_u64()).unwrap_or(0);

            let timestamp = format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, minute);

            let rx = item.get("rx").and_then(|v| v.as_u64()).unwrap_or(0);
            let tx = item.get("tx").and_then(|v| v.as_u64()).unwrap_or(0);

            if rx > 0 || tx > 0 {
                points.push(TrafficPoint { timestamp, rx, tx });
            }
        }
    }

    points
}

// ============ DIAGNOSTICS ENDPOINTS ============

pub async fn ping(Json(payload): Json<PingRequest>) -> Result<Json<PingResult>, (StatusCode, String)> {
    // Validate host (prevent command injection)
    if !payload.host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == ':') {
        return Err((StatusCode::BAD_REQUEST, "Invalid hostname".to_string()));
    }

    let count = payload.count.unwrap_or(4).min(20);
    let count_str = count.to_string();

    let output = Command::new("ping")
        .args(["-c", &count_str, "-W", "2", &payload.host])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let success = output.status.success();

    // Parse ping output
    let mut packets_sent = count;
    let mut packets_received = 0u32;
    let mut avg_latency = None;

    for line in stdout.lines() {
        if line.contains("packets transmitted") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                packets_sent = parts[0].parse().unwrap_or(count);
                packets_received = parts[3].parse().unwrap_or(0);
            }
        }
        if line.contains("rtt min/avg/max") || line.contains("round-trip min/avg/max") {
            if let Some(stats) = line.split('=').nth(1) {
                let values: Vec<&str> = stats.split('/').collect();
                if values.len() >= 2 {
                    avg_latency = values[1].trim().parse().ok();
                }
            }
        }
    }

    let packet_loss = if packets_sent > 0 {
        ((packets_sent - packets_received) as f32 / packets_sent as f32) * 100.0
    } else {
        100.0
    };

    Ok(Json(PingResult {
        host: payload.host,
        success,
        output: stdout,
        packets_sent,
        packets_received,
        packet_loss,
        avg_latency,
    }))
}

pub async fn traceroute(Json(payload): Json<TracerouteRequest>) -> Result<Json<TracerouteResult>, (StatusCode, String)> {
    // Validate host
    if !payload.host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == ':') {
        return Err((StatusCode::BAD_REQUEST, "Invalid hostname".to_string()));
    }

    let output = Command::new("traceroute")
        .args(["-m", "20", "-w", "2", &payload.host])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Parse traceroute output
    let mut hops = Vec::new();
    for line in stdout.lines().skip(1) { // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let hop: u32 = parts[0].parse().unwrap_or(0);
        if hop == 0 {
            continue;
        }

        let (host, ip, latency) = if parts.len() > 1 && parts[1] == "*" {
            ("*".to_string(), None, None)
        } else if parts.len() >= 3 {
            let h = parts[1].to_string();
            let i = parts.get(2).map(|s| s.trim_matches(|c| c == '(' || c == ')').to_string());
            let l = parts.get(3).map(|s| s.to_string());
            (h, i, l)
        } else {
            ("*".to_string(), None, None)
        };

        hops.push(TracerouteHop { hop, host, ip, latency });
    }

    Ok(Json(TracerouteResult {
        host: payload.host,
        output: stdout,
        hops,
    }))
}

pub async fn dns_lookup(Json(payload): Json<DnsLookupRequest>) -> Result<Json<DnsLookupResult>, (StatusCode, String)> {
    // Validate hostname
    if !payload.hostname.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-') {
        return Err((StatusCode::BAD_REQUEST, "Invalid hostname".to_string()));
    }

    let record_type = payload.record_type.unwrap_or_else(|| "A".to_string());

    // Validate record type
    let valid_types = ["A", "AAAA", "MX", "NS", "TXT", "CNAME", "SOA", "PTR"];
    if !valid_types.contains(&record_type.to_uppercase().as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid record type".to_string()));
    }

    let output = Command::new("dig")
        .args(["+short", &record_type.to_uppercase(), &payload.hostname])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let results: Vec<String> = stdout.lines().map(|s| s.to_string()).filter(|s| !s.is_empty()).collect();

    Ok(Json(DnsLookupResult {
        hostname: payload.hostname,
        record_type: record_type.to_uppercase(),
        results,
        output: stdout,
    }))
}

pub async fn speed_test() -> Result<Json<SpeedTestResult>, (StatusCode, String)> {
    // Run speedtest-cli
    let output = Command::new("speedtest-cli")
        .args(["--simple"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    let mut ping_ms = None;
    let mut download_mbps = None;
    let mut upload_mbps = None;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            match parts[0] {
                "Ping:" => ping_ms = parts[1].parse().ok(),
                "Download:" => download_mbps = parts[1].parse().ok(),
                "Upload:" => upload_mbps = parts[1].parse().ok(),
                _ => {}
            }
        }
    }

    Ok(Json(SpeedTestResult {
        running: false,
        completed: true,
        download_mbps,
        upload_mbps,
        ping_ms,
        server: None,
        output: stdout,
    }))
}

// ============ SYSTEM LOGS ENDPOINTS ============

pub async fn logs(Json(payload): Json<LogsRequest>) -> Result<Json<LogsResult>, (StatusCode, String)> {
    let mut args = vec!["--no-pager".to_string(), "-o".to_string(), "short-iso".to_string()];

    if let Some(unit) = &payload.unit {
        // Validate unit name
        if !unit.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err((StatusCode::BAD_REQUEST, "Invalid unit name".to_string()));
        }
        args.push("-u".to_string());
        args.push(unit.clone());
    }

    if let Some(priority) = &payload.priority {
        args.push("-p".to_string());
        args.push(priority.clone());
    }

    let lines = payload.lines.unwrap_or(100).min(1000);
    args.push("-n".to_string());
    args.push(lines.to_string());

    if let Some(since) = &payload.since {
        args.push("--since".to_string());
        args.push(since.clone());
    }

    let output = Command::new("journalctl")
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut logs = String::from_utf8_lossy(&output.stdout).to_string();

    // Apply grep filter if specified
    if let Some(grep) = &payload.grep {
        logs = logs.lines()
            .filter(|line| line.to_lowercase().contains(&grep.to_lowercase()))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let line_count = logs.lines().count();

    Ok(Json(LogsResult { logs, line_count }))
}

pub async fn log_units() -> Result<Json<Vec<LogUnit>>, (StatusCode, String)> {
    // Return list of common units to filter by
    let units = vec![
        LogUnit { name: "".to_string(), description: "All Logs".to_string() },
        LogUnit { name: "dnsmasq".to_string(), description: "DHCP & DNS".to_string() },
        LogUnit { name: "hostapd".to_string(), description: "WiFi AP".to_string() },
        LogUnit { name: "docker".to_string(), description: "Docker".to_string() },
        LogUnit { name: "AdGuardHome".to_string(), description: "AdGuard Home".to_string() },
        LogUnit { name: "sshd".to_string(), description: "SSH Server".to_string() },
        LogUnit { name: "cloudflared".to_string(), description: "Cloudflare Tunnel".to_string() },
        LogUnit { name: "tailscaled".to_string(), description: "Tailscale".to_string() },
        LogUnit { name: "clamav-daemon".to_string(), description: "ClamAV".to_string() },
        LogUnit { name: "NetworkManager".to_string(), description: "Network Manager".to_string() },
        LogUnit { name: "kernel".to_string(), description: "Kernel".to_string() },
    ];

    Ok(Json(units))
}

// ============ BACKUP/RESTORE ENDPOINTS ============

const BACKUP_DIR: &str = "/opt/routerui/backups";

pub async fn create_backup() -> Result<Json<BackupInfo>, (StatusCode, String)> {
    // Ensure backup directory exists
    fs::create_dir_all(BACKUP_DIR)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Read all config files
    let dnsmasq = fs::read_to_string("/etc/dnsmasq.d/router.conf").ok();
    let hostapd = fs::read_to_string("/etc/hostapd/hostapd.conf").ok();
    let static_leases = fs::read_to_string("/etc/dnsmasq.d/static-leases.conf").ok();
    let wol_devices = fs::read_to_string("/opt/routerui/wol-devices.json").ok();
    let protection_whitelist = fs::read_to_string("/opt/routerui/protection-whitelist.json").ok();

    // Get iptables rules
    let iptables = Command::new("iptables-save")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string());

    // Get hostname
    let hostname = Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "router".to_string());

    let backup = BackupData {
        version: "1.0".to_string(),
        created: Utc::now().to_rfc3339(),
        hostname,
        configs: BackupConfigs {
            dnsmasq,
            hostapd,
            iptables,
            static_leases,
            wol_devices,
            protection_whitelist,
        },
    };

    // Create filename with timestamp
    let filename = format!("backup_{}.json", Utc::now().format("%Y%m%d_%H%M%S"));
    let filepath = format!("{}/{}", BACKUP_DIR, filename);

    // Write backup
    let json = serde_json::to_string_pretty(&backup)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fs::write(&filepath, &json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let size = json.len() as u64;

    Ok(Json(BackupInfo {
        filename,
        created: backup.created,
        size,
    }))
}

pub async fn list_backups() -> Result<Json<Vec<BackupInfo>>, (StatusCode, String)> {
    let mut backups = Vec::new();

    if let Ok(entries) = fs::read_dir(BACKUP_DIR) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(metadata) = entry.metadata() {
                    let filename = entry.file_name().to_string_lossy().to_string();
                    let created = metadata.modified()
                        .map(|t| {
                            let datetime: chrono::DateTime<Utc> = t.into();
                            datetime.to_rfc3339()
                        })
                        .unwrap_or_default();

                    backups.push(BackupInfo {
                        filename,
                        created,
                        size: metadata.len(),
                    });
                }
            }
        }
    }

    // Sort by filename (which includes timestamp) descending
    backups.sort_by(|a, b| b.filename.cmp(&a.filename));

    Ok(Json(backups))
}

pub async fn download_backup(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<BackupData>, (StatusCode, String)> {
    let filename = payload.get("filename")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing filename".to_string()))?;

    // Validate filename (prevent path traversal)
    if filename.contains("..") || filename.contains('/') {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename".to_string()));
    }

    let filepath = format!("{}/{}", BACKUP_DIR, filename);
    let content = fs::read_to_string(&filepath)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let backup: BackupData = serde_json::from_str(&content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(backup))
}

pub async fn restore_backup(
    Json(payload): Json<BackupConfigs>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut restored = Vec::new();
    let mut errors = Vec::new();

    // Restore dnsmasq config
    if let Some(config) = &payload.dnsmasq {
        match fs::write("/etc/dnsmasq.d/router.conf", config) {
            Ok(_) => restored.push("dnsmasq"),
            Err(e) => errors.push(format!("dnsmasq: {}", e)),
        }
    }

    // Restore hostapd config
    if let Some(config) = &payload.hostapd {
        match fs::write("/etc/hostapd/hostapd.conf", config) {
            Ok(_) => restored.push("hostapd"),
            Err(e) => errors.push(format!("hostapd: {}", e)),
        }
    }

    // Restore static leases
    if let Some(config) = &payload.static_leases {
        match fs::write("/etc/dnsmasq.d/static-leases.conf", config) {
            Ok(_) => restored.push("static_leases"),
            Err(e) => errors.push(format!("static_leases: {}", e)),
        }
    }

    // Restore WOL devices
    if let Some(config) = &payload.wol_devices {
        match fs::write("/opt/routerui/wol-devices.json", config) {
            Ok(_) => restored.push("wol_devices"),
            Err(e) => errors.push(format!("wol_devices: {}", e)),
        }
    }

    // Restore protection whitelist
    if let Some(config) = &payload.protection_whitelist {
        match fs::write("/opt/routerui/protection-whitelist.json", config) {
            Ok(_) => restored.push("protection_whitelist"),
            Err(e) => errors.push(format!("protection_whitelist: {}", e)),
        }
    }

    // Restore iptables (requires special handling)
    if let Some(rules) = &payload.iptables {
        let mut child = Command::new("iptables-restore")
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(rules.as_bytes());
        }

        match child.wait() {
            Ok(status) if status.success() => restored.push("iptables"),
            Ok(_) => errors.push("iptables: restore failed".to_string()),
            Err(e) => errors.push(format!("iptables: {}", e)),
        }
    }

    Ok(Json(serde_json::json!({
        "success": errors.is_empty(),
        "restored": restored,
        "errors": errors
    })))
}

pub async fn delete_backup(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let filename = payload.get("filename")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing filename".to_string()))?;

    // Validate filename
    if filename.contains("..") || filename.contains('/') {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename".to_string()));
    }

    let filepath = format!("{}/{}", BACKUP_DIR, filename);
    fs::remove_file(&filepath)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}
