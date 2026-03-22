use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;

fn check_api_key(headers: &HeaderMap) -> Result<(), AppError> {
    let expected = std::env::var("API_KEY").unwrap_or_default();
    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

// ── Blog ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BlogPayload {
    pub slug: Option<String>,
    pub title_fr: Option<String>,
    pub title_en: Option<String>,
    pub title_km: Option<String>,
    pub content_fr: Option<String>,
    pub content_en: Option<String>,
    pub content_km: Option<String>,
    pub excerpt_fr: Option<String>,
    pub excerpt_en: Option<String>,
    pub excerpt_km: Option<String>,
    pub tags: Option<Value>,
    pub published: Option<bool>,
}

pub async fn create_blog(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<BlogPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let slug = p.slug.ok_or(AppError::Validation("slug required".into()))?;
    let title_fr = p.title_fr.ok_or(AppError::Validation("title_fr required".into()))?;
    let title_en = p.title_en.ok_or(AppError::Validation("title_en required".into()))?;
    let content_fr = p.content_fr.ok_or(AppError::Validation("content_fr required".into()))?;
    let content_en = p.content_en.ok_or(AppError::Validation("content_en required".into()))?;
    let excerpt_fr = p.excerpt_fr.ok_or(AppError::Validation("excerpt_fr required".into()))?;
    let excerpt_en = p.excerpt_en.ok_or(AppError::Validation("excerpt_en required".into()))?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO blog_posts
         (slug, title_fr, title_en, title_km, content_fr, content_en, content_km,
          excerpt_fr, excerpt_en, excerpt_km, tags, published)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         RETURNING id",
    )
    .bind(&slug)
    .bind(&title_fr)
    .bind(&title_en)
    .bind(&p.title_km)
    .bind(&content_fr)
    .bind(&content_en)
    .bind(&p.content_km)
    .bind(&excerpt_fr)
    .bind(&excerpt_en)
    .bind(&p.excerpt_km)
    .bind(p.tags.unwrap_or(json!([])))
    .bind(p.published.unwrap_or(false))
    .fetch_one(&pool)
    .await?;

    Ok(Json(json!({ "created": true, "id": id })))
}

pub async fn update_blog(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(p): Json<BlogPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;

    macro_rules! field {
        ($opt:expr, $col:expr) => {
            if $opt.is_some() { sets.push(format!("{} = ${i}", $col)); i += 1; }
        };
    }
    field!(p.title_fr, "title_fr");
    field!(p.title_en, "title_en");
    field!(p.title_km, "title_km");
    field!(p.content_fr, "content_fr");
    field!(p.content_en, "content_en");
    field!(p.content_km, "content_km");
    field!(p.excerpt_fr, "excerpt_fr");
    field!(p.excerpt_en, "excerpt_en");
    field!(p.excerpt_km, "excerpt_km");
    field!(p.tags, "tags");
    field!(p.published, "published");

    if sets.is_empty() {
        return Err(AppError::Validation("no fields to update".into()));
    }
    sets.push(format!("updated_at = NOW()"));
    let sql = format!("UPDATE blog_posts SET {} WHERE slug = ${i}", sets.join(", "));

    let mut q = sqlx::query(&sql);
    if let Some(v) = &p.title_fr   { q = q.bind(v); }
    if let Some(v) = &p.title_en   { q = q.bind(v); }
    if let Some(v) = &p.title_km   { q = q.bind(v); }
    if let Some(v) = &p.content_fr { q = q.bind(v); }
    if let Some(v) = &p.content_en { q = q.bind(v); }
    if let Some(v) = &p.content_km { q = q.bind(v); }
    if let Some(v) = &p.excerpt_fr { q = q.bind(v); }
    if let Some(v) = &p.excerpt_en { q = q.bind(v); }
    if let Some(v) = &p.excerpt_km { q = q.bind(v); }
    if let Some(v) = &p.tags       { q = q.bind(v); }
    if let Some(v) = p.published   { q = q.bind(v); }
    q = q.bind(&slug);

    let r = q.execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_blog(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let r = sqlx::query("DELETE FROM blog_posts WHERE slug = $1")
        .bind(&slug).execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "deleted": true })))
}

