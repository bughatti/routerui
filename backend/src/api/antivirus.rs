use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::path::Path;

const QUARANTINE_DIR: &str = "/opt/routerui/quarantine";
const SCAN_LOG_DIR: &str = "/opt/routerui/scan-logs";

// ============ DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct AntivirusStatus {
    pub installed: bool,
    pub daemon_running: bool,
    pub version: String,
    pub signature_version: String,
    pub signature_date: String,
    pub signature_count: u64,
    pub last_update: String,
    pub quarantine_count: u32,
}

#[derive(Debug, Serialize)]
pub struct ScanResult {
    pub id: String,
    pub path: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub status: String, // "running", "completed", "error"
    pub files_scanned: u32,
    pub threats_found: u32,
    pub threats: Vec<ThreatInfo>,
    pub duration_secs: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ThreatInfo {
    pub file_path: String,
    pub threat_name: String,
    pub action_taken: String, // "quarantined", "deleted", "none"
}

#[derive(Debug, Serialize)]
pub struct QuarantineEntry {
    pub id: String,
    pub original_path: String,
    pub threat_name: String,
    pub quarantined_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub path: String,
    pub quarantine: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct QuarantineAction {
    pub id: String,
    pub action: String, // "delete", "restore"
}

#[derive(Debug, Serialize, Deserialize)]
struct ScanLogEntry {
    id: String,
    path: String,
    started_at: String,
    completed_at: Option<String>,
    status: String,
    files_scanned: u32,
    threats_found: u32,
    threats: Vec<ThreatInfo>,
}

// ============ HELPER FUNCTIONS ============

fn ensure_dirs() {
    let _ = fs::create_dir_all(QUARANTINE_DIR);
    let _ = fs::create_dir_all(SCAN_LOG_DIR);
}

fn get_clamav_version() -> (String, String, String, u64) {
    // Get ClamAV version
    let version_output = Command::new("clamscan")
        .args(["--version"])
        .output();

    let version = version_output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // Parse signature info from version string
    // Format: ClamAV 1.4.3/27881/Thu Jan 15 03:27:34 2026
    let mut sig_version = "Unknown".to_string();
    let mut sig_date = "Unknown".to_string();
    let mut sig_count: u64 = 0;

    if version.contains('/') {
        let parts: Vec<&str> = version.split('/').collect();
        if parts.len() >= 2 {
            sig_version = parts[1].to_string();
        }
        if parts.len() >= 3 {
            sig_date = parts[2].to_string();
        }
    }

    // Get signature count from database files
    let db_files = ["/var/lib/clamav/main.cvd", "/var/lib/clamav/daily.cvd", "/var/lib/clamav/bytecode.cvd"];
    for db_file in &db_files {
        if let Ok(output) = Command::new("sigtool")
            .args(["--info", db_file])
            .output()
        {
            let info = String::from_utf8_lossy(&output.stdout);
            for line in info.lines() {
                if line.starts_with("Signatures:") {
                    if let Some(count_str) = line.split(':').nth(1) {
                        if let Ok(count) = count_str.trim().parse::<u64>() {
                            sig_count += count;
                        }
                    }
                }
            }
        }
    }

    // Get last update time from freshclam log or file mtime
    let last_update = fs::metadata("/var/lib/clamav/daily.cvd")
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "Unknown".to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string());

