//! # HTTP 处理器模块
//!
//! 定义了所有 HTTP 请求处理器的模块入口。
//! 包含三个主要的处理器：
//! - `api` - 观众端 API 处理器（答题验证、状态查询等）
//! - `chat` - 聊天室处理器（发送消息、设置昵称等）
//! - `srs` - SRS 回调处理器（推流/拉流事件回调）

// 子模块声明
pub mod api;   // API 处理器模块
pub mod chat;  // 聊天室处理器模块
pub mod srs;   // SRS 回调处理器模块
pub mod streaming_info;

// 导出公共处理器函数，供 main.rs 中使用
pub use api::{api_handler};           // API 请求主处理器
pub use chat::{chat_handler};         // 聊天室请求处理器
pub use srs::{srs_callback_handler};  // SRS 回调处理器
pub use streaming_info::{streaming_info_handler};  // SRS 回调处理器
