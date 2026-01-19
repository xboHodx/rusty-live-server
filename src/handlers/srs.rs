//! # SRS 回调处理器模块
//!
//! 处理来自 SRS（Simple Realtime Server）的回调事件。
//!
//! ## 支持的回调类型
//! - `on_publish` - 主播开始推流
//! - `on_play` - 观众开始拉流
//! - `on_unpublish` - 主播停止推流
//! - `on_stop` - 观众停止拉流
//!
//! ## 回调验证流程
//! 1. 解析 SRS 发送的 JSON 数据
//! 2. 从 param 字段中提取查询参数
//! 3. 验证密钥/权限
//! 4. 更新内部状态
//! 5. 返回响应给 SRS（允许/拒绝）

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

// ============================================================================
// 数据结构定义
// ============================================================================

/// SRS 回调请求结构
///
/// SRS 在发生事件时会向此服务发送 POST 请求
#[derive(Debug, Deserialize)]
pub struct SrsCallbackRequest {
    /// 回调类型：on_publish, on_play, on_unpublish, on_stop
    pub action: String,
    /// 客户端 IP 地址
    pub ip: String,
    /// 应用名称（如 "live"）
    pub app: String,
    /// 流名称（如 "stream_name"）
    pub stream: String,
    /// 查询参数字符串（包含 secret, session_id 等信息）
    pub param: String,
    /// TC URL（未使用，保留以兼容 SRS 协议）
    #[serde(rename = "tcUrl", default)]
    pub _tc_url: String,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 解析 param 字段中的查询参数
///
/// ### 参数格式
/// param 字段通常包含类似 `?secret=xxx&rid=yyy` 的字符串
///
/// ### 返回值
/// 返回解析后的键值对 HashMap
fn parse_param(param: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // 移除前导 '?'（如果存在）
    let param = param.strip_prefix('?').unwrap_or(param);

    // 按 '&' 分割键值对
    for pair in param.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            // 移除值中可能存在的额外 '?'（处理嵌套情况）
            let value = value.split('?').next().unwrap_or(value);
            result.insert(key.to_string(), value.to_string());
        }
    }

    result
}

// ============================================================================
// SRS 回调处理器
// ============================================================================

/// SRS 回调主处理器
///
/// ### 路由
/// `POST /`（端口 8848）
///
/// ### 请求格式
/// SRS 发送 JSON 格式的回调数据
///
/// ### 响应格式
/// - 成功：HTTP 200 + "0"
/// - 失败：HTTP 403 + "rua"
pub async fn srs_callback_handler(
    State(state): State<Arc<crate::state::AppState>>,
    Json(payload): Json<SrsCallbackRequest>,
) -> Response {
    tracing::debug!("SRS 回调: action={}, ip={}", payload.action, payload.ip);

    // 根据回调类型分发到相应的处理函数
    match payload.action.as_str() {
        "on_publish" => handle_on_publish(state, payload).await,
        "on_play" => handle_on_play(state, payload).await,
        "on_unpublish" => handle_on_unpublish(state, payload).await,
        "on_stop" => handle_on_stop(state, payload).await,
        _ => {
            tracing::warn!("未知的 SRS 回调类型: {}", payload.action);
            srs_forbidden_response()
        }
    }
}

// ============================================================================
// 具体回调处理函数
// ============================================================================

