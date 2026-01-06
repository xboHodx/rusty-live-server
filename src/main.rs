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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting live-server-rs...");

    // Load configuration
    let config = Config::from_env();

    // Ensure directories exist
    tokio::fs::create_dir_all(&config.dump_path).await?;
    tokio::fs::create_dir_all(&config.secret_path.parent().unwrap()).await?;

    // Check if secret file exists, create a default one if not
    if !config.secret_path.exists() {
        info!("Secret file not found, creating default one");
        tokio::fs::write(&config.secret_path, "secret_my_stream_key\n").await?;
        info!("Default secret key created: secret_my_stream_key");
        info!(
            "Please edit {} to set your stream key",
            config.secret_path.display()
        );
    }

    // Initialize application state
    let state = match AppState::new(config.clone()) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Failed to initialize application state: {}", e);
            return Err(e);
        }
    };

    // Build API router (port 3484)
    let api_app = Router::new()
        .route("/api.php", get(handlers::api_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());
  
    // Build chat router (port 3614)
    let chat_app = Router::new()
        .route("/chat.php", post(handlers::chat_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Build SRS callback router (port 8848)
    let srs_app = Router::new()
        .route("/", post(handlers::srs_callback_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Start background tick task for SRS database cleanup
    let srs_db_for_tick = state.srs_db.clone();
    let tick_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            interval.tick().await;

            // Get the inner lock for write operations
            let mut inner = srs_db_for_tick.write();

            // Do cleanup
            let now = chrono::Utc::now();

            // Check if streamer is expired
            if inner.streamer.is_expired() {
                tracing::debug!("srs_db.tick(): streamer expired, removing all");
                inner.reset();
                continue;
            }

            inner.clients.retain(|ip, clients| {
                clients.retain(|rid, client| {
                    if client.is_expired() {
                        tracing::debug!("removing expired client: (ip={}, rid={})", ip, rid);
                        false
                    } else {
                        true
                    }
                });
                if clients.is_empty() {
                    return false;
                }
                return true;
            });
        }
    });

    // Start the three servers concurrently
    let api_addr: SocketAddr = config.api_addr().parse()?;
    let chat_addr: SocketAddr = config.chat_addr().parse()?;
    let srs_addr: SocketAddr = config.srs_addr().parse()?;

    let api_server = tokio::spawn(async move {
        info!("API server listening on {}", api_addr);
        let tcp_listener = tokio::net::TcpListener::bind(api_addr).await.unwrap();
        axum::serve(tcp_listener, api_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    let chat_server = tokio::spawn(async move {
        info!("Chat server listening on {}", chat_addr);
        let tcp_listener = tokio::net::TcpListener::bind(chat_addr).await.unwrap();
        axum::serve(tcp_listener, chat_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    let srs_server = tokio::spawn(async move {
        info!("SRS callback server listening on {}", srs_addr);
        let tcp_listener = tokio::net::TcpListener::bind(srs_addr).await.unwrap();
        axum::serve(tcp_listener, srs_app)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap();
    });

    info!("All servers started successfully");
    info!("API:      http://{}", config.api_addr());
    info!("Chat:     http://{}", config.chat_addr());
    info!("SRS:      http://{}", config.srs_addr());

    // Wait for any server to finish or shutdown signal
    tokio::select! {
        _ = api_server => {
            info!("API server shut down");
        }
        _ = chat_server => {
            info!("Chat server shut down");
        }
        _ = srs_server => {
            info!("SRS callback server shut down");
        }
        _ = shutdown_signal() => {
            info!("Shutdown signal received");
        }
    }

    // Abort the tick task
    tick_task.abort();

    info!("live-server-rs stopped");
    Ok(())
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C");
        },
        _ = terminate => {
            info!("Received terminate signal");
        },
    }
}
