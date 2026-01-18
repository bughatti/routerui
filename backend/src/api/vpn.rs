use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;

// ============ TAILSCALE DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub logged_in: bool,
    pub tailscale_ip: Option<String>,
    pub hostname: Option<String>,
    pub dns_name: Option<String>,
    pub exit_node_active: bool,
    pub exit_node_advertised: bool,
    pub advertised_routes: Vec<String>,
    pub login_url: Option<String>,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct TailscaleDevice {
    pub name: String,
    pub dns_name: String,
    pub tailscale_ip: String,
    pub os: String,
    pub online: bool,
    pub is_exit_node: bool,
    pub is_current: bool,
    pub relay: Option<String>, // DERP relay if not direct
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct TailscaleNetcheck {
    pub udp: bool,
    pub ipv4: bool,
    pub ipv6: bool,
    pub mapping_varies_by_dest: bool,
    pub hair_pinning: bool,
    pub preferred_derp: String,
    pub derp_latencies: Vec<DerpLatency>,
}

#[derive(Debug, Serialize)]
pub struct DerpLatency {
    pub region: String,
    pub latency_ms: f64,
}

#[derive(Debug, Deserialize)]
pub struct TailscaleConnect {
    pub advertise_routes: Option<String>,      // e.g., "10.22.22.0/24"
    pub advertise_exit_node: Option<bool>,
    pub hostname: Option<String>,
    pub accept_routes: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct TailscaleLoginUrl {
    pub url: String,
}

// ============ GLUETUN/NORDVPN DATA STRUCTURES ============

#[derive(Debug, Serialize)]
pub struct GluetunStatus {
    pub container_running: bool,
    pub container_name: Option<String>,
    pub vpn_connected: bool,
    pub vpn_ip: Option<String>,
    pub vpn_country: Option<String>,
    pub vpn_city: Option<String>,
    pub vpn_provider: String,
    pub port_forwarded: Option<u16>,
}

// ============ COMBINED VPN STATUS ============

#[derive(Debug, Serialize)]
pub struct VpnOverview {
    pub tailscale: TailscaleStatus,
    pub gluetun: GluetunStatus,
}

// ============ HELPER FUNCTIONS ============

fn tailscale_installed() -> bool {
    Command::new("which")
        .arg("tailscale")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn parse_tailscale_status() -> TailscaleStatus {
    if !tailscale_installed() {
        return TailscaleStatus {
            installed: false,
            running: false,
            logged_in: false,
            tailscale_ip: None,
            hostname: None,
            dns_name: None,
            exit_node_active: false,
            exit_node_advertised: false,
            advertised_routes: vec![],
            login_url: None,
            version: String::new(),
        };
    }

    // Check if daemon is running
    let running = Command::new("systemctl")
        .args(["is-active", "tailscaled"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    // Get version
    let version = Command::new("tailscale")
        .arg("version")
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_default();

    // Get status JSON
    let status_output = Command::new("tailscale")
        .args(["status", "--json"])
        .output();

    let mut logged_in = false;
    let mut tailscale_ip = None;
    let mut hostname = None;
    let mut dns_name = None;
    let mut exit_node_active = false;

    if let Ok(output) = status_output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                logged_in = json.get("BackendState")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "Running")
                    .unwrap_or(false);

                if let Some(self_info) = json.get("Self") {
                    tailscale_ip = self_info.get("TailscaleIPs")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    hostname = self_info.get("HostName")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    dns_name = self_info.get("DNSName")
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim_end_matches('.').to_string());
                }

                exit_node_active = json.get("ExitNodeStatus")
                    .and_then(|v| v.get("Online"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
            }
        }
    }

    // Check prefs for advertised routes and exit node
    let prefs_output = Command::new("tailscale")
        .args(["debug", "prefs"])
        .output();

    let mut exit_node_advertised = false;
    let mut advertised_routes = vec![];

    if let Ok(output) = prefs_output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                exit_node_advertised = json.get("AdvertisesExitNode")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if let Some(routes) = json.get("AdvertiseRoutes").and_then(|v| v.as_array()) {
                    advertised_routes = routes.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }
        }
    }

    // Check if needs login
    let login_url = if !logged_in {
        // Try to get login URL from daemon status
        let service_output = Command::new("systemctl")
            .args(["status", "tailscaled"])
            .output();

        if let Ok(output) = service_output {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines()
                .find(|l| l.contains("https://login.tailscale.com"))
                .and_then(|l| l.split_whitespace().find(|w| w.starts_with("https://")))
                .map(String::from)
        } else {
            None
        }
    } else {
        None
    };

    TailscaleStatus {
        installed: true,
        running,
        logged_in,
        tailscale_ip,
        hostname,
        dns_name,
        exit_node_active,
        exit_node_advertised,
        advertised_routes,
        login_url,
        version,
    }
}

