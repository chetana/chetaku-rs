use axum::{extract::{Query, State}, http::HeaderMap, Json};
use serde::Deserialize;
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

// ── Sport type helpers ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SportQuery {
    pub sport: Option<String>, // "cycling" | "running" | "swimming" | absent = all
}

fn sport_types(sport: &Option<String>) -> Vec<String> {
    match sport.as_deref() {
        Some("cycling") => vec!["Ride", "VirtualRide", "MountainBikeRide", "GravelRide", "EBikeRide", "Velomobile"],
        Some("running") => vec!["Run", "TrailRun", "VirtualRun"],
        Some("swimming") => vec!["Swim", "OpenWaterSwim"],
        _ => vec![],
    }.into_iter().map(String::from).collect()
}

// ── GET /strava/activities ────────────────────────────────────────────────────

pub async fn list(
    State(pool): State<PgPool>,
    Query(params): Query<SportQuery>,
) -> Result<Json<Vec<CyclingActivity>>, AppError> {
    let types = sport_types(&params.sport);
    let activities = if types.is_empty() {
        sqlx::query_as::<_, CyclingActivity>(
            "SELECT id, name, sport_type, start_date, distance_m, moving_time_s, elapsed_time_s,
                    elevation_gain_m, average_speed_ms, max_speed_ms, average_watts,
                    average_heartrate, max_heartrate, average_cadence, calories,
                    kudos_count, pr_count, trainer, commute, map_polyline, synced_at
             FROM strava_activities ORDER BY start_date DESC"
        ).fetch_all(&pool).await?
    } else {
        sqlx::query_as::<_, CyclingActivity>(
            "SELECT id, name, sport_type, start_date, distance_m, moving_time_s, elapsed_time_s,
                    elevation_gain_m, average_speed_ms, max_speed_ms, average_watts,
                    average_heartrate, max_heartrate, average_cadence, calories,
                    kudos_count, pr_count, trainer, commute, map_polyline, synced_at
             FROM strava_activities WHERE sport_type = ANY($1) ORDER BY start_date DESC"
        ).bind(&types[..]).fetch_all(&pool).await?
    };

    Ok(Json(activities))
}

// ── GET /strava/stats ─────────────────────────────────────────────────────────

