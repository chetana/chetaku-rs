use axum::{extract::State, Json};
use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::error::AppError;

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

async fn count(pool: &PgPool, sql: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await
}

pub async fn handler(State(pool): State<PgPool>) -> Result<Json<Stats>, AppError> {
    // ── Counts de base ────────────────────────────────────────────────────────
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

    // ── Totaux épisodes / playtime ────────────────────────────────────────────
    let total_episodes_watched: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(episodes_watched), 0) FROM media_entries WHERE media_type IN ('anime', 'series')"
    ).fetch_one(&pool).await?;

    let total_playtime_hours: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(playtime_hours), 0) FROM media_entries WHERE media_type = 'game'"
    ).fetch_one(&pool).await?;

    // ── Top genres (compat existant — count only) ────────────────────────────
    let top_genres_rows = sqlx::query(
        "SELECT genre, COUNT(*) as count
         FROM media_entries, UNNEST(genres) AS genre
         GROUP BY genre ORDER BY count DESC LIMIT 10"
    ).fetch_all(&pool).await?;

    let top_genres = top_genres_rows.into_iter().map(|row| GenreCount {
        genre: row.get("genre"),
        count: row.get("count"),
    }).collect();

    // ── Genres pondérés par score (anime) ────────────────────────────────────
    let anime_genre_rows = sqlx::query(
        "SELECT genre,
                COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
                ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
         FROM media_entries, UNNEST(genres) AS genre
         WHERE media_type = 'anime' AND score IS NOT NULL
         GROUP BY genre ORDER BY love_score DESC LIMIT 10"
    ).fetch_all(&pool).await?;

    let top_anime_genres = anime_genre_rows.into_iter().map(|row| GenreStat {
        genre: row.get("genre"),
        count: row.get("count"),
        avg_score: row.get::<f64, _>("avg_score"),
        love_score: row.get::<f64, _>("love_score"),
    }).collect();

    // ── Genres pondérés par score (jeux) ─────────────────────────────────────
    let game_genre_rows = sqlx::query(
        "SELECT genre,
                COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
                ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
         FROM media_entries, UNNEST(genres) AS genre
         WHERE media_type = 'game' AND score IS NOT NULL
         GROUP BY genre ORDER BY love_score DESC LIMIT 10"
    ).fetch_all(&pool).await?;

    let top_game_genres = game_genre_rows.into_iter().map(|row| GenreStat {
        genre: row.get("genre"),
        count: row.get("count"),
        avg_score: row.get::<f64, _>("avg_score"),
        love_score: row.get::<f64, _>("love_score"),
    }).collect();

    // ── Distribution des scores (anime) ──────────────────────────────────────
    let anime_score_rows = sqlx::query(
        "SELECT score, COUNT(*) as count
         FROM media_entries WHERE media_type = 'anime' AND score IS NOT NULL
         GROUP BY score ORDER BY score"
    ).fetch_all(&pool).await?;

    let anime_score_distribution = anime_score_rows.into_iter().map(|row| ScoreCount {
        score: row.get("score"),
        count: row.get("count"),
    }).collect();

    // ── Distribution des scores (jeux) ───────────────────────────────────────
    let game_score_rows = sqlx::query(
        "SELECT score, COUNT(*) as count
         FROM media_entries WHERE media_type = 'game' AND score IS NOT NULL
         GROUP BY score ORDER BY score"
    ).fetch_all(&pool).await?;

    let game_score_distribution = game_score_rows.into_iter().map(|row| ScoreCount {
        score: row.get("score"),
        count: row.get("count"),
    }).collect();

    // ── Statuts anime ─────────────────────────────────────────────────────────
    let anime_status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count
         FROM media_entries WHERE media_type = 'anime'
         GROUP BY status ORDER BY count DESC"
    ).fetch_all(&pool).await?;

    let anime_status = anime_status_rows.into_iter().map(|row| StatusCount {
        status: row.get("status"),
        count: row.get("count"),
    }).collect();

    // ── Statuts jeux ─────────────────────────────────────────────────────────
    let game_status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count
         FROM media_entries WHERE media_type = 'game'
         GROUP BY status ORDER BY count DESC"
    ).fetch_all(&pool).await?;

    let game_status = game_status_rows.into_iter().map(|row| StatusCount {
        status: row.get("status"),
        count: row.get("count"),
    }).collect();

    // ── Studios anime (top 6) ─────────────────────────────────────────────────
    let anime_studio_rows = sqlx::query(
        "SELECT creator, COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 1)::float8 as avg_score
         FROM media_entries
         WHERE media_type = 'anime' AND creator IS NOT NULL
         GROUP BY creator ORDER BY count DESC, avg_score DESC NULLS LAST LIMIT 6"
    ).fetch_all(&pool).await?;

    let top_anime_studios = anime_studio_rows.into_iter().map(|row| CreatorStat {
        creator: row.get("creator"),
        count: row.get("count"),
        avg_score: row.get::<Option<f64>, _>("avg_score"),
    }).collect();

    // ── Développeurs jeux (top 6) ─────────────────────────────────────────────
    let game_dev_rows = sqlx::query(
        "SELECT creator, COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 1)::float8 as avg_score
         FROM media_entries
         WHERE media_type = 'game' AND creator IS NOT NULL
         GROUP BY creator ORDER BY count DESC, avg_score DESC NULLS LAST LIMIT 6"
    ).fetch_all(&pool).await?;

    let top_game_developers = game_dev_rows.into_iter().map(|row| CreatorStat {
        creator: row.get("creator"),
        count: row.get("count"),
        avg_score: row.get::<Option<f64>, _>("avg_score"),
    }).collect();

    // ── Films ─────────────────────────────────────────────────────────────────
    let total_movies   = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'movie'").await?;
    let movies_completed = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'movie' AND status = 'completed'").await?;

    let average_movie_score: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(score::float) FROM media_entries WHERE media_type = 'movie' AND score IS NOT NULL"
    ).fetch_one(&pool).await?;

    let movie_genre_rows = sqlx::query(
        "SELECT genre,
                COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
                ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
         FROM media_entries, UNNEST(genres) AS genre
         WHERE media_type = 'movie' AND score IS NOT NULL
         GROUP BY genre ORDER BY love_score DESC LIMIT 10"
    ).fetch_all(&pool).await?;

    let top_movie_genres = movie_genre_rows.into_iter().map(|row| GenreStat {
        genre: row.get("genre"),
        count: row.get("count"),
        avg_score: row.get::<f64, _>("avg_score"),
        love_score: row.get::<f64, _>("love_score"),
    }).collect();

    let movie_score_rows = sqlx::query(
        "SELECT score, COUNT(*) as count
         FROM media_entries WHERE media_type = 'movie' AND score IS NOT NULL
         GROUP BY score ORDER BY score"
    ).fetch_all(&pool).await?;

    let movie_score_distribution = movie_score_rows.into_iter().map(|row| ScoreCount {
        score: row.get("score"),
        count: row.get("count"),
    }).collect();

    let movie_status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count
         FROM media_entries WHERE media_type = 'movie'
         GROUP BY status ORDER BY count DESC"
    ).fetch_all(&pool).await?;

    let movie_status = movie_status_rows.into_iter().map(|row| StatusCount {
        status: row.get("status"),
        count: row.get("count"),
    }).collect();

    let movie_director_rows = sqlx::query(
        "SELECT creator, COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 1)::float8 as avg_score
         FROM media_entries
         WHERE media_type = 'movie' AND creator IS NOT NULL
         GROUP BY creator ORDER BY count DESC, avg_score DESC NULLS LAST LIMIT 6"
    ).fetch_all(&pool).await?;

    let top_movie_directors = movie_director_rows.into_iter().map(|row| CreatorStat {
        creator: row.get("creator"),
        count: row.get("count"),
        avg_score: row.get::<Option<f64>, _>("avg_score"),
    }).collect();

    // ── Séries ────────────────────────────────────────────────────────────────
    let total_series   = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'series'").await?;
    let series_completed = count(&pool, "SELECT COUNT(*) FROM media_entries WHERE media_type = 'series' AND status = 'completed'").await?;

    let average_series_score: Option<f64> = sqlx::query_scalar(
        "SELECT AVG(score::float) FROM media_entries WHERE media_type = 'series' AND score IS NOT NULL"
    ).fetch_one(&pool).await?;

    let series_genre_rows = sqlx::query(
        "SELECT genre,
                COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 2)::float8 as avg_score,
                ROUND((COUNT(*) * AVG(score::float))::numeric, 2)::float8 as love_score
         FROM media_entries, UNNEST(genres) AS genre
         WHERE media_type = 'series' AND score IS NOT NULL
         GROUP BY genre ORDER BY love_score DESC LIMIT 10"
    ).fetch_all(&pool).await?;

    let top_series_genres = series_genre_rows.into_iter().map(|row| GenreStat {
        genre: row.get("genre"),
        count: row.get("count"),
        avg_score: row.get::<f64, _>("avg_score"),
        love_score: row.get::<f64, _>("love_score"),
    }).collect();

    let series_score_rows = sqlx::query(
        "SELECT score, COUNT(*) as count
         FROM media_entries WHERE media_type = 'series' AND score IS NOT NULL
         GROUP BY score ORDER BY score"
    ).fetch_all(&pool).await?;

    let series_score_distribution = series_score_rows.into_iter().map(|row| ScoreCount {
        score: row.get("score"),
        count: row.get("count"),
    }).collect();

    let series_status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count
         FROM media_entries WHERE media_type = 'series'
         GROUP BY status ORDER BY count DESC"
    ).fetch_all(&pool).await?;

    let series_status = series_status_rows.into_iter().map(|row| StatusCount {
        status: row.get("status"),
        count: row.get("count"),
    }).collect();

    let series_creator_rows = sqlx::query(
        "SELECT creator, COUNT(*) as count,
                ROUND(AVG(score::float)::numeric, 1)::float8 as avg_score
         FROM media_entries
         WHERE media_type = 'series' AND creator IS NOT NULL
         GROUP BY creator ORDER BY count DESC, avg_score DESC NULLS LAST LIMIT 6"
    ).fetch_all(&pool).await?;

    let top_series_creators = series_creator_rows.into_iter().map(|row| CreatorStat {
        creator: row.get("creator"),
        count: row.get("count"),
        avg_score: row.get::<Option<f64>, _>("avg_score"),
    }).collect();

    Ok(Json(Stats {
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
    }))
}
