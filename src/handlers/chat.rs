//! # 聊天室处理器模块
//!
//! 处理聊天室相关的 HTTP 请求，包括：
//! - 客户端连接（hello）
//! - 设置昵称（setname）
//! - 设置直播间名称（setlivename）
//! - 获取聊天消息（getchat）
//! - 发送聊天消息（sendchat）
//! - 获取观众人数（getaudiences）
//! - 保存聊天快照（savesnapshot）

use super::super::error::chat_forbidden_response;
use axum::{
    extract::{Query, State},
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::json;
use std::sync::Arc;

// ============================================================================
// 数据结构定义
// ============================================================================

/// 聊天室 URL 查询参数
///
/// 客户端通过 URL 参数传递会话标识
#[derive(Debug, serde::Deserialize)]
pub struct ChatParams {
    /// 请求 ID / 会话 ID
    rid: String,
}

/// 聊天室请求体
///
/// 使用 tagged enum 区分不同的操作类型
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "action")]
pub enum ChatRequest {
    /// 客户端连接
    #[serde(rename = "hello")]
    Hello,
    /// 设置用户昵称
    #[serde(rename = "setname")]
    SetName { name: String },
    /// 设置直播间名称（仅主播）
    #[serde(rename = "setlivename")]
    SetLiveName { name: String },
    /// 获取聊天消息
    #[serde(rename = "getchat")]
    GetChat {
        /// 获取之前消息的时间戳（与 next 二选一）
        prev: Option<f64>,
        /// 获取之后消息的时间戳（与 prev 二选一）
        next: Option<f64>,
    },
    /// 发送聊天消息
    #[serde(rename = "sendchat")]
    SendChat { chat: String },
    /// 获取观众人数
    #[serde(rename = "getaudiences")]
    GetAudiences,
    /// 保存聊天快照（仅主播）
    #[serde(rename = "savesnapshot")]
    SaveSnapshot,
}

/// 聊天室响应结构
///
/// 根据请求类型返回不同的字段组合
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// 状态标识（"Okay" 表示成功，"Nope" 表示失败）
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    /// 用户昵称
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// 聊天消息列表
    #[serde(skip_serializing_if = "Option::is_none")]
    chatmsgs: Option<Vec<serde_json::Value>>,
    /// 观众人数信息
    #[serde(skip_serializing_if = "Option::is_none")]
    audiences: Option<AudienceInfo>,
}

/// 观众人数信息
#[derive(Debug, Serialize)]
pub struct AudienceInfo {
    /// 当前在线人数（从 SRS 获取，-1 表示未知）
    current: i32,
    /// 累计唯一用户数
    total: usize,
}

impl ChatResponse {
    /// 创建空的响应对象
    pub fn new() -> Self {
        Self {
            status: None,
            name: None,
            chatmsgs: None,
            audiences: None,
        }
    }

    /// 设置状态（链式调用）
    pub fn with_status(mut self, status: &str) -> Self {
        self.status = Some(status.to_string());
        self
    }

    /// 设置昵称（链式调用）
    pub fn with_name(mut self, name: Option<String>) -> Self {
        self.name = name;
        self
    }

    /// 设置聊天消息列表（链式调用）
    pub fn with_chatmsgs(mut self, msgs: Vec<serde_json::Value>) -> Self {
        self.chatmsgs = Some(msgs);
        self
    }

    /// 设置观众人数（链式调用）
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

// ============================================================================
// 辅助函数
// ============================================================================

/// 从请求头中提取客户端真实 IP 地址
///
/// ### 优先级
/// 1. `X-Forwarded-For` 头 - 当服务位于反向代理（如 Nginx）后面时
/// 2. `ConnectInfo` 中的远程地址 - 直接连接时的客户端地址
///
/// ### 为什么需要这个？
/// - 生产环境通常使用 Nginx 等 HTTP 服务器作为反向代理
/// - 这种情况下，rusty-live-server 看到的远程地址是 127.0.0.1（本地代理）
/// - 真实客户端 IP 在 `X-Forwarded-For` 头中
fn get_client_ip(headers: &axum::http::HeaderMap, remote_addr: &str) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        // X-Forwarded-For 可能包含多个 IP（客户端, 代理1, 代理2...）
        // 取第一个（原始客户端 IP）
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            // 没有 X-Forwarded-For 头，直接使用远程地址
            // 远程地址格式为 "IP:PORT"，需要提取 IP 部分
            remote_addr
                .split(':')
                .next()
                .unwrap_or(remote_addr)
                .to_string()
        })
}

