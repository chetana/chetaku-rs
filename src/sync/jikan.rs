use anyhow::Result;
use serde::Deserialize;

// ─── Structs de réponse Jikan v4 ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JikanResponse {
    data: JikanAnime,
}

#[derive(Debug, Deserialize)]
struct JikanAnime {
    mal_id: i32,
    title: String,
    title_japanese: Option<String>,
    episodes: Option<i32>,
    images: JikanImages,
    genres: Vec<JikanGenre>,
    studios: Vec<JikanStudio>,
    year: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct JikanImages {
    jpg: JikanJpg,
}

#[derive(Debug, Deserialize)]
struct JikanJpg {
    large_image_url: Option<String>,
    image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JikanGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct JikanStudio {
    name: String,
}

// ─── Struct normalisée renvoyée au reste de l'app ─────────────────────────

pub struct AnimeData {
    pub mal_id: i32,
    pub title: String,
    pub title_original: Option<String>,
    pub episodes_total: Option<i32>,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub creator: Option<String>, // studio principal
    pub year: Option<i32>,
}

pub async fn fetch_anime(mal_id: i32) -> Result<AnimeData> {
    let url = format!("https://api.jikan.moe/v4/anime/{mal_id}/full");
    let resp: JikanResponse = reqwest::get(&url).await?.json().await?;
    let a = resp.data;

    // Jikan rate-limit : 3 req/sec — on attend un peu entre les appels
    tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;

    Ok(AnimeData {
        mal_id: a.mal_id,
        title: a.title,
        title_original: a.title_japanese,
        episodes_total: a.episodes,
        cover_url: a.images.jpg.large_image_url.or(a.images.jpg.image_url),
        genres: a.genres.into_iter().map(|g| g.name).collect(),
        creator: a.studios.into_iter().next().map(|s| s.name),
        year: a.year,
    })
}
