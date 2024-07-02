// server.rs
use std::sync::Arc;

use axum::Router;
use axum::routing::{post, get};
use tokio::signal;
use tracing::info;

use crate::handlers::register::register;
use crate::handlers::decrypt::decrypt_keys_handler;
use crate::mongo::AppState;

pub fn create_app(db: mongodb::Database) -> Router {
    let app_state = Arc::new(AppState { db });
    Router::new()
    .route("/register", post(register))
    .route("/decrypt_keys", get(decrypt_keys_handler))
    .with_state(app_state)
}

pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("signal received, starting graceful shutdown");
}
