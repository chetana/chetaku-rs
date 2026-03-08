use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::{error::AppError, models::MediaEntry};

#[derive(Debug, Deserialize)]
pub struct MediaQuery {
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    pub status: Option<String>,
    pub q: Option<String>,
}

const SELECT: &str =
    "SELECT id, media_type, external_id, title, title_original, status, score,
            episodes_watched, episodes_total, playtime_hours, platform,
            cover_url, genres, creator, year, notes, synced_at, created_at
     FROM media_entries";

pub async fn list(
    State(pool): State<PgPool>,
    Query(params): Query<MediaQuery>,
) -> Result<Json<Vec<MediaEntry>>, AppError> {
    let mut conditions: Vec<String> = vec![];
    let mut idx: u32 = 1;

    if params.media_type.is_some() {
        conditions.push(format!("media_type = ${idx}"));
        idx += 1;
    }
    if params.status.is_some() {
        conditions.push(format!("status = ${idx}"));
        idx += 1;
    }
    if params.q.is_some() {
        conditions.push(format!(
            "(title ILIKE ${idx} OR title_original ILIKE ${idx})"
        ));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("{SELECT} {where_clause} ORDER BY created_at DESC");
    let mut query = sqlx::query_as::<_, MediaEntry>(&sql);

    if let Some(t) = &params.media_type {
        query = query.bind(t);
    }
    if let Some(s) = &params.status {
        query = query.bind(s);
    }
    if let Some(q) = &params.q {
        query = query.bind(format!("%{q}%"));
    }

    let entries = query.fetch_all(&pool).await?;
    Ok(Json(entries))
}

pub async fn get_one(
    State(pool): State<PgPool>,
    Path((media_type, external_id)): Path<(String, i32)>,
) -> Result<Json<MediaEntry>, AppError> {
    let entry = sqlx::query_as::<_, MediaEntry>(
        &format!("{SELECT} WHERE media_type = $1 AND external_id = $2")
    )
    .bind(&media_type)
    .bind(external_id)
    .fetch_optional(&pool)
    .await?
    .ok_or(AppError::NotFound)?;

    Ok(Json(entry))
}
