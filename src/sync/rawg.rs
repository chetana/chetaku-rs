use anyhow::Result;
use serde::Deserialize;

// ─── Structs de réponse RAWG v1 ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawgGame {
    id: i32,
    name: String,
    background_image: Option<String>,
    genres: Vec<RawgGenre>,
    developers: Vec<RawgDeveloper>,
    released: Option<String>, // "2023-09-22"
}

#[derive(Debug, Deserialize)]
struct RawgGenre {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RawgDeveloper {
    name: String,
}

// ─── Struct normalisée ────────────────────────────────────────────────────

pub struct GameData {
    pub rawg_id: i32,
    pub title: String,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub creator: Option<String>, // developer principal
    pub year: Option<i32>,
}

pub async fn fetch_game(rawg_id: i32) -> Result<GameData> {
    let api_key = std::env::var("RAWG_API_KEY")
        .expect("RAWG_API_KEY must be set");
    let url = format!("https://api.rawg.io/api/games/{rawg_id}?key={api_key}");

    let game: RawgGame = reqwest::get(&url).await?.json().await?;

    // Extrait l'année depuis la date "YYYY-MM-DD"
    let year = game.released.as_deref()
        .and_then(|d| d.split('-').next())
        .and_then(|y| y.parse().ok());

    Ok(GameData {
        rawg_id: game.id,
        title: game.name,
        cover_url: game.background_image,
        genres: game.genres.into_iter().map(|g| g.name).collect(),
        creator: game.developers.into_iter().next().map(|d| d.name),
        year,
    })
}
