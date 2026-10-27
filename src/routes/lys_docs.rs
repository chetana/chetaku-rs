//! lys-en-france : documents (scans) d'une tâche. Stockage dans un bucket S3 PRIVÉ ; la lecture passe par une
//! URL signée courte, générée seulement après le garde d'auth du routeur `/lys`. Jamais d'URL permanente,
//! jamais de listing du bucket. Aucune erreur S3 n'est renvoyée au client (journal seulement).

use axum::{
    extract::{FromRequest, Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use s3::Bucket;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::auth::Claims;
use crate::error::AppError;

/// Taille maximale d'un envoi (corps brut) : 12 Mio, uniquement sur la route d'envoi.
pub const MAX_UPLOAD: usize = 12 * 1024 * 1024;
/// Validité d'une URL de lecture en production.
pub const URL_TTL_SECS: u32 = 300;
const MAX_CONCURRENT_UPLOADS: usize = 3;
const MAX_FILENAME_CHARS: usize = 120;

/// Stockage des documents, partagé par les trois routes. `bucket: None` = non configuré → 503.
#[derive(Clone)]
pub struct DocStore {
    bucket: Option<&'static Bucket>,
    url_ttl_secs: u32,
    permits: Arc<Semaphore>,
}

impl DocStore {
    pub fn new(bucket: Option<&'static Bucket>, url_ttl_secs: u32) -> Self {
        Self { bucket, url_ttl_secs, permits: Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS)) }
    }

    pub fn from_env() -> Self {
        Self::new(crate::s3cache::lys_bucket(), URL_TTL_SECS)
    }

    pub fn disabled() -> Self {
        Self::new(None, URL_TTL_SECS)
    }

    fn bucket(&self) -> Result<&'static Bucket, AppError> {
        self.bucket.ok_or(AppError::Unavailable)
    }
}

// ── Règles pures ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileKind {
    pub ext: &'static str,
    pub mime: &'static str,
}

const HEIC_BRANDS: [&[u8; 4]; 8] = [b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"hevm", b"hevs"];
const HEIF_BRANDS: [&[u8; 4]; 3] = [b"mif1", b"msf1", b"heif"];

/// Type d'un fichier d'après ses PREMIERS OCTETS uniquement (ni extension ni Content-Type du client).
pub fn detect_kind(b: &[u8]) -> Option<FileKind> {
    if b.starts_with(b"%PDF-") {
        return Some(FileKind { ext: "pdf", mime: "application/pdf" });
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(FileKind { ext: "jpg", mime: "image/jpeg" });
    }
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some(FileKind { ext: "png", mime: "image/png" });
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        let brand: [u8; 4] = b[8..12].try_into().ok()?;
        if HEIC_BRANDS.iter().any(|h| **h == brand) {
            return Some(FileKind { ext: "heic", mime: "image/heic" });
        }
        if HEIF_BRANDS.iter().any(|h| **h == brand) {
            return Some(FileKind { ext: "heif", mime: "image/heif" });
        }
    }
    None
}

/// Clé objet : jamais dérivée du nom d'origine.
pub fn object_key(task_slug: &str, id: Uuid, ext: &str) -> String {
    format!("docs/{task_slug}/{id}.{ext}")
}

/// UUID strict : forme canonique `8-4-4-4-12` en hexadécimal, sans accolades, sans `urn:`, sans variante compacte.
pub fn parse_doc_id(s: &str) -> Option<Uuid> {
    let canonical = s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    if canonical { Uuid::parse_str(s).ok() } else { None }
}

