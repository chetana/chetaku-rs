//! Rapports des automatisations : `POST /runs` (token machine `RUNS_WRITE_TOKEN`, idempotent sur `run_id`).
//! Le markdown va sur S3 avant la ligne Postgres : au pire un objet orphelin, jamais une ligne qui pointe dans le vide.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::error::AppError;
use crate::s3cache;

pub const MAX_BODY: usize = 512 * 1024;
const MAX_REPORT: usize = 256 * 1024;
const MAX_HEADLINE: usize = 300;
const MAX_JSON: usize = 64 * 1024; // metrics / items / impact, chacun
const STATUSES: [&str; 4] = ["ok", "noop", "alert", "error"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRun {
    run_id: String,
    automation: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    status: String,
    #[serde(default)]
    headline: String,
    #[serde(default)]
    metrics: Option<Value>,
    #[serde(default)]
    items: Option<Value>,
    #[serde(default)]
    impact: Option<Value>,
    #[serde(default)]
    report_md: Option<String>,
}

#[derive(Debug)]
pub struct NewRun {
    pub run_id: String,
    pub automation: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: String,
    pub headline: String,
    pub metrics: Value,
    pub items: Value,
    pub impact: Value,
    pub report_md: Option<String>,
}

impl NewRun {
    /// `run_id` et `automation` sont validés : aucun `/` ni `..` possible dans la clé.
    pub fn report_key(&self) -> String {
        format!("reports/{}/{}/{}.md", self.automation, self.started_at.format("%Y-%m-%d"), self.run_id)
    }
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Token absent, vide ou < 16 caractères → refus systématique.
pub fn token_ok(expected: Option<&str>, given: &str) -> bool {
    match expected {
        Some(t) if t.len() >= 16 => ct_eq(t.as_bytes(), given.as_bytes()),
        _ => false,
    }
}

pub(crate) fn valid_ident(s: &str, allow_extra: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.bytes().all(|c| c.is_ascii_alphanumeric() || allow_extra.as_bytes().contains(&c))
        && !s.starts_with('.')
        && !s.contains("..")
}

fn json_field(v: Option<Value>, want_array: bool, name: &str) -> Result<Value, String> {
    let default = if want_array { json!([]) } else { json!({}) };
    let v = v.unwrap_or(default);
    let shape_ok = if want_array { v.is_array() } else { v.is_object() };
    if !shape_ok {
        return Err(format!("{name}: {} attendu", if want_array { "tableau" } else { "objet" }));
    }
    if v.to_string().len() > MAX_JSON {
        return Err(format!("{name} trop volumineux"));
    }
    Ok(v)
}

pub fn validate(body: &[u8]) -> Result<NewRun, String> {
    let r: RawRun = serde_json::from_slice(body).map_err(|e| format!("JSON invalide: {e}"))?;
    if !valid_ident(&r.automation, "-", 64) || r.automation.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("automation invalide (a-z 0-9 -, ≤ 64)".into());
    }
    if !valid_ident(&r.run_id, "-_.:", 128) {
        return Err("run_id invalide (A-Z a-z 0-9 - _ . :, ≤ 128)".into());
    }
    if !STATUSES.contains(&r.status.as_str()) {
        return Err("status invalide (ok|noop|alert|error)".into());
    }
    if r.headline.chars().count() > MAX_HEADLINE || r.headline.contains('\n') {
        return Err("headline: une ligne, ≤ 300 caractères".into());
    }
    if let Some(e) = r.ended_at {
        if e < r.started_at {
            return Err("ended_at antérieur à started_at".into());
        }
    }
    if let Some(m) = &r.report_md {
        if m.len() > MAX_REPORT {
            return Err("report_md trop volumineux".into());
        }
    }
    Ok(NewRun {
        metrics: json_field(r.metrics, false, "metrics")?,
        items: json_field(r.items, true, "items")?,
        impact: json_field(r.impact, false, "impact")?,
        run_id: r.run_id,
        automation: r.automation,
        started_at: r.started_at,
        ended_at: r.ended_at,
        status: r.status,
        headline: r.headline,
        report_md: r.report_md.filter(|m| !m.trim().is_empty()),
    })
}

pub async fn create(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let given = headers.get("x-runs-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if !token_ok(std::env::var("RUNS_WRITE_TOKEN").ok().as_deref(), given) {
        return Err(AppError::NotFound);
    }
    let run = validate(&body).map_err(AppError::Validation)?;

    let report_key = match &run.report_md {
        Some(md) => {
            let key = run.report_key();
            s3cache::put_bytes(&key, md.as_bytes(), "text/markdown; charset=utf-8")
                .await
                .map_err(|e| AppError::ExternalApi(format!("S3: {e}")))?;
            Some(key)
        }
        None => None,
    };

    let inserted: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO automation_runs
           (run_id, automation, started_at, ended_at, status, headline, metrics, items, impact, report_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (run_id) DO NOTHING
         RETURNING id",
    )
    .bind(&run.run_id)
    .bind(&run.automation)
    .bind(run.started_at)
    .bind(run.ended_at)
    .bind(&run.status)
    .bind(&run.headline)
    .bind(&run.metrics)
    .bind(&run.items)
    .bind(&run.impact)
    .bind(&report_key)
    .fetch_optional(&pool)
    .await?;

    Ok(match inserted {
        Some((id,)) => (StatusCode::CREATED, Json(json!({ "id": id, "run_id": run.run_id, "created": true }))),
        None => (StatusCode::OK, Json(json!({ "run_id": run.run_id, "created": false }))),
    })
}

/// Lecture : token machine, ou JWT Logto dont le `sub`/email figure dans la liste blanche.
pub fn reader_ok(expected: Option<&str>, given: Option<&str>) -> bool {
    token_ok(expected, given.unwrap_or(""))
}

pub(crate) async fn guard(headers: &HeaderMap) -> Result<(), AppError> {
    let machine = headers.get("x-runs-token").and_then(|v| v.to_str().ok());
    if reader_ok(std::env::var("RUNS_WRITE_TOKEN").ok().as_deref(), machine) {
        return Ok(());
    }
    let Some(jwt) = crate::auth::bearer(headers) else { return Err(AppError::NotFound) };
    let Some(auth) = crate::auth::authenticator() else { return Err(AppError::NotFound) };
    let claims = auth.validate(jwt).await.map_err(|_| AppError::Unauthorized)?;
    if crate::auth::allowed().permits(&claims) { Ok(()) } else { Err(AppError::Unauthorized) }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Group { Hour, Day, Month }

impl Group {
    pub fn parse(s: &str) -> Result<Group, String> {
        match s {
            "hour" => Ok(Group::Hour),
            "day" => Ok(Group::Day),
            "month" => Ok(Group::Month),
            _ => Err("group invalide (hour|day|month)".into()),
        }
    }
    /// Littéral SQL sûr : jamais la valeur reçue.
    pub fn trunc(self) -> &'static str {
        match self { Group::Hour => "hour", Group::Day => "day", Group::Month => "month" }
    }
}

pub fn clamp_limit(l: Option<i64>) -> i64 {
    l.unwrap_or(100).clamp(1, 500)
}

pub(crate) fn check_range(from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> Result<(), String> {
    match (from, to) {
        (Some(f), Some(t)) if f >= t => Err("from doit précéder to".into()),
        _ => Ok(()),
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    automation: Option<String>,
    status: Option<String>,
    limit: Option<i64>,
    group: Option<String>,
}

struct Filters {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    automation: Option<String>,
    status: Option<String>,
}

fn filters(q: &ListQuery) -> Result<Filters, AppError> {
    check_range(q.from, q.to).map_err(AppError::Validation)?;
    if let Some(a) = &q.automation {
        if !valid_ident(a, "-", 64) {
            return Err(AppError::Validation("automation invalide".into()));
        }
    }
    if let Some(s) = &q.status {
        if !STATUSES.contains(&s.as_str()) {
            return Err(AppError::Validation("status invalide".into()));
        }
    }
    Ok(Filters { from: q.from, to: q.to, automation: q.automation.clone(), status: q.status.clone() })
}

const WHERE: &str = "WHERE ($1::timestamptz IS NULL OR started_at >= $1)
      AND ($2::timestamptz IS NULL OR started_at < $2)
      AND ($3::text IS NULL OR automation = $3)
      AND ($4::text IS NULL OR status = $4)";

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
struct RunRow {
    id: i64,
    run_id: String,
    automation: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    status: String,
    headline: String,
    metrics: Value,
    impact: Value,
    has_report: bool,
}

pub async fn list(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    guard(&headers).await?;
    let f = filters(&q)?;
    let sql = format!(
        "SELECT id, run_id, automation, started_at, ended_at, status, headline, metrics, impact,
                (report_key IS NOT NULL) AS has_report
         FROM automation_runs {WHERE} ORDER BY started_at DESC LIMIT $5"
    );
    let rows: Vec<RunRow> = sqlx::query_as(&sql)
        .bind(f.from).bind(f.to).bind(&f.automation).bind(&f.status)
        .bind(clamp_limit(q.limit))
        .fetch_all(&pool)
        .await?;
    Ok(Json(json!({ "runs": rows })))
}

pub async fn summary(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    guard(&headers).await?;
    let f = filters(&q)?;
    let group = Group::parse(q.group.as_deref().unwrap_or("day")).map_err(AppError::Validation)?;
    let sql = format!(
        "SELECT to_char(date_trunc('{g}', started_at AT TIME ZONE 'Europe/Paris'), 'YYYY-MM-DD\"T\"HH24:MI:SS') AS bucket,
                count(*) AS total,
                count(*) FILTER (WHERE status = 'ok') AS ok,
                count(*) FILTER (WHERE status = 'noop') AS noop,
                count(*) FILTER (WHERE status = 'alert') AS alert,
                count(*) FILTER (WHERE status = 'error') AS error
         FROM automation_runs {WHERE} GROUP BY 1 ORDER BY 1 DESC LIMIT 500",
        g = group.trunc()
    );
    let rows: Vec<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(&sql)
        .bind(f.from).bind(f.to).bind(&f.automation).bind(&f.status)
        .fetch_all(&pool)
        .await?;
    let buckets: Vec<Value> = rows
        .into_iter()
        .map(|(bucket, total, ok, noop, alert, error)| {
            json!({ "bucket": bucket, "total": total, "ok": ok, "noop": noop, "alert": alert, "error": error })
        })
        .collect();
    Ok(Json(json!({ "group": group.trunc(), "tz": "Europe/Paris", "buckets": buckets })))
}

pub async fn get_one(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Value>, AppError> {
    guard(&headers).await?;
    let row: Option<(Value,)> = sqlx::query_as(
        "SELECT jsonb_build_object('id', id, 'run_id', run_id, 'automation', automation,
                'started_at', started_at, 'ended_at', ended_at, 'status', status, 'headline', headline,
                'metrics', metrics, 'items', items, 'impact', impact, 'has_report', report_key IS NOT NULL)
         FROM automation_runs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;
    row.map(|(v,)| Json(v)).ok_or(AppError::NotFound)
}

pub async fn report(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    guard(&headers).await?;
    let key: Option<(Option<String>,)> = sqlx::query_as("SELECT report_key FROM automation_runs WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await?;
    let Some((Some(key),)) = key else { return Err(AppError::NotFound) };
    match s3cache::get_raw(&key).await {
        Ok(Some(bytes)) => Ok((
            [
                (axum::http::header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
                (axum::http::header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response()),
        Ok(None) => Err(AppError::NotFound),
        Err(e) => Err(AppError::ExternalApi(format!("S3: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_body() -> Value {
        json!({
            "run_id": "mr-review-watch-2026-10-22T14:05:00Z",
            "automation": "mr-review-watch",
            "started_at": "2026-10-22T14:05:00Z",
            "ended_at": "2026-10-22T14:09:12Z",
            "status": "ok",
            "headline": "2 MR relues, 1 fil fermé",
            "metrics": {"mr_reviewed": 2},
            "items": [{"kind": "mr", "ref": "!1200", "url": "https://gitlab.com/x", "title": "t", "action": "review"}],
            "impact": {"summary": "x"},
            "report_md": "# Rapport\n- ok"
        })
    }
    fn v(b: &Value) -> Result<NewRun, String> { validate(b.to_string().as_bytes()) }

    #[test]
    fn accepte_un_rapport_complet() {
        let r = v(&ok_body()).unwrap();
        assert_eq!(r.status, "ok");
        assert_eq!(r.report_key(), "reports/mr-review-watch/2026-10-22/mr-review-watch-2026-10-22T14:05:00Z.md");
    }

    #[test]
    fn accepte_le_minimum_et_met_les_defauts() {
        let r = v(&json!({"run_id":"a-1","automation":"sentry-watch","started_at":"2026-10-22T09:00:00Z","status":"noop"})).unwrap();
        assert_eq!(r.metrics, json!({}));
        assert_eq!(r.items, json!([]));
        assert!(r.report_md.is_none());
    }

    #[test]
    fn refuse_statut_inconnu() {
        let mut b = ok_body(); b["status"] = json!("fait");
        assert!(v(&b).unwrap_err().contains("status"));
    }

    #[test]
    fn refuse_traversee_de_chemin() {
        for bad in ["../x", "a/b", "a..b", ".caché", ""] {
            let mut b = ok_body(); b["run_id"] = json!(bad);
            assert!(v(&b).is_err(), "run_id {bad:?} accepté");
        }
        for bad in ["MR", "a/b", "a_b", "../x", ""] {
            let mut b = ok_body(); b["automation"] = json!(bad);
            assert!(v(&b).is_err(), "automation {bad:?} accepté");
        }
    }

    #[test]
    fn refuse_champ_inconnu() {
        let mut b = ok_body(); b["token"] = json!("x");
        assert!(v(&b).is_err());
    }

    #[test]
    fn refuse_dates_incoherentes_et_headline_multiligne() {
        let mut b = ok_body(); b["ended_at"] = json!("2026-10-22T14:00:00Z");
        assert!(v(&b).unwrap_err().contains("ended_at"));
        let mut b = ok_body(); b["headline"] = json!("a\nb");
        assert!(v(&b).is_err());
    }

    #[test]
    fn refuse_formes_json_et_rapport_trop_gros() {
        let mut b = ok_body(); b["items"] = json!({"a": 1});
        assert!(v(&b).unwrap_err().contains("items"));
        let mut b = ok_body(); b["metrics"] = json!([1]);
        assert!(v(&b).unwrap_err().contains("metrics"));
        let mut b = ok_body(); b["report_md"] = json!("x".repeat(MAX_REPORT + 1));
        assert!(v(&b).unwrap_err().contains("report_md"));
    }

    #[test]
    fn rapport_vide_equivaut_a_aucun_rapport() {
        let mut b = ok_body(); b["report_md"] = json!("  \n");
        assert!(v(&b).unwrap().report_md.is_none());
    }

    #[test]
    fn group_n_accepte_que_hour_day_month() {
        assert_eq!(Group::parse("hour").unwrap().trunc(), "hour");
        assert_eq!(Group::parse("day").unwrap().trunc(), "day");
        assert_eq!(Group::parse("month").unwrap().trunc(), "month");
        for bad in ["", "year", "day; drop table automation_runs", "HOUR"] {
            assert!(Group::parse(bad).is_err(), "{bad:?} accepté");
        }
    }

    #[test]
    fn limite_bornee() {
        assert_eq!(clamp_limit(None), 100);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(50)), 50);
        assert_eq!(clamp_limit(Some(100_000)), 500);
    }

    #[test]
    fn plage_incoherente_refusee() {
        let a: DateTime<Utc> = "2026-10-22T00:00:00Z".parse().unwrap();
        let b: DateTime<Utc> = "2026-10-23T00:00:00Z".parse().unwrap();
        assert!(check_range(Some(a), Some(b)).is_ok());
        assert!(check_range(Some(b), Some(a)).is_err());
        assert!(check_range(None, Some(a)).is_ok());
    }

    #[test]
    fn lecture_refusee_sans_token_valide() {
        let t = "0123456789abcdef0123";
        assert!(reader_ok(Some(t), Some(t)));
        assert!(!reader_ok(Some(t), None));
        assert!(!reader_ok(None, Some("")));
        assert!(!reader_ok(None, None));
    }

    #[test]
    fn token_echoue_ferme() {
        assert!(!token_ok(None, ""));
        assert!(!token_ok(Some(""), ""));
        assert!(!token_ok(Some("court"), "court"));
        let t = "0123456789abcdef0123";
        assert!(token_ok(Some(t), t));
        assert!(!token_ok(Some(t), "0123456789abcdef0124"));
        assert!(!token_ok(Some(t), ""));
    }
}
