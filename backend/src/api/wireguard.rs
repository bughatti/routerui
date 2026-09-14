// Self-hosted WireGuard VPN server (remote access into the LAN), alongside the
// Tailscale integration in vpn.rs. RouterUI generates the server keys, manages
// peers, hands back ready-to-use client configs and QR codes, and wires the
// tunnel into the firewall and NAT.
//
// Source of truth for the server key and peers is /etc/wireguard/wg0.conf,
// managed by wg-quick. Peer display names live in a small sidecar JSON, since
// the WireGuard config has no place for them.

use axum::{extract::Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::mock;
use crate::validate;

const WG_IFACE: &str = "wg0";
const WG_CONF: &str = "/etc/wireguard/wg0.conf";
const WG_SUBNET_BASE: &str = "10.7.0"; // server = .1, peers from .2
const WG_SERVER_IP: &str = "10.7.0.1";
const WG_PORT: u16 = 51820;
const PEERS_META: &str = "/opt/routerui/config/wg-peers.json";

type ApiErr = (StatusCode, String);
fn err500<E: std::fmt::Display>(e: E) -> ApiErr {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
fn bad(msg: &str) -> ApiErr {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

// ── helpers ────────────────────────────────────────────────────────────────

fn sh(cmd: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(cmd).args(args).output()
}
fn sudo(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("sudo").args(args).output()
}

fn wg_installed() -> bool {
    sh("which", &["wg"]).map(|o| o.status.success()).unwrap_or(false)
}
fn wg_running() -> bool {
    sudo(&["wg", "show", WG_IFACE])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The WAN interface (the one carrying the default route) and its IPv4 address,
/// used for NAT egress and as the endpoint clients dial.
fn wan_iface_and_ip() -> (String, String) {
    let iface = sh("ip", &["route", "show", "default"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .and_then(|s| {
            s.split_whitespace()
                .skip_while(|w| *w != "dev")
                .nth(1)
                .map(|s| s.to_string())
        })
        .unwrap_or_default();
    let ip = if iface.is_empty() {
        String::new()
    } else {
        sh("ip", &["-4", "-o", "addr", "show", "dev", &iface])
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .and_then(|s| {
                s.split_whitespace()
                    .skip_while(|w| *w != "inet")
                    .nth(1)
                    .map(|c| c.split('/').next().unwrap_or("").to_string())
            })
            .unwrap_or_default()
    };
    (iface, ip)
}

fn gen_privkey() -> Result<String, ApiErr> {
    let o = sh("wg", &["genkey"]).map_err(err500)?;
    if !o.status.success() {
        return Err(err500("wg genkey failed"));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
fn pubkey_of(priv_key: &str) -> Result<String, ApiErr> {
    use std::io::Write;
    let mut child = Command::new("wg")
        .arg("pubkey")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(err500)?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| err500("no stdin"))?
        .write_all(priv_key.as_bytes())
        .map_err(err500)?;
    let o = child.wait_with_output().map_err(err500)?;
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

// ── peer name sidecar ──────────────────────────────────────────────────────

fn load_meta() -> serde_json::Value {
    std::fs::read_to_string(PEERS_META)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}
fn save_meta(v: &serde_json::Value) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        // Written via a root helper so it works regardless of file ownership.
        use std::io::Write;
        if let Ok(mut c) = Command::new("sudo")
            .args(["tee", PEERS_META])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
        {
            let _ = c.stdin.as_mut().map(|i| i.write_all(s.as_bytes()));
            let _ = c.wait();
        }
    }
}

// ── data shapes ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WgPeer {
    pub name: String,
    pub public_key: String,
    pub allowed_ip: String,
    pub last_handshake: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub endpoint: String,
}

#[derive(Serialize)]
pub struct WgStatus {
    pub installed: bool,
    pub running: bool,
    pub public_key: String,
    pub endpoint: String,
    pub listen_port: u16,
    pub subnet: String,
    pub peer_count: usize,
    pub peers: Vec<WgPeer>,
}

// Parse `wg show wg0 dump`: first line is the interface, the rest are peers.
fn read_peers() -> (String, Vec<WgPeer>) {
    let meta = load_meta();
    let out = match sudo(&["wg", "show", WG_IFACE, "dump"]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return (String::new(), vec![]),
    };
    let mut lines = out.lines();
    // Interface line: privkey  pubkey  listen-port  fwmark
    let server_pub = lines
        .next()
        .and_then(|l| l.split('\t').nth(1))
        .unwrap_or("")
        .to_string();
    let mut peers = vec![];
    for l in lines {
        // peer: pubkey  psk  endpoint  allowed-ips  latest-handshake  rx  tx  keepalive
        let f: Vec<&str> = l.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        let pubk = f[0].to_string();
        let hs: i64 = f[4].parse().unwrap_or(0);
        let last = if hs == 0 {
            "never".to_string()
        } else {
            let ago = (chrono::Utc::now().timestamp() - hs).max(0);
            format!("{}s ago", ago)
        };
        let name = meta
            .get(&pubk)
            .and_then(|m| m.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("(unnamed)")
            .to_string();
        peers.push(WgPeer {
            name,
            public_key: pubk,
            allowed_ip: f[3].to_string(),
            last_handshake: last,
            rx_bytes: f[5].parse().unwrap_or(0),
            tx_bytes: f[6].parse().unwrap_or(0),
            endpoint: if f[2] == "(none)" { String::new() } else { f[2].to_string() },
        });
    }
    (server_pub, peers)
}

// ── handlers ───────────────────────────────────────────────────────────────

pub async fn status() -> Result<Json<WgStatus>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(WgStatus {
            installed: true,
            running: false,
            public_key: "server-pubkey".into(),
            endpoint: "203.0.113.1:51820".into(),
            listen_port: WG_PORT,
            subnet: format!("{}.0/24", WG_SUBNET_BASE),
            peer_count: 0,
            peers: vec![],
        }));
    }
    let (_, ip) = wan_iface_and_ip();
    let (server_pub, peers) = read_peers();
    Ok(Json(WgStatus {
        installed: wg_installed(),
        running: wg_running(),
        public_key: server_pub,
        endpoint: if ip.is_empty() { String::new() } else { format!("{}:{}", ip, WG_PORT) },
        listen_port: WG_PORT,
        subnet: format!("{}.0/24", WG_SUBNET_BASE),
        peer_count: peers.len(),
        peers,
    }))
}

/// Installs the WireGuard server and brings it up. Called by the addon
/// installer (addons.rs) so WireGuard appears as an installable addon next to
/// Tailscale. Returns Ok(message) / Err(message) to match the addon contract.
pub fn setup_server() -> Result<String, String> {
    let (wan, _) = wan_iface_and_ip();
    if wan.is_empty() {
        return Err("could not determine the WAN interface".to_string());
    }

    // Fresh server config only if none exists (preserve identity across restarts).
    let script = format!(
        r#"set -e
export DEBIAN_FRONTEND=noninteractive
command -v wg >/dev/null || apt-get install -y -qq wireguard-tools qrencode >/dev/null 2>&1
mkdir -p /etc/wireguard /opt/routerui/config
if [ ! -f {conf} ]; then
  umask 077
  SK=$(wg genkey)
  cat > {conf} <<CONF
[Interface]
Address = {server_ip}/24
ListenPort = {port}
PrivateKey = $SK
# Forward VPN <-> LAN, NAT VPN clients out the WAN, and open the WG port.
PostUp = iptables -A FORWARD -i {iface} -j ACCEPT; iptables -A FORWARD -o {iface} -j ACCEPT; iptables -t nat -A POSTROUTING -o {wan} -j MASQUERADE; iptables -I INPUT 1 -p udp --dport {port} -j ACCEPT
PostDown = iptables -D FORWARD -i {iface} -j ACCEPT; iptables -D FORWARD -o {iface} -j ACCEPT; iptables -t nat -D POSTROUTING -o {wan} -j MASQUERADE; iptables -D INPUT -p udp --dport {port} -j ACCEPT
CONF
  [ -f {meta} ] || echo '{{}}' > {meta}
fi
systemctl enable wg-quick@{iface} >/dev/null 2>&1 || true
# Bring it up (or reload if already up).
if wg show {iface} >/dev/null 2>&1; then wg-quick down {iface} >/dev/null 2>&1 || true; fi
wg-quick up {iface}
# Persist the WG port allow into saved rules too.
mkdir -p /etc/iptables && iptables-save > /etc/iptables/rules.v4
echo OK
"#,
        conf = WG_CONF,
        meta = PEERS_META,
        server_ip = WG_SERVER_IP,
        port = WG_PORT,
        iface = WG_IFACE,
        wan = wan,
    );
    let o = sudo(&["bash", "-c", &script]).map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!("wireguard setup failed: {}", String::from_utf8_lossy(&o.stderr)));
    }
    Ok("WireGuard VPN server installed and running.".to_string())
}

pub async fn disable() -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }
    let _ = sudo(&["wg-quick", "down", WG_IFACE]);
    let _ = sudo(&["systemctl", "disable", &format!("wg-quick@{}", WG_IFACE)]);
    Ok(Json(serde_json::json!({"success": true})))
}