fn get_gluetun_status() -> GluetunStatus {
    // Check if gluetun container is running
    let container_output = Command::new("docker")
        .args(["ps", "--filter", "name=gluetun", "--format", "{{.Names}}"])
        .output();

    let container_running = container_output
        .as_ref()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    let container_name = container_output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    if !container_running {
        return GluetunStatus {
            container_running: false,
            container_name: None,
            vpn_connected: false,
            vpn_ip: None,
            vpn_country: None,
            vpn_city: None,
            vpn_provider: "NordVPN".to_string(),
            port_forwarded: None,
        };
    }

    // Get VPN status from gluetun API (runs on port 8000 inside container)
    let ip_response = Command::new("docker")
        .args(["exec", "gluetun", "wget", "-qO-", "http://127.0.0.1:8000/v1/publicip/ip"])
        .output()
        .ok();

    let mut vpn_ip = None;
    let mut vpn_country = None;
    let mut vpn_city = None;

    if let Some(output) = ip_response {
        let text = String::from_utf8_lossy(&output.stdout);
        // Try to parse as JSON first
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
            vpn_ip = json.get("public_ip")
                .and_then(|v| v.as_str())
                .map(String::from);
            vpn_country = json.get("country")
                .and_then(|v| v.as_str())
                .map(String::from);
            vpn_city = json.get("city")
                .and_then(|v| v.as_str())
                .map(String::from);
        } else {
            // Fallback: treat as plain IP string
            let trimmed = text.trim().trim_matches('"');
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                vpn_ip = Some(trimmed.to_string());
            }
        }
    }

    let vpn_connected = vpn_ip.is_some();

    // Get forwarded port if available
    let port_forwarded = Command::new("docker")
        .args(["exec", "gluetun", "wget", "-qO-", "http://127.0.0.1:8000/v1/openvpn/portforwarded"])
        .output()
        .ok()
        .and_then(|o| {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                json.get("port").and_then(|v| v.as_u64()).map(|p| p as u16)
            } else {
                None
            }
        });

    GluetunStatus {
        container_running,
        container_name,
        vpn_connected,
        vpn_ip,
        vpn_country,
        vpn_city,
        vpn_provider: "NordVPN".to_string(),
        port_forwarded,
    }
}

// ============ API ENDPOINTS ============

pub async fn overview() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::vpn::overview()));
    }

    let tailscale = parse_tailscale_status();
    let gluetun = get_gluetun_status();

    Ok(Json(serde_json::to_value(VpnOverview { tailscale, gluetun }).unwrap()))
}

pub async fn tailscale_status() -> Result<Json<TailscaleStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(TailscaleStatus {
            installed: true,
            running: true,
            logged_in: true,
            tailscale_ip: Some("100.100.100.1".to_string()),
            hostname: Some("mock-router".to_string()),
            dns_name: Some("mock-router.tail12345.ts.net".to_string()),
            exit_node_active: false,
            exit_node_advertised: false,
            advertised_routes: vec!["10.22.22.0/24".to_string()],
            login_url: None,
            version: "1.56.0".to_string(),
        }));
    }

    Ok(Json(parse_tailscale_status()))
}

pub async fn tailscale_devices() -> Result<Json<Vec<TailscaleDevice>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![
            TailscaleDevice { name: "mock-router".to_string(), dns_name: "mock-router.tail12345.ts.net".to_string(), tailscale_ip: "100.100.100.1".to_string(), os: "linux".to_string(), online: true, is_exit_node: false, is_current: true, relay: None, rx_bytes: Some(1048576), tx_bytes: Some(524288) },
            TailscaleDevice { name: "desktop".to_string(), dns_name: "desktop.tail12345.ts.net".to_string(), tailscale_ip: "100.100.100.2".to_string(), os: "windows".to_string(), online: true, is_exit_node: false, is_current: false, relay: None, rx_bytes: Some(2097152), tx_bytes: Some(1048576) },
        ]));
    }

    let output = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Ok(Json(vec![]));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut devices = Vec::new();

    // Get current device info
    let self_id = json.get("Self")
        .and_then(|v| v.get("ID"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Parse peer list
    if let Some(peers) = json.get("Peer").and_then(|v| v.as_object()) {
        for (_id, peer) in peers {
            let name = peer.get("HostName")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let dns_name = peer.get("DNSName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_end_matches('.')
                .to_string();

            let tailscale_ip = peer.get("TailscaleIPs")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let os = peer.get("OS")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let online = peer.get("Online")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let is_exit_node = peer.get("ExitNode")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let relay = peer.get("Relay")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(String::from);

            let rx_bytes = peer.get("RxBytes")
                .and_then(|v| v.as_u64());

            let tx_bytes = peer.get("TxBytes")
                .and_then(|v| v.as_u64());

            devices.push(TailscaleDevice {
                name,
                dns_name,
                tailscale_ip,
                os,
                online,
                is_exit_node,
                is_current: false,
                relay,
                rx_bytes,
                tx_bytes,
            });
        }
    }

    // Add self
    if let Some(self_info) = json.get("Self") {
        let name = self_info.get("HostName")
            .and_then(|v| v.as_str())
            .unwrap_or("this-device")
            .to_string();

        let dns_name = self_info.get("DNSName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim_end_matches('.')
            .to_string();

        let tailscale_ip = self_info.get("TailscaleIPs")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let os = self_info.get("OS")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        devices.insert(0, TailscaleDevice {
            name,
            dns_name,
            tailscale_ip,
            os,
            online: true,
            is_exit_node: false,
            is_current: true,
            relay: None,
            rx_bytes: None,
            tx_bytes: None,
        });
    }

    Ok(Json(devices))
}

pub async fn tailscale_connect(
    Json(payload): Json<TailscaleConnect>,
) -> Result<Json<TailscaleLoginUrl>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(TailscaleLoginUrl { url: "https://login.tailscale.com/a/mock123456 (mock)".to_string() }));
    }

    let mut args = vec!["up".to_string()];

    if let Some(routes) = payload.advertise_routes {
        // Validate routes format
        if !routes.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '/' || c == ',') {
            return Err((StatusCode::BAD_REQUEST, "Invalid route format".to_string()));
        }
        args.push(format!("--advertise-routes={}", routes));
    }

    if payload.advertise_exit_node == Some(true) {
        args.push("--advertise-exit-node".to_string());
    }

    if let Some(hostname) = payload.hostname {
        // Validate hostname
        if !hostname.chars().all(|c| c.is_alphanumeric() || c == '-') {
            return Err((StatusCode::BAD_REQUEST, "Invalid hostname".to_string()));
        }
        args.push(format!("--hostname={}", hostname));
    }

    if payload.accept_routes == Some(true) {
        args.push("--accept-routes".to_string());
    }

    // Run tailscale up and capture login URL
    let output = Command::new("sudo")
        .arg("tailscale")
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Find login URL in output
    let url = combined.lines()
        .find(|l| l.contains("https://login.tailscale.com") || l.contains("https://"))
        .and_then(|l| l.split_whitespace().find(|w| w.starts_with("https://")))
        .unwrap_or("")
        .to_string();

    Ok(Json(TailscaleLoginUrl { url }))
}

