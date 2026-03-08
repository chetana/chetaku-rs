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
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Db(e)           => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::NotFound        => (StatusCode::NOT_FOUND, "Not found".to_string()),
            AppError::ExternalApi(e)  => (StatusCode::BAD_GATEWAY, e.clone()),
            AppError::Unauthorized    => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
