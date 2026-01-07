//! # API 处理模块
//!
//! 处理观众端的 HTTP API 请求，包括：
//! - 连接鉴权（答题验证）
//! - 答案提交
//! - 状态查询
//! - 结束直播（主播权限）

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

// ============================================================================
// 数据结构定义
// ============================================================================

/// 直播流状态枚举
///
/// 表示当前直播流的不同状态，用于响应客户端的状态查询
#[derive(Debug, Clone, PartialEq)]
pub enum StreamStatus {
    /// 未注册 - 客户端尚未连接
    Unregistered,
    /// 被封禁 - 答错题目被暂时禁止
    Banned,
    /// 等待答题 - 客户端已连接但尚未通过验证
    Pending,
    /// 直播中 - 主播正在推流
    Live,
    /// 暂停 - 主播暂时中断推流（如网络问题）
    Paused,
    /// 已结束 - 直播已停止
    Ended,
}

impl StreamStatus {
    /// 将状态转换为字符串返回给客户端
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unregistered => "unregistered",
            Self::Banned => "banned",
            Self::Pending => "pending",
            Self::Live => "live",
            Self::Paused => "paused",
            Self::Ended => "ended",
        }
    }
}

/// API 请求参数（规范化后的英文字段名）
///
/// 客户端通过 URL 查询参数传递这些字段
#[derive(Debug, serde::Deserialize)]
pub struct ApiParams {
    /// 会话/请求 ID - 客户端的唯一标识符
    session_id: String,
    /// 要执行的操作类型
    /// - "connect": 连接并获取题目
    action: Option<String>,
    /// 答题提交 - 用户输入的答案
    answer: Option<String>,
    /// 状态查询 - 任意值都会触发状态查询
    status: Option<String>,
    /// 结束直播 - 必须为 "true"
    /// 仅主播（publisher）可执行
    end: Option<String>,
}

/// API 响应结构（规范化后的英文字段名）
///
/// 根据不同的请求类型，响应可能包含不同的字段组合
#[derive(Debug, Serialize)]
pub struct ApiResponse {
    /// 直播间名称
    /// 所有响应都可能包含此字段
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_name: Option<String>,

    /// 视频 URI - 用于播放 FLV 流
    /// 答题成功后返回此字段
    #[serde(skip_serializing_if = "Option::is_none")]
    video_uri: Option<String>,

    /// 答题问题
    /// 新用户连接时返回此字段
    #[serde(skip_serializing_if = "Option::is_none")]
    question: Option<String>,

    /// 是否为主播标识
    /// 使用 secret 验证成功后返回 true
    #[serde(skip_serializing_if = "Option::is_none")]
    is_publisher: Option<bool>,

    /// 当前直播状态
    /// 状态查询时返回此字段
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_status: Option<String>,
}

impl ApiResponse {
    /// 创建一个空的响应对象
    pub fn new() -> Self {
        Self {
            stream_name: None,
            video_uri: None,
            question: None,
            is_publisher: None,
            stream_status: None,
        }
    }

    /// 设置直播间名称（链式调用）
    pub fn with_stream_name(mut self, name: String) -> Self {
        self.stream_name = Some(name);
        self
    }

    /// 设置视频 URI（链式调用）
    /// 格式: "app=xxx&stream=xxx"
    pub fn with_video_uri(mut self, uri: String) -> Self {
        self.video_uri = Some(uri);
        self
    }

    /// 设置答题问题（链式调用）
    pub fn with_question(mut self, q: String) -> Self {
        self.question = Some(q);
        self
    }

    /// 标记为主播（链式调用）
    pub fn with_publisher(mut self) -> Self {
        self.is_publisher = Some(true);
        self
    }

    /// 设置直播状态（链式调用）
    pub fn with_stream_status(mut self, status: &str) -> Self {
        self.stream_status = Some(status.to_string());
        self
    }
}

