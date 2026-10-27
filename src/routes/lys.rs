//! lys-en-france : sous-routeur `/lys`, protégé par un JWT Logto de l'audience dédiée + liste blanche de `sub`.
//! Le garde est un `route_layer` : toute route ajoutée ici est protégée d'office. Inscription Logto ouverte
//! côté tenant : la liste blanche est la seule barrière, d'où le 403 distinct d'un simple 401.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderValue, Method},
    middleware::{from_fn_with_state, Next},
    response::Response,
    routing::{delete, get, patch, post, put},
    Extension, Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, CorsLayer};

use crate::auth::{self, Allowed, Authenticator, Claims};
use crate::error::AppError;
use crate::routes::lys_docs::{self, DocStore, DocumentOut};

const MAX_BODY: usize = 16 * 1024;
const MAX_NOTE_CHARS: usize = 2000;
const STATUSES: [&str; 4] = ["todo", "doing", "done", "blocked"];

#[derive(Clone)]
struct LysAuth {
    authenticator: Option<Arc<Authenticator>>,
    allowed: Arc<Allowed>,
}

/// Routeur de production : audience et liste blanche lues dans l'environnement.
pub fn router() -> Router<PgPool> {
    router_with_store(auth::authenticator_lys(), auth::allowed_lys(), DocStore::from_env())
}

#[cfg(test)]
fn router_with(authenticator: Option<Arc<Authenticator>>, allowed: Arc<Allowed>) -> Router<PgPool> {
    router_with_store(authenticator, allowed, DocStore::disabled())
}

fn router_with_store(authenticator: Option<Arc<Authenticator>>, allowed: Arc<Allowed>, store: DocStore) -> Router<PgPool> {
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("https://lys-en-france.chetana.fr"),
            HeaderValue::from_static("http://localhost:5173"),
        ])
        .allow_methods([Method::GET, Method::PUT, Method::PATCH, Method::POST, Method::DELETE])
        .allow_headers(AllowHeaders::list([header::AUTHORIZATION, header::CONTENT_TYPE]));
    Router::new()
        .route("/lys/me", get(me))
        .route("/lys/plan", get(plan))
        .route("/lys/items/{slug}/done", put(put_item_done))
        .route("/lys/tasks/{slug}", patch(patch_task))
        .route("/lys/documents/{id}/url", get(lys_docs::read_url))
        .route("/lys/documents/{id}", delete(lys_docs::delete))
        // Limite de corps relevée sur cette seule route : la couche interne l'emporte sur les 16 Kio ci-dessous.
        .route(
            "/lys/tasks/{slug}/documents",
            post(lys_docs::upload).layer(DefaultBodyLimit::max(lys_docs::MAX_UPLOAD)),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .layer(Extension(store))
        .route_layer(from_fn_with_state(LysAuth { authenticator, allowed }, guard))
        // CORS à l'extérieur du garde : un préflight n'a pas d'Authorization.
        .layer(cors)
}

