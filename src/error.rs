use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Not found")]
    NotFound,

    #[error("External API error: {0}")]
    ExternalApi(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden")]
    Forbidden,

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Unprocessable: {0}")]
    Unprocessable(String),

    #[error("Payload too large")]
    PayloadTooLarge,

    #[error("Unsupported media type")]
    UnsupportedMediaType,

    /// Fonctionnalité non configurée (ex. stockage des documents sans variables LYS_S3_*).
    #[error("Unavailable")]
    Unavailable,

    /// Échec d'un service de stockage : le détail reste dans les logs, jamais dans la réponse.
    #[error("Storage error")]
    Storage,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Db(e) => {
                tracing::error!("erreur base de données: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".to_string())
            }
            AppError::NotFound        => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::ExternalApi(e)  => (StatusCode::BAD_GATEWAY, e.clone()),
            AppError::Unauthorized      => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden         => (StatusCode::FORBIDDEN, "Forbidden".to_string()),
            AppError::Validation(msg)   => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unprocessable(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::PayloadTooLarge    => (StatusCode::PAYLOAD_TOO_LARGE, "Payload too large".to_string()),
            AppError::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "type de fichier refusé (PDF, JPEG, PNG ou HEIC attendu)".to_string(),
            ),
            AppError::Unavailable        => (StatusCode::SERVICE_UNAVAILABLE, "stockage indisponible".to_string()),
            AppError::Storage            => (StatusCode::BAD_GATEWAY, "stockage en erreur".to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn forbidden_donne_403_distinct_du_401() {
        assert_eq!(AppError::Forbidden.into_response().status(), StatusCode::FORBIDDEN);
        assert_eq!(AppError::Unauthorized.into_response().status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn erreur_db_ne_fuit_pas_le_detail() {
        let r = AppError::Db(sqlx::Error::Protocol("relation lys_fr.secret introuvable".into())).into_response();
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = String::from_utf8(to_bytes(r.into_body(), 4096).await.unwrap().to_vec()).unwrap();
        assert!(!body.contains("lys_fr"), "détail DB exposé: {body}");
    }
}
