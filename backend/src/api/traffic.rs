//! Per-client traffic insight (UniFi-style "client" view).
//!
//! Four layers, all built on mechanisms already on the box:
//!   L1  per-client cumulative usage  — an iptables accounting chain that counts
//!       forwarded bytes per LAN client (survives connection close).
//!   L2  live connections per client  — `conntrack -L` grouped by source IP.
//!   L3  real-time bandwidth per client — a 2s sampler diffs the L1 counters.
//!   L4  top domains per client        — AdGuard's query log filtered by client.
//!
//! Privacy: an admin `insights_enabled` toggle gates the whole feature, and a
//! `retention_hours` cap bounds how long domain history is kept (applied to
//! AdGuard's query-log retention when AdGuard is installed). Cumulative counters
//! and live flows are not written to disk; only the in-memory rate ring and
//! AdGuard's own log persist anything.

use crate::api::netutil;
use crate::mock;
use crate::validate;
use axum::{
    extract::{Json, Query},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const LEASES: &str = "/var/lib/misc/dnsmasq.leases";
const ACCT_CHAIN: &str = "ROUTERUI_ACCT";
const SETTINGS_FILE: &str = "/opt/routerui/config/traffic.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSettings {
    pub insights_enabled: bool,
    pub retention_hours: u32,
}

impl Default for TrafficSettings {
    fn default() -> Self {
        // On by default (this is a core router view), with a modest retention cap.
        TrafficSettings { insights_enabled: true, retention_hours: 48 }
    }
}

fn load_settings() -> TrafficSettings {
    fs::read_to_string(SETTINGS_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(s: &TrafficSettings) -> Result<(), String> {
    fs::create_dir_all("/opt/routerui/config").map_err(|e| e.to_string())?;
    fs::write(SETTINGS_FILE, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn sudo(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("sudo").args(args).output()
}

// -------- DHCP leases (client identity) --------

#[derive(Debug, Clone, Serialize)]
pub struct Client {
    pub ip: String,
    pub mac: String,
    pub hostname: String,
    pub rx_bytes: u64, // downloaded (toward the client)
    pub tx_bytes: u64, // uploaded (from the client)
    pub rx_rate_bps: u64,
    pub tx_rate_bps: u64,
}

/// (ip, mac, hostname) for every current DHCP lease.
fn leases() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    if let Ok(txt) = fs::read_to_string(LEASES) {
        for line in txt.lines() {
            // "<expiry> <mac> <ip> <hostname> <clientid>"
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() >= 4 && validate::is_ipv4(f[2]) {
                let host = if f[3] == "*" { String::new() } else { f[3].to_string() };
                out.push((f[2].to_string(), f[1].to_string(), host));
            }
        }
    }
    out
}

// -------- L1: iptables accounting chain --------

/// Ensure the accounting chain exists, is hooked into FORWARD once, and has an
/// upload (-s) and download (-d) counting rule for every given client IP.
/// Counting rules have no jump target, so they tally bytes and fall through.
fn ensure_accounting(client_ips: &[String]) {
    // Create chain if missing.
    let have_chain = sudo(&["iptables", "-n", "-L", ACCT_CHAIN])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !have_chain {
        let _ = sudo(&["iptables", "-N", ACCT_CHAIN]);
    }
    // Hook it into FORWARD once (jump at the top so every forwarded packet is
    // seen; the chain only counts and returns).
    let hooked = sudo(&["iptables", "-C", "FORWARD", "-j", ACCT_CHAIN])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !hooked {
        let _ = sudo(&["iptables", "-I", "FORWARD", "1", "-j", ACCT_CHAIN]);
    }
    // Per-client counting rules (idempotent via -C check).
    for ip in client_ips {
        for dir in ["-s", "-d"] {
            let exists = sudo(&["iptables", "-C", ACCT_CHAIN, dir, ip])
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !exists {
                let _ = sudo(&["iptables", "-A", ACCT_CHAIN, dir, ip]);
            }
        }
    }
}

/// Read (tx_bytes, rx_bytes) per client IP from the accounting chain.
/// `-s <ip>` = traffic sourced by the client = upload (tx).
/// `-d <ip>` = traffic destined to the client = download (rx).
fn read_counters() -> HashMap<String, (u64, u64)> {
    let mut map: HashMap<String, (u64, u64)> = HashMap::new();
    if let Ok(o) = sudo(&["iptables", "-L", ACCT_CHAIN, "-n", "-v", "-x"]) {
        let text = String::from_utf8_lossy(&o.stdout);
        for line in text.lines() {
            let f: Vec<&str> = line.split_whitespace().collect();
            // Columns: "<pkts> <bytes> <target> <prot> <opt> <in> <out> <source>
            // <destination>". Our counting rules have NO target, so that column
            // is blank and split_whitespace collapses it — index positions
            // shift. Parse from the ends instead: bytes is always the 2nd field,
            // and source/destination are always the last two (no trailing match
            // extensions on these plain -s/-d rules). Header and chain lines fail
            // the bytes parse and are skipped.
            if f.len() < 8 {
                continue;
            }
            let Ok(bytes) = f[1].parse::<u64>() else { continue };
            let source = f[f.len() - 2].split('/').next().unwrap_or("");
            let dest = f[f.len() - 1].split('/').next().unwrap_or("");
            if source != "0.0.0.0" && validate::is_ipv4(source) {
                map.entry(source.to_string()).or_default().0 += bytes; // upload (client is src)
            } else if dest != "0.0.0.0" && validate::is_ipv4(dest) {
                map.entry(dest.to_string()).or_default().1 += bytes; // download (client is dst)
            }
        }
    }
    map
}

// -------- L3: rate sampler --------

struct Sample {
    at: Instant,
    tx: u64,
    rx: u64,
}
struct SampleState {
    // ip -> (previous sample, current rate bps)
    prev: HashMap<String, Sample>,
    rate: HashMap<String, (u64, u64)>, // (tx_bps, rx_bps)
}

fn sampler() -> &'static Mutex<SampleState> {
    static S: OnceLock<Mutex<SampleState>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(SampleState { prev: HashMap::new(), rate: HashMap::new() })
    })
}

