use sqlx::SqlitePool;

pub async fn migrate(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT UNIQUE NOT NULL,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'viewer',
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_login TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            token_hash TEXT UNIQUE NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT NOT NULL,
            ip_address TEXT,
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Index for session lookups
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token_hash)
        "#,
    )
    .execute(pool)
    .await?;

    // Index for expired session cleanup
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at)
        "#,
    )
    .execute(pool)
    .await?;

    tracing::info!("Database migrations complete");
    Ok(())
}

pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<crate::models::User>, sqlx::Error> {
    sqlx::query_as::<_, crate::models::User>(
        "SELECT id, username, password_hash, role, enabled, created_at, last_login FROM users WHERE username = ?"
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn get_user_by_id(pool: &SqlitePool, id: i64) -> Result<Option<crate::models::User>, sqlx::Error> {
    sqlx::query_as::<_, crate::models::User>(
        "SELECT id, username, password_hash, role, enabled, created_at, last_login FROM users WHERE id = ?"
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn count_users(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(result.0)
}
