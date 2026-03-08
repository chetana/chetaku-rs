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
    let entries = match (&params.media_type, &params.status) {
        (Some(t), Some(s)) => {
            sqlx::query_as::<_, MediaEntry>(&format!("{SELECT} WHERE media_type = $1 AND status = $2 ORDER BY created_at DESC"))
                .bind(t).bind(s).fetch_all(&pool).await?
        }
        (Some(t), None) => {
            sqlx::query_as::<_, MediaEntry>(&format!("{SELECT} WHERE media_type = $1 ORDER BY created_at DESC"))
                .bind(t).fetch_all(&pool).await?
        }
        _ => {
            sqlx::query_as::<_, MediaEntry>(&format!("{SELECT} ORDER BY created_at DESC"))
                .fetch_all(&pool).await?
        }
    };
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
