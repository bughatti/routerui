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

#[derive(Debug, Serialize, Clone)]
pub struct CoreService {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_installed: bool,
    pub is_running: bool,
    pub required: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct OptionalFeature {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub is_installed: bool,
    pub is_running: bool,
    pub requires_docker: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateAdminRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct NetworkConfigRequest {
    pub wan_interface: String,
    pub lan_interface: String,
    pub wifi_interface: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallResult {
    pub id: String,
    pub success: bool,
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

fn has_wireless_interface() -> bool {
    std::path::Path::new("/sys/class/net")
        .read_dir()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|entry| {
                let wireless_path = entry.path().join("wireless");
                wireless_path.exists()
            })
        })
        .unwrap_or(false)
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
            total_steps: 6,
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
        current_step: if setup_complete { 6 } else { 1 },
        total_steps: 6,
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

/// Get core services status
pub async fn get_core_services() -> Result<Json<Vec<CoreService>>, (StatusCode, String)> {
    let mut services = Vec::new();

    // dnsmasq - DHCP/DNS (always required)
    let (dnsmasq_installed, dnsmasq_running) = detect_service_status("dnsmasq");
    services.push(CoreService {
        id: "dnsmasq".to_string(),
        name: "dnsmasq".to_string(),
        description: "DHCP server and DNS forwarder".to_string(),
        is_installed: dnsmasq_installed,
        is_running: dnsmasq_running,
        required: true,
    });

    // iptables/nftables - Firewall (always required)
    let iptables_installed = detect_command_exists("iptables") || detect_command_exists("nft");
    services.push(CoreService {
        id: "firewall".to_string(),
        name: "Firewall (iptables)".to_string(),
        description: "Network firewall and NAT".to_string(),
        is_installed: iptables_installed,
        is_running: iptables_installed, // If installed, considered "running"
        required: true,
    });

    // hostapd - WiFi AP (only if wireless interface exists)
    if has_wireless_interface() {
        let (hostapd_installed, hostapd_running) = detect_service_status("hostapd");
        services.push(CoreService {
            id: "hostapd".to_string(),
            name: "hostapd".to_string(),
            description: "WiFi Access Point".to_string(),
            is_installed: hostapd_installed,
            is_running: hostapd_running,
            required: true,
        });
    }

    Ok(Json(services))
}

/// Get optional features
pub async fn get_features() -> Result<Json<Vec<OptionalFeature>>, (StatusCode, String)> {
    let mut features = Vec::new();
    let docker_installed = detect_command_exists("docker");

    // === Non-Docker Features ===

    // Docker itself
    let (_, docker_running) = detect_service_status("docker");
    features.push(OptionalFeature {
        id: "docker".to_string(),
        name: "Docker".to_string(),
        description: "Container runtime for additional services".to_string(),
        category: "Core".to_string(),
        is_installed: docker_installed,
        is_running: docker_running,
        requires_docker: false,
    });

    // AdGuard Home
    let (adguard_installed, adguard_running) = detect_service_status("AdGuardHome");
    features.push(OptionalFeature {
        id: "adguard".to_string(),
        name: "AdGuard Home".to_string(),
        description: "Network-wide ad blocking and enhanced DNS".to_string(),
        category: "DNS".to_string(),
        is_installed: adguard_installed,
        is_running: adguard_running,
        requires_docker: false,
    });

    // Tailscale
    let tailscale_installed = detect_command_exists("tailscale");
    let (_, tailscale_running) = detect_service_status("tailscaled");
    features.push(OptionalFeature {
        id: "tailscale".to_string(),
        name: "Tailscale".to_string(),
        description: "Mesh VPN for secure remote access".to_string(),
        category: "VPN".to_string(),
        is_installed: tailscale_installed,
        is_running: tailscale_running,
        requires_docker: false,
    });

    // ClamAV
    let clamav_installed = detect_command_exists("clamscan");
    let (_, clamav_running) = detect_service_status("clamav-daemon");
    features.push(OptionalFeature {
        id: "clamav".to_string(),
        name: "ClamAV".to_string(),
        description: "Antivirus scanner".to_string(),
        category: "Security".to_string(),
        is_installed: clamav_installed,
        is_running: clamav_running,
        requires_docker: false,
    });

    // === Docker-dependent Features ===

    // Gluetun
    let (gluetun_exists, gluetun_running) = detect_docker_container("gluetun");
    features.push(OptionalFeature {
        id: "gluetun".to_string(),
        name: "Gluetun".to_string(),
        description: "VPN client for NordVPN, Mullvad, etc.".to_string(),
        category: "VPN".to_string(),
        is_installed: gluetun_exists,
        is_running: gluetun_running,
        requires_docker: true,
    });

    // Radarr
    let radarr_running = detect_port_listening(7878) || detect_docker_container("radarr").1;
    features.push(OptionalFeature {
        id: "radarr".to_string(),
        name: "Radarr".to_string(),
        description: "Movie collection manager".to_string(),
        category: "Media".to_string(),
        is_installed: radarr_running,
        is_running: radarr_running,
        requires_docker: true,
    });

    // Sonarr
    let sonarr_running = detect_port_listening(8989) || detect_docker_container("sonarr").1;
    features.push(OptionalFeature {
        id: "sonarr".to_string(),
        name: "Sonarr".to_string(),
        description: "TV show collection manager".to_string(),
        category: "Media".to_string(),
        is_installed: sonarr_running,
        is_running: sonarr_running,
        requires_docker: true,
    });

    // Jellyfin
    let jellyfin_running = detect_port_listening(8096) || detect_docker_container("jellyfin").1;
    features.push(OptionalFeature {
        id: "jellyfin".to_string(),
        name: "Jellyfin".to_string(),
        description: "Media streaming server".to_string(),
        category: "Media".to_string(),
        is_installed: jellyfin_running,
        is_running: jellyfin_running,
        requires_docker: true,
    });

    // Transmission
    let (transmission_exists, transmission_running) = detect_docker_container("transmission");
    features.push(OptionalFeature {
        id: "transmission".to_string(),
        name: "Transmission".to_string(),
        description: "BitTorrent client".to_string(),
        category: "Media".to_string(),
        is_installed: transmission_exists,
        is_running: transmission_running,
        requires_docker: true,
    });

    Ok(Json(features))
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

/// Save network configuration
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

/// Install a core service
pub async fn install_core_service(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<InstallResult>, (StatusCode, String)> {
    let service_id = payload.get("id")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing service id".to_string()))?;

    let result = match service_id {
        "dnsmasq" => install_dnsmasq().await,
        "firewall" => install_firewall().await,
        "hostapd" => install_hostapd().await,
        _ => Err(format!("Unknown core service: {}", service_id)),
    };

    match result {
        Ok(msg) => Ok(Json(InstallResult {
            id: service_id.to_string(),
            success: true,
            message: msg,
        })),
        Err(msg) => Ok(Json(InstallResult {
            id: service_id.to_string(),
            success: false,
            message: msg,
        })),
    }
}

/// Install an optional feature
pub async fn install_feature(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<InstallResult>, (StatusCode, String)> {
    let feature_id = payload.get("id")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "Missing feature id".to_string()))?;

    let result = match feature_id {
        "docker" => install_docker().await,
        "adguard" => install_adguard().await,
        "tailscale" => install_tailscale().await,
        "clamav" => install_clamav().await,
        "gluetun" => install_gluetun().await,
        "radarr" => install_radarr().await,
        "sonarr" => install_sonarr().await,
        "jellyfin" => install_jellyfin().await,
        "transmission" => install_transmission().await,
        _ => Err(format!("Unknown feature: {}", feature_id)),
    };

    match result {
        Ok(msg) => Ok(Json(InstallResult {
            id: feature_id.to_string(),
            success: true,
            message: msg,
        })),
        Err(msg) => Ok(Json(InstallResult {
            id: feature_id.to_string(),
            success: false,
            message: msg,
        })),
    }
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

// ============ CORE SERVICE INSTALLATION ============

async fn install_dnsmasq() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y dnsmasq"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        // Basic dnsmasq config for router
        let config = r#"
# RouterUI dnsmasq configuration
interface=eth0
dhcp-range=192.168.1.100,192.168.1.250,12h
dhcp-option=option:router,192.168.1.1
dhcp-option=option:dns-server,192.168.1.1
"#;
        std::fs::write("/etc/dnsmasq.d/routerui.conf", config).ok();

        // Restart dnsmasq
        Command::new("systemctl")
            .args(["restart", "dnsmasq"])
            .output()
            .ok();

        Ok("dnsmasq installed and configured".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_firewall() -> Result<String, String> {
    // iptables is usually pre-installed, but ensure it's there
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y iptables iptables-persistent"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() || detect_command_exists("iptables") {
        // Enable IP forwarding
        Command::new("bash")
            .args(["-c", "echo 1 > /proc/sys/net/ipv4/ip_forward"])
            .output()
            .ok();

        // Make it persistent
        Command::new("bash")
            .args(["-c", "echo 'net.ipv4.ip_forward=1' >> /etc/sysctl.conf && sysctl -p"])
            .output()
            .ok();

        // Basic NAT rule (will be configured properly with actual interfaces later)
        Command::new("bash")
            .args(["-c", "iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE"])
            .output()
            .ok();

        Ok("Firewall configured with NAT enabled".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_hostapd() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y hostapd"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        // Unmask and enable hostapd
        Command::new("systemctl")
            .args(["unmask", "hostapd"])
            .output()
            .ok();

        Ok("hostapd installed (configure WiFi settings in Network tab)".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// ============ OPTIONAL FEATURE INSTALLATION ============

async fn install_docker() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y docker.io docker-compose && systemctl enable docker && systemctl start docker"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Docker installed successfully".to_string())
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
        Ok("AdGuard Home installed - complete setup at http://localhost:3000".to_string())
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
        Ok("Tailscale installed - run 'tailscale up' to connect".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_clamav() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y clamav clamav-daemon && systemctl enable clamav-daemon"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        // Update virus definitions in background
        Command::new("bash")
            .args(["-c", "freshclam &"])
            .output()
            .ok();
        Ok("ClamAV installed - updating virus definitions".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_gluetun() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            mkdir -p /opt/routerui/config/gluetun && \
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
                -v /opt/routerui/config/gluetun:/gluetun \
                --restart=unless-stopped \
                qmcgaw/gluetun
        "#])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Gluetun installed - configure VPN credentials in Docker settings".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_radarr() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            mkdir -p /opt/routerui/config/radarr /media/movies /media/downloads && \
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
        Ok("Radarr installed - access at http://localhost:7878".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_sonarr() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            mkdir -p /opt/routerui/config/sonarr /media/tv /media/downloads && \
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
        Ok("Sonarr installed - access at http://localhost:8989".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_jellyfin() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            mkdir -p /opt/routerui/config/jellyfin /media/tv /media/movies && \
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
        Ok("Jellyfin installed - access at http://localhost:8096".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_transmission() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", r#"
            mkdir -p /opt/routerui/config/transmission /media/downloads /media/watch && \
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
        Ok("Transmission installed - access at http://localhost:9091".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
