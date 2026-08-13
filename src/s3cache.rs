//! Cache Object Storage (Scaleway S3) pour les lectures publiques.
//!
//! But : une Serverless SQL est facturée tant qu'elle est « active ». Chaque lecture
//! non-cachée (crawler, proxy chetana.fr, accès direct chetaku.chetana.fr) la réveille
//! et la maintient chaude 24/7. En servant ces lectures depuis S3, la base n'est touchée
//! que sur cache-miss (refresh TTL) ou sur écriture admin (qui invalide le cache) → elle dort.
//!
//! Robustesse : entièrement best-effort. Si S3 est indisponible ou mal configuré, TOUT
//! retombe silencieusement sur la base (le site ne casse jamais, on perd juste le cache).
//! Format objet : `{"cached_at": <unix_secs>, "data": <payload d'origine>}` — le client
//! reçoit `data` brut, identique à l'ancienne réponse.

use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use s3::creds::Credentials;
use s3::{Bucket, Region};

/// Durée de vie du cache. Les écritures admin invalident déjà les clés concernées ; ce TTL
/// n'est qu'un filet de sécurité contre une modification faite hors des endpoints admin.
/// 24 h : chaque endpoint ne réveille la base qu'au plus 1×/jour pour se rafraîchir (au lieu
/// de 4×/jour à 6h) → moins de mini-réveils facturés. La fraîcheur reste immédiate via l'invalidation.
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 3600); // 24 h

/// Bucket S3 initialisé paresseusement depuis l'env. `None` = cache désactivé (fallback base).
fn bucket() -> Option<&'static Bucket> {
    static B: OnceLock<Option<Box<Bucket>>> = OnceLock::new();
    B.get_or_init(|| {
        let access = std::env::var("S3_ACCESS_KEY").ok()?;
        let secret = std::env::var("S3_SECRET_KEY").ok()?;
        let name = std::env::var("S3_BUCKET").ok()?;
        let region_name = std::env::var("S3_REGION").unwrap_or_else(|_| "fr-par".to_string());
        let endpoint = std::env::var("S3_ENDPOINT")
            .unwrap_or_else(|_| format!("https://s3.{region_name}.scw.cloud"));
        let region = Region::Custom { region: region_name, endpoint };
        let creds = match Credentials::new(Some(&access), Some(&secret), None, None, None) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("s3cache: credentials invalides, cache désactivé: {e}");
                return None;
            }
        };
        match Bucket::new(&name, region, creds) {
            Ok(b) => {
                tracing::info!("s3cache: bucket '{name}' actif");
                Some(b)
            }
            Err(e) => {
                tracing::warn!("s3cache: init bucket échouée, cache désactivé: {e}");
                None
            }
        }
    })
    .as_deref()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path_for(key: &str) -> String {
    format!("cache/{key}.json")
}

/// Retourne le payload caché si présent ET plus récent que `ttl`, sinon `None`.
pub async fn get_fresh(key: &str, ttl: Duration) -> Option<serde_json::Value> {
    let b = bucket()?;
    let resp = b.get_object(path_for(key)).await.ok()?;
    if resp.status_code() != 200 {
        return None;
    }
    let wrapper: serde_json::Value = serde_json::from_slice(resp.bytes()).ok()?;
    let cached_at = wrapper.get("cached_at")?.as_u64()?;
    if now_secs().saturating_sub(cached_at) > ttl.as_secs() {
        return None; // périmé → cache-miss → refresh depuis la base
    }
    Some(wrapper.get("data")?.clone())
}

/// Écrit le payload dans le cache (best-effort, erreurs ignorées).
pub async fn put(key: &str, data: &serde_json::Value) {
    let Some(b) = bucket() else { return };
    let wrapper = serde_json::json!({ "cached_at": now_secs(), "data": data });
    let body = match serde_json::to_vec(&wrapper) {
        Ok(v) => v,
        Err(_) => return,
    };
    if let Err(e) = b
        .put_object_with_content_type(path_for(key), &body, "application/json")
        .await
    {
        tracing::warn!("s3cache: put '{key}' échoué: {e}");
    }
}

/// Supprime des clés du cache (appelé après une écriture admin). Best-effort.
pub async fn invalidate(keys: &[&str]) {
    let Some(b) = bucket() else { return };
    for k in keys {
        if let Err(e) = b.delete_object(path_for(k)).await {
            tracing::warn!("s3cache: invalidate '{k}' échoué: {e}");
        }
    }
}
