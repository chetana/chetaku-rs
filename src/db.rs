use sqlx::postgres::PgPoolOptions;
use sqlx::{Executor, PgPool};

pub async fn create_pool() -> anyhow::Result<PgPool> {
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    // search_path forcé à public : Scaleway Serverless SQL a un search_path vide
    // par défaut (inoffensif sur Neon dont le défaut inclut déjà public).
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET search_path TO public").await?;
                Ok(())
            })
        })
        .connect(&url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Migrations applied");
    Ok(())
}
