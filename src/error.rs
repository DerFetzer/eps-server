use axum::{http::StatusCode, response::IntoResponse};

pub(crate) enum AppError {
    InternalServerError(eyre::Error),
    NotFound(eyre::Error),
    BadRequest(eyre::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::InternalServerError(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::NotFound(e) => (StatusCode::NOT_FOUND, e.to_string()),
            Self::BadRequest(e) => (StatusCode::BAD_REQUEST, e.to_string()),
        }
        .into_response()
    }
}
