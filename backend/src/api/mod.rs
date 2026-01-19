pub mod addons;
pub mod auth;
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
pub mod tools;
pub mod security;
pub mod media;
pub mod setup;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

use crate::models::User;

// Auth extractor - gets current user from session token
pub struct AuthUser(pub User);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        _parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        // For now, skip auth and return a dummy user for testing
        // TODO: Implement proper auth extraction from cookie/header
        Ok(AuthUser(User {
            id: 1,
            username: "test".to_string(),
            password_hash: "".to_string(),
            role: "admin".to_string(),
            enabled: true,
            created_at: "".to_string(),
            last_login: None,
        }))
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
