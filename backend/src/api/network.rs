use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::fs;
use std::collections::HashMap;

use crate::mock;

const DNSMASQ_CONF: &str = "/etc/dnsmasq.d/router.conf";
const DNSMASQ_LEASES: &str = "/var/lib/misc/dnsmasq.leases";
const DNSMASQ_STATIC: &str = "/etc/dnsmasq.d/static-leases.conf";
const HOSTAPD_CONF: &str = "/etc/hostapd/hostapd.conf";
const STATIC_ROUTES_FILE: &str = "/opt/routerui/static-routes.json";
const WOL_DEVICES_FILE: &str = "/opt/routerui/wol-devices.json";
const LOCAL_DNS_FILE: &str = "/etc/dnsmasq.d/local-dns.conf";

// ============ INTERFACES ============

#[derive(Debug, Serialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac_address: String,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub state: String,
    pub mtu: u32,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub interface_type: String, // wan, lan, wifi, loopback
}

pub async fn interfaces() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::network::interfaces()));
    }

    let output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let json_str = String::from_utf8_lossy(&output.stdout);
    let ifaces: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut interfaces = Vec::new();

    for iface in ifaces {
        let name = iface["ifname"].as_str().unwrap_or("").to_string();

        // Skip docker and virtual interfaces
        if name.starts_with("docker") || name.starts_with("br-") || name.starts_with("veth") {
            continue;
        }

        let mut state = iface["operstate"].as_str().unwrap_or("UNKNOWN").to_string();
        let mac = iface["address"].as_str().unwrap_or("").to_string();
        let mtu = iface["mtu"].as_u64().unwrap_or(1500) as u32;

        let mut ipv4 = None;
        let mut ipv6 = None;

        if let Some(addr_info) = iface["addr_info"].as_array() {
            for addr in addr_info {
                let family = addr["family"].as_str().unwrap_or("");
                let local = addr["local"].as_str().unwrap_or("");
                let prefix = addr["prefixlen"].as_u64().unwrap_or(0);

                if family == "inet" && ipv4.is_none() {
                    ipv4 = Some(format!("{}/{}", local, prefix));
                } else if family == "inet6" && ipv6.is_none() && !local.starts_with("fe80") {
                    ipv6 = Some(format!("{}/{}", local, prefix));
                }
            }
        }

        // Improve state display for virtual interfaces
        if state == "UNKNOWN" && ipv4.is_some() {
            state = "Active".to_string();
        }

        // Get RX/TX stats
        let (rx_bytes, tx_bytes) = get_interface_stats(&name);

        // Determine interface type
        let interface_type = match name.as_str() {
            "tailscale0" => "vpn",
            "br0" => "bridge",
            "enp1s0" => "wan",
            "enp2s0" => "lan",
            "wlo1" | "wlan0" => "wifi",
            "lo" => "loopback",
            _ => "other",
        }.to_string();

        interfaces.push(NetworkInterface {
            name,
            mac_address: mac,
            ipv4,
            ipv6,
            state,
            mtu,
            rx_bytes,
            tx_bytes,
            interface_type,
        });
    }

    Ok(Json(serde_json::to_value(interfaces).unwrap()))
}

fn get_interface_stats(name: &str) -> (u64, u64) {
    let rx_path = format!("/sys/class/net/{}/statistics/rx_bytes", name);
    let tx_path = format!("/sys/class/net/{}/statistics/tx_bytes", name);

    let rx = fs::read_to_string(&rx_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    let tx = fs::read_to_string(&tx_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);

    (rx, tx)
}

// ============ DHCP ============

#[derive(Debug, Serialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: String,
    pub range_end: String,
    pub lease_time: String,
    pub gateway: String,
    pub dns_server: String,
}

#[derive(Debug, Serialize)]
pub struct DhcpLease {
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: String,
    pub expires: String,
    pub is_static: bool,
}

#[derive(Debug, Serialize)]
pub struct DhcpStatus {
    pub config: DhcpConfig,
    pub leases: Vec<DhcpLease>,
    pub static_leases: Vec<StaticLease>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StaticLease {
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: String,
}

#[derive(Debug, Deserialize)]
pub struct AddStaticLease {
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveStaticLease {
    pub mac_address: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDhcpConfig {
    pub range_start: String,
    pub range_end: String,
    pub lease_time: String,
}

pub async fn dhcp_status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::network::dhcp_status()));
    }

    // Parse dnsmasq config
    let config = parse_dnsmasq_config()?;

    // Parse active leases
    let leases = parse_dhcp_leases()?;

    // Parse static leases
    let static_leases = load_static_leases();

    Ok(Json(serde_json::to_value(DhcpStatus {
        config,
        leases,
        static_leases,
    }).unwrap()))
}

