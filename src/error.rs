use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Ошибки приложения, отображаемые пользователю.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("forbidden")]
    Forbidden,
    #[error("banned")]
    Banned,
    #[error("rate limited")]
    RateLimited,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found".into()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden".into()),
            AppError::Banned => (StatusCode::FORBIDDEN, "You are banned".into()),
            AppError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "You are posting too fast".into(),
            ),
            AppError::Internal(err) => {
                tracing::error!("internal error: {err:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
            }
        };
        let mut resp = Json(json!({ "error": body })).into_response();
        *resp.status_mut() = status;
        resp
    }
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }
}
