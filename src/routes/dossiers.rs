//! dossiers : sous-routeur `/dossiers`, protégé par un JWT Logto de l'audience dédiée + liste blanche de `sub`.
//! Le garde est un `route_layer` : toute route ajoutée ici est protégée d'office. Même modèle que `/lys`.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{header, HeaderValue, Method},
    middleware::{from_fn_with_state, Next},
    response::Response,
    routing::{get, put},
    Extension, Json, Router,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, CorsLayer};

use crate::auth::{self, Allowed, Authenticator, Claims};
use crate::error::AppError;

const MAX_BODY: usize = 1024;

#[derive(Clone)]
struct DossiersAuth {
    authenticator: Option<Arc<Authenticator>>,
    allowed: Arc<Allowed>,
}

pub fn router() -> Router<PgPool> {
    router_with(auth::authenticator_dossiers(), auth::allowed_dossiers())
}

fn router_with(authenticator: Option<Arc<Authenticator>>, allowed: Arc<Allowed>) -> Router<PgPool> {
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("https://chet-admin.chetana.fr"),
            HeaderValue::from_static("http://localhost:5174"),
        ])
        .allow_methods([Method::GET, Method::PUT])
        .allow_headers(AllowHeaders::list([header::AUTHORIZATION, header::CONTENT_TYPE]));
    Router::new()
        .route("/dossiers/plan", get(plan))
        .route("/dossiers/tasks/{key}/done", put(put_done))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .route_layer(from_fn_with_state(DossiersAuth { authenticator, allowed }, guard))
        // CORS à l'extérieur du garde : un préflight n'a pas d'Authorization.
        .layer(cors)
}

async fn guard(State(a): State<DossiersAuth>, mut req: Request, next: Next) -> Result<Response, AppError> {
    let token = auth::bearer(req.headers()).ok_or(AppError::Unauthorized)?;
    let authenticator = a.authenticator.as_ref().ok_or(AppError::Unauthorized)?;
    let claims = authenticator.validate(token).await.map_err(|e| {
        tracing::debug!("dossiers: jeton refusé: {e}");
        AppError::Unauthorized
    })?;
    if !a.allowed.permits(&claims) {
        tracing::warn!("dossiers: sub hors liste blanche: {}", claims.sub);
        return Err(AppError::Forbidden);
    }
    req.extensions_mut().insert(claims);
    Ok(next.run(req).await)
}

#[derive(FromRow)]
struct SubjectRow {
    id: String,
    title: String,
    status: String,
    locked: bool,
}

#[derive(FromRow)]
struct TaskRow {
    key: String,
    subject_id: String,
    text: String,
    done: bool,
    done_at: Option<DateTime<Utc>>,
}

#[derive(FromRow, Serialize)]
struct DeadlineOut {
    due: NaiveDate,
    subject: String,
    action: String,
}

async fn plan(State(pool): State<PgPool>) -> Result<Json<Value>, AppError> {
    let subjects: Vec<SubjectRow> =
        sqlx::query_as("SELECT id, title, status, locked FROM dossiers.subject ORDER BY position").fetch_all(&pool).await?;
    let tasks: Vec<TaskRow> =
        sqlx::query_as("SELECT key, subject_id, text, done, done_at FROM dossiers.task ORDER BY subject_id, position")
            .fetch_all(&pool)
            .await?;
    let deadlines: Vec<DeadlineOut> =
        sqlx::query_as("SELECT due, subject, action FROM dossiers.deadline ORDER BY due, id").fetch_all(&pool).await?;
    let sujets: Vec<Value> = subjects
        .iter()
        .map(|s| {
            let own: Vec<Value> = tasks
                .iter()
                .filter(|t| t.subject_id == s.id)
                .map(|t| json!({ "key": t.key, "text": t.text, "done": t.done, "doneAt": t.done_at }))
                .collect();
            json!({ "id": s.id, "title": s.title, "status": s.status, "locked": s.locked, "tasks": own })
        })
        .collect();
    Ok(Json(json!({ "sujets": sujets, "deadlines": deadlines })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DoneBody {
    done: bool,
}

async fn put_done(
    State(pool): State<PgPool>,
    Extension(claims): Extension<Claims>,
    Path(key): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let DoneBody { done } =
        serde_json::from_slice(&body).map_err(|_| AppError::Validation("corps invalide : {\"done\": bool}".into()))?;
    let changed = sqlx::query(
        "UPDATE dossiers.task SET done = $2, done_at = CASE WHEN $2 THEN now() END, \
         done_by = CASE WHEN $2 THEN $3 END, updated_at = now() WHERE key = $1",
    )
    .bind(&key)
    .bind(done)
    .bind(&claims.sub)
    .execute(&pool)
    .await?
    .rows_affected();
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "key": key, "done": done })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::{Request as HttpRequest, StatusCode}};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    fn app(allowed: &str) -> Router {
        let pool = PgPoolOptions::new().connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
        router_with(None, Arc::new(Allowed::new(allowed, ""))).with_state(pool)
    }

    async fn status(app: Router, method: &str, uri: &str) -> StatusCode {
        let req = HttpRequest::builder().method(method).uri(uri).body(Body::empty()).unwrap();
        app.oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn sans_jeton_toutes_les_routes_refusent() {
        assert_eq!(status(app("u1"), "GET", "/dossiers/plan").await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(app("u1"), "PUT", "/dossiers/tasks/lys-1/done").await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sans_authenticator_configure_personne_nentre() {
        let req = HttpRequest::builder().uri("/dossiers/plan").header("authorization", "Bearer abc.def.ghi").body(Body::empty()).unwrap();
        assert_eq!(app("u1").oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }
}
