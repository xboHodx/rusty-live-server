//! # Rusty Live Server - 主程序入口
//!
//! 高性能直播互动服务器，与 SRS（Simple Realtime Server）集成。
//!
//! ## 功能概述
//! - 管理直播推流和观众拉流鉴权
//! - 基于答题的观众入场验证
//! - 实时聊天室功能
//! - 主播身份验证和权限管理
//!
//! ## 服务设计
//! - **端口 8848**: 统一服务
//!   - `/` → SRS 回调
//!   - `/api` → 认证答题
//!   - `/chat` → 聊天室

mod config;
mod error;
mod handlers;
mod state;

use axum::{
    routing::{get, post},
    Router,
};
use config::Config;
use state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower_http::trace::TraceLayer;
use tracing::{info, Level};

/// 程序入口点
///
/// ### 启动流程
/// 1. 初始化日志系统
/// 2. 加载配置
/// 3. 确保必要目录存在
/// 4. 检查/创建密钥文件
/// 5. 初始化应用状态
/// 6. 构建统一路由
/// 7. 启动后台清理任务
/// 8. 启动 HTTP 服务
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ========================================
    // 1. 初始化日志系统
    // ========================================
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("正在启动 live-server-rs...");

    // ========================================
    // 2. 加载配置
    // ========================================
    let config = Config::from_env();

    // ========================================
    // 3. 确保必要目录存在
    // ========================================
    tokio::fs::create_dir_all(&config.dump_path).await?;
    tokio::fs::create_dir_all(&config.secret_path.parent().unwrap()).await?;

    // ========================================
    // 4. 检查密钥文件，不存在则创建默认密钥
    // ========================================
    if !config.secret_path.exists() {
        info!("未找到密钥文件，正在创建默认密钥文件");
        tokio::fs::write(&config.secret_path, "secret_my_stream_key\n").await?;
        info!("已创建默认密钥: secret_my_stream_key");
        info!(
            "请编辑 {} 以设置您的推流密钥",
            config.secret_path.display()
        );
    }

    // ========================================
    // 5. 初始化应用状态
    // ========================================
    let state = match AppState::new(config.clone()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("初始化应用状态失败: {}", e);
            return Err(e);
        }
    };

    // ========================================
    // 6. 构建统一路由（端口 8848）
    // ========================================
    let app = Router::new()
        .route("/", post(handlers::srs_callback_handler))  // SRS 回调
        .route("/api", get(handlers::api_handler))          // 认证答题
        .route("/chat", post(handlers::chat_handler))       // 聊天室
        .route("/streaming_info", get(handlers::streaming_info_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // ========================================
    // 7. 启动后台任务
    // ========================================
    // 每 10 秒清理过期的客户端和主播记录
    let srs_db_for_tick = state.srs_db.clone();
    let tick_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;
            srs_db_for_tick.tick();
        }
    });

    // 从srs获取观众人数
    let srs_api_url = config.srs_api_addr();
    let streaming_info = state.streaming_info.clone();
    let streaming_info_task_handle = streaming_info.tick(srs_api_url);

    // ========================================
    // 8. 启动 HTTP 服务
    // ========================================
    let addr: SocketAddr = config.addr().parse()?;

    info!("服务启动成功，监听于 {}", addr);
    info!("  /      → SRS 回调");
    info!("  /api   → 认证答题");
    info!("  /chat  → 聊天室");
    info!("  /streaming_info  → 流信息");

    let tcp_listener = tokio::net::TcpListener::bind(addr).await?;

    axum::serve(tcp_listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    // 中止后台清理任务
    tick_task.abort();
    streaming_info_task_handle.abort();

    info!("live-server-rs 已停止");
    Ok(())
}

/// 优雅关闭信号处理
///
/// 监听以下信号并触发关闭流程：
/// - Ctrl+C (SIGINT)
/// - SIGTERM (仅 Unix 系统)
async fn shutdown_signal() {
    // 监听 Ctrl+C
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("无法安装 Ctrl+C 处理器");
    };

    // 监听 SIGTERM（仅 Unix 系统）
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("无法安装信号处理器")
            .recv()
            .await;
    };

    // 非 Unix 系统使用永不完成的 Future
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("收到 Ctrl+C");
        },
        _ = terminate => {
            info!("收到 terminate 信号");
        },
    }
}
