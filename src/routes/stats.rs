use axum::{extract::State, Json};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row};

use crate::error::AppError;

// ── Types ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct GenreCount {
    pub genre: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct GenreStat {
    pub genre: String,
    pub count: i64,
    pub avg_score: f64,
    pub love_score: f64,
}

#[derive(Debug, Serialize)]
pub struct ScoreCount {
    pub score: i16,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct StatusCount {
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct CreatorStat {
    pub creator: String,
    pub count: i64,
    pub avg_score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub total_anime: i64,
    pub total_games: i64,
    pub total_movies: i64,
    pub total_series: i64,
    pub anime_completed: i64,
    pub games_completed: i64,
    pub movies_completed: i64,
    pub series_completed: i64,
    pub anime_watching: i64,
    pub games_playing: i64,
    pub average_anime_score: Option<f64>,
    pub average_game_score: Option<f64>,
    pub average_movie_score: Option<f64>,
    pub average_series_score: Option<f64>,
    pub total_episodes_watched: i64,
    pub total_playtime_hours: i64,
    pub top_genres: Vec<GenreCount>,
    pub top_anime_genres: Vec<GenreStat>,
    pub top_game_genres: Vec<GenreStat>,
    pub top_movie_genres: Vec<GenreStat>,
    pub top_series_genres: Vec<GenreStat>,
    pub anime_score_distribution: Vec<ScoreCount>,
    pub game_score_distribution: Vec<ScoreCount>,
    pub movie_score_distribution: Vec<ScoreCount>,
    pub series_score_distribution: Vec<ScoreCount>,
    pub anime_status: Vec<StatusCount>,
    pub game_status: Vec<StatusCount>,
    pub movie_status: Vec<StatusCount>,
    pub series_status: Vec<StatusCount>,
    pub top_anime_studios: Vec<CreatorStat>,
    pub top_game_developers: Vec<CreatorStat>,
    pub top_movie_directors: Vec<CreatorStat>,
    pub top_series_creators: Vec<CreatorStat>,
}

// ── Helpers SQL ────────────────────────────────────────────────────────────────

async fn count(pool: &PgPool, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await
}

async fn genre_stats(pool: &PgPool, media_type: &str) -> Result<Vec<GenreStat>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT genre,
                COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
                ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
         FROM media_entries, UNNEST(genres) AS genre
         WHERE media_type = '{media_type}' AND score IS NOT NULL
         GROUP BY genre ORDER BY love_score DESC LIMIT 10"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| GenreStat {
        genre: r.get("genre"),
        count: r.get("count"),
        avg_score: r.get("avg_score"),
        love_score: r.get("love_score"),
    }).collect())
}

async fn score_dist(pool: &PgPool, media_type: &str) -> Result<Vec<ScoreCount>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT score, COUNT(*) as count
         FROM media_entries WHERE media_type = '{media_type}' AND score IS NOT NULL
         GROUP BY score ORDER BY score"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| ScoreCount {
        score: r.get("score"),
        count: r.get("count"),
    }).collect())
}

async fn status_counts(pool: &PgPool, media_type: &str) -> Result<Vec<StatusCount>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT status, COUNT(*) as count
         FROM media_entries WHERE media_type = '{media_type}'
         GROUP BY status ORDER BY count DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| StatusCount {
        status: r.get("status"),
        count: r.get("count"),
    }).collect())
}

async fn top_creators(pool: &PgPool, media_type: &str) -> Result<Vec<CreatorStat>, sqlx::Error> {
    let rows = sqlx::query(&format!(
        "SELECT creator, COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 1)::float8 as avg_score
         FROM media_entries
         WHERE media_type = '{media_type}' AND creator IS NOT NULL
         GROUP BY creator ORDER BY count DESC, avg_score DESC NULLS LAST LIMIT 6"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| CreatorStat {
        creator: r.get("creator"),
        count: r.get("count"),
        avg_score: r.get("avg_score"),
    }).collect())
}

// ── Calcul + stockage en DB ───────────────────────────────────────────────────

