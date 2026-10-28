//! POST /lys/chat contre un faux serveur Cline local (CLINE_BASE_URL). Les cas sans base utilisent un plan fixe et
//! un pool paresseux ; le seul cas qui lit le plan en base est ignoré sans LYS_TEST_DATABASE_URL. Clé factice.

use super::*;
use crate::routes::lys_chat::{tests::plan_sensible, ChatService};
use crate::routes::lys_docs::DocStore;
use axum::{extract::State as AxState, routing::post, Json as AxJson, Router};
use std::sync::Mutex;
use std::time::Duration;

const FR: &str = "Validez le VLS-TS avant le 2 février 2027 (tâche vls-ts-anef).";
const KM: &str = "សូមបញ្ជាក់សុពលភាព VLS-TS មុនថ្ងៃទី២ ខែកុម្ភៈ ឆ្នាំ២០២៧";

#[derive(Clone)]
enum Reply {
    Text(String),
    Wrapped(String),
    Status(u16, String),
    Slow,
}

struct Fake {
    fr: Reply,
    km: Reply,
    calls: Vec<(Value, Option<String>)>,
}

type Shared = Arc<Mutex<Fake>>;

async fn fake_handler(AxState(f): AxState<Shared>, headers: axum::http::HeaderMap, AxJson(body): AxJson<Value>) -> axum::response::Response {
    use axum::response::IntoResponse;
    let is_translation = body["messages"][0]["content"].as_str().unwrap_or("").contains("traducteur");
    let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).map(String::from);
    let reply = {
        let mut g = f.lock().unwrap();
        g.calls.push((body, auth));
        if is_translation { g.km.clone() } else { g.fr.clone() }
    };
    let msg = |t: &str| json!({"choices": [{"finish_reason": "stop", "message": {"content": t}}]});
    match reply {
        Reply::Text(t) => AxJson(msg(&t)).into_response(),
        Reply::Wrapped(t) => AxJson(json!({"data": msg(&t)})).into_response(),
        Reply::Status(c, b) => (StatusCode::from_u16(c).unwrap(), b).into_response(),
        Reply::Slow => {
            tokio::time::sleep(Duration::from_secs(5)).await;
            AxJson(msg("trop tard")).into_response()
        }
    }
}

async fn fake_cline(fr: Reply, km: Reply) -> (String, Shared) {
    let shared: Shared = Arc::new(Mutex::new(Fake { fr, km, calls: vec![] }));
    let app = Router::new().route("/v1/chat/completions", post(fake_handler)).with_state(shared.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (format!("http://{addr}/v1/chat/completions"), shared)
}

async fn serve_chat(svc: ChatService, subs: &str, pool: PgPool) -> String {
    let mut keys = KeyMap::new();
    keys.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
    let auth = Some(Arc::new(Authenticator::with_static_keys(ISS, AUD, keys)));
    let app = super::super::router_with_chat(auth, Arc::new(Allowed::new(subs, "")), DocStore::disabled(), svc).with_state(pool);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

fn lazy_pool() -> PgPool {
    PgPool::connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap()
}

async fn ask(base: &str, sub: Option<&str>, body: &str) -> (StatusCode, Value) {
    let mut r = reqwest::Client::new().post(format!("{base}/lys/chat")).header("content-type", "application/json").body(body.to_string());
    if let Some(s) = sub {
        r = r.bearer_auth(token(claims(s)));
    }
    let resp = r.send().await.unwrap();
    let st = StatusCode::from_u16(resp.status().as_u16()).unwrap();
    (st, resp.json().await.unwrap_or(Value::Null))
}

const Q: &str = r#"{"question":"Par quoi commencer ?","lang":"fr"}"#;

async fn setup(fr: Reply, km: Reply, per_user: u32, global: u32) -> (String, Shared) {
    let (url, shared) = fake_cline(fr, km).await;
    let svc = ChatService::for_test(&url, per_user, global, Duration::from_secs(2), Some(plan_sensible()));
    (serve_chat(svc, "lys1,lys2", lazy_pool()).await, shared)
}

#[tokio::test]
async fn chat_nominal_200_fr_et_km() {
    let (base, fake) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 30, 300).await;
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["answer_fr"], FR);
    assert_eq!(v["answer_km"], KM);
    assert_eq!(v["machine_translated"], true);
    assert_eq!(v["task_slugs"], json!(["vls-ts-anef"]));
    assert_eq!(v["remaining_today"], 29);
    let g = fake.lock().unwrap();
    assert_eq!(g.calls.len(), 2, "une réponse puis une traduction");
    let (b1, auth) = &g.calls[0];
    assert_eq!(auth.as_deref(), Some("Bearer cle-factice-de-test"));
    assert_eq!(b1["model"], "cline-pass/glm-5.3-flash");
    assert_eq!(b1["stream"], false);
    assert_eq!(b1["reasoning_effort"], "low");
    assert_eq!(b1["messages"][1]["content"], "<question>Par quoi commencer ?</question>");
    let sent = b1.to_string();
    for secret in ["NOTE-PERSO-SECRETE", "LIEU-RDV-SECRET", "SUB-SECRET", "passeport-SECRET", "lys1"] {
        assert!(!sent.contains(secret), "{secret} envoyé à Cline");
    }
    assert!(g.calls[1].0["messages"][1]["content"].as_str().unwrap().contains(FR));
}

