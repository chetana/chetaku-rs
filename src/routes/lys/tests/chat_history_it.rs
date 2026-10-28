//! Historique du chat : faux Cline local + stockage d'objets en mémoire (ni base ni S3 requis).

use super::*;
use crate::routes::lys_chat::{tests::plan_sensible, ChatService};
use crate::routes::lys_docs::DocStore;
use crate::routes::lys_history::ChatHistory;
use axum::{routing::post, Json as AxJson, Router};
use std::time::Duration;

const FR: &str = "Validez le VLS-TS avant le 2 février 2027 (tâche vls-ts-anef).";
const KM: &str = "សូមបញ្ជាក់សុពលភាព VLS-TS មុនថ្ងៃទី២ ខែកុម្ភៈ ឆ្នាំ២០២៧";
const Q: &str = r#"{"question":"Par quoi commencer ?","lang":"fr"}"#;

async fn fake_cline() -> String {
    async fn h(AxJson(body): AxJson<Value>) -> AxJson<Value> {
        let tr = body["messages"][0]["content"].as_str().unwrap_or("").contains("traducteur");
        let t = if tr { KM } else { FR };
        AxJson(json!({"choices": [{"finish_reason": "stop", "message": {"content": t}}]}))
    }
    let app = Router::new().route("/v1/chat/completions", post(h));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}/v1/chat/completions")
}

async fn serve(history: ChatHistory, per_user: u32) -> String {
    let url = fake_cline().await;
    let svc = ChatService::for_test(&url, per_user, 300, Duration::from_secs(2), Some(plan_sensible()));
    let mut keys = KeyMap::new();
    keys.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
    let auth = Some(Arc::new(Authenticator::with_static_keys(ISS, AUD, keys)));
    let pool = PgPool::connect_lazy("postgres://x:x@127.0.0.1:1/x").unwrap();
    let app = super::super::router_with_history(auth, Arc::new(Allowed::new("lys1,lys2", "")), DocStore::disabled(), svc, history).with_state(pool);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

async fn call(base: &str, m: reqwest::Method, path: &str, sub: Option<&str>, body: Option<&str>) -> (StatusCode, Value) {
    let mut r = reqwest::Client::new().request(m, format!("{base}{path}"));
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

async fn ask(base: &str, sub: &str) -> (StatusCode, Value) {
    call(base, reqwest::Method::POST, "/lys/chat", Some(sub), Some(Q)).await
}

async fn history(base: &str, sub: &str, q: &str) -> (StatusCode, Value) {
    call(base, reqwest::Method::GET, &format!("/lys/chat/history{q}"), Some(sub), None).await
}

#[tokio::test]
async fn post_enregistre_puis_historique_partage_trie_avec_mine() {
    let h = ChatHistory::in_memory();
    let base = serve(h.clone(), 30).await;
    let (st, a) = ask(&base, "lys1").await;
    assert_eq!(st, StatusCode::OK, "{a}");
    let id1 = a["id"].as_str().expect("id");
    assert!(a["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(crate::routes::lys_history::parse_id(id1).is_some());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let (_, b) = ask(&base, "lys2").await;
    let id2 = b["id"].as_str().unwrap().to_string();
    assert_eq!(a["answer_fr"], FR, "la réponse du chat reste inchangée");
    assert_eq!(h.object_count(), 2);

    let (st, v) = history(&base, "lys1", "").await;
    assert_eq!(st, StatusCode::OK);
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], id2.as_str(), "le plus récent d'abord");
    assert_eq!((items[0]["mine"].as_bool(), items[1]["mine"].as_bool()), (Some(false), Some(true)), "vu par lys1");
    assert_eq!((items[1]["question"].as_str(), items[1]["answer_km"].as_str()), (Some("Par quoi commencer ?"), Some(KM)));
    assert_eq!(items[1]["task_slugs"], json!(["vls-ts-anef"]));
    assert!(!v.to_string().contains("author") && !v.to_string().contains("lys2"), "author_sub ne sort jamais");
    let (_, v2) = history(&base, "lys2", "?limit=1").await;
    assert_eq!(v2["items"].as_array().unwrap().len(), 1);
    assert_eq!(v2["items"][0]["mine"], true);
    assert_eq!(history(&base, "lys1", "?limit=abc").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn suppression_204_404_400_par_n_importe_quel_compte() {
    let h = ChatHistory::in_memory();
    let base = serve(h.clone(), 30).await;
    let id = ask(&base, "lys1").await.1["id"].as_str().unwrap().to_string();
    let path = format!("/lys/chat/history/{id}");
    let (st, _) = call(&base, reqwest::Method::DELETE, &path, Some("lys2"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT, "historique partagé : l'autre compte peut supprimer");
    assert_eq!(history(&base, "lys1", "").await.1["items"], json!([]));
    assert_eq!(call(&base, reqwest::Method::DELETE, &path, Some("lys1"), None).await.0, StatusCode::NOT_FOUND);
    let (st, v) = call(&base, reqwest::Method::DELETE, "/lys/chat/history/..%2F..%2Fx", Some("lys1"), None).await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid")));
    let (st, v) = call(&base, reqwest::Method::DELETE, "/lys/chat/history/pas-un-id", Some("lys1"), None).await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid")));
}

#[tokio::test]
async fn stockage_en_panne_le_post_reste_200_avec_id_null() {
    let base = serve(ChatHistory::broken(), 30).await;
    let (st, v) = ask(&base, "lys1").await;
    assert_eq!(st, StatusCode::OK, "{v}");
    assert!(v["id"].is_null() && v["created_at"].is_null());
    assert_eq!(v["answer_fr"], FR);
    let (st, v) = history(&base, "lys1", "").await;
    assert_eq!((st, v["kind"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("unavailable")));
}

#[tokio::test]
async fn stockage_desactive_post_id_null_et_historique_vide() {
    let base = serve(ChatHistory::disabled(), 30).await;
    let (st, v) = ask(&base, "lys1").await;
    assert_eq!(st, StatusCode::OK);
    assert!(v["id"].is_null() && v["created_at"].is_null());
    let (st, v) = history(&base, "lys1", "").await;
    assert_eq!((st, v), (StatusCode::OK, json!({"items": []})));
}

#[tokio::test]
async fn le_plafond_du_chat_ne_vise_pas_get_ni_delete() {
    let base = serve(ChatHistory::in_memory(), 1).await;
    let id = ask(&base, "lys1").await.1["id"].as_str().unwrap().to_string();
    assert_eq!(ask(&base, "lys1").await.0, StatusCode::TOO_MANY_REQUESTS);
    for _ in 0..3 {
        assert_eq!(history(&base, "lys1", "").await.0, StatusCode::OK);
    }
    let (st, _) = call(&base, reqwest::Method::DELETE, &format!("/lys/chat/history/{id}"), Some("lys1"), None).await;
    assert_eq!(st, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn historique_401_sans_jeton_403_hors_liste_blanche() {
    let base = serve(ChatHistory::in_memory(), 30).await;
    let id = "2026-10-28_0b9e1c52-3f0a-4c1e-9a55-6d2f7a1b8c34";
    for (m, p) in [(reqwest::Method::GET, "/lys/chat/history".to_string()), (reqwest::Method::DELETE, format!("/lys/chat/history/{id}"))] {
        assert_eq!(call(&base, m.clone(), &p, None, None).await.0, StatusCode::UNAUTHORIZED);
        let (st, v) = call(&base, m, &p, Some("intrus"), None).await;
        assert_eq!((st, v), (StatusCode::FORBIDDEN, json!({"error": "Forbidden"})));
    }
}