fn parse_dnsmasq_config() -> Result<DhcpConfig, (StatusCode, String)> {
    let content = fs::read_to_string(DNSMASQ_CONF)
        .or_else(|_| fs::read_to_string("/etc/dnsmasq.conf"))
        .unwrap_or_default();

    let mut range_start = String::new();
    let mut range_end = String::new();
    let mut lease_time = "24h".to_string();
    let mut gateway = "10.22.22.1".to_string();
    let mut dns_server = "10.22.22.1".to_string();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("dhcp-range=") {
            let parts: Vec<&str> = line.trim_start_matches("dhcp-range=").split(',').collect();
            if parts.len() >= 2 {
                range_start = parts[0].to_string();
                range_end = parts[1].to_string();
            }
            if parts.len() >= 3 {
                lease_time = parts[2].to_string();
            }
        } else if line.starts_with("dhcp-option=3,") {
            gateway = line.trim_start_matches("dhcp-option=3,").to_string();
        } else if line.starts_with("dhcp-option=6,") {
            dns_server = line.trim_start_matches("dhcp-option=6,").to_string();
        }
    }

    Ok(DhcpConfig {
        enabled: true,
        range_start,
        range_end,
        lease_time,
        gateway,
        dns_server,
    })
}

fn parse_dhcp_leases() -> Result<Vec<DhcpLease>, (StatusCode, String)> {
    let content = fs::read_to_string(DNSMASQ_LEASES).unwrap_or_default();
    let static_leases = load_static_leases();
    let static_macs: Vec<String> = static_leases.iter().map(|l| l.mac_address.to_lowercase()).collect();

    let mut leases = Vec::new();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let expires_ts: i64 = parts[0].parse().unwrap_or(0);
            let mac = parts[1].to_string();
            let ip = parts[2].to_string();
            let hostname = parts[3].to_string();

            let expires = if expires_ts == 0 {
                "Never".to_string()
            } else {
                chrono::DateTime::from_timestamp(expires_ts, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "Unknown".to_string())
            };

            let is_static = static_macs.contains(&mac.to_lowercase());

            leases.push(DhcpLease {
                mac_address: mac,
                ip_address: ip,
                hostname,
                expires,
                is_static,
            });
        }
    }

    Ok(leases)
}

fn load_static_leases() -> Vec<StaticLease> {
    // Parse from dnsmasq static leases file
    let content = fs::read_to_string(DNSMASQ_STATIC).unwrap_or_default();
    let mut leases = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("dhcp-host=") {
            let parts: Vec<&str> = line.trim_start_matches("dhcp-host=").split(',').collect();
            if parts.len() >= 2 {
                let mac = parts[0].to_string();
                let ip = parts[1].to_string();
                let hostname = parts.get(2).map(|s| s.to_string()).unwrap_or_default();
                leases.push(StaticLease {
                    mac_address: mac,
                    ip_address: ip,
                    hostname,
                });
            }
        }
    }

    leases
}