pub async fn tailscale_disconnect() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let output = Command::new("sudo")
        .args(["tailscale", "down"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn tailscale_logout() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let output = Command::new("sudo")
        .args(["tailscale", "logout"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn tailscale_set_exit_node(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let enable = payload.get("enable")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "exit_node": enable, "mock": true })));
    }

    let mut args = vec!["set".to_string()];

    if enable {
        args.push("--advertise-exit-node".to_string());
    } else {
        args.push("--advertise-exit-node=false".to_string());
    }

    let output = Command::new("sudo")
        .arg("tailscale")
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({ "success": true, "exit_node": enable })))
}

pub async fn tailscale_netcheck() -> Result<Json<TailscaleNetcheck>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(TailscaleNetcheck {
            udp: true,
            ipv4: true,
            ipv6: false,
            mapping_varies_by_dest: false,
            hair_pinning: true,
            preferred_derp: "DERP 1".to_string(),
            derp_latencies: vec![
                DerpLatency { region: "1".to_string(), latency_ms: 25.0 },
                DerpLatency { region: "2".to_string(), latency_ms: 45.0 },
            ],
        }));
    }

    let output = Command::new("tailscale")
        .args(["netcheck", "--format=json"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Netcheck failed".to_string()));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut derp_latencies = Vec::new();
    if let Some(regions) = json.get("RegionLatency").and_then(|v| v.as_object()) {
        for (region, latency) in regions {
            if let Some(ms) = latency.as_f64() {
                derp_latencies.push(DerpLatency {
                    region: region.clone(),
                    latency_ms: ms * 1000.0, // Convert to ms
                });
            }
        }
    }
    derp_latencies.sort_by(|a, b| a.latency_ms.partial_cmp(&b.latency_ms).unwrap());

    let preferred_derp = json.get("PreferredDERP")
        .and_then(|v| v.as_u64())
        .map(|n| format!("DERP {}", n))
        .unwrap_or_default();

    Ok(Json(TailscaleNetcheck {
        udp: json.get("UDP").and_then(|v| v.as_bool()).unwrap_or(false),
        ipv4: json.get("IPv4").and_then(|v| v.as_bool()).unwrap_or(false),
        ipv6: json.get("IPv6").and_then(|v| v.as_bool()).unwrap_or(false),
        mapping_varies_by_dest: json.get("MappingVariesByDestIP").and_then(|v| v.as_bool()).unwrap_or(false),
        hair_pinning: json.get("HairPinning").and_then(|v| v.as_bool()).unwrap_or(false),
        preferred_derp,
        derp_latencies,
    }))
}

// ============ GLUETUN ENDPOINTS ============

pub async fn gluetun_status() -> Result<Json<GluetunStatus>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(GluetunStatus {
            container_running: true,
            container_name: Some("gluetun".to_string()),
            vpn_connected: true,
            vpn_ip: Some("185.220.100.100".to_string()),
            vpn_country: Some("United States".to_string()),
            vpn_city: Some("New York".to_string()),
            vpn_provider: "NordVPN".to_string(),
            port_forwarded: Some(51820),
        }));
    }

    Ok(Json(get_gluetun_status()))
}

pub async fn gluetun_restart() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let output = Command::new("docker")
        .args(["restart", "gluetun"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    Ok(Json(serde_json::json!({ "success": true })))
}
