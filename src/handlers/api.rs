use super::super::{
    error::{forbidden_json_response},
    state::ClientStatus,
};
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

/// API query parameters
#[derive(Debug, serde::Deserialize)]
pub struct ApiParams {
    rid: String,
    hello: Option<String>,
    #[serde(rename = "Kotae!!")]
    kotae: Option<String>,
    #[serde(rename = "WhatThe")]
    what_the: Option<String>,
    #[serde(rename = "Owari")]
    owari: Option<String>,
}

/// API response structure
#[derive(Debug, Serialize)]
pub struct ApiResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "Gonamae")]
    gonamae: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "video_uri")]
    video_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "Shitsumon!")]
    shitsumon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "Okaeri")]
    okaeri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "jitsuwa")]
    jitsuwa: Option<String>,
}

impl ApiResponse {
    pub fn new() -> Self {
        Self {
            gonamae: None,
            video_uri: None,
            shitsumon: None,
            okaeri: None,
            jitsuwa: None,
        }
    }

    pub fn with_gonamae(mut self, name: String) -> Self {
        self.gonamae = Some(name);
        self
    }

    pub fn with_video_uri(mut self, uri: String) -> Self {
        self.video_uri = Some(uri);
        self
    }

    pub fn with_question(mut self, q: String) -> Self {
        self.shitsumon = Some(q);
        self
    }

    pub fn with_okaeri(mut self) -> Self {
        self.okaeri = Some("master".to_string());
        self
    }

    pub fn with_jitsuwa(mut self, status: &str) -> Self {
        self.jitsuwa = Some(status.to_string());
        self
    }
}

impl Default for ApiResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract client IP from X-Forwarded-For header if present
fn get_client_ip(headers: &axum::http::HeaderMap, remote_addr: &str) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            // Extract IP from remote_addr (format: "IP:PORT")
            remote_addr
                .split(':')
                .next()
                .unwrap_or(remote_addr)
                .to_string()
        })
}

/// API handler for /api.php
pub async fn api_handler(
    State(state): State<Arc<super::super::AppState>>,
    Query(params): Query<ApiParams>,
    headers: axum::http::HeaderMap,
    connect_info: axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Response {
    let client_ip = get_client_ip(&headers, &connect_info.to_string());
    let client_rid = params.rid;

    tracing::debug!("API request: ip={}, rid={}", client_ip, client_rid);

    let mut response = ApiResponse::new();
    let db = state.srs_db.read();

    // Everyone can see the stream name
    if let Some(name) = db.get_stream_name() {
        response = response.with_gonamae(name.to_string());
    }

    // Handle hello=live request
    if params.hello.as_deref() == Some("live") {
        if db.has_client(&client_ip, &client_rid) {
            let status = db.get_client_status(&client_ip, &client_rid);
            match status {
                Some(ClientStatus::Legal) | Some(ClientStatus::Playing) | Some(ClientStatus::Resting) => {
                    if let Some(uri) = db.get_stream_uri() {
                        response = response.with_video_uri(uri.to_string());
                    }
                    if db.client_is_publisher(&client_ip, &client_rid) {
                        response = response.with_okaeri();
                        tracing::debug!("({}, {}): welcome, master", client_ip, client_rid);
                    }
                }
                Some(ClientStatus::Nil) => {
                    response = response.with_video_uri("app=genshin&straem=impact".to_string());
                    tracing::debug!("({}, {}): RICKY ROLL: wrong answer", client_ip, client_rid);
                }
                _ => {
                    // Pending or other - return question again
                    if let Some((q, _)) = db.get_client_qa(&client_ip, &client_rid) {
                        response = response.with_question(q.to_string());
                    }
                }
            }
        } else {
            // New client - send a question
            drop(db); // Release read lock before write
            let (q, a) = state.banner_db.random_question();

            let is_public = {
                let db = state.srs_db.read();
                db.is_public()
            };

            let q_with_answer = if is_public {
                format!("{}(answer=\"{}\")", q, a)
            } else {
                q
            };

            tracing::debug!(
                "({}, {}): added record: q=\"{}\", a=\"{}\"",
                client_ip,
                client_rid,
                q_with_answer,
                a
            );

            {
                let mut db = state.srs_db.write();
                db.add_client(client_ip.clone(), client_rid.clone());
                db.set_client_qa(&client_ip, &client_rid, q_with_answer.clone(), a);
            }
            response = response.with_question(q_with_answer);
        }
        return Json(response).into_response();
    }

    // Handle Kotae!! (answer)
    if let Some(answer) = params.kotae {
        if !db.has_client(&client_ip, &client_rid) {
            return forbidden_json_response();
        }

        // Check if it's a streamer secret
        if answer.starts_with("secret_") {
            drop(db);
            let mut db = state.srs_db.write();

            if db.connect_streamer(client_rid.clone(), &answer) {
                db.update_client_activity(&client_ip, &client_rid, ClientStatus::Legal);
                db.set_client_publisher(&client_ip, &client_rid);
                response = response.with_okaeri();
                if let Some(uri) = db.get_stream_uri() {
                    response = response.with_video_uri(uri.to_string());
                }
                tracing::debug!("({}, {}): welcome, master", client_ip, client_rid);
            } else {
                db.update_client_activity(&client_ip, &client_rid, ClientStatus::Nil);
                response = response.with_video_uri("app=ehviewer&straem=lolicon".to_string());
                tracing::debug!("({}, {}): fake master", client_ip, client_rid);
            }
            return Json(response).into_response();
        }

        // Regular client answer
        let status = db.get_client_status(&client_ip, &client_rid);
        if status != Some(ClientStatus::Pending) {
            return Json(json!({"act": "Nope."})).into_response();
        }

        drop(db);
        let mut db = state.srs_db.write();

        let correct = db
            .get_client_qa(&client_ip, &client_rid)
            .map(|(_, correct_answer)| correct_answer == answer)
            .unwrap_or(false);

        if correct {
            db.update_client_activity(&client_ip, &client_rid, ClientStatus::Legal);
            if let Some(uri) = db.get_stream_uri() {
                response = response.with_video_uri(uri.to_string());
            }
        } else {
            db.update_client_activity(&client_ip, &client_rid, ClientStatus::Nil);
            response = response.with_video_uri("app=ehviewer&straem=lolicon".to_string());
            tracing::debug!("({}, {}): wrong answer", client_ip, client_rid);
        }
        return Json(response).into_response();
    }

    // Handle Owari=oyasumi (end streaming)
    if params.owari.as_deref() == Some("oyasumi") {
        drop(db);
        let mut db = state.srs_db.write();

        if db.end_streaming(Some(&client_rid)) {
            // Reset chat database
            state.chat_db.write().reset();
            tracing::debug!("({}, {}): master ended the stream", client_ip, client_rid);
            return (axum::http::StatusCode::OK, "\"oyasumi\"").into_response();
        } else {
            return forbidden_json_response();
        }
    }

    // Handle WhatThe=Fuck (status check)
    if params.what_the.as_deref() == Some("Fuck") {
        let jitsuwa = if !db.has_client(&client_ip, &client_rid) {
            "Kimi no Na wa"
        } else {
            match db.get_client_status(&client_ip, &client_rid) {
                Some(ClientStatus::Nil) => "all-good",
                Some(ClientStatus::Pending) => "Nanka ie yo",
                _ if !db.is_streaming() => "Mo-owari",
                _ if !db.is_actively_streaming() => "Chottomatte",
                _ => "naninani",
            }
        };
        response = response.with_jitsuwa(jitsuwa);
        return Json(response).into_response();
    }

    // Unknown request
    forbidden_json_response()
}
