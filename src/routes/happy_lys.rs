//! Synchronisation minimale pour happy-lys-2026.chetana.fr (site cadeau statique) :
//! l'état des « bons à valoir » cochés + le souhait libre, dans UN objet S3 fixe.
//!
//! Pas d'authentification utilisateur : l'URL contient un secret (`HAPPY_LYS_TOKEN`, comparé en temps
//! constant) — il évite les bots mais pas quelqu'un qui lirait le JS du site, d'où des garde-fous :
//! clé S3 en dur (aucune clé issue de l'utilisateur), schéma strict, corps ≤ 2 Ko, texte ≤ 300
//! caractères, et le rate-limit global par IP de main.rs. Sans `HAPPY_LYS_TOKEN`, les routes répondent 404.
//! Aucune base de données touchée (la Serverless SQL ne se réveille pas).

use axum::{
    body::Bytes,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{json, Map, Value};

use crate::s3cache;

const KEY: &str = "happy-lys/state.json";
pub const MAX_BODY: usize = 2048;
const MAX_TXT_CHARS: usize = 300;
const MAX_KEYS: usize = 20;

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn token_ok(given: &str) -> bool {
    match std::env::var("HAPPY_LYS_TOKEN") {
        Ok(t) if t.len() >= 16 => ct_eq(t.as_bytes(), given.as_bytes()),
        _ => false,
    }
}

fn is_bon_key(k: &str) -> bool {
    let rest = match k.strip_prefix('b') { Some(r) => r, None => return false };
    (1..=2).contains(&rest.len()) && rest.bytes().all(|c| c.is_ascii_digit())
}

/// Valide et normalise l'état envoyé par le navigateur. Champs autorisés :
/// `b0`..`b99` (bool : bon utilisé), `txt` (souhait libre), `t` (horodatage client en ms).
pub fn sanitize(body: &[u8]) -> Result<Map<String, Value>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "JSON invalide".to_string())?;
    let obj = v.as_object().ok_or_else(|| "objet attendu".to_string())?;
    if obj.len() > MAX_KEYS {
        return Err("trop de champs".into());
    }
    let mut out = Map::new();
    for (k, val) in obj {
        if is_bon_key(k) {
            out.insert(k.clone(), Value::Bool(val.as_bool().ok_or_else(|| format!("{k}: booléen attendu"))?));
        } else if k == "txt" {
            let t: String = val.as_str().ok_or_else(|| "txt: texte attendu".to_string())?
                .chars().filter(|c| !c.is_control() || *c == '\n').collect();
            let t = t.trim().to_string();
            if t.chars().count() > MAX_TXT_CHARS {
                return Err("txt trop long".into());
            }
            out.insert("txt".into(), Value::String(t));
        } else if k == "t" {
            out.insert("t".into(), json!(val.as_u64().ok_or_else(|| "t: entier attendu".to_string())?));
        } else {
            return Err(format!("champ inconnu: {k}"));
        }
    }
    Ok(out)
}

fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-store")], body).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    json_response(status, json!({ "error": msg }).to_string().into_bytes())
}

pub async fn get_state(Path(token): Path<String>) -> Response {
    if !token_ok(&token) { return err(StatusCode::NOT_FOUND, "not found"); }
    match s3cache::get_raw(KEY).await {
        Ok(Some(b)) => json_response(StatusCode::OK, b),
        Ok(None) => json_response(StatusCode::OK, b"{}".to_vec()),
        Err(e) => { tracing::warn!("happy-lys: lecture S3 échouée: {e}"); err(StatusCode::SERVICE_UNAVAILABLE, "stockage indisponible") }
    }
}