// ── Projects ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ProjectPayload {
    pub slug: Option<String>,
    pub title_fr: Option<String>,
    pub title_en: Option<String>,
    pub title_km: Option<String>,
    pub description_fr: Option<String>,
    pub description_en: Option<String>,
    pub description_km: Option<String>,
    pub tags: Option<Value>,
    pub github_url: Option<String>,
    pub demo_url: Option<String>,
    pub image_url: Option<String>,
    pub project_type: Option<String>,
    pub featured: Option<bool>,
}

pub async fn create_project(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<ProjectPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let slug = p.slug.ok_or(AppError::Validation("slug required".into()))?;
    let title_fr = p.title_fr.ok_or(AppError::Validation("title_fr required".into()))?;
    let title_en = p.title_en.ok_or(AppError::Validation("title_en required".into()))?;
    let desc_fr = p.description_fr.ok_or(AppError::Validation("description_fr required".into()))?;
    let desc_en = p.description_en.ok_or(AppError::Validation("description_en required".into()))?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO projects
         (slug, title_fr, title_en, title_km, description_fr, description_en, description_km,
          tags, github_url, demo_url, image_url, type, featured)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         RETURNING id",
    )
    .bind(&slug).bind(&title_fr).bind(&title_en).bind(&p.title_km)
    .bind(&desc_fr).bind(&desc_en).bind(&p.description_km)
    .bind(p.tags.unwrap_or(json!([])))
    .bind(&p.github_url).bind(&p.demo_url).bind(&p.image_url)
    .bind(p.project_type.as_deref().unwrap_or("project"))
    .bind(p.featured.unwrap_or(false))
    .fetch_one(&pool).await?;

    Ok(Json(json!({ "created": true, "id": id })))
}

pub async fn update_project(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    Json(p): Json<ProjectPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;
    macro_rules! field {
        ($opt:expr, $col:expr) => {
            if $opt.is_some() { sets.push(format!("{} = ${i}", $col)); i += 1; }
        };
    }
    field!(p.title_fr, "title_fr"); field!(p.title_en, "title_en"); field!(p.title_km, "title_km");
    field!(p.description_fr, "description_fr"); field!(p.description_en, "description_en");
    field!(p.description_km, "description_km"); field!(p.tags, "tags");
    field!(p.github_url, "github_url"); field!(p.demo_url, "demo_url");
    field!(p.image_url, "image_url"); field!(p.project_type, "type"); field!(p.featured, "featured");

    if sets.is_empty() { return Err(AppError::Validation("no fields to update".into())); }
    let sql = format!("UPDATE projects SET {} WHERE slug = ${i}", sets.join(", "));

    let mut q = sqlx::query(&sql);
    if let Some(v) = &p.title_fr       { q = q.bind(v); }
    if let Some(v) = &p.title_en       { q = q.bind(v); }
    if let Some(v) = &p.title_km       { q = q.bind(v); }
    if let Some(v) = &p.description_fr { q = q.bind(v); }
    if let Some(v) = &p.description_en { q = q.bind(v); }
    if let Some(v) = &p.description_km { q = q.bind(v); }
    if let Some(v) = &p.tags           { q = q.bind(v); }
    if let Some(v) = &p.github_url     { q = q.bind(v); }
    if let Some(v) = &p.demo_url       { q = q.bind(v); }
    if let Some(v) = &p.image_url      { q = q.bind(v); }
    if let Some(v) = &p.project_type   { q = q.bind(v); }
    if let Some(v) = p.featured        { q = q.bind(v); }
    q = q.bind(&slug);

    let r = q.execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_project(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let r = sqlx::query("DELETE FROM projects WHERE slug = $1")
        .bind(&slug).execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "deleted": true })))
}

// ── Experiences ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExperiencePayload {
    pub company: Option<String>,
    pub role_fr: Option<String>,
    pub role_en: Option<String>,
    pub role_km: Option<String>,
    pub date_start: Option<String>,
    pub date_end: Option<String>,
    pub location: Option<String>,
    pub bullets_fr: Option<Value>,
    pub bullets_en: Option<Value>,
    pub bullets_km: Option<Value>,
    pub sort_order: Option<i32>,
}

