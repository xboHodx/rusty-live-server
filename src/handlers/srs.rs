use super::super::{
    error::{srs_forbidden_response, srs_success_response},
    state::ClientStatus,
};
use axum::{
    extract::State,
    response::Response,
    Json,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

/// SRS callback request structure
#[derive(Debug, Deserialize)]
pub struct SrsCallbackRequest {
    pub action: String,
    pub ip: String,
    pub app: String,
    pub stream: String,
    pub param: String,
    #[serde(rename = "tcUrl", default)]
    pub _tc_url: String,
}

/// Parse query string from param
fn parse_param(param: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // Remove leading '?' if present
    let param = param.strip_prefix('?').unwrap_or(param);

    for pair in param.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let value = value.split('?').next().unwrap_or(value);
            result.insert(key.to_string(), value.to_string());
        }
    }

    result
}

/// SRS callback handler
pub async fn srs_callback_handler(
    State(state): State<Arc<crate::state::AppState>>,
    Json(payload): Json<SrsCallbackRequest>,
) -> Response {
    tracing::debug!("SRS callback: action={}, ip={}", payload.action, payload.ip);

    match payload.action.as_str() {
        "on_publish" => handle_on_publish(state, payload).await,
        "on_play" => handle_on_play(state, payload).await,
        "on_unpublish" => handle_on_unpublish(state, payload).await,
        "on_stop" => handle_on_stop(state, payload).await,
        _ => {
            tracing::warn!("Unknown SRS action: {}", payload.action);
            srs_forbidden_response()
        }
    }
}

/// Handle on_publish callback
async fn handle_on_publish(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    let queries = parse_param(&payload.param);

    let secret = match queries.get("secret") {
        Some(s) => s.clone(),
        None => {
            tracing::debug!("srs call back refused: no key presented");
            return srs_forbidden_response();
        }
    };

    let is_streaming = state.srs_db.read().is_streaming();

    if is_streaming {
        // If already streaming, try to resume
        let mut srs_db = state.srs_db.write();

        if srs_db.resume_streaming(payload.ip.clone(), &secret, payload.app.clone(), payload.stream.clone()) {
            tracing::debug!("server ({}) resumed streaming", payload.ip);
            srs_success_response()
        } else {
            tracing::debug!("srs call back refused: another streamer is already streaming");
            srs_forbidden_response()
        }
    } else {
        // New stream
        let mut srs_db = state.srs_db.write();

        if srs_db.verify_streamer(&secret) {
            srs_db.register_streamer(payload.ip.clone(), secret, payload.app.clone(), payload.stream.clone());

            // Check if public mode
            if let Some(public_val) = queries.get("public") {
                if public_val.to_lowercase() == "true" {
                    srs_db.set_public(true);
                    tracing::debug!("server ({}) started streaming in public mode", payload.ip);
                } else {
                    srs_db.set_public(false);
                    tracing::debug!("server ({}) started streaming", payload.ip);
                }
            } else {
                tracing::debug!("server ({}) started streaming", payload.ip);
            }

            // Reset chat database
            state.chat_db.write().reset();

            srs_success_response()
        } else {
            tracing::debug!("srs call back refused: not a valid streamer");
            srs_forbidden_response()
        }
    }
}

/// Handle on_play callback
async fn handle_on_play(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    let queries = parse_param(&payload.param);
    let rid = queries.get("rid").cloned().unwrap_or_default();

    let srs_db = state.srs_db.read();

    if !srs_db.has_client(&payload.ip, &rid) {
        tracing::debug!("srs call back refused: not a recorded watcher");
        return srs_forbidden_response();
    }

    let client_status = srs_db.get_client_status(&payload.ip, &rid);
    drop(srs_db);
    match client_status {
        Some(ClientStatus::Pending) | Some(ClientStatus::Nil) => {
            tracing::debug!("srs call back refused: not a permitted watcher");
            return srs_forbidden_response();
        }
        _ => {}
    }

    let mut srs_db = state.srs_db.write();
    srs_db.update_client_activity(&payload.ip, &rid, ClientStatus::Playing);

    srs_success_response()
}

/// Handle on_unpublish callback
async fn handle_on_unpublish(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    let mut srs_db = state.srs_db.write();
    srs_db.pause_streaming();
    tracing::debug!("server ({}) stopped streaming", payload.ip);
    srs_success_response()
}

/// Handle on_stop callback
async fn handle_on_stop(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    let queries = parse_param(&payload.param);
    let rid = queries.get("rid").cloned();

    let mut srs_db = state.srs_db.write();

    if rid.is_some() && srs_db.has_client(&payload.ip, rid.as_deref().unwrap_or("")) {
        srs_db.update_client_activity(&payload.ip, rid.as_deref().unwrap_or(""), ClientStatus::Resting);
    }

    srs_success_response()
}
