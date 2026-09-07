use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::AppError;
use crate::s3cache::{self, CACHE_TTL};

// ── Comments ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Comment {
    pub id: i32,
    pub post_id: i32,
    pub author_name: String,
    pub content: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreateComment {
    pub post_id: i32,
    pub author_name: String,
    pub content: String,
    pub honeypot: Option<String>,
}

pub async fn list_comments(
    State(pool): State<PgPool>,
    Path(post_id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ckey = format!("comments_{post_id}");
    if let Some(v) = s3cache::get_fresh(&ckey, CACHE_TTL).await {
        return Ok(Json(v));
    }
    let comments = sqlx::query_as::<_, Comment>(
        "SELECT id, post_id, author_name, content, created_at
         FROM comments WHERE post_id = $1 AND approved = true ORDER BY created_at DESC",
    )
    .bind(post_id)
    .fetch_all(&pool)
    .await?;
    let v = serde_json::to_value(&comments).unwrap_or_else(|_| serde_json::json!([]));
    s3cache::put(&ckey, &v).await;
    Ok(Json(v))
}

pub async fn create_comment(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateComment>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Honeypot bot trap
    if !payload.honeypot.as_deref().unwrap_or("").is_empty() {
        return Ok(Json(serde_json::json!({ "created": true })));
    }
    if payload.content.len() > 1000 {
        return Err(AppError::Validation("Content too long".into()));
    }
    if payload.content.matches("http").count() > 2 {
        return Err(AppError::Validation("Too many links".into()));
    }
    if payload.author_name.trim().is_empty() || payload.content.trim().is_empty() {
        return Err(AppError::Validation("Missing fields".into()));
    }

    sqlx::query(
        "INSERT INTO comments (post_id, author_name, content) VALUES ($1, $2, $3)",
    )
    .bind(payload.post_id)
    .bind(payload.author_name.trim())
    .bind(payload.content.trim())
    .execute(&pool)
    .await?;

    s3cache::invalidate(&[&format!("comments_{}", payload.post_id)]).await;
    Ok(Json(serde_json::json!({ "created": true })))
}

// ── Messages (contact form) ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateMessage {
    pub name: String,
    pub email: String,
    pub content: String,
    pub honeypot: Option<String>,
}

pub async fn create_message(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateMessage>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Honeypot bot trap
    if !payload.honeypot.as_deref().unwrap_or("").is_empty() {
        return Ok(Json(serde_json::json!({ "created": true })));
    }
    if payload.name.trim().is_empty()
        || payload.email.trim().is_empty()
        || payload.content.trim().is_empty()
    {
        return Err(AppError::Validation("Missing fields".into()));
    }

    sqlx::query("INSERT INTO messages (name, email, content) VALUES ($1, $2, $3)")
        .bind(payload.name.trim())
        .bind(payload.email.trim())
        .bind(payload.content.trim())
        .execute(&pool)
        .await?;

    Ok(Json(serde_json::json!({ "created": true })))
}
