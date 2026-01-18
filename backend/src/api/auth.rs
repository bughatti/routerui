use axum::{
    extract::State,
    http::{header::SET_COOKIE, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

use crate::{
    auth,
    db,
    models::{LoginRequest, LoginResponse, UserPublic},
    AppState,
};

use super::AuthUser;

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Response, (StatusCode, String)> {
    // Find user
    let user = db::get_user_by_username(&state.db, &payload.username)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    // Check if enabled
    if !user.enabled {
        return Err((StatusCode::FORBIDDEN, "Account disabled".to_string()));
    }

    // Verify password
    if !auth::verify_password(&payload.password, &user.password_hash) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    // Create session
    let token = auth::create_session(&state.db, user.id, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update last login
    sqlx::query("UPDATE users SET last_login = datetime('now') WHERE id = ?")
        .bind(user.id)
        .execute(&state.db)
        .await
        .ok();

    let response = LoginResponse {
        token: token.clone(),
        user: UserPublic {
            id: user.id,
            username: user.username,
            role: user.role,
        },
    };

    // Set cookie (4 hour expiry)
    let cookie = format!(
        "session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        token,
        4 * 60 * 60
    );

    Ok((
        [(SET_COOKIE, cookie)],
        Json(response),
    ).into_response())
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // In a real implementation, we'd get the token from the request
    // For now, just return success
    tracing::info!("User {} logged out", user.username);
    
    // Clear cookie
    let cookie = "session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0";
    
    Ok((
        [(SET_COOKIE, cookie)],
        Json(serde_json::json!({ "success": true })),
    ))
}

pub async fn me(
    AuthUser(user): AuthUser,
) -> Json<UserPublic> {
    Json(UserPublic {
        id: user.id,
        username: user.username,
        role: user.role,
    })
}