async fn guard(State(a): State<LysAuth>, mut req: Request, next: Next) -> Result<Response, AppError> {
    let token = auth::bearer(req.headers()).ok_or(AppError::Unauthorized)?;
    let authenticator = a.authenticator.as_ref().ok_or(AppError::Unauthorized)?;
    let claims = authenticator.validate(token).await.map_err(|e| {
        tracing::debug!("lys: jeton refusé: {e}");
        AppError::Unauthorized
    })?;
    if !a.allowed.permits(&claims) {
        tracing::warn!("lys: sub hors liste blanche: {}", claims.sub);
        return Err(AppError::Forbidden);
    }
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

async fn me(Extension(claims): Extension<Claims>) -> Json<Value> {
    Json(json!({ "sub": claims.sub }))
}

// ── Modèle de lecture ────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
struct ItemRow {
    #[serde(skip)]
    task_slug: String,
    slug: String,
    kind: String,
    position: i32,
    label_fr: String,
    label_km: Option<String>,
    done: bool,
    done_at: Option<DateTime<Utc>>,
    done_by: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
struct TaskRow {
    slug: String,
    category_slug: String,
    position: i32,
    title_fr: String,
    title_km: Option<String>,
    help_fr: Option<String>,
    help_km: Option<String>,
    status: String,
    note: String,
    due_date: Option<NaiveDate>,
    due_override: Option<NaiveDate>,
    legal_deadline: Option<NaiveDate>,
    done_at: Option<DateTime<Utc>>,
    km_status: String,
}

#[derive(Debug, Clone, FromRow)]
struct CategoryRow {
    slug: String,
    position: i32,
    title_fr: String,
    title_km: Option<String>,
}

#[derive(Debug, Serialize)]
struct TaskOut {
    #[serde(flatten)]
    row: TaskRow,
    effective_due: Option<NaiveDate>,
    items_done: usize,
    items_total: usize,
    items: Vec<ItemRow>,
    documents: Vec<DocumentOut>,
}

fn task_out(row: TaskRow, items: Vec<ItemRow>, documents: Vec<DocumentOut>) -> TaskOut {
    TaskOut {
        effective_due: row.due_override.or(row.due_date),
        items_done: items.iter().filter(|i| i.done).count(),
        items_total: items.len(),
        row,
        items,
        documents,
    }
}

const TASK_COLS: &str = "slug, category_slug, position, title_fr, title_km, help_fr, help_km, status, note, \
     due_date, due_override, legal_deadline, done_at, km_status";
const ITEM_COLS: &str = "task_slug, slug, kind, position, label_fr, label_km, done, done_at, done_by";

/// Prochaine étape : première tâche ni terminée ni bloquée, par échéance effective
/// (`due_override` sinon `due_date`, sans échéance en dernier), puis ordre de la catégorie, puis de la tâche.
fn pick_next<'a>(tasks: &'a [TaskOut], cats: &[CategoryRow]) -> Option<&'a TaskOut> {
    let cat_pos = |slug: &str| cats.iter().find(|c| c.slug == slug).map_or(i32::MAX, |c| c.position);
    tasks
        .iter()
        .filter(|t| t.row.status != "done" && t.row.status != "blocked")
        .min_by_key(|t| (t.effective_due.is_none(), t.effective_due, cat_pos(&t.row.category_slug), t.row.position))
}

fn build_plan(cats: Vec<CategoryRow>, tasks: Vec<TaskRow>, items: Vec<ItemRow>, docs: Vec<DocumentOut>) -> Value {
    let outs: Vec<TaskOut> = tasks
        .into_iter()
        .map(|row| {
            let its = items.iter().filter(|i| i.task_slug == row.slug).cloned().collect();
            let ds = docs.iter().filter(|d| d.task_slug == row.slug).cloned().collect();
            task_out(row, its, ds)
        })
        .collect();
    let next = pick_next(&outs, &cats).map(|t| {
        json!({
            "slug": t.row.slug,
            "category_slug": t.row.category_slug,
            "title_fr": t.row.title_fr,
            "title_km": t.row.title_km,
            "status": t.row.status,
            "effective_due": t.effective_due,
            "legal_deadline": t.row.legal_deadline,
        })
    });
    let tasks_done = |ts: &[&TaskOut]| ts.iter().filter(|t| t.row.status == "done").count();
    let all: Vec<&TaskOut> = outs.iter().collect();
    let totals = json!({
        "tasks_done": tasks_done(&all),
        "tasks_total": all.len(),
        "items_done": all.iter().map(|t| t.items_done).sum::<usize>(),
        "items_total": all.iter().map(|t| t.items_total).sum::<usize>(),
    });
    let categories: Vec<Value> = cats
        .iter()
        .map(|c| {
            let ts: Vec<&TaskOut> = outs.iter().filter(|t| t.row.category_slug == c.slug).collect();
            json!({
                "slug": c.slug,
                "position": c.position,
                "title_fr": c.title_fr,
                "title_km": c.title_km,
                "tasks_done": tasks_done(&ts),
                "tasks_total": ts.len(),
                "items_done": ts.iter().map(|t| t.items_done).sum::<usize>(),
                "items_total": ts.iter().map(|t| t.items_total).sum::<usize>(),
                "tasks": ts,
            })
        })
        .collect();
    json!({ "totals": totals, "next": next, "categories": categories })
}

async fn plan(State(pool): State<PgPool>) -> Result<Json<Value>, AppError> {
    // Un seul instantané : les trois lectures partagent la même transaction.
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
    let cats: Vec<CategoryRow> =
        sqlx::query_as("SELECT slug, position, title_fr, title_km FROM lys_fr.category ORDER BY position, slug")
            .fetch_all(&mut *tx)
            .await?;
    let tasks: Vec<TaskRow> = sqlx::query_as(&format!("SELECT {TASK_COLS} FROM lys_fr.task ORDER BY position, slug"))
        .fetch_all(&mut *tx)
        .await?;
    let items: Vec<ItemRow> = sqlx::query_as(&format!("SELECT {ITEM_COLS} FROM lys_fr.item ORDER BY position, slug"))
        .fetch_all(&mut *tx)
        .await?;
    let docs = lys_docs::list_active(&mut tx, None).await?;
    tx.commit().await?;
    Ok(Json(build_plan(cats, tasks, items, docs)))
}

