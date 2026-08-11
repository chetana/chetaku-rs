use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};

pub async fn create_pool() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    // search_path forcé à public : Scaleway Serverless SQL a un search_path vide
    // par défaut (inoffensif sur Neon dont le défaut inclut déjà public).
    // idle_timeout court + min_connections=0 : on relâche vite les connexions pour que
    // la base Serverless SQL puisse scale-to-zero (sinon elle reste facturée au CPU).
    // connect_lazy : AUCUNE connexion ouverte au démarrage. La première connexion n'est
    // établie qu'à la première requête réelle. Crucial : sinon chaque cold-start du container
    // (réveillé par un crawler) réveillerait la Serverless SQL rien qu'au boot.
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(0)
        .idle_timeout(std::time::Duration::from_secs(10))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET search_path TO public").await?;
                Ok(())
            })
        })
        .connect_lazy(&url)?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Migrations applied");
    Ok(())
}
