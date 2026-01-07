//! # 错误处理模块
//!
//! 定义了应用程序中使用的各种错误类型和 HTTP 响应辅助函数。
//! 包括 API 错误响应、SRS 回调响应和聊天室禁止响应。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use std::fmt;

/// API 错误枚举
///
/// 定义了应用程序中可能出现的各种错误类型
#[derive(Debug)]
pub enum ApiError {
    /// 403 禁止访问 - 客户端无权限执行该操作
    Forbidden(String),
    /// 404 未找到 - 请求的资源不存在
    NotFound(String),
    /// 400 错误请求 - 客户端请求格式错误或参数无效
    BadRequest(String),
    /// 500 内部错误 - 服务器端发生未预期的错误
    Internal(String),
}

/// 实现 Display trait，支持错误信息格式化输出
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

/// 实现 Error trait，使 ApiError 可以作为标准错误类型使用
impl std::error::Error for ApiError {}

/// 实现 IntoResponse trait，将 ApiError 转换为 HTTP 响应
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        // 根据错误类型确定 HTTP 状态码和消息
        let (status, message) = match &self {
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        // 将错误信息包装成 JSON 响应
        let body = json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

// ============================================================================
// SRS 回调专用响应函数
// ============================================================================

/// SRS 回调拒绝响应
///
/// 当 SRS 回调验证失败时返回此响应。
/// 响应体为 "rua"，HTTP 状态码为 403。
///
/// ### 使用场景
/// - 推流密钥验证失败
/// - 拉流客户端未通过答题验证
/// - 其他权限验证失败的情况
pub fn srs_forbidden_response() -> Response {
    (StatusCode::FORBIDDEN, "rua").into_response()
}

/// SRS 回调成功响应
///
/// 当 SRS 回调验证成功时返回此响应。
/// 响应体为 "0"，HTTP 状态码为 200。
///
/// ### 注意事项
/// SRS 通过判断响应状态码是否为 200 来决定是否允许操作，
/// 响应体 "0" 是与原 Python 版本保持兼容的约定。
pub fn srs_success_response() -> Response {
    (StatusCode::OK, "0").into_response()
}

// ============================================================================
// API 和聊天室响应函数
// ============================================================================

/// API 禁止访问响应（JSON 格式）
///
/// 当 API 请求因权限不足被拒绝时返回此响应。
///
/// ### 响应内容
/// - HTTP 状态码: 403 Forbidden
/// - Content-Type: application/json
/// - 响应体: "It was a joke"（与原版本保持一致的彩蛋消息）
pub fn forbidden_json_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        [("Content-Type", "application/json")],
        "It was a joke",
    )
        .into_response()
}

/// 聊天室禁止访问响应
///
/// 当聊天室请求因权限不足被拒绝时返回此响应。
///
/// ### 响应内容
/// - HTTP 状态码: 403 Forbidden
/// - Content-Type: application/json
/// - 响应体: "Haha, fat chance"（与原版本保持一致的彩蛋消息）
///
/// ### 使用场景
/// - 直播未开始时尝试访问聊天室
/// - 客户端未通过答题验证
/// - 其他聊天室权限验证失败的情况
pub fn chat_forbidden_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        [("Content-Type", "application/json")],
        "Haha, fat chance",
    )
        .into_response()
}
