use axum::{extract::{Path, State}, http::HeaderMap, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;

#[derive(Debug, Deserialize)]
pub struct UpdatePayload {
    pub status: Option<String>,
    pub platform: Option<String>,
    pub episodes_watched: Option<i32>,
    pub score: Option<i16>,
    pub notes: Option<String>,
}

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

pub async fn update_entry(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(payload): Json<UpdatePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    // Build SET clauses dynamically
    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;

    if payload.status.is_some()           { sets.push(format!("status = ${i}"));            i += 1; }
    if payload.platform.is_some()         { sets.push(format!("platform = ${i}"));           i += 1; }
    if payload.episodes_watched.is_some() { sets.push(format!("episodes_watched = ${i}"));   i += 1; }
    if payload.score.is_some()            { sets.push(format!("score = ${i}"));               i += 1; }
    if payload.notes.is_some()            { sets.push(format!("notes = ${i}"));               i += 1; }

    if sets.is_empty() {
        return Ok(Json(json!({ "updated": false, "reason": "no fields provided" })));
    }

    let where_clause = format!("WHERE id = ${i}");
    let sql = format!("UPDATE media_entries SET {} {}", sets.join(", "), where_clause);

    let mut q = sqlx::query(&sql);

    if let Some(v) = &payload.status           { q = q.bind(v); }
    if let Some(v) = &payload.platform         { q = q.bind(v); }
    if let Some(v) = payload.episodes_watched  { q = q.bind(v); }
    if let Some(v) = payload.score             { q = q.bind(v); }
    if let Some(v) = &payload.notes            { q = q.bind(v); }

    q = q.bind(id);

    let result = q.execute(&pool).await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(json!({ "updated": true, "id": id })))
}
