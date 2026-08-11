use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::AppError;
use crate::s3cache::{self, CACHE_TTL};

// ── Projects ────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Project {
    pub id: i32,
    pub slug: String,
    pub title_fr: String,
    pub title_en: String,
    pub title_km: Option<String>,
    pub description_fr: String,
    pub description_en: String,
    pub description_km: Option<String>,
    pub tags: serde_json::Value,
    pub github_url: Option<String>,
    pub demo_url: Option<String>,
    pub image_url: Option<String>,
    #[sqlx(rename = "type")]
    pub project_type: Option<String>,
    pub featured: Option<bool>,
    pub created_at: NaiveDateTime,
}

pub async fn list_projects(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(v) = s3cache::get_fresh("projects", CACHE_TTL).await {
        return Ok(Json(v));
    }
    let projects = sqlx::query_as::<_, Project>(
        "SELECT id, slug, title_fr, title_en, title_km,
                description_fr, description_en, description_km,
                tags, github_url, demo_url, image_url, type, featured, created_at
         FROM projects ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    let v = serde_json::to_value(&projects).unwrap_or_else(|_| serde_json::json!([]));
    s3cache::put("projects", &v).await;
    Ok(Json(v))
}

pub async fn get_project(
    State(pool): State<PgPool>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ckey = format!("project_{slug}");
    if let Some(v) = s3cache::get_fresh(&ckey, CACHE_TTL).await {
        return Ok(Json(v));
    }
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, slug, title_fr, title_en, title_km,
                description_fr, description_en, description_km,
                tags, github_url, demo_url, image_url, type, featured, created_at
         FROM projects WHERE slug = $1",
    )
    .bind(&slug)
    .fetch_optional(&pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let v = serde_json::to_value(&project).unwrap_or(serde_json::Value::Null);
    s3cache::put(&ckey, &v).await;
    Ok(Json(v))
}

// ── Experiences ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Experience {
    pub id: i32,
    pub company: String,
    pub role_fr: String,
    pub role_en: String,
    pub role_km: Option<String>,
    pub date_start: String,
    pub date_end: Option<String>,
    pub location: Option<String>,
    pub bullets_fr: serde_json::Value,
    pub bullets_en: serde_json::Value,
    pub bullets_km: Option<serde_json::Value>,
    pub sort_order: Option<i32>,
}

pub async fn list_experiences(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(v) = s3cache::get_fresh("experiences", CACHE_TTL).await {
        return Ok(Json(v));
    }
    let experiences = sqlx::query_as::<_, Experience>(
        "SELECT id, company, role_fr, role_en, role_km,
                date_start, date_end, location,
                bullets_fr, bullets_en, bullets_km, sort_order
         FROM experiences ORDER BY sort_order ASC",
    )
    .fetch_all(&pool)
    .await?;
    let v = serde_json::to_value(&experiences).unwrap_or_else(|_| serde_json::json!([]));
    s3cache::put("experiences", &v).await;
    Ok(Json(v))
}

// ── Skills ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Skill {
    pub id: i32,
    pub category: String,
    pub name: String,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

pub async fn list_skills(
    State(pool): State<PgPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(v) = s3cache::get_fresh("skills", CACHE_TTL).await {
        return Ok(Json(v));
    }
    let skills = sqlx::query_as::<_, Skill>(
        "SELECT id, category, name, color, sort_order
         FROM skills ORDER BY category, sort_order ASC",
    )
    .fetch_all(&pool)
    .await?;
    let v = serde_json::to_value(&skills).unwrap_or_else(|_| serde_json::json!([]));
    s3cache::put("skills", &v).await;
    Ok(Json(v))
}
