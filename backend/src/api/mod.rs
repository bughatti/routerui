pub mod addons;
pub mod auth;
pub mod netutil;
pub mod firewall;
pub mod protection;
pub mod antivirus;
pub mod network;
pub mod adguard;
pub mod dashboard;
pub mod system;
pub mod users;
pub mod services;
pub mod docker;
pub mod vpn;
pub mod wireguard;
pub mod tools;
pub mod security;
pub mod media;
pub mod setup;
pub mod vlan;
pub mod guest;
pub mod qos;
pub mod ddns;
pub mod upnp;
pub mod failover;
pub mod traffic;

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap, StatusCode},
};

use crate::{models::User, AppState};

/// Pulls the session token from an `Authorization: Bearer <token>` header or
/// the `session` cookie, whichever is present.
pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(t) = v.strip_prefix("Bearer ") {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    let cookies = headers.get(header::COOKIE).and_then(|v| v.to_str().ok())?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(t) = part.strip_prefix("session=") {
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// True once the setup wizard has recorded completion. While false, the setup
/// routes are reachable without a session so the first admin can be created;
/// afterwards they require authentication like everything else.
pub async fn setup_complete(pool: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM setup_config WHERE key = 'setup_complete'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|v| v == "true")
    .unwrap_or(false)
}

// Auth extractor - resolves the current user from the session token, or 401.
pub struct AuthUser(pub User);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = token_from_headers(&parts.headers)
            .ok_or((StatusCode::UNAUTHORIZED, "Authentication required"))?;
        match crate::auth::validate_session(&state.db, &token).await {
            Ok(Some(user)) if user.enabled => Ok(AuthUser(user)),
            Ok(Some(_)) => Err((StatusCode::FORBIDDEN, "Account disabled")),
            Ok(None) => Err((StatusCode::UNAUTHORIZED, "Invalid or expired session")),
            Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "Auth check failed")),
        }
    }
}

// Role checker
pub fn require_role(user: &User, required: &[&str]) -> Result<(), (StatusCode, &'static str)> {
    if required.contains(&user.role.as_str()) {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "Insufficient permissions"))
    }
}
