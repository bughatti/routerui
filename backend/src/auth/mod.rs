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
    let count = crate::db::count_users(pool).await?;
    
    if count == 0 {
        let password_hash = hash_password("myV!ct0r@2014!!")
            .map_err(|e| Box::<dyn std::error::Error>::from(e))?;
        sqlx::query("INSERT INTO users (username, password_hash, role, enabled) VALUES (?, ?, 'admin', 1)")
            .bind("claudeadmin")
            .bind(&password_hash)
            .execute(pool)
            .await?;
        
        tracing::info!("Created default admin user: claudeadmin");
    }
    
    Ok(())
}