pub async fn create_experience(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<ExperiencePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let company = p.company.ok_or(AppError::Validation("company required".into()))?;
    let role_fr = p.role_fr.ok_or(AppError::Validation("role_fr required".into()))?;
    let role_en = p.role_en.ok_or(AppError::Validation("role_en required".into()))?;
    let date_start = p.date_start.ok_or(AppError::Validation("date_start required".into()))?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO experiences
         (company, role_fr, role_en, role_km, date_start, date_end, location,
          bullets_fr, bullets_en, bullets_km, sort_order)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         RETURNING id",
    )
    .bind(&company).bind(&role_fr).bind(&role_en).bind(&p.role_km)
    .bind(&date_start).bind(&p.date_end).bind(&p.location)
    .bind(p.bullets_fr.unwrap_or(json!([])))
    .bind(p.bullets_en.unwrap_or(json!([])))
    .bind(&p.bullets_km)
    .bind(p.sort_order.unwrap_or(0))
    .fetch_one(&pool).await?;

    Ok(Json(json!({ "created": true, "id": id })))
}

pub async fn update_experience(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(p): Json<ExperiencePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;
    macro_rules! field {
        ($opt:expr, $col:expr) => {
            if $opt.is_some() { sets.push(format!("{} = ${i}", $col)); i += 1; }
        };
    }
    field!(p.company, "company"); field!(p.role_fr, "role_fr"); field!(p.role_en, "role_en");
    field!(p.role_km, "role_km"); field!(p.date_start, "date_start"); field!(p.date_end, "date_end");
    field!(p.location, "location"); field!(p.bullets_fr, "bullets_fr");
    field!(p.bullets_en, "bullets_en"); field!(p.bullets_km, "bullets_km");
    field!(p.sort_order, "sort_order");

    if sets.is_empty() { return Err(AppError::Validation("no fields to update".into())); }
    let sql = format!("UPDATE experiences SET {} WHERE id = ${i}", sets.join(", "));

    let mut q = sqlx::query(&sql);
    if let Some(v) = &p.company    { q = q.bind(v); }
    if let Some(v) = &p.role_fr    { q = q.bind(v); }
    if let Some(v) = &p.role_en    { q = q.bind(v); }
    if let Some(v) = &p.role_km    { q = q.bind(v); }
    if let Some(v) = &p.date_start { q = q.bind(v); }
    if let Some(v) = &p.date_end   { q = q.bind(v); }
    if let Some(v) = &p.location   { q = q.bind(v); }
    if let Some(v) = &p.bullets_fr { q = q.bind(v); }
    if let Some(v) = &p.bullets_en { q = q.bind(v); }
    if let Some(v) = &p.bullets_km { q = q.bind(v); }
    if let Some(v) = p.sort_order  { q = q.bind(v); }
    q = q.bind(id);

    let r = q.execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_experience(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let r = sqlx::query("DELETE FROM experiences WHERE id = $1")
        .bind(id).execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "deleted": true })))
}

// ── Skills ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SkillPayload {
    pub category: Option<String>,
    pub name: Option<String>,
    pub color: Option<String>,
    pub sort_order: Option<i32>,
}

pub async fn create_skill(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(p): Json<SkillPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let category = p.category.ok_or(AppError::Validation("category required".into()))?;
    let name = p.name.ok_or(AppError::Validation("name required".into()))?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO skills (category, name, color, sort_order) VALUES ($1,$2,$3,$4) RETURNING id",
    )
    .bind(&category)
    .bind(&name)
    .bind(p.color.as_deref().unwrap_or("purple"))
    .bind(p.sort_order.unwrap_or(0))
    .fetch_one(&pool).await?;

    Ok(Json(json!({ "created": true, "id": id })))
}

pub async fn update_skill(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(p): Json<SkillPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;
    macro_rules! field {
        ($opt:expr, $col:expr) => {
            if $opt.is_some() { sets.push(format!("{} = ${i}", $col)); i += 1; }
        };
    }
    field!(p.category, "category"); field!(p.name, "name");
    field!(p.color, "color"); field!(p.sort_order, "sort_order");

    if sets.is_empty() { return Err(AppError::Validation("no fields to update".into())); }
    let sql = format!("UPDATE skills SET {} WHERE id = ${i}", sets.join(", "));

    let mut q = sqlx::query(&sql);
    if let Some(v) = &p.category   { q = q.bind(v); }
    if let Some(v) = &p.name       { q = q.bind(v); }
    if let Some(v) = &p.color      { q = q.bind(v); }
    if let Some(v) = p.sort_order  { q = q.bind(v); }
    q = q.bind(id);

    let r = q.execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_skill(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;
    let r = sqlx::query("DELETE FROM skills WHERE id = $1")
        .bind(id).execute(&pool).await?;
    if r.rows_affected() == 0 { return Err(AppError::NotFound); }
    Ok(Json(json!({ "deleted": true })))
}
