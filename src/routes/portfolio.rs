use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::PgPool;

use crate::error::AppError;

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

pub async fn list_projects(State(pool): State<PgPool>) -> Result<Json<Vec<Project>>, AppError> {
    let projects = sqlx::query_as::<_, Project>(
        "SELECT id, slug, title_fr, title_en, title_km,
                description_fr, description_en, description_km,
                tags, github_url, demo_url, image_url, type, featured, created_at
         FROM projects ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(projects))
}

pub async fn get_project(
    State(pool): State<PgPool>,
    Path(slug): Path<String>,
) -> Result<Json<Project>, AppError> {
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
    Ok(Json(project))
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
) -> Result<Json<Vec<Experience>>, AppError> {
    let experiences = sqlx::query_as::<_, Experience>(
        "SELECT id, company, role_fr, role_en, role_km,
                date_start, date_end, location,
                bullets_fr, bullets_en, bullets_km, sort_order
         FROM experiences ORDER BY sort_order ASC",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(experiences))
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

pub async fn list_skills(State(pool): State<PgPool>) -> Result<Json<Vec<Skill>>, AppError> {
    let skills = sqlx::query_as::<_, Skill>(
        "SELECT id, category, name, color, sort_order
         FROM skills ORDER BY category, sort_order ASC",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(skills))
}
