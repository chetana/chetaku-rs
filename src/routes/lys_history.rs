//! lys-en-france : historique PARTAGÉ du chat. Un objet JSON par échange dans le bucket privé des documents,
//! clé `chat/AAAA/MM/JJ/<uuid>.json` (jour de Paris). Enregistrement au mieux : une panne de stockage ne fait
//! jamais échouer `POST /lys/chat`. Ni question ni réponse dans les journaux ; `author_sub` ne sort jamais.

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, SecondsFormat, Utc};
use s3::Bucket;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::Claims;
use crate::routes::lys_chat::{paris_today, ChatError};

pub const DEFAULT_LIMIT: usize = 50;
pub const MAX_LIMIT: usize = 50;
/// Profondeur de lecture : au-delà, les échanges restent stockés mais ne sont plus listés.
pub const LOOKBACK_DAYS: i64 = 30;

// ── Modèle et règles pures ───────────────────────────────────────────────────

/// Contenu de l'objet stocké.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Exchange {
    pub id: String,
    pub created_at: String,
    pub author_sub: String,
    pub question: String,
    pub answer_fr: String,
    pub answer_km: Option<String>,
    pub task_slugs: Vec<String>,
}

/// Forme publique : jamais d'`author_sub`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub created_at: String,
    pub question: String,
    pub answer_fr: String,
    pub answer_km: Option<String>,
    pub task_slugs: Vec<String>,
    pub mine: bool,
}

pub fn to_item(e: Exchange, id: String, viewer_sub: &str) -> HistoryItem {
    HistoryItem {
        mine: e.author_sub == viewer_sub,
        id,
        created_at: e.created_at,
        question: e.question,
        answer_fr: e.answer_fr,
        answer_km: e.answer_km,
        task_slugs: e.task_slugs,
    }
}

pub fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `AAAA-MM-JJ_<uuid>`, jour de Paris.
pub fn make_id(day: NaiveDate, uuid: Uuid) -> String {
    format!("{}_{}", day.format("%Y-%m-%d"), uuid.hyphenated())
}

fn lower_uuid(s: &str) -> Option<Uuid> {
    let canonical = s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_digit() || ('a'..='f').contains(&c),
        });
    if canonical { Uuid::parse_str(s).ok() } else { None }
}

/// Validation stricte : `\d{4}-\d{2}-\d{2}_<uuid minuscule canonique>` et date réelle. Rien d'autre.
pub fn parse_id(id: &str) -> Option<(NaiveDate, Uuid)> {
    if id.len() != 47 || !id.is_ascii() {
        return None;
    }
    let (day, rest) = id.split_at(10);
    let uuid = lower_uuid(rest.strip_prefix('_')?)?;
    let shape = day.char_indices().all(|(i, c)| if i == 4 || i == 7 { c == '-' } else { c.is_ascii_digit() });
    if !shape {
        return None;
    }
    Some((NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?, uuid))
}

pub fn key_for(day: NaiveDate, uuid: Uuid) -> String {
    format!("{}{}.json", day_prefix(day), uuid.hyphenated())
}

pub fn day_prefix(day: NaiveDate) -> String {
    format!("chat/{}/", day.format("%Y/%m/%d"))
}

/// Clé d'un id public ; `None` si l'id est invalide (aucune clé n'est construite alors).
pub fn key_from_id(id: &str) -> Option<String> {
    parse_id(id).map(|(d, u)| key_for(d, u))
}

/// Id public d'une clé listée ; `None` pour tout ce qui n'a pas exactement la forme attendue pour ce jour.
pub fn id_from_key(day: NaiveDate, key: &str) -> Option<String> {
    let stem = key.strip_prefix(day_prefix(day).as_str())?.strip_suffix(".json")?;
    lower_uuid(stem).map(|u| make_id(day, u))
}

/// Du plus récent au plus ancien (départage stable par id), borné à `limit`.
pub fn sort_and_limit(mut items: Vec<HistoryItem>, limit: usize) -> Vec<HistoryItem> {
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
    items.truncate(limit);
    items
}

pub fn clamp_limit(raw: Option<&str>) -> Result<usize, &'static str> {
    match raw {
        None => Ok(DEFAULT_LIMIT),
        Some(s) => s.trim().parse::<usize>().map(|n| n.clamp(1, MAX_LIMIT)).map_err(|_| "limit invalide"),
    }
}

// ── Stockage d'objets ────────────────────────────────────────────────────────

