use axum::{
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::mock;
use super::AuthUser;

const ADGUARD_URL: &str = "http://10.22.22.1:3000";
const ADGUARD_USER: &str = "admin";
const ADGUARD_PASS: &str = "routerui123";

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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("AdGuard connection failed: {}", e)))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let stats: serde_json::Value = c
        .get(format!("{}/control/stats", ADGUARD_URL))
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
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
    
    c.post(format!("{}/control/filtering/set_rules", ADGUARD_URL))
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
        .json(&serde_json::json!({ "rules": rules }))
        .send()
        .await
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
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?
        .json()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    let rules: Vec<String> = status.user_rules.into_iter().filter(|r| r != &payload.rule).collect();
    
    c.post(format!("{}/control/filtering/set_rules", ADGUARD_URL))
        .basic_auth(ADGUARD_USER, Some(ADGUARD_PASS))
        .json(&serde_json::json!({ "rules": rules }))
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
    
    Ok(Json(serde_json::json!({ "success": true })))
}
