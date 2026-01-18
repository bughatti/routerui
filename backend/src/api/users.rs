use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::{
    auth,
    models::{User, UserCreate, UserPublic, UserUpdate, PasswordStrength},
    AppState,
};

use super::{require_role, AuthUser};

pub async fn list(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<Json<Vec<UserPublic>>, (StatusCode, &'static str)> {
    require_role(&user, &["admin"])?;

    let users: Vec<User> = sqlx::query_as(
        "SELECT id, username, password_hash, role, enabled, created_at, last_login FROM users ORDER BY id"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(
        users
            .into_iter()
            .map(|u| UserPublic {
                id: u.id,
                username: u.username,
                role: u.role,
            })
            .collect(),
    ))
}

pub async fn get(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<UserPublic>, (StatusCode, &'static str)> {
    // Users can view themselves, admins can view anyone
    if user.id != id {
        require_role(&user, &["admin"])?;
    }

    let target: User = sqlx::query_as(
        "SELECT id, username, password_hash, role, enabled, created_at, last_login FROM users WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?
    .ok_or((StatusCode::NOT_FOUND, "User not found"))?;

    Ok(Json(UserPublic {
        id: target.id,
        username: target.username,
        role: target.role,
    }))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Json(payload): Json<UserCreate>,
) -> Result<Json<UserPublic>, (StatusCode, String)> {
    require_role(&user, &["admin"])
        .map_err(|(s, m)| (s, m.to_string()))?;

    // Validate role
    if !["admin", "operator", "viewer"].contains(&payload.role.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "Invalid role".to_string()));
    }

    // Hash password
    let password_hash = auth::hash_password(&payload.password)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let result = sqlx::query(
        "INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?)"
    )
    .bind(&payload.username)
    .bind(&password_hash)
    .bind(&payload.role)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            (StatusCode::CONFLICT, "Username already exists".to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    })?;

    Ok(Json(UserPublic {
        id: result.last_insert_rowid(),
        username: payload.username,
        role: payload.role,
    }))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    Json(payload): Json<UserUpdate>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Users can update themselves (limited), admins can update anyone
    let is_self = user.id == id;
    if !is_self {
        require_role(&user, &["admin"])
            .map_err(|(s, m)| (s, m.to_string()))?;
    }

    // Non-admins can only change their password
    if is_self && user.role != "admin" {
        if payload.role.is_some() || payload.enabled.is_some() || payload.username.is_some() {
            return Err((StatusCode::FORBIDDEN, "Can only change password".to_string()));
        }
    }

    // Build update query dynamically
    let mut updates = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(ref username) = payload.username {
        updates.push("username = ?");
        values.push(username.clone());
    }

    if let Some(ref password) = payload.password {
        let hash = auth::hash_password(password)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        updates.push("password_hash = ?");
        values.push(hash);
    }

    if let Some(ref role) = payload.role {
        if !["admin", "operator", "viewer"].contains(&role.as_str()) {
            return Err((StatusCode::BAD_REQUEST, "Invalid role".to_string()));
        }
        updates.push("role = ?");
        values.push(role.clone());
    }

    if let Some(enabled) = payload.enabled {
        updates.push("enabled = ?");
        values.push(if enabled { "1" } else { "0" }.to_string());
    }

    if updates.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    let query = format!("UPDATE users SET {} WHERE id = ?", updates.join(", "));
    
    let mut q = sqlx::query(&query);
    for v in &values {
        q = q.bind(v);
    }
    q = q.bind(id);

    q.execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn delete(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, &'static str)> {
    require_role(&user, &["admin"])?;

    // Can't delete yourself
    if user.id == id {
        return Err((StatusCode::BAD_REQUEST, "Cannot delete yourself"));
    }

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error"))?;

    Ok(Json(serde_json::json!({ "success": true })))
}

// Password strength check endpoint
pub async fn check_password_strength(
    Json(payload): Json<serde_json::Value>,
) -> Json<PasswordStrength> {
    let password = payload
        .get("password")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    
    Json(auth::check_password_strength(password))
}
