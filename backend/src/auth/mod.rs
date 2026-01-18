use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{Duration, Utc};
use rand::Rng;
use sqlx::SqlitePool;

use crate::models::{PasswordStrength, Session, User};

const SESSION_DURATION_HOURS: i64 = 4;

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)
        .map_err(|e| e.to_string())?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

pub fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

pub fn hash_token(token: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    token.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

pub fn check_password_strength(password: &str) -> PasswordStrength {
    let mut score = 0u8;
    let mut suggestions = Vec::new();

    if password.len() >= 8 { score += 1; } 
    else { suggestions.push("Use at least 8 characters".to_string()); }

    if password.len() >= 12 { score += 1; }

    if password.chars().any(|c| c.is_uppercase()) { score += 1; } 
    else { suggestions.push("Add uppercase letters".to_string()); }

    if password.chars().any(|c| c.is_numeric()) { score += 1; } 
    else { suggestions.push("Add numbers".to_string()); }

    if password.chars().any(|c| !c.is_alphanumeric()) { score += 1; } 
    else { suggestions.push("Add special characters".to_string()); }

    let label = match score {
        0..=1 => "Weak",
        2 => "Fair",
        3 => "Medium",
        _ => "Strong",
    }.to_string();

    PasswordStrength { score, label, suggestions }
}

pub async fn create_session(
    pool: &SqlitePool,
    user_id: i64,
    ip_address: Option<&str>,
) -> Result<String, sqlx::Error> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let expires_at = (Utc::now() + Duration::hours(SESSION_DURATION_HOURS)).to_rfc3339();

    sqlx::query("INSERT INTO sessions (user_id, token_hash, expires_at, ip_address) VALUES (?, ?, ?, ?)")
        .bind(user_id)
        .bind(&token_hash)
        .bind(&expires_at)
        .bind(ip_address)
        .execute(pool)
        .await?;

    Ok(token)
}

pub async fn validate_session(pool: &SqlitePool, token: &str) -> Result<Option<User>, sqlx::Error> {
    let token_hash = hash_token(token);
    let now = Utc::now().to_rfc3339();

    let session: Option<Session> = sqlx::query_as(
        "SELECT id, user_id, token_hash, created_at, expires_at, ip_address FROM sessions WHERE token_hash = ? AND expires_at > ?"
    )
    .bind(&token_hash)
    .bind(&now)
    .fetch_optional(pool)
    .await?;

    match session {
        Some(s) => crate::db::get_user_by_id(pool, s.user_id).await,
        None => Ok(None),
    }
}

pub async fn create_default_admin(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // First check if setup wizard has completed
    // If setup_config table exists and has setup_complete = true, we can create fallback admin
    // Otherwise, let the setup wizard create the admin account

    let setup_complete = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='setup_config'"
    )
        .fetch_one(pool)
        .await
        .map(|c| c > 0)
        .unwrap_or(false);

    if !setup_complete {
        // Setup wizard hasn't run yet, don't create default admin
        tracing::info!("Setup not complete - skipping default admin creation");
        return Ok(());
    }

    // Check if setup was marked complete
    let is_setup_done = sqlx::query_scalar::<_, String>(
        "SELECT value FROM setup_config WHERE key = 'setup_complete'"
    )
        .fetch_optional(pool)
        .await?
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_setup_done {
        tracing::info!("Setup wizard in progress - skipping default admin creation");
        return Ok(());
    }

    // Setup is complete, check if we need a fallback admin
    let count = crate::db::count_users(pool).await?;

    if count == 0 {
        let password_hash = hash_password("admin")
            .map_err(|e| Box::<dyn std::error::Error>::from(e))?;
        sqlx::query("INSERT INTO users (username, password_hash, role, enabled) VALUES (?, ?, 'admin', 1)")
            .bind("admin")
            .bind(&password_hash)
            .execute(pool)
            .await?;

        tracing::info!("Created fallback admin user: admin/admin");
    }

    Ok(())
}
