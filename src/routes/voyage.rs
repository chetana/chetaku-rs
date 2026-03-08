use axum::{extract::{Path, State}, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;

fn check_api_key(headers: &HeaderMap) -> Result<(), AppError> {
    let expected = std::env::var("API_KEY").unwrap_or_default();
    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Voyage {
    pub id: i32,
    pub title: String,
    pub country_code: String,
    pub country_name: String,
    pub continent: String,
    pub date_start: chrono::NaiveDate,
    pub date_end: chrono::NaiveDate,
    pub lat: f64,
    pub lng: f64,
    pub distance_km: i32,
    pub cover_gcs_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct YearCount {
    pub year: i32,
    pub trips: i64,
}

#[derive(Debug, Serialize)]
pub struct VoyageStats {
    pub total_trips: i64,
    pub total_countries: i64,
    pub total_km: i64,
    pub continents: Vec<String>,
    pub by_year: Vec<YearCount>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVoyagePayload {
    pub title: String,
    pub country_code: String,
    pub country_name: String,
    pub continent: String,
    pub date_start: chrono::NaiveDate,
    pub date_end: chrono::NaiveDate,
    pub lat: f64,
    pub lng: f64,
    pub distance_km: i32,
    pub cover_gcs_path: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVoyagePayload {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub cover_gcs_path: Option<String>,
    pub distance_km: Option<i32>,
}

// ── GET /voyage ───────────────────────────────────────────────────────────────

pub async fn list(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<Voyage>>, AppError> {
    let voyages = sqlx::query_as::<_, Voyage>(
        "SELECT id, title, country_code, country_name, continent,
                date_start, date_end, lat, lng, distance_km,
                cover_gcs_path, notes, created_at
         FROM voyages ORDER BY date_start DESC"
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(voyages))
}

// ── GET /voyage/stats ─────────────────────────────────────────────────────────

async fn compute_stats(pool: &PgPool) -> Result<VoyageStats, AppError> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        total_trips: Option<i64>,
        total_countries: Option<i64>,
        total_km: Option<i64>,
    }

    let t: Totals = sqlx::query_as::<_, Totals>(
        "SELECT COUNT(*)::BIGINT as total_trips,
                COUNT(DISTINCT country_code)::BIGINT as total_countries,
                COALESCE(SUM(distance_km), 0)::BIGINT as total_km
         FROM voyages"
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    #[derive(sqlx::FromRow)]
    struct ContinentRow { continent: String }
    let continents: Vec<String> = sqlx::query_as::<_, ContinentRow>(
        "SELECT DISTINCT continent FROM voyages ORDER BY continent"
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?
    .into_iter()
    .map(|r| r.continent)
    .collect();

    let by_year: Vec<YearCount> = sqlx::query_as::<_, YearCount>(
        "SELECT EXTRACT(year FROM date_start)::INT as year,
                COUNT(*)::BIGINT as trips
         FROM voyages GROUP BY year ORDER BY year DESC"
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(VoyageStats {
        total_trips:    t.total_trips.unwrap_or(0),
        total_countries: t.total_countries.unwrap_or(0),
        total_km:       t.total_km.unwrap_or(0),
        continents,
        by_year,
    })
}

pub async fn stats(
    State(pool): State<PgPool>,
) -> Result<Json<Value>, AppError> {
    let cache_key = "voyage_stats";

    let cached: Option<Value> = sqlx::query_scalar(
        "SELECT value FROM stats_cache WHERE key = $1 AND computed_at > NOW() - interval '30 seconds'"
    )
    .bind(cache_key)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Db)?;

    if let Some(v) = cached {
        return Ok(Json(v));
    }

    let result = compute_stats(&pool).await?;
    let value = serde_json::to_value(&result)
        .map_err(|e| AppError::ExternalApi(format!("voyage stats serialize: {e}")))?;

    sqlx::query(
        "INSERT INTO stats_cache (key, value, computed_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, computed_at = NOW()"
    )
    .bind(cache_key)
    .bind(&value)
    .execute(&pool)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(value))
}

// ── POST /voyage ──────────────────────────────────────────────────────────────

pub async fn create(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Json(payload): Json<CreateVoyagePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let id: i32 = sqlx::query_scalar(
        "INSERT INTO voyages
         (title, country_code, country_name, continent, date_start, date_end,
          lat, lng, distance_km, cover_gcs_path, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         RETURNING id"
    )
    .bind(&payload.title)
    .bind(&payload.country_code)
    .bind(&payload.country_name)
    .bind(&payload.continent)
    .bind(payload.date_start)
    .bind(payload.date_end)
    .bind(payload.lat)
    .bind(payload.lng)
    .bind(payload.distance_km)
    .bind(&payload.cover_gcs_path)
    .bind(&payload.notes)
    .fetch_one(&pool)
    .await
    .map_err(AppError::Db)?;

    invalidate_cache(&pool).await;
    Ok(Json(json!({ "created": true, "id": id })))
}

// ── PATCH /voyage/{id} ────────────────────────────────────────────────────────

pub async fn update(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateVoyagePayload>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let mut sets: Vec<String> = vec![];
    let mut i = 1i32;

    if payload.title.is_some()          { sets.push(format!("title = ${i}"));           i += 1; }
    if payload.notes.is_some()          { sets.push(format!("notes = ${i}"));            i += 1; }
    if payload.cover_gcs_path.is_some() { sets.push(format!("cover_gcs_path = ${i}"));  i += 1; }
    if payload.distance_km.is_some()    { sets.push(format!("distance_km = ${i}"));      i += 1; }

    if sets.is_empty() {
        return Ok(Json(json!({ "updated": false, "reason": "no fields provided" })));
    }

    sets.push(format!("updated_at = NOW()"));
    let sql = format!("UPDATE voyages SET {} WHERE id = ${i}", sets.join(", "));

    let mut q = sqlx::query(&sql);
    if let Some(v) = &payload.title          { q = q.bind(v); }
    if let Some(v) = &payload.notes          { q = q.bind(v); }
    if let Some(v) = &payload.cover_gcs_path { q = q.bind(v); }
    if let Some(v) = payload.distance_km     { q = q.bind(v); }
    q = q.bind(id);

    let result = q.execute(&pool).await.map_err(AppError::Db)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    invalidate_cache(&pool).await;
    Ok(Json(json!({ "updated": true, "id": id })))
}

// ── DELETE /voyage/{id} ───────────────────────────────────────────────────────

pub async fn delete_voyage(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let result = sqlx::query("DELETE FROM voyages WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    invalidate_cache(&pool).await;
    Ok(Json(json!({ "deleted": true, "id": id })))
}

// ── Cache invalidation ────────────────────────────────────────────────────────

async fn invalidate_cache(pool: &PgPool) {
    let _ = sqlx::query("DELETE FROM stats_cache WHERE key = 'voyage_stats'")
        .execute(pool)
        .await;
}
