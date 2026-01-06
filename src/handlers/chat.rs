use super::super::error::chat_forbidden_response;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

/// Chat query parameters (rid in URL)
#[derive(Debug, serde::Deserialize)]
pub struct ChatParams {
    rid: String,
}

/// Chat request body
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "action")]
pub enum ChatRequest {
    #[serde(rename = "hello")]
    Hello,
    #[serde(rename = "setname")]
    SetName { name: String },
    #[serde(rename = "setlivename")]
    SetLiveName { name: String },
    #[serde(rename = "getchat")]
    GetChat { prev: Option<f64>, next: Option<f64> },
    #[serde(rename = "sendchat")]
    SendChat { chat: String },
    #[serde(rename = "getaudiences")]
    GetAudiences,
    #[serde(rename = "savesnapshot")]
    SaveSnapshot,
}

/// Chat response structure
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chatmsgs: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audiences: Option<AudienceInfo>,
}

#[derive(Debug, Serialize)]
pub struct AudienceInfo {
    current: i32,
    total: usize,
}

impl ChatResponse {
    pub fn new() -> Self {
        Self {
            status: None,
            name: None,
            chatmsgs: None,
            audiences: None,
        }
    }

    pub fn with_status(mut self, status: &str) -> Self {
        self.status = Some(status.to_string());
        self
    }

    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    pub fn with_chatmsgs(mut self, msgs: Vec<serde_json::Value>) -> Self {
        self.chatmsgs = Some(msgs);
        self
    }

    pub fn with_audiences(mut self, current: i32, total: usize) -> Self {
        self.audiences = Some(AudienceInfo { current, total });
        self
    }
}

impl Default for ChatResponse {
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
            remote_addr
                .split(':')
                .next()
                .unwrap_or(remote_addr)
                .to_string()
        })
}

/// Chat handler for /chat.php
pub async fn chat_handler(
    State(state): State<Arc<super::super::AppState>>,
    Query(params): Query<ChatParams>,
    headers: axum::http::HeaderMap,
    connect_info: axum::extract::ConnectInfo<std::net::SocketAddr>,
    body: String,
) -> Response {
    let client_ip = get_client_ip(&headers, &connect_info.to_string());
    let client_rid = params.rid;

    // Check if streaming
    {
        let srs_db = state.srs_db.read();
        if !srs_db.is_streaming() {
            return chat_forbidden_response();
        }

        // Check if client is authorized
        if !srs_db.has_authorized_client(&client_ip, &client_rid) {
            return Json(json!({"status": "Nope"})).into_response();
        }
    }

    // Parse request body
    let request: ChatRequest = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(_) => return chat_forbidden_response(),
    };

    let mut response = ChatResponse::new();

    match request {
        ChatRequest::Hello => {
            let chat_db = state.chat_db.read();
            let name = chat_db.get_client_name(&client_ip, &client_rid);
            let msgs = chat_db.get_chat_from(-1.0, false);
            response = response
                .with_status("Okay")
                .with_name(name)
                .with_chatmsgs(msgs);
        }

        ChatRequest::SetName { name } => {
            let mut chat_db = state.chat_db.write();
            let success = chat_db.set_client_name(&client_ip, &client_rid, name.clone());
            response = response
                .with_status(if success { "Okay" } else { "Nope" })
                .with_name(chat_db.get_client_name(&client_ip, &client_rid));
        }

        ChatRequest::SetLiveName { name } => {
            let is_publisher = {
                let srs_db = state.srs_db.read();
                srs_db.client_is_publisher(&client_ip, &client_rid)
            };

            if is_publisher {
                let mut srs_db = state.srs_db.write();
                srs_db.set_stream_name(name.clone());
                response = response.with_status("Okay");
            } else {
                response = response.with_status("Nope");
            }
            response = response.with_name({
                let srs_db = state.srs_db.read();
                srs_db.get_stream_name().map(|s| s.to_string())
            });
        }

        ChatRequest::GetChat { prev, next } => {
            let (stamp, is_prev) = if let Some(p) = prev {
                (p, true)
            } else if let Some(n) = next {
                (n, false)
            } else {
                return chat_forbidden_response();
            };

            let chat_db = state.chat_db.read();
            let msgs = chat_db.get_chat_from(stamp, is_prev);
            response = response
                .with_status("Okay")
                .with_chatmsgs(msgs);
        }

        ChatRequest::SendChat { chat } => {
            let is_publisher = {
                let srs_db = state.srs_db.read();
                srs_db.client_is_publisher(&client_ip, &client_rid)
            };

            let mut chat_db = state.chat_db.write();
            chat_db.add_entry(client_ip, client_rid, chat, is_publisher);
            response = response.with_status("Okay");
        }

        ChatRequest::GetAudiences => {
            let total = {
                let chat_db = state.chat_db.read();
                chat_db.size()
            };

            response = response
                .with_status("Okay")
                .with_audiences(-1, total);
        }

        ChatRequest::SaveSnapshot => {
            let is_publisher = {
                let srs_db = state.srs_db.read();
                srs_db.client_is_publisher(&client_ip, &client_rid)
            };

            if is_publisher {
                let chat_db = state.chat_db.read();
                chat_db.dump_full();
                tracing::debug!("({}, {}): master saved the chat history", client_ip, client_rid);
                response = response.with_status("Okay");
            } else {
                response = response.with_status("Nope");
            }
        }
    }

    Json(response).into_response()
}