/// One sampler pass: refresh accounting rules for current leases, read counters,
/// and compute per-client bit rates from the delta since the last pass.
pub fn sample_once() {
    if !load_settings().insights_enabled {
        return;
    }
    let ips: Vec<String> = leases().into_iter().map(|(ip, _, _)| ip).collect();
    ensure_accounting(&ips);
    let counters = read_counters();
    let now = Instant::now();
    let Ok(mut st) = sampler().lock() else { return };
    let mut new_rate = HashMap::new();
    for (ip, (tx, rx)) in &counters {
        if let Some(prev) = st.prev.get(ip) {
            let dt = now.duration_since(prev.at).as_secs_f64().max(0.001);
            let tx_bps = (((tx.saturating_sub(prev.tx)) as f64) * 8.0 / dt) as u64;
            let rx_bps = (((rx.saturating_sub(prev.rx)) as f64) * 8.0 / dt) as u64;
            new_rate.insert(ip.clone(), (tx_bps, rx_bps));
        }
        st.prev.insert(ip.clone(), Sample { at: now, tx: *tx, rx: *rx });
    }
    st.rate = new_rate;
}

fn current_rates() -> HashMap<String, (u64, u64)> {
    sampler().lock().map(|s| s.rate.clone()).unwrap_or_default()
}

// -------- Handlers --------

pub async fn settings() -> Result<Json<TrafficSettings>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(TrafficSettings::default()));
    }
    Ok(Json(load_settings()))
}

pub async fn set_settings(
    _user: crate::api::AuthUser,
    Json(mut s): Json<TrafficSettings>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if s.retention_hours > 24 * 30 {
        s.retention_hours = 24 * 30; // cap at 30 days
    }
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }
    save_settings(&s).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    // Per-client domain visibility needs dnsmasq query logging (it's the only
    // resolver that sees the real client IP); turn it on with insights, off when
    // insights are disabled so we're not logging every query for nothing.
    set_dnsmasq_logging(s.insights_enabled);
    // Best-effort: align AdGuard's query-log retention with the cap when present.
    let _ = crate::api::adguard::set_querylog_retention(s.retention_hours).await;
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn clients() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "enabled": true, "clients": [] })));
    }
    let settings = load_settings();
    if !settings.insights_enabled {
        return Ok(Json(serde_json::json!({ "enabled": false, "clients": [] })));
    }
    let ls = leases();
    let ips: Vec<String> = ls.iter().map(|(ip, _, _)| ip.clone()).collect();
    ensure_accounting(&ips);
    let counters = read_counters();
    let rates = current_rates();
    let mut clients: Vec<Client> = ls
        .into_iter()
        .map(|(ip, mac, hostname)| {
            let (tx, rx) = counters.get(&ip).copied().unwrap_or((0, 0));
            let (tx_bps, rx_bps) = rates.get(&ip).copied().unwrap_or((0, 0));
            Client { ip, mac, hostname, rx_bytes: rx, tx_bytes: tx, rx_rate_bps: rx_bps, tx_rate_bps: tx_bps }
        })
        .collect();
    // Busiest first.
    clients.sort_by(|a, b| (b.rx_bytes + b.tx_bytes).cmp(&(a.rx_bytes + a.tx_bytes)));
    Ok(Json(serde_json::json!({ "enabled": true, "clients": clients })))
}

