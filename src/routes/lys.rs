//! lys-en-france : sous-routeur `/lys`, protégé par un JWT Logto de l'audience dédiée + liste blanche de `sub`.
//! Le garde est un `route_layer` : toute route ajoutée ici est protégée d'office. Inscription Logto ouverte
//! côté tenant : la liste blanche est la seule barrière, d'où le 403 distinct d'un simple 401.

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, Method},
    middleware::{from_fn_with_state, Next},
    response::Response,
    routing::get,
    Extension, Json, Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::cors::{AllowHeaders, CorsLayer};

use crate::auth::{self, Allowed, Authenticator, Claims};
use crate::error::AppError;

#[derive(Clone)]
struct LysAuth {
    authenticator: Option<Arc<Authenticator>>,
    allowed: Arc<Allowed>,
}

/// Routeur de production : audience et liste blanche lues dans l'environnement.
pub fn router() -> Router<PgPool> {
    router_with(auth::authenticator_lys(), auth::allowed_lys())
}

fn router_with(authenticator: Option<Arc<Authenticator>>, allowed: Arc<Allowed>) -> Router<PgPool> {
    let cors = CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("https://lys-en-france.chetana.fr"),
            HeaderValue::from_static("http://localhost:5173"),
        ])
        .allow_methods([Method::GET, Method::PUT, Method::PATCH, Method::POST, Method::DELETE])
        .allow_headers(AllowHeaders::list([header::AUTHORIZATION, header::CONTENT_TYPE]));
    Router::new()
        .route("/lys/me", get(me))
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
}