fn save_static_leases(leases: &[StaticLease]) -> Result<(), (StatusCode, String)> {
    let mut content = String::from("# Static DHCP leases - managed by RouterUI\n");
    for lease in leases {
        if lease.hostname.is_empty() {
            content.push_str(&format!("dhcp-host={},{}\n", lease.mac_address, lease.ip_address));
        } else {
            content.push_str(&format!("dhcp-host={},{},{}\n", lease.mac_address, lease.ip_address, lease.hostname));
        }
    }

    fs::write(DNSMASQ_STATIC, &content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Reload dnsmasq
    let _ = Command::new("sudo")
        .args(["systemctl", "reload", "dnsmasq"])
        .output();

    Ok(())
}

pub async fn add_static_lease(
    Json(payload): Json<AddStaticLease>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut leases = load_static_leases();

    // Check for duplicate
    if leases.iter().any(|l| l.mac_address.to_lowercase() == payload.mac_address.to_lowercase()) {
        return Err((StatusCode::BAD_REQUEST, "MAC address already has a static lease".to_string()));
    }

    leases.push(StaticLease {
        mac_address: payload.mac_address,
        ip_address: payload.ip_address,
        hostname: payload.hostname.unwrap_or_default(),
    });

    save_static_leases(&leases)?;

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove_static_lease(
    Json(payload): Json<RemoveStaticLease>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut leases = load_static_leases();
    leases.retain(|l| l.mac_address.to_lowercase() != payload.mac_address.to_lowercase());
    save_static_leases(&leases)?;

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn update_dhcp_config(
    Json(payload): Json<UpdateDhcpConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    // Read current config
    let current = fs::read_to_string(DNSMASQ_CONF)
        .or_else(|_| fs::read_to_string("/etc/dnsmasq.conf"))
        .unwrap_or_default();

    // Update dhcp-range line
    let new_range = format!("dhcp-range={},{},{}", payload.range_start, payload.range_end, payload.lease_time);

    let mut new_content = String::new();
    let mut found_range = false;

    for line in current.lines() {
        if line.trim().starts_with("dhcp-range=") {
            new_content.push_str(&new_range);
            new_content.push('\n');
            found_range = true;
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    if !found_range {
        new_content.push_str(&new_range);
        new_content.push('\n');
    }

    fs::write(DNSMASQ_CONF, &new_content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Reload dnsmasq
    Command::new("sudo")
        .args(["systemctl", "reload", "dnsmasq"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"success": true})))
}

// ============ WIFI ============

#[derive(Debug, Serialize)]
pub struct WifiConfig {
    pub enabled: bool,
    pub ssid: String,
    pub password: String,
    pub channel: u32,
    pub hw_mode: String,
    pub security: String,
    pub hidden: bool,
    pub country_code: String,
}

pub async fn wifi_status() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::network::wifi_status()));
    }

    let content = fs::read_to_string(HOSTAPD_CONF).unwrap_or_default();

    let mut config = WifiConfig {
        enabled: true,
        ssid: String::new(),
        password: String::new(),
        channel: 1,
        hw_mode: "g".to_string(),
        security: "WPA2".to_string(),
        hidden: false,
        country_code: "US".to_string(),
    };

    for line in content.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "ssid" => config.ssid = value.to_string(),
                "wpa_passphrase" => config.password = value.to_string(),
                "channel" => config.channel = value.parse().unwrap_or(1),
                "hw_mode" => config.hw_mode = value.to_string(),
                "wpa" => {
                    config.security = match value {
                        "1" => "WPA",
                        "2" => "WPA2",
                        "3" => "WPA/WPA2",
                        _ => "WPA2",
                    }.to_string();
                }
                "ignore_broadcast_ssid" => config.hidden = value == "1",
                "country_code" => config.country_code = value.to_string(),
                _ => {}
            }
        }
    }

    // Check if hostapd is running
    config.enabled = Command::new("systemctl")
        .args(["is-active", "hostapd"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false);

    Ok(Json(serde_json::to_value(config).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct UpdateWifiConfig {
    pub ssid: Option<String>,
    pub password: Option<String>,
    pub channel: Option<u32>,
    pub hidden: Option<bool>,
}

pub async fn update_wifi(
    Json(payload): Json<UpdateWifiConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let content = fs::read_to_string(HOSTAPD_CONF)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut new_content = String::new();

    for line in content.lines() {
        let line_trimmed = line.trim();

        if let Some(ref ssid) = payload.ssid {
            if line_trimmed.starts_with("ssid=") {
                new_content.push_str(&format!("ssid={}\n", ssid));
                continue;
            }
        }

        if let Some(ref password) = payload.password {
            if line_trimmed.starts_with("wpa_passphrase=") {
                new_content.push_str(&format!("wpa_passphrase={}\n", password));
                continue;
            }
        }

        if let Some(channel) = payload.channel {
            if line_trimmed.starts_with("channel=") {
                new_content.push_str(&format!("channel={}\n", channel));
                continue;
            }
        }

        if let Some(hidden) = payload.hidden {
            if line_trimmed.starts_with("ignore_broadcast_ssid=") {
                new_content.push_str(&format!("ignore_broadcast_ssid={}\n", if hidden { "1" } else { "0" }));
                continue;
            }
        }

        new_content.push_str(line);
        new_content.push('\n');
    }

    // Write config
    Command::new("sudo")
        .args(["tee", HOSTAPD_CONF])
        .stdin(std::process::Stdio::piped())
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    fs::write("/tmp/hostapd.conf.new", &new_content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Command::new("sudo")
        .args(["cp", "/tmp/hostapd.conf.new", HOSTAPD_CONF])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Restart hostapd
    Command::new("sudo")
        .args(["systemctl", "restart", "hostapd"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn toggle_wifi(
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "enabled": enabled, "mock": true})));
    }

    let action = if enabled { "start" } else { "stop" };

    Command::new("sudo")
        .args(["systemctl", action, "hostapd"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"success": true, "enabled": enabled})))
}