    (version, sig_version, last_update, sig_count)
}

fn is_daemon_running() -> bool {
    Command::new("systemctl")
        .args(["is-active", "clamav-daemon"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

fn count_quarantine() -> u32 {
    fs::read_dir(QUARANTINE_DIR)
        .map(|entries| entries.count() as u32)
        .unwrap_or(0)
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{:x}", timestamp)
}

fn load_scan_history() -> Vec<ScanLogEntry> {
    let history_file = format!("{}/history.json", SCAN_LOG_DIR);
    fs::read_to_string(history_file)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_scan_history(history: &[ScanLogEntry]) -> Result<(), std::io::Error> {
    ensure_dirs();
    let history_file = format!("{}/history.json", SCAN_LOG_DIR);
    let json = serde_json::to_string_pretty(history)?;
    fs::write(history_file, json)
}

// ============ API ENDPOINTS ============

// Get antivirus status
pub async fn status() -> Result<Json<AntivirusStatus>, (StatusCode, String)> {
    let installed = Command::new("which")
        .args(["clamscan"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let (version, sig_version, last_update, sig_count) = get_clamav_version();
    let daemon_running = is_daemon_running();
    let quarantine_count = count_quarantine();

    // Get signature date from version string
    let sig_date = version.split('/').nth(2).unwrap_or("Unknown").to_string();

    Ok(Json(AntivirusStatus {
        installed,
        daemon_running,
        version: version.split('/').next().unwrap_or("Unknown").to_string(),
        signature_version: sig_version,
        signature_date: sig_date,
        signature_count: sig_count,
        last_update,
        quarantine_count,
    }))
}

// Update virus signatures
pub async fn update_signatures() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Stop freshclam service temporarily
    let _ = Command::new("sudo")
        .args(["systemctl", "stop", "clamav-freshclam"])
        .output();

    // Run freshclam
    let output = Command::new("sudo")
        .args(["freshclam"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Restart freshclam service
    let _ = Command::new("sudo")
        .args(["systemctl", "start", "clamav-freshclam"])
        .output();

    let success = output.status.success();
    let message = String::from_utf8_lossy(&output.stdout).to_string();

    Ok(Json(serde_json::json!({
        "success": success,
        "message": message
    })))
}

// Start a scan
pub async fn start_scan(
    Json(payload): Json<ScanRequest>,
) -> Result<Json<ScanResult>, (StatusCode, String)> {
    ensure_dirs();

    let scan_id = generate_id();
    let path = payload.path.clone();
    let quarantine = payload.quarantine.unwrap_or(true);

    // Validate path exists
    if !Path::new(&path).exists() {
        return Err((StatusCode::BAD_REQUEST, format!("Path does not exist: {}", path)));
    }

    let started_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Build clamscan command
    let mut args = vec![
        "-r".to_string(),           // Recursive
        "--infected".to_string(),   // Only show infected files
        "--no-summary".to_string(), // We'll parse our own summary
    ];

    if quarantine {
        args.push("--move".to_string());
        args.push(QUARANTINE_DIR.to_string());
    }
    args.push(path.clone());

    // Run scan
    let output = Command::new("sudo")
        .args(["clamscan"])
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Parse results
    let mut threats = Vec::new();
    let mut files_scanned: u32 = 0;

    for line in stdout.lines() {
        if line.contains(": ") && line.contains("FOUND") {
            let parts: Vec<&str> = line.splitn(2, ": ").collect();
            if parts.len() == 2 {
                let file_path = parts[0].to_string();
                let threat_name = parts[1].replace(" FOUND", "").to_string();
                threats.push(ThreatInfo {
                    file_path,
                    threat_name,
                    action_taken: if quarantine { "quarantined".to_string() } else { "none".to_string() },
                });
            }
        }
    }

    // Try to get file count from stderr (clamscan outputs stats there)
    for line in stderr.lines() {
        if line.contains("Scanned files:") {
            if let Some(count_str) = line.split(':').nth(1) {
                files_scanned = count_str.trim().parse().unwrap_or(0);
            }
        }
    }

    // If we couldn't parse the count, estimate based on scan
    if files_scanned == 0 {
        // Count files in path
        if let Ok(output) = Command::new("find")
            .args([&path, "-type", "f"])
            .output()
        {
            files_scanned = String::from_utf8_lossy(&output.stdout).lines().count() as u32;
        }
    }

    let completed_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let threats_found = threats.len() as u32;

    // Calculate duration
    let duration_secs = Some(0u32); // TODO: calculate actual duration

    let result = ScanResult {
        id: scan_id.clone(),
        path: path.clone(),
        started_at: started_at.clone(),
        completed_at: Some(completed_at.clone()),
        status: "completed".to_string(),
        files_scanned,
        threats_found,
        threats: threats.clone(),
        duration_secs,
    };

    // Save to history
    let mut history = load_scan_history();
    history.insert(0, ScanLogEntry {
        id: scan_id,
        path,
        started_at,
        completed_at: Some(completed_at),
        status: "completed".to_string(),
        files_scanned,
        threats_found,
        threats,
    });

    // Keep only last 50 scans
    history.truncate(50);
    let _ = save_scan_history(&history);

    Ok(Json(result))
}

// Get scan history
pub async fn scan_history() -> Result<Json<Vec<ScanResult>>, (StatusCode, String)> {
    let history = load_scan_history();

    let results: Vec<ScanResult> = history
        .into_iter()
        .map(|entry| ScanResult {
            id: entry.id,
            path: entry.path,
            started_at: entry.started_at,
            completed_at: entry.completed_at,
            status: entry.status,
            files_scanned: entry.files_scanned,
            threats_found: entry.threats_found,
            threats: entry.threats,
            duration_secs: None,
        })
        .collect();

    Ok(Json(results))
}

// Get quarantine list
pub async fn quarantine_list() -> Result<Json<Vec<QuarantineEntry>>, (StatusCode, String)> {
    ensure_dirs();

    let mut entries = Vec::new();

    if let Ok(dir) = fs::read_dir(QUARANTINE_DIR) {
        for entry in dir.flatten() {
            let path = entry.path();
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let metadata = entry.metadata().ok();
            let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
            let quarantined_at = metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| {
                    chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default()
                })
                .unwrap_or_default();

            entries.push(QuarantineEntry {
                id: filename.clone(),
                original_path: filename.clone(), // Note: original path is lost in quarantine
                threat_name: "Unknown".to_string(),
                quarantined_at,
                size_bytes: size,
            });
        }
    }

    Ok(Json(entries))
}

// Handle quarantine action (delete or restore)
pub async fn quarantine_action(
    Json(payload): Json<QuarantineAction>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let quarantine_path = format!("{}/{}", QUARANTINE_DIR, payload.id);

    if !Path::new(&quarantine_path).exists() {
        return Err((StatusCode::NOT_FOUND, "File not found in quarantine".to_string()));
    }

    match payload.action.as_str() {
        "delete" => {
            Command::new("sudo")
                .args(["rm", "-f", &quarantine_path])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            Ok(Json(serde_json::json!({
                "success": true,
                "message": "File permanently deleted"
            })))
        }
        "restore" => {
            // For safety, restore to a "restored" directory
            let restore_dir = "/opt/routerui/restored";
            let _ = fs::create_dir_all(restore_dir);

            let restore_path = format!("{}/{}", restore_dir, payload.id);
            Command::new("sudo")
                .args(["mv", &quarantine_path, &restore_path])
                .output()
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("File restored to {}", restore_path)
            })))
        }
        _ => Err((StatusCode::BAD_REQUEST, "Invalid action. Use 'delete' or 'restore'".to_string()))
    }
}

// Quick scan common locations
pub async fn quick_scan() -> Result<Json<ScanResult>, (StatusCode, String)> {
    // Scan common user directories
    let paths = vec!["/home", "/tmp", "/var/tmp"];
    let combined_path = paths.join(" ");

    start_scan(Json(ScanRequest {
        path: "/home".to_string(),
        quarantine: Some(true),
    })).await
}

// Toggle daemon
pub async fn toggle_daemon(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let enable = payload.get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let action = if enable { "start" } else { "stop" };

    Command::new("sudo")
        .args(["systemctl", action, "clamav-daemon"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "daemon_running": enable
    })))
}