pub async fn compute_and_store(pool: &PgPool) -> Result<Value, AppError> {
    let (
        (total_anime, total_games, total_movies, total_series),
        (anime_completed, games_completed, movies_completed, series_completed),
        (anime_watching, games_playing),
        (average_anime_score, average_game_score, average_movie_score, average_series_score),
        (total_episodes_watched, total_playtime_hours),
        top_genres_rows,
        (top_anime_genres, top_game_genres, top_movie_genres, top_series_genres),
        (anime_score_distribution, game_score_distribution, movie_score_distribution, series_score_distribution),
        (anime_status, game_status, movie_status, series_status),
        (top_anime_studios, top_game_developers, top_movie_directors, top_series_creators),
    ) = tokio::try_join!(
        async { tokio::try_join!(
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'movie'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'series'"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime' AND status = 'completed'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game' AND status = 'completed'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'movie' AND status = 'completed'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'series' AND status = 'completed'"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'anime' AND status = 'watching'"),
            count(pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'game' AND status = 'playing'"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(score::float) FROM media_entries WHERE media_type = 'anime' AND score IS NOT NULL").fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(score::float) FROM media_entries WHERE media_type = 'game' AND score IS NOT NULL").fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(score::float) FROM media_entries WHERE media_type = 'movie' AND score IS NOT NULL").fetch_one(pool),
            sqlx::query_scalar::<_, Option<f64>>("SELECT AVG(score::float) FROM media_entries WHERE media_type = 'series' AND score IS NOT NULL").fetch_one(pool),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(episodes_watched), 0) FROM media_entries WHERE media_type IN ('anime', 'series')").fetch_one(pool),
            sqlx::query_scalar::<_, i64>("SELECT COALESCE(SUM(playtime_hours), 0) FROM media_entries WHERE media_type = 'game'").fetch_one(pool),
        ).map_err(AppError::Db) },
        async { sqlx::query(
            "SELECT genre, COUNT(*) as count FROM media_entries, UNNEST(genres) AS genre GROUP BY genre ORDER BY count DESC LIMIT 10"
        ).fetch_all(pool).await.map_err(AppError::Db) },
        async { tokio::try_join!(
            genre_stats(pool, "anime"), genre_stats(pool, "game"),
            genre_stats(pool, "movie"), genre_stats(pool, "series"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            score_dist(pool, "anime"), score_dist(pool, "game"),
            score_dist(pool, "movie"), score_dist(pool, "series"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            status_counts(pool, "anime"), status_counts(pool, "game"),
            status_counts(pool, "movie"), status_counts(pool, "series"),
        ).map_err(AppError::Db) },
        async { tokio::try_join!(
            top_creators(pool, "anime"), top_creators(pool, "game"),
            top_creators(pool, "movie"), top_creators(pool, "series"),
        ).map_err(AppError::Db) },
    )?;

    let top_genres = top_genres_rows.into_iter().map(|r: sqlx::postgres::PgRow| GenreCount {
        genre: r.get("genre"),
        count: r.get("count"),
    }).collect();

    let stats = Stats {
        total_anime, total_games, total_movies, total_series,
        anime_completed, games_completed, movies_completed, series_completed,
        anime_watching, games_playing,
        average_anime_score, average_game_score, average_movie_score, average_series_score,
        total_episodes_watched, total_playtime_hours,
        top_genres,
        top_anime_genres, top_game_genres, top_movie_genres, top_series_genres,
        anime_score_distribution, game_score_distribution,
        movie_score_distribution, series_score_distribution,
        anime_status, game_status, movie_status, series_status,
        top_anime_studios, top_game_developers, top_movie_directors, top_series_creators,
    };

    let value = serde_json::to_value(&stats)
        .map_err(|e| AppError::ExternalApi(format!("stats serialize: {e}")))?;

    sqlx::query(
        "INSERT INTO stats_cache (key, value, computed_at)
         VALUES ('media_stats', $1, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, computed_at = NOW()"
    )
    .bind(&value)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    tracing::info!("Stats recomputed and cached in DB");
    Ok(value)
}

/// À appeler après tout ajout / modification / suppression
pub async fn invalidate(pool: &PgPool) {
    if let Err(e) = sqlx::query("DELETE FROM stats_cache WHERE key = 'media_stats'")
        .execute(pool)
        .await
    {
        tracing::warn!("Failed to invalidate stats cache: {e}");
    }
}

// ── Handler ────────────────────────────────────────────────────────────────────

pub async fn handler(State(pool): State<PgPool>) -> Result<Json<Value>, AppError> {
    // 1 seule requête rapide — lit le JSONB stocké
    let cached: Option<Value> = sqlx::query_scalar(
        "SELECT value FROM stats_cache WHERE key = 'media_stats' AND computed_at > NOW() - interval '30 seconds'"
    )
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Db)?;

    if let Some(value) = cached {
        return Ok(Json(value));
    }

    // Cache absent → calculer maintenant (premier appel, ou après invalidation)
    Ok(Json(compute_and_store(&pool).await?))
}