// ============ DNS ============

#[derive(Debug, Serialize)]
pub struct DnsConfig {
    pub upstream_servers: Vec<String>,
    pub local_entries: Vec<LocalDnsEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalDnsEntry {
    pub hostname: String,
    pub ip_address: String,
}

#[derive(Debug, Deserialize)]
pub struct AddLocalDns {
    pub hostname: String,
    pub ip_address: String,
}

#[derive(Debug, Deserialize)]
pub struct RemoveLocalDns {
    pub hostname: String,
}

pub async fn dns_status() -> Result<Json<DnsConfig>, (StatusCode, String)> {
    let content = fs::read_to_string(DNSMASQ_CONF)
        .or_else(|_| fs::read_to_string("/etc/dnsmasq.conf"))
        .unwrap_or_default();

    let mut upstream_servers = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("server=") {
            upstream_servers.push(line.trim_start_matches("server=").to_string());
        }
    }

    let local_entries = load_local_dns();

    Ok(Json(DnsConfig {
        upstream_servers,
        local_entries,
    }))
}

fn load_local_dns() -> Vec<LocalDnsEntry> {
    let content = fs::read_to_string(LOCAL_DNS_FILE).unwrap_or_default();
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("address=/") {
            // Format: address=/hostname/ip
            let parts: Vec<&str> = line.trim_start_matches("address=/").split('/').collect();
            if parts.len() >= 2 {
                entries.push(LocalDnsEntry {
                    hostname: parts[0].to_string(),
                    ip_address: parts[1].to_string(),
                });
            }
        }
    }

    entries
}

fn save_local_dns(entries: &[LocalDnsEntry]) -> Result<(), (StatusCode, String)> {
    let mut content = String::from("# Local DNS entries - managed by RouterUI\n");
    for entry in entries {
        content.push_str(&format!("address=/{}/{}\n", entry.hostname, entry.ip_address));
    }

    fs::write(LOCAL_DNS_FILE, &content)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Reload dnsmasq
    let _ = Command::new("sudo")
        .args(["systemctl", "reload", "dnsmasq"])
        .output();

    Ok(())
}

pub async fn add_local_dns(
    Json(payload): Json<AddLocalDns>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut entries = load_local_dns();

    // Check for duplicate
    if entries.iter().any(|e| e.hostname == payload.hostname) {
        return Err((StatusCode::BAD_REQUEST, "Hostname already exists".to_string()));
    }

    entries.push(LocalDnsEntry {
        hostname: payload.hostname,
        ip_address: payload.ip_address,
    });

    save_local_dns(&entries)?;

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove_local_dns(
    Json(payload): Json<RemoveLocalDns>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut entries = load_local_dns();
    entries.retain(|e| e.hostname != payload.hostname);
    save_local_dns(&entries)?;

    Ok(Json(serde_json::json!({"success": true})))
}

// ============ STATIC ROUTES ============

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StaticRoute {
    pub destination: String,
    pub gateway: String,
    pub interface: Option<String>,
    pub metric: Option<u32>,
}

pub async fn routes() -> Result<Json<Vec<StaticRoute>>, (StatusCode, String)> {
    let output = Command::new("ip")
        .args(["route", "show"])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let routes_str = String::from_utf8_lossy(&output.stdout);
    let mut routes = Vec::new();

    for line in routes_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let destination = parts[0].to_string();
        let mut gateway = String::new();
        let mut interface = None;
        let mut metric = None;

        let mut i = 1;
        while i < parts.len() {
            match parts[i] {
                "via" if i + 1 < parts.len() => {
                    gateway = parts[i + 1].to_string();
                    i += 2;
                }
                "dev" if i + 1 < parts.len() => {
                    interface = Some(parts[i + 1].to_string());
                    i += 2;
                }
                "metric" if i + 1 < parts.len() => {
                    metric = parts[i + 1].parse().ok();
                    i += 2;
                }
                _ => i += 1,
            }
        }

        routes.push(StaticRoute {
            destination,
            gateway,
            interface,
            metric,
        });
    }

    Ok(Json(routes))
}

