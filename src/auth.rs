//! Lecture du tableau de bord : access token JWT Logto (JWKS distant) + liste blanche côté serveur.
//! Aucune clé n'est lue dans le token : l'algorithme vient de la clé JWKS (RSA/EC), jamais de l'en-tête.

use axum::http::HeaderMap;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
    time::{Duration, Instant},
};

const JWKS_TTL: Duration = Duration::from_secs(600);
const REFETCH_MIN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
}

pub type KeyMap = HashMap<String, (Algorithm, DecodingKey)>;

pub fn bearer(headers: &HeaderMap) -> Option<&str> {
    let v = headers.get("authorization")?.to_str().ok()?;
    let t = v.strip_prefix("Bearer ")?.trim();
    (!t.is_empty()).then_some(t)
}

pub fn verify_with_keys(token: &str, keys: &KeyMap, issuer: &str, audience: &str) -> Result<Claims, String> {
    let header = decode_header(token).map_err(|e| format!("en-tête invalide: {e}"))?;
    let kid = header.kid.ok_or("kid absent")?;
    let (alg, key) = keys.get(&kid).ok_or("kid inconnu")?;
    let mut v = Validation::new(*alg);
    v.set_issuer(&[issuer]);
    v.set_audience(&[audience]);
    v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    decode::<Claims>(token, key, &v).map(|d| d.claims).map_err(|e| format!("token invalide: {e}"))
}

/// Liste blanche : un `sub` Logto ou un email ; les deux vides → personne n'entre.
pub struct Allowed {
    subs: Vec<String>,
    emails: Vec<String>,
}

fn split_list(s: &str, lower: bool) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| if lower { x.to_lowercase() } else { x.to_string() })
        .collect()
}

impl Allowed {
    pub fn new(subs: &str, emails: &str) -> Self {
        Allowed { subs: split_list(subs, false), emails: split_list(emails, true) }
    }
    pub fn permits(&self, c: &Claims) -> bool {
        if !c.sub.is_empty() && self.subs.contains(&c.sub) {
            return true;
        }
        c.email.as_deref().map(|e| e.to_lowercase()).is_some_and(|e| !e.is_empty() && self.emails.contains(&e))
    }
}

pub fn allowed() -> &'static Allowed {
    static A: OnceLock<Allowed> = OnceLock::new();
    A.get_or_init(|| {
        Allowed::new(
            &std::env::var("DASHBOARD_ALLOWED_SUBS").unwrap_or_default(),
            &std::env::var("DASHBOARD_ALLOWED_EMAILS").unwrap_or_default(),
        )
    })
}

#[derive(Default)]
struct Cache {
    keys: KeyMap,
    fetched: Option<Instant>,
    attempted: Option<Instant>,
}

pub struct Authenticator {
    jwks_url: String,
    issuer: String,
    audience: String,
    cache: RwLock<Cache>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
}

fn parse_jwk(j: &Jwk) -> Option<(Algorithm, DecodingKey)> {
    match j.kty.as_str() {
        "RSA" => DecodingKey::from_rsa_components(j.n.as_deref()?, j.e.as_deref()?).ok().map(|k| (Algorithm::RS256, k)),
        "EC" => {
            let alg = match j.crv.as_deref()? {
                "P-256" => Algorithm::ES256,
                "P-384" => Algorithm::ES384,
                _ => return None,
            };
            DecodingKey::from_ec_components(j.x.as_deref()?, j.y.as_deref()?).ok().map(|k| (alg, k))
        }
        _ => None,
    }
}

impl Authenticator {
    pub fn new(endpoint: &str, audience: &str) -> Self {
        let base = endpoint.trim_end_matches('/');
        Authenticator {
            jwks_url: format!("{base}/oidc/jwks"),
            issuer: format!("{base}/oidc"),
            audience: audience.to_string(),
            cache: RwLock::new(Cache::default()),
        }
    }

    /// Tests : clés injectées, aucun accès réseau (le JWKS est déclaré frais et déjà tenté).
    #[cfg(test)]
    pub fn with_static_keys(issuer: &str, audience: &str, keys: KeyMap) -> Self {
        let now = Instant::now();
        Authenticator {
            jwks_url: "http://127.0.0.1:1/oidc/jwks".into(),
            issuer: issuer.into(),
            audience: audience.into(),
            cache: RwLock::new(Cache { keys, fetched: Some(now), attempted: Some(now) }),
        }
    }

    fn needs_refresh(&self, kid: Option<&str>) -> bool {
        let c = self.cache.read().unwrap();
        let stale = c.fetched.map_or(true, |t| t.elapsed() > JWKS_TTL);
        let unknown = kid.is_some_and(|k| !c.keys.contains_key(k));
        let throttled = c.attempted.is_some_and(|t| t.elapsed() < REFETCH_MIN);
        (stale || unknown) && !throttled
    }

