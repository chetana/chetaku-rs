use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::AppError;
use crate::s3cache::{self, CACHE_TTL};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BlogPostSummary {
    pub id: i32,
    pub slug: String,
    pub title_fr: String,
    pub title_en: String,
    pub title_km: Option<String>,
    pub excerpt_fr: String,
    pub excerpt_en: String,
    pub excerpt_km: Option<String>,
    pub tags: serde_json::Value,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct BlogPostFull {
    pub id: i32,
    pub slug: String,
    pub title_fr: String,
    pub title_en: String,
    pub title_km: Option<String>,
    pub content_fr: String,
    pub content_en: String,
    pub content_km: Option<String>,
    pub excerpt_fr: String,
    pub excerpt_en: String,
    pub excerpt_km: Option<String>,
    pub tags: serde_json::Value,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub async fn list(State(pool): State<PgPool>) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(v) = s3cache::get_fresh("blog", CACHE_TTL).await {
        return Ok(Json(v));
    }
    let posts = sqlx::query_as::<_, BlogPostSummary>(
        "SELECT id, slug, title_fr, title_en, title_km,
                excerpt_fr, excerpt_en, excerpt_km, tags, created_at, updated_at
         FROM blog_posts WHERE published = true ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    let v = serde_json::to_value(&posts).unwrap_or_else(|_| serde_json::json!([]));
    s3cache::put("blog", &v).await;
    Ok(Json(v))
}

pub async fn get_one(
    State(pool): State<PgPool>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ckey = format!("blogpost_{slug}");
    if let Some(v) = s3cache::get_fresh(&ckey, CACHE_TTL).await {
        return Ok(Json(v));
    }
    let post = sqlx::query_as::<_, BlogPostFull>(
        "SELECT id, slug, title_fr, title_en, title_km,
                content_fr, content_en, content_km,
                excerpt_fr, excerpt_en, excerpt_km, tags, created_at, updated_at
         FROM blog_posts WHERE slug = $1 AND published = true",
    )
    .bind(&slug)
    .fetch_optional(&pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let v = serde_json::to_value(&post).unwrap_or(serde_json::Value::Null);
    s3cache::put(&ckey, &v).await;
    Ok(Json(v))
}