#[derive(Deserialize)]
pub struct FlowQuery {
    pub ip: Option<String>,
}

#[derive(Serialize)]
pub struct Flow {
    pub proto: String,
    pub src: String,
    pub dst: String,
    pub dst_port: String,
    pub dst_host: String,
    pub state: String,
    pub bytes: u64,
}

pub async fn connections(
    Query(q): Query<FlowQuery>,
) -> Result<Json<Vec<Flow>>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(vec![]));
    }
    if !load_settings().insights_enabled {
        return Ok(Json(vec![]));
    }
    if let Some(ip) = &q.ip {
        if !validate::is_ipv4(ip) {
            return Err((StatusCode::BAD_REQUEST, "invalid ip".into()));
        }
    }
    let lan_ips: std::collections::HashSet<String> = leases().into_iter().map(|(ip, _, _)| ip).collect();
    let out = sudo(&["conntrack", "-L", "-o", "extended"])
        .or_else(|_| sudo(&["conntrack", "-L"]))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("conntrack: {e} (install conntrack)")))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut flows = Vec::new();
    for line in text.lines() {
        // e.g. "tcp 6 431999 ESTABLISHED src=192.168.1.5 dst=1.2.3.4 sport=52344 dport=443 ... bytes=..."
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 4 {
            continue;
        }
        let proto = f[0].to_string();
        let mut src = String::new();
        let mut dst = String::new();
        let mut dport = String::new();
        let mut state = String::new();
        let mut bytes: u64 = 0;
        for tok in &f {
            if let Some(v) = tok.strip_prefix("src=") {
                if src.is_empty() { src = v.to_string(); }
            } else if let Some(v) = tok.strip_prefix("dst=") {
                if dst.is_empty() { dst = v.to_string(); }
            } else if let Some(v) = tok.strip_prefix("dport=") {
                if dport.is_empty() { dport = v.to_string(); }
            } else if let Some(v) = tok.strip_prefix("bytes=") {
                bytes += v.parse::<u64>().unwrap_or(0);
            } else if matches!(*tok, "ESTABLISHED" | "TIME_WAIT" | "CLOSE_WAIT" | "SYN_SENT" | "FIN_WAIT") {
                state = tok.to_string();
            }
        }
        // Only flows originated by a LAN client (src is a known lease).
        if !lan_ips.contains(&src) {
            continue;
        }
        if let Some(filter) = &q.ip {
            if &src != filter {
                continue;
            }
        }
        let dst_host = reverse_dns(&dst);
        flows.push(Flow {
            proto,
            src,
            dst: dst.clone(),
            dst_port: dport,
            dst_host,
            state: if state.is_empty() { "-".into() } else { state },
            bytes,
        });
    }
    flows.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    flows.truncate(500);
    Ok(Json(flows))
}

/// Best-effort cached reverse DNS. Returns "" if unknown; never blocks long.
fn reverse_dns(ip: &str) -> String {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(c) = cache.lock() {
        if let Some(v) = c.get(ip) {
            return v.clone();
        }
    }
    // 1s ceiling so a slow PTR never stalls the response.
    let name = Command::new("timeout")
        .args(["1", "getent", "hosts", ip])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .and_then(|s| s.split_whitespace().nth(1).map(|h| h.to_string()))
        .unwrap_or_default();
    if let Ok(mut c) = cache.lock() {
        c.insert(ip.to_string(), name.clone());
    }
    name
}

#[derive(Deserialize)]
pub struct DomainQuery {
    pub ip: String,
}

