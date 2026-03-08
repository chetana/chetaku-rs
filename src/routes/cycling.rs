use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{error::AppError, sync::strava};

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
pub struct CyclingActivity {
    pub id: i64,
    pub name: String,
    pub sport_type: String,
    pub start_date: chrono::DateTime<chrono::Utc>,
    pub distance_m: f64,
    pub moving_time_s: i32,
    pub elapsed_time_s: i32,
    pub elevation_gain_m: f64,
    pub average_speed_ms: Option<f64>,
    pub max_speed_ms: Option<f64>,
    pub average_watts: Option<f64>,
    pub average_heartrate: Option<f64>,
    pub max_heartrate: Option<f64>,
    pub average_cadence: Option<f64>,
    pub calories: Option<f64>,
    pub kudos_count: i32,
    pub pr_count: i32,
    pub trainer: bool,
    pub commute: bool,
    pub map_polyline: Option<String>,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MonthlyTotal {
    pub month: String,   // 'YYYY-MM'
    pub km: f64,
    pub elevation_m: f64,
    pub rides: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SportTypeStat {
    pub sport_type: String,
    pub count: i64,
    pub km: f64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TopRide {
    pub id: i64,
    pub name: String,
    pub start_date: chrono::DateTime<chrono::Utc>,
    pub distance_m: f64,
    pub elevation_gain_m: f64,
    pub moving_time_s: i32,
    pub average_speed_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct CyclingStats {
    pub total_rides: i64,
    pub total_km: f64,
    pub total_elevation_m: f64,
    pub total_moving_time_s: i64,
    pub best_ride_km: f64,
    pub best_elevation_m: f64,
    pub average_km_per_ride: f64,
    pub monthly: Vec<MonthlyTotal>,
    pub by_sport_type: Vec<SportTypeStat>,
    pub top_rides: Vec<TopRide>,
}

// ── GET /cycling/activities ───────────────────────────────────────────────────

pub async fn list(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<CyclingActivity>>, AppError> {
    let activities = sqlx::query_as::<_, CyclingActivity>(
        "SELECT id, name, sport_type, start_date, distance_m, moving_time_s, elapsed_time_s,
                elevation_gain_m, average_speed_ms, max_speed_ms, average_watts,
                average_heartrate, max_heartrate, average_cadence, calories,
                kudos_count, pr_count, trainer, commute, map_polyline, synced_at
         FROM strava_activities
         ORDER BY start_date DESC"
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(activities))
}

// ── GET /cycling/stats ────────────────────────────────────────────────────────

pub async fn stats(
    State(pool): State<PgPool>,
) -> Result<Json<CyclingStats>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        total_rides: Option<i64>,
        total_km: Option<f64>,
        total_elevation_m: Option<f64>,
        total_moving_time_s: Option<i64>,
        best_ride_km: Option<f64>,
        best_elevation_m: Option<f64>,
    }

    let (totals, monthly, by_type, top) = tokio::join!(
        sqlx::query_as::<_, Totals>(
            "SELECT
               COUNT(*)::BIGINT as total_rides,
               COALESCE(SUM(distance_m) / 1000.0, 0)::FLOAT8 as total_km,
               COALESCE(SUM(elevation_gain_m), 0)::FLOAT8 as total_elevation_m,
               COALESCE(SUM(moving_time_s)::BIGINT, 0) as total_moving_time_s,
               COALESCE(MAX(distance_m) / 1000.0, 0)::FLOAT8 as best_ride_km,
               COALESCE(MAX(elevation_gain_m), 0)::FLOAT8 as best_elevation_m
             FROM strava_activities"
        ).fetch_one(&pool),

        sqlx::query_as::<_, MonthlyTotal>(
            "SELECT
               TO_CHAR(start_date, 'YYYY-MM') as month,
               ROUND((SUM(distance_m) / 1000.0)::numeric, 1)::FLOAT8 as km,
               ROUND(SUM(elevation_gain_m)::numeric, 0)::FLOAT8 as elevation_m,
               COUNT(*)::BIGINT as rides
             FROM strava_activities
             WHERE start_date >= NOW() - INTERVAL '12 months'
             GROUP BY month
             ORDER BY month ASC"
        ).fetch_all(&pool),

        sqlx::query_as::<_, SportTypeStat>(
            "SELECT
               sport_type,
               COUNT(*)::BIGINT as count,
               ROUND((SUM(distance_m) / 1000.0)::numeric, 1)::FLOAT8 as km
             FROM strava_activities
             GROUP BY sport_type
             ORDER BY count DESC"
        ).fetch_all(&pool),

        sqlx::query_as::<_, TopRide>(
            "SELECT id, name, start_date, distance_m, elevation_gain_m, moving_time_s, average_speed_ms
             FROM strava_activities
             ORDER BY distance_m DESC
             LIMIT 5"
        ).fetch_all(&pool),
    );

    let t: Totals = totals.map_err(AppError::Db)?;
    let total_rides = t.total_rides.unwrap_or(0);
    let total_km = t.total_km.unwrap_or(0.0);
    let average_km = if total_rides > 0 { total_km / total_rides as f64 } else { 0.0 };

    Ok(Json(CyclingStats {
        total_rides,
        total_km,
        total_elevation_m: t.total_elevation_m.unwrap_or(0.0),
        total_moving_time_s: t.total_moving_time_s.unwrap_or(0),
        best_ride_km: t.best_ride_km.unwrap_or(0.0),
        best_elevation_m: t.best_elevation_m.unwrap_or(0.0),
        average_km_per_ride: (average_km * 10.0).round() / 10.0,
        monthly: monthly.map_err(AppError::Db)?,
        by_sport_type: by_type.map_err(AppError::Db)?,
        top_rides: top.map_err(AppError::Db)?,
    }))
}

// ── POST /cycling/sync ────────────────────────────────────────────────────────

pub async fn sync(
    State(pool): State<PgPool>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    check_api_key(&headers)?;

    let access_token = strava::get_access_token().await?;
    let activities = strava::fetch_all_activities(&access_token).await?;
    let total = activities.len();
    let mut synced = 0usize;

    for a in activities {
        let result = sqlx::query(
            "INSERT INTO strava_activities
             (id, name, sport_type, start_date, distance_m, moving_time_s, elapsed_time_s,
              elevation_gain_m, average_speed_ms, max_speed_ms, average_watts,
              average_heartrate, max_heartrate, average_cadence, calories,
              kudos_count, pr_count, trainer, commute, map_polyline, synced_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,NOW())
             ON CONFLICT (id) DO UPDATE SET
               name             = EXCLUDED.name,
               kudos_count      = EXCLUDED.kudos_count,
               pr_count         = EXCLUDED.pr_count,
               average_watts    = EXCLUDED.average_watts,
               average_heartrate = EXCLUDED.average_heartrate,
               max_heartrate    = EXCLUDED.max_heartrate,
               calories         = EXCLUDED.calories,
               synced_at        = NOW()"
        )
        .bind(a.id)
        .bind(&a.name)
        .bind(&a.sport_type)
        .bind(a.start_date)
        .bind(a.distance_m)
        .bind(a.moving_time_s)
        .bind(a.elapsed_time_s)
        .bind(a.elevation_gain_m)
        .bind(a.average_speed_ms)
        .bind(a.max_speed_ms)
        .bind(a.average_watts)
        .bind(a.average_heartrate)
        .bind(a.max_heartrate)
        .bind(a.average_cadence)
        .bind(a.calories)
        .bind(a.kudos_count)
        .bind(a.pr_count)
        .bind(a.trainer)
        .bind(a.commute)
        .bind(a.map_polyline.as_deref())
        .execute(&pool)
        .await;

        match result {
            Ok(_) => synced += 1,
            Err(e) => tracing::warn!("Failed to upsert activity {}: {e}", a.id),
        }
    }

    Ok(Json(json!({ "synced": synced, "total": total })))
}