async fn load_task(tx: &mut Transaction<'_, Postgres>, slug: &str) -> Result<TaskOut, AppError> {
    let row: TaskRow = sqlx::query_as(&format!("SELECT {TASK_COLS} FROM lys_fr.task WHERE slug = $1"))
        .bind(slug)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let items: Vec<ItemRow> =
        sqlx::query_as(&format!("SELECT {ITEM_COLS} FROM lys_fr.item WHERE task_slug = $1 ORDER BY position, slug"))
            .bind(slug)
            .fetch_all(&mut **tx)
            .await?;
    let docs = lys_docs::list_active(tx, Some(slug)).await?;
    Ok(task_out(row, items, docs))
}

// ── Corps de requête ─────────────────────────────────────────────────────────

/// JSON mal formé → 400 ; JSON valide mais champ inconnu, manquant ou de mauvais type → 422.
fn parse_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, AppError> {
    serde_json::from_slice(body).map_err(|e| {
        if e.is_data() {
            AppError::Unprocessable(format!("corps invalide: {e}"))
        } else {
            AppError::Validation(format!("JSON invalide: {e}"))
        }
    })
}

/// Distingue `"due_override": null` (effacer) d'un champ absent (ne pas toucher).
fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DoneBody {
    done: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskPatch {
    status: Option<String>,
    note: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    due_override: Option<Option<NaiveDate>>,
}

fn validate_note(note: &str) -> Result<(), AppError> {
    if note.chars().count() > MAX_NOTE_CHARS {
        return Err(AppError::Validation(format!("note trop longue (max {MAX_NOTE_CHARS} caractères)")));
    }
    if note.chars().any(|c| c.is_control() && c != '\n') {
        return Err(AppError::Validation("note: caractères de contrôle interdits (sauf retour à la ligne)".into()));
    }
    Ok(())
}

// ── Statut de tâche ──────────────────────────────────────────────────────────

/// Statut après un cochage. Règle : toutes les cases cochées → `done`. Une tâche `done` dont on décoche
/// une case redescend (`doing` s'il reste des cases cochées, sinon `todo`). Une tâche `todo` dont une case
/// est cochée passe `doing`, et redevient `todo` quand on décoche la dernière case cochée. Tout le reste est
/// conservé : un `blocked` posé à la main ne bouge jamais (sauf toutes cases cochées), et cocher une case sur
/// une tâche déclarée `done` à la main ne la rouvre pas.
fn status_after_toggle(current: &str, done: i64, total: i64, unchecked: bool) -> &'static str {
    if total > 0 && done == total {
        return "done";
    }
    match current {
        "done" if unchecked => if done > 0 { "doing" } else { "todo" },
        "done" => "done",
        "todo" if done > 0 => "doing",
        "doing" if unchecked && done == 0 => "todo",
        "doing" => "doing",
        "blocked" => "blocked",
        _ => "todo",
    }
}

