use crate::error::AppError;
use serde::Deserialize;

// ─── Réponses API TMDB ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TmdbMovieResponse {
    id: i32,
    title: String,
    original_title: Option<String>,
    genres: Vec<TmdbGenre>,
    release_date: Option<String>,
    poster_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbTvResponse {
    id: i32,
    name: String,
    original_name: Option<String>,
    genres: Vec<TmdbGenre>,
    first_air_date: Option<String>,
    poster_path: Option<String>,
    number_of_episodes: Option<i32>,
    created_by: Vec<TmdbCreator>,
    networks: Vec<TmdbNetwork>,
}

#[derive(Debug, Deserialize)]
struct TmdbCreditsResponse {
    crew: Vec<TmdbCrewMember>,
}

#[derive(Debug, Deserialize)]
struct TmdbGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct TmdbCreator {
    name: String,
}

#[derive(Debug, Deserialize)]
struct TmdbNetwork {
    name: String,
}

#[derive(Debug, Deserialize)]
struct TmdbCrewMember {
    name: String,
    job: String,
}

// ─── Données normalisées ──────────────────────────────────────────────────────

pub struct MovieData {
    pub tmdb_id: i32,
    pub title: String,
    pub title_original: Option<String>,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub creator: Option<String>, // réalisateur
    pub year: Option<i32>,
}

pub struct SeriesData {
    pub tmdb_id: i32,
    pub title: String,
    pub title_original: Option<String>,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub creator: Option<String>, // créateur ou réseau
    pub year: Option<i32>,
    pub episodes_total: Option<i32>,
}

// ─── Fonctions de fetch ───────────────────────────────────────────────────────

fn extract_year(date: Option<&str>) -> Option<i32> {
    date?.get(..4)?.parse().ok()
}

pub async fn fetch_movie(tmdb_id: i32, api_key: &str) -> Result<MovieData, AppError> {
    let client = reqwest::Client::new();

    let movie: TmdbMovieResponse = client
        .get(format!("https://api.themoviedb.org/3/movie/{tmdb_id}"))
        .query(&[("api_key", api_key)])
        .send()
        .await
        .map_err(|e| AppError::ExternalApi(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::ExternalApi(format!("movie {tmdb_id}: {e}")))?;

    let credits: TmdbCreditsResponse = client
        .get(format!("https://api.themoviedb.org/3/movie/{tmdb_id}/credits"))
        .query(&[("api_key", api_key)])
        .send()
        .await
        .map_err(|e| AppError::ExternalApi(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::ExternalApi(format!("credits {tmdb_id}: {e}")))?;

    let director = credits.crew.into_iter()
        .find(|c| c.job == "Director")
        .map(|c| c.name);

    Ok(MovieData {
        tmdb_id: movie.id,
        title: movie.title,
        title_original: movie.original_title,
        cover_url: movie.poster_path.map(|p| format!("https://image.tmdb.org/t/p/w500{p}")),
        genres: movie.genres.into_iter().map(|g| g.name).collect(),
        creator: director,
        year: extract_year(movie.release_date.as_deref()),
    })
}

pub async fn fetch_series(tmdb_id: i32, api_key: &str) -> Result<SeriesData, AppError> {
    let tv: TmdbTvResponse = reqwest::Client::new()
        .get(format!("https://api.themoviedb.org/3/tv/{tmdb_id}"))
        .query(&[("api_key", api_key)])
        .send()
        .await
        .map_err(|e| AppError::ExternalApi(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::ExternalApi(format!("tv {tmdb_id}: {e}")))?;

    let TmdbTvResponse {
        id, name, original_name, genres, first_air_date, poster_path,
        number_of_episodes, created_by, networks,
    } = tv;

    let creator = created_by.into_iter().next()
        .map(|c| c.name)
        .or_else(|| networks.into_iter().next().map(|n| n.name));

    Ok(SeriesData {
        tmdb_id: id,
        title: name,
        title_original: original_name.filter(|s| !s.is_empty()),
        cover_url: poster_path.map(|p| format!("https://image.tmdb.org/t/p/w500{p}")),
        genres: genres.into_iter().map(|g| g.name).collect(),
        creator,
        year: extract_year(first_air_date.as_deref()),
        episodes_total: number_of_episodes,
    })
}
