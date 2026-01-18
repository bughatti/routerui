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

#[derive(Debug, Serialize)]
pub struct FeatureStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_installed: bool,
    pub is_running: bool,
    pub can_install: bool,
    pub install_method: String, // "apt", "docker", "script"
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfigRequest {
    pub wan_interface: String,
    pub lan_interface: String,
    pub wifi_interface: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct FeatureSelectionRequest {
    pub features: Vec<FeatureSelection>,
}

#[derive(Debug, Deserialize)]
pub struct FeatureSelection {
    pub id: String,
    pub enabled: bool,
    pub install: bool,
}

#[derive(Debug, Serialize)]
pub struct InstallProgress {
    pub feature_id: String,
    pub status: String, // "pending", "installing", "complete", "failed"
    pub progress: u8,
    pub message: String,
}

// ============ DETECTION FUNCTIONS ============

fn detect_command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn detect_service_status(service: &str) -> (bool, bool) {
    // Returns (is_installed, is_running)
    let installed = Command::new("systemctl")
        .args(["list-unit-files", &format!("{}.service", service)])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(service))
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", service])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    (installed, running)
}

fn detect_docker_container(name: &str) -> (bool, bool) {
    // Returns (exists, is_running)
    let exists = Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l == name))
        .unwrap_or(false);

    let running = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l == name))
        .unwrap_or(false);

    (exists, running)
}

fn detect_port_listening(port: u16) -> bool {
    Command::new("ss")
        .args(["-tlnp"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{}", port)))
        .unwrap_or(false)
}

// ============ API ENDPOINTS ============

/// Check if setup is complete
pub async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SetupStatus>, (StatusCode, String)> {
    // Check for setup_config table
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
            total_steps: 5,
        }));
    }

    // Check if setup_complete flag is set
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
        current_step: if setup_complete { 5 } else { 1 },
        total_steps: 5,
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

/// Get available features and their installation status
pub async fn get_features() -> Result<Json<Vec<FeatureStatus>>, (StatusCode, String)> {
    let mut features = Vec::new();

    // Docker
    let docker_installed = detect_command_exists("docker");
    let (_, docker_running) = detect_service_status("docker");
    features.push(FeatureStatus {
        id: "docker".to_string(),
        name: "Docker".to_string(),
        description: "Container management for running services".to_string(),
        category: "Core".to_string(),
        is_installed: docker_installed,
        is_running: docker_running,
        can_install: true,
        install_method: "apt".to_string(),
    });

    // Tailscale
    let tailscale_installed = detect_command_exists("tailscale");
    let (_, tailscale_running) = detect_service_status("tailscaled");
    features.push(FeatureStatus {
        id: "tailscale".to_string(),
        name: "Tailscale VPN".to_string(),
        description: "Mesh VPN for secure remote access".to_string(),
        category: "VPN".to_string(),
        is_installed: tailscale_installed,
        is_running: tailscale_running,
        can_install: true,
        install_method: "script".to_string(),
    });

    // Gluetun (Docker-based VPN)
    let (gluetun_exists, gluetun_running) = detect_docker_container("gluetun");
    features.push(FeatureStatus {
        id: "gluetun".to_string(),
        name: "Gluetun VPN Client".to_string(),
        description: "VPN client for NordVPN, Mullvad, etc.".to_string(),
        category: "VPN".to_string(),
        is_installed: gluetun_exists,
        is_running: gluetun_running,
        can_install: docker_installed,
        install_method: "docker".to_string(),
    });

    // AdGuard Home
    let (adguard_installed, adguard_running) = detect_service_status("AdGuardHome");
    features.push(FeatureStatus {
        id: "adguard".to_string(),
        name: "AdGuard Home".to_string(),
        description: "Network-wide ad blocking and DNS".to_string(),
        category: "DNS".to_string(),
        is_installed: adguard_installed,
        is_running: adguard_running,
        can_install: true,
        install_method: "script".to_string(),
    });

    // dnsmasq
    let (dnsmasq_installed, dnsmasq_running) = detect_service_status("dnsmasq");
    features.push(FeatureStatus {
        id: "dnsmasq".to_string(),
        name: "dnsmasq".to_string(),
        description: "DHCP and DNS server".to_string(),
        category: "Core".to_string(),
        is_installed: dnsmasq_installed,
        is_running: dnsmasq_running,
        can_install: true,
        install_method: "apt".to_string(),
    });

    // ClamAV
    let clamav_installed = detect_command_exists("clamscan");
    let (_, clamav_running) = detect_service_status("clamav-daemon");
    features.push(FeatureStatus {
        id: "clamav".to_string(),
        name: "ClamAV Antivirus".to_string(),
        description: "Open-source antivirus scanner".to_string(),
        category: "Security".to_string(),
        is_installed: clamav_installed,
        is_running: clamav_running,
        can_install: true,
        install_method: "apt".to_string(),
    });

    // Radarr
    let radarr_running = detect_port_listening(7878) || detect_docker_container("radarr").1;
    features.push(FeatureStatus {
        id: "radarr".to_string(),
        name: "Radarr".to_string(),
        description: "Movie collection manager".to_string(),
        category: "Media".to_string(),
        is_installed: radarr_running,
        is_running: radarr_running,
        can_install: docker_installed,
        install_method: "docker".to_string(),
    });

    // Sonarr
    let sonarr_running = detect_port_listening(8989) || detect_docker_container("sonarr").1;
    features.push(FeatureStatus {
        id: "sonarr".to_string(),
        name: "Sonarr".to_string(),
        description: "TV show collection manager".to_string(),
        category: "Media".to_string(),
        is_installed: sonarr_running,
        is_running: sonarr_running,
        can_install: docker_installed,
        install_method: "docker".to_string(),
    });

    // Jellyfin
    let jellyfin_running = detect_port_listening(8096) || detect_docker_container("jellyfin").1;
    features.push(FeatureStatus {
        id: "jellyfin".to_string(),
        name: "Jellyfin".to_string(),
        description: "Media streaming server".to_string(),
        category: "Media".to_string(),
        is_installed: jellyfin_running,
        is_running: jellyfin_running,
        can_install: docker_installed,
        install_method: "docker".to_string(),
    });

    // Transmission
    let (transmission_exists, transmission_running) = detect_docker_container("transmission");
    let transmission_service = detect_service_status("transmission-daemon");
    features.push(FeatureStatus {
        id: "transmission".to_string(),
        name: "Transmission".to_string(),
        description: "BitTorrent client".to_string(),
        category: "Media".to_string(),
        is_installed: transmission_exists || transmission_service.0,
        is_running: transmission_running || transmission_service.1,
        can_install: true,
        install_method: "docker".to_string(),
    });

    Ok(Json(features))
}