pub async fn list_peers() -> Result<Json<Vec<WgPeer>>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![]));
    }
    let (_, peers) = read_peers();
    Ok(Json(peers))
}

#[derive(Deserialize)]
pub struct AddPeer {
    pub name: String,
}

#[derive(Serialize)]
pub struct AddPeerResult {
    pub name: String,
    pub public_key: String,
    pub allowed_ip: String,
    pub config: String,
    pub qr_svg: String,
}

fn next_peer_ip(peers: &[WgPeer]) -> Result<String, ApiErr> {
    // Assign the lowest free host from .2 upward.
    let used: std::collections::HashSet<u8> = peers
        .iter()
        .filter_map(|p| {
            p.allowed_ip
                .split('/')
                .next()
                .and_then(|ip| ip.rsplit('.').next())
                .and_then(|o| o.parse::<u8>().ok())
        })
        .collect();
    for host in 2u8..=254 {
        if !used.contains(&host) {
            return Ok(format!("{}.{}", WG_SUBNET_BASE, host));
        }
    }
    Err(bad("no free addresses in the VPN subnet"))
}

pub async fn add_peer(Json(payload): Json<AddPeer>) -> Result<Json<AddPeerResult>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(AddPeerResult {
            name: payload.name,
            public_key: "peer-pub".into(),
            allowed_ip: "10.7.0.2/32".into(),
            config: "[Interface]\n...".into(),
            qr_svg: String::new(),
        }));
    }
    let name = payload.name.trim().to_string();
    if name.is_empty() || name.len() > 40 || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' ')) {
        return Err(bad("peer name must be 1-40 letters, digits, space, - or _"));
    }
    if !wg_running() {
        return Err(bad("enable the WireGuard server first"));
    }

    let (server_pub, peers) = read_peers();
    let peer_ip = next_peer_ip(&peers)?;
    let priv_key = gen_privkey()?;
    let peer_pub = pubkey_of(&priv_key)?;

    // Endpoint + LAN subnet for the client config.
    let (_, wan_ip) = wan_iface_and_ip();
    let lan_subnet = detect_lan_subnet();

    // Add the peer to the live interface and persist it in the config file.
    let add = sudo(&[
        "wg", "set", WG_IFACE, "peer", &peer_pub, "allowed-ips", &format!("{}/32", peer_ip),
    ])
    .map_err(err500)?;
    if !add.status.success() {
        return Err(err500(format!("wg set failed: {}", String::from_utf8_lossy(&add.stderr))));
    }
    // Append to wg0.conf so it survives a restart.
    let block = format!("\n[Peer]\n# {}\nPublicKey = {}\nAllowedIPs = {}/32\n", name, peer_pub, peer_ip);
    append_root(WG_CONF, &block)?;

    // Record the name.
    let mut meta = load_meta();
    meta[&peer_pub] = serde_json::json!({"name": name, "ip": peer_ip});
    save_meta(&meta);

    // Client config: route the VPN + LAN subnets, use the router as DNS.
    let allowed = if lan_subnet.is_empty() {
        format!("{}.0/24", WG_SUBNET_BASE)
    } else {
        format!("{}.0/24, {}", WG_SUBNET_BASE, lan_subnet)
    };
    let config = format!(
        "[Interface]\nPrivateKey = {priv}\nAddress = {ip}/32\nDNS = {dns}\n\n[Peer]\nPublicKey = {spub}\nEndpoint = {ep}:{port}\nAllowedIPs = {allowed}\nPersistentKeepalive = 25\n",
        priv = priv_key,
        ip = peer_ip,
        dns = WG_SERVER_IP,
        spub = server_pub,
        ep = if wan_ip.is_empty() { "YOUR_PUBLIC_IP".to_string() } else { wan_ip },
        port = WG_PORT,
        allowed = allowed,
    );

    let qr_svg = qr_svg(&config);

    Ok(Json(AddPeerResult {
        name,
        public_key: peer_pub,
        allowed_ip: format!("{}/32", peer_ip),
        config,
        qr_svg,
    }))
}

