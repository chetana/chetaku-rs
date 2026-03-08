mod db;
mod error;
mod models;
mod routes;
mod sync;

use axum::{Router, routing::{get, patch, post}};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Charge .env si présent (dev local)
    dotenvy::dotenv().ok();

    // Logs
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "chetaku=debug,tower_http=debug".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Pool DB
    let pool = db::create_pool().await?;
    db::run_migrations(&pool).await?;

    // CORS — autorise chetana.dev + localhost dev
    let cors = CorsLayer::new()
        .allow_origin([
            "https://chetana.dev".parse().unwrap(),
            "http://localhost:3000".parse().unwrap(),
        ])
        .allow_methods(Any)
        .allow_headers(Any);

    // Router
    let app = Router::new()
        .route("/health",                    get(routes::health::handler))
        .route("/media",                     get(routes::media::list))
        .route("/media/{media_type}/{id}",   get(routes::media::get_one))
        .route("/stats",                     get(routes::stats::handler))
        .route("/sync/anime",                post(routes::sync::sync_anime))
        .route("/sync/game",                 post(routes::sync::sync_game))
        .route("/sync/movie",                post(routes::sync::sync_movie))
        .route("/sync/series",               post(routes::sync::sync_series))
        .route("/media/{id}",                patch(routes::update::update_entry)
                                             .delete(routes::update::delete_entry))
        .route("/cycling/activities",        get(routes::cycling::list))
        .route("/cycling/stats",             get(routes::cycling::stats))
        .route("/cycling/sync",              post(routes::cycling::sync))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!("chetaku-rs listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
