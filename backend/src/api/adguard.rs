use axum::{
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::mock;
use super::AuthUser;

const ADGUARD_URL: &str = "http://127.0.0.1:3000";
const ADGUARD_CRED_FILE: &str = "/opt/routerui/config/adguard.cred";

// AdGuard admin credentials are generated when the addon is installed and
// stored in ADGUARD_CRED_FILE ("user:password"). No secret is baked into the
// binary. Falls back to the historical default only if the file is absent, so
// an already-configured older install keeps working.
fn adguard_creds() -> (String, String) {
    if let Ok(contents) = std::fs::read_to_string(ADGUARD_CRED_FILE) {
        if let Some((u, p)) = contents.trim().split_once(':') {
            if !u.is_empty() && !p.is_empty() {
                return (u.to_string(), p.to_string());
            }
        }
    }
    ("admin".to_string(), "routerui123".to_string())
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .connect_timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Serialize)]
pub struct AdGuardOverview {
    pub protection_enabled: bool,
    pub running: bool,
    pub dns_queries: u64,
    pub blocked_filtering: u64,
    pub blocked_percentage: f64,
    pub avg_processing_time: f64,
}

#[derive(Serialize, Deserialize)]
pub struct FilterStatus {
    pub enabled: bool,
    pub filters: Vec<Filter>,
    pub user_rules: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Filter {
    pub id: i64,
    pub url: String,
    pub name: String,
    pub enabled: bool,
    pub rules_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct QueryLogEntry {
    pub time: String,
    pub client: String,
    pub question: QueryQuestion,
    pub reason: String,
}

#[derive(Serialize, Deserialize)]
pub struct QueryQuestion {
    pub name: String,
    #[serde(rename = "type")]
    pub qtype: String,
}

pub async fn overview(
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::adguard::overview()));
    }

    let c = client();
    
    let status: serde_json::Value = c
        .get(format!("{}/control/status", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("AdGuard connection failed: {}", e)))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let stats: serde_json::Value = c
        .get(format!("{}/control/stats", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let dns_queries = stats["num_dns_queries"].as_u64().unwrap_or(0);
    let blocked = stats["num_blocked_filtering"].as_u64().unwrap_or(0);
    
    Ok(Json(serde_json::to_value(AdGuardOverview {
        protection_enabled: status["protection_enabled"].as_bool().unwrap_or(false),
        running: status["running"].as_bool().unwrap_or(false),
        dns_queries,
        blocked_filtering: blocked,
        blocked_percentage: if dns_queries > 0 { (blocked as f64 / dns_queries as f64) * 100.0 } else { 0.0 },
        avg_processing_time: stats["avg_processing_time"].as_f64().unwrap_or(0.0),
    }).unwrap()))
}

#[derive(Deserialize)]
pub struct ProtectionToggle {
    pub enabled: bool,
}

pub async fn toggle_protection(
    _user: AuthUser,
    Json(payload): Json<ProtectionToggle>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "protection_enabled": payload.enabled, "mock": true })));
    }

    let c = client();
    
    c.post(format!("{}/control/dns_config", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .json(&serde_json::json!({ "protection_enabled": payload.enabled }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    Ok(Json(serde_json::json!({ "success": true, "protection_enabled": payload.enabled })))
}

pub async fn query_log(
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::adguard::querylog()));
    }

    let c = client();
    
    let response: serde_json::Value = c
        .get(format!("{}/control/querylog?limit=100", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let entries: Vec<QueryLogEntry> = response["data"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default();

    Ok(Json(serde_json::to_value(entries).unwrap()))
}

pub async fn filters(
    _user: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(mock::adguard::filters()));
    }

    let c = client();

    let response: FilterStatus = c
        .get(format!("{}/control/filtering/status", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Json(serde_json::to_value(response).unwrap()))
}

#[derive(Deserialize)]
pub struct FilterToggle {
    pub url: String,
    pub enabled: bool,
}

pub async fn toggle_filter(
    _user: AuthUser,
    Json(payload): Json<FilterToggle>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let c = client();
    
    c.post(format!("{}/control/filtering/set_url", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .json(&serde_json::json!({ "url": payload.url, "data": { "enabled": payload.enabled } }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Deserialize)]
pub struct CustomRule {
    pub rule: String,
}

pub async fn add_rule(
    _user: AuthUser,
    Json(payload): Json<CustomRule>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "rule": payload.rule, "mock": true })));
    }

    let c = client();
    
    let status: FilterStatus = c
        .get(format!("{}/control/filtering/status", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let mut rules = status.user_rules;
    if !rules.contains(&payload.rule) {
        rules.push(payload.rule.clone());
    }
    
    // AdGuard's set_rules returns 200 with an empty body on success; do NOT
    // call .json() on it (that yields "error decoding response body"). Only
    // check the HTTP status via error_for_status(), which never reads the body.
    c.post(format!("{}/control/filtering/set_rules", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .json(&serde_json::json!({ "rules": rules }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .error_for_status()
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true, "rule": payload.rule })))
}

pub async fn remove_rule(
    _user: AuthUser,
    Json(payload): Json<CustomRule>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if mock::is_mock_mode() {
        return Ok(Json(serde_json::json!({ "success": true, "mock": true })));
    }

    let c = client();
    
    let status: FilterStatus = c
        .get(format!("{}/control/filtering/status", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let rules: Vec<String> = status.user_rules.into_iter().filter(|r| r != &payload.rule).collect();
    
    // set_rules returns an empty 200 body on success; check status only.
    c.post(format!("{}/control/filtering/set_rules", ADGUARD_URL))
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .json(&serde_json::json!({ "rules": rules }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .error_for_status()
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// ---- Per-client traffic insight helpers (used by api::traffic, L4) ----

/// Top resolved domains for one client IP, from AdGuard's query log. Returns
/// `[{ "domain": <name>, "count": <n> }]` busiest first. Errors if AdGuard is
/// not reachable (the traffic module treats that as "L4 unavailable").
pub async fn top_domains_for_client(ip: &str) -> Result<Vec<serde_json::Value>, String> {
    if mock::is_mock_mode() {
        return Ok(vec![]);
    }
    let c = client();
    // `search` narrows the log to this client; response_status=all keeps blocked
    // and allowed alike so the picture is complete.
    let url = format!(
        "{}/control/querylog?limit=1000&search={}&response_status=all",
        ADGUARD_URL, ip
    );
    let resp = c
        .get(url)
        .basic_auth(adguard_creds().0, Some(adguard_creds().1))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
        for entry in data {
            // Only count rows actually from this client (search is fuzzy).
            let client_ip = entry.get("client").and_then(|c| c.as_str()).unwrap_or("");
            if client_ip != ip {
                continue;
            }
            if let Some(name) = entry
                .get("question")
                .and_then(|q| q.get("name"))
                .and_then(|n| n.as_str())
            {
                if !name.is_empty() {
                    *counts.entry(name.to_ascii_lowercase()).or_default() += 1;
                }
            }
        }
    }
    let mut ranked: Vec<(String, u64)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.truncate(20);
    Ok(ranked
        .into_iter()
        .map(|(domain, count)| serde_json::json!({ "domain": domain, "count": count }))
        .collect())
}

/// Best-effort: align AdGuard's query-log retention with the traffic-insight
/// retention cap (in hours). Silently succeeds as a no-op if AdGuard is absent
/// or the endpoint shape differs by version — this is a privacy convenience,
/// not a hard dependency.
pub async fn set_querylog_retention(hours: u32) -> Result<(), String> {
    if mock::is_mock_mode() {
        return Ok(());
    }
    let c = client();
    // Newer AdGuard expects the interval in milliseconds.
    let interval_ms: u64 = (hours as u64).max(1) * 3600 * 1000;
    let body = serde_json::json!({
        "enabled": true,
        "interval": interval_ms,
        "anonymize_client_ip": false
    });
    // Try the current endpoint, then the legacy one; ignore a version mismatch.
    for ep in ["/control/querylog/config/update", "/control/querylog_config"] {
        let r = c
            .post(format!("{}{}", ADGUARD_URL, ep))
            .basic_auth(adguard_creds().0, Some(adguard_creds().1))
            .json(&body)
            .send()
            .await;
        if let Ok(resp) = r {
            if resp.status().is_success() {
                return Ok(());
            }
        }
    }
    Ok(())
}