#[derive(Clone)]
enum Backend {
    S3(&'static Bucket),
    #[cfg(test)]
    Memory(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>>),
    #[cfg(test)]
    Broken,
}

pub struct StoreError;

/// Historique des échanges. `None` = stockage non configuré : enregistrement ignoré, lecture vide.
#[derive(Clone)]
pub struct ChatHistory(Option<Backend>);

impl Backend {
    async fn put(&self, key: &str, body: &[u8]) -> Result<(), StoreError> {
        match self {
            Backend::S3(b) => b.put_object_with_content_type(key, body, "application/json").await.map(|_| ()).map_err(|_| StoreError),
            #[cfg(test)]
            Backend::Memory(m) => {
                m.lock().unwrap().insert(key.to_string(), body.to_vec());
                Ok(())
            }
            #[cfg(test)]
            Backend::Broken => Err(StoreError),
        }
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StoreError> {
        match self {
            Backend::S3(b) => match b.get_object(key).await {
                Ok(r) if r.status_code() == 200 => Ok(Some(r.bytes().to_vec())),
                Ok(r) if r.status_code() == 404 => Ok(None),
                Ok(_) => Err(StoreError),
                Err(e) => {
                    let m = e.to_string();
                    if m.contains("404") || m.contains("NoSuchKey") { Ok(None) } else { Err(StoreError) }
                }
            },
            #[cfg(test)]
            Backend::Memory(m) => Ok(m.lock().unwrap().get(key).cloned()),
            #[cfg(test)]
            Backend::Broken => Err(StoreError),
        }
    }

    async fn delete(&self, key: &str) -> Result<(), StoreError> {
        match self {
            Backend::S3(b) => b.delete_object(key).await.map(|_| ()).map_err(|_| StoreError),
            #[cfg(test)]
            Backend::Memory(m) => {
                m.lock().unwrap().remove(key);
                Ok(())
            }
            #[cfg(test)]
            Backend::Broken => Err(StoreError),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StoreError> {
        match self {
            Backend::S3(b) => {
                let pages = b.list(prefix.to_string(), None).await.map_err(|_| StoreError)?;
                Ok(pages.into_iter().flat_map(|p| p.contents).map(|o| o.key).collect())
            }
            #[cfg(test)]
            Backend::Memory(m) => Ok(m.lock().unwrap().keys().filter(|k| k.starts_with(prefix)).cloned().collect()),
            #[cfg(test)]
            Backend::Broken => Err(StoreError),
        }
    }
}

impl ChatHistory {
    /// Même bucket privé et même client que les documents.
    pub fn from_env() -> Self {
        Self(crate::s3cache::lys_bucket().map(Backend::S3))
    }

    pub fn disabled() -> Self {
        Self(None)
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self(Some(Backend::Memory(Default::default())))
    }

    #[cfg(test)]
    pub fn broken() -> Self {
        Self(Some(Backend::Broken))
    }

    #[cfg(test)]
    pub fn object_count(&self) -> usize {
        match &self.0 {
            Some(Backend::Memory(m)) => m.lock().unwrap().len(),
            _ => 0,
        }
    }

    #[cfg(test)]
    pub fn put_raw_for_test(&self, key: &str, body: &[u8]) {
        if let Some(Backend::Memory(m)) = &self.0 {
            m.lock().unwrap().insert(key.to_string(), body.to_vec());
        }
    }

    /// Enregistre un échange ; renvoie `(id, created_at)` ou `None` (non configuré ou en échec, journalisé sans contenu).
    pub async fn record(&self, now: DateTime<Utc>, author_sub: &str, question: &str, answer_fr: &str, answer_km: Option<&str>, task_slugs: &[String]) -> Option<(String, String)> {
        let backend = self.0.as_ref()?;
        let day = paris_today(now);
        let uuid = Uuid::new_v4();
        let id = make_id(day, uuid);
        let created_at = rfc3339(now);
        let exchange = Exchange {
            id: id.clone(),
            created_at: created_at.clone(),
            author_sub: author_sub.to_string(),
            question: question.to_string(),
            answer_fr: answer_fr.to_string(),
            answer_km: answer_km.map(str::to_string),
            task_slugs: task_slugs.to_vec(),
        };
        let Ok(body) = serde_json::to_vec(&exchange) else {
            tracing::warn!("lys chat: historique, sérialisation impossible");
            return None;
        };
        if backend.put(&key_for(day, uuid), &body).await.is_err() {
            tracing::warn!("lys chat: historique, enregistrement impossible ({id})");
            return None;
        }
        Some((id, created_at))
    }

    /// Les `limit` derniers échanges, jour par jour en remontant (jour de Paris) sur `LOOKBACK_DAYS` jours.
    /// Un jour est lu en entier avant de trier : les clés portent un uuid aléatoire, pas l'heure.
    pub async fn recent(&self, now: DateTime<Utc>, viewer_sub: &str, limit: usize) -> Result<Vec<HistoryItem>, StoreError> {
        let Some(backend) = self.0.as_ref() else { return Ok(vec![]) };
        let today = paris_today(now);
        let mut items: Vec<HistoryItem> = Vec::new();
        for back in 0..=LOOKBACK_DAYS {
            let day = today - ChronoDuration::days(back);
            for key in backend.list(&day_prefix(day)).await? {
                let Some(id) = id_from_key(day, &key) else { continue };
                match backend.get(&key).await {
                    Ok(Some(bytes)) => match serde_json::from_slice::<Exchange>(&bytes) {
                        Ok(e) => items.push(to_item(e, id, viewer_sub)),
                        Err(_) => tracing::warn!("lys chat: historique, objet illisible ignoré ({id})"),
                    },
                    Ok(None) => {}
                    Err(_) => tracing::warn!("lys chat: historique, lecture impossible ({id})"),
                }
            }
            if items.len() >= limit {
                break;
            }
        }
        Ok(sort_and_limit(items, limit))
    }

    /// `Ok(true)` supprimé, `Ok(false)` absent.
    pub async fn delete(&self, id: &str) -> Result<bool, StoreError> {
        let Some(backend) = self.0.as_ref() else { return Err(StoreError) };
        let key = key_from_id(id).ok_or(StoreError)?;
        if backend.get(&key).await?.is_none() {
            return Ok(false);
        }
        backend.delete(&key).await?;
        Ok(true)
    }

    pub fn is_enabled(&self) -> bool {
        self.0.is_some()
    }
}

// ── Routes ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    limit: Option<String>,
}

/// `GET /lys/chat/history?limit=N`
pub async fn list(
    Extension(history): Extension<ChatHistory>,
    Extension(claims): Extension<Claims>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Value>, ChatError> {
    let limit = clamp_limit(q.limit.as_deref()).map_err(ChatError::Invalid)?;
    let items = history.recent(Utc::now(), &claims.sub, limit).await.map_err(|_| {
        tracing::warn!("lys chat: historique, lecture en erreur");
        ChatError::Unavailable
    })?;
    Ok(Json(json!({ "items": items })))
}

/// `DELETE /lys/chat/history/{id}` : l'id est validé avant toute clé S3.
pub async fn delete(Extension(history): Extension<ChatHistory>, Path(id): Path<String>) -> Result<Response, ChatError> {
    if parse_id(&id).is_none() {
        return Err(ChatError::Invalid("id invalide"));
    }
    if !history.is_enabled() {
        return Err(ChatError::Unavailable);
    }
    match history.delete(&id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT.into_response()),
        Ok(false) => Err(ChatError::NotFound),
        Err(_) => {
            tracing::warn!("lys chat: historique, suppression en erreur ({id})");
            Err(ChatError::Unavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "0b9e1c52-3f0a-4c1e-9a55-6d2f7a1b8c34";

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn ex(created: &str, author: &str) -> Exchange {
        Exchange {
            id: "x".into(),
            created_at: created.into(),
            author_sub: author.into(),
            question: "Q ?".into(),
            answer_fr: "R".into(),
            answer_km: None,
            task_slugs: vec!["vls-ts-anef".into()],
        }
    }

    #[test]
    fn id_et_cle_construits_et_inverses() {
        let u: Uuid = U.parse().unwrap();
        let id = make_id(d("2026-10-28"), u);
        assert_eq!(id, format!("2026-10-28_{U}"));
        assert_eq!(parse_id(&id), Some((d("2026-10-28"), u)));
        assert_eq!(key_from_id(&id).unwrap(), format!("chat/2026/10/28/{U}.json"));
        assert_eq!(id_from_key(d("2026-10-28"), &format!("chat/2026/10/28/{U}.json")), Some(id));
        assert_eq!(id_from_key(d("2026-10-29"), &format!("chat/2026/10/28/{U}.json")), None, "autre jour");
    }

    #[test]
    fn id_invalide_refuse_sans_cle() {
        for bad in [
            "",
            "../../etc/passwd",
            &format!("2026-10-28_{U}/../x"),
            &format!("2026-10-28/{U}"),
            &format!("2026-10-28_{}", U.to_uppercase()),
            &format!("2026-13-28_{U}"),
            &format!("2026-02-30_{U}"),
            &format!("26-10-28_{U}"),
            &format!("2026-10-28_{U}.json"),
            &format!(" 2026-10-28_{U}"),
            &format!("2026-10-28_{}", U.replace('-', "")),
            "2026-10-28_../../../../chat/2026/10/28/aa",
            &format!("2026-10-28_{}%2e", &U[..35]),
            &format!("２０２６-10-28_{U}"),
        ] {
            assert!(parse_id(bad).is_none(), "{bad:?} aurait dû être refusé");
            assert!(key_from_id(bad).is_none());
        }
    }

    #[test]
    fn id_from_key_refuse_ce_qui_n_a_pas_la_forme() {
        let day = d("2026-10-28");
        for bad in ["chat/2026/10/28/", "chat/2026/10/28/x.json", "chat/2026/10/28/a/b.json", &format!("chat/2026/10/28/{U}.txt")] {
            assert_eq!(id_from_key(day, bad), None, "{bad}");
        }
    }

    #[test]
    fn serialisation_publique_sans_author_sub() {
        let item = to_item(ex("2026-10-28T10:00:00Z", "SUB-SECRET"), "id1".into(), "autre");
        let s = serde_json::to_string(&item).unwrap();
        assert!(!s.contains("SUB-SECRET") && !s.contains("author"), "{s}");
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["mine"], false);
        assert!(v["answer_km"].is_null());
        assert_eq!(v["task_slugs"], json!(["vls-ts-anef"]));
    }

    #[test]
    fn mine_selon_le_sub_du_jeton() {
        assert!(to_item(ex("t", "lys1"), "i".into(), "lys1").mine);
        assert!(!to_item(ex("t", "lys1"), "i".into(), "lys2").mine);
    }

    #[test]
    fn tri_recent_d_abord_et_limite() {
        let it = |id: &str, t: &str| to_item(ex(t, "a"), id.into(), "a");
        let out = sort_and_limit(
            vec![it("b", "2026-10-28T10:00:00Z"), it("c", "2026-10-29T08:00:00Z"), it("a", "2026-10-28T10:00:00Z"), it("d", "2026-10-27T23:00:00Z")],
            3,
        );
        let ids: Vec<&str> = out.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["c", "b", "a"], "plus récent d'abord, égalité départagée par id, borné à 3");
    }

    #[test]
    fn limite_par_defaut_max_et_invalide() {
        assert_eq!(clamp_limit(None), Ok(50));
        assert_eq!(clamp_limit(Some("10")), Ok(10));
        assert_eq!(clamp_limit(Some("500")), Ok(50));
        assert_eq!(clamp_limit(Some("0")), Ok(1));
        assert!(clamp_limit(Some("abc")).is_err());
        assert!(clamp_limit(Some("-3")).is_err());
    }

    #[test]
    fn horodatage_rfc3339_utc() {
        assert_eq!(rfc3339("2026-10-28T10:00:00.123Z".parse().unwrap()), "2026-10-28T10:00:00Z");
    }

    #[tokio::test]
    async fn enregistre_puis_relit_par_jour_de_paris() {
        let h = ChatHistory::in_memory();
        // 23h30 UTC le 28 = 00h30 le 29 à Paris : la clé porte le jour de Paris.
        let now: DateTime<Utc> = "2026-10-28T23:30:00Z".parse().unwrap();
        let (id, created) = h.record(now, "lys1", "Q", "R", Some("K"), &[]).await.unwrap();
        assert!(id.starts_with("2026-10-29_"), "{id}");
        assert_eq!(created, "2026-10-28T23:30:00Z");
        let items = h.recent(now, "lys1", 50).await.ok().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].id.as_str(), items[0].mine, items[0].answer_km.as_deref()), (id.as_str(), true, Some("K")));
    }