/// 处理 on_publish 回调
///
/// 当主播开始推流时触发。
///
/// ### 验证流程
/// 1. 从 param 中提取 secret 参数
/// 2. 如果没有 secret，拒绝
/// 3. 如果已在推流，尝试恢复（验证 secret）
/// 4. 如果未推流，验证 secret 并注册新主播
/// 5. 检查是否为公开模式
/// 6. 重置聊天室数据库
async fn handle_on_publish(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    // 解析查询参数
    let queries = parse_param(&payload.param);

    // 获取推流密钥
    let secret = match queries.get("secret") {
        Some(s) => s.clone(),
        None => {
            tracing::debug!("SRS 回调拒绝: 未提供密钥");
            return srs_forbidden_response();
        }
    };

    // 检查是否已在推流
    let is_streaming = state.srs_db.inner.read().is_streaming();

    if is_streaming {
        // 已在推流，尝试恢复（可能是网络问题导致的重新推流）
        let mut srs_db = state.srs_db.inner.write();

        if srs_db.resume_streaming(payload.ip.clone(), &secret, payload.app.clone(), payload.stream.clone()) {
            tracing::debug!("推流者 ({}) 恢复推流", payload.ip);
            srs_success_response()
        } else {
            tracing::debug!("SRS 回调拒绝: 已有其他推流者在推流");
            srs_forbidden_response()
        }
    } else {
        // 新推流
        let mut srs_db = state.srs_db.inner.write();

        // 验证密钥
        if srs_db.verify_streamer(&secret) {
            // 注册新主播
            srs_db.register_streamer(payload.ip.clone(), secret, payload.app.clone(), payload.stream.clone());

            // 检查是否为公开模式
            if let Some(public_val) = queries.get("public") {
                if public_val.to_lowercase() == "true" {
                    srs_db.set_public(true);
                    tracing::debug!("推流者 ({}) 开始公开模式推流", payload.ip);
                } else {
                    srs_db.set_public(false);
                    tracing::debug!("推流者 ({}) 开始推流", payload.ip);
                }
            } else {
                tracing::debug!("推流者 ({}) 开始推流", payload.ip);
            }

            // 重置聊天室数据库
            state.chat_db.inner.write().reset();

            srs_success_response()
        } else {
            tracing::debug!("SRS 回调拒绝: 无效的推流密钥");
            srs_forbidden_response()
        }
    }
}

/// 处理 on_play 回调
///
/// 当观众开始拉流时触发。
///
/// ### 验证流程
/// 1. 从 param 中提取 session_id 参数（向后兼容 rid）
/// 2. 检查客户端是否已注册
/// 3. 检查客户端状态是否允许拉流
/// 4. 更新客户端状态为 Playing
async fn handle_on_play(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    // 解析查询参数（优先使用 session_id，向后兼容 rid）
    let queries = parse_param(&payload.param);
    let session_id = queries
        .get("session_id")
        .or_else(|| queries.get("rid"))
        .cloned()
        .unwrap_or_default();

    let srs_db = state.srs_db.inner.read();

    // 检查客户端是否已注册（只检查 session_id，因为 SRS 回调的 IP 是 Docker 内部 IP）
    let client_status = srs_db.get_client_status_any_ip(&session_id);

    let (client_ip, client_status) = match client_status {
        Some((ip, status)) => (ip, status),
        None => {
            tracing::debug!("SRS 回调拒绝: 客户端未注册 session_id={}", session_id);
            return srs_forbidden_response();
        }
    };

    drop(srs_db);

    match client_status {
        ClientStatus::Pending | ClientStatus::Nil => {
            // 待答题或被封禁，不允许拉流
            tracing::debug!("SRS 回调拒绝: 客户端未获得许可 session_id={}", session_id);
            return srs_forbidden_response();
        }
        _ => {}
    }

    // 更新客户端状态为 Playing
    let mut srs_db = state.srs_db.inner.write();
    srs_db.update_client_activity(&client_ip, &session_id, ClientStatus::Playing);

    srs_success_response()
}

/// 处理 on_unpublish 回调
///
/// 当主播停止推流时触发。
///
/// ### 处理流程
/// 将主播状态设置为 Pausing（暂停），允许一段时间内恢复
async fn handle_on_unpublish(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    let mut srs_db = state.srs_db.inner.write();
    srs_db.pause_streaming();
    tracing::debug!("推流者 ({}) 停止推流", payload.ip);
    srs_success_response()
}

/// 处理 on_stop 回调
///
/// 当观众停止拉流时触发。
///
/// ### 处理流程
/// 将客户端状态更新为 Resting（暂离）
async fn handle_on_stop(
    state: Arc<crate::state::AppState>,
    payload: SrsCallbackRequest,
) -> Response {
    // 解析查询参数（优先使用 session_id，向后兼容 rid）
    let queries = parse_param(&payload.param);
    let session_id = queries.get("session_id").or_else(|| queries.get("rid")).cloned();

    let mut srs_db = state.srs_db.inner.write();

    // 如果客户端存在，更新状态为 Resting
    if session_id.is_some() && srs_db.has_client(&payload.ip, session_id.as_deref().unwrap_or("")) {
        srs_db.update_client_activity(&payload.ip, session_id.as_deref().unwrap_or(""), ClientStatus::Resting);
    }

    srs_success_response()
}
