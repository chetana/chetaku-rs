//! Intégration des documents : Postgres jetable (LYS_TEST_DATABASE_URL) ET S3 compatible jetable
//! (LYS_TEST_S3_ENDPOINT, LYS_TEST_S3_ACCESS_KEY, LYS_TEST_S3_SECRET_KEY). Variable absente : test ignoré.
//! Clés factices de test uniquement.

use super::*;
use crate::routes::lys_docs::{DocStore, MAX_UPLOAD};
use s3::{Bucket, BucketConfiguration};
use std::time::Duration;

static BUCKET_SEQ: AtomicU32 = AtomicU32::new(0);

struct DApi {
    base: String,
    http: reqwest::Client,
    pool: PgPool,
    bucket: Option<&'static Bucket>,
}

async fn s3_bucket() -> Option<&'static Bucket> {
    let endpoint = std::env::var("LYS_TEST_S3_ENDPOINT").ok()?;
    let access = std::env::var("LYS_TEST_S3_ACCESS_KEY").ok()?;
    let secret = std::env::var("LYS_TEST_S3_SECRET_KEY").ok()?;
    let name = format!("lys-t-{}-{}", std::process::id(), BUCKET_SEQ.fetch_add(1, Ordering::SeqCst));
    let region = s3::Region::Custom { region: "fr-par".into(), endpoint: endpoint.clone() };
    let creds = s3::creds::Credentials::new(Some(&access), Some(&secret), None, None, None).unwrap();
    Bucket::create_with_path_style(&name, region, creds, BucketConfiguration::default()).await.unwrap();
    let b = crate::s3cache::build_bucket(&name, &access, &secret, "fr-par", &endpoint, true).unwrap();
    Some(Box::leak(b))
}

