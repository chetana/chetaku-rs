//! Sessions Claude Code actives : `PUT /live-sessions/{machine}` (token machine) remplace l'état, `GET /live-sessions` le relit.
//! Une ligne par machine, sans historique ; la fraîcheur se juge sur l'horloge du serveur.

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;
use crate::routes::runs::{guard, token_ok, valid_ident};

pub const MAX_BODY: usize = 128 * 1024;
const MAX_SESSIONS: usize = 200;
pub const STALE_AFTER_SECS: i64 = 150;

#[derive(Debug, Deserialize)]
struct RawSession {
    session_id: String,
    #[serde(default)]
    name: String,
    kind: String,
    status: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    pid: Option<i64>,
    #[serde(default)]
    started_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct RawPayload {
    reported_at: DateTime<Utc>,
    sessions: Vec<RawSession>,
}

#[derive(Debug)]
pub struct LivePayload {
    pub reported_at: DateTime<Utc>,
    pub sessions: Value,
}

fn clip(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

pub fn validate(body: &[u8]) -> Result<LivePayload, String> {
    let raw: RawPayload = serde_json::from_slice(body).map_err(|e| format!("JSON invalide: {e}"))?;
    if raw.sessions.len() > MAX_SESSIONS {
        return Err(format!("trop de sessions (≤ {MAX_SESSIONS})"));
    }
    let mut out = Vec::with_capacity(raw.sessions.len());
    for s in raw.sessions {
        if !valid_ident(&s.session_id, "-_", 64) {
            return Err("session_id invalide (a-z A-Z 0-9 - _, ≤ 64)".into());
        }
        if !valid_ident(&s.kind, "-_", 24) || !valid_ident(&s.status, "-_", 24) {
            return Err("kind/status invalide (a-z A-Z 0-9 - _, ≤ 24)".into());
        }
        out.push(json!({
            "session_id": s.session_id,
            "name": clip(&s.name, 120),
            "kind": s.kind,
            "status": s.status,
            "cwd": clip(&s.cwd, 300),
            "pid": s.pid,
            "started_at": s.started_at,
        }));
    }
    Ok(LivePayload { reported_at: raw.reported_at, sessions: Value::Array(out) })
}

pub async fn put(
    State(pool): State<PgPool>,
    Path(machine): Path<String>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, AppError> {
    let given = headers.get("x-runs-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !token_ok(std::env::var("RUNS_WRITE_TOKEN").ok().as_deref(), given) {
        return Err(AppError::NotFound);
    }
    if !valid_ident(&machine, "-", 64) || machine.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(AppError::Validation("machine invalide (a-z 0-9 -, ≤ 64)".into()));
    }
    let p = validate(&body).map_err(AppError::Validation)?;
    let count = p.sessions.as_array().map_or(0, |a| a.len());

    sqlx::query(
        "INSERT INTO live_sessions (machine, sessions, reported_at, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (machine) DO UPDATE
           SET sessions = EXCLUDED.sessions, reported_at = EXCLUDED.reported_at, updated_at = NOW()",
    )
    .bind(&machine)
    .bind(&p.sessions)
    .bind(p.reported_at)
    .execute(&pool)
    .await?;

    Ok(Json(json!({ "machine": machine, "sessions": count })))
}

pub async fn list(State(pool): State<PgPool>, headers: HeaderMap) -> Result<Json<Value>, AppError> {
    guard(&headers).await?;
    let rows: Vec<(String, Value, DateTime<Utc>, DateTime<Utc>, i64)> = sqlx::query_as(
        "SELECT machine, sessions, reported_at, updated_at,
                GREATEST(0, EXTRACT(EPOCH FROM (NOW() - updated_at))::BIGINT)
         FROM live_sessions ORDER BY machine",
    )
    .fetch_all(&pool)
    .await?;

    let machines: Vec<Value> = rows
        .into_iter()
        .map(|(machine, sessions, reported_at, updated_at, age)| machine_json(machine, sessions, reported_at, updated_at, age))
        .collect();
    Ok(Json(json!({ "server_time": Utc::now(), "stale_after_secs": STALE_AFTER_SECS, "machines": machines })))
}

// Une machine silencieuse garde ses dernières sessions mais reste `stale` : l'interface ne les montre jamais comme actives.
pub fn machine_json(machine: String, sessions: Value, reported_at: DateTime<Utc>, updated_at: DateTime<Utc>, age_secs: i64) -> Value {
    json!({
        "machine": machine,
        "sessions": sessions,
        "reported_at": reported_at,
        "updated_at": updated_at,
        "age_secs": age_secs,
        "stale": age_secs > STALE_AFTER_SECS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(sessions: Value) -> Vec<u8> {
        serde_json::to_vec(&json!({ "reported_at": "2026-10-23T17:00:00Z", "sessions": sessions })).unwrap()
    }

    fn sess(id: &str) -> Value {
        json!({ "session_id": id, "name": "djust-98", "kind": "interactive", "status": "busy",
                "cwd": "/home/cyin/djust", "pid": 8064, "started_at": "2026-10-23T16:00:00Z" })
    }

    #[test]
    fn accepte_une_liste_valide_et_vide() {
        let p = validate(&body(json!([sess("adab5e26-9df3-49d3-8661-1f7a805e9f8b")]))).unwrap();
        assert_eq!(p.sessions.as_array().unwrap().len(), 1);
        assert_eq!(validate(&body(json!([]))).unwrap().sessions, json!([]));
    }

    #[test]
    fn refuse_un_identifiant_dangereux() {
        for bad in ["../x", "a/b", "a b", "", &"a".repeat(65)] {
            assert!(validate(&body(json!([sess(bad)]))).is_err(), "{bad:?} aurait dû être refusé");
        }
    }

    #[test]
    fn refuse_kind_ou_status_non_identifiant() {
        let mut s = sess("abc");
        s["status"] = json!("busy; DROP");
        assert!(validate(&body(json!([s]))).is_err());
        let mut s = sess("abc");
        s["kind"] = json!("");
        assert!(validate(&body(json!([s]))).is_err());
    }

    #[test]
    fn plafonne_le_nombre_de_sessions() {
        let many: Vec<Value> = (0..=MAX_SESSIONS).map(|i| sess(&format!("s{i}"))).collect();
        assert!(validate(&body(Value::Array(many))).is_err());
    }

    #[test]
    fn tronque_nom_et_chemin_et_retire_les_caracteres_de_controle() {
        let mut s = sess("abc");
        s["name"] = json!(format!("a\u{0007}{}", "x".repeat(500)));
        s["cwd"] = json!("y".repeat(900));
        let p = validate(&body(json!([s]))).unwrap();
        let v = &p.sessions[0];
        assert_eq!(v["name"].as_str().unwrap().chars().count(), 120);
        assert!(!v["name"].as_str().unwrap().contains('\u{0007}'));
        assert_eq!(v["cwd"].as_str().unwrap().len(), 300);
    }

    #[test]
    fn refuse_json_invalide_ou_sans_date() {
        assert!(validate(b"pas du json").is_err());
        assert!(validate(br#"{"sessions": []}"#).is_err());
    }

    #[test]
    fn une_machine_silencieuse_est_perimee() {
        let t = Utc::now();
        assert_eq!(machine_json("m".into(), json!([]), t, t, STALE_AFTER_SECS)["stale"], false);
        assert_eq!(machine_json("m".into(), json!([]), t, t, STALE_AFTER_SECS + 1)["stale"], true);
    }
}
