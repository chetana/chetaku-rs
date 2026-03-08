use axum::{extract::State, Json};
use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::error::AppError;

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total_anime: i64,
    pub total_games: i64,
    pub anime_completed: i64,
    pub games_completed: i64,
    pub anime_watching: i64,
    pub games_playing: i64,
    pub average_anime_score: Option<f64>,
    pub average_game_score: Option<f64>,
    pub top_genres: Vec<GenreCount>,
}

#[derive(Debug, Serialize)]
pub struct GenreCount {
    pub genre: String,
    pub count: i64,
}

async fn count(pool: &PgPool, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await
}

pub async fn handler(State(pool): State<PgPool>) -> Result<Json<Stats>, AppError> {
    let total_anime     = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime'").await?;
    let total_games     = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game'").await?;
    let anime_completed = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime' AND status = 'completed'").await?;
    let games_completed = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game' AND status = 'completed'").await?;
    let anime_watching  = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime' AND status = 'watching'").await?;
    let games_playing   = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game' AND status = 'playing'").await?;

    let average_anime_score: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(score::float) FROM media_entries WHERE media_type = 'anime' AND score IS NOT NULL"
    ).fetch_one(&pool).await?;

    let average_game_score: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(score::float) FROM media_entries WHERE media_type = 'game' AND score IS NOT NULL"
    ).fetch_one(&pool).await?;

    let rows = sqlx::query(
        "SELECT genre, COUNT(*) as count
         FROM media_entries, UNNEST(genres) AS genre
         GROUP BY genre ORDER BY count DESC LIMIT 10"
    )
    .fetch_all(&pool)
    .await?;

    let top_genres = rows.into_iter().map(|row| GenreCount {
        genre: row.get("genre"),
        count: row.get("count"),
    }).collect();

    Ok(Json(Stats {
        total_anime, total_games,
        anime_completed, games_completed,
        anime_watching, games_playing,
        average_anime_score, average_game_score,
        top_genres,
    }))
}