async fn compute_stats(pool: &PgPool, types: &[String]) -> Result<CyclingStats, AppError> {
    #[derive(sqlx::FromRow)]
    struct Totals {
        total_rides: Option<i64>,
        total_km: Option<f64>,
        total_elevation_m: Option<f64>,
        total_moving_time_s: Option<i64>,
        best_ride_km: Option<f64>,
        best_elevation_m: Option<f64>,
    }

    let filtered = !types.is_empty();

    macro_rules! q {
        ($sql:expr, one, $T:ty) => {{
            let mut query = sqlx::query_as::<_, $T>($sql);
            if filtered { query = query.bind(types); }
            query.fetch_one(pool).await.map_err(AppError::Db)?
        }};
        ($sql:expr, all, $T:ty) => {{
            let mut query = sqlx::query_as::<_, $T>($sql);
            if filtered { query = query.bind(types); }
            query.fetch_all(pool).await.map_err(AppError::Db)?
        }};
    }

    let w = if filtered { " WHERE sport_type = ANY($1)" } else { "" };
    let wm = if filtered {
        "WHERE sport_type = ANY($1) AND start_date >= NOW() - INTERVAL '12 months'"
    } else {
        "WHERE start_date >= NOW() - INTERVAL '12 months'"
    };

    let sql_totals = format!(
        "SELECT COUNT(*)::BIGINT as total_rides,
                COALESCE(SUM(distance_m)/1000.0,0)::FLOAT8 as total_km,
                COALESCE(SUM(elevation_gain_m),0)::FLOAT8 as total_elevation_m,
                COALESCE(SUM(moving_time_s)::BIGINT,0) as total_moving_time_s,
                COALESCE(MAX(distance_m)/1000.0,0)::FLOAT8 as best_ride_km,
                COALESCE(MAX(elevation_gain_m),0)::FLOAT8 as best_elevation_m
         FROM strava_activities{w}"
    );
    let sql_monthly = format!(
        "SELECT TO_CHAR(start_date,'YYYY-MM') as month,
                ROUND((SUM(distance_m)/1000.0)::numeric,1)::FLOAT8 as km,
                ROUND(SUM(elevation_gain_m)::numeric,0)::FLOAT8 as elevation_m,
                COUNT(*)::BIGINT as rides
         FROM strava_activities {wm}
         GROUP BY month ORDER BY month ASC"
    );
    let sql_by_type = format!(
        "SELECT sport_type,
                COUNT(*)::BIGINT as count,
                ROUND((SUM(distance_m)/1000.0)::numeric,1)::FLOAT8 as km
         FROM strava_activities{w}
         GROUP BY sport_type ORDER BY count DESC"
    );
    let sql_top = format!(
        "SELECT id, name, start_date, distance_m, elevation_gain_m, moving_time_s, average_speed_ms
         FROM strava_activities{w}
         ORDER BY distance_m DESC LIMIT 5"
    );

    let t: Totals                    = q!(&sql_totals,  one, Totals);
    let monthly: Vec<MonthlyTotal>   = q!(&sql_monthly, all, MonthlyTotal);
    let by_type: Vec<SportTypeStat>  = q!(&sql_by_type, all, SportTypeStat);
    let top: Vec<TopRide>            = q!(&sql_top,     all, TopRide);

    let total_rides = t.total_rides.unwrap_or(0);
    let total_km = t.total_km.unwrap_or(0.0);
    let average_km = if total_rides > 0 { total_km / total_rides as f64 } else { 0.0 };

    Ok(CyclingStats {
        total_rides,
        total_km,
        total_elevation_m: t.total_elevation_m.unwrap_or(0.0),
        total_moving_time_s: t.total_moving_time_s.unwrap_or(0),
        best_ride_km: t.best_ride_km.unwrap_or(0.0),
        best_elevation_m: t.best_elevation_m.unwrap_or(0.0),
        average_km_per_ride: (average_km * 10.0).round() / 10.0,
        monthly,
        by_sport_type: by_type,
        top_rides: top,
    })
}

pub async fn stats(
    State(pool): State<PgPool>,
    Query(params): Query<SportQuery>,
) -> Result<Json<Value>, AppError> {
    let sport = params.sport.as_deref().unwrap_or("all");
    let cache_key = format!("strava_{sport}");

    // 1 requête rapide — JSONB en cache si < 30s
    let cached: Option<Value> = sqlx::query_scalar(
        "SELECT value FROM stats_cache WHERE key = $1 AND computed_at > NOW() - interval '30 seconds'"
    )
    .bind(&cache_key)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Db)?;

    if let Some(v) = cached {
        return Ok(Json(v));
    }

    // Cache absent ou expiré → recompute
    let types = sport_types(&params.sport);
    let result = compute_stats(&pool, &types).await?;
    let value = serde_json::to_value(&result)
        .map_err(|e| AppError::ExternalApi(format!("strava stats serialize: {e}")))?;

    sqlx::query(
        "INSERT INTO stats_cache (key, value, computed_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, computed_at = NOW()"
    )
    .bind(&cache_key)
    .bind(&value)
    .execute(&pool)
    .await
    .map_err(AppError::Db)?;

    Ok(Json(value))
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

    // Invalide les 3 caches strava
    if let Err(e) = sqlx::query("DELETE FROM stats_cache WHERE key LIKE 'strava_%'")
        .execute(&pool).await
    {
        tracing::warn!("Failed to invalidate strava stats cache: {e}");
    }

    Ok(Json(json!({ "synced": synced, "total": total })))
}