#[tokio::test]
async fn chat_lang_km_donne_aussi_les_deux_langues() {
    let (base, _f) = setup(Reply::Wrapped(FR.into()), Reply::Wrapped(KM.into()), 30, 300).await;
    let (st, v) = ask(&base, Some("lys1"), r#"{"question":"តើ?","lang":"km"}"#).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!((v["answer_fr"].as_str(), v["answer_km"].as_str()), (Some(FR), Some(KM)), "format enveloppé {{data:{{choices}}}} accepté");
}

#[tokio::test]
async fn chat_400_question_ou_lang_invalide() {
    let (base, fake) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 30, 300).await;
    let long = format!(r#"{{"question":"{}","lang":"fr"}}"#, "a".repeat(501));
    for body in [
        r#"{"question":"   ","lang":"fr"}"#,
        r#"{"question":"x","lang":"en"}"#,
        r#"{"question":"x"}"#,
        r#"{"question":"a\nb","lang":"fr"}"#,
        "pas du json",
        long.as_str(),
    ] {
        let (st, v) = ask(&base, Some("lys1"), body).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(v["kind"], "invalid");
        assert!(v["error"].is_string());
    }
    assert!(fake.lock().unwrap().calls.is_empty(), "aucun appel Cline sur une requête invalide");
    let (_, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(v["remaining_today"], 29, "les 400 ne consomment pas le quota");
}

#[tokio::test]
async fn chat_429_apres_n_questions() {
    let (base, _f) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 2, 300).await;
    assert_eq!(ask(&base, Some("lys1"), Q).await.1["remaining_today"], 1);
    assert_eq!(ask(&base, Some("lys1"), Q).await.1["remaining_today"], 0);
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["kind"], "daily_limit");
    assert_eq!(ask(&base, Some("lys2"), Q).await.0, StatusCode::OK, "autre sub : compteur distinct");
}

#[tokio::test]
async fn chat_429_plafond_global() {
    let (base, _f) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 30, 2).await;
    assert_eq!(ask(&base, Some("lys1"), Q).await.0, StatusCode::OK);
    assert_eq!(ask(&base, Some("lys2"), Q).await.0, StatusCode::OK);
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("daily_limit")));
}

#[tokio::test]
async fn chat_503_desactive() {
    let base = serve_chat(ChatService::disabled(), "lys1", lazy_pool()).await;
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("unavailable")));
}

#[tokio::test]
async fn chat_503_cline_en_erreur_sans_fuite_et_quota_rendu() {
    let corps_cline = "SECRET-CLINE-CORPS-ERREUR cle-factice-de-test";
    let (base, _f) = setup(Reply::Status(500, corps_cline.into()), Reply::Text(KM.into()), 1, 300).await;
    let resp = reqwest::Client::new()
        .post(format!("{base}/lys/chat"))
        .bearer_auth(token(claims("lys1")))
        .header("content-type", "application/json")
        .body(Q)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 503);
    let body = resp.text().await.unwrap();
    assert!(body.contains(r#""kind":"unavailable""#));
    assert!(!body.contains("SECRET-CLINE") && !body.contains("cle-factice"), "fuite: {body}");
    let (st, _) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "le quota (1/jour) a été rendu : pas de 429");
}