pub async fn domains(
    Query(q): Query<DomainQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "available": false, "domains": [] })));
    }
    let settings = load_settings();
    if !settings.insights_enabled {
        return Ok(Json(serde_json::json!({ "available": false, "domains": [] })));
    }
    if !validate::is_ipv4(&q.ip) {
        return Err((StatusCode::BAD_REQUEST, "invalid ip".into()));
    }
    // Primary source: dnsmasq's own query log. dnsmasq is the client-facing
    // resolver, so it sees the REAL client IP (unlike AdGuard, which sits behind
    // dnsmasq and attributes every forwarded query to the router). This is the
    // only source that attributes domains to the right device in this topology.
    let list = dnsmasq_domains_for(&q.ip, settings.retention_hours);
    if !list.is_empty() {
        return Ok(Json(serde_json::json!({ "available": true, "domains": list })));
    }
    // Fallback: AdGuard's per-client log (accurate only if AdGuard is the
    // client-facing resolver). Returns empty gracefully when unavailable.
    match crate::api::adguard::top_domains_for_client(&q.ip).await {
        Ok(l) if !l.is_empty() => Ok(Json(serde_json::json!({ "available": true, "domains": l }))),
        _ => {
            // Available=true (we can attribute) but nothing logged yet, vs. the
            // query log being off. Report availability from whether logging is on.
            let logging_on = std::path::Path::new(DNSMASQ_LOG_CONF).exists();
            Ok(Json(serde_json::json!({ "available": logging_on, "domains": [] })))
        }
    }
}

const DNSMASQ_LOG_CONF: &str = "/etc/dnsmasq.d/routerui-logging.conf";

/// Top domains a specific client resolved, from dnsmasq's journal. dnsmasq logs
/// lines like `query[A] github.com from 192.168.1.50`; we count the domains
/// asked for by `ip` over the retention window.
fn dnsmasq_domains_for(ip: &str, retention_hours: u32) -> Vec<serde_json::Value> {
    let since = format!("-{}h", retention_hours.max(1));
    let out = Command::new("sudo")
        .args(["journalctl", "-u", "dnsmasq", "--no-pager", "-o", "cat", "--since", &since])
        .output();
    let mut counts: HashMap<String, u64> = HashMap::new();
    if let Ok(o) = out {
        let text = String::from_utf8_lossy(&o.stdout);
        let from_ip = format!("from {}", ip);
        for line in text.lines() {
            // Only this client's forward queries (skip cached/reply lines).
            if !line.contains(&from_ip) {
                continue;
            }
            if let Some(qpos) = line.find("query[") {
                // after "query[TYPE] " comes the domain, then " from <ip>"
                let rest = &line[qpos..];
                if let Some(sp) = rest.find("] ") {
                    let after = &rest[sp + 2..];
                    let domain = after.split_whitespace().next().unwrap_or("");
                    if !domain.is_empty() {
                        *counts.entry(domain.to_ascii_lowercase()).or_default() += 1;
                    }
                }
            }
        }
    }
    let mut ranked: Vec<(String, u64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.truncate(20);
    ranked
        .into_iter()
        .map(|(domain, count)| serde_json::json!({ "domain": domain, "count": count }))
        .collect()
}

/// Turn dnsmasq per-query logging on or off (needed for accurate per-client L4).
/// Writing a tiny drop-in + reload is enough; kept separate so it's obvious in
/// the config what RouterUI added.
fn set_dnsmasq_logging(on: bool) {
    let already_on = std::path::Path::new(DNSMASQ_LOG_CONF).exists();
    if on == already_on {
        return; // no change; don't bounce dnsmasq needlessly
    }
    if on {
        let _ = fs::write(
            DNSMASQ_LOG_CONF,
            "# RouterUI: per-client domain visibility (Traffic Insight)\nlog-queries\n",
        );
    } else {
        let _ = fs::remove_file(DNSMASQ_LOG_CONF);
    }
    // `log-queries` is a startup option: a reload (SIGHUP) does NOT apply it, so
    // a restart is required. Brief (~1s) DNS blip, only when the toggle changes.
    let _ = Command::new("sudo").args(["systemctl", "restart", "dnsmasq"]).output();
}

pub async fn reset(
    _user: crate::api::AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }
    let _ = sudo(&["iptables", "-Z", ACCT_CHAIN]);
    if let Ok(mut st) = sampler().lock() {
        st.prev.clear();
        st.rate.clear();
    }
    Ok(Json(serde_json::json!({ "success": true })))
}

/// Rebuild accounting on boot so counters start fresh but the chain is present,
/// and make dnsmasq logging match the saved setting.
pub fn reapply_on_boot() {
    let s = load_settings();
    set_dnsmasq_logging(s.insights_enabled);
    if !s.insights_enabled {
        return;
    }
    let ips: Vec<String> = leases().into_iter().map(|(ip, _, _)| ip).collect();
    ensure_accounting(&ips);
}