impl Default for ApiResponse {
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
// API 处理器
// ============================================================================

/// API 请求主处理器
///
/// ### 路由
/// `GET /api.php`
///
/// ### 支持的操作
/// | 操作 | 参数 | 说明 |
/// |------|------|------|
/// | 连接 | `action=connect` | 新用户连接，获取答题问题 |
/// | 答题 | `answer=<答案>` | 提交答案验证 |
/// | 查询状态 | `status=check` | 查询当前直播状态 |
/// | 结束直播 | `end=true` | 主播结束直播 |
///
/// ### 响应格式
/// ```json
/// {
///   "stream_name": "直播间名称",
///   "video_uri": "app=live&stream=test",
///   "question": "问题内容",
///   "is_publisher": true,
///   "stream_status": "live"
/// }
/// ```
pub async fn api_handler(
    State(state): State<Arc<super::super::AppState>>,
    Query(params): Query<ApiParams>,
    headers: axum::http::HeaderMap,
    connect_info: axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Response {
    // 提取客户端 IP 和会话 ID
    let client_ip = get_client_ip(&headers, &connect_info.to_string());
    let client_sid = params.session_id.clone();

    tracing::debug!("API 请求: ip={}, session_id={}", client_ip, client_sid);

    // 初始化响应对象
    let mut response = ApiResponse::new();

    // 获取数据库读锁（后续根据需要升级为写锁）
    let srs_db_read = state.srs_db.read();

    // ========================================
    // 设置直播间名称（所有响应都包含）
    // ========================================
    // 如果主播设置了直播间名称，所有客户端都能看到
    if let Some(name) = srs_db_read.get_stream_name() {
        response = response.with_stream_name(name.to_string());
    }

    // ========================================
    // 处理连接请求 (action=connect)
    // ========================================
    if params.action.as_deref() == Some("connect") {
        // 情况1: 已存在的客户端
        if srs_db_read.has_client(&client_ip, &client_sid) {
            let status = srs_db_read.get_client_status(&client_ip, &client_sid);

            match status {
                // 已通过验证的用户（Legal/Playing/Resting）
                // 直接返回播放地址
                Some(ClientStatus::Legal) | Some(ClientStatus::Playing) | Some(ClientStatus::Resting) => {
                    if let Some(uri) = srs_db_read.get_stream_uri() {
                        response = response.with_video_uri(uri.to_string());
                    }
                    // 如果是主播，标记 is_publisher=true
                    if srs_db_read.client_is_publisher(&client_ip, &client_sid) {
                        response = response.with_publisher();
                        tracing::debug!("({}, {}): 主播已连接", client_ip, client_sid);
                    }
                }
                // 答错题被封禁的用户（Nil）
                // 返回假的视频地址作为惩罚
                Some(ClientStatus::Nil) => {
                    response = response.with_video_uri("app=genshin&straem=impact".to_string());
                    tracing::debug!("({}, {}): 被封禁的客户端（答错题）", client_ip, client_sid);
                }
                // 其他状态（主要是 Pending）- 再次返回题目
                _ => {
                    if let Some((q, _)) = srs_db_read.get_client_qa(&client_ip, &client_sid) {
                        response = response.with_question(q.to_string());
                    }
                }
            }
        } else {
            // 情况2: 新用户 - 发放答题问题            
            // 检查是否为公开模式（无需答题）
            let is_public = {
                srs_db_read.is_public()
            };
            drop(srs_db_read);
            
            // 从题库随机抽取一道题
            let (q, a) = state.banner_db.random_question();

            // 公开模式下，题目会附带答案
            let q_with_answer = if is_public {
                format!("{}(answer=\"{}\")", q, a)
            } else {
                q
            };

            tracing::debug!(
                "({}, {}): 新客户端: 问题=\"{}\", 答案=\"{}\"",
                client_ip,
                client_sid,
                q_with_answer,
                a
            );

            // 在数据库中注册新客户端并存储题目
            {
                let mut srs_db_write = state.srs_db.write();
                srs_db_write.add_client(client_ip.clone(), client_sid.clone());
                srs_db_write.set_client_qa(&client_ip, &client_sid, q_with_answer.clone(), a);
            }

            response = response.with_question(q_with_answer);
        }
        return Json(response).into_response();
    }

    // ========================================
    // 处理答题提交 (answer=<答案>)
    // ========================================
    if let Some(answer) = params.answer {
        // 检查客户端是否存在
        if !srs_db_read.has_client(&client_ip, &client_sid) {
            return forbidden_json_response();
        }

        // 特殊情况：答案以 "secret_" 开头
        // 这是主播用于验证身份的方式
        // 主播可以跳过答题，直接输入推流密钥验证身份
        if answer.starts_with("secret_") {
            drop(srs_db_read);
            let mut db = state.srs_db.write();

            // 验证 secret 是否正确
            if db.connect_streamer(client_sid.clone(), &answer) {
                // 验证成功 - 标记为主播
                db.update_client_activity(&client_ip, &client_sid, ClientStatus::Legal);
                db.set_client_publisher(&client_ip, &client_sid);
                response = response.with_publisher();
                if let Some(uri) = db.get_stream_uri() {
                    response = response.with_video_uri(uri.to_string());
                }
                tracing::debug!("({}, {}): 主播身份验证成功", client_ip, client_sid);
            } else {
                // 验证失败 - 返回假的视频地址
                db.update_client_activity(&client_ip, &client_sid, ClientStatus::Nil);
                response = response.with_video_uri("app=ehviewer&straem=lolicon".to_string());
                tracing::debug!("({}, {}): 无效的主播密钥", client_ip, client_sid);
            }
            return Json(response).into_response();
        }

        // 普通用户答题
        // 只允许 Pending 状态的用户提交答案
        let status = srs_db_read.get_client_status(&client_ip, &client_sid);
        if status != Some(ClientStatus::Pending) {
            return Json(json!({"error": "Not in pending state"})).into_response();
        }

        drop(srs_db_read);
        let mut srs_db_write = state.srs_db.write();

        // 获取存储的正确答案并验证
        let correct = srs_db_write
            .get_client_qa(&client_ip, &client_sid)
            .map(|(_, correct_answer)| correct_answer == answer)
            .unwrap_or(false);

        if correct {
            // 答对了 - 状态改为 Legal，返回播放地址
            srs_db_write.update_client_activity(&client_ip, &client_sid, ClientStatus::Legal);
            if let Some(uri) = srs_db_write.get_stream_uri() {
                response = response.with_video_uri(uri.to_string());
            }
        } else {
            // 答错了 - 状态改为 Nil（被封禁），返回假地址
            srs_db_write.update_client_activity(&client_ip, &client_sid, ClientStatus::Nil);
            response = response.with_video_uri("app=ehviewer&straem=lolicon".to_string());
            tracing::debug!("({}, {}): 答案错误", client_ip, client_sid);
        }
        return Json(response).into_response();
    }

    // ========================================
    // 处理结束直播请求 (end=true)
    // ========================================
    if params.end.as_deref() == Some("true") {
        drop(srs_db_read);
        let mut db = state.srs_db.write();

        // 只有当前主播可以结束直播
        if db.end_streaming(Some(&client_sid)) {
            // 清空聊天记录
            state.chat_db.write().reset();
            tracing::debug!("({}, {}): 主播结束了直播", client_ip, client_sid);
            return (axum::http::StatusCode::OK, "\"ok\"").into_response();
        } else {
            return forbidden_json_response();
        }
    }

    // ========================================
    // 处理状态查询请求 (status=check)
    // ========================================
    if params.status.is_some() {
        // 根据当前状态确定返回的状态值
        let stream_status = if !srs_db_read.has_client(&client_ip, &client_sid) {
            // 客户端不存在
            StreamStatus::Unregistered
        } else {
            match srs_db_read.get_client_status(&client_ip, &client_sid) {
                // 答错题被禁
                Some(ClientStatus::Nil) => StreamStatus::Banned,
                // 待答题
                Some(ClientStatus::Pending) => StreamStatus::Pending,
                // 主播没有在推流
                _ if !srs_db_read.is_streaming() => StreamStatus::Ended,
                // 主播推流中但处于暂停状态
                _ if !srs_db_read.is_actively_streaming() => StreamStatus::Paused,
                // 正常直播中
                _ => StreamStatus::Live,
            }
        };

        response = response.with_stream_status(stream_status.as_str());
        return Json(response).into_response();
    }

    // ========================================
    // 无法识别的请求
    // ========================================
    forbidden_json_response()
}