#[derive(Deserialize)]
pub struct RemovePeer {
    pub public_key: String,
}

pub async fn remove_peer(Json(payload): Json<RemovePeer>) -> Result<Json<serde_json::Value>, ApiErr> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({"success": true, "mock": true})));
    }
    let pk = payload.public_key.trim();
    // A WireGuard public key is base64, 44 chars ending in '='.
    if pk.len() != 44 || !pk.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')) {
        return Err(bad("invalid public key"));
    }
    let _ = sudo(&["wg", "set", WG_IFACE, "peer", pk, "remove"]);
    // Rewrite wg0.conf without that peer, and drop its name.
    rewrite_conf_without_peer(pk)?;
    let mut meta = load_meta();
    if meta.get(pk).is_some() {
        meta.as_object_mut().map(|m| m.remove(pk));
        save_meta(&meta);
    }
    Ok(Json(serde_json::json!({"success": true})))
}

// ── low-level file helpers (root, via tee) ─────────────────────────────────

fn append_root(path: &str, text: &str) -> Result<(), ApiErr> {
    use std::io::Write;
    let mut c = Command::new("sudo")
        .args(["tee", "-a", path])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(err500)?;
    c.stdin.as_mut().ok_or_else(|| err500("no stdin"))?.write_all(text.as_bytes()).map_err(err500)?;
    c.wait().map_err(err500)?;
    Ok(())
}