fn is_invisible_format_char(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Nom affichable : dernier segment de chemin, sans contrôle ni caractère bidirectionnel, sans caractère
/// réservé des systèmes de fichiers, borné. Sert à l'affichage et au téléchargement, jamais à la clé.
pub fn sanitize_filename(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .filter(|c| !c.is_control() && !is_invisible_format_char(*c))
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let bounded: String = trimmed.chars().take(MAX_FILENAME_CHARS).collect();
    let bounded = bounded.trim().to_string();
    if bounded.is_empty() { "document".to_string() } else { bounded }
}

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `Content-Disposition` imposé à la lecture : téléchargement, jamais affichage dans le navigateur.
pub fn content_disposition(filename: &str) -> String {
    let ascii: String = filename
        .chars()
        .map(|c| if c.is_ascii() && !c.is_ascii_control() && c != '"' && c != '\\' { c } else { '_' })
        .collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{}", pct_encode(filename))
}

// ── Modèle ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct DocumentOut {
    pub id: Uuid,
    #[serde(skip)]
    pub task_slug: String,
    pub item_slug: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub uploaded_by: String,
    pub uploaded_at: DateTime<Utc>,
}

pub const DOC_COLS: &str = "id, task_slug, item_slug, filename, content_type, size_bytes, uploaded_by, uploaded_at";

/// Documents non supprimés, dans l'ordre d'envoi.
pub async fn list_active(tx: &mut Transaction<'_, Postgres>, task_slug: Option<&str>) -> Result<Vec<DocumentOut>, AppError> {
    let rows = sqlx::query_as(&format!(
        "SELECT {DOC_COLS} FROM lys_fr.document WHERE deleted_at IS NULL AND ($1::text IS NULL OR task_slug = $1) \
         ORDER BY uploaded_at, id"
    ))
    .bind(task_slug)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UploadQuery {
    item: Option<String>,
    filename: Option<String>,
}

fn storage_error(what: &str, e: impl std::fmt::Display) -> AppError {
    tracing::error!("lys: stockage ({what}) en erreur: {e}");
    AppError::Storage
}

/// `POST /lys/tasks/{slug}/documents?item=&filename=` : corps BRUT = octets du fichier.
pub async fn upload(
    State(pool): State<PgPool>,
    Extension(store): Extension<DocStore>,
    Extension(claims): Extension<Claims>,
    Path(task_slug): Path<String>,
    Query(q): Query<UploadQuery>,
    req: Request,
) -> Result<Response, AppError> {
    let bucket = store.bucket()?;
    let task_exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM lys_fr.task WHERE slug = $1")
        .bind(&task_slug)
        .fetch_optional(&pool)
        .await?;
    task_exists.ok_or(AppError::NotFound)?;
    if let Some(item) = &q.item {
        let owner: Option<String> = sqlx::query_scalar("SELECT task_slug FROM lys_fr.item WHERE slug = $1")
            .bind(item)
            .fetch_optional(&pool)
            .await?;
        if owner.as_deref() != Some(task_slug.as_str()) {
            return Err(AppError::NotFound);
        }
    }
    // Le corps n'est lu qu'une fois le créneau obtenu : au plus 3 fichiers en mémoire à la fois.
    let _permit = store.permits.acquire().await.map_err(|e| storage_error("semaphore", e))?;
    let body = axum::body::Bytes::from_request(req, &()).await.map_err(|r| {
        if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
            AppError::PayloadTooLarge
        } else {
            AppError::Validation("corps illisible".into())
        }
    })?;
    if body.len() > MAX_UPLOAD {
        return Err(AppError::PayloadTooLarge);
    }
    if body.is_empty() {
        return Err(AppError::Validation("fichier vide".into()));
    }
    let kind = detect_kind(&body).ok_or(AppError::UnsupportedMediaType)?;
    let filename = sanitize_filename(q.filename.as_deref().unwrap_or(""));
    let id = Uuid::new_v4();
    let key = object_key(&task_slug, id, kind.ext);

    let put = bucket
        .put_object_with_content_type(&key, &body, kind.mime)
        .await
        .map_err(|e| storage_error("put", e))?;
    if put.status_code() / 100 != 2 {
        return Err(storage_error("put", format!("statut {}", put.status_code())));
    }
    let inserted: Result<DocumentOut, sqlx::Error> = sqlx::query_as(&format!(
        "INSERT INTO lys_fr.document (id, task_slug, item_slug, s3_key, filename, content_type, size_bytes, uploaded_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {DOC_COLS}"
    ))
    .bind(id)
    .bind(&task_slug)
    .bind(&q.item)
    .bind(&key)
    .bind(&filename)
    .bind(kind.mime)
    .bind(body.len() as i64)
    .bind(&claims.sub)
    .fetch_one(&pool)
    .await;
    match inserted {
        Ok(doc) => Ok((StatusCode::CREATED, Json(json!({ "document": doc }))).into_response()),
        Err(e) => {
            // Pas d'objet orphelin : la ligne n'existe pas, l'objet non plus.
            if let Err(d) = bucket.delete_object(&key).await {
                tracing::error!("lys: nettoyage de l'objet orphelin en erreur: {d}");
            }
            Err(e.into())
        }
    }
}

