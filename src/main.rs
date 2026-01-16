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
//! ## 三端口服务设计
//! - **端口 3484**: API 服务 - 观众鉴权和答题逻辑
//! - **端口 3614**: 聊天服务 - 轮询式聊天室
//! - **端口 8848**: SRS 回调服务 - 接收推流/拉流事件

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
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::{info, Level};

/// 程序入口点
///
/// ### 启动流程
/// 1. 初始化日志系统
/// 2. 加载配置
/// 3. 确保必要目录存在
/// 4. 检查/创建密钥文件
/// 5. 初始化应用状态
/// 6. 启动后台清理任务
/// 7. 启动三个 HTTP 服务
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
    // 6. 构建 API 路由（端口 3484）
    // ========================================
    let api_app = Router::new()
        .route("/api.php", get(handlers::api_handler))
        .layer(TraceLayer::new_for_http())
        .fallback_service(ServeDir::new("stratic"))
        .with_state(state.clone());

    // ========================================
    // 7. 构建聊天室路由（端口 3614）
    // ========================================
    let chat_app = Router::new()
        .route("/chat.php", post(handlers::chat_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // ========================================
    // 8. 构建 SRS 回调路由（端口 8848）
    // ========================================
    let srs_app = Router::new()
        .route("/", post(handlers::srs_callback_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // ========================================
    // 9. 启动后台清理任务
    // ========================================
    // 每 10 秒清理过期的客户端和主播记录
    let srs_db_for_tick = state.srs_db.clone();
    let tick_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;

            // 获取写锁进行清理操作
            let mut inner = srs_db_for_tick.write();

            // 获取当前时间
            let now = chrono::Utc::now();

            // 检查主播是否过期
            if inner.streamer.is_expired() {
                tracing::debug!("srs_db.tick(): 主播已过期，清除所有数据");
                inner.reset();
                continue;
            }

            // 清理过期的客户端
            inner.clients.retain(|ip, clients| {
                clients.retain(|session_id, client| {
                    if client.is_expired() {
                        tracing::debug!("移除过期客户端: (ip={}, session_id={})", ip, session_id);
                        false
                    } else {
                        true
                    }
                });
                // 如果 IP 下没有客户端了，移除该 IP 条目
                if clients.is_empty() {
                    return false;
                }
                return true;
            });
        }
    });

    // ========================================
    // 10. 启动三个 HTTP 服务
    // ========================================
    let api_addr: SocketAddr = config.api_addr().parse()?;
    let chat_addr: SocketAddr = config.chat_addr().parse()?;
    let srs_addr: SocketAddr = config.srs_addr().parse()?;

    // 启动 API 服务
    let api_server = tokio::spawn(async move {
        info!("API 服务监听在 {}", api_addr);
        let tcp_listener = tokio::net::TcpListener::bind(api_addr).await.unwrap();
        axum::serve(tcp_listener, api_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    // 启动聊天室服务
    let chat_server = tokio::spawn(async move {
        info!("聊天室服务监听在 {}", chat_addr);
        let tcp_listener = tokio::net::TcpListener::bind(chat_addr).await.unwrap();
        axum::serve(tcp_listener, chat_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    // 启动 SRS 回调服务
    let srs_server = tokio::spawn(async move {
        info!("SRS 回调服务监听在 {}", srs_addr);
        let tcp_listener = tokio::net::TcpListener::bind(srs_addr).await.unwrap();
        axum::serve(tcp_listener, srs_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    // ========================================
    // 11. 等待任一服务结束或关闭信号
    // ========================================
    info!("所有服务启动成功");
    info!("API:      http://{}", config.api_addr());
    info!("Chat:     http://{}", config.chat_addr());
    info!("SRS:      http://{}", config.srs_addr());

    tokio::select! {
        _ = api_server => {
            info!("API 服务已关闭");
        }
        _ = chat_server => {
            info!("聊天室服务已关闭");
        }
        _ = srs_server => {
            info!("SRS 回调服务已关闭");
        }
        _ = shutdown_signal() => {
            info!("收到关闭信号");
        }
    }

    // 中止后台清理任务
    tick_task.abort();

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
