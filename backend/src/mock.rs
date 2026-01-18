use std::env;

pub fn is_mock_mode() -> bool {
    env::var("ROUTERUI_MOCK")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

// Mock data for dashboard
pub mod dashboard {
    use serde_json::json;

    pub fn overview() -> serde_json::Value {
        json!({
            "system": {
                "hostname": "mock-router",
                "uptime_seconds": 86400,
                "uptime_formatted": "1 day, 0:00:00",
                "cpu_usage": 15.5,
                "memory": {
                    "total_mb": 16000,
                    "used_mb": 4000,
                    "percent_used": 25.0
                },
                "storage": {
                    "total_mb": 500000,
                    "used_mb": 125000,
                    "percent_used": 25.0
                }
            },
            "wan_status": {
                "connected": true,
                "interface": "enp1s0",
                "ip_address": "192.168.12.100",
                "gateway": "192.168.12.1"
            },
            "interfaces": [
                {
                    "name": "enp1s0",
                    "state": "UP",
                    "mac_address": "00:11:22:33:44:55",
                    "ipv4": "192.168.12.100",
                    "rx_bytes": 1073741824,
                    "tx_bytes": 536870912
                },
                {
                    "name": "enp2s0",
                    "state": "UP",
                    "mac_address": "00:11:22:33:44:56",
                    "ipv4": "10.22.22.1",
                    "rx_bytes": 2147483648_i64,
                    "tx_bytes": 1073741824_i64
                },
                {
                    "name": "wlo1",
                    "state": "UP",
                    "mac_address": "00:11:22:33:44:57",
                    "ipv4": null,
                    "rx_bytes": 536870912,
                    "tx_bytes": 268435456
                }
            ],
            "services": [
                { "name": "sshd", "display_name": "SSH Server", "status": "active" },
                { "name": "dnsmasq", "display_name": "DHCP/DNS", "status": "active" },
                { "name": "docker", "display_name": "Docker", "status": "active" },
                { "name": "adguardhome", "display_name": "AdGuard Home", "status": "active" }
            ]
        })
    }
}

// Mock data for network
pub mod network {
    use serde_json::json;

    pub fn interfaces() -> serde_json::Value {
        json!([
            {
                "name": "enp1s0",
                "interface_type": "wan",
                "state": "UP",
                "mac_address": "00:11:22:33:44:55",
                "ipv4": "192.168.12.100",
                "ipv6": null,
                "rx_bytes": 1073741824,
                "tx_bytes": 536870912,
                "speed": "1000Mb/s"
            },
            {
                "name": "enp2s0",
                "interface_type": "lan",
                "state": "UP",
                "mac_address": "00:11:22:33:44:56",
                "ipv4": "10.22.22.1",
                "ipv6": null,
                "rx_bytes": 2147483648_i64,
                "tx_bytes": 1073741824_i64,
                "speed": "1000Mb/s"
            },
            {
                "name": "wlo1",
                "interface_type": "wifi",
                "state": "UP",
                "mac_address": "00:11:22:33:44:57",
                "ipv4": null,
                "ipv6": null,
                "rx_bytes": 536870912,
                "tx_bytes": 268435456,
                "speed": null
            },
            {
                "name": "tailscale0",
                "interface_type": "vpn",
                "state": "Active",
                "mac_address": null,
                "ipv4": "100.100.100.1",
                "ipv6": null,
                "rx_bytes": 104857600,
                "tx_bytes": 52428800,
                "speed": null
            }
        ])
    }

    pub fn dhcp_status() -> serde_json::Value {
        json!({
            "enabled": true,
            "interface": "br0",
            "range_start": "10.22.22.100",
            "range_end": "10.22.22.200",
            "lease_time": "24h",
            "gateway": "10.22.22.1",
            "dns": ["10.22.22.1"],
            "leases": [
                { "mac": "aa:bb:cc:dd:ee:01", "ip": "10.22.22.131", "hostname": "Pixel-7-Pro", "expires": "2026-01-19 10:00:00" },
                { "mac": "aa:bb:cc:dd:ee:02", "ip": "10.22.22.185", "hostname": "desktop-pc", "expires": "2026-01-19 12:00:00" }
            ],
            "static_leases": []
        })
    }

    pub fn wifi_status() -> serde_json::Value {
        json!({
            "enabled": true,
            "interface": "wlo1",
            "ssid": "MockNetwork",
            "channel": 6,
            "band": "2.4GHz",
            "security": "WPA2",
            "connected_clients": 3
        })
    }
}

// Mock data for firewall
pub mod firewall {
    use serde_json::json;

    pub fn status() -> serde_json::Value {
        json!({
            "enabled": true,
            "default_policy": "DROP",
            "rules_count": 12
        })
    }

    pub fn rules() -> serde_json::Value {
        json!([
            { "chain": "INPUT", "target": "ACCEPT", "protocol": "all", "source": "0.0.0.0/0", "interface": "lo" },
            { "chain": "INPUT", "target": "ACCEPT", "protocol": "all", "source": "0.0.0.0/0", "interface": "br0" },
            { "chain": "INPUT", "target": "ACCEPT", "protocol": "all", "source": "0.0.0.0/0", "state": "ESTABLISHED,RELATED" }
        ])
    }

    pub fn port_forwards() -> serde_json::Value {
        json!([])
    }
}

// Mock data for security
pub mod security {
    use serde_json::json;

    pub fn overview() -> serde_json::Value {
        json!({
            "firewall_drops_24h": 156,
            "blocklist_hits": {
                "spamhaus_drop": 12,
                "emerging_threats": 8
            },
            "failed_ssh_attempts_24h": 3,
            "active_connections": 24,
            "recent_events": [
                {
                    "timestamp": "2026-01-18T10:30:00",
                    "event_type": "Failed Login",
                    "source_ip": "192.168.12.50",
                    "details": "Failed password for invalid user admin",
                    "severity": "high",
                    "is_external": true
                },
                {
                    "timestamp": "2026-01-18T10:25:00",
                    "event_type": "Successful Login",
                    "source_ip": "10.22.22.185",
                    "details": "Accepted publickey for claudeadmin",
                    "severity": "info",
                    "is_external": false
                }
            ],
            "top_blocked_ips": [
                { "ip": "45.155.205.100", "hits": 50, "last_seen": "2026-01-18 10:00", "reason": "Blocklist" }
            ],
            "ssh_sessions": [
                { "user": "claudeadmin", "source_ip": "10.22.22.185", "timestamp": "2026-01-18 09:00", "status": "Active" }
            ]
        })
    }

    pub fn connections() -> serde_json::Value {
        json!([
            { "local_addr": "10.22.22.1:22", "remote_addr": "10.22.22.185:54321", "state": "ESTABLISHED", "process": "sshd" },
            { "local_addr": "10.22.22.1:8080", "remote_addr": "10.22.22.185:54322", "state": "ESTABLISHED", "process": "routerui" }
        ])
    }
}

// Mock data for media
pub mod media {
    use serde_json::json;

    pub fn overview() -> serde_json::Value {
        json!({
            "storage": {
                "total_gb": 5588.72,
                "used_gb": 898.25,
                "free_gb": 4690.47,
                "percent_used": 16.1,
                "mount_point": "/mnt/external"
            },
            "library": {
                "movies": 158,
                "tv_shows": 86
            },
            "recent_movies": [
                { "title": "The Matrix (1999)", "date": "2026-01-18", "status": "Imported", "quality": "Bluray-1080p", "size_mb": 0 },
                { "title": "Inception (2010)", "date": "2026-01-17", "status": "Imported", "quality": "Bluray-1080p", "size_mb": 0 },
                { "title": "Interstellar (2014)", "date": "2026-01-17", "status": "Imported", "quality": "Bluray-2160p", "size_mb": 0 }
            ],
            "recent_shows": [
                { "title": "Breaking Bad S01E01", "date": "2026-01-18", "status": "Imported", "quality": "HDTV-1080p", "size_mb": 0 },
                { "title": "The Office S05E10", "date": "2026-01-17", "status": "Imported", "quality": "WEBRip-1080p", "size_mb": 0 }
            ],
            "jellyfin": {
                "movie_count": 218,
                "series_count": 86,
                "episode_count": 836,
                "active_streams": 1,
                "server_name": "MockJellyfin",
                "version": "10.11.5"
            }
        })
    }
}

// Mock data for AdGuard
pub mod adguard {
    use serde_json::json;

    pub fn overview() -> serde_json::Value {
        json!({
            "protection_enabled": true,
            "dns_queries": 125000,
            "blocked_filtering": 15000,
            "blocked_percentage": 12.0,
            "avg_processing_time": 5.2
        })
    }

    pub fn querylog() -> serde_json::Value {
        json!([
            { "time": "2026-01-18T10:30:00Z", "client": "10.22.22.185", "question": { "name": "google.com", "qtype": "A" }, "reason": "NotFilteredNotFound" },
            { "time": "2026-01-18T10:29:55Z", "client": "10.22.22.131", "question": { "name": "ads.example.com", "qtype": "A" }, "reason": "FilteredBlackList" },
            { "time": "2026-01-18T10:29:50Z", "client": "10.22.22.185", "question": { "name": "github.com", "qtype": "A" }, "reason": "NotFilteredNotFound" }
        ])
    }

    pub fn filters() -> serde_json::Value {
        json!({
            "filters": [
                { "id": 1, "name": "AdGuard DNS filter", "enabled": true, "rules_count": 50000 },
                { "id": 2, "name": "AdAway Default Blocklist", "enabled": true, "rules_count": 6000 }
            ],
            "user_rules": ["@@||example.com^", "||ads.badsite.com^"]
        })
    }
}

// Mock data for Docker
pub mod docker {
    use serde_json::json;

    pub fn status() -> serde_json::Value {
        json!({
            "installed": true,
            "running": true,
            "version": "24.0.7"
        })
    }

    pub fn containers() -> serde_json::Value {
        json!([
            { "id": "abc123", "name": "radarr", "image": "linuxserver/radarr", "status": "Up 2 days", "state": "running", "ports": "7878:7878" },
            { "id": "def456", "name": "sonarr", "image": "linuxserver/sonarr", "status": "Up 2 days", "state": "running", "ports": "8989:8989" },
            { "id": "ghi789", "name": "transmission", "image": "linuxserver/transmission", "status": "Up 2 days", "state": "running", "ports": "9091:9091" }
        ])
    }
}

// Mock data for VPN
pub mod vpn {
    use serde_json::json;

    pub fn overview() -> serde_json::Value {
        json!({
            "tailscale": {
                "installed": true,
                "running": true,
                "ip": "100.100.100.1",
                "hostname": "mock-router",
                "exit_node": null
            },
            "gluetun": {
                "installed": true,
                "running": true,
                "provider": "nordvpn",
                "server": "us-nyc-001"
            }
        })
    }
}

// Mock data for services
pub mod services {
    use serde_json::json;

    pub fn list() -> serde_json::Value {
        json!([
            { "name": "sshd", "display_name": "SSH Server", "status": "active", "enabled": true },
            { "name": "dnsmasq", "display_name": "DHCP/DNS", "status": "active", "enabled": true },
            { "name": "docker", "display_name": "Docker", "status": "active", "enabled": true },
            { "name": "adguardhome", "display_name": "AdGuard Home", "status": "active", "enabled": true },
            { "name": "tailscaled", "display_name": "Tailscale", "status": "active", "enabled": true }
        ])
    }
}

// Mock data for system
pub mod system {
    use serde_json::json;

    pub fn status() -> serde_json::Value {
        json!({
            "hostname": "mock-router",
            "os": "Ubuntu 24.04 LTS",
            "kernel": "6.5.0-generic",
            "uptime": "1 day, 5 hours",
            "cpu_model": "Intel N150",
            "cpu_cores": 4,
            "memory_total_mb": 16000,
            "memory_used_mb": 4000
        })
    }
}
