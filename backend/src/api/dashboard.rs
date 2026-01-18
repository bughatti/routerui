use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::mock;
use crate::system;
use super::AuthUser;

#[derive(Serialize)]
pub struct DashboardOverview {
    pub system: system::SystemStatus,
    pub interfaces: Vec<system::NetworkInterface>,
    pub services: Vec<system::ServiceStatus>,
    pub wan_status: WanStatus,
    pub lan_clients: u32,
}

#[derive(Serialize)]
pub struct WanStatus {
    pub connected: bool,
    pub interface: String,
    pub ip_address: Option<String>,
    pub gateway: Option<String>,
}

pub async fn overview(
    AuthUser(_user): AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::dashboard::overview()));
    }

    let system = system::get_system_status()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let interfaces = system::get_interfaces()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let services = system::get_services()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Find WAN interface (enp1s0)
    let wan_iface = interfaces.iter().find(|i| i.name == "enp1s0");
    let wan_status = WanStatus {
        connected: wan_iface.map(|i| i.state == "UP").unwrap_or(false),
        interface: "enp1s0".to_string(),
        ip_address: wan_iface.and_then(|i| i.ipv4.clone()),
        gateway: get_default_gateway(),
    };

    // Count DHCP leases for LAN clients
    let lan_clients = count_dhcp_leases();

    Ok(Json(serde_json::to_value(DashboardOverview {
        system,
        interfaces,
        services,
        wan_status,
        lan_clients,
    }).unwrap()))
}

fn get_default_gateway() -> Option<String> {
    std::process::Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .nth(2)
                .map(|s| s.to_string())
        })
}

fn count_dhcp_leases() -> u32 {
    // Try common lease file locations
    let paths = [
        "/var/lib/misc/dnsmasq.leases",
        "/var/lib/dnsmasq/dnsmasq.leases",
    ];

    for path in paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return content.lines().count() as u32;
        }
    }
    0
}