    async fn refresh(&self) {
        self.cache.write().unwrap().attempted = Some(Instant::now());
        let fetched = async {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build().ok()?;
            let set: serde_json::Value = client.get(&self.jwks_url).send().await.ok()?.json().await.ok()?;
            let jwks: Vec<Jwk> = serde_json::from_value(set.get("keys")?.clone()).ok()?;
            Some(jwks)
        }
        .await;
        match fetched {
            Some(jwks) => {
                let keys: KeyMap = jwks.iter().filter_map(|j| Some((j.kid.clone()?, parse_jwk(j)?))).collect();
                let mut c = self.cache.write().unwrap();
                c.keys = keys;
                c.fetched = Some(Instant::now());
            }
            None => tracing::warn!("auth: récupération JWKS échouée ({})", self.jwks_url),
        }
    }

    pub async fn validate(&self, token: &str) -> Result<Claims, String> {
        let kid = decode_header(token).ok().and_then(|h| h.kid);
        if self.needs_refresh(kid.as_deref()) {
            self.refresh().await;
        }
        let c = self.cache.read().unwrap();
        verify_with_keys(token, &c.keys, &self.issuer, &self.audience)
    }
}

/// `None` tant que LOGTO_ENDPOINT et LOGTO_AUDIENCE ne sont pas tous deux définis : la lecture par JWT reste fermée.
pub fn authenticator() -> Option<&'static Authenticator> {
    static A: OnceLock<Option<Authenticator>> = OnceLock::new();
    A.get_or_init(|| {
        authenticator_from(
            std::env::var("LOGTO_ENDPOINT").ok().as_deref(),
            std::env::var("LOGTO_AUDIENCE").ok().as_deref(),
        )
    })
    .as_ref()
}

/// Authenticator de lys-en-france : même Logto, audience propre (`LYS_LOGTO_AUDIENCE`).
/// Un jeton d'une autre audience (job-dashboard) échoue donc à la validation.
pub fn authenticator_lys() -> Option<Arc<Authenticator>> {
    static A: OnceLock<Option<Arc<Authenticator>>> = OnceLock::new();
    A.get_or_init(|| {
        authenticator_from(
            std::env::var("LOGTO_ENDPOINT").ok().as_deref(),
            std::env::var("LYS_LOGTO_AUDIENCE").ok().as_deref(),
        )
        .map(Arc::new)
    })
    .clone()
}

fn authenticator_from(endpoint: Option<&str>, audience: Option<&str>) -> Option<Authenticator> {
    let endpoint = endpoint.filter(|s| !s.is_empty())?;
    let audience = audience.filter(|s| !s.is_empty())?;
    Some(Authenticator::new(endpoint, audience))
}

/// Liste blanche lys-en-france : `sub` uniquement (jamais d'e-mail) ; variable absente ou vide → personne.
pub fn allowed_lys() -> Arc<Allowed> {
    static A: OnceLock<Arc<Allowed>> = OnceLock::new();
    A.get_or_init(|| Arc::new(allowed_lys_from(std::env::var("LYS_ALLOWED_SUBS").ok().as_deref()))).clone()
}

fn allowed_lys_from(subs: Option<&str>) -> Allowed {
    Allowed::new(subs.unwrap_or(""), "")
}

#[cfg(test)]
mod lys_tests {
    use super::*;

    #[test]
    fn allowed_lys_variable_absente_ou_vide_refuse_tout() {
        let c = Claims { sub: "u1".into(), email: Some("a@b.c".into()) };
        assert!(!allowed_lys_from(None).permits(&c));
        assert!(!allowed_lys_from(Some("")).permits(&c));
        assert!(!allowed_lys_from(Some(" , ")).permits(&c));
        assert!(allowed_lys_from(Some("u1, u2")).permits(&c));
    }

    #[test]
    fn allowed_lys_ignore_les_emails() {
        let c = Claims { sub: "zzz".into(), email: Some("lys@example.com".into()) };
        assert!(!allowed_lys_from(Some("lys@example.com")).permits(&c));
    }