pub async fn put_state(Path(token): Path<String>, body: Bytes) -> Response {
    if !token_ok(&token) { return err(StatusCode::NOT_FOUND, "not found"); }
    if body.len() > MAX_BODY { return err(StatusCode::PAYLOAD_TOO_LARGE, "corps trop gros"); }
    let new = match sanitize(&body) { Ok(m) => m, Err(m) => return err(StatusCode::BAD_REQUEST, &m) };

    // Un état plus ancien ne doit jamais écraser un plus récent (deux appareils, requête en retard…) :
    // on renvoie alors l'état stocké, que le client adopte.
    let new_t = new.get("t").and_then(|v| v.as_u64()).unwrap_or(0);
    if let Ok(Some(old_raw)) = s3cache::get_raw(KEY).await {
        if let Ok(old) = serde_json::from_slice::<Value>(&old_raw) {
            if old.get("t").and_then(|v| v.as_u64()).unwrap_or(0) > new_t {
                return json_response(StatusCode::OK, old_raw);
            }
        }
    }
    let out = serde_json::to_vec(&Value::Object(new.clone())).unwrap_or_default();
    if let Err(e) = s3cache::put_raw(KEY, &out).await {
        tracing::warn!("happy-lys: écriture S3 échouée: {e}");
        return err(StatusCode::SERVICE_UNAVAILABLE, "stockage indisponible");
    }
    let used = new.iter().filter(|(k, v)| is_bon_key(k) && v.as_bool() == Some(true)).count();
    tracing::info!("happy-lys: état mis à jour — {used} bon(s) utilisé(s), souhait: {:?}", new.get("txt").and_then(|v| v.as_str()).unwrap_or(""));
    json_response(StatusCode::OK, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepte_un_etat_valide() {
        let m = sanitize(r#"{"b0":true,"b3":false,"txt":" un voyage à Paris ","t":1790000000000}"#.as_bytes()).unwrap();
        assert_eq!(m["b0"], Value::Bool(true));
        assert_eq!(m["b3"], Value::Bool(false));
        assert_eq!(m["txt"], "un voyage à Paris");
        assert_eq!(m["t"], 1790000000000u64);
    }
    #[test]
    fn accepte_un_etat_vide() { assert!(sanitize(b"{}").unwrap().is_empty()); }
    #[test]
    fn refuse_les_champs_inconnus() {
        assert!(sanitize(br#"{"admin":true}"#).is_err());
        assert!(sanitize(br#"{"b":true}"#).is_err());
        assert!(sanitize(br#"{"b123":true}"#).is_err());
        assert!(sanitize(br#"{"bx":true}"#).is_err());
    }
    #[test]
    fn refuse_les_mauvais_types() {
        assert!(sanitize(br#"{"b0":"oui"}"#).is_err());
        assert!(sanitize(br#"{"txt":42}"#).is_err());
        assert!(sanitize(br#"{"t":"hier"}"#).is_err());
        assert!(sanitize(br#"{"t":-1}"#).is_err());
        assert!(sanitize(b"[1,2]").is_err());
        assert!(sanitize(b"pas du json").is_err());
    }
    #[test]
    fn borne_le_texte_en_caracteres_pas_en_octets() {
        let ok = format!(r#"{{"txt":"{}"}}"#, "ក".repeat(300));          // 300 caractères khmers = 900 octets
        assert!(sanitize(ok.as_bytes()).is_ok());
        let ko = format!(r#"{{"txt":"{}"}}"#, "ក".repeat(301));
        assert!(sanitize(ko.as_bytes()).is_err());
    }
    #[test]
    fn retire_les_caracteres_de_controle() {
        let m = sanitize(br#"{"txt":"a\u0000b\u0007c"}"#).unwrap();
        assert_eq!(m["txt"], "abc");
    }
    #[test]
    fn refuse_trop_de_champs() {
        let body = format!("{{{}}}", (0..21).map(|i| format!(r#""b{i}":true"#)).collect::<Vec<_>>().join(","));
        assert!(sanitize(body.as_bytes()).is_err());
    }
    #[test]
    fn comparaison_du_token() {
        assert!(ct_eq(b"abcdef", b"abcdef"));
        assert!(!ct_eq(b"abcdef", b"abcdeg"));
        assert!(!ct_eq(b"abc", b"abcd"));
    }
}
