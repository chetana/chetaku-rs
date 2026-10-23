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

pub const MAX_BODY: usize = 256 * 1024;
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
    #[serde(default)]
    title: String,
    #[serde(default)]
    last_prompt: String,
    #[serde(default)]
    doing: String,
    #[serde(default)]
    todos: Vec<RawTodo>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    last_activity_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct RawTodo {
    content: String,
    #[serde(default)]
    status: String,
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

const MAX_TODOS: usize = 30;
const LINK_PREFIX: &str = "https://claude.ai/code/";

// Seul un lien vers une session claude.ai/code est conservé ; tout le reste est écarté, jamais une URL arbitraire.
fn safe_link(l: Option<&str>) -> Option<String> {
    let id = l?.strip_prefix(LINK_PREFIX)?;
    (id.starts_with("session_") && valid_ident(id, "_", 80)).then(|| format!("{LINK_PREFIX}{id}"))
}

fn clean_todos(todos: Vec<RawTodo>) -> Vec<Value> {
    todos
        .into_iter()
        .take(MAX_TODOS)
        .filter_map(|t| {
            let content = clip(&t.content, 200);
            let status = if valid_ident(&t.status, "-_", 24) { t.status } else { "pending".to_string() };
            (!content.is_empty()).then(|| json!({ "content": content, "status": status }))
        })
        .collect()
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
            "title": clip(&s.title, 160),
            "last_prompt": clip(&s.last_prompt, 300),
            "doing": clip(&s.doing, 400),
            "todos": clean_todos(s.todos),
            "link": safe_link(s.link.as_deref()),
            "last_activity_at": s.last_activity_at,
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
    fn conserve_titre_activite_taches_et_lien() {
        let mut s = sess("abc");
        s["title"] = json!("Daily note");
        s["last_prompt"] = json!("fais le point");
        s["doing"] = json!("Je lance les tests");
        s["todos"] = json!([{ "content": "a faire", "status": "in_progress" }, { "content": "", "status": "pending" }]);
        s["link"] = json!("https://claude.ai/code/session_01TgChoXa7Fogd9CC85avdUu");
        s["last_activity_at"] = json!("2026-10-23T17:05:00Z");
        let v = validate(&body(json!([s]))).unwrap().sessions[0].clone();
        assert_eq!(v["title"], "Daily note");
        assert_eq!(v["doing"], "Je lance les tests");
        assert_eq!(v["todos"].as_array().unwrap().len(), 1);
        assert_eq!(v["link"], "https://claude.ai/code/session_01TgChoXa7Fogd9CC85avdUu");
    }

    #[test]
    fn ecarte_les_liens_non_claude_et_les_statuts_de_tache_invalides() {
        for bad in ["https://evil.example/x", "http://claude.ai/code/session_abc", "https://claude.ai/code/../x", "javascript:alert(1)", "https://claude.ai/code/autre_chose"] {
            assert_eq!(safe_link(Some(bad)), None, "{bad}");
        }
        let t = clean_todos(vec![RawTodo { content: "x".into(), status: "a b; DROP".into() }]);
        assert_eq!(t[0]["status"], "pending");
    }

    #[test]
    fn plafonne_les_taches_et_tronque_les_textes() {
        let mut s = sess("abc");
        s["todos"] = Value::Array((0..50).map(|i| json!({ "content": format!("t{i}"), "status": "pending" })).collect());
        s["doing"] = json!("d".repeat(900));
        let v = validate(&body(json!([s]))).unwrap().sessions[0].clone();
        assert_eq!(v["todos"].as_array().unwrap().len(), MAX_TODOS);
        assert_eq!(v["doing"].as_str().unwrap().chars().count(), 400);
    }

    #[test]
    fn un_ancien_expediteur_sans_ces_champs_reste_accepte() {
        let v = validate(&body(json!([sess("abc")]))).unwrap().sessions[0].clone();
        assert_eq!(v["todos"], json!([]));
        assert_eq!(v["link"], Value::Null);
        assert_eq!(v["title"], "");
    }

    #[test]
    fn une_machine_silencieuse_est_perimee() {
        let t = Utc::now();
        assert_eq!(machine_json("m".into(), json!([]), t, t, STALE_AFTER_SECS)["stale"], false);
        assert_eq!(machine_json("m".into(), json!([]), t, t, STALE_AFTER_SECS + 1)["stale"], true);
    }
}
