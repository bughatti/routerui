// UPnP IGD + NAT-PMP support via miniupnpd (the standard Linux UPnP daemon).
//
// UPnP lets LAN devices (consoles, some apps) open WAN ports on themselves
// automatically. That is a convenience *and* an attack surface, so this is OFF
// by default and only ever configured when the user explicitly enables it.
//
// When enabled we write a locked-down /etc/miniupnpd/miniupnpd.conf:
//   - secure_mode=yes  (a client may only map ports to its own IP)
//   - listening only on the LAN interface, external mappings only on the WAN
//   - an allow rule scoped to the LAN subnet, then a deny-all default
// Active mappings are read back from miniupnpd's lease file.

use axum::{extract::Json, http::StatusCode};
use serde::Serialize;
use std::process::Command;

use crate::mock;
use super::AuthUser;

const MINIUPNPD_CONF: &str = "/etc/miniupnpd/miniupnpd.conf";
const LEASE_FILE: &str = "/var/lib/miniupnpd/upnp.leases";

type ApiErr = (StatusCode, String);
fn err500<E: std::fmt::Display>(e: E) -> ApiErr {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

// ── helpers ────────────────────────────────────────────────────────────────

fn sh(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(cmd).args(args).output()
}
fn sudo(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("sudo").args(args).output()
}

fn miniupnpd_installed() -> bool {
    sh("which", &["miniupnpd"]).map(|o| o.status.success()).unwrap_or(false)
}
/// True when `systemctl <verb> miniupnpd` reports the expected single word.
fn systemctl_is(verb: &str, want: &str) -> bool {
    sh("systemctl", &[verb, "miniupnpd"])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == want)
        .unwrap_or(false)
}

/// The WAN interface (the one carrying the default route). Mirrors the helper
/// in wireguard.rs so UPnP maps ports on the same interface NAT uses.
fn wan_iface() -> String {
    sh("ip", &["route", "show", "default"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .and_then(|s| {
            s.split_whitespace()
                .skip_while(|w| *w != "dev")
                .nth(1)
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

// ── handlers ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UpnpStatus {
    pub installed: bool,
    pub enabled: bool,
    pub running: bool,
    pub wan_interface: String,
}

pub async fn status() -> Result<Json<UpnpStatus>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(UpnpStatus {
            installed: true,
            enabled: false,
            running: false,
            wan_interface: "eth0".into(),
        }));
    }
    Ok(Json(UpnpStatus {
        installed: miniupnpd_installed(),
        enabled: systemctl_is("is-enabled", "enabled"),
        running: systemctl_is("is-active", "active"),
        wan_interface: wan_iface(),
    }))
}

/// Installs miniupnpd if needed, writes a hardened config scoped to the LAN,
/// and enables + starts the service. Off until the user calls this.
pub async fn enable(_user: AuthUser) -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }

    let wan = wan_iface();
    if wan.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "could not determine the WAN interface".into()));
    }
    // Detect the LAN interface here (shared helper) rather than in the script,
    // so virtual interfaces like docker0/veth/wg are never mistaken for the LAN.
    let lan = super::netutil::lan_iface();
    if lan.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "could not determine the LAN interface".into()));
    }

    // The config is written with a LAN-scoped allow rule followed by a deny-all
    // default, and secure_mode so a client can only ever map ports to its own
    // address. The LAN's subnet is derived from the chosen interface.
    let script = format!(
        r#"set -e
export DEBIAN_FRONTEND=noninteractive
command -v miniupnpd >/dev/null || {{ apt-get update -qq; apt-get install -y -qq miniupnpd >/dev/null 2>&1; }}
mkdir -p /etc/miniupnpd /var/lib/miniupnpd

WAN="{wan}"
LAN="{lan}"
LAN_CIDR=$(ip -o -4 addr show dev "$LAN" | awk '{{print $4}}' | head -n1)
if [ -z "$LAN_CIDR" ]; then echo "LAN interface $LAN has no IPv4 address" >&2; exit 1; fi
# Network address of the LAN subnet (e.g. 192.168.1.0/24) for the allow rule.
LAN_SUBNET=$(ipcalc -n "$LAN_CIDR" 2>/dev/null | awk -F= '/^NETWORK=/{{print $2}}')
PREFIX=${{LAN_CIDR#*/}}
if [ -z "$LAN_SUBNET" ]; then
  IP=${{LAN_CIDR%/*}}; O1=${{IP%%.*}}; R=${{IP#*.}}; O2=${{R%%.*}}; R=${{R#*.}}; O3=${{R%%.*}}
  LAN_SUBNET="$O1.$O2.$O3.0"
fi

UUID=$(cat /proc/sys/kernel/random/uuid)
cat > {conf} <<CONF
# Managed by RouterUI. UPnP IGD + NAT-PMP, locked to the LAN.
ext_ifname=$WAN
listening_ip=$LAN
enable_natpmp=yes
enable_upnp=yes
secure_mode=yes
system_uptime=yes
notify_interval=60
clean_ruleset_interval=600
uuid=$UUID
lease_file={lease}
# Only allow LAN clients to request modest port ranges to their own subnet,
# then deny everything else by default.
allow 1024-65535 $LAN_SUBNET/$PREFIX 1024-65535
deny 0-65535 0.0.0.0/0 0-65535
CONF

# Debian gate that must be flipped on for the service to run.
if [ -f /etc/default/miniupnpd ]; then
  sed -i 's/^START_DAEMON=.*/START_DAEMON="yes"/' /etc/default/miniupnpd || true
  grep -q '^START_DAEMON=' /etc/default/miniupnpd || echo 'START_DAEMON="yes"' >> /etc/default/miniupnpd
fi

: > {lease}
systemctl enable miniupnpd >/dev/null 2>&1 || true
systemctl restart miniupnpd
echo OK
"#,
        wan = wan,
        lan = lan,
        conf = MINIUPNPD_CONF,
        lease = LEASE_FILE,
    );

    let o = sudo(&["bash", "-c", &script]).map_err(err500)?;
    if !o.status.success() {
        return Err(err500(format!(
            "miniupnpd setup failed: {}",
            String::from_utf8_lossy(&o.stderr)
        )));
    }
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "UPnP/NAT-PMP enabled (LAN devices can now request port mappings)."
    })))
}

pub async fn disable(_user: AuthUser) -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }
    let _ = sudo(&["systemctl", "stop", "miniupnpd"]);
    let _ = sudo(&["systemctl", "disable", "miniupnpd"]);
    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Serialize)]
pub struct UpnpMapping {
    pub protocol: String,
    pub external_port: u16,
    pub internal_ip: String,
    pub internal_port: u16,
    pub description: String,
}

/// Active mappings, read from miniupnpd's lease file. Each line is
/// `PROTO:eport:internal_ip:iport:timestamp:description`. Missing/empty file
/// (UPnP off or nothing mapped) yields an empty list rather than an error.
pub async fn mappings() -> Result<Json<Vec<UpnpMapping>>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![]));
    }
    // Read via sudo since the lease file is root-owned.
    let out = match sudo(&["cat", LEASE_FILE]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Ok(Json(vec![])),
    };

    let mut list = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.splitn(6, ':').collect();
        if f.len() < 5 {
            continue;
        }
        list.push(UpnpMapping {
            protocol: f[0].to_uppercase(),
            external_port: f[1].parse().unwrap_or(0),
            internal_ip: f[2].to_string(),
            internal_port: f[3].parse().unwrap_or(0),
            description: f.get(5).unwrap_or(&"").to_string(),
        });
    }
    Ok(Json(list))
}
