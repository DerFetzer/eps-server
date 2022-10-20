use std::{error::Error, fmt::Display};

use axum::{http::StatusCode, response::IntoResponse};

#[derive(Debug)]
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

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let error = match self {
            AppError::InternalServerError(e) => e,
            AppError::NotFound(e) => e,
            AppError::BadRequest(e) => e,
        };
        write!(f, "{error}")
    }
}

impl Error for AppError {}