fn strict_id(raw: &str) -> Result<Uuid, AppError> {
    parse_doc_id(raw).ok_or_else(|| AppError::Validation("identifiant de document invalide".into()))
}

/// `GET /lys/documents/{id}/url` : URL signée de lecture (téléchargement), courte durée.
pub async fn read_url(
    State(pool): State<PgPool>,
    Extension(store): Extension<DocStore>,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    let id = strict_id(&raw_id)?;
    let bucket = store.bucket()?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT s3_key, filename FROM lys_fr.document WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&pool)
            .await?;
    let (key, filename) = row.ok_or(AppError::NotFound)?;
    let queries = HashMap::from([("response-content-disposition".to_string(), content_disposition(&filename))]);
    let url = bucket
        .presign_get(&key, store.url_ttl_secs, Some(queries))
        .await
        .map_err(|e| storage_error("presign", e))?;
    let mut resp = Json(json!({ "url": url, "expires_in": store.url_ttl_secs })).into_response();
    resp.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(resp)
}

/// `DELETE /lys/documents/{id}` : suppression douce en base + `delete_object` (le bucket versionné garde la
/// version). Idempotent : refaire l'appel sur un document déjà supprimé réessaie seulement l'objet.
pub async fn delete(
    State(pool): State<PgPool>,
    Extension(store): Extension<DocStore>,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    let id = strict_id(&raw_id)?;
    let bucket = store.bucket()?;
    let key: String = sqlx::query_scalar(
        "UPDATE lys_fr.document SET deleted_at = COALESCE(deleted_at, now()) WHERE id = $1 RETURNING s3_key",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .ok_or(AppError::NotFound)?;
    let resp = bucket.delete_object(&key).await.map_err(|e| storage_error("delete", e))?;
    if resp.status_code() / 100 != 2 && resp.status_code() != 404 {
        return Err(storage_error("delete", format!("statut {}", resp.status_code())));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_prefix(prefix: &[u8]) -> Vec<u8> {
        let mut v = prefix.to_vec();
        v.extend_from_slice(&[0u8; 32]);
        v
    }

    #[test]
    fn octets_magiques_de_chaque_type() {
        assert_eq!(detect_kind(&with_prefix(b"%PDF-1.7")).unwrap().mime, "application/pdf");
        assert_eq!(detect_kind(&with_prefix(&[0xFF, 0xD8, 0xFF, 0xE0])).unwrap().ext, "jpg");
        let png = with_prefix(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(detect_kind(&png).unwrap().mime, "image/png");
        let mut heic = vec![0, 0, 0, 0x18];
        heic.extend_from_slice(b"ftypheic");
        heic.extend_from_slice(&[0u8; 16]);
        assert_eq!(detect_kind(&heic).unwrap(), FileKind { ext: "heic", mime: "image/heic" });
        let mut heif = vec![0, 0, 0, 0x18];
        heif.extend_from_slice(b"ftypmif1");
        heif.extend_from_slice(&[0u8; 16]);
        assert_eq!(detect_kind(&heif).unwrap().mime, "image/heif");
    }

    #[test]
    fn faux_pdf_executable_refuse() {
        // Un .exe renommé en .pdf : seuls les octets comptent.
        assert!(detect_kind(&with_prefix(b"MZ\x90\x00")).is_none());
        assert!(detect_kind(b"#!/bin/sh\nrm -rf /").is_none());
        assert!(detect_kind(b"<html><script>alert(1)</script>").is_none());
        assert!(detect_kind(b"  %PDF-1.7").is_none(), "préfixe avant %PDF- refusé");
    }

    #[test]
    fn ftyp_d_une_video_ou_trop_court_refuse() {
        let mut mp4 = vec![0, 0, 0, 0x18];
        mp4.extend_from_slice(b"ftypisom");
        mp4.extend_from_slice(&[0u8; 16]);
        assert!(detect_kind(&mp4).is_none());
        assert!(detect_kind(b"").is_none());
        assert!(detect_kind(b"%PDF").is_none());
        assert!(detect_kind(&[0xFF, 0xD8]).is_none());
        assert!(detect_kind(&[0, 0, 0, 0x18, b'f', b't', b'y', b'p', b'h', b'e']).is_none());
    }

    #[test]
    fn cle_independante_du_nom_d_origine() {
        let id = Uuid::parse_str("123e4567-e89b-42d3-a456-426614174000").unwrap();
        assert_eq!(object_key("vls-ts-anef", id, "pdf"), "docs/vls-ts-anef/123e4567-e89b-42d3-a456-426614174000.pdf");
    }

    #[test]
    fn uuid_strict() {
        assert!(parse_doc_id("123e4567-e89b-42d3-a456-426614174000").is_some());
        assert!(parse_doc_id("123E4567-E89B-42D3-A456-426614174000").is_some());
        for bad in [
            "123e4567e89b42d3a456426614174000",
            "{123e4567-e89b-42d3-a456-426614174000}",
            "urn:uuid:123e4567-e89b-42d3-a456-426614174000",
            "123e4567-e89b-42d3-a456-42661417400",
            "123e4567-e89b-42d3-a456-4266141740000",
            "../../etc/passwd",
            "123e4567-e89b-42d3-a456-42661417400g",
            "",
        ] {
            assert!(parse_doc_id(bad).is_none(), "{bad:?} aurait dû être refusé");
        }
    }

    #[test]
    fn nom_assaini() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("C:\\Users\\lys\\acte naissance.pdf"), "acte naissance.pdf");
        assert_eq!(sanitize_filename("a\u{0}b\nc\td.pdf"), "abcd.pdf");
        assert_eq!(sanitize_filename("fichier\u{202E}fdp.exe"), "fichierfdp.exe", "bidi retiré");
        assert_eq!(sanitize_filename("<scr\"ipt>|?*.png"), "_scr_ipt____.png");
        assert_eq!(sanitize_filename("  ..  "), "document");
        assert_eq!(sanitize_filename(""), "document");
        assert_eq!(sanitize_filename("passeport Sophia été.jpg"), "passeport Sophia été.jpg");
        assert_eq!(sanitize_filename(&"x".repeat(500)).chars().count(), MAX_FILENAME_CHARS);
    }

    #[test]
    fn disposition_attachment_et_nom_encode() {
        let d = content_disposition("acte été \"1\".pdf");
        assert!(d.starts_with("attachment; filename=\""));
        assert!(d.contains("filename*=UTF-8''acte%20%C3%A9t%C3%A9%20%221%22.pdf"));
        assert!(!d.contains("é"), "partie ASCII sans caractère non ASCII");
        assert!(!d.contains("\"1\""), "guillemets neutralisés dans la partie ASCII");
    }
}