    #[tokio::test]
    async fn lecture_bornee_a_30_jours_et_ignore_les_objets_illisibles() {
        let h = ChatHistory::in_memory();
        let now: DateTime<Utc> = "2026-10-28T10:00:00Z".parse().unwrap();
        let vieux: DateTime<Utc> = "2026-09-20T10:00:00Z".parse().unwrap(); // 38 jours
        let limite: DateTime<Utc> = "2026-09-28T10:00:00Z".parse().unwrap(); // 30 jours
        h.record(vieux, "a", "vieux", "R", None, &[]).await.unwrap();
        h.record(limite, "a", "limite", "R", None, &[]).await.unwrap();
        h.put_raw_for_test(&format!("chat/2026/10/28/{U}.json"), b"{pas du json SECRET-CONTENU");
        let items = h.recent(now, "a", 50).await.ok().unwrap();
        let qs: Vec<&str> = items.iter().map(|i| i.question.as_str()).collect();
        assert_eq!(qs, ["limite"]);
    }

    #[tokio::test]
    async fn supprime_et_signale_l_absent() {
        let h = ChatHistory::in_memory();
        let (id, _) = h.record(Utc::now(), "a", "Q", "R", None, &[]).await.unwrap();
        assert!(h.delete(&id).await.ok().unwrap());
        assert!(!h.delete(&id).await.ok().unwrap());
        assert_eq!(h.object_count(), 0);
    }
}
