use std::{net::SocketAddr, sync::Arc};

use crate::{AppState, routes};

pub async fn serve(state: Arc<AppState>) {
    let app = routes::router(state.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], state.config.api_port));
    println!("API server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
