use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use std::fmt;

#[derive(Debug)]
pub enum ApiError {
    Forbidden(String),
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            ApiError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            ApiError::Internal(msg) => write!(f, "Internal Error: {}", msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

// SRS callback specific error responses
pub fn srs_forbidden_response() -> Response {
    (StatusCode::FORBIDDEN, "rua").into_response()
}

pub fn srs_success_response() -> Response {
    (StatusCode::OK, "0").into_response()
}

pub fn forbidden_json_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        [("Content-Type", "application/json")],
        "It was a joke",
    )
        .into_response()
}

pub fn chat_forbidden_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        [("Content-Type", "application/json")],
        "Haha, fat chance",
    )
        .into_response()
}