/// Create admin account during setup
pub async fn create_admin(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateAdminRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Validate input
    if payload.username.len() < 3 {
        return Err((StatusCode::BAD_REQUEST, "Username must be at least 3 characters".to_string()));
    }
    if payload.password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "Password must be at least 6 characters".to_string()));
    }

    // Hash password
    let password_hash = crate::auth::hash_password(&payload.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Check if admin already exists
    let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE role = 'admin'")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    if existing > 0 {
        return Err((StatusCode::CONFLICT, "Admin account already exists".to_string()));
    }

    // Create admin user
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

/// Save network configuration
pub async fn save_network_config(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NetworkConfigRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Ensure setup_config table exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS setup_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"
    )
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Save network config
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

/// Save feature selection and trigger installations
pub async fn save_features(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<FeatureSelectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Ensure setup_config table exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS setup_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"
    )
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Save enabled features
    let enabled: Vec<String> = payload.features.iter()
        .filter(|f| f.enabled)
        .map(|f| f.id.clone())
        .collect();

    sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('enabled_features', ?)")
        .bind(serde_json::to_string(&enabled).unwrap())
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Get features to install
    let mut to_install: Vec<String> = payload.features.iter()
        .filter(|f| f.install)
        .map(|f| f.id.clone())
        .collect();

    // Docker-dependent features - if any are selected, ensure Docker is installed first
    let docker_dependent = ["gluetun", "radarr", "sonarr", "jellyfin", "transmission"];
    let needs_docker = to_install.iter().any(|f| docker_dependent.contains(&f.as_str()));
    let docker_installed = detect_command_exists("docker");

    if needs_docker && !docker_installed && !to_install.contains(&"docker".to_string()) {
        // Insert Docker at the beginning so it installs first
        to_install.insert(0, "docker".to_string());
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "enabled_features": enabled,
        "to_install": to_install
    })))
}