async fn dapi(with_storage: bool, ttl: u32) -> Option<DApi> {
    let pool = fresh_pool().await?;
    let bucket = if with_storage { Some(s3_bucket().await?) } else { None };
    let mut keys = KeyMap::new();
    keys.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
    let auth = Some(Arc::new(Authenticator::with_static_keys(ISS, AUD, keys)));
    let store = DocStore::new(bucket, ttl);
    let app = super::super::router_with_store(auth, Arc::new(Allowed::new("lys1,lys2", "")), store).with_state(pool.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    Some(DApi { base: format!("http://{addr}"), http: reqwest::Client::new(), pool, bucket })
}

fn pdf(size: usize) -> Vec<u8> {
    let mut v = b"%PDF-1.7\n".to_vec();
    v.extend((0..size.saturating_sub(v.len())).map(|i| (i % 251) as u8));
    v
}

impl DApi {
    async fn send(&self, m: reqwest::Method, path: &str, sub: Option<&str>, body: Option<Vec<u8>>) -> (StatusCode, Value) {
        let mut r = self.http.request(m, format!("{}{path}", self.base));
        if let Some(s) = sub {
            r = r.bearer_auth(token(claims(s)));
        }
        if let Some(b) = body {
            // Content-Type volontairement trompeur : seuls les octets comptent.
            r = r.header("content-type", "application/pdf").body(b);
        }
        let resp = match r.send().await {
            Ok(r) => r,
            Err(e) => panic!("requête {path}: {e:?}"),
        };
        let st = StatusCode::from_u16(resp.status().as_u16()).unwrap();
        (st, resp.json().await.unwrap_or(Value::Null))
    }
    async fn upload(&self, task: &str, query: &str, body: Vec<u8>) -> (StatusCode, Value) {
        self.send(reqwest::Method::POST, &format!("/lys/tasks/{task}/documents{query}"), Some("lys1"), Some(body)).await
    }
    async fn url_of(&self, id: &str) -> (StatusCode, Value) {
        self.send(reqwest::Method::GET, &format!("/lys/documents/{id}/url"), Some("lys2"), None).await
    }
    async fn remove(&self, id: &str) -> (StatusCode, Value) {
        self.send(reqwest::Method::DELETE, &format!("/lys/documents/{id}"), Some("lys1"), None).await
    }
}

macro_rules! doc_test {
    ($name:ident, $ttl:expr, |$a:ident| $body:block) => {
        #[tokio::test]
        async fn $name() {
            let Some($a) = dapi(true, $ttl).await else {
                eprintln!("ignoré : LYS_TEST_DATABASE_URL ou LYS_TEST_S3_* absente");
                return;
            };
            $body
        }
    };
}

doc_test!(envoi_8_mo_201_puis_lecture_par_url_signee, 300, |a| {
    let file = pdf(8 * 1024 * 1024);
    let (st, v) = a.upload("vls-ts-anef", "?item=vls-ts-anef.1&filename=../Acte%20%C3%A9t%C3%A9.pdf", file.clone()).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
    let d = &v["document"];
    assert_eq!(d["filename"], "Acte été.pdf", "nom assaini");
    assert_eq!(d["content_type"], "application/pdf");
    assert_eq!(d["size_bytes"], file.len());
    assert_eq!(d["item_slug"], "vls-ts-anef.1");
    assert_eq!(d["uploaded_by"], "lys1");
    let id = d["id"].as_str().unwrap().to_string();
    let key: String = sqlx::query_scalar("SELECT s3_key FROM lys_fr.document WHERE id = $1::uuid").bind(&id).fetch_one(&a.pool).await.unwrap();
    assert_eq!(key, format!("docs/vls-ts-anef/{id}.pdf"), "clé sans le nom d'origine");
    let stored = a.bucket.unwrap().get_object(&key).await.unwrap();
    assert_eq!(stored.bytes().len(), file.len());
    assert_eq!(stored.bytes().as_ref(), file.as_slice());

    let plan = a.send(reqwest::Method::GET, "/lys/plan", Some("lys1"), None).await.1;
    let task = &plan["categories"][0]["tasks"][0];
    assert_eq!(task["slug"], "vls-ts-anef");
    assert_eq!(task["documents"][0]["id"], id.as_str());
    assert_eq!(task["documents"][0]["item_slug"], "vls-ts-anef.1");
    assert_eq!(plan["categories"][0]["tasks"][1]["documents"], json!([]), "tableau vide sans document");

    let (st, u) = a.url_of(&id).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(u["expires_in"], 300);
    let url = u["url"].as_str().unwrap();
    assert!(url.contains("response-content-disposition=") && url.contains("X-Amz-Signature="), "{url}");
    // Lecture directe sur le S3 comme le ferait un navigateur : sans Authorization.
    let r = reqwest::get(url).await.unwrap();
    assert_eq!(r.status(), 200);
    let cd = r.headers().get("content-disposition").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    assert!(cd.starts_with("attachment"), "content-disposition renvoyé par le S3: {cd:?}");
    assert_eq!(r.bytes().await.unwrap().as_ref(), file.as_slice());
});

doc_test!(corps_13_mo_413_et_limite_16_kio_conservee_ailleurs, 300, |a| {
    let (st, _) = a.upload("cv", "", pdf(MAX_UPLOAD + 1024 * 1024)).await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.document").fetch_one(&a.pool).await.unwrap();
    assert_eq!(n, 0);
    let big = format!(r#"{{"note":"{}"}}"#, "x".repeat(20 * 1024));
    let (st, _) = a.send(reqwest::Method::PATCH, "/lys/tasks/cv", Some("lys1"), Some(big.into_bytes())).await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE, "les autres routes gardent 16 Kio");
    // Juste sous la limite : accepté.
    let (st, v) = a.upload("cv", "", pdf(MAX_UPLOAD)).await;
    assert_eq!(st, StatusCode::CREATED, "{v}");
});

doc_test!(url_expiree_refusee, 2, |a| {
    let (_, v) = a.upload("cv", "", pdf(2048)).await;
    let id = v["document"]["id"].as_str().unwrap().to_string();
    let (_, u) = a.url_of(&id).await;
    let url = u["url"].as_str().unwrap().to_string();
    assert_eq!(reqwest::get(&url).await.unwrap().status(), 200, "valable avant l'expiration");
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(reqwest::get(&url).await.unwrap().status(), 403, "refusée après expiration");
});

doc_test!(url_sans_signature_ou_alteree_refusee, 300, |a| {
    let (_, v) = a.upload("cv", "", pdf(2048)).await;
    let id = v["document"]["id"].as_str().unwrap().to_string();
    let (_, u) = a.url_of(&id).await;
    let url = u["url"].as_str().unwrap();
    let nue = url.split('?').next().unwrap();
    assert_eq!(reqwest::get(nue).await.unwrap().status(), 403, "sans signature");
    let sig_pos = url.find("X-Amz-Signature=").unwrap() + "X-Amz-Signature=".len();
    let mut alteree = url.to_string();
    let c = if &url[sig_pos..sig_pos + 1] == "0" { "1" } else { "0" };
    alteree.replace_range(sig_pos..sig_pos + 1, c);
    assert_eq!(reqwest::get(&alteree).await.unwrap().status(), 403, "signature altérée");
    // La signature d'un objet ne vaut pas pour un autre.
    let autre = url.replace(&id, "00000000-0000-4000-8000-000000000000");
    assert_ne!(reqwest::get(&autre).await.unwrap().status(), 200);
});

doc_test!(suppression_douce_idempotente_puis_url_404, 300, |a| {
    let (_, v) = a.upload("cv", "", pdf(4096)).await;
    let id = v["document"]["id"].as_str().unwrap().to_string();
    let key: String = sqlx::query_scalar("SELECT s3_key FROM lys_fr.document WHERE id = $1::uuid").bind(&id).fetch_one(&a.pool).await.unwrap();
    assert_eq!(a.remove(&id).await.0, StatusCode::NO_CONTENT);
    let (deleted, n): (bool, i64) =
        sqlx::query_as("SELECT deleted_at IS NOT NULL, count(*) OVER () FROM lys_fr.document WHERE id = $1::uuid")
            .bind(&id).fetch_one(&a.pool).await.unwrap();
    assert!(deleted && n == 1, "ligne conservée, marquée supprimée");
    let gone = a.bucket.unwrap().get_object(&key).await;
    assert!(matches!(gone, Err(s3::error::S3Error::HttpFailWithBody(404, _))), "objet retiré du bucket: {gone:?}");
    assert_eq!(a.url_of(&id).await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.remove(&id).await.0, StatusCode::NO_CONTENT, "idempotent");
    let plan = a.send(reqwest::Method::GET, "/lys/plan", Some("lys1"), None).await.1;
    let cv = plan["categories"].as_array().unwrap().iter().flat_map(|c| c["tasks"].as_array().unwrap().iter()).find(|t| t["slug"] == "cv").unwrap().clone();
    assert_eq!(cv["documents"], json!([]));
    assert_eq!(a.remove("00000000-0000-4000-8000-000000000000").await.0, StatusCode::NOT_FOUND);
});

doc_test!(refus_415_404_400_et_aucune_trace, 300, |a| {
    let mut exe = b"MZ\x90\x00".to_vec();
    exe.extend_from_slice(&[0u8; 512]);
    assert_eq!(a.upload("cv", "?filename=facture.pdf", exe).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(a.upload("cv", "", Vec::new()).await.0, StatusCode::BAD_REQUEST, "fichier vide");
    assert_eq!(a.upload("tache-inconnue", "", pdf(100)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.upload("cv", "?item=item-inconnu", pdf(100)).await.0, StatusCode::NOT_FOUND);
    assert_eq!(a.upload("cv", "?item=vls-ts-anef.1", pdf(100)).await.0, StatusCode::NOT_FOUND, "item d'une autre tâche");
    for bad in ["pas-un-uuid", "123e4567e89b42d3a456426614174000", "%7B123e4567-e89b-42d3-a456-426614174000%7D"] {
        assert_eq!(a.url_of(bad).await.0, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(a.remove(bad).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
    assert_eq!(a.url_of("00000000-0000-4000-8000-000000000000").await.0, StatusCode::NOT_FOUND);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.document").fetch_one(&a.pool).await.unwrap();
    assert_eq!(n, 0, "aucune ligne après les refus");
});

doc_test!(auth_401_403_sur_les_routes_documents, 300, |a| {
    use reqwest::Method as M;
    let (_, v) = a.upload("cv", "", pdf(100)).await;
    let id = v["document"]["id"].as_str().unwrap().to_string();
    let routes = [
        (M::POST, "/lys/tasks/cv/documents".to_string(), Some(pdf(100))),
        (M::GET, format!("/lys/documents/{id}/url"), None),
        (M::DELETE, format!("/lys/documents/{id}"), None),
    ];
    for (m, path, body) in routes {
        assert_eq!(a.send(m.clone(), &path, None, body.clone()).await.0, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(a.send(m, &path, Some("intrus"), body).await.0, StatusCode::FORBIDDEN, "{path}");
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM lys_fr.document WHERE deleted_at IS NULL").fetch_one(&a.pool).await.unwrap();
    assert_eq!(n, 1, "un sub hors liste ne crée ni ne supprime rien");
});

doc_test!(cinq_envois_simultanes_passent_tous, 300, |a| {
    let a = Arc::new(a);
    let mut hs = Vec::new();
    for i in 0..5 {
        let a = a.clone();
        hs.push(tokio::spawn(async move { a.upload("cv", &format!("?filename=f{i}.pdf"), pdf(3 * 1024 * 1024)).await.0 }));
    }
    for h in hs {
        assert_eq!(h.await.unwrap(), StatusCode::CREATED);
    }
});

#[tokio::test]
async fn stockage_absent_503_sur_les_documents_le_reste_de_lys_fonctionne() {
    let Some(a) = dapi(false, 300).await else {
        eprintln!("ignoré : LYS_TEST_DATABASE_URL absente");
        return;
    };
    assert_eq!(a.upload("cv", "", pdf(100)).await.0, StatusCode::SERVICE_UNAVAILABLE);
    let id = "00000000-0000-4000-8000-000000000000";
    let (st, v) = a.url_of(id).await;
    assert_eq!((st, v["error"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("stockage indisponible")));
    assert_eq!(a.remove(id).await.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(a.send(reqwest::Method::GET, "/lys/me", Some("lys1"), None).await.0, StatusCode::OK);
    assert_eq!(a.send(reqwest::Method::GET, "/lys/plan", Some("lys1"), None).await.0, StatusCode::OK);
}
