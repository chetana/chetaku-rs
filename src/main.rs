mod db;
mod error;
mod models;
mod routes;
mod s3cache;
mod sync;

use axum::{Router, routing::{get, patch, post}};
use axum::{extract::Request, http::StatusCode, middleware::{from_fn, Next}, response::Response};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── Rate-limit par IP (protection anti-bot / anti-abus de coût) ──────────────
// Fenêtre glissante en mémoire : RL_MAX requêtes par IP sur RL_WINDOW → 429 au-delà.
// Best-effort : l'état est perdu au scale-to-zero (sans gravité pour une protection).
const RL_WINDOW: Duration = Duration::from_secs(60);
const RL_MAX: usize = 120; // 120 req/min/IP : large pour un humain, borne un bot

fn rl_map() -> &'static Mutex<HashMap<String, Vec<Instant>>> {
    static M: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

// IP réelle du client via X-Forwarded-For (Scaleway le pose devant le container).
fn client_ip(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

async fn rate_limit(req: Request, next: Next) -> Result<Response, StatusCode> {
    let ip = client_ip(&req);
    let now = Instant::now();
    {
        let mut map = rl_map().lock().unwrap();
        let hits = map.entry(ip).or_default();
        hits.retain(|t| now.duration_since(*t) < RL_WINDOW);
        if hits.len() >= RL_MAX {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        hits.push(now);
        // nettoyage opportuniste pour borner la mémoire
        if map.len() > 5000 {
            map.retain(|_, v| v.iter().any(|t| now.duration_since(*t) < RL_WINDOW));
        }
    }
    Ok(next.run(req).await)
}

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

    // Pool DB (paresseux : ne se connecte pas au boot)
    let pool = db::create_pool().await?;
    // Migrations : PAS à chaque démarrage (ça réveillerait la Serverless SQL à chaque cold-start).
    // Le schéma prod est stable ; pour appliquer une nouvelle migration, déployer avec RUN_MIGRATIONS=1.
    if std::env::var("RUN_MIGRATIONS").as_deref() == Ok("1") {
        db::run_migrations(&pool).await?;
    } else {
        tracing::info!("migrations ignorées au boot (RUN_MIGRATIONS!=1) — base non réveillée");
    }

    // CORS — autorise chetana.dev + chetlys + localhost dev
    let cors = CorsLayer::new()
        .allow_origin([
            "https://chetana.fr".parse().unwrap(),
            "https://chetana.dev".parse().unwrap(),
            "https://chetlys.vercel.app".parse().unwrap(),
            "http://localhost:3000".parse().unwrap(),
            "http://localhost:5173".parse().unwrap(),
        ])
        .allow_methods(Any)
        .allow_headers(Any);

    // Router
    let app = Router::new()
        .route("/health",                    get(routes::health::handler))
        // ── medialist / strava / voyage RETIRÉS (2026-08-10) : expérimentations sans valeur CV
        //    qui réveillaient la Serverless SQL (bots/crawls) → endpoints supprimés = 0 fuite.
        //    Le code des handlers reste dans routes/ (dormant) au cas où on rebâtirait plus tard.
        // Portfolio / CV
        .route("/blog",                      get(routes::blog::list))
        .route("/blog/{slug}",               get(routes::blog::get_one))
        .route("/projects",                  get(routes::portfolio::list_projects))
        .route("/projects/{slug}",           get(routes::portfolio::get_project))
        .route("/experiences",               get(routes::portfolio::list_experiences))
        .route("/skills",                    get(routes::portfolio::list_skills))
        // Contact
        .route("/comments/{post_id}",        get(routes::contact::list_comments))
        .route("/comments",                  post(routes::contact::create_comment))
        .route("/messages",                  post(routes::contact::create_message))
        // Admin write endpoints (protected by x-api-key)
        .route("/blog",                      post(routes::admin::create_blog))
        .route("/blog/{slug}",               patch(routes::admin::update_blog).delete(routes::admin::delete_blog))
        .route("/projects",                  post(routes::admin::create_project))
        .route("/projects/{slug}",           patch(routes::admin::update_project).delete(routes::admin::delete_project))
        .route("/experiences",               post(routes::admin::create_experience))
        .route("/experiences/{id}",          patch(routes::admin::update_experience).delete(routes::admin::delete_experience))
        .route("/skills",                    post(routes::admin::create_skill))
        .route("/skills/{id}",               patch(routes::admin::update_skill).delete(routes::admin::delete_skill))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(from_fn(rate_limit))
        .with_state(pool);

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{port}");
    tracing::info!("chetaku-rs listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