/// Install a specific feature
pub async fn install_feature(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<InstallProgress>, (StatusCode, String)> {
    let feature_id = payload.get("feature_id")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing feature_id".to_string()))?;

    let result = match feature_id {
        "docker" => install_docker().await,
        "tailscale" => install_tailscale().await,
        "adguard" => install_adguard().await,
        "dnsmasq" => install_dnsmasq().await,
        "clamav" => install_clamav().await,
        "gluetun" => install_gluetun().await,
        "radarr" => install_radarr().await,
        "sonarr" => install_sonarr().await,
        "jellyfin" => install_jellyfin().await,
        "transmission" => install_transmission().await,
        _ => Err(format!("Unknown feature: {}", feature_id)),
    };

    match result {
        Ok(msg) => Ok(Json(InstallProgress {
            feature_id: feature_id.to_string(),
            status: "complete".to_string(),
            progress: 100,
            message: msg,
        })),
        Err(msg) => Ok(Json(InstallProgress {
            feature_id: feature_id.to_string(),
            status: "failed".to_string(),
            progress: 0,
            message: msg,
        })),
    }
}

/// Complete setup
pub async fn complete(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Ensure setup_config table exists
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS setup_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )"
    )
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Mark setup as complete
    sqlx::query("INSERT OR REPLACE INTO setup_config (key, value) VALUES ('setup_complete', 'true')")
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Setup complete! You can now log in."
    })))
}

// ============ INSTALLATION FUNCTIONS ============

async fn install_docker() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && apt-get install -y docker.io docker-compose && systemctl enable docker && systemctl start docker"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Docker installed successfully".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_tailscale() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "curl -fsSL https://tailscale.com/install.sh | sh"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Tailscale installed successfully. Run 'tailscale up' to connect.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_adguard() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "curl -s -S -L https://raw.githubusercontent.com/AdguardTeam/AdGuardHome/master/scripts/install.sh | sh -s -- -v"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("AdGuard Home installed. Access setup at http://localhost:3000".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_dnsmasq() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && apt-get install -y dnsmasq"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("dnsmasq installed successfully".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_clamav() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && apt-get install -y clamav clamav-daemon && systemctl enable clamav-daemon && freshclam"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("ClamAV installed successfully".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_gluetun() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            docker pull qmcgaw/gluetun:latest && \
            docker run -d \
                --name=gluetun \
                --cap-add=NET_ADMIN \
                --device /dev/net/tun:/dev/net/tun \
                -e VPN_SERVICE_PROVIDER=nordvpn \
                -e VPN_TYPE=openvpn \
                -p 8888:8888/tcp \
                -p 8388:8388/tcp \
                -p 8388:8388/udp \
                --restart=unless-stopped \
                qmcgaw/gluetun
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Gluetun installed. Configure VPN credentials in container settings.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_radarr() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            docker pull lscr.io/linuxserver/radarr:latest && \
            docker run -d \
                --name=radarr \
                -e PUID=1000 \
                -e PGID=1000 \
                -e TZ=America/Denver \
                -p 7878:7878 \
                -v /opt/routerui/config/radarr:/config \
                -v /media/movies:/movies \
                -v /media/downloads:/downloads \
                --restart=unless-stopped \
                lscr.io/linuxserver/radarr:latest
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Radarr installed. Access at http://localhost:7878".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_sonarr() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            docker pull lscr.io/linuxserver/sonarr:latest && \
            docker run -d \
                --name=sonarr \
                -e PUID=1000 \
                -e PGID=1000 \
                -e TZ=America/Denver \
                -p 8989:8989 \
                -v /opt/routerui/config/sonarr:/config \
                -v /media/tv:/tv \
                -v /media/downloads:/downloads \
                --restart=unless-stopped \
                lscr.io/linuxserver/sonarr:latest
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Sonarr installed. Access at http://localhost:8989".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_jellyfin() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            docker pull lscr.io/linuxserver/jellyfin:latest && \
            docker run -d \
                --name=jellyfin \
                -e PUID=1000 \
                -e PGID=1000 \
                -e TZ=America/Denver \
                -p 8096:8096 \
                -v /opt/routerui/config/jellyfin:/config \
                -v /media/tv:/data/tvshows \
                -v /media/movies:/data/movies \
                --restart=unless-stopped \
                lscr.io/linuxserver/jellyfin:latest
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Jellyfin installed. Access at http://localhost:8096".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_transmission() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            docker pull lscr.io/linuxserver/transmission:latest && \
            docker run -d \
                --name=transmission \
                -e PUID=1000 \
                -e PGID=1000 \
                -e TZ=America/Denver \
                -p 9091:9091 \
                -p 51413:51413 \
                -p 51413:51413/udp \
                -v /opt/routerui/config/transmission:/config \
                -v /media/downloads:/downloads \
                -v /media/watch:/watch \
                --restart=unless-stopped \
                lscr.io/linuxserver/transmission:latest
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Transmission installed. Access at http://localhost:9091".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