#[derive(Debug, Deserialize)]
pub struct AddRoute {
    pub destination: String,
    pub gateway: String,
    pub interface: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemoveRoute {
    pub destination: String,
}

pub async fn add_route(
    Json(payload): Json<AddRoute>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut args = vec!["ip", "route", "add", &payload.destination, "via", &payload.gateway];

    let iface;
    if let Some(ref interface) = payload.interface {
        iface = interface.clone();
        args.push("dev");
        args.push(&iface);
    }

    let output = Command::new("sudo")
        .args(&args)
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    // Save to persistent storage
    save_route_persistent(&payload)?;

    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn remove_route(
    Json(payload): Json<RemoveRoute>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let output = Command::new("sudo")
        .args(["ip", "route", "del", &payload.destination])
        .output()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !output.status.success() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR,
            String::from_utf8_lossy(&output.stderr).to_string()));
    }

    // Remove from persistent storage
    remove_route_persistent(&payload.destination)?;

    Ok(Json(serde_json::json!({"success": true})))
}

fn save_route_persistent(route: &AddRoute) -> Result<(), (StatusCode, String)> {
    let mut routes = load_persistent_routes();
    routes.push(StaticRoute {
        destination: route.destination.clone(),
        gateway: route.gateway.clone(),
        interface: route.interface.clone(),
        metric: None,
    });

    let json = serde_json::to_string_pretty(&routes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(STATIC_ROUTES_FILE, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn remove_route_persistent(destination: &str) -> Result<(), (StatusCode, String)> {
    let mut routes = load_persistent_routes();
    routes.retain(|r| r.destination != destination);

    let json = serde_json::to_string_pretty(&routes)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(STATIC_ROUTES_FILE, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(())
}

fn load_persistent_routes() -> Vec<StaticRoute> {
    fs::read_to_string(STATIC_ROUTES_FILE)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

// ============ WAKE ON LAN ============

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WolDevice {
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<String>,
}

pub async fn wol_devices() -> Result<Json<Vec<WolDevice>>, (StatusCode, String)> {
    let devices = load_wol_devices();
    Ok(Json(devices))
}

fn load_wol_devices() -> Vec<WolDevice> {
    fs::read_to_string(WOL_DEVICES_FILE)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_wol_devices(devices: &[WolDevice]) -> Result<(), (StatusCode, String)> {
    let json = serde_json::to_string_pretty(devices)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    fs::write(WOL_DEVICES_FILE, json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct AddWolDevice {
    pub name: String,
    pub mac_address: String,
    pub ip_address: Option<String>,
}

pub async fn add_wol_device(
    Json(payload): Json<AddWolDevice>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut devices = load_wol_devices();

    devices.push(WolDevice {
        name: payload.name,
        mac_address: payload.mac_address,
        ip_address: payload.ip_address,
    });

    save_wol_devices(&devices)?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Debug, Deserialize)]
pub struct RemoveWolDevice {
    pub mac_address: String,
}

pub async fn remove_wol_device(
    Json(payload): Json<RemoveWolDevice>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let mut devices = load_wol_devices();
    devices.retain(|d| d.mac_address.to_lowercase() != payload.mac_address.to_lowercase());
    save_wol_devices(&devices)?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Debug, Deserialize)]
pub struct WakeDevice {
    pub mac_address: String,
}

pub async fn wake_device(
    Json(payload): Json<WakeDevice>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({
            "success": true,
            "message": format!("Wake packet sent to {} (mock)", payload.mac_address),
            "mock": true
        })));
    }

    // Try etherwake first, then wakeonlan
    let result = Command::new("sudo")
        .args(["etherwake", "-i", "enp2s0", &payload.mac_address])
        .output();

    if result.is_err() || !result.as_ref().unwrap().status.success() {
        // Fallback to wakeonlan
        Command::new("wakeonlan")
            .args([&payload.mac_address])
            .output()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Wake packet sent to {}", payload.mac_address)
    })))
}