#[tokio::test]
async fn chat_503_cline_trop_lent() {
    let (base, _f) = setup(Reply::Slow, Reply::Text(KM.into()), 30, 300).await;
    let t0 = std::time::Instant::now();
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("unavailable")));
    assert!(t0.elapsed() < Duration::from_secs(4), "coupé à la limite configurée (2 s en test)");
}

#[tokio::test]
async fn chat_traduction_ko_donne_answer_km_null() {
    let (base, _f) = setup(Reply::Text(FR.into()), Reply::Status(500, "boom".into()), 30, 300).await;
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert_eq!(v["answer_fr"], FR);
    assert!(v["answer_km"].is_null());
    assert_eq!(v["machine_translated"], true);
}

#[tokio::test]
async fn chat_traduction_refusee_par_les_garde_fous_donne_null() {
    for mauvais in ["Hello world", "សូម 你好", "សូមបញ្ជាក់ VLS-TS មុនថ្ងៃទី៣ ខែកុម្ភៈ ឆ្នាំ២០២៧"] {
        let (base, _f) = setup(Reply::Text(FR.into()), Reply::Text(mauvais.into()), 30, 300).await;
        let (st, v) = ask(&base, Some("lys1"), Q).await;
        assert_eq!(st, StatusCode::OK);
        assert!(v["answer_km"].is_null(), "{mauvais} aurait dû être refusé");
    }
}

#[tokio::test]
async fn chat_401_sans_jeton_et_403_hors_liste_blanche() {
    let (base, fake) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 30, 300).await;
    assert_eq!(ask(&base, None, Q).await.0, StatusCode::UNAUTHORIZED);
    let (st, v) = ask(&base, Some("intrus"), Q).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(v, json!({"error": "Forbidden"}));
    assert!(fake.lock().unwrap().calls.is_empty());
}

#[tokio::test]
async fn chat_corps_au_dela_de_16_kio_refuse() {
    let (base, _f) = setup(Reply::Text(FR.into()), Reply::Text(KM.into()), 30, 300).await;
    let big = format!(r#"{{"question":"{}","lang":"fr"}}"#, "a".repeat(20 * 1024));
    let (st, _) = ask(&base, Some("lys1"), &big).await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn chat_lit_le_plan_en_base_sans_donnee_personnelle() {
    let Some(a) = api().await else {
        eprintln!("ignoré : LYS_TEST_DATABASE_URL absente");
        return;
    };
    sqlx::query("UPDATE lys_fr.task SET note = 'NOTE-PERSO-SECRETE' WHERE slug = 'vls-ts-anef'").execute(&a.pool).await.unwrap();
    sqlx::query("UPDATE lys_fr.item SET done = true, done_by = 'SUB-SECRET-DB' WHERE slug = 'vls-ts-anef.1'").execute(&a.pool).await.unwrap();
    sqlx::query("INSERT INTO lys_fr.appointment (task_slug, starts_at, place, note, created_by) VALUES ('vls-ts-anef', '2026-11-09T13:30:00Z', 'LIEU-RDV-SECRET', 'NOTE-RDV-SECRET', 'SUB-SECRET-DB')")
        .execute(&a.pool)
        .await
        .unwrap();
    let (url, fake) = fake_cline(Reply::Text(FR.into()), Reply::Text(KM.into())).await;
    let svc = ChatService::for_test(&url, 30, 300, Duration::from_secs(5), None);
    let base = serve_chat(svc, "lys1", a.pool.clone()).await;
    let (st, v) = ask(&base, Some("lys1"), Q).await;
    assert_eq!(st, StatusCode::OK, "{v}");
    let g = fake.lock().unwrap();
    let system = g.calls[0].0["messages"][0]["content"].as_str().unwrap().to_string();
    assert!(system.contains("Valider le VLS-TS"), "le plan de la base est dans le contexte");
    for secret in ["NOTE-PERSO-SECRETE", "LIEU-RDV-SECRET", "NOTE-RDV-SECRET", "SUB-SECRET-DB", "lys1"] {
        assert!(!g.calls[0].0.to_string().contains(secret), "{secret} envoyé à Cline");
    }
}