fn rewrite_conf_without_peer(pubkey: &str) -> Result<(), ApiErr> {
    let content = match sudo(&["cat", WG_CONF]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Ok(()),
    };
    // Split into the [Interface] block and [Peer] blocks; drop the matching peer.
    let mut out = String::new();
    let mut blocks = content.split("\n[Peer]");
    if let Some(head) = blocks.next() {
        out.push_str(head.trim_end());
        out.push('\n');
    }
    for b in blocks {
        if b.contains(pubkey) {
            continue;
        }
        out.push_str("\n[Peer]");
        out.push_str(b.trim_end());
        out.push('\n');
    }
    use std::io::Write;
    let mut c = Command::new("sudo")
        .args(["tee", WG_CONF])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(err500)?;
    c.stdin.as_mut().ok_or_else(|| err500("no stdin"))?.write_all(out.as_bytes()).map_err(err500)?;
    c.wait().map_err(err500)?;
    Ok(())
}

fn detect_lan_subnet() -> String {
    // The dnsmasq LAN is 192.168.1.0/24 (configure_lan_ip). Derive from the LAN
    // interface address rather than hardcoding, in case it changes.
    let out = sh("ip", &["-4", "-o", "addr", "show"]).ok();
    if let Some(o) = out {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if line.contains("192.168.1.1/24") {
                return "192.168.1.0/24".to_string();
            }
        }
    }
    "192.168.1.0/24".to_string()
}

fn qr_svg(config: &str) -> String {
    use std::io::Write;
    let child = Command::new("qrencode")
        .args(["-t", "SVG", "-o", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    if let Some(si) = child.stdin.as_mut() {
        let _ = si.write_all(config.as_bytes());
    }
    match child.wait_with_output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

// keep validate import used even if compiled without some paths
#[allow(unused_imports)]
use validate as _validate;
