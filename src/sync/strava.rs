use chrono::{DateTime, Utc};
use serde::Deserialize;
use tokio::time::{sleep, Duration};

use crate::error::AppError;

// ── Token refresh ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

pub async fn get_access_token() -> Result<String, AppError> {
    let client_id = std::env::var("STRAVA_CLIENT_ID")
        .map_err(|_| AppError::ExternalApi("STRAVA_CLIENT_ID not set".into()))?;
    let client_secret = std::env::var("STRAVA_CLIENT_SECRET")
        .map_err(|_| AppError::ExternalApi("STRAVA_CLIENT_SECRET not set".into()))?;
    let refresh_token = std::env::var("STRAVA_REFRESH_TOKEN")
        .map_err(|_| AppError::ExternalApi("STRAVA_REFRESH_TOKEN not set".into()))?;

    let client = reqwest::Client::new();
    let resp = client
        .post("https://www.strava.com/api/v3/oauth/token")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("grant_type",    "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| AppError::ExternalApi(format!("Strava token request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::ExternalApi(format!(
            "Strava token error: HTTP {}",
            resp.status()
        )));
    }

    let token: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ExternalApi(format!("Strava token parse error: {e}")))?;

    Ok(token.access_token)
}

// ── Activity structs ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct StravaMap {
    summary_polyline: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawActivity {
    id: i64,
    name: String,
    sport_type: String,
    start_date: DateTime<Utc>,
    distance: f64,
    moving_time: i32,
    elapsed_time: i32,
    total_elevation_gain: f64,
    average_speed: Option<f64>,
    max_speed: Option<f64>,
    average_watts: Option<f64>,
    device_watts: Option<bool>,
    average_heartrate: Option<f64>,
    max_heartrate: Option<f64>,
    average_cadence: Option<f64>,
    kilojoules: Option<f64>,
    kudos_count: Option<i32>,
    pr_count: Option<i32>,
    trainer: Option<bool>,
    commute: Option<bool>,
    map: Option<StravaMap>,
}

pub struct StravaActivity {
    pub id: i64,
    pub name: String,
    pub sport_type: String,
    pub start_date: DateTime<Utc>,
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
}

const CYCLING_TYPES: &[&str] = &[
    "Ride",
    "VirtualRide",
    "MountainBikeRide",
    "GravelRide",
    "EBikeRide",
    "Velomobile",
];

// ── Fetch all cycling activities (paginated) ──────────────────────────────────

pub async fn fetch_all_activities(access_token: &str) -> Result<Vec<StravaActivity>, AppError> {
    let client = reqwest::Client::new();
    let mut all: Vec<StravaActivity> = vec![];
    let mut page = 1u32;

    loop {
        let resp = client
            .get("https://www.strava.com/api/v3/athlete/activities")
            .bearer_auth(access_token)
            .query(&[
                ("per_page", "200"),
                ("page",     &page.to_string()),
            ])
            .send()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Strava activities request failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(AppError::ExternalApi(format!(
                "Strava activities error: HTTP {}",
                resp.status()
            )));
        }

        let raw: Vec<RawActivity> = resp
            .json()
            .await
            .map_err(|e| AppError::ExternalApi(format!("Strava activities parse error: {e}")))?;

        if raw.is_empty() {
            break;
        }

        for a in raw {
            if !CYCLING_TYPES.contains(&a.sport_type.as_str()) {
                continue;
            }
            // kilojoules → kcal approximation (1 kJ ≈ 0.239 kcal)
            let calories = a.kilojoules.map(|kj| kj * 0.239);
            // Only include watts if device measured (not estimated)
            let average_watts = if a.device_watts.unwrap_or(false) {
                a.average_watts
            } else {
                None
            };

            all.push(StravaActivity {
                id:               a.id,
                name:             a.name,
                sport_type:       a.sport_type,
                start_date:       a.start_date,
                distance_m:       a.distance,
                moving_time_s:    a.moving_time,
                elapsed_time_s:   a.elapsed_time,
                elevation_gain_m: a.total_elevation_gain,
                average_speed_ms: a.average_speed,
                max_speed_ms:     a.max_speed,
                average_watts,
                average_heartrate: a.average_heartrate,
                max_heartrate:    a.max_heartrate,
                average_cadence:  a.average_cadence,
                calories,
                kudos_count:      a.kudos_count.unwrap_or(0),
                pr_count:         a.pr_count.unwrap_or(0),
                trainer:          a.trainer.unwrap_or(false),
                commute:          a.commute.unwrap_or(false),
                map_polyline:     a.map.and_then(|m| m.summary_polyline).filter(|s| !s.is_empty()),
            });
        }

        page += 1;
        // Rate limit: 100 req/15min → 400ms entre pages
        sleep(Duration::from_millis(400)).await;
    }

    tracing::info!("Strava: fetched {} cycling activities", all.len());
    Ok(all)
}
