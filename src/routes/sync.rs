use axum::{extract::State, http::HeaderMap, Json};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    error::AppError,
    models::{SyncAnimePayload, SyncGamePayload, SyncMoviePayload, SyncSeriesPayload},
    routes::stats,
    sync::{jikan, rawg, tmdb},
};

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

pub async fn sync_anime(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SyncAnimePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let status = payload.status.as_deref().unwrap_or("completed").to_string();
    let mut synced = 0usize;

    for mal_id in &payload.mal_ids {
        match jikan::fetch_anime(*mal_id).await {
            Ok(e) => {
                sqlx::query(
                    "INSERT INTO media_entries
                     (media_type, external_id, title, title_original, status,
                      episodes_total, cover_url, genres, creator, year, synced_at)
                     VALUES ('anime', $1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
                     ON CONFLICT (media_type, external_id) DO UPDATE SET
                       title = EXCLUDED.title, title_original = EXCLUDED.title_original,
                       episodes_total = EXCLUDED.episodes_total, cover_url = EXCLUDED.cover_url,
                       genres = EXCLUDED.genres, creator = EXCLUDED.creator,
                       year = EXCLUDED.year, synced_at = NOW()"
                )
                .bind(e.mal_id)
                .bind(&e.title)
                .bind(&e.title_original)
                .bind(&status)
                .bind(e.episodes_total)
                .bind(&e.cover_url)
                .bind(&e.genres)
                .bind(&e.creator)
                .bind(e.year)
                .execute(&pool)
                .await?;
                synced += 1;
            }
            Err(err) => tracing::warn!("Failed to sync anime {mal_id}: {err}"),
        }
    }

    if synced > 0 { let _ = stats::invalidate(&pool).await; }
    Ok(Json(json!({ "synced": synced, "total": payload.mal_ids.len() })))
}

pub async fn sync_game(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SyncGamePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let status = payload.status.as_deref().unwrap_or("completed").to_string();
    let mut synced = 0usize;

    for rawg_id in &payload.rawg_ids {
        match rawg::fetch_game(*rawg_id).await {
            Ok(e) => {
                sqlx::query(
                    "INSERT INTO media_entries
                     (media_type, external_id, title, status, platform,
                      cover_url, genres, creator, year, synced_at)
                     VALUES ('game', $1, $2, $3, $4, $5, $6, $7, $8, NOW())
                     ON CONFLICT (media_type, external_id) DO UPDATE SET
                       title = EXCLUDED.title, platform = EXCLUDED.platform,
                       cover_url = EXCLUDED.cover_url, genres = EXCLUDED.genres,
                       creator = EXCLUDED.creator, year = EXCLUDED.year,
                       synced_at = NOW()"
                )
                .bind(e.rawg_id)
                .bind(&e.title)
                .bind(&status)
                .bind(payload.platform.as_deref())
                .bind(&e.cover_url)
                .bind(&e.genres)
                .bind(&e.creator)
                .bind(e.year)
                .execute(&pool)
                .await?;
                synced += 1;
            }
            Err(err) => tracing::warn!("Failed to sync game {rawg_id}: {err}"),
        }
    }

    if synced > 0 { let _ = stats::invalidate(&pool).await; }
    Ok(Json(json!({ "synced": synced, "total": payload.rawg_ids.len() })))
}

pub async fn sync_movie(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SyncMoviePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let api_key = std::env::var("TMDB_API_KEY").unwrap_or_default();
    let status = payload.status.as_deref().unwrap_or("completed").to_string();
    let mut synced = 0usize;

    for tmdb_id in &payload.tmdb_ids {
        match tmdb::fetch_movie(*tmdb_id, &api_key).await {
            Ok(e) => {
                sqlx::query(
                    "INSERT INTO media_entries
                     (media_type, external_id, title, title_original, status,
                      cover_url, genres, creator, year, synced_at)
                     VALUES ('movie', $1, $2, $3, $4, $5, $6, $7, $8, NOW())
                     ON CONFLICT (media_type, external_id) DO UPDATE SET
                       title = EXCLUDED.title, title_original = EXCLUDED.title_original,
                       cover_url = EXCLUDED.cover_url, genres = EXCLUDED.genres,
                       creator = EXCLUDED.creator, year = EXCLUDED.year,
                       synced_at = NOW()"
                )
                .bind(e.tmdb_id)
                .bind(&e.title)
                .bind(&e.title_original)
                .bind(&status)
                .bind(&e.cover_url)
                .bind(&e.genres)
                .bind(&e.creator)
                .bind(e.year)
                .execute(&pool)
                .await?;
                synced += 1;
            }
            Err(err) => tracing::warn!("Failed to sync movie {tmdb_id}: {err}"),
        }
    }

    if synced > 0 { let _ = stats::invalidate(&pool).await; }
    Ok(Json(json!({ "synced": synced, "total": payload.tmdb_ids.len() })))
}

pub async fn sync_series(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<SyncSeriesPayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let api_key = std::env::var("TMDB_API_KEY").unwrap_or_default();
    let status = payload.status.as_deref().unwrap_or("completed").to_string();
    let mut synced = 0usize;

    for tmdb_id in &payload.tmdb_ids {
        match tmdb::fetch_series(*tmdb_id, &api_key).await {
            Ok(e) => {
                sqlx::query(
                    "INSERT INTO media_entries
                     (media_type, external_id, title, title_original, status,
                      episodes_total, cover_url, genres, creator, year, synced_at)
                     VALUES ('series', $1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
                     ON CONFLICT (media_type, external_id) DO UPDATE SET
                       title = EXCLUDED.title, title_original = EXCLUDED.title_original,
                       episodes_total = EXCLUDED.episodes_total, cover_url = EXCLUDED.cover_url,
                       genres = EXCLUDED.genres, creator = EXCLUDED.creator,
                       year = EXCLUDED.year, synced_at = NOW()"
                )
                .bind(e.tmdb_id)
                .bind(&e.title)
                .bind(&e.title_original)
                .bind(&status)
                .bind(e.episodes_total)
                .bind(&e.cover_url)
                .bind(&e.genres)
                .bind(&e.creator)
                .bind(e.year)
                .execute(&pool)
                .await?;
                synced += 1;
            }
            Err(err) => tracing::warn!("Failed to sync series {tmdb_id}: {err}"),
        }
    }

    if synced > 0 { let _ = stats::invalidate(&pool).await; }
    Ok(Json(json!({ "synced": synced, "total": payload.tmdb_ids.len() })))
}