async fn put_item_done(
    State(pool): State<PgPool>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let DoneBody { done } = parse_body(&body)?;
    let mut tx = pool.begin().await?;
    let task_slug: String = sqlx::query_scalar("SELECT task_slug FROM lys_fr.item WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    // Verrou de la tâche : les cochages concurrents sur ses cases sont sérialisés, le décompte reste juste.
    let current: String = sqlx::query_scalar("SELECT status FROM lys_fr.task WHERE slug = $1 FOR UPDATE")
        .bind(&task_slug)
        .fetch_one(&mut *tx)
        .await?;
    let changed = sqlx::query(
        "UPDATE lys_fr.item SET done = $2, done_at = CASE WHEN $2 THEN now() END, done_by = CASE WHEN $2 THEN $3 END \
         WHERE slug = $1 AND done IS DISTINCT FROM $2",
    )
    .bind(&slug)
    .bind(done)
    .bind(&claims.sub)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    let (done_count, total): (i64, i64) =
        sqlx::query_as("SELECT count(*) FILTER (WHERE done), count(*) FROM lys_fr.item WHERE task_slug = $1")
            .bind(&task_slug)
            .fetch_one(&mut *tx)
            .await?;
    let next = status_after_toggle(&current, done_count, total, changed && !done);
    if next != current {
        sqlx::query("UPDATE lys_fr.task SET status = $2, done_at = CASE WHEN $2 = 'done' THEN now() END, updated_at = now() WHERE slug = $1")
            .bind(&task_slug)
            .bind(next)
            .execute(&mut *tx)
            .await?;
    }
    let task = load_task(&mut tx, &task_slug).await?;
    tx.commit().await?;
    let item = task.items.iter().find(|i| i.slug == slug).cloned();
    Ok(Json(json!({ "item": item, "task": task })))
}

async fn patch_task(State(pool): State<PgPool>, Path(slug): Path<String>, body: Bytes) -> Result<Json<Value>, AppError> {
    let patch: TaskPatch = parse_body(&body)?;
    if patch.status.is_none() && patch.note.is_none() && patch.due_override.is_none() {
        return Err(AppError::Validation("aucun champ à modifier (status, note, due_override)".into()));
    }
    if let Some(s) = &patch.status {
        if !STATUSES.contains(&s.as_str()) {
            return Err(AppError::Validation(format!("status invalide (attendu: {})", STATUSES.join(", "))));
        }
    }
    if let Some(n) = &patch.note {
        validate_note(n)?;
    }
    let mut tx = pool.begin().await?;
    let (status, note, due_override): (String, String, Option<NaiveDate>) =
        sqlx::query_as("SELECT status, note, due_override FROM lys_fr.task WHERE slug = $1 FOR UPDATE")
            .bind(&slug)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(AppError::NotFound)?;
    let new_status = patch.status.unwrap_or_else(|| status.clone());
    let new_note = patch.note.unwrap_or(note);
    let new_due = patch.due_override.unwrap_or(due_override);
    sqlx::query(
        "UPDATE lys_fr.task SET status = $2, note = $3, due_override = $4, updated_at = now(), \
         done_at = CASE WHEN $2 <> 'done' THEN NULL WHEN status = 'done' THEN done_at ELSE now() END \
         WHERE slug = $1",
    )
    .bind(&slug)
    .bind(&new_status)
    .bind(&new_note)
    .bind(new_due)
    .execute(&mut *tx)
    .await?;
    let task = load_task(&mut tx, &slug).await?;
    tx.commit().await?;
    Ok(Json(json!({ "task": task })))
}

#[cfg(test)]
mod tests {
    use crate::auth::{Allowed, Authenticator, KeyMap};
    use axum::http::StatusCode;
    use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header};
    use serde_json::{json, Value};
    use sqlx::PgPool;
    use std::sync::Arc;

    const ISS: &str = "https://auth.chetana.fr/oidc";
    const AUD: &str = "https://lys-en-france.chetana.fr/api";
    const AUD_DASHBOARD: &str = "https://job-dashboard.chetana.fr/api";
    const SECRET: &[u8] = b"secret-de-test-pour-hs256-uniquement";

    fn exp(delta: i64) -> i64 { chrono::Utc::now().timestamp() + delta }
    fn claims(sub: &str) -> Value { json!({"iss": ISS, "aud": AUD, "sub": sub, "exp": exp(600)}) }
    fn token(c: Value) -> String {
        let mut h = Header::new(Algorithm::HS256);
        h.kid = Some("k1".into());
        encode(&h, &c, &EncodingKey::from_secret(SECRET)).unwrap()
    }

    /// Serveur réel sur un port local, sans réseau externe : clé de test, pool paresseux jamais sollicité.
    async fn serve(subs: &str) -> String {
        let mut keys = KeyMap::new();
        keys.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
        let auth = Some(Arc::new(Authenticator::with_static_keys(ISS, AUD, keys)));
        let pool = PgPool::connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
        let app = super::router_with(auth, Arc::new(Allowed::new(subs, ""))).with_state(pool);
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        format!("http://{addr}")
    }

    async fn get_me(base: &str, bearer: Option<&str>) -> (StatusCode, Value) {
        let mut r = reqwest::Client::new().get(format!("{base}/lys/me"));
        if let Some(t) = bearer { r = r.bearer_auth(t); }
        let resp = r.send().await.unwrap();
        let st = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        (st, resp.json().await.unwrap_or(Value::Null))
    }

    #[tokio::test]
    async fn me_renvoie_le_sub_autorise() {
        let base = serve("lys1,lys2").await;
        let (st, body) = get_me(&base, Some(&token(claims("lys1")))).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body, json!({"sub": "lys1"}));
    }

    #[tokio::test]
    async fn sans_jeton_401() {
        let base = serve("lys1").await;
        assert_eq!(get_me(&base, None).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn mauvais_iss_aud_exp_401() {
        let base = serve("lys1").await;
        let mut c = claims("lys1"); c["iss"] = json!("https://evil.example/oidc");
        assert_eq!(get_me(&base, Some(&token(c))).await.0, StatusCode::UNAUTHORIZED);
        let mut c = claims("lys1"); c["aud"] = json!("autre-api");
        assert_eq!(get_me(&base, Some(&token(c))).await.0, StatusCode::UNAUTHORIZED);
        let mut c = claims("lys1"); c["exp"] = json!(exp(-3600));
        assert_eq!(get_me(&base, Some(&token(c))).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn alg_none_401() {
        use base64::Engine;
        let b = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let t = format!("{}.{}.", b.encode(r#"{"alg":"none","kid":"k1"}"#), b.encode(claims("lys1").to_string()));
        let base = serve("lys1").await;
        assert_eq!(get_me(&base, Some(&t)).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn jeton_de_l_audience_job_dashboard_401() {
        let base = serve("lys1").await;
        let mut c = claims("lys1"); c["aud"] = json!(AUD_DASHBOARD);
        assert_eq!(get_me(&base, Some(&token(c))).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sub_hors_liste_403() {
        let base = serve("lys1").await;
        let (st, body) = get_me(&base, Some(&token(claims("intrus")))).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({"error": "Forbidden"}));
    }

    #[tokio::test]
    async fn liste_vide_refuse_meme_un_jeton_valide() {
        let base = serve("").await;
        assert_eq!(get_me(&base, Some(&token(claims("lys1")))).await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn audience_non_configuree_refuse() {
        let pool = PgPool::connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
        let app = super::router_with(None, Arc::new(Allowed::new("lys1", ""))).with_state(pool);
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        let (st, _) = get_me(&format!("http://{addr}"), Some(&token(claims("lys1")))).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn preflight_cors_sans_jeton_seulement_pour_l_origine_lys() {
        let base = serve("lys1").await;
        let c = reqwest::Client::new();
        let pre = |origin: &'static str| c
            .request(reqwest::Method::OPTIONS, format!("{base}/lys/me"))
            .header("origin", origin)
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "authorization")
            .send();
        let ok = pre("https://lys-en-france.chetana.fr").await.unwrap();
        assert!(ok.status().is_success());
        assert_eq!(ok.headers().get("access-control-allow-origin").unwrap(), "https://lys-en-france.chetana.fr");
        let ko = pre("https://job-dashboard.chetana.fr").await.unwrap();
        assert!(ko.headers().get("access-control-allow-origin").is_none());
    }

    // ── Règles pures ────────────────────────────────────────────────────────

    #[test]
    fn statut_apres_cochage() {
        use super::status_after_toggle as f;
        assert_eq!(f("todo", 1, 3, false), "doing");
        assert_eq!(f("todo", 3, 3, false), "done");
        assert_eq!(f("doing", 3, 3, false), "done");
        assert_eq!(f("blocked", 3, 3, false), "done");
        assert_eq!(f("done", 2, 3, true), "doing");
        assert_eq!(f("done", 0, 3, true), "todo");
        assert_eq!(f("done", 2, 3, false), "done", "done posé à la main non rouvert par un cochage");
        assert_eq!(f("blocked", 1, 3, false), "blocked");
        assert_eq!(f("doing", 0, 3, true), "todo");
        assert_eq!(f("doing", 0, 3, false), "doing", "doing posé à la main conservé");
        assert_eq!(f("doing", 1, 3, true), "doing");
        assert_eq!(f("todo", 0, 0, false), "todo");
    }

    // ── Intégration : Postgres jetable (LYS_TEST_DATABASE_URL), schéma et seed réels ──
    // Variable absente : les tests sont ignorés (affichés comme tels), `cargo test` reste vert sans Docker.

    use std::sync::atomic::{AtomicU32, Ordering};

    static DB_SEQ: AtomicU32 = AtomicU32::new(0);

    async fn fresh_pool() -> Option<PgPool> {
        let admin = std::env::var("LYS_TEST_DATABASE_URL").ok()?;
        let name = format!("lys_t_{}_{}", std::process::id(), DB_SEQ.fetch_add(1, Ordering::SeqCst));
        let root = PgPool::connect(&admin).await.unwrap();
        sqlx::query(&format!("CREATE DATABASE {name}")).execute(&root).await.unwrap();
        let url = match admin.rsplit_once('/') {
            Some((base, _)) => format!("{base}/{name}"),
            None => unreachable!(),
        };
        let pool = sqlx::postgres::PgPoolOptions::new().max_connections(10).connect(&url).await.unwrap();
        let root_dir = env!("CARGO_MANIFEST_DIR");
        for f in ["migrations/007_lys_en_france.sql", "migrations/seed/lys_fr_seed.sql"] {
            let sql = std::fs::read_to_string(format!("{root_dir}/{f}")).unwrap();
            sqlx::raw_sql(&sql).execute(&pool).await.unwrap();
        }
        Some(pool)
    }

    struct Api {
        base: String,
        http: reqwest::Client,
        pool: PgPool,
    }

    async fn api() -> Option<Api> {
        let pool = fresh_pool().await?;
        let mut keys = KeyMap::new();
        keys.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
        let auth = Some(Arc::new(Authenticator::with_static_keys(ISS, AUD, keys)));
        let app = super::router_with(auth, Arc::new(Allowed::new("lys1,lys2", ""))).with_state(pool.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = l.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        Some(Api { base: format!("http://{addr}"), http: reqwest::Client::new(), pool })
    }

    impl Api {
        async fn call(&self, m: reqwest::Method, path: &str, sub: Option<&str>, body: Option<&str>) -> (StatusCode, Value) {
            let mut r = self.http.request(m, format!("{}{path}", self.base));
            if let Some(s) = sub {
                r = r.bearer_auth(token(claims(s)));
            }
            if let Some(b) = body {
                r = r.header("content-type", "application/json").body(b.to_string());
            }
            let resp = r.send().await.unwrap();
            let st = StatusCode::from_u16(resp.status().as_u16()).unwrap();
            (st, resp.json().await.unwrap_or(Value::Null))
        }
        async fn plan(&self) -> Value {
            let (st, v) = self.call(reqwest::Method::GET, "/lys/plan", Some("lys1"), None).await;
            assert_eq!(st, StatusCode::OK);
            v
        }
        async fn done(&self, item: &str, done: bool, sub: &str) -> (StatusCode, Value) {
            self.call(reqwest::Method::PUT, &format!("/lys/items/{item}/done"), Some(sub), Some(&format!(r#"{{"done":{done}}}"#))).await
        }
        async fn patch(&self, task: &str, body: &str) -> (StatusCode, Value) {
            self.call(reqwest::Method::PATCH, &format!("/lys/tasks/{task}"), Some("lys1"), Some(body)).await
        }
        async fn status_of(&self, task: &str) -> String {
            sqlx::query_scalar("SELECT status FROM lys_fr.task WHERE slug = $1").bind(task).fetch_one(&self.pool).await.unwrap()
        }
    }

    macro_rules! db_test {
        ($name:ident, |$a:ident| $body:block) => {
            #[tokio::test]
            async fn $name() {
                let Some($a) = api().await else {
                    eprintln!("ignoré : LYS_TEST_DATABASE_URL absente");
                    return;
                };
                $body
            }
        };
    }

    db_test!(plan_complet_ordonne_avec_compteurs, |a| {
        let p = a.plan().await;
        let cats = p["categories"].as_array().unwrap();
        let positions: Vec<i64> = cats.iter().map(|c| c["position"].as_i64().unwrap()).collect();
        let mut sorted = positions.clone();
        sorted.sort();
        assert_eq!(positions, sorted);
        let n_tasks: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.task").fetch_one(&a.pool).await.unwrap();
        let n_items: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.item").fetch_one(&a.pool).await.unwrap();
        assert_eq!(p["totals"]["tasks_total"], n_tasks);
        assert_eq!(p["totals"]["items_total"], n_items);
        assert_eq!(p["totals"]["items_done"], 0);
        let sejour = &cats[0];
        assert_eq!(sejour["slug"], "sejour");
        let t = &sejour["tasks"][0];
        assert_eq!(t["slug"], "vls-ts-anef");
        assert!(t["title_km"].is_null() && t["help_km"].is_null(), "KM absent = null");
        assert_eq!(t["status"], "todo");
        assert_eq!(t["items"][0]["slug"], "vls-ts-anef.1");
        assert_eq!(t["items"][0]["done"], false);
        assert!(t["items"][0]["done_by"].is_null());
        assert_eq!(t["items_total"], t["items"].as_array().unwrap().len());
    });

    db_test!(next_est_la_tache_a_l_echeance_la_plus_proche, |a| {
        let expected: String = sqlx::query_scalar(
            "SELECT t.slug FROM lys_fr.task t JOIN lys_fr.category c ON c.slug = t.category_slug \
             ORDER BY coalesce(t.due_override, t.due_date) NULLS LAST, c.position, t.position LIMIT 1",
        ).fetch_one(&a.pool).await.unwrap();
        let p = a.plan().await;
        assert_eq!(p["next"]["slug"], expected.as_str());
        a.patch(&expected, r#"{"status":"done"}"#).await;
        assert_ne!(a.plan().await["next"]["slug"], expected.as_str());
        // Terminée → la suivante prend la place ; bloquée → ignorée.
        a.patch("vls-ts-anef", r#"{"status":"done"}"#).await;
        let p = a.plan().await;
        assert_ne!(p["next"]["slug"], "vls-ts-anef");
        a.patch("disney-noel", r#"{"status":"blocked"}"#).await;
        let p = a.plan().await;
        assert_ne!(p["next"]["slug"], "disney-noel");
        // Une échéance modifiée repasse devant.
        a.patch("cv", r#"{"due_override":"2026-01-01"}"#).await;
        assert_eq!(a.plan().await["next"]["slug"], "cv");
    });

    db_test!(cocher_pose_done_at_done_by_et_statut_doing, |a| {
        let (st, v) = a.done("vls-ts-anef.1", true, "lys2").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["item"]["done"], true);
        assert_eq!(v["item"]["done_by"], "lys2");
        assert!(v["item"]["done_at"].is_string());
        assert_eq!(v["task"]["status"], "doing");
        assert_eq!(v["task"]["items_done"], 1);
        let p = a.plan().await;
        assert_eq!(p["totals"]["items_done"], 1);
        assert_eq!(p["categories"][0]["items_done"], 1);
    });

    db_test!(cocher_deux_fois_est_idempotent, |a| {
        let (_, v1) = a.done("cv.1", true, "lys1").await;
        let (st, v2) = a.done("cv.1", true, "lys2").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v1["item"]["done_at"], v2["item"]["done_at"], "done_at conservé");
        assert_eq!(v2["item"]["done_by"], "lys1", "done_by conservé");
        let (_, d1) = a.done("cv.1", false, "lys1").await;
        let (st, d2) = a.done("cv.1", false, "lys1").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(d1["item"], d2["item"]);
        assert!(d2["item"]["done_at"].is_null() && d2["item"]["done_by"].is_null());
    });

    db_test!(toutes_cases_cochees_termine_la_tache_puis_decocher_la_rouvre, |a| {
        let items: Vec<String> = sqlx::query_scalar("SELECT slug FROM lys_fr.item WHERE task_slug = 'ofii-cir'")
            .fetch_all(&a.pool).await.unwrap();
        for i in &items {
            a.done(i, true, "lys1").await;
        }
        assert_eq!(a.status_of("ofii-cir").await, "done");
        let done_at: Option<chrono::DateTime<chrono::Utc>> =
            sqlx::query_scalar("SELECT done_at FROM lys_fr.task WHERE slug = 'ofii-cir'").fetch_one(&a.pool).await.unwrap();
        assert!(done_at.is_some());
        a.done(&items[0], false, "lys1").await;
        assert_eq!(a.status_of("ofii-cir").await, "doing");
        for i in &items {
            a.done(i, false, "lys1").await;
        }
        assert_eq!(a.status_of("ofii-cir").await, "todo");
    });

    db_test!(statut_pose_a_la_main_n_est_pas_ecrase, |a| {
        a.patch("mutuelle", r#"{"status":"blocked"}"#).await;
        a.done("mutuelle.1", true, "lys1").await;
        assert_eq!(a.status_of("mutuelle").await, "blocked");
        a.patch("banque-rdv", r#"{"status":"done"}"#).await;
        a.done("banque-rdv.1", true, "lys1").await;
        assert_eq!(a.status_of("banque-rdv").await, "done");
    });

    db_test!(patch_note_statut_echeance, |a| {
        let (st, v) = a.patch("cv", r#"{"status":"doing","note":"ligne 1\nligne 2","due_override":"2026-11-20"}"#).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(v["task"]["status"], "doing");
        assert_eq!(v["task"]["note"], "ligne 1\nligne 2");
        assert_eq!(v["task"]["due_override"], "2026-11-20");
        assert_eq!(v["task"]["effective_due"], "2026-11-20");
        // null efface l'échéance ; champ absent ne touche pas le reste.
        let (_, v) = a.patch("cv", r#"{"due_override":null}"#).await;
        assert!(v["task"]["due_override"].is_null());
        assert_eq!(v["task"]["note"], "ligne 1\nligne 2");
        // done pose done_at, un autre statut l'efface.
        let (_, v) = a.patch("cv", r#"{"status":"done"}"#).await;
        assert!(v["task"]["done_at"].is_string());
        let (_, v) = a.patch("cv", r#"{"status":"todo"}"#).await;
        assert!(v["task"]["done_at"].is_null());
        let exact = "é".repeat(2000);
        let (st, _) = a.patch("cv", &format!(r#"{{"note":"{exact}"}}"#)).await;
        assert_eq!(st, StatusCode::OK, "2000 caractères (pas octets) acceptés");
    });

    db_test!(patch_refus_400_422_404, |a| {
        let long = "x".repeat(2001);
        assert_eq!(a.patch("cv", &format!(r#"{{"note":"{long}"}}"#)).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(a.patch("cv", r#"{"note":"a\u0000b"}"#).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(a.patch("cv", r#"{"note":"a\tb"}"#).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(a.patch("cv", r#"{"status":"fini"}"#).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(a.patch("cv", r#"{}"#).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(a.patch("cv", r#"{"note":"#).await.0, StatusCode::BAD_REQUEST);
        let (st, body) = a.patch("cv", r#"{"note":"ok","titre":"x"}"#).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body["error"].is_string());
        assert_eq!(a.patch("cv", r#"{"due_override":"demain"}"#).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(a.patch("cv", r#"{"status":3}"#).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(a.patch("inconnue", r#"{"note":"x"}"#).await.0, StatusCode::NOT_FOUND);
        let note: String = sqlx::query_scalar("SELECT note FROM lys_fr.task WHERE slug='cv'").fetch_one(&a.pool).await.unwrap();
        assert_eq!(note, "", "aucun refus n'a écrit");
    });

    db_test!(put_refus_422_404, |a| {
        assert_eq!(a.done("cv.9", true, "lys1").await.0, StatusCode::NOT_FOUND);
        let (st, _) = a.call(reqwest::Method::PUT, "/lys/items/cv.1/done", Some("lys1"), Some(r#"{"done":"oui"}"#)).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
        let (st, _) = a.call(reqwest::Method::PUT, "/lys/items/cv.1/done", Some("lys1"), Some(r#"{"done":true,"x":1}"#)).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
        let (st, _) = a.call(reqwest::Method::PUT, "/lys/items/cv.1/done", Some("lys1"), Some(r#"{}"#)).await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    });

    db_test!(auth_401_403_sur_toutes_les_routes, |a| {
        use reqwest::Method as M;
        let routes = [
            (M::GET, "/lys/plan", None),
            (M::PUT, "/lys/items/cv.1/done", Some(r#"{"done":true}"#)),
            (M::PATCH, "/lys/tasks/cv", Some(r#"{"note":"x"}"#)),
        ];
        for (m, path, body) in routes {
            assert_eq!(a.call(m.clone(), path, None, body).await.0, StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(a.call(m, path, Some("intrus"), body).await.0, StatusCode::FORBIDDEN, "{path}");
        }
        let faits: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.item WHERE done").fetch_one(&a.pool).await.unwrap();
        let note: String = sqlx::query_scalar("SELECT note FROM lys_fr.task WHERE slug='cv'").fetch_one(&a.pool).await.unwrap();
        assert_eq!((faits, note.as_str()), (0, ""), "un sub hors liste n'écrit rien");
    });

    db_test!(cochages_concurrents_restent_coherents, |a| {
        let items: Vec<String> = sqlx::query_scalar("SELECT slug FROM lys_fr.item WHERE task_slug = 'vls-ts-anef'")
            .fetch_all(&a.pool).await.unwrap();
        let a = Arc::new(a);
        let mut hs = Vec::new();
        for i in items.iter().chain(items.iter()) {
            let (a, i) = (a.clone(), i.clone());
            hs.push(tokio::spawn(async move { a.done(&i, true, "lys1").await.0 }));
        }
        for h in hs {
            assert_eq!(h.await.unwrap(), StatusCode::OK);
        }
        assert_eq!(a.status_of("vls-ts-anef").await, "done");
        // Mélange cocher/décocher du même élément : jamais d'erreur, état final cohérent avec le décompte.
        let mut hs = Vec::new();
        for n in 0..12 {
            let a = a.clone();
            hs.push(tokio::spawn(async move { a.done("ofii-cir.1", n % 2 == 0, "lys2").await.0 }));
        }
        for h in hs {
            assert_eq!(h.await.unwrap(), StatusCode::OK);
        }
        let (done, st): (bool, String) = sqlx::query_as(
            "SELECT (SELECT done FROM lys_fr.item WHERE slug='ofii-cir.1'), (SELECT status FROM lys_fr.task WHERE slug='ofii-cir')",
        ).fetch_one(&a.pool).await.unwrap();
        assert_eq!(st, if done { "doing" } else { "todo" });
    });

    mod docs_it;
}
