use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemStatus {
    pub hostname: String,
    pub uptime: String,
    pub load_average: Vec<f64>,
    pub memory: MemoryInfo,
    pub storage: StorageInfo,
    pub cpu_cores: u32,
    pub cpu_usage: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_mb: u64,
    pub used_mb: u64,
    pub free_mb: u64,
    pub percent_used: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageInfo {
    pub total_gb: f64,
    pub used_gb: f64,
    pub free_gb: f64,
    pub percent_used: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub state: String,
    pub mac_address: Option<String>,
    pub ipv4: Option<String>,
    pub ipv6: Vec<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub enabled: bool,
}

pub fn get_system_status() -> Result<SystemStatus, std::io::Error> {
    let hostname = std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    let uptime_output = Command::new("uptime")
        .arg("-p")
        .output()?;
    let uptime = String::from_utf8_lossy(&uptime_output.stdout).trim().to_string();

    let loadavg = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    let load_parts: Vec<f64> = loadavg
        .split_whitespace()
        .take(3)
        .filter_map(|s| s.parse().ok())
        .collect();

    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let memory = parse_meminfo(&meminfo);

    let cpu_cores = std::thread::available_parallelism()
        .map(|p| p.get() as u32)
        .unwrap_or(1);

    // CPU usage: 1-minute load average as percentage of cores
    let cpu_usage = if !load_parts.is_empty() && cpu_cores > 0 {
        ((load_parts[0] / cpu_cores as f64) * 100.0).min(100.0)
    } else {
        0.0
    };

    let storage = get_storage_info();

    Ok(SystemStatus {
        hostname,
        uptime,
        load_average: load_parts,
        memory,
        storage,
        cpu_cores,
        cpu_usage,
    })
}

fn get_storage_info() -> StorageInfo {
    let output = Command::new("df")
        .args(["-B1", "/"])
        .output()
        .ok();
    
    if let Some(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > 1 {
            let parts: Vec<&str> = lines[1].split_whitespace().collect();
            if parts.len() >= 4 {
                let total: f64 = parts[1].parse().unwrap_or(0.0) / 1_073_741_824.0;
                let used: f64 = parts[2].parse().unwrap_or(0.0) / 1_073_741_824.0;
                let free: f64 = parts[3].parse().unwrap_or(0.0) / 1_073_741_824.0;
                let percent = if total > 0.0 { (used / total) * 100.0 } else { 0.0 };
                return StorageInfo {
                    total_gb: (total * 10.0).round() / 10.0,
                    used_gb: (used * 10.0).round() / 10.0,
                    free_gb: (free * 10.0).round() / 10.0,
                    percent_used: (percent * 10.0).round() / 10.0,
                };
            }
        }
    }
    StorageInfo { total_gb: 0.0, used_gb: 0.0, free_gb: 0.0, percent_used: 0.0 }
}

fn parse_meminfo(content: &str) -> MemoryInfo {
    let mut total = 0u64;
    let mut available = 0u64;

    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_kb_value(line);
        } else if line.starts_with("MemAvailable:") {
            available = parse_kb_value(line);
        }
    }

    let total_mb = total / 1024;
    let free_mb = available / 1024;
    let used_mb = total_mb.saturating_sub(free_mb);
    let percent_used = if total_mb > 0 {
        (used_mb as f64 / total_mb as f64) * 100.0
    } else {
        0.0
    };

    MemoryInfo { total_mb, used_mb, free_mb, percent_used }
}

fn parse_kb_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub fn get_interfaces() -> Result<Vec<NetworkInterface>, std::io::Error> {
    let output = Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()?;
    
    let json_str = String::from_utf8_lossy(&output.stdout);
    
    let interfaces: Vec<NetworkInterface> = serde_json::from_str(&json_str)
        .map(|v: Vec<serde_json::Value>| {
            v.into_iter()
                .filter_map(|iface| parse_interface(&iface))
                .collect()
        })
        .unwrap_or_default();

    Ok(interfaces)
}

fn parse_interface(value: &serde_json::Value) -> Option<NetworkInterface> {
    let name = value.get("ifname")?.as_str()?.to_string();
    
    if name == "lo" { return None; }

    let state = value.get("operstate")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();

    let mac_address = value.get("address")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let addr_info = value.get("addr_info").and_then(|v| v.as_array());
    
    let mut ipv4 = None;
    let mut ipv6 = Vec::new();

    if let Some(addrs) = addr_info {
        for addr in addrs {
            let family = addr.get("family").and_then(|v| v.as_str());
            let local = addr.get("local").and_then(|v| v.as_str());
            let prefixlen = addr.get("prefixlen").and_then(|v| v.as_u64());

            if let (Some(family), Some(local), Some(prefix)) = (family, local, prefixlen) {
                let addr_str = format!("{}/{}", local, prefix);
                if family == "inet" && ipv4.is_none() {
                    ipv4 = Some(addr_str);
                } else if family == "inet6" && !local.starts_with("fe80") {
                    ipv6.push(addr_str);
                }
            }
        }
    }

    let rx_bytes = std::fs::read_to_string(format!("/sys/class/net/{}/statistics/rx_bytes", name))
        .ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);

    let tx_bytes = std::fs::read_to_string(format!("/sys/class/net/{}/statistics/tx_bytes", name))
        .ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);

    // Improve state display for virtual interfaces
    let final_state = if state == "UNKNOWN" && ipv4.is_some() { "Active".to_string() } else { state };
    Some(NetworkInterface { name, state: final_state, mac_address, ipv4, ipv6, rx_bytes, tx_bytes })
}

pub fn get_services() -> Result<Vec<ServiceStatus>, std::io::Error> {
    let services_to_check = vec![
        ("dnsmasq", "DHCP/DNS Server"),
        ("hostapd", "WiFi Access Point"),
        ("cloudflared", "Cloudflare Tunnel"),
        ("AdGuardHome", "Ad Blocker"),
        ("docker", "Docker"),
        ("ssh", "SSH Server"),
        ("netfilter-persistent", "Firewall"),
    ];

    let mut statuses = Vec::new();
    for (name, display_name) in services_to_check {
        let status_output = Command::new("systemctl").args(["is-active", name]).output()?;
        let status = String::from_utf8_lossy(&status_output.stdout).trim().to_string();
        let enabled_output = Command::new("systemctl").args(["is-enabled", name]).output()?;
        let enabled = String::from_utf8_lossy(&enabled_output.stdout).trim() == "enabled";
        statuses.push(ServiceStatus { name: name.to_string(), display_name: display_name.to_string(), status, enabled });
    }
    Ok(statuses)
}
