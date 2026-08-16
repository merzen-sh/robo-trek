use std::{net::SocketAddr, sync::Arc};

use crate::{AppState, routes};
use tracing::info;

/// Serves the API, shutting down gracefully on Ctrl-C.
pub async fn serve(state: Arc<AppState>) -> Result<(), std::io::Error> {
    let app = routes::router(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.api_port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API server listening on {addr}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
}

pub async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
