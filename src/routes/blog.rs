use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::AppError;

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

pub async fn list(State(pool): State<PgPool>) -> Result<Json<Vec<BlogPostSummary>>, AppError> {
    let posts = sqlx::query_as::<_, BlogPostSummary>(
        "SELECT id, slug, title_fr, title_en, title_km,
                excerpt_fr, excerpt_en, excerpt_km, tags, created_at, updated_at
         FROM blog_posts WHERE published = true ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(posts))
}

pub async fn get_one(
    State(pool): State<PgPool>,
    Path(slug): Path<String>,
) -> Result<Json<BlogPostFull>, AppError> {
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
    Ok(Json(post))
}