    #[test]
    fn authenticator_lys_exige_endpoint_et_audience() {
        assert!(authenticator_from(None, Some("aud")).is_none());
        assert!(authenticator_from(Some("https://auth.chetana.fr"), None).is_none());
        assert!(authenticator_from(Some("https://auth.chetana.fr"), Some("")).is_none());
        assert!(authenticator_from(Some("https://auth.chetana.fr"), Some("aud")).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde_json::{json, Value};

    const ISS: &str = "https://auth.chetana.fr/oidc";
    const AUD: &str = "https://job-dashboard.chetana.fr/api";
    const SECRET: &[u8] = b"secret-de-test-pour-hs256-uniquement";

    fn keys() -> KeyMap {
        let mut m = KeyMap::new();
        m.insert("k1".into(), (Algorithm::HS256, DecodingKey::from_secret(SECRET)));
        m
    }
    fn exp(delta: i64) -> i64 { chrono::Utc::now().timestamp() + delta }
    fn token_with(claims: Value, kid: Option<&str>, secret: &[u8]) -> String {
        let mut h = Header::new(Algorithm::HS256);
        h.kid = kid.map(String::from);
        encode(&h, &claims, &EncodingKey::from_secret(secret)).unwrap()
    }
    fn good() -> Value { json!({"iss": ISS, "aud": AUD, "sub": "u123", "exp": exp(600), "email": "user@example.com"}) }
    fn check(t: &str) -> Result<Claims, String> { verify_with_keys(t, &keys(), ISS, AUD) }

    #[test]
    fn accepte_un_token_valide() {
        let c = check(&token_with(good(), Some("k1"), SECRET)).unwrap();
        assert_eq!(c.sub, "u123");
        assert_eq!(c.email.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn refuse_mauvais_issuer_audience_expire_ou_sans_sub() {
        let mut c = good(); c["iss"] = json!("https://evil.example/oidc");
        assert!(check(&token_with(c, Some("k1"), SECRET)).is_err());
        let mut c = good(); c["aud"] = json!("autre-api");
        assert!(check(&token_with(c, Some("k1"), SECRET)).is_err());
        let mut c = good(); c["exp"] = json!(exp(-3600));
        assert!(check(&token_with(c, Some("k1"), SECRET)).is_err());
        let mut c = good(); c.as_object_mut().unwrap().remove("sub");
        assert!(check(&token_with(c, Some("k1"), SECRET)).is_err());
        let mut c = good(); c.as_object_mut().unwrap().remove("exp");
        assert!(check(&token_with(c, Some("k1"), SECRET)).is_err());
    }

    #[test]
    fn refuse_signature_fausse_kid_inconnu_ou_absent() {
        assert!(check(&token_with(good(), Some("k1"), b"un-autre-secret-de-meme-longueur!")).is_err());
        assert!(check(&token_with(good(), Some("inconnu"), SECRET)).is_err());
        assert!(check(&token_with(good(), None, SECRET)).is_err());
    }

    #[test]
    fn refuse_alg_none_et_charabia() {
        use base64::Engine;
        let b = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let none = format!("{}.{}.", b.encode(r#"{"alg":"none","kid":"k1"}"#), b.encode(good().to_string()));
        assert!(check(&none).is_err());
        assert!(check("pas.un.jwt").is_err());
        assert!(check("").is_err());
    }

    #[test]
    fn liste_blanche_par_sub_ou_email() {
        let c = |sub: &str, email: Option<&str>| Claims { sub: sub.into(), email: email.map(String::from) };
        let a = Allowed::new("u123, u456", "User@Example.com");
        assert!(a.permits(&c("u123", None)));
        assert!(a.permits(&c("zzz", Some("user@example.com"))));
        assert!(!a.permits(&c("zzz", Some("intrus@gmail.com"))));
        assert!(!a.permits(&c("zzz", None)));
    }

    #[test]
    fn liste_vide_refuse_tout() {
        let a = Allowed::new("", " , ");
        assert!(!a.permits(&Claims { sub: "".into(), email: Some("".into()) }));
        assert!(!a.permits(&Claims { sub: "u1".into(), email: None }));
    }

    #[test]
    fn jwks_reel_de_auth_chetana_fr_ec_p384() {
        let j: Jwk = serde_json::from_str(r#"{"kid": "-SBdewyVnBpMp3IdiRBh6_M749kz-ENz8Dvqn7dSXdg", "kty": "EC", "crv": "P-384", "x": "7DG65f3xkGAn4eXCbuxpDUNr5YytPRsc7F1_DG-_Dc4ajfFLpZhfpcMEMWP1SV_-", "y": "KAxQcw5ozoSquG1YNy-8IuO7VH66snn6pO5BGooVZMVR2-MXGMGHFs-xv8yGMrXm"}"#).unwrap();
        let (alg, _) = parse_jwk(&j).expect("clé EC P-384 refusée");
        assert_eq!(alg, Algorithm::ES384);
    }

    #[test]
    fn jwk_inconnu_ou_incomplet_ignore() {
        let j: Jwk = serde_json::from_str(r#"{"kid":"a","kty":"oct"}"#).unwrap();
        assert!(parse_jwk(&j).is_none());
        let j: Jwk = serde_json::from_str(r#"{"kid":"a","kty":"EC","crv":"P-521","x":"AA","y":"AA"}"#).unwrap();
        assert!(parse_jwk(&j).is_none());
    }

    #[test]
    fn extrait_le_bearer() {
        let mut h = HeaderMap::new();
        assert_eq!(bearer(&h), None);
        h.insert("authorization", HeaderValue::from_static("Bearer abc.def.ghi"));
        assert_eq!(bearer(&h), Some("abc.def.ghi"));
        h.insert("authorization", HeaderValue::from_static("Basic abc"));
        assert_eq!(bearer(&h), None);
        h.insert("authorization", HeaderValue::from_static("Bearer "));
        assert_eq!(bearer(&h), None);
    }
}
