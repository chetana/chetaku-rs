use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Anime,
    Game,
    Movie,
    Series,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    // Anime / Séries
    Watching,
    // Jeux
    Playing,
    // Commun
    Completed,
    Dropped,
    PlanToWatch,
    PlanToPlay,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MediaEntry {
    pub id: i32,
    pub media_type: String,
    pub external_id: i32,
    pub title: String,
    pub title_original: Option<String>,
    pub status: String,
    pub score: Option<i16>,

    // Anime
    pub episodes_watched: Option<i32>,
    pub episodes_total: Option<i32>,

    // Jeux
    pub playtime_hours: Option<i32>,
    pub platform: Option<String>,

    // Commun
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub creator: Option<String>,
    pub year: Option<i32>,
    pub notes: Option<String>,
    pub synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ─── Payloads de sync ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SyncAnimePayload {
    pub mal_ids: Vec<i32>,
    /// optionnel : statut à assigner ("watching", "completed", etc.)
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyncGamePayload {
    pub rawg_ids: Vec<i32>,
    pub status: Option<String>,
    pub platform: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyncMoviePayload {
    pub tmdb_ids: Vec<i32>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SyncSeriesPayload {
    pub tmdb_ids: Vec<i32>,
    pub status: Option<String>,
}