// ============================================================================
// 聊天室处理器
// ============================================================================

/// 聊天室请求主处理器
///
/// ### 路由
/// `POST /chat.php?rid=<session_id>`
///
/// ### 请求格式
/// ```json
/// {
///   "action": "hello|setname|setlivename|getchat|sendchat|getaudiences|savesnapshot",
///   ... // 其他 action 相关参数
/// }
/// ```
///
/// ### 响应格式
/// ```json
/// {
///   "status": "Okay|Nope",
///   "name": "用户昵称",
///   "chatmsgs": [...],
///   "audiences": {"current": -1, "total": 10}
/// }
/// ```
pub async fn chat_handler(
    State(state): State<Arc<super::super::AppState>>,
    Query(params): Query<ChatParams>,
    headers: axum::http::HeaderMap,
    connect_info: axum::extract::ConnectInfo<std::net::SocketAddr>,
    body: String,
) -> Response {
    // 提取客户端 IP 和会话 ID
    let client_ip = get_client_ip(&headers, &connect_info.to_string());
    let client_rid = params.rid;

    // ========================================
    // 权限验证
    // ========================================
    {
        let srs_db = state.srs_db.read();
        // 检查直播是否已开始
        if !srs_db.is_streaming() {
            return chat_forbidden_response();
        }

        // 检查客户端是否已通过答题验证
        if !srs_db.has_authorized_client(&client_ip, &client_rid) {
            return Json(json!({"status": "Nope"})).into_response();
        }
    }

    // 解析请求体
    let request: ChatRequest = match serde_json::from_str(&body) {
        Ok(req) => req,
        Err(_) => return chat_forbidden_response(),
    };

    let mut response = ChatResponse::new();

    // ========================================
    // 根据操作类型分发处理
    // ========================================
    match request {
        // --- 客户端连接 ---
        ChatRequest::Hello => {
            let chat_db = state.chat_db.read();
            let name = chat_db.get_client_name(&client_ip, &client_rid);
            let msgs = chat_db.get_chat_from(-1.0, false);
            response = response
                .with_status("Okay")
                .with_name(name)
                .with_chatmsgs(msgs);
        }

        // --- 设置用户昵称 ---
        ChatRequest::SetName { name } => {
            let mut chat_db = state.chat_db.write();
            let success = chat_db.set_client_name(&client_ip, &client_rid, name.clone());
            response = response
                .with_status(if success { "Okay" } else { "Nope" })
                .with_name(chat_db.get_client_name(&client_ip, &client_rid));
        }

        // --- 设置直播间名称（仅主播） ---
        ChatRequest::SetLiveName { name } => {
            // 检查是否为主播
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
            // 无论如何都返回当前直播间名称
            response = response.with_name({
                let srs_db = state.srs_db.read();
                srs_db.get_stream_name().map(|s| s.to_string())
            });
        }

        // --- 获取聊天消息 ---
        ChatRequest::GetChat { prev, next } => {
            // 必须提供 prev 或 next 之一
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

        // --- 发送聊天消息 ---
        ChatRequest::SendChat { chat } => {
            // 检查是否为主播
            let is_publisher = {
                let srs_db = state.srs_db.read();
                srs_db.client_is_publisher(&client_ip, &client_rid)
            };

            // 添加消息到数据库
            let mut chat_db = state.chat_db.write();
            chat_db.add_entry(client_ip, client_rid, chat, is_publisher);
            response = response.with_status("Okay");
        }

        // --- 获取观众人数 ---
        ChatRequest::GetAudiences => {
            // 获取累计用户数
            let total = {
                let chat_db = state.chat_db.read();
                chat_db.size()
            };

            // 返回观众人数（current 从 SRS 获取，暂不实现，返回 -1）
            response = response
                .with_status("Okay")
                .with_audiences(-1, total);
        }

        // --- 保存聊天快照（仅主播） ---
        ChatRequest::SaveSnapshot => {
            // 检查是否为主播
            let is_publisher = {
                let srs_db = state.srs_db.read();
                srs_db.client_is_publisher(&client_ip, &client_rid)
            };

            if is_publisher {
                let chat_db = state.chat_db.read();
                chat_db.dump_full();
                tracing::debug!("({}, {}): 主播保存了聊天记录", client_ip, client_rid);
                response = response.with_status("Okay");
            } else {
                response = response.with_status("Nope");
            }
        }
    }

    Json(response).into_response()
}
