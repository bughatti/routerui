use axum::{http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Serialize, Clone)]
pub struct AddonStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddonInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: AddonStatus,
    pub install_command: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InstallRequest {
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct InstallResult {
    pub success: bool,
    pub message: String,
}

/// Get status of all addons
pub async fn status() -> Result<Json<HashMap<String, AddonStatus>>, (StatusCode, String)> {
    let mut addons = HashMap::new();

    // AdGuard Home
    addons.insert("adguard".to_string(), check_adguard());

    // VPN (Tailscale or Gluetun)
    addons.insert("vpn".to_string(), check_vpn());

    // Docker
    addons.insert("docker".to_string(), check_docker());

    // Media (any of Radarr, Sonarr, Jellyfin, Plex)
    addons.insert("media".to_string(), check_media());

    // Antivirus (ClamAV)
    addons.insert("antivirus".to_string(), check_antivirus());

    // Protection (crowdsec or fail2ban)
    addons.insert("protection".to_string(), check_protection());

    // Security monitor
    addons.insert("security".to_string(), check_security());

    Ok(Json(addons))
}

/// List all available addons with details
pub async fn list() -> Result<Json<Vec<AddonInfo>>, (StatusCode, String)> {
    let addons = vec![
        AddonInfo {
            id: "adguard".to_string(),
            name: "AdGuard Home".to_string(),
            description: "Network-wide ad blocking and DNS management".to_string(),
            status: check_adguard(),
            install_command: Some("curl -s -S -L https://raw.githubusercontent.com/AdguardTeam/AdGuardHome/master/scripts/install.sh | sh -s -- -v".to_string()),
        },
        AddonInfo {
            id: "tailscale".to_string(),
            name: "Tailscale VPN".to_string(),
            description: "Mesh VPN for secure remote access".to_string(),
            status: check_tailscale(),
            install_command: Some("curl -fsSL https://tailscale.com/install.sh | sh".to_string()),
        },
        AddonInfo {
            id: "docker".to_string(),
            name: "Docker".to_string(),
            description: "Container runtime for running additional services".to_string(),
            status: check_docker(),
            install_command: Some("apt-get install -y docker.io docker-compose && systemctl enable docker && systemctl start docker".to_string()),
        },
        AddonInfo {
            id: "antivirus".to_string(),
            name: "ClamAV Antivirus".to_string(),
            description: "Open-source antivirus scanner".to_string(),
            status: check_antivirus(),
            install_command: Some("apt-get install -y clamav clamav-daemon && systemctl enable clamav-daemon".to_string()),
        },
        AddonInfo {
            id: "crowdsec".to_string(),
            name: "CrowdSec".to_string(),
            description: "Collaborative security engine for threat detection".to_string(),
            status: check_crowdsec(),
            install_command: Some("curl -s https://packagecloud.io/install/repositories/crowdsec/crowdsec/script.deb.sh | bash && apt-get install -y crowdsec".to_string()),
        },
        AddonInfo {
            id: "jellyfin".to_string(),
            name: "Jellyfin".to_string(),
            description: "Free media streaming server (requires Docker)".to_string(),
            status: check_jellyfin(),
            install_command: None, // Docker-based install
        },
        AddonInfo {
            id: "pihole".to_string(),
            name: "Pi-hole".to_string(),
            description: "Network-wide ad blocking (alternative to AdGuard)".to_string(),
            status: check_pihole(),
            install_command: Some("curl -sSL https://install.pi-hole.net | bash".to_string()),
        },
    ];

    Ok(Json(addons))
}

/// Install an addon
pub async fn install(
    Json(payload): Json<InstallRequest>,
) -> Result<Json<InstallResult>, (StatusCode, String)> {
    let result = match payload.id.as_str() {
        "adguard" => install_adguard().await,
        "tailscale" => install_tailscale().await,
        "docker" => install_docker().await,
        "antivirus" => install_antivirus().await,
        "crowdsec" => install_crowdsec().await,
        "jellyfin" => install_jellyfin().await,
        _ => Err(format!("Unknown addon: {}", payload.id)),
    };

    match result {
        Ok(msg) => Ok(Json(InstallResult {
            success: true,
            message: msg,
        })),
        Err(msg) => Ok(Json(InstallResult {
            success: false,
            message: msg,
        })),
    }
}

// ============ CHECK FUNCTIONS ============

fn check_adguard() -> AddonStatus {
    let installed = Command::new("which")
        .arg("AdGuardHome")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
        || std::path::Path::new("/opt/AdGuardHome/AdGuardHome").exists();

    let running = Command::new("systemctl")
        .args(["is-active", "AdGuardHome"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_vpn() -> AddonStatus {
    let tailscale = check_tailscale();
    let gluetun = check_gluetun();

    AddonStatus {
        installed: tailscale.installed || gluetun.installed,
        running: tailscale.running || gluetun.running,
        version: None,
    }
}

fn check_tailscale() -> AddonStatus {
    let installed = Command::new("which")
        .arg("tailscale")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "tailscaled"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_gluetun() -> AddonStatus {
    let running = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l == "gluetun"))
        .unwrap_or(false);

    AddonStatus {
        installed: running,
        running,
        version: None,
    }
}

fn check_docker() -> AddonStatus {
    let installed = Command::new("which")
        .arg("docker")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "docker"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_media() -> AddonStatus {
    // Check for any media service
    let jellyfin = check_jellyfin();
    let radarr = check_port(7878);
    let sonarr = check_port(8989);
    let plex = check_port(32400);

    AddonStatus {
        installed: jellyfin.installed || radarr || sonarr || plex,
        running: jellyfin.running || radarr || sonarr || plex,
        version: None,
    }
}

fn check_jellyfin() -> AddonStatus {
    let running = check_port(8096)
        || Command::new("docker")
            .args(["ps", "--format", "{{.Names}}"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l == "jellyfin"))
            .unwrap_or(false);

    AddonStatus {
        installed: running,
        running,
        version: None,
    }
}

fn check_antivirus() -> AddonStatus {
    let installed = Command::new("which")
        .arg("clamscan")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "clamav-daemon"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_protection() -> AddonStatus {
    let crowdsec = check_crowdsec();
    let fail2ban = check_fail2ban();

    AddonStatus {
        installed: crowdsec.installed || fail2ban.installed,
        running: crowdsec.running || fail2ban.running,
        version: None,
    }
}

fn check_crowdsec() -> AddonStatus {
    let installed = Command::new("which")
        .arg("cscli")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "crowdsec"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_fail2ban() -> AddonStatus {
    let installed = Command::new("which")
        .arg("fail2ban-client")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let running = Command::new("systemctl")
        .args(["is-active", "fail2ban"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_security() -> AddonStatus {
    // Security monitoring - check if we have basic tools
    let netstat = Command::new("which")
        .arg("ss")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    AddonStatus {
        installed: netstat,
        running: netstat,
        version: None,
    }
}

fn check_pihole() -> AddonStatus {
    let installed = std::path::Path::new("/etc/pihole").exists();
    let running = Command::new("systemctl")
        .args(["is-active", "pihole-FTL"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    AddonStatus {
        installed,
        running,
        version: None,
    }
}

fn check_port(port: u16) -> bool {
    Command::new("ss")
        .args(["-tlnp"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(":{}", port)))
        .unwrap_or(false)
}

// ============ INSTALL FUNCTIONS ============

async fn install_adguard() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "curl -s -S -L https://raw.githubusercontent.com/AdguardTeam/AdGuardHome/master/scripts/install.sh | sh -s -- -v"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("AdGuard Home installed. Complete setup at http://localhost:3000".to_string())
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
        Ok("Tailscale installed. Run 'tailscale up' to connect.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_docker() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y docker.io docker-compose && systemctl enable docker && systemctl start docker"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("Docker installed and running.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_antivirus() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y clamav clamav-daemon && systemctl enable clamav-daemon && freshclam &"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("ClamAV installed. Virus definitions are updating in background.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_crowdsec() -> Result<String, String> {
    let output = Command::new("bash")
        .args(["-c", "curl -s https://packagecloud.io/install/repositories/crowdsec/crowdsec/script.deb.sh | bash && apt-get install -y crowdsec && systemctl enable crowdsec && systemctl start crowdsec"])
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok("CrowdSec installed and running.".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

async fn install_jellyfin() -> Result<String, String> {
    // Check if Docker is installed first
    let docker_installed = check_docker().installed;
    if !docker_installed {
        return Err("Docker is required. Please install Docker first.".to_string());
    }

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
        Ok("Jellyfin installed. Access at http://localhost:8096".to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
